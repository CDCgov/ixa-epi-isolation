//! This module provides a lightweight profiling interface for simulations, tracking
//! event counts and measuring elapsed time for named operations (“spans”). It supports:
//!
//! - **Event counting** – Track how often named events occur during a run.
//! - **Rate calculation** – Compute rates (events per second) since the first count.
//! - **Span timing** – Measure time intervals between span creation and reporting.
//! - **Efficiency reporting** – Report forecasting efficiency when "forecasted infection" and
//!   "accepted infection" counts are available.
//!
//! ## Example Output
//!
//! ```ignore
//! Span Label                           Count          Duration  % runtime
//! ----------------------------------------------------------------------
//! load_synth_population                    1       950us 792ns      0.36%
//! infection_attempt                     1035     6ms 33us 91ns      2.28%
//! sample_setting                        1035     3ms 66us 52ns      1.16%
//! get_contact                           1035   1ms 135us 202ns      0.43%
//! schedule_next_forecasted_infection    1286  22ms 329us 102ns      8.44%
//! Total Measured                        1385  23ms 897us 146ns      9.03%
//!
//! Event Label            Count  Rate (per sec)
//! --------------------------------------------
//! property progression      36          136.05
//! recovery                  27          102.04
//! accepted infection     1,035        3,911.50
//! forecasted infection   1,286        4,860.09
//!
//! Infection Forecasting Efficiency: 80.48% (1,035 accepted of 1,286 forecasted)
//! ```
//!
//! ## How to Use
//!
//! **Count an event** by calling:
//!
//! ```rust,ignore
//! increment_named_count("forecasted infection");
//! ```
//!
//! **Time an operation** using:
//!
//! ```rust,ignore
//! let span = open_span("forecast loop");
//! // operation code here (algorithm, function call, etc.)
//! close_span(span);
//! ```
//!
//! The `close_span()` function consumes the span, so it can't be reused. If you do not explicitly
//! call `close_span()`, a span instance will automatically be closed when it goes out of scope
//! and is dropped. Consequently, it is impossible to forget to close a span. This is especially
//! convenient in situations where the function scope has multiple exit points:
//!
//! ```rust, ignore
//! fn complicated_function() {
//!     let _span = open_span("complicated function");
//!     // Complicated control flow here, maybe with lots of return statements.
//! } // `_span` goes out of scope, automatically closed.
//! ```
//!
//! Notice the `_` prefix in the variable name `_span` will silence warnings about an "unused"
//! variable.
//!
//! **Print all profiling data** (counts, rates, spans, and forecast efficiency):
//!
//! ```rust,ignore
//! /// Place this in your main function after the simulation completes.
//! print_profiling_data();
//! ```
//!
//! This function delegates to:
//!
//! - `print_named_counts()` – Shows counts and rates.
//! - `print_named_spans()` – Shows elapsed times for spans.
//! - `print_forecast_efficiency()` – Prints efficiency if special counts are available.
//!
//! ## Special Span and Count Names
//!
//! Keep in mind that spans can be overlapping or nested, and so the total sum of all time
//! within all spans will not necessarily be the total running time in general. The
//! `"Total Measured"` span is a special span that is open if and only if any other span is
//! open. It tells you how much of the total running time is covered by some span.
//!
//! If you track both `"forecasted infection"` and `"accepted infection"`, an additional
//! efficiency percentage will be reported automatically, as in this example:
//!
//! ```rust,ignore
//! context.add_plan(next_time, move |context| {
//!     increment_named_count("forecasted infection");
//!
//!     if evaluate_forecast(context, person, forecasted_total_infectiousness) {
//!         if let Some(setting_id) = context.get_setting_for_contact(person) {
//!             if let Some(next_contact) = infection_attempt(context, person, setting_id) {
//!                 increment_named_count("accepted infection");
//!                 context.infect_person(next_contact, Some(person), None, None);
//!             }
//!         }
//!     }
//!
//!     schedule_next_forecasted_infection(context, person);
//! });
//! ```
#![allow(dead_code)]
use humantime::format_duration;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use ixa::HashMap;

const TOTAL_MEASURED: &str = "Total Measured";
static PROFILING_DATA: OnceLock<Mutex<ProfilingDataContainer>> = OnceLock::new();

