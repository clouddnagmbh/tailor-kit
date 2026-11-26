// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! ISO 8601 timestamps of the hosts' JSON (`2026-11-20T09:00:00Z`, `…+02:00`, with or
//! without fractions) as Unix seconds.

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix seconds of `text`; `0` when it is not a timestamp.
pub fn unix_seconds(text: &str) -> i64 {
    parse(text).unwrap_or(0)
}

fn parse(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', ' '])?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let (clock, offset) = match rest.find(['Z', 'z', '+']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => match rest.rfind('-') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "Z"),
        },
    };
    let mut c = clock.split(':');
    let h: i64 = c.next()?.parse().ok()?;
    let min: i64 = c.next()?.parse().ok()?;
    let sec: i64 = c
        .next()
        .map(|s| s.split('.').next().unwrap_or("0"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let shift = match offset.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            let digits: String = offset[1..].chars().filter(char::is_ascii_digit).collect();
            let oh: i64 = digits.get(0..2)?.parse().ok()?;
            let om: i64 = digits.get(2..4).and_then(|s| s.parse().ok()).unwrap_or(0);
            sign * (oh * 3600 + om * 60)
        }
        _ => 0,
    };
    Some(days_from_civil(y, m, day) * 86_400 + h * 3600 + min * 60 + sec - shift)
}

#[cfg(test)]
mod tests {
    use super::unix_seconds;

    #[test]
    fn parses_the_forms_the_hosts_use() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z"), 0);
        assert_eq!(unix_seconds("2026-11-20T09:00:00Z"), 1_795_165_200);
        assert_eq!(unix_seconds("2026-11-20T09:00:00.123Z"), 1_795_165_200);
        assert_eq!(unix_seconds("2026-11-20T11:00:00+02:00"), 1_795_165_200);
        assert_eq!(unix_seconds("2026-11-20T04:00:00.5-05:00"), 1_795_165_200);
        assert_eq!(unix_seconds("garbage"), 0);
    }
}
