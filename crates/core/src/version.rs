//! Dotted numeric versions (`8.4`, `8.4.27`, `11.4.13`).

use std::cmp::Ordering;

/// Numeric components of a version; non-numeric input yields `None`.
pub fn parts(v: &str) -> Option<Vec<u32>> {
    v.trim().split('.').map(|p| p.parse().ok()).collect()
}

/// Orders versions numerically (`8.10` > `8.9`); unparsable ones sort first.
pub fn cmp(a: &str, b: &str) -> Ordering {
    parts(a).cmp(&parts(b))
}

/// True if `wanted` (e.g. `8.4`) equals `version` or is a component-wise prefix of it.
pub fn matches(wanted: &str, version: &str) -> bool {
    match (parts(wanted), parts(version)) {
        (Some(w), Some(v)) => !w.is_empty() && v.starts_with(&w),
        _ => false,
    }
}

/// The highest of `versions` matching `wanted`.
pub fn best_match<'a, I: IntoIterator<Item = &'a str>>(wanted: &str, versions: I) -> Option<&'a str> {
    versions
        .into_iter()
        .filter(|v| matches(wanted, v))
        .max_by(|a, b| cmp(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_ordering() {
        assert_eq!(cmp("8.10.0", "8.9.30"), Ordering::Greater);
        assert_eq!(cmp("11.4.13", "11.4.2"), Ordering::Greater);
    }

    #[test]
    fn prefix_matching() {
        assert!(matches("8.4", "8.4.27"));
        assert!(matches("8.4.27", "8.4.27"));
        assert!(!matches("8.4", "8.40.1"));
        assert!(!matches("8", "x"));
        assert_eq!(best_match("8.4", ["8.3.36", "8.4.2", "8.4.27"]), Some("8.4.27"));
        assert_eq!(best_match("7", ["8.3.36"]), None);
    }
}
