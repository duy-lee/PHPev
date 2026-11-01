//! `.htaccess` rewrite rules (mod_rewrite subset), so plain-PHP projects
//! written for Apache run unchanged.
//!
//! Supported: `RewriteEngine`, `RewriteBase`, `RewriteOptions Inherit|InheritBefore`,
//! `RewriteCond` (regex, `-f -d -s -l -F`, `=`, `<`, `>`; flags `NC`, `OR`) and
//! `RewriteRule` (flags `L`, `END`, `R[=code]`, `NC`, `QSA`, `QSD`, `F`, `G`, `S=n`, `E=var:value`).
//! Other directives (`php_value`, `AddDefaultCharset`, `Options`, …) are ignored;
//! PHP settings belong in `.user.ini`, which php-cgi reads itself.
//!
//! Per-directory semantics as in Apache: the deepest directory on the path
//! with a `.htaccess` decides the rules (a parent's apply only with
//! `RewriteOptions Inherit`), patterns see the path relative to that
//! directory, and after an internal rewrite the new path is processed again
//! until it no longer changes.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use http::{HeaderMap, StatusCode};
use regex_lite::{Regex, RegexBuilder};

use crate::log::log_warn;

/// Rounds of internal rewriting before giving up (Apache's LimitInternalRecursion).
const MAX_ROUNDS: usize = 10;

#[derive(Default)]
pub struct Htaccess {
    /// `None`: not set here, inherited from the parent directory.
    engine: Option<bool>,
    base: Option<String>,
    inherit: Inherit,
    rules: Vec<Rule>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Inherit {
    #[default]
    No,
    After,
    Before,
}

struct Rule {
    pattern: Regex,
    negate: bool,
    subst: String,
    conds: Vec<Cond>,
    flags: RuleFlags,
}

#[derive(Default)]
struct RuleFlags {
    last: bool,
    end: bool,
    redirect: Option<u16>,
    qsa: bool,
    qsd: bool,
    forbidden: bool,
    gone: bool,
    skip: usize,
    env: Vec<(String, String)>,
}

struct Cond {
    test: String,
    check: Check,
    negate: bool,
    or: bool,
}

enum Check {
    Regex(Regex),
    File,
    Dir,
    NonEmpty,
    Link,
    Equal(String),
    Less(String),
    Greater(String),
}

// ---------------------------------------------------------------- parsing

impl Htaccess {
    /// Parses the file; problems are returned as warnings and the offending
    /// line is skipped.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut ht = Self::default();
        let mut warnings = Vec::new();
        let mut conds: Vec<Cond> = Vec::new();
        // Depth of sections whose content is skipped (`<Files>`, `<IfModule !mod_rewrite.c>`, …).
        let mut skipping = 0usize;
        let mut joined = String::new();
        for (no, raw) in text.lines().enumerate() {
            // A trailing backslash continues the line.
            if let Some(head) = raw.strip_suffix('\\') {
                joined.push_str(head);
                continue;
            }
            joined.push_str(raw);
            let line = std::mem::take(&mut joined);
            let line = line.trim();
            let no = no + 1;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("</") {
                skipping = skipping.saturating_sub(1);
                continue;
            }
            if let Some(open) = line.strip_prefix('<') {
                let open = open.trim_end_matches('>').trim();
                let (name, arg) = open.split_once(char::is_whitespace).unwrap_or((open, ""));
                let transparent =
                    skipping == 0 && name.eq_ignore_ascii_case("IfModule") && !arg.trim().starts_with('!');
                if !transparent {
                    skipping += 1;
                }
                continue;
            }
            if skipping > 0 {
                continue;
            }
            let args = split_args(line);
            let Some((directive, args)) = args.split_first() else {
                continue;
            };
            match directive.to_ascii_lowercase().as_str() {
                "rewriteengine" => ht.engine = Some(args.first().is_some_and(|a| a.eq_ignore_ascii_case("on"))),
                "rewritebase" => ht.base = args.first().cloned(),
                "rewriteoptions" => {
                    for a in args {
                        match a.to_ascii_lowercase().as_str() {
                            "inherit" => ht.inherit = Inherit::After,
                            "inheritbefore" => ht.inherit = Inherit::Before,
                            _ => warnings.push(format!("line {no}: RewriteOptions {a} is not supported")),
                        }
                    }
                }
                "rewritecond" => match parse_cond(args) {
                    Ok(c) => conds.push(c),
                    Err(e) => warnings.push(format!("line {no}: {e}")),
                },
                "rewriterule" => {
                    let conds = std::mem::take(&mut conds);
                    match parse_rule(args, conds) {
                        Ok(r) => ht.rules.push(r),
                        Err(e) => warnings.push(format!("line {no}: {e}")),
                    }
                }
                "rewritemap" | "rewritelock" => warnings.push(format!("line {no}: {directive} is not supported")),
                _ => {}
            }
        }
        (ht, warnings)
    }
}