fn profiling_data() -> MutexGuard<'static, ProfilingDataContainer> {
    PROFILING_DATA.get_or_init(
        || Mutex::new(ProfilingDataContainer::default())
    ).try_lock().unwrap_or_else(|_| panic!("Cannot acquire lock on ProfilingDataContainer."))
}

pub struct Span {
    label: &'static str,
    start_time: Instant,
}

impl Span {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            start_time: Instant::now(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let mut container = profiling_data();
        container.close_span(self);
    }
}


#[derive(Default)]
struct ProfilingDataContainer {
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
    pub coverage: Option<Instant>
}

impl ProfilingDataContainer {
    pub fn increment_named_count(&mut self, key: &'static str) {
        self.init_start_time();
        self
            .counts
            .entry(key)
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }

    pub fn get_named_count(&self, key: &'static str) -> Option<usize> {
        self.counts.get(&key).copied()
    }

    fn init_start_time(&mut self) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
    }

    pub fn open_span(&mut self, label: &'static str) -> Span {
        self.init_start_time();
        if self.open_span_count == 0 {
            // Start recording coverage time.
            self.coverage = Some(Instant::now());
        }
        self.open_span_count += 1;
        Span::new(label)
    }

    /// Do not call directly. This method is called from `Span::drop`.
    pub fn close_span(&mut self, span: &Span) {
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
        self
            .spans
            .entry(label)
            .and_modify(|(time, count)| { *time += elapsed; *count += 1; })
            .or_insert((elapsed, 1));
    }

    fn print_named_counts(&self) {
        if self.counts.is_empty() {
            // nothing to report
            return;
        }
        let elapsed = self
            .start_time
            .unwrap()
            .elapsed()
            .as_secs_f64();

        let mut rows = vec![
            // The header row
            vec![
                "Event Label".to_string(),
                "Count".to_string(),
                "Rate (per sec)".to_string(),
            ],
        ];

        // Collect data rows
        for (key, count) in self.counts.iter() {
            #[allow(clippy::cast_precision_loss)]
            let rate = (*count as f64) / elapsed;
            rows.push(vec![
                (*key).to_string(),
                format_with_commas(*count),
                format_with_commas_f64(rate),
            ]);
        }

        println!();
        print_formatted_table(&rows);
    }

    fn print_named_spans(&self) {
        if  self.open_span_count != 0 {
            println!("OPEN SPAN COUNT NONZERO: {}", self.open_span_count);
        }
        if self.coverage.is_some() {
            println!("total_measured span is NOT None");
        }

        if self.spans.is_empty() {
            // nothing to report
            return;
        }
        let elapsed = self
            .start_time
            .unwrap()
            .elapsed()
            .as_secs_f64();

        let mut rows = vec![vec![
            "Span Label".to_string(),
            "Count".to_string(),
            "Duration".to_string(),
            "% runtime".to_string(),
        ]];

        for (key, (duration, count)) in self.spans.iter().filter(|(key, _)| {*key != &TOTAL_MEASURED}) {
            let percent_runtime = duration.as_secs_f64() / elapsed * 100.0;
            rows.push(vec![
                (*key).to_string(),
                count.to_string(),
                format!("{}", format_duration(*duration)),
                // format_with_commas_f64(percent_runtime),
                format!("{:.2}%", percent_runtime),
            ]);
        }
        // Make TOTAL_MEASURED the last row
        let (duration, count) = self.spans.get(&TOTAL_MEASURED).unwrap();
        let percent_runtime = duration.as_secs_f64() / elapsed * 100.0;
        rows.push(vec![
            (*TOTAL_MEASURED).to_string(),
            count.to_string(),
            format!("{}", format_duration(*duration)),
            // format_with_commas_f64(percent_runtime),
            format!("{:.2}%", percent_runtime),
        ]);

        println!();
        print_formatted_table(&rows);
    }

    fn print_forecast_efficiency(&self) {
        // Forecasting efficiency summary
        if let (Some(accepted), Some(forecasted)) = (
            self.get_named_count("accepted infection"),
            self.get_named_count("forecasted infection"),
        ) {
            #[allow(clippy::cast_precision_loss)]
            let efficiency = (accepted as f64) / (forecasted as f64) * 100.0;
            println!();
            println!(
                "Infection Forecasting Efficiency: {:.2}% ({} accepted of {} forecasted)\n",
                efficiency,
                format_with_commas(accepted),
                format_with_commas(forecasted)
            );
        }
    }
}

