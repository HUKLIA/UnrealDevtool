//! Wall-clock helpers for scheduling a build at a time of day.
//!
//! Standard-library time is UTC-only, and this needs the user's local time, so
//! it asks Windows for it (`GetLocalTime`) instead of pulling in a date crate
//! for one number.

/// Seconds since local midnight, or `None` if Windows will not say.
pub fn local_seconds_of_day() -> Option<u32> {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16, month: u16, day_of_week: u16, day: u16,
        hour: u16, minute: u16, second: u16, millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(out: *mut SystemTime);
    }
    let mut t = SystemTime::default();
    // SAFETY: `t` is a live, correctly laid out SYSTEMTIME; the call only writes into it.
    unsafe { GetLocalTime(&mut t) };
    Some(t.hour as u32 * 3600 + t.minute as u32 * 60 + t.second as u32)
}

/// Parses `"23:30"`, `"7:05"` or `"0730"` into (hour, minute).
pub fn parse_hhmm(text: &str) -> Option<(u32, u32)> {
    let t = text.trim();
    let (h, m) = match t.split_once(':') {
        Some((h, m)) => (h, m),
        None if t.len() == 4 => t.split_at(2),
        None => return None,
    };
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Seconds from `now` (seconds since midnight) until the next `hour:minute`.
/// A time that has already passed today means tomorrow; exactly now means in a
/// full day, so a schedule set "for now" does not fire instantly by accident.
pub fn secs_until(hour: u32, minute: u32, now: u32) -> u64 {
    let target = hour * 3600 + minute * 60;
    if target > now { (target - now) as u64 } else { (86_400 - now + target) as u64 }
}

/// "2h 10m" / "45m" / "30s" for a countdown.
pub fn countdown(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 { format!("{h}h {m:02}m") } else if m > 0 { format!("{m}m") } else { format!("{s}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse_in_the_forms_people_type() {
        assert_eq!(parse_hhmm("23:30"), Some((23, 30)));
        assert_eq!(parse_hhmm(" 7:05 "), Some((7, 5)));
        assert_eq!(parse_hhmm("0730"), Some((7, 30)));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("12:60"), None);
        assert_eq!(parse_hhmm("noon"), None);
        assert_eq!(parse_hhmm(""), None);
    }

    #[test]
    fn the_next_occurrence_wraps_past_midnight() {
        let at = |h: u32, m: u32| h * 3600 + m * 60;
        assert_eq!(secs_until(23, 0, at(22, 0)), 3600);
        assert_eq!(secs_until(1, 0, at(23, 0)), 2 * 3600);
        assert_eq!(secs_until(9, 0, at(9, 0)), 86_400, "exactly now means tomorrow");
        assert_eq!(countdown(7800), "2h 10m");
        assert_eq!(countdown(300), "5m");
    }

    #[test]
    fn windows_reports_a_plausible_local_time() {
        let s = local_seconds_of_day().expect("local time");
        assert!(s < 86_400);
    }
}
