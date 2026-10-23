//! The PHPev block in the Windows hosts file (`phpev hosts sync`, S1).

pub const BEGIN: &str = "# BEGIN PHPev (managed by `phpev hosts sync`; edits inside are overwritten)";
pub const END: &str = "# END PHPev";

/// A hostname PHPev may write: lowercase DNS labels joined by dots.
pub fn is_valid_host(h: &str) -> bool {
    h.len() <= 253
        && h.contains('.')
        && h.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Returns `existing` with the PHPev block replaced by entries for `hosts`
/// (both 127.0.0.1 and ::1, so IPv6-first lookups do not stall). An empty list
/// removes the block. Lines outside the block are kept byte for byte.
pub fn render(existing: &str, hosts: &[String]) -> String {
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in existing.lines() {
        if line.trim() == BEGIN {
            inside = true;
        } else if inside && line.trim() == END {
            inside = false;
        } else if !inside {
            out.push(line);
        }
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let mut text = out.join(newline);
    if !hosts.is_empty() {
        if !text.is_empty() {
            // End the last kept line, then one blank line before our block.
            text.push_str(newline);
            text.push_str(newline);
        }
        let mut block = vec![BEGIN.to_owned()];
        for h in hosts {
            block.push(format!("127.0.0.1 {h}"));
            block.push(format!("::1 {h}"));
        }
        block.push(END.to_owned());
        text.push_str(&block.join(newline));
    }
    text.push_str(newline);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(is_valid_host("blog.localhost"));
        assert!(is_valid_host("my-app.test"));
        assert!(!is_valid_host("localhost"));
        assert!(!is_valid_host("Bad.test"));
        assert!(!is_valid_host("a b.test"));
        assert!(!is_valid_host("x.test\r\n1.2.3.4 evil.com"));
        assert!(!is_valid_host("-a.test"));
    }

    #[test]
    fn replaces_only_our_block_and_keeps_crlf() {
        let original = "# hosts\r\n127.0.0.1 mine.dev\r\n\r\n";
        let once = render(original, &["a.localhost".into()]);
        assert_eq!(
            once,
            format!(
                "# hosts\r\n127.0.0.1 mine.dev\r\n\r\n{BEGIN}\r\n127.0.0.1 a.localhost\r\n::1 a.localhost\r\n{END}\r\n"
            )
        );
        let twice = render(&once, &["b.localhost".into()]);
        assert!(twice.contains("b.localhost") && !twice.contains("a.localhost"));
        assert!(twice.starts_with("# hosts\r\n127.0.0.1 mine.dev\r\n"));
        assert_eq!(render(&twice, &[]), "# hosts\r\n127.0.0.1 mine.dev\r\n");
    }
}
