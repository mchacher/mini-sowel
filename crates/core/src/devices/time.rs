//! Dates as Sowel writes them: ISO 8601 in UTC, with milliseconds (`2026-11-21T09:12:03.120Z`).
//! The caller gives the time as milliseconds since 1970-01-01 UTC.

const MS_PER_DAY: u64 = 86_400_000;

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar.
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// (year, month, day) to days since 1970-01-01.
pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-11-21T09:12:03.120Z` for the given milliseconds since the epoch.
pub fn format_iso(ms: u64) -> String {
    let days = i64::try_from(ms / MS_PER_DAY).unwrap_or(i64::MAX / 2);
    let rest = ms % MS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

/// The milliseconds since the epoch of a date written by `format_iso`; `None` for any other
/// form.
pub fn parse_iso(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() != 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' {
        return None;
    }
    if b[16] != b':' || b[19] != b'.' || b[23] != b'Z' {
        return None;
    }
    let number = |from: usize, to: usize| text.get(from..to)?.parse::<u32>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second, milli) = (
        number(11, 13)?,
        number(14, 16)?,
        number(17, 19)?,
        number(20, 23)?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 59 {
        return None;
    }
    let days = u64::try_from(days_from_civil(i64::from(year), month, day)).ok()?;
    Some(
        days * MS_PER_DAY
            + u64::from(hour) * 3_600_000
            + u64::from(minute) * 60_000
            + u64::from(second) * 1000
            + u64::from(milli),
    )
}
