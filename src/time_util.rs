use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeedBucket {
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Older,
}

impl FeedBucket {
    pub fn label(self) -> &'static str {
        match self {
            FeedBucket::ThisWeek => "This week",
            FeedBucket::LastWeek => "Last week",
            FeedBucket::ThisMonth => "This month",
            FeedBucket::LastMonth => "Last month",
            FeedBucket::Older => "Older",
        }
    }
}

pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(target_os = "windows")]
pub fn now_local() -> LocalDateTime {
    #[repr(C)]
    struct SystemTimeWin {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }

    unsafe extern "system" {
        fn GetLocalTime(lp_system_time: *mut SystemTimeWin);
    }

    let mut st = SystemTimeWin {
        w_year: 2026,
        w_month: 1,
        w_day_of_week: 0,
        w_day: 1,
        w_hour: 0,
        w_minute: 0,
        w_second: 0,
        w_milliseconds: 0,
    };
    unsafe {
        GetLocalTime(&mut st);
    }
    LocalDateTime {
        year: st.w_year as i32,
        month: st.w_month as u8,
        day: st.w_day as u8,
        hour: st.w_hour as u8,
        minute: st.w_minute as u8,
        second: st.w_second as u8,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn now_local() -> LocalDateTime {
    // Fallback UTC breakdown for non-Windows builds
    let secs = now_unix_secs().max(0);
    let days = secs / 86_400;
    let sod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    LocalDateTime {
        year,
        month,
        day,
        hour: (sod / 3600) as u8,
        minute: ((sod % 3600) / 60) as u8,
        second: (sod % 60) as u8,
    }
}

/// Howard Hinnant's Gregorian civil date -> days since 1970-01-01.
pub fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(not(target_os = "windows"))]
fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u8, d as u8)
}

pub fn parse_ymd(s: &str) -> Option<(i32, u8, u8)> {
    let trimmed = s.trim();
    let mut parts = trimmed.split('-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.get(0..2)?.parse().ok()?;
    if (1900..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
        Some((y, m, d))
    } else {
        None
    }
}

/// Returns day of week where Sunday = 0, Monday = 1, ..., Saturday = 6.
/// (1970-01-01 was a Thursday = 4.)
pub fn weekday_sun0(days_since_epoch: i64) -> i64 {
    (days_since_epoch + 4).rem_euclid(7)
}

/// Classifies a release into TIDAL's exact Sunday-start calendar Feed buckets:
/// - `This week`: on or after the Sunday starting the current week
/// - `Last week`: the 7-day Sunday..=Saturday block immediately preceding this week
/// - `This month`: earlier in the current calendar month
/// - `Last month`: previous calendar month
/// - `Older`: anything older or without an exact verified date
pub fn classify_release_bucket(
    release_date: Option<&str>,
    _year: Option<u16>,
    _is_latest: bool,
    today: LocalDateTime,
) -> FeedBucket {
    if let Some((ry, rm, rd)) = release_date.and_then(parse_ymd) {
        let today_days = days_from_civil(today.year, today.month, today.day);
        let rel_days = days_from_civil(ry, rm, rd);

        let this_week_start = today_days - weekday_sun0(today_days);
        let last_week_start = this_week_start - 7;

        if rel_days >= this_week_start {
            return FeedBucket::ThisWeek;
        }
        if rel_days >= last_week_start {
            return FeedBucket::LastWeek;
        }
        if ry == today.year && rm == today.month {
            return FeedBucket::ThisMonth;
        }
        let (prev_y, prev_m) = if today.month == 1 {
            (today.year - 1, 12)
        } else {
            (today.year, today.month - 1)
        };
        if ry == prev_y && rm == prev_m {
            return FeedBucket::LastMonth;
        }
        return FeedBucket::Older;
    }

    // Without a verified exact release date, never falsely place a release in "This month"
    FeedBucket::Older
}

/// Computes the Unix timestamp (seconds) of the most recent occurrence of local `hour:minute`.
pub fn most_recent_scheduled_unix(
    now_unix: i64,
    local: LocalDateTime,
    target_hour: u8,
    target_minute: u8,
) -> i64 {
    let now_sod = (local.hour as i64) * 3600 + (local.minute as i64) * 60 + (local.second as i64);
    let target_sod = (target_hour.min(23) as i64) * 3600 + (target_minute.min(59) as i64) * 60;
    let delta = if now_sod >= target_sod {
        now_sod - target_sod
    } else {
        now_sod + (86_400 - target_sod)
    };
    now_unix - delta
}

pub fn format_12h_time(hour: u8, minute: u8) -> String {
    let h24 = hour % 24;
    let period = if h24 < 12 { "AM" } else { "PM" };
    let h12 = match h24 % 12 {
        0 => 12,
        n => n,
    };
    format!("{h12}:{minute:02} {period}")
}

pub fn format_relative_age(last_unix: Option<i64>, now_unix: i64) -> String {
    let Some(ts) = last_unix else {
        return "Never refreshed".to_string();
    };
    let diff = (now_unix - ts).max(0);
    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{} min ago", diff / 60)
    } else if diff < 86_400 {
        let hrs = diff / 3600;
        if hrs == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hrs} hours ago")
        }
    } else {
        let days = diff / 86_400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{days} days ago")
        }
    }
}

