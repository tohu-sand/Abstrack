//! PDF dates (ISO 32000-1, 7.9.4): `D:YYYYMMDDHHmmSSOHH'mm'`, every part after the year optional.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Offset from UTC in minutes; `None` when the string gives no zone.
    pub offset_minutes: Option<i16>,
}

impl PdfDate {
    pub fn parse(text: &str) -> Option<PdfDate> {
        let s = text.trim();
        let s = s.strip_prefix("D:").unwrap_or(s);
        let bytes = s.as_bytes();
        let digits = |from: usize, len: usize| -> Option<u32> {
            let part = bytes.get(from..from + len)?;
            if !part.iter().all(u8::is_ascii_digit) {
                return None;
            }
            std::str::from_utf8(part).ok()?.parse().ok()
        };
        let year = digits(0, 4)? as i32;
        let mut pos = 4;
        let mut field = |default: u32, max: u32| -> u32 {
            match digits(pos, 2) {
                Some(v) if v <= max => {
                    pos += 2;
                    v
                }
                _ => default,
            }
        };
        let month = field(1, 12).max(1) as u8;
        let day = field(1, 31).max(1) as u8;
        let hour = field(0, 23) as u8;
        let minute = field(0, 59) as u8;
        let second = field(0, 59) as u8;
        let offset_minutes = match bytes.get(pos) {
            Some(b'Z') => Some(0),
            Some(&sign @ (b'+' | b'-')) => {
                let oh = digits(pos + 1, 2).unwrap_or(0) as i16;
                let after = pos + 3;
                let om_at = if bytes.get(after) == Some(&b'\'') { after + 1 } else { after };
                let om = digits(om_at, 2).unwrap_or(0) as i16;
                let total = oh * 60 + om;
                Some(if sign == b'-' { -total } else { total })
            }
            _ => None,
        };
        Some(PdfDate {
            year,
            month,
            day,
            hour,
            minute,
            second,
            offset_minutes,
        })
    }

    /// Seconds since 1970-01-01 UTC. A date without a zone is taken as UTC.
    pub fn timestamp(&self) -> i64 {
        let days = days_from_civil(self.year, self.month as u32, self.day as u32);
        let local = days * 86_400
            + self.hour as i64 * 3600
            + self.minute as i64 * 60
            + self.second as i64;
        local - self.offset_minutes.unwrap_or(0) as i64 * 60
    }

    /// The form Acrobat writes, e.g. `D:20260124170922+09'00'`.
    pub fn to_pdf_string(&self) -> String {
        let mut s = format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        );
        match self.offset_minutes {
            Some(0) => s.push('Z'),
            Some(off) => {
                let sign = if off < 0 { '-' } else { '+' };
                let off = off.unsigned_abs();
                s.push_str(&format!("{sign}{:02}'{:02}'", off / 60, off % 60));
            }
            None => {}
        }
        s
    }

    /// `ts` in seconds since 1970-01-01 UTC, given as the local time of the
    /// zone `offset_minutes` east of UTC.
    pub fn from_timestamp(ts: i64, offset_minutes: i16) -> PdfDate {
        let local = ts + offset_minutes as i64 * 60;
        let days = local.div_euclid(86_400);
        let secs = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        PdfDate {
            year,
            month: month as u8,
            day: day as u8,
            hour: (secs / 3600) as u8,
            minute: (secs / 60 % 60) as u8,
            second: (secs % 60) as u8,
            offset_minutes: Some(offset_minutes),
        }
    }
}

impl PartialOrd for PdfDate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PdfDate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp().cmp(&other.timestamp())
    }
}

/// Shown as written (local time of the writer), e.g. `2026/01/24 17:09`.
impl fmt::Display for PdfDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}/{:02}/{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

// Howard Hinnant's algorithms for the proleptic Gregorian calendar.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_acrobat_form() {
        let d = PdfDate::parse("D:20260124170922+09'00'").unwrap();
        assert_eq!((d.year, d.month, d.day, d.hour, d.minute, d.second), (2026, 1, 24, 17, 9, 22));
        assert_eq!(d.offset_minutes, Some(540));
        assert_eq!(d.to_pdf_string(), "D:20260124170922+09'00'");
    }

    #[test]
    fn orders_across_zones() {
        let tokyo = PdfDate::parse("D:20260124170000+09'00'").unwrap();
        let utc = PdfDate::parse("D:20260124080001Z").unwrap();
        assert!(tokyo < utc);
    }

    #[test]
    fn partial_dates() {
        let d = PdfDate::parse("D:2025").unwrap();
        assert_eq!((d.month, d.day, d.offset_minutes), (1, 1, None));
        let d = PdfDate::parse("D:20251103120451-05'30").unwrap();
        assert_eq!(d.offset_minutes, Some(-330));
        assert!(PdfDate::parse("yesterday").is_none());
    }

    #[test]
    fn timestamp_round_trip() {
        let d = PdfDate::parse("D:20240229235959+09'00'").unwrap();
        let back = PdfDate::from_timestamp(d.timestamp(), 540);
        assert_eq!(back, d);
    }
}
