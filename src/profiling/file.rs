use ixa::execution_stats::ExecutionStatistics;
#[cfg(feature = "profiling")]
use ixa::HashMap;
#[cfg(feature = "profiling")]
use serde::Serialize;
use std::path::Path;
#[cfg(feature = "profiling")]
use std::{
    fs::File,
    io::Write,
    sync::MutexGuard,
    time::{Duration, SystemTime},
};

#[cfg(feature = "profiling")]
use super::{
    profiling_data, ComputedValue, ProfilingDataContainer, NAMED_COUNTS_HEADERS,
    NAMED_SPANS_HEADERS,
};

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct ProfilingData {
    date_time: SystemTime,
    execution_statistics: ExecutionStatistics,
    named_counts_headers: Vec<String>,
    named_counts_data: Vec<(String, usize, f64)>,
    named_spans_headers: Vec<String>,
    named_spans_data: Vec<(String, usize, Duration, f64)>,
    computed_statistics: HashMap<&'static str, ComputedValue>,
}

#[cfg(feature = "profiling")]
pub fn write_profiling_data_to_file<P: AsRef<Path>>(
    file_path: P,
    execution_statistics: ExecutionStatistics,
) -> std::io::Result<()> {
    let mut container: MutexGuard<'static, ProfilingDataContainer> = profiling_data();

    // Compute first to avoid double borrow
    let stat_count = container.computed_statistics.len();
    for idx in 0..stat_count {
        // Temporarily take the statistic, because we need immutable access to `container`.
        let mut statistic = container.computed_statistics[idx].take().unwrap();
        statistic.value = (statistic.computer)(&container);
        // Return the statistic
        container.computed_statistics[idx] = Some(statistic);
    }

    let computed_statistics = container.computed_statistics.iter().filter_map(|stat| {
        let stat = stat.as_ref().unwrap();
        if stat.value.is_none() {
            None
        } else {
            Some((stat.label, stat.value.clone()))
        }
    });
    let computed_statistics = computed_statistics.collect::<HashMap<_, _>>(); //HashMap::from_iter(computed_statistics);

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
        computed_statistics,
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