/// Splits a directive line into words; double quotes group words. A
/// backslash is kept (it is part of the regex), so `Windows\ Phone` stays
/// one word.
fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars();
    let mut started = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// `[NC,OR]` → lower-case flag words.
fn flag_words(arg: Option<&String>) -> Result<Vec<String>, String> {
    let Some(arg) = arg else { return Ok(Vec::new()) };
    let inner = arg
        .strip_prefix('[')
        .and_then(|a| a.strip_suffix(']'))
        .ok_or_else(|| format!("flags must be in brackets: {arg}"))?;
    Ok(inner
        .split(',')
        .map(|f| f.trim().to_owned())
        .filter(|f| !f.is_empty())
        .collect())
}

fn compile(pattern: &str, nocase: bool) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(nocase)
        .build()
        .map_err(|e| format!("cannot use the pattern {pattern}: {e}"))
}

fn parse_cond(args: &[String]) -> Result<Cond, String> {
    let [test, pattern, rest @ ..] = args else {
        return Err("RewriteCond needs a test string and a pattern".into());
    };
    let mut nocase = false;
    let mut or = false;
    for f in flag_words(rest.first())? {
        match f.to_ascii_lowercase().as_str() {
            "nc" | "nocase" => nocase = true,
            "or" | "ornext" => or = true,
            "nv" | "novary" => {}
            _ => return Err(format!("RewriteCond flag {f} is not supported")),
        }
    }
    let (negate, pattern) = match pattern.strip_prefix('!') {
        Some(p) => (true, p),
        None => (false, pattern.as_str()),
    };
    let check = match pattern {
        "-f" | "-F" => Check::File,
        "-d" => Check::Dir,
        "-s" => Check::NonEmpty,
        "-l" | "-L" => Check::Link,
        p if p.starts_with('-') && p.len() == 2 => return Err(format!("RewriteCond test {p} is not supported")),
        p => {
            if let Some(v) = p.strip_prefix('=') {
                Check::Equal(v.to_owned())
            } else if let Some(v) = p.strip_prefix('<') {
                Check::Less(v.to_owned())
            } else if let Some(v) = p.strip_prefix('>') {
                Check::Greater(v.to_owned())
            } else {
                Check::Regex(compile(p, nocase)?)
            }
        }
    };
    Ok(Cond {
        test: test.clone(),
        check,
        negate,
        or,
    })
}

fn parse_rule(args: &[String], conds: Vec<Cond>) -> Result<Rule, String> {
    let [pattern, subst, rest @ ..] = args else {
        return Err("RewriteRule needs a pattern and a substitution".into());
    };
    let mut flags = RuleFlags::default();
    let mut nocase = false;
    for f in flag_words(rest.first())? {
        let (name, value) = match f.split_once('=') {
            Some((n, v)) => (n.to_ascii_lowercase(), Some(v)),
            None => (f.to_ascii_lowercase(), None),
        };
        match name.as_str() {
            "l" | "last" => flags.last = true,
            "end" => flags.end = true,
            "r" | "redirect" => {
                let code = match value {
                    None => 302,
                    Some(v) if v.eq_ignore_ascii_case("permanent") => 301,
                    Some(v) if v.eq_ignore_ascii_case("temp") => 302,
                    Some(v) => v.parse().map_err(|_| format!("bad redirect code {v}"))?,
                };
                flags.redirect = Some(code);
            }
            "nc" | "nocase" => nocase = true,
            "qsa" | "qsappend" => flags.qsa = true,
            "qsd" | "qsdiscard" => flags.qsd = true,
            "f" | "forbidden" => flags.forbidden = true,
            "g" | "gone" => flags.gone = true,
            "s" | "skip" => {
                flags.skip = value
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| format!("bad skip count in {f}"))?;
            }
            "e" | "env" => {
                let v = value.unwrap_or("");
                if let Some((k, val)) = v.split_once(':') {
                    flags.env.push((k.to_owned(), val.to_owned()));
                }
            }
            // No effect here: PHPev neither escapes nor has other handlers.
            "ne" | "noescape" | "pt" | "passthrough" | "nv" | "novary" | "dpi" | "discardpath" | "b" | "bnp" => {}
            _ => return Err(format!("RewriteRule flag {f} is not supported")),
        }
    }
    let (negate, pattern) = match pattern.strip_prefix('!') {
        Some(p) => (true, p),
        None => (false, pattern.as_str()),
    };
    Ok(Rule {
        pattern: compile(pattern, nocase)?,
        negate,
        subst: subst.clone(),
        conds,
        flags,
    })
}

