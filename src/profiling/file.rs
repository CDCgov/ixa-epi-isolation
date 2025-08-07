use ixa::execution_stats::ExecutionStatistics;
use std::path::Path;
#[cfg(feature = "profiling")]
use std::{
    fs::File,
    io::Write,
    time::{Duration, SystemTime},
};

#[cfg(feature = "profiling")]
use serde::Serialize;

#[cfg(feature = "profiling")]
use crate::profiling::{data::profiling_data, NAMED_COUNTS_HEADERS, NAMED_SPANS_HEADERS};

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct ProfilingData {
    date_time: SystemTime,
    execution_statistics: ExecutionStatistics,
    named_counts_headers: Vec<String>,
    named_counts_data: Vec<(String, usize, f64)>,
    named_spans_headers: Vec<String>,
    named_spans_data: Vec<(String, usize, Duration, f64)>,
}

#[cfg(feature = "profiling")]
pub fn write_profiling_data_to_file<P: AsRef<Path>>(
    file_path: P,
    execution_statistics: ExecutionStatistics,
) -> std::io::Result<()> {
    let container = profiling_data();
    let profiling_data = ProfilingData {
        date_time: SystemTime::now(),
        execution_statistics,
        named_counts_headers: NAMED_COUNTS_HEADERS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        named_counts_data: container.get_named_counts_table(),
        named_spans_headers: NAMED_SPANS_HEADERS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        named_spans_data: container.get_named_spans_table(),
    };

    let json =
        serde_json::to_string_pretty(&profiling_data).expect("ProfilingData serialization failed");

    let mut file = File::create(file_path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

#[cfg(not(feature = "profiling"))]
pub fn write_profiling_data_to_file<P: AsRef<Path>>(
    _file_path: P,
    _execution_statistics: ExecutionStatistics,
) -> std::io::Result<()> {
    Ok(())
}
