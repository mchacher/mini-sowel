//! The home's time zone (spec 026 R2.5): a fixed list of IANA names, each with its POSIX rule
//! (`CET-1CEST,M3.5.0,M10.5.0/3`), and the offset from UTC a rule gives at an instant. The rules are
//! today's: a past date under another law gets today's rule. Only the `Mm.w.d[/time]` form of the
//! daylight-saving rules is read (`Jn` and `n` are refused); a zone whose rule is not a fixed yearly
//! rule is not in the list.

use crate::devices::{civil_from_days, days_from_civil};

/// The time zones the box knows, IANA name and POSIX rule, `UTC` first. Fixed, compiled in
/// (bound: this list, 59 entries); a stored name that is not here is dropped at load.
pub const TIME_ZONES: &[(&str, &str)] = &[
    ("UTC", "UTC0"),
    ("Europe/London", "GMT0BST,M3.5.0/1,M10.5.0"),
    ("Europe/Dublin", "GMT0IST,M3.5.0/1,M10.5.0"),
    ("Europe/Lisbon", "WET0WEST,M3.5.0/1,M10.5.0"),
    ("Atlantic/Canary", "WET0WEST,M3.5.0/1,M10.5.0"),
    ("Atlantic/Reykjavik", "GMT0"),
    ("Europe/Paris", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Brussels", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Luxembourg", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Amsterdam", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Berlin", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Zurich", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Monaco", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Madrid", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Rome", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Vienna", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Prague", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Warsaw", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Budapest", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Copenhagen", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Oslo", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Stockholm", "CET-1CEST,M3.5.0,M10.5.0/3"),
    ("Europe/Athens", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
    ("Europe/Helsinki", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
    ("Europe/Bucharest", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
    ("Europe/Kyiv", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
    ("Europe/Istanbul", "<+03>-3"),
    ("Europe/Moscow", "MSK-3"),
    ("America/New_York", "EST5EDT,M3.2.0,M11.1.0"),
    ("America/Toronto", "EST5EDT,M3.2.0,M11.1.0"),
    ("America/Chicago", "CST6CDT,M3.2.0,M11.1.0"),
    ("America/Denver", "MST7MDT,M3.2.0,M11.1.0"),
    ("America/Phoenix", "MST7"),
    ("America/Los_Angeles", "PST8PDT,M3.2.0,M11.1.0"),
    ("America/Vancouver", "PST8PDT,M3.2.0,M11.1.0"),
    ("America/Anchorage", "AKST9AKDT,M3.2.0,M11.1.0"),
    ("Pacific/Honolulu", "HST10"),
    ("America/Montreal", "EST5EDT,M3.2.0,M11.1.0"),
    ("America/Mexico_City", "CST6"),
    ("America/Sao_Paulo", "<-03>3"),
    ("America/Guadeloupe", "AST4"),
    ("America/Martinique", "AST4"),
    ("America/Cayenne", "<-03>3"),
    ("America/Miquelon", "<-03>3<-02>,M3.2.0,M11.1.0"),
    ("Indian/Reunion", "<+04>-4"),
    ("Indian/Mayotte", "EAT-3"),
    ("Pacific/Noumea", "<+11>-11"),
    ("Pacific/Tahiti", "<-10>10"),
    ("Africa/Abidjan", "GMT0"),
    ("Africa/Dakar", "GMT0"),
    ("Africa/Johannesburg", "SAST-2"),
    ("Asia/Dubai", "<+04>-4"),
    ("Asia/Kolkata", "IST-5:30"),
    ("Asia/Singapore", "<+08>-8"),
    ("Asia/Shanghai", "CST-8"),
    ("Asia/Tokyo", "JST-9"),
    ("Australia/Perth", "AWST-8"),
    ("Australia/Sydney", "AEST-10AEDT,M10.1.0,M4.1.0/3"),
    ("Pacific/Auckland", "NZST-12NZDT,M9.5.0,M4.1.0/3"),
];

/// The time zone of this name in the list (exact case), parsed.
pub fn find(name: &str) -> Option<TimeZone> {
    TIME_ZONES
        .iter()
        .find(|(known, _)| *known == name)
        .and_then(|(_, rule)| TimeZone::parse(rule).ok())
}

/// True when the name is in the list.
pub fn is_known(name: &str) -> bool {
    TIME_ZONES.iter().any(|(known, _)| *known == name)
}

/// A day of the year a rule names: `Mm.w.d`, month 1 to 12, week 1 to 5 (5: the last), weekday 0
/// (Sunday) to 6, at `time` seconds after the local midnight (02:00 by default, may be negative or
/// past 24 h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rule {
    month: u32,
    week: u32,
    weekday: u32,
    time: i64,
}

impl Rule {
    /// Seconds since 1970 of the rule's instant in `year`, read as local time (no offset).
    fn local_seconds(&self, year: i64) -> i64 {
        let first = days_from_civil(year, self.month, 1);
        // 1970-01-01 was a Thursday (4).
        let first_weekday = (first + 4).rem_euclid(7) as u32;
        let mut day = 1 + (self.weekday + 7 - first_weekday) % 7 + (self.week - 1) * 7;
        let length = month_length(year, self.month);
        while day > length {
            day -= 7;
        }
        (first + i64::from(day) - 1) * 86_400 + self.time
    }
}

fn month_length(year: i64, month: u32) -> u32 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    u32::try_from(next - days_from_civil(year, month, 1)).unwrap_or(31)
}

/// Daylight saving: its offset, and when it starts (read in standard time) and ends (read in
/// daylight time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dst {
    offset: i64,
    start: Rule,
    end: Rule,
}

/// A parsed POSIX rule. Offsets are seconds east of UTC (`CET-1` is +3600).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeZone {
    standard: i64,
    dst: Option<Dst>,
}

impl TimeZone {
    /// UTC.
    pub const UTC: TimeZone = TimeZone {
        standard: 0,
        dst: None,
    };

    /// Parses a POSIX rule (`std offset [dst [offset] ,start[/time],end[/time]]`).
    pub fn parse(text: &str) -> Result<TimeZone, String> {
        let mut p = Parser {
            bytes: text.as_bytes(),
            at: 0,
        };
        p.name()?;
        let standard = -p.offset()?;
        if p.done() {
            return Ok(TimeZone {
                standard,
                dst: None,
            });
        }
        p.name()?;
        let offset = if p.peek() == Some(b',') {
            standard + 3600
        } else {
            -p.offset()?
        };
        p.expect(b',')?;
        let start = p.rule()?;
        p.expect(b',')?;
        let end = p.rule()?;
        if !p.done() {
            return Err(format!("unexpected text at {} in {text}", p.at));
        }
        Ok(TimeZone {
            standard,
            dst: Some(Dst { offset, start, end }),
        })
    }

    /// The offset from UTC, in seconds east, at this instant (milliseconds since 1970, UTC).
    pub fn offset_at(&self, utc_ms: i64) -> i64 {
        let Some(dst) = self.dst else {
            return self.standard;
        };
        let utc = utc_ms.div_euclid(1000);
        let (year, _, _) = civil_from_days((utc + self.standard).div_euclid(86_400));
        let start = dst.start.local_seconds(year) - self.standard;
        let end = dst.end.local_seconds(year) - dst.offset;
        let in_dst = if start < end {
            utc >= start && utc < end
        } else {
            // Southern hemisphere: daylight saving spans the turn of the year.
            !(utc >= end && utc < start)
        };
        if in_dst { dst.offset } else { self.standard }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", byte as char, self.at))
        }
    }

    /// A zone name: at least 3 letters, or `<...>` of letters, digits, `+` and `-` (at least 3).
    fn name(&mut self) -> Result<(), String> {
        if self.peek() == Some(b'<') {
            self.at += 1;
            let begin = self.at;
            while self
                .peek()
                .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-')
            {
                self.at += 1;
            }
            let length = self.at - begin;
            self.expect(b'>')?;
            if length < 3 {
                return Err("quoted name too short".into());
            }
            return Ok(());
        }
        let begin = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
            self.at += 1;
        }
        if self.at - begin < 3 {
            return Err(format!("name expected at {begin}"));
        }
        Ok(())
    }

    /// A number of 1 or more digits, at most `max`.
    fn number(&mut self, max: i64) -> Result<i64, String> {
        let begin = self.at;
        let mut value: i64 = 0;
        while let Some(b) = self.peek().filter(u8::is_ascii_digit) {
            value = value * 10 + i64::from(b - b'0');
            if value > max {
                return Err(format!("number too large at {begin}"));
            }
            self.at += 1;
        }
        if self.at == begin {
            return Err(format!("number expected at {begin}"));
        }
        Ok(value)
    }

    /// `[+-]hh[:mm[:ss]]` in seconds, POSIX sign (positive is west of UTC); hours at most `max`.
    fn signed_time(&mut self, max: i64) -> Result<i64, String> {
        let sign = match self.peek() {
            Some(b'-') => {
                self.at += 1;
                -1
            }
            Some(b'+') => {
                self.at += 1;
                1
            }
            _ => 1,
        };
        let mut seconds = self.number(max)? * 3600;
        if self.peek() == Some(b':') {
            self.at += 1;
            seconds += self.number(59)? * 60;
            if self.peek() == Some(b':') {
                self.at += 1;
                seconds += self.number(59)?;
            }
        }
        Ok(sign * seconds)
    }

    fn offset(&mut self) -> Result<i64, String> {
        self.signed_time(24)
    }

    /// `Mm.w.d[/time]`; the `Jn` and `n` forms are refused.
    fn rule(&mut self) -> Result<Rule, String> {
        self.expect(b'M')?;
        let month = self.number(12)?;
        self.expect(b'.')?;
        let week = self.number(5)?;
        self.expect(b'.')?;
        let weekday = self.number(6)?;
        if month < 1 || week < 1 {
            return Err("month or week out of range".into());
        }
        let time = if self.peek() == Some(b'/') {
            self.at += 1;
            self.signed_time(167)?
        } else {
            7200
        };
        // The ranges were checked: the values fit.
        Ok(Rule {
            month: u32::try_from(month).map_err(|e| e.to_string())?,
            week: u32::try_from(week).map_err(|e| e.to_string())?,
            weekday: u32::try_from(weekday).map_err(|e| e.to_string())?,
            time,
        })
    }
}
