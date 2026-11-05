/// Local wall-clock fields shared by filenames and profile timestamps.
pub(crate) struct DateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

pub(crate) fn local_datetime(seconds: i64) -> DateTime {
    try_local_datetime(seconds).unwrap_or_else(|| utc_datetime(seconds))
}

#[cfg(unix)]
fn try_local_datetime(seconds: i64) -> Option<DateTime> {
    let raw_time = libc::time_t::try_from(seconds).ok()?;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` remain valid for the duration of the call.
    let result = unsafe { libc::localtime_r(&raw_time, tm.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: A non-null result means `tm` was initialized by `localtime_r`.
    Some(datetime_from_tm(unsafe { tm.assume_init() }))
}

#[cfg(windows)]
fn try_local_datetime(seconds: i64) -> Option<DateTime> {
    let raw_time = libc::time_t::try_from(seconds).ok()?;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` remain valid for the duration of the call.
    let result = unsafe { libc::localtime_s(tm.as_mut_ptr(), &raw_time) };
    if result != 0 {
        return None;
    }
    // SAFETY: A zero result means `tm` was initialized by `localtime_s`.
    Some(datetime_from_tm(unsafe { tm.assume_init() }))
}

#[cfg(not(any(unix, windows)))]
fn try_local_datetime(_seconds: i64) -> Option<DateTime> {
    None
}

#[cfg(any(unix, windows))]
fn datetime_from_tm(tm: libc::tm) -> DateTime {
    DateTime {
        year: i64::from(tm.tm_year) + 1900,
        month: (tm.tm_mon + 1).clamp(1, 12) as u32,
        day: tm.tm_mday.clamp(1, 31) as u32,
        hour: tm.tm_hour.clamp(0, 23) as u32,
        minute: tm.tm_min.clamp(0, 59) as u32,
        second: tm.tm_sec.clamp(0, 59) as u32,
    }
}

fn utc_datetime(seconds: i64) -> DateTime {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    DateTime {
        year,
        month: month as u32,
        day: day as u32,
        hour: seconds_of_day / 3_600,
        minute: (seconds_of_day % 3_600) / 60,
        second: seconds_of_day % 60,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_fallback_preserves_seconds_and_calendar_boundaries() {
        for (seconds, expected) in [
            (-1, (1969, 12, 31, 23, 59, 59)),
            (0, (1970, 1, 1, 0, 0, 0)),
            (1_709_251_199, (2024, 2, 29, 23, 59, 59)),
            (1_709_251_200, (2024, 3, 1, 0, 0, 0)),
        ] {
            let d = utc_datetime(seconds);
            assert_eq!((d.year, d.month, d.day, d.hour, d.minute, d.second), expected);
        }
    }
}
