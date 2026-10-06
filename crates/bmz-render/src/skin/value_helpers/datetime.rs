use super::*;

/// beatoraja numeric refs 243..249. A missing value uses Integer.MIN_VALUE
/// for Lua; the number renderer separately treats that sentinel as absent.
pub fn last_play_datetime_numbers(played_at: Option<i64>) -> [i32; 7] {
    let Some(seconds) = played_at.filter(|seconds| *seconds > 0) else {
        return [i32::MIN; 7];
    };
    let mut values = [i32::MIN; 7];
    values[0] = i32::try_from(seconds).unwrap_or(i32::MIN);
    if let Some(date) = unix_seconds_to_local_datetime(seconds) {
        values[1..].copy_from_slice(&[
            date.year,
            date.month as i32,
            date.day as i32,
            date.hour as i32,
            date.minute as i32,
            date.second as i32,
        ]);
    }
    values
}

pub(super) fn lookup_text(values: &[(TextSlot, String)], slot: TextSlot) -> String {
    values
        .iter()
        .find(|(candidate, _)| *candidate == slot)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

pub(super) fn lookup_number(values: &[(NumberSlot, i64)], slot: NumberSlot) -> i64 {
    values
        .iter()
        .find(|(candidate, _)| *candidate == slot)
        .map(|(_, value)| *value)
        .unwrap_or_default()
}

pub(super) fn current_datetime_number(ref_id: i32) -> Option<i64> {
    let seconds =
        SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs().min(i64::MAX as u64) as i64;
    let date = unix_seconds_to_local_datetime(seconds)
        .unwrap_or_else(|| unix_seconds_to_utc_datetime(seconds));
    match ref_id {
        21 => Some(date.year as i64),
        22 => Some(date.month as i64),
        23 => Some(date.day as i64),
        24 => Some(date.hour as i64),
        25 => Some(date.minute as i64),
        26 => Some(date.second as i64),
        _ => None,
    }
}

pub(super) fn skin_judge_region_text(state: &SkinDrawState, region: usize) -> Option<String> {
    if region >= MAX_JUDGE_REGIONS || state.judge_ms[region].is_none() {
        return None;
    }
    judge_index_text(state.judge_index[region]?).map(str::to_string)
}

pub(super) fn skin_judge_timing_text(state: &SkinDrawState, region: usize) -> Option<&'static str> {
    if region >= MAX_JUDGE_REGIONS || state.judge_ms[region].is_none() {
        return None;
    }
    match state.judge_timing_sign[region] {
        Some(1) => Some("FAST"),
        Some(-1) => Some("SLOW"),
        _ => None,
    }
}

pub(super) fn judge_index_text(index: usize) -> Option<&'static str> {
    Some(match index {
        0 => "PGREAT",
        1 => "GREAT",
        2 => "GOOD",
        3 => "BAD",
        4 => "POOR",
        5 => "EMPTY POOR",
        _ => return None,
    })
}

pub(super) fn skin_judge_region_color(
    state: &SkinDrawState,
    region: usize,
    alpha: f32,
) -> Option<Color> {
    if region >= MAX_JUDGE_REGIONS || state.judge_ms[region].is_none() {
        return None;
    }
    Some(match state.judge_index[region]? {
        0 => Color::rgba(112.0 / 255.0, 224.0 / 255.0, 1.0, alpha),
        1 | 2 => Color::rgba(1.0, 224.0 / 255.0, 80.0 / 255.0, alpha),
        3..=5 => Color::rgba(1.0, 88.0 / 255.0, 82.0 / 255.0, alpha),
        _ => return None,
    })
}

pub(super) fn skin_judge_timing_color(
    state: &SkinDrawState,
    region: usize,
    alpha: f32,
) -> Option<Color> {
    if region >= MAX_JUDGE_REGIONS || state.judge_ms[region].is_none() {
        return None;
    }
    Some(match state.judge_timing_sign[region]? {
        1 => Color::rgba(72.0 / 255.0, 176.0 / 255.0, 1.0, alpha),
        -1 => Color::rgba(1.0, 88.0 / 255.0, 82.0 / 255.0, alpha),
        _ => return None,
    })
}

#[cfg(unix)]
pub(super) fn unix_seconds_to_local_datetime(seconds: i64) -> Option<SkinDateTime> {
    let raw_time = seconds as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` are valid pointers for the duration of the call.
    // `localtime_r` initializes `tm` on success and returns null on failure.
    let result = unsafe { libc::localtime_r(&raw_time, tm.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: The non-null result means `tm` has been fully initialized.
    let tm = unsafe { tm.assume_init() };
    Some(datetime_from_tm(tm))
}

#[cfg(windows)]
pub(super) fn unix_seconds_to_local_datetime(seconds: i64) -> Option<SkinDateTime> {
    let raw_time = seconds as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` are valid pointers for the duration of the call.
    // `localtime_s` initializes `tm` when it returns zero.
    let result = unsafe { libc::localtime_s(tm.as_mut_ptr(), &raw_time) };
    if result != 0 {
        return None;
    }
    // SAFETY: A zero return value means `tm` has been fully initialized.
    let tm = unsafe { tm.assume_init() };
    Some(datetime_from_tm(tm))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn unix_seconds_to_local_datetime(_seconds: i64) -> Option<SkinDateTime> {
    None
}

pub(super) fn datetime_from_tm(tm: libc::tm) -> SkinDateTime {
    SkinDateTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1).clamp(1, 12) as u32,
        day: tm.tm_mday.clamp(1, 31) as u32,
        hour: tm.tm_hour.clamp(0, 23) as u32,
        minute: tm.tm_min.clamp(0, 59) as u32,
        second: tm.tm_sec.clamp(0, 59) as u32,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SkinDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

pub(super) fn unix_seconds_to_utc_datetime(seconds: i64) -> SkinDateTime {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    SkinDateTime {
        year,
        month,
        day,
        hour: seconds_of_day / 3_600,
        minute: (seconds_of_day % 3_600) / 60,
        second: seconds_of_day % 60,
    }
}

pub(super) fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod last_play_tests {
    use super::*;

    #[test]
    fn last_play_calendar_uses_os_local_time_and_one_based_months() {
        // Leap day and the signed 32-bit timestamp boundary; no TZ environment
        // mutation, which would race the renderer's other datetime tests.
        for seconds in [1_709_208_000, 2_147_483_648] {
            let date = unix_seconds_to_local_datetime(seconds).unwrap();
            let values = last_play_datetime_numbers(Some(seconds));
            assert_eq!(
                values[1..],
                [
                    date.year,
                    date.month as i32,
                    date.day as i32,
                    date.hour as i32,
                    date.minute as i32,
                    date.second as i32
                ]
            );
            assert!((1..=12).contains(&values[2]));
            assert!((0..=23).contains(&values[4]));
        }
    }
}
