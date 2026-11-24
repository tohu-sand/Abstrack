//! The local time for the dates written into annotations, and the name of the
//! person writing when none is set.

use annot::PdfDate;

/// Now, in local time with its offset from UTC, as Acrobat writes dates.
pub fn now() -> PdfDate {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    PdfDate::from_timestamp(seconds, offset_minutes())
}

/// The name Acrobat would use by default: the login name.
pub fn login_name() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default()
}

#[cfg(windows)]
fn offset_minutes() -> i16 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut info = TIME_ZONE_INFORMATION::default();
    // SAFETY: the function fills the structure it is given and keeps no reference to it.
    let kind = unsafe { GetTimeZoneInformation(&mut info) };
    // 1 and 2 are TIME_ZONE_ID_STANDARD and TIME_ZONE_ID_DAYLIGHT; otherwise Bias alone applies.
    let extra = match kind {
        1 => info.StandardBias,
        2 => info.DaylightBias,
        _ => 0,
    };
    // Bias is UTC minus local time.
    (-(info.Bias + extra)) as i16
}

#[cfg(not(windows))]
fn offset_minutes() -> i16 {
    // `date +%z` prints the offset as +HHMM.
    let Ok(out) = std::process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    match (digits.get(..2).and_then(|h| h.parse::<i16>().ok()), digits.get(2..4).and_then(|m| m.parse::<i16>().ok())) {
        (Some(h), Some(m)) => sign * (h * 60 + m),
        _ => 0,
    }
}