// ---------------------------------------------------------------- cache

/// Parsed `.htaccess` files by directory, re-read when the file changes
/// (one `stat` per directory per request).
#[derive(Default)]
pub struct Cache(Mutex<HashMap<PathBuf, Entry>>);

struct Entry {
    stamp: (Option<SystemTime>, u64),
    parsed: Arc<Htaccess>,
}

impl Cache {
    /// The `.htaccess` of `dir`, or `None` if it has none.
    fn get(&self, dir: &Path) -> Option<Arc<Htaccess>> {
        let file = dir.join(".htaccess");
        let meta = std::fs::metadata(&file).ok().filter(|m| m.is_file());
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(meta) = meta else {
            map.remove(dir);
            return None;
        };
        let stamp = (meta.modified().ok(), meta.len());
        if let Some(e) = map.get(dir) {
            if e.stamp == stamp {
                return Some(e.parsed.clone());
            }
        }
        let bytes = std::fs::read(&file).ok()?;
        let (parsed, warnings) = Htaccess::parse(&String::from_utf8_lossy(&bytes));
        for w in warnings {
            log_warn!("{}: {w}", file.display());
        }
        let parsed = Arc::new(parsed);
        map.insert(
            dir.to_owned(),
            Entry {
                stamp,
                parsed: parsed.clone(),
            },
        );
        Some(parsed)
    }
}

// ---------------------------------------------------------------- rewriting

/// Facts about the request that `%{VAR}` can read.
pub struct Request<'a> {
    pub method: &'a str,
    /// The original path and query, for THE_REQUEST.
    pub path_and_query: &'a str,
    pub protocol: &'a str,
    pub host: &'a str,
    pub https: bool,
    pub port: u16,
    pub remote: SocketAddr,
    pub headers: &'a HeaderMap,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// No `.htaccess` rules apply: route as usual.
    NoRules,
    /// Rules ran; serve `path` (no leading `/`, may be empty for the
    /// docroot). `query` replaces the query string when set.
    Serve {
        path: String,
        query: Option<String>,
        env: Vec<(String, String)>,
    },
    Redirect {
        status: StatusCode,
        location: String,
    },
    Status(StatusCode),
}

/// The rule set for `path`: the deepest directory with a `.htaccess`, its
/// URL prefix (`""` or `"admin/"`) and the rules in the order they run.
struct RuleSet {
    prefix: String,
    base: Option<String>,
    rules: Vec<Arc<Htaccess>>,
}