pub fn increment_named_count(key: &'static str) {
    let mut container = profiling_data();
    container.increment_named_count(key);
}

pub fn open_span(label: &'static str) -> Span {
    let mut container = profiling_data();
    container.open_span(label)
}

/// Call this if you want to explicitly close a span before the end of the scope in which the
/// span was defined. Equivalent to `span.drop()`.
pub fn close_span(_span: Span) {
    // `span` is dropped here, and `"ProfilingDataContainer::close_span` is called
    // from `Span::drop`.
}

/// Prints all collected profiling data.
pub fn print_profiling_data() {
    let container = profiling_data();
    container.print_named_spans();
    container.print_named_counts();
    container.print_forecast_efficiency();
}

/// Prints a table of the named counts, if any.
fn print_named_counts() {
    let container = profiling_data();
    container.print_named_counts();
}

/// Prints a table of the spans, if any.
fn print_named_spans() {
    let container = profiling_data();
    container.print_named_spans();
}

/// Prints the forecast efficiency.
fn print_forecast_efficiency() {
    let container = profiling_data();
    container.print_forecast_efficiency();
}

/// Prints a table with aligned columns, using the first row as a header.
/// The first column is left-aligned; remaining columns are right-aligned.
/// Automatically adjusts column widths and inserts a separator line.
fn print_formatted_table(rows: &[Vec<String>]) {
    if rows.len() < 2 {
        return;
    }

    let num_cols = rows[0].len();
    let mut col_widths = vec![0; num_cols];

    // Compute max column widths
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            col_widths[i] = col_widths[i].max(cell.len());
        }
    }

    // Print header row
    let header = &rows[0];
    for (i, cell) in header.iter().enumerate() {
        if i == 0 {
            print!("{:<width$} ", cell, width = col_widths[i] + 1);
        } else {
            print!("{:>width$} ", cell, width = col_widths[i] + 1);
        }
    }
    println!();

    // Print separator
    let total_width: usize = col_widths.iter().map(|w| *w + 1).sum::<usize>() + 2;
    println!("{}", "-".repeat(total_width));

    // Print data rows
    for row in &rows[1..] {
        // First column left-aligned, rest right-aligned
        for (i, cell) in row.iter().enumerate() {
            if i == 0 {
                print!("{:<width$} ", cell, width = col_widths[i] + 1);
            } else {
                print!("{:>width$} ", cell, width = col_widths[i] + 1);
            }
        }
        println!();
    }
}

/// Formats an integer with thousands separator.
fn format_with_commas(value: usize) -> String {
    let s = value.to_string();
    let mut result = String::new();
    let bytes = s.as_bytes();
    let len = bytes.len();

    for (i, &b) in bytes.iter().enumerate() {
        result.push(b as char);
        let digits_left = len - i - 1;
        if digits_left > 0 && digits_left % 3 == 0 {
            result.push(',');
        }
    }

    result
}

