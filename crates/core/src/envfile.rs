//! Minimal `.env` reader/editor for `phpev import`.
//! Edits keep every other line (comments, order, blank lines) untouched.

/// Value of `key`, unquoted; `None` if the key is absent or commented out.
pub fn get(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = split(line)?;
        (k == key).then(|| unquote(v))
    })
}

/// Sets `key` to `value`: replaces the active line, else the first commented
/// `# KEY=` line (Laravel's .env.example ships DB_HOST etc. commented), else
/// appends.
pub fn set(text: &str, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let quoted = quote(value);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let active = lines.iter().position(|l| split(l).is_some_and(|(k, _)| k == key));
    let commented = || {
        lines.iter().position(|l| {
            l.trim_start()
                .strip_prefix('#')
                .and_then(split)
                .is_some_and(|(k, _)| k == key)
        })
    };
    match active.or_else(commented) {
        Some(i) => lines[i] = format!("{key}={quoted}"),
        None => lines.push(format!("{key}={quoted}")),
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    out
}

/// Active keys, in file order.
pub fn keys(text: &str) -> Vec<&str> {
    text.lines().filter_map(|l| split(l).map(|(k, _)| k)).collect()
}

fn split(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    if line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (k, v) = line.split_once('=')?;
    let k = k.trim();
    (!k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some((k, v.trim()))
}

fn unquote(v: &str) -> String {
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|r| r.split_once(q)).map(|(inner, _)| inner) {
            return inner.to_owned();
        }
    }
    // Unquoted: an inline comment starts at " #".
    v.split(" #").next().unwrap_or(v).trim().to_owned()
}

fn quote(v: &str) -> String {
    if v.is_empty() || v.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:/-@+".contains(&b)) {
        v.to_owned()
    } else {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "APP_NAME=Laravel\nAPP_URL=http://localhost\n\nDB_CONNECTION=sqlite\n# DB_HOST=127.0.0.1\n# DB_PORT=3306\nDB_PASSWORD=\"p w\" # note\nMAIL_FROM='a@b'\n";

    #[test]
    fn reads_values() {
        assert_eq!(get(EXAMPLE, "APP_URL").as_deref(), Some("http://localhost"));
        assert_eq!(get(EXAMPLE, "DB_PASSWORD").as_deref(), Some("p w"));
        assert_eq!(get(EXAMPLE, "MAIL_FROM").as_deref(), Some("a@b"));
        assert_eq!(get(EXAMPLE, "DB_HOST"), None);
    }

    #[test]
    fn edits_in_place_and_uncomments() {
        let t = set(EXAMPLE, "APP_URL", "http://blog.localhost");
        let t = set(&t, "DB_PORT", "3307");
        let t = set(&t, "NEW_KEY", "a b");
        assert!(t.contains("APP_URL=http://blog.localhost\n"));
        assert!(t.contains("\nDB_PORT=3307\n") && !t.contains("# DB_PORT"));
        assert!(t.contains("# DB_HOST=127.0.0.1"));
        assert!(t.ends_with("NEW_KEY=\"a b\"\n"));
        assert_eq!(t.lines().count(), EXAMPLE.lines().count() + 1);
    }
}
