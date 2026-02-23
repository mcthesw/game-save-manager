mod compress;
mod decompress;
mod timestamp;

pub use compress::compress_to_file;
pub use decompress::decompress_from_file;
pub(crate) use timestamp::{
    ZipTimestampInterpretation, system_time_to_zip_datetime,
    zip_timestamp_interpretation_from_comment,
};

#[cfg(test)]
pub(crate) use compress::add_directory;
#[cfg(test)]
pub(crate) use timestamp::{
    ZIP_COMMENT_LOCAL_TIME_MARKER, local_result_to_timestamp, zip_datetime_to_system_time,
};