fn rule_set(cache: &Cache, docroot: &Path, path: &str) -> Option<RuleSet> {
    // Every directory from the docroot down to the last existing one on the path.
    let mut chain: Vec<(String, Arc<Htaccess>)> = Vec::new();
    let mut dir = docroot.to_path_buf();
    let mut prefix = String::new();
    if let Some(h) = cache.get(&dir) {
        chain.push((prefix.clone(), h));
    }
    let mut segs: Vec<&str> = path.split('/').collect();
    segs.pop(); // the last segment is a file name (or empty after a trailing `/`)
    for seg in segs {
        if seg.is_empty() {
            continue;
        }
        dir.push(seg);
        if !dir.is_dir() {
            break;
        }
        prefix.push_str(seg);
        prefix.push('/');
        if let Some(h) = cache.get(&dir) {
            chain.push((prefix.clone(), h));
        }
    }

    let (prefix, deepest) = chain.last()?.clone();
    let engine = chain.iter().rev().find_map(|(_, h)| h.engine).unwrap_or(false);
    if !engine {
        return None;
    }
    // Inherit pulls in the parent's rules (which may inherit in turn).
    let mut rules = vec![deepest.clone()];
    let mut i = chain.len() - 1;
    while i > 0 && chain[i].1.inherit != Inherit::No {
        let parent = chain[i - 1].1.clone();
        if chain[i].1.inherit == Inherit::Before {
            rules.insert(0, parent);
        } else {
            rules.push(parent);
        }
        i -= 1;
    }
    Some(RuleSet {
        prefix,
        base: deepest.base.clone(),
        rules,
    })
}

/// Applies the `.htaccess` rules to `path` (decoded, without the leading
/// `/`; a trailing `/` is kept).
pub fn rewrite(cache: &Cache, docroot: &Path, path: &str, query: Option<&str>, req: &Request<'_>) -> Outcome {
    let mut path = path.to_owned();
    let mut query: Option<String> = query.map(str::to_owned);
    let original = (path.clone(), query.clone());
    let mut env = Vec::new();
    let mut ran = false;

    for _ in 0..MAX_ROUNDS {
        let Some(set) = rule_set(cache, docroot, &path) else {
            break;
        };
        ran = true;
        let before = (path.clone(), query.clone());
        let mut end = false;
        let mut skip = 0usize;
        let rules = set.rules.iter().flat_map(|h| h.rules.iter());
        for rule in rules {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            // Rules see the path relative to the `.htaccess` directory.
            let Some(rel) = path.strip_prefix(set.prefix.as_str()) else {
                break;
            };
            let caps: Vec<String> = match rule.pattern.captures(rel) {
                Some(c) if !rule.negate => c
                    .iter()
                    .map(|m| m.map_or(String::new(), |m| m.as_str().to_owned()))
                    .collect(),
                None if rule.negate => Vec::new(),
                _ => continue,
            };
            let Some(cond_caps) = conds_match(&rule.conds, &caps, docroot, &path, query.as_deref(), req) else {
                continue;
            };
            let f = &rule.flags;
            if f.forbidden {
                return Outcome::Status(StatusCode::FORBIDDEN);
            }
            if f.gone {
                return Outcome::Status(StatusCode::GONE);
            }
            for (k, v) in &f.env {
                let k = expand(k, &caps, &cond_caps, docroot, &path, query.as_deref(), req);
                let v = expand(v, &caps, &cond_caps, docroot, &path, query.as_deref(), req);
                env.push((k, v));
            }
            skip = f.skip;

            if rule.subst != "-" {
                let subst = expand(&rule.subst, &caps, &cond_caps, docroot, &path, query.as_deref(), req);
                let (target, new_query) = match subst.split_once('?') {
                    Some((t, q)) => (t.to_owned(), Some(q.to_owned())),
                    None => (subst, None),
                };
                query = match (new_query, f.qsd) {
                    (Some(q), _) if f.qsa => match query.as_deref() {
                        Some(old) if !old.is_empty() => Some(format!("{q}&{old}")),
                        _ => Some(q),
                    },
                    (Some(q), _) => Some(q),
                    (None, true) => None,
                    (None, false) => query,
                };
                let lower = target.to_ascii_lowercase();
                if lower.starts_with("http://") || lower.starts_with("https://") {
                    return Outcome::Redirect {
                        status: redirect_status(f.redirect),
                        location: with_query(&target, query.as_deref()),
                    };
                }
                path = match target.strip_prefix('/') {
                    Some(abs) => abs.to_owned(),
                    None => format!("{}{target}", base_prefix(set.base.as_deref(), &set.prefix)),
                };
                if f.redirect.is_some() {
                    let scheme = if req.https { "https" } else { "http" };
                    let default_port = if req.https { 443 } else { 80 };
                    let port = if req.port == default_port {
                        String::new()
                    } else {
                        format!(":{}", req.port)
                    };
                    return Outcome::Redirect {
                        status: redirect_status(f.redirect),
                        location: with_query(&format!("{scheme}://{}{port}/{path}", req.host), query.as_deref()),
                    };
                }
            }
            if f.end {
                end = true;
            }
            if f.last || f.end {
                break;
            }
        }
        if end || (path.clone(), query.clone()) == before {
            return finish(path, query, env, &original);
        }
    }
    if !ran {
        return Outcome::NoRules;
    }
    log_warn!(
        "htaccess: rewrite of /{} did not settle after {MAX_ROUNDS} rounds",
        original.0
    );
    Outcome::Status(StatusCode::INTERNAL_SERVER_ERROR)
}

