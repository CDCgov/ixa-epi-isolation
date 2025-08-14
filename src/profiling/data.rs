use super::Span;
#[cfg(feature = "profiling")]
use super::TOTAL_MEASURED;
#[cfg(feature = "profiling")]
use ixa::HashMap;
#[cfg(feature = "profiling")]
use std::{
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};

#[cfg(feature = "profiling")]
static PROFILING_DATA: OnceLock<Mutex<ProfilingDataContainer>> = OnceLock::new();

/// During testing, tests that are meant to panic can poison the mutex. Since we don't care
/// about accuracy of profiling data during tests, we just reset the poison flag.
#[cfg(all(feature = "profiling", test))]
pub(super) fn profiling_data() -> MutexGuard<'static, ProfilingDataContainer> {
    #[cfg(test)]
    PROFILING_DATA
        .get_or_init(|| Mutex::new(ProfilingDataContainer::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Acquires an exclusive lock on the profiling data, blocking until it's available.
#[cfg(all(feature = "profiling", not(test)))]
pub(super) fn profiling_data() -> MutexGuard<'static, ProfilingDataContainer> {
    #[cfg(not(test))]
    PROFILING_DATA
        .get_or_init(|| Mutex::new(ProfilingDataContainer::default()))
        .lock()
        .unwrap()
}

#[cfg(feature = "profiling")]
#[derive(Default)]
pub(super) struct ProfilingDataContainer {
    pub start_time: Option<Instant>,
    pub counts: HashMap<&'static str, usize>,
    // We store span counts with the span duration, because they are updated when
    // the spans are and displayed with the spans rather than with the other counts.
    pub spans: HashMap<&'static str, (Duration, usize)>,
    // The number of spans that are currently open. We use this and the `total_measured` span to
    // compute the amount of time accounted for by all the spans. This together with total
    // runtime can tell you if there is significant runtime not accounted for by the existing
    // spans. When `open_span_count` transitions from `0`, the `total_measured` span is opened.
    // When `open_span_count` transitions back to `0`, `total_measured` is closed and duration
    // is recorded.
    pub open_span_count: usize,
    pub coverage: Option<Instant>,
}

#[cfg(feature = "profiling")]
impl ProfilingDataContainer {
    pub fn increment_named_count(&mut self, key: &'static str) {
        self.init_start_time();
        self.counts.entry(key).and_modify(|v| *v += 1).or_insert(1);
    }

    pub fn get_named_count(&self, key: &'static str) -> Option<usize> {
        self.counts.get(&key).copied()
    }

    fn init_start_time(&mut self) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
    }

    fn open_span(&mut self, label: &'static str) -> Span {
        self.init_start_time();
        if self.open_span_count == 0 {
            // Start recording coverage time.
            self.coverage = Some(Instant::now());
        }
        self.open_span_count += 1;
        Span::new(label)
    }

    /// Do not call directly. This method is called from `Span::drop`.
    pub(super) fn close_span(&mut self, span: &Span) {
        self.open_span_count -= 1;
        if self.open_span_count == 0 {
            // stop recording coverage time. The `total_measured` must be `Some(..)` if
            // `open_span_count` was nonzero, so unwrap always succeeds.
            let coverage = self.coverage.take().unwrap();
            self.close_span_without_coverage(TOTAL_MEASURED, coverage.elapsed());
        }
        self.close_span_without_coverage(span.label, span.start_time.elapsed());
    }

    /// Closes the span without checking the coverage span.
    fn close_span_without_coverage(&mut self, label: &'static str, elapsed: Duration) {
        self.spans
            .entry(label)
            .and_modify(|(time, count)| {
                *time += elapsed;
                *count += 1;
            })
            .or_insert((elapsed, 1));
    }

    /// Constructs a table of ("Event Label", "Count", "Rate (per sec)"). Used to print
    /// stats to the console and write the stats to a file.
    pub(super) fn get_named_counts_table(&self) -> Vec<(String, usize, f64)> {
        let elapsed = self.start_time.unwrap().elapsed().as_secs_f64();

        let mut rows = vec![];

        // Collect data rows
        for (key, count) in &self.counts {
            #[allow(clippy::cast_precision_loss)]
            let rate = (*count as f64) / elapsed;

            rows.push(((*key).to_string(), *count, rate));
        }

        rows
    }

    /// Constructs a table of "Span Label", "Count", "Duration", "% runtime". Used to print
    /// stats to the console and write the stats to a file.
    pub(super) fn get_named_spans_table(&self) -> Vec<(String, usize, Duration, f64)> {
        let elapsed = self.start_time.unwrap().elapsed().as_secs_f64();

        let mut rows = vec![];

        // Add all regular span rows
        for (&label, &(duration, count)) in self.spans.iter().filter(|(k, _)| *k != &TOTAL_MEASURED)
        {
            rows.push((
                label.to_string(),
                count,
                duration,
                duration.as_secs_f64() / elapsed * 100.0,
            ));
        }

        // Add the "Total measured" row at the end
        if let Some(&(duration, count)) = self.spans.get(&TOTAL_MEASURED) {
            rows.push((
                TOTAL_MEASURED.to_string(),
                count,
                duration,
                duration.as_secs_f64() / elapsed * 100.0,
            ));
        }

        rows
    }
}

#[cfg(feature = "profiling")]
pub fn increment_named_count(key: &'static str) {
    let mut container = profiling_data();
    container.increment_named_count(key);
}

#[cfg(not(feature = "profiling"))]
pub fn increment_named_count(_key: &'static str) {}

#[cfg(feature = "profiling")]
pub fn open_span(label: &'static str) -> Span {
    let mut container = profiling_data();
    container.open_span(label)
}

#[cfg(not(feature = "profiling"))]
pub fn open_span(label: &'static str) -> Span {
    Span::new(label)
}

/// Call this if you want to explicitly close a span before the end of the scope in which the
/// span was defined. Equivalent to `span.drop()`.
pub fn close_span(_span: Span) {
    // The `span` is dropped here, and `"ProfilingDataContainer::close_span` is called
    // from `Span::drop`. Incidentally, this is the same implementation as `span.drop()`!
}
