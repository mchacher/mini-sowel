//! Dates as Sowel writes them: ISO 8601 in UTC, with milliseconds (`2026-11-21T09:12:03.120Z`).

const MS_PER_DAY: u64 = 86_400_000;

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_written_as_sowel_writes_it() {
        assert_eq!(format_iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_iso(1_795_252_323_120), "2026-11-21T09:12:03.120Z");
        assert_eq!(format_iso(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