fn finish(
    path: String,
    query: Option<String>,
    env: Vec<(String, String)>,
    original: &(String, Option<String>),
) -> Outcome {
    let query = if query == original.1 {
        None
    } else {
        Some(query.unwrap_or_default())
    };
    Outcome::Serve { path, query, env }
}

fn redirect_status(code: Option<u16>) -> StatusCode {
    match code {
        Some(c @ (301 | 302 | 303 | 307 | 308)) => StatusCode::from_u16(c).unwrap_or(StatusCode::FOUND),
        _ => StatusCode::FOUND,
    }
}

fn with_query(url: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("{url}?{q}"),
        _ => url.to_owned(),
    }
}

/// Where a relative substitution lands: `RewriteBase`, else the directory.
fn base_prefix(base: Option<&str>, dir_prefix: &str) -> String {
    match base {
        Some(b) => {
            let b = b.trim_start_matches('/');
            if b.is_empty() || b.ends_with('/') {
                b.to_owned()
            } else {
                format!("{b}/")
            }
        }
        None => dir_prefix.to_owned(),
    }
}

/// Evaluates the conditions (AND, with `OR` joining a condition to the next).
/// Returns the captures of the last matching regex condition, for `%N`.
fn conds_match(
    conds: &[Cond],
    caps: &[String],
    docroot: &Path,
    path: &str,
    query: Option<&str>,
    req: &Request<'_>,
) -> Option<Vec<String>> {
    let mut cond_caps = Vec::new();
    let mut i = 0;
    while i < conds.len() {
        let c = &conds[i];
        let test = expand(&c.test, caps, &cond_caps, docroot, path, query, req);
        let matched = match &c.check {
            Check::Regex(re) => match re.captures(&test) {
                Some(m) if !c.negate => {
                    cond_caps = m
                        .iter()
                        .map(|m| m.map_or(String::new(), |m| m.as_str().to_owned()))
                        .collect();
                    true
                }
                Some(_) => false,
                None => c.negate,
            },
            check => {
                let r = match check {
                    Check::File => Path::new(&test).is_file(),
                    Check::Dir => Path::new(&test).is_dir(),
                    Check::NonEmpty => std::fs::metadata(&test).is_ok_and(|m| m.is_file() && m.len() > 0),
                    Check::Link => std::fs::symlink_metadata(&test).is_ok_and(|m| m.file_type().is_symlink()),
                    Check::Equal(v) => test == *v,
                    Check::Less(v) => test < *v,
                    Check::Greater(v) => test > *v,
                    Check::Regex(_) => unreachable!(),
                };
                r != c.negate
            }
        };
        if c.or {
            if matched {
                // The rest of this OR group is satisfied.
                while i < conds.len() && conds[i].or {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        if !matched {
            return None;
        }
        i += 1;
    }
    Some(cond_caps)
}

/// Expands `$N` (rule captures), `%N` (condition captures) and `%{VAR}`.
fn expand(
    s: &str,
    caps: &[String],
    cond_caps: &[String],
    docroot: &Path,
    path: &str,
    query: Option<&str>,
    req: &Request<'_>,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(&n) = chars.peek() {
                    if n == '$' || n == '%' {
                        out.push(n);
                        chars.next();
                        continue;
                    }
                }
                out.push(c);
            }
            '$' | '%' if chars.peek().is_some_and(char::is_ascii_digit) => {
                let n = chars.next().and_then(|d| d.to_digit(10)).unwrap_or(0) as usize;
                let from = if c == '$' { caps } else { cond_caps };
                out.push_str(from.get(n).map_or("", String::as_str));
            }
            '%' if chars.peek() == Some(&'{') => {
                chars.next();
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out.push_str(&variable(&name, docroot, path, query, req));
            }
            c => out.push(c),
        }
    }
    out
}