pub fn format_short_date(ymd: &str) -> Option<String> {
    let (y, m, d) = parse_ymd(ymd)?;
    let mon = match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };
    Some(format!("{mon} {d}, {y}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_granular_tidal_buckets() {
        // Monday, Nov 23, 2026 -> This week starts Sun Nov 22; Last week is Sun Nov 15 .. Sat Nov 21
        let today = LocalDateTime {
            year: 2026,
            month: 11,
            day: 23,
            hour: 14,
            minute: 30,
            second: 0,
        };
        assert_eq!(weekday_sun0(days_from_civil(2026, 11, 23)), 1); // Monday
        assert_eq!(
            classify_release_bucket(Some("2026-11-23"), Some(2026), false, today),
            FeedBucket::ThisWeek
        );
        assert_eq!(
            classify_release_bucket(Some("2026-11-22"), Some(2026), false, today),
            FeedBucket::ThisWeek
        );
        assert_eq!(
            classify_release_bucket(Some("2026-11-21"), Some(2026), false, today),
            FeedBucket::LastWeek
        );
        assert_eq!(
            classify_release_bucket(Some("2026-11-15"), Some(2026), false, today),
            FeedBucket::LastWeek
        );
        // Friday, Nov 13 is before Sun Nov 15 -> This month!
        assert_eq!(
            classify_release_bucket(Some("2026-11-13"), Some(2026), false, today),
            FeedBucket::ThisMonth
        );
        assert_eq!(
            classify_release_bucket(Some("2026-11-02"), Some(2026), false, today),
            FeedBucket::ThisMonth
        );
        assert_eq!(
            classify_release_bucket(Some("2026-10-15"), Some(2026), false, today),
            FeedBucket::LastMonth
        );
        assert_eq!(
            classify_release_bucket(Some("2026-08-01"), Some(2026), false, today),
            FeedBucket::Older
        );
    }

    #[test]
    fn computes_daily_schedule_and_missed_window() {
        let local = LocalDateTime {
            year: 2026,
            month: 11,
            day: 23,
            hour: 14,
            minute: 0,
            second: 0,
        };
        let now = 1_795_442_400_i64;
        // 12:00 AM today was 14 hours ago
        let sched_midnight = most_recent_scheduled_unix(now, local, 0, 0);
        assert_eq!(now - sched_midnight, 14 * 3600);

        // 6:00 PM hasn't happened yet today, so most recent was 20 hours ago (yesterday 6 PM)
        let sched_6pm = most_recent_scheduled_unix(now, local, 18, 0);
        assert_eq!(now - sched_6pm, 20 * 3600);
    }

    #[test]
    fn formats_12h_times() {
        assert_eq!(format_12h_time(0, 0), "12:00 AM");
        assert_eq!(format_12h_time(9, 30), "9:30 AM");
        assert_eq!(format_12h_time(12, 0), "12:00 PM");
        assert_eq!(format_12h_time(21, 15), "9:15 PM");
    }
}
