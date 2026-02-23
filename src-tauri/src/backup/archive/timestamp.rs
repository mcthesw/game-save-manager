use chrono::{Datelike, LocalResult, TimeZone, Timelike, Utc};
use std::time::SystemTime;

pub(crate) const ZIP_COMMENT_LOCAL_TIME_MARKER: &str = "RGSM_TS_MODE=LOCAL_V1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ZipTimestampInterpretation {
    LegacyUtc,
    LocalTime,
}

pub(crate) fn zip_timestamp_interpretation_from_comment(
    comment: &[u8],
) -> ZipTimestampInterpretation {
    if comment == ZIP_COMMENT_LOCAL_TIME_MARKER.as_bytes() {
        ZipTimestampInterpretation::LocalTime
    } else {
        ZipTimestampInterpretation::LegacyUtc
    }
}

fn zip_datetime_to_naive_datetime(zip_time: zip::DateTime) -> chrono::NaiveDateTime {
    chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(
            zip_time.year() as i32,
            zip_time.month() as u32,
            zip_time.day() as u32,
        )
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1980, 1, 1).expect("valid date")),
        chrono::NaiveTime::from_hms_opt(
            zip_time.hour() as u32,
            zip_time.minute() as u32,
            zip_time.second() as u32,
        )
        .unwrap_or_default(),
    )
}

pub(crate) fn system_time_to_zip_datetime(system_time: SystemTime) -> zip::DateTime {
    let datetime = chrono::DateTime::<chrono::Local>::from(system_time).naive_local();

    zip::DateTime::from_date_and_time(
        datetime.year() as u16,
        datetime.month() as u8,
        datetime.day() as u8,
        datetime.hour() as u8,
        datetime.minute() as u8,
        datetime.second() as u8,
    )
    .unwrap_or_default()
}

pub(crate) fn zip_datetime_to_system_time(
    zip_time: zip::DateTime,
    interpretation: ZipTimestampInterpretation,
) -> SystemTime {
    let datetime = zip_datetime_to_naive_datetime(zip_time);
    let timestamp = match interpretation {
        ZipTimestampInterpretation::LegacyUtc => datetime.and_utc().timestamp(),
        ZipTimestampInterpretation::LocalTime => {
            local_result_to_timestamp(datetime, chrono::Local.from_local_datetime(&datetime))
        }
    };

    if timestamp < 0 {
        SystemTime::UNIX_EPOCH
    } else {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64)
    }
}

pub(crate) fn local_result_to_timestamp(
    datetime: chrono::NaiveDateTime,
    local_result: LocalResult<chrono::DateTime<chrono::Local>>,
) -> i64 {
    match local_result {
        LocalResult::Single(local_time) => local_time.with_timezone(&Utc).timestamp(),
        LocalResult::Ambiguous(early, _late) => early.with_timezone(&Utc).timestamp(),
        LocalResult::None => datetime.and_utc().timestamp(),
    }
}