fn variable(name: &str, docroot: &Path, path: &str, query: Option<&str>, req: &Request<'_>) -> String {
    let header = |h: &str| {
        req.headers
            .get(h)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned()
    };
    if let Some(h) = name.strip_prefix("HTTP:") {
        return header(h);
    }
    match name {
        "HTTPS" => (if req.https { "on" } else { "off" }).to_owned(),
        "HTTP_HOST" => {
            let h = header("host");
            if h.is_empty() {
                req.host.to_owned()
            } else {
                h
            }
        }
        "REQUEST_URI" => format!("/{path}"),
        "REQUEST_FILENAME" | "SCRIPT_FILENAME" => request_filename(docroot, path).to_string_lossy().into_owned(),
        "QUERY_STRING" => query.unwrap_or("").to_owned(),
        "REQUEST_METHOD" => req.method.to_owned(),
        "REQUEST_SCHEME" => (if req.https { "https" } else { "http" }).to_owned(),
        "SERVER_NAME" => req.host.to_owned(),
        "SERVER_PORT" => req.port.to_string(),
        "SERVER_ADDR" => "127.0.0.1".to_owned(),
        "SERVER_PROTOCOL" => req.protocol.to_owned(),
        "SERVER_SOFTWARE" => concat!("PHPev/", env!("CARGO_PKG_VERSION")).to_owned(),
        "REMOTE_ADDR" => req.remote.ip().to_string(),
        "REMOTE_PORT" => req.remote.port().to_string(),
        "DOCUMENT_ROOT" => docroot.to_string_lossy().into_owned(),
        "THE_REQUEST" => format!("{} {} {}", req.method, req.path_and_query, req.protocol),
        "IS_SUBREQ" => "false".to_owned(),
        n if n.starts_with("HTTP_") => header(&n["HTTP_".len()..].replace('_', "-")),
        _ => String::new(),
    }
}