/// Formats a float with thousands separator.
fn format_with_commas_f64(value: f64) -> String {
    // Format to two decimal places
    let formatted = format!("{:.2}", value.abs()); // format positive part only
    let mut parts = formatted.splitn(2, '.');

    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next(); // optional

    // Format integer part with commas
    let mut result = String::new();
    let bytes = int_part.as_bytes();
    let len = bytes.len();

    for (i, &b) in bytes.iter().enumerate() {
        result.push(b as char);
        let digits_left = len - i - 1;
        if digits_left > 0 && digits_left % 3 == 0 {
            result.push(',');
        }
    }

    // Add decimal part
    if let Some(frac) = frac_part {
        result.push('.');
        result.push_str(frac);
    }

    // Reapply negative sign if needed
    if value.is_sign_negative() {
        result.insert(0, '-');
    }

    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unreadable_literal)]
    use super::*;
    use std::time::Duration;

    #[test]
    fn increments_named_count_correctly() {

        increment_named_count("test_event");
        increment_named_count("test_event");
        increment_named_count("another_event");

        let data = profiling_data();
        assert_eq!(data.get_named_count("test_event"), Some(2));
        assert_eq!(data.get_named_count("another_event"), Some(1));
    }

    #[test]
    fn start_time_initialized_on_first_increment() {
        let mut data = profiling_data();
        assert!(data.start_time.is_none());

        data.increment_named_count("first_event");

        assert!(data.start_time.is_some());
    }

    #[test]
    fn print_named_counts_outputs_expected_format() {
        // Inject a fixed start time 1 second ago
        let mut data = profiling_data();
        data.start_time =
            Some(Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
        data.counts.insert("event1", 5);

        data.print_named_counts(); // should print " event1  5  5.00 per second"
    }

    #[test]
    fn print_named_counts_computes_forecast_efficiency() {
        let mut data = profiling_data();
        data.start_time =
            Some(Instant::now().checked_sub(Duration::from_secs(2)).unwrap());
        data.counts.insert("forecasted infection", 10);
        data.counts.insert("accepted infection", 4);

        data.print_named_counts(); // should print "40.00% efficiency"
    }

    // region Tests for `format_with_commas()`
    #[test]
    fn formats_single_digit() {
        assert_eq!(format_with_commas(7), "7");
    }

    #[test]
    fn formats_two_digits() {
        assert_eq!(format_with_commas(42), "42");
    }

    #[test]
    fn formats_three_digits() {
        assert_eq!(format_with_commas(999), "999");
    }

    #[test]
    fn formats_four_digits() {
        assert_eq!(format_with_commas(1000), "1,000");
    }

    #[test]
    fn formats_five_digits() {
        assert_eq!(format_with_commas(27_171), "27,171");
    }

    #[test]
    fn formats_six_digits() {
        assert_eq!(format_with_commas(123_456), "123,456");
    }

    #[test]
    fn formats_seven_digits() {
        assert_eq!(format_with_commas(1_000_000), "1,000,000");
    }

    #[test]
    fn formats_zero() {
        assert_eq!(format_with_commas(0), "0");
    }

    #[test]
    fn formats_large_number() {
        assert_eq!(format_with_commas(9_876_543_210), "9,876,543,210");
    }

    // endregion Tests for `format_with_commas()`

    // region Tests for `format_with_commas_f64()`
    #[test]
    fn formats_small_integer() {
        assert_eq!(format_with_commas_f64(7.0), "7.00");
        assert_eq!(format_with_commas_f64(42.0), "42.00");
    }

    #[test]
    fn formats_small_decimal() {
        #![allow(clippy::approx_constant)]
        assert_eq!(format_with_commas_f64(3.14), "3.14");
        assert_eq!(format_with_commas_f64(0.99), "0.99");
    }

    #[test]
    fn formats_zero_f64() {
        assert_eq!(format_with_commas_f64(0.0), "0.00");
    }

    #[test]
    fn formats_exact_thousand() {
        assert_eq!(format_with_commas_f64(1000.0), "1,000.00");
    }

    #[test]
    fn formats_large_number_f64() {
        assert_eq!(format_with_commas_f64(1234567.89), "1,234,567.89");
        assert_eq!(format_with_commas_f64(123456789.0), "123,456,789.00");
    }

    #[test]
    fn formats_number_with_rounding_up() {
        assert_eq!(format_with_commas_f64(999.999), "1,000.00");
        assert_eq!(format_with_commas_f64(999999.999), "1,000,000.00");
    }

    #[test]
    fn formats_number_with_rounding_down() {
        assert_eq!(format_with_commas_f64(1234.444), "1,234.44");
    }

    #[test]
    fn formats_negative_number() {
        assert_eq!(format_with_commas_f64(-1234567.89), "-1,234,567.89");
    }

    #[test]
    fn formats_negative_rounding_edge() {
        assert_eq!(format_with_commas_f64(-999.995), "-1,000.00");
    }

    // endregion Tests for `format_with_commas_f64()`

    #[test]
    fn print_named_spans_outputs_expected_format() {
        let mut container = profiling_data();

        // Set a fixed start time 10 seconds ago
        container.start_time =
            Some(Instant::now().checked_sub(Duration::from_secs(10)).unwrap());

        // Add sample spans data
        container.spans.insert("database_query", (Duration::from_millis(1500), 42));
        container.spans.insert("api_request", (Duration::from_millis(800), 120));
        container.spans.insert("data_processing", (Duration::from_secs(5), 15));
        container.spans.insert("file_io", (Duration::from_millis(350), 78));
        container.spans.insert("rendering", (Duration::from_secs(2), 30));
        container.print_named_spans();
    }
}
