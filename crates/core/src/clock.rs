//! UTC calendar time without a date/time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Today's UTC date as `YYYY-MM-DD`.
pub fn today() -> String {
    let (y, m, d) = civil_from_days((now_secs() / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Current UTC time as `YYYY-MM-DD HH:MM:SS`.
pub fn timestamp() -> String {
    let secs = now_secs();
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (H. Hinnant).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_749), (2026, 10, 23));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn timestamp_shape() {
        let t = timestamp();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[10..11], " ");
    }
}