/// The file Apache maps the path to: the docroot plus the path up to its
/// first component that is not a directory (the rest would be PATH_INFO).
fn request_filename(docroot: &Path, path: &str) -> PathBuf {
    let mut file = docroot.to_path_buf();
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        file.push(seg);
        if !file.is_dir() {
            break;
        }
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    struct Site {
        root: PathBuf,
        cache: Cache,
        headers: HeaderMap,
        https: bool,
    }

    impl Site {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let root = std::env::temp_dir().join(format!("phpev-htaccess-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (f, text) in files {
                let p = root.join(f);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, text).unwrap();
            }
            Self {
                root,
                cache: Cache::default(),
                headers: HeaderMap::new(),
                https: false,
            }
        }

        fn run(&self, path_and_query: &str) -> Outcome {
            let (path, query) = match path_and_query.split_once('?') {
                Some((p, q)) => (p, Some(q)),
                None => (path_and_query, None),
            };
            let req = Request {
                method: "GET",
                path_and_query,
                protocol: "HTTP/1.1",
                host: "app.localhost",
                https: self.https,
                port: if self.https { 443 } else { 80 },
                remote: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 50000),
                headers: &self.headers,
            };
            rewrite(&self.cache, &self.root, path.trim_start_matches('/'), query, &req)
        }

        fn serves(&self, url: &str) -> String {
            match self.run(url) {
                Outcome::Serve { path, .. } => path,
                other => panic!("{url}: expected Serve, got {other:?}"),
            }
        }
    }

    impl Drop for Site {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// The rules of a plain-PHP project that keys companies by a `v_<name>/` prefix.
    const LEGACY: &str = r#"
adddefaultcharset shift_jis
php_flag short_open_tag On

RewriteEngine On
RewriteCond %{HTTPS} off
RewriteRule ^((v_secure|v_other)/module/login/login.php)$ https://%{HTTP_HOST}/$1 [R=301,L]

RewriteCond %{HTTP_USER_AGENT} (iPhone|iPad|Android|Windows\ Phone) [NC]
RewriteRule ^(v_[a-z]+)/keitai/(.*)$ /smart/$3

RewriteRule ^(.*)/sid([a-z0-9]+)/(.*)$ /keitai/$3
RewriteRule ^(v_[a-z]+)\/rss\/(down_staff_info\.php.*)|(.*\.(pem|pfx|ini|txt|csv))$ /rss/deny [L]
RewriteRule ^(v_[a-z]+)/(.*)$ $2

RewriteRule ^([a-z]+)\.php(.*)$ index.php
PHP_VALUE max_input_vars 2000
"#;

    fn legacy(name: &str) -> Site {
        Site::new(
            name,
            &[
                (".htaccess", LEGACY),
                ("index.php", ""),
                ("staff.php", ""),
                ("images/logo.gif", "x"),
                ("module/login/login.php", ""),
                ("keitai/index.php", ""),
                ("keitai/.htaccess", "AddDefaultCharset shift_jis\n"),
                (
                    "admin/.htaccess",
                    "RewriteEngine On\nRewriteRule ^([a-z]+)\\.php(.*)$ index.php\n",
                ),
                ("admin/index.php", ""),
                ("admin/user.php", ""),
            ],
        )
    }

    #[test]
    fn company_prefix_is_stripped() {
        let s = legacy("company_prefix_is_stripped");
        assert_eq!(s.serves("/v_den/"), "");
        assert_eq!(s.serves("/v_den/images/logo.gif"), "images/logo.gif");
        assert_eq!(s.serves("/v_den/module/login/login.php"), "module/login/login.php");
        // A top-level script goes through index.php, also after the prefix is stripped.
        assert_eq!(s.serves("/v_den/staff.php"), "index.php");
        assert_eq!(s.serves("/staff.php"), "index.php");
        assert_eq!(s.serves("/images/logo.gif"), "images/logo.gif");
    }

    #[test]
    fn query_string_is_kept() {
        let s = legacy("query_string_is_kept");
        assert_eq!(
            s.run("/v_den/index.php?page=attend"),
            Outcome::Serve {
                path: "index.php".into(),
                query: None,
                env: vec![]
            }
        );
    }

    #[test]
    fn deny_and_sid_rules() {
        let s = legacy("deny_and_sid_rules");
        assert_eq!(s.serves("/v_den/rss/down_staff_info.php?x=1"), "rss/deny");
        assert_eq!(s.serves("/v_den/sidabc123/menu.php"), "keitai/menu.php");
    }

    #[test]
    fn condition_on_user_agent() {
        let mut s = legacy("condition_on_user_agent");
        assert_eq!(s.serves("/v_den/keitai/index.php"), "keitai/index.php");
        s.headers.insert(
            http::header::USER_AGENT,
            "Mozilla/5.0 (windows phone 8.1)".parse().unwrap(),
        );
        // `$3` does not exist in the rule: it expands to nothing.
        assert_eq!(s.serves("/v_den/keitai/index.php"), "smart/");
    }

    #[test]
    fn redirect_with_condition() {
        let mut s = legacy("redirect_with_condition");
        assert_eq!(
            s.run("/v_secure/module/login/login.php"),
            Outcome::Redirect {
                status: StatusCode::MOVED_PERMANENTLY,
                location: "https://app.localhost/v_secure/module/login/login.php".into()
            }
        );
        s.https = true;
        assert_eq!(s.serves("/v_secure/module/login/login.php"), "module/login/login.php");
    }

    #[test]
    fn deepest_htaccess_wins() {
        let s = legacy("deepest_htaccess_wins");
        // admin/.htaccess replaces the root rules, relative to admin/.
        assert_eq!(s.serves("/admin/user.php"), "admin/index.php");
        assert_eq!(s.serves("/admin/v_x/a.gif"), "admin/v_x/a.gif");
        // keitai/.htaccess has no rules, so nothing is rewritten there.
        assert_eq!(s.serves("/v_den/keitai/x.php"), "keitai/x.php");
    }

    #[test]
    fn laravel_front_controller() {
        let s = Site::new(
            "laravel",
            &[
                (
                    ".htaccess",
                    r#"<IfModule mod_rewrite.c>
    <IfModule mod_negotiation.c>
        Options -MultiViews -Indexes
    </IfModule>

    RewriteEngine On

    # Handle Authorization Header
    RewriteCond %{HTTP:Authorization} .
    RewriteRule .* - [E=HTTP_AUTHORIZATION:%{HTTP:Authorization}]

    # Redirect Trailing Slashes If Not A Folder...
    RewriteCond %{REQUEST_FILENAME} !-d
    RewriteCond %{REQUEST_URI} (.+)/$
    RewriteRule ^ %1 [L,R=301]

    # Send Requests To Front Controller...
    RewriteCond %{REQUEST_FILENAME} !-d
    RewriteCond %{REQUEST_FILENAME} !-f
    RewriteRule ^ index.php [L]
</IfModule>
"#,
                ),
                ("index.php", ""),
                ("info.php", ""),
                ("css/app.css", ""),
            ],
        );
        assert_eq!(s.serves("/users/42"), "index.php");
        assert_eq!(s.serves("/css/app.css"), "css/app.css");
        assert_eq!(s.serves("/info.php/a/b"), "info.php/a/b");
        assert_eq!(s.serves("/css/"), "css/");
        assert_eq!(
            s.run("/users/?page=2"),
            Outcome::Redirect {
                status: StatusCode::MOVED_PERMANENTLY,
                location: "http://app.localhost/users?page=2".into()
            }
        );
    }

    #[test]
    fn flags_and_query() {
        let s = Site::new(
            "flags",
            &[(
                ".htaccess",
                "RewriteEngine on\n\
                 RewriteBase /app\n\
                 RewriteRule ^old$ new?a=1 [QSA,L]\n\
                 RewriteRule ^drop$ index.php? [L]\n\
                 RewriteRule ^secret - [F]\n\
                 RewriteRule ^gone - [G]\n\
                 RewriteCond %{QUERY_STRING} =x [OR]\n\
                 RewriteCond %{QUERY_STRING} =y\n\
                 RewriteRule ^either$ /yes [END]\n\
                 RewriteRule ^go$ http://example.com/ [R]\n\
                 RewriteRule ^(loopx*)$ /$1x\n\
                 <Files .env>\n\
                 RewriteRule ^files$ nope\n\
                 </Files>\n",
            )],
        );
        assert_eq!(
            s.run("/old?b=2"),
            Outcome::Serve {
                path: "app/new".into(),
                query: Some("a=1&b=2".into()),
                env: vec![]
            }
        );
        assert_eq!(
            s.run("/drop?b=2"),
            Outcome::Serve {
                path: "app/index.php".into(),
                query: Some(String::new()),
                env: vec![]
            }
        );
        assert_eq!(s.run("/secret"), Outcome::Status(StatusCode::FORBIDDEN));
        assert_eq!(s.run("/gone"), Outcome::Status(StatusCode::GONE));
        assert_eq!(s.serves("/either?y"), "yes");
        assert_eq!(s.serves("/either?z"), "either");
        assert_eq!(
            s.run("/go"),
            Outcome::Redirect {
                status: StatusCode::FOUND,
                location: "http://example.com/".into()
            }
        );
        assert_eq!(s.run("/loop"), Outcome::Status(StatusCode::INTERNAL_SERVER_ERROR));
        assert_eq!(s.serves("/files"), "files");
    }

    #[test]
    fn engine_off_or_no_file_means_no_rules() {
        let s = Site::new("off", &[(".htaccess", "RewriteRule ^ index.php\n")]);
        assert_eq!(s.run("/x"), Outcome::NoRules);
        let s = Site::new("none", &[("index.php", "")]);
        assert_eq!(s.run("/x"), Outcome::NoRules);
    }

    #[test]
    fn bad_lines_are_reported_and_skipped() {
        let (ht, warnings) = Htaccess::parse(
            "RewriteEngine On\nRewriteRule ^(a $1\nRewriteRule ^b$ c [ZZ]\nRewriteMap m txt:x\nRewriteRule ^d$ e\n",
        );
        assert_eq!(ht.rules.len(), 1);
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    #[test]
    fn htaccess_changes_are_picked_up() {
        let s = Site::new("reload", &[(".htaccess", "RewriteEngine On\nRewriteRule ^a$ b\n")]);
        assert_eq!(s.serves("/a"), "b");
        std::fs::write(s.root.join(".htaccess"), "RewriteEngine On\nRewriteRule ^a$ cc\n").unwrap();
        assert_eq!(s.serves("/a"), "cc");
    }
}
