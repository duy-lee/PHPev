//! Dotted numeric versions (`8.4`, `8.4.27`, `11.4.13`), possibly followed by
//! a suffix. windows.php.net publishes `8.4.27-1` when it rebuilds 8.4.27 (e.g.
//! for a security fix in a bundled library), and other forms may follow, so any
//! suffix is accepted: the numbers decide the order, the suffix only breaks ties.
//!
//! - no suffix: the release itself;
//! - `alpha`, `beta`, `RC`, `dev`, `pre` (any case, with a number or not): a
//!   pre-release, older than the release;
//! - anything else (`-1`, `_2`, `a`, `-p1`): a rebuild, newer than the release,
//!   ordered by the first number in it, then by its text.

use std::cmp::Ordering;

/// Where a suffix places a version among others with the same numbers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Suffix {
    /// `dev` < `alpha` < `beta` < `pre` < `rc`, then the number after it.
    Pre(u8, u32, String),
    Release,
    /// The first number in the suffix (0 if none), then its text.
    Rebuild(u32, String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Parsed {
    numbers: Vec<u32>,
    suffix: Suffix,
}

/// `None` unless `v` starts with dotted numbers (`8`, `8.4`, `8.4.27`).
fn parse(v: &str) -> Option<Parsed> {
    let v = v.trim();
    let end = v.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(v.len());
    // `8.5.12.fix`: the last dot belongs to the suffix.
    let head = v[..end].strip_suffix('.').unwrap_or(&v[..end]);
    let numbers = head.split('.').map(|p| p.parse().ok()).collect::<Option<Vec<u32>>>()?;
    let text = v[head.len()..]
        .trim_start_matches(['-', '_', '.', '+', ' '])
        .to_ascii_lowercase();
    let first_number = |s: &str| -> u32 {
        let digits: String = s
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().unwrap_or(0)
    };
    let suffix = if text.is_empty() {
        Suffix::Release
    } else if let Some(kind) = ["dev", "alpha", "beta", "pre", "rc"]
        .iter()
        .position(|k| text.starts_with(k))
    {
        Suffix::Pre(kind as u8, first_number(&text), text)
    } else {
        Suffix::Rebuild(first_number(&text), text)
    };
    Some(Parsed { numbers, suffix })
}

/// Numeric components of a version, without its suffix; `None` if it does not
/// start with a number.
pub fn parts(v: &str) -> Option<Vec<u32>> {
    parse(v).map(|p| p.numbers)
}

/// Orders versions: numbers first (`8.10` > `8.9`), then the suffix
/// (`8.4.27-RC1` < `8.4.27` < `8.4.27-1` < `8.4.27-2`); unparsable ones sort first.
pub fn cmp(a: &str, b: &str) -> Ordering {
    parse(a).cmp(&parse(b))
}

/// True if `wanted` (e.g. `8.4`) equals `version` or is a component-wise prefix
/// of its numbers. A `wanted` with a suffix (`8.4.27-1`) must match exactly.
pub fn matches(wanted: &str, version: &str) -> bool {
    match (parse(wanted), parse(version)) {
        (Some(w), Some(v)) if w.suffix != Suffix::Release => w == v,
        (Some(w), Some(v)) => !w.numbers.is_empty() && v.numbers.starts_with(&w.numbers),
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

    #[test]
    fn rebuilds_are_newer_than_the_release() {
        assert_eq!(cmp("8.4.27-1", "8.4.27"), Ordering::Greater);
        assert_eq!(cmp("8.4.27-2", "8.4.27-1"), Ordering::Greater);
        assert_eq!(cmp("8.4.27-10", "8.4.27-9"), Ordering::Greater);
        assert_eq!(cmp("8.4.28", "8.4.27-1"), Ordering::Greater);
        assert_eq!(parts("8.4.27-1"), Some(vec![8, 4, 27]));
        assert!(matches("8.4", "8.4.27-1"));
        assert!(matches("8.4.27", "8.4.27-1"));
        assert!(matches("8.4.27-1", "8.4.27-1"));
        assert!(!matches("8.4.27-1", "8.4.27"));
        assert_eq!(best_match("8.4", ["8.4.27", "8.4.27-1"]), Some("8.4.27-1"));
    }

    #[test]
    fn any_suffix_keeps_the_version_in_place() {
        for odd in ["8.5.12a", "8.5.12_1", "8.5.12-p1", "8.5.12+build3", "8.5.12.fix"] {
            assert_eq!(parts(odd), Some(vec![8, 5, 12]), "{odd}");
            assert_eq!(cmp(odd, "8.5.12"), Ordering::Greater, "{odd}");
            assert_eq!(cmp(odd, "8.5.13"), Ordering::Less, "{odd}");
            assert!(matches("8.5", odd), "{odd}");
        }
        let mut list = vec!["8.4.27", "8.5.12_1", "8.3.36-1", "8.5.12", "8.5.13", "x"];
        list.sort_by(|a, b| cmp(b, a));
        assert_eq!(list, ["8.5.13", "8.5.12_1", "8.5.12", "8.4.27", "8.3.36-1", "x"]);
    }

    #[test]
    fn pre_releases_are_older_than_the_release() {
        assert_eq!(cmp("8.5.0-RC1", "8.5.0"), Ordering::Less);
        assert_eq!(cmp("8.5.0RC2", "8.5.0-rc1"), Ordering::Greater);
        assert_eq!(cmp("8.5.0-beta3", "8.5.0-RC1"), Ordering::Less);
        assert_eq!(cmp("8.5.0-alpha1", "8.4.27"), Ordering::Greater);
        assert_eq!(best_match("8.5", ["8.5.0-RC1", "8.5.0"]), Some("8.5.0"));
    }

    #[test]
    fn not_a_version() {
        assert_eq!(parts("latest"), None);
        assert_eq!(parts(""), None);
        assert!(!matches("8.4", "nightly"));
    }
}
