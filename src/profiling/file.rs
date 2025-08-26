use super::computed_statistic::{ComputedStatistic, ComputedValue};
#[cfg(feature = "profiling")]
use super::profiling_data;
use ixa::execution_stats::ExecutionStatistics;
use ixa::HashMap;
#[cfg(feature = "profiling")]
use serde::{Serialize, Serializer};
use std::path::Path;
#[cfg(feature = "profiling")]
use std::{
    fs::File,
    io::Write,
    time::{Duration, SystemTime},
};

/// A wrapper around Duration the serialization format of which we have control over.
#[cfg(feature = "profiling")]
#[derive(Debug, Copy, Clone)]
struct SerializableDuration(pub Duration);

#[cfg(feature = "profiling")]
impl Serialize for SerializableDuration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0.as_secs_f64())
    }
}

/// A version of `ExecutionStatistics` the serialization format of which we have control over.
#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct SerializableExecutionStatistics {
    max_memory_usage: u64,
    cpu_time: SerializableDuration,
    wall_time: SerializableDuration,

    // Per person stats
    population: usize,
    cpu_time_per_person: SerializableDuration,
    wall_time_per_person: SerializableDuration,
    memory_per_person: u64,
}

#[cfg(feature = "profiling")]
impl From<ExecutionStatistics> for SerializableExecutionStatistics {
    fn from(value: ExecutionStatistics) -> Self {
        SerializableExecutionStatistics {
            max_memory_usage: value.max_memory_usage,
            cpu_time: SerializableDuration(value.cpu_time),
            wall_time: SerializableDuration(value.wall_time),
            population: value.population,
            cpu_time_per_person: SerializableDuration(value.cpu_time_per_person),
            wall_time_per_person: SerializableDuration(value.wall_time_per_person),
            memory_per_person: value.memory_per_person,
        }
    }
}

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct SpanRecord {
    label: String,
    count: usize,
    duration: SerializableDuration,
    percent_runtime: f64,
}

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct CountRecord {
    label: String,
    count: usize,
    rate_per_second: f64,
}

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct ProfilingData {
    date_time: SystemTime,
    execution_statistics: SerializableExecutionStatistics,
    named_counts: Vec<CountRecord>,
    named_spans: Vec<SpanRecord>,
    computed_statistics: HashMap<&'static str, ComputedStatisticRecord>,
}

#[cfg(feature = "profiling")]
#[derive(Serialize)]
struct ComputedStatisticRecord {
    description: &'static str,
    value: ComputedValue,
}

#[cfg(feature = "profiling")]
pub fn write_profiling_data_to_file<P: AsRef<Path>>(
    file_path: P,
    execution_statistics: ExecutionStatistics,
) -> std::io::Result<()> {
    let mut container = profiling_data();
    let named_spans_data = container.get_named_spans_table();
    let named_spans_data = named_spans_data
        .into_iter()
        .map(|(label, count, duration, percent_runtime)| SpanRecord {
            label,
            count,
            duration: SerializableDuration(duration),
            percent_runtime,
        })
        .collect();
    let named_counts_data = container.get_named_counts_table();
    let named_counts_data = named_counts_data
        .into_iter()
        .map(|(label, count, rate_per_second)| CountRecord {
            label,
            count,
            rate_per_second,
        })
        .collect();

    // Compute first to avoid double borrow
    let stat_count = container.computed_statistics.len();
    for idx in 0..stat_count {
        // Temporarily take the statistic, because we need immutable access to `container`.
        let mut statistic = container.computed_statistics[idx].take().unwrap();
        statistic.value = statistic.functions.compute(&container);
        // Return the statistic
        container.computed_statistics[idx] = Some(statistic);
    }

    let computed_statistics = container.computed_statistics.iter().filter_map(|stat| {
        let stat = stat.as_ref().unwrap();
        if stat.value.is_none() {
            None
        } else {
            Some((
                stat.label,
                ComputedStatisticRecord {
                    description: stat.description,
                    value: stat.value.unwrap(),
                },
            ))
        }
    });
    let computed_statistics = computed_statistics.collect::<HashMap<_, _>>();

    let profiling_data = ProfilingData {
        date_time: SystemTime::now(),
        execution_statistics: execution_statistics.into(),
        named_counts: named_counts_data,
        named_spans: named_spans_data,
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
