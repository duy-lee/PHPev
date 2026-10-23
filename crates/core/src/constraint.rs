//! Composer version constraints, enough for `require.php` (DESIGN_SPEC §6).
//!
//! Supports `||`/`|` (or), `,`/space (and), `^`, `~`, `>=`, `>`, `<=`, `<`,
//! `!=`, `=`/`==`, wildcards (`8.*`, `8.2.x`, `*`), hyphen ranges
//! (`8.1 - 8.3`) and ignores stability flags (`@dev`) and a leading `v`.

use std::cmp::Ordering;

type V = [u32; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Ge,
    Gt,
    Le,
    Lt,
    Eq,
    Ne,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    /// OR of ANDs.
    any_of: Vec<Vec<(Op, V)>>,
}

impl Constraint {
    pub fn parse(text: &str) -> Option<Self> {
        let any_of = text
            .split("||")
            .flat_map(|p| p.split('|'))
            .map(parse_and)
            .collect::<Option<Vec<_>>>()?;
        (!any_of.is_empty()).then_some(Self { any_of })
    }

    /// True if `version` (e.g. `8.4.27`) satisfies the constraint.
    pub fn matches(&self, version: &str) -> bool {
        let Some(v) = parse_version(version) else { return false };
        self.any_of
            .iter()
            .any(|all| all.iter().all(|(op, bound)| test(*op, &v, bound)))
    }
}

fn test(op: Op, v: &V, bound: &V) -> bool {
    let ord = v.cmp(bound);
    match op {
        Op::Ge => ord != Ordering::Less,
        Op::Gt => ord == Ordering::Greater,
        Op::Le => ord != Ordering::Greater,
        Op::Lt => ord == Ordering::Less,
        Op::Eq => ord == Ordering::Equal,
        Op::Ne => ord != Ordering::Equal,
    }
}

fn parse_and(part: &str) -> Option<Vec<(Op, V)>> {
    let part = part.trim();
    if part.is_empty() {
        return None;
    }
    // Hyphen range: "A - B" (spaces around the dash are required by Composer).
    if let Some((low, high)) = part.split_once(" - ") {
        let low = parse_partial(low.trim())?;
        let (high_v, high_len) = parse_partial(high.trim())?;
        let upper = if high_len < 3 {
            (Op::Lt, bump(high_v, high_len - 1))
        } else {
            (Op::Le, high_v)
        };
        return Some(vec![(Op::Ge, low.0), upper]);
    }
    let mut out = Vec::new();
    for atom in part.split([',', ' ']).filter(|a| !a.trim().is_empty()) {
        out.extend(parse_atom(atom.trim())?);
    }
    Some(out)
}

fn parse_atom(atom: &str) -> Option<Vec<(Op, V)>> {
    let atom = atom.split('@').next()?.trim();
    if atom == "*" || atom.is_empty() {
        return Some(vec![(Op::Ge, [0, 0, 0])]);
    }
    for (prefix, op) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        ("!=", Op::Ne),
        ("==", Op::Eq),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(rest) = atom.strip_prefix(prefix) {
            let (v, _) = parse_partial(rest.trim())?;
            return Some(vec![(op, v)]);
        }
    }
    if let Some(rest) = atom.strip_prefix('^') {
        let (v, len) = parse_partial(rest)?;
        // Bump the first non-zero component (or the last given one if all are zero).
        let i = v[..len].iter().position(|&p| p != 0).unwrap_or(len - 1);
        return Some(vec![(Op::Ge, v), (Op::Lt, bump(v, i))]);
    }
    if let Some(rest) = atom.strip_prefix('~') {
        let (v, len) = parse_partial(rest)?;
        // ~1.2 → <2.0 ; ~1.2.3 → <1.3.0
        let i = len.saturating_sub(2);
        return Some(vec![(Op::Ge, v), (Op::Lt, bump(v, i))]);
    }
    // Wildcards: 8.*, 8.2.x
    let parts: Vec<&str> = atom.trim_start_matches(['v', 'V']).split('.').collect();
    if let Some(w) = parts.iter().position(|p| matches!(*p, "*" | "x" | "X")) {
        if w == 0 {
            return Some(vec![(Op::Ge, [0, 0, 0])]);
        }
        let (v, _) = parse_partial(&parts[..w].join("."))?;
        return Some(vec![(Op::Ge, v), (Op::Lt, bump(v, w - 1))]);
    }
    let (v, _) = parse_partial(atom)?;
    Some(vec![(Op::Eq, v)])
}

/// `8.2` → ([8,2,0], 2 given components).
fn parse_partial(s: &str) -> Option<(V, usize)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let nums: Vec<u32> = s.split('.').take(3).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if nums.is_empty() {
        return None;
    }
    let mut v = [0; 3];
    v[..nums.len()].copy_from_slice(&nums);
    Some((v, nums.len()))
}

fn parse_version(s: &str) -> Option<V> {
    parse_partial(s).map(|(v, _)| v)
}

/// Increments component `i` and zeroes the rest: bump([8,2,5], 0) = [9,0,0].
fn bump(mut v: V, i: usize) -> V {
    v[i] += 1;
    for p in v.iter_mut().skip(i + 1) {
        *p = 0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(c: &str, v: &str) -> bool {
        Constraint::parse(c).unwrap_or_else(|| panic!("parse {c}")).matches(v)
    }

    #[test]
    fn caret_and_tilde() {
        assert!(ok("^8.2", "8.2.0") && ok("^8.2", "8.4.27") && !ok("^8.2", "9.0.0") && !ok("^8.2", "8.1.34"));
        assert!(ok("~8.1", "8.4.0") && !ok("~8.1", "9.0.0"));
        assert!(ok("~8.1.3", "8.1.9") && !ok("~8.1.3", "8.2.0"));
        assert!(ok("^0.3", "0.3.9") && !ok("^0.3", "0.4.0"));
    }

    #[test]
    fn ranges_and_or() {
        assert!(ok(">=8.1 <8.4", "8.3.36") && !ok(">=8.1 <8.4", "8.4.0"));
        assert!(ok(">=8.1,<8.4", "8.1.0"));
        assert!(ok("^7.4|^8.0", "8.4.27") && ok("^7.4 || ^8.0", "7.4.33") && !ok("^7.4||^8.0", "7.3.0"));
        assert!(ok("8.1 - 8.3", "8.3.99") && !ok("8.1 - 8.3", "8.4.0"));
        assert!(ok("!=8.2.0", "8.2.1") && !ok("!=8.2.0", "8.2.0"));
    }

    #[test]
    fn wildcards_exact_and_flags() {
        assert!(ok("8.*", "8.4.1") && !ok("8.*", "7.4.0"));
        assert!(ok("8.2.x", "8.2.30") && !ok("8.2.x", "8.3.0"));
        assert!(ok("*", "5.6.0"));
        assert!(ok("8.2.10", "8.2.10") && !ok("8.2.10", "8.2.11"));
        assert!(ok(">=8.2@dev", "8.3.0"));
        assert!(ok("v8.2.*", "8.2.1"));
        assert!(Constraint::parse("banana").is_none());
    }
}
