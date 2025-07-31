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
//! Span Label                                       Duration  % runtime
//! --------------------------------------------------------------------
//! add_itinerary                            48ms 366us 300ns      0.94%
//! load_synth_population                    89ms 345us 833ns      1.73%
//! transmission_report                      26ms 758us 668ns      0.52%
//! sample_setting                           30ms 268us 858ns      0.59%
//! get_contact                          1s 440ms 483us 937ns     27.95%
//! infection_attempt                    1s 539ms 507us 163ns     29.87%
//! schedule_next_forecasted_infection   1s 723ms 370us 609ns     33.44%
//!
//! Event Label              Count  Rate (per sec)
//! ----------------------------------------------
//! property progression    94,956       18,422.71
//! recovery                67,758       13,145.94
//! accepted infection     132,133       25,635.53
//! forecasted infection   144,778       28,088.82
//!
//! Infection Forecasting Efficiency: 91.27% (132,133 accepted of 144,778 forecasted)
//! ```
//!
//! ## How to Use
//!
//! **Count an event** by calling:
//!
//! ```rust,ignore
//! context.increment_named_count("forecasted infection");
//! ```
//!
//! **Time an operation** using:
//!
//! ```rust,ignore
//! let span = Span::new("forecast loop");
//! // operation code here (algorithm, function call, etc.)
//! context.add_span(span);
//! ```
//!
//! **Print all profiling data** (counts, rates, spans, and forecast efficiency):
//!
//! ```rust,ignore
//! context.print_profiling_data();
//! ```
//!
//! This function delegates to:
//!
//! - `print_named_counts()` – Shows counts and rates.
//! - `print_named_spans()` – Shows elapsed times for spans.
//! - `print_forecast_efficiency()` – Prints efficiency if special counts are available.
//!
//! ### Special Event Names
//!
//! If you track both `"forecasted infection"` and `"accepted infection"`, an additional efficiency percentage will be reported automatically.
//!
//! ### Example
//!
//! ```rust,ignore
//! context.add_plan(next_time, move |context| {
//!     context.increment_named_count("forecasted infection");
//!
//!     if evaluate_forecast(context, person, forecasted_total_infectiousness) {
//!         if let Some(setting_id) = context.get_setting_for_contact(person) {
//!             if let Some(next_contact) = infection_attempt(context, person, setting_id) {
//!                 context.increment_named_count("accepted infection");
//!                 context.infect_person(next_contact, Some(person), None, None);
//!             }
//!         }
//!     }
//!
//!     schedule_next_forecasted_infection(context, person);
//! });
//! ```

use humantime::format_duration;
use ixa::{define_data_plugin, Context, HashMap, PluginContext};
use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
struct ProfilingDataContainer {
    pub start_time: RefCell<Option<Instant>>,
    pub counts: RefCell<HashMap<&'static str, usize>>,
    pub spans: RefCell<HashMap<&'static str, Duration>>,
}

impl ProfilingDataContainer {
    pub fn get_named_count(&self, key: &'static str) -> Option<usize> {
        self.counts.borrow().get(&key).copied()
    }

    pub fn init_start_time(&self) {
        if self.start_time.borrow().is_none() {
            *self.start_time.borrow_mut() = Some(Instant::now());
        }
    }
}

define_data_plugin!(
    ProfilingDataPlugin,
    ProfilingDataContainer,
    ProfilingDataContainer::default()
);

pub struct Span {
    pub label: &'static str,
    pub start_time: Instant,
}

impl Span {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            start_time: Instant::now(),
        }
    }
}

pub trait ContextProfilingExt: PluginContext {
    fn increment_named_count(&mut self, key: &'static str) {
        let container = self.get_data(ProfilingDataPlugin);
        container.init_start_time();
        container
            .counts
            .borrow_mut()
            .entry(key)
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }

    fn add_span(&self, span: Span) {
        self.add_span_duration(span.label, span.start_time.elapsed());
    }

    fn add_span_duration(&self, key: &'static str, elapsed: Duration) {
        let container = self.get_data(ProfilingDataPlugin);
        container.init_start_time();
        container
            .spans
            .borrow_mut()
            .entry(key)
            .and_modify(|v| *v += elapsed)
            .or_insert(elapsed);
    }

    fn print_profiling_data(&self) {
        self.print_named_spans();
        self.print_named_counts();
        self.print_forecast_efficiency();
    }

    fn print_named_counts(&self) {
        let container = self.get_data(ProfilingDataPlugin);
        if container.counts.borrow().is_empty() {
            // nothing to report
            return;
        }
        let elapsed = container
            .start_time
            .borrow()
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
        for (key, count) in container.counts.borrow().iter() {
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
        let container = self.get_data(ProfilingDataPlugin);
        if container.spans.borrow().is_empty() {
            // nothing to report
            return;
        }
        let elapsed = container
            .start_time
            .borrow()
            .unwrap()
            .elapsed()
            .as_secs_f64();

        let mut rows = vec![vec![
            "Span Label".to_string(),
            "Duration".to_string(),
            "% runtime".to_string(),
        ]];

        for (key, duration) in container.spans.borrow().iter() {
            let percent_runtime = duration.as_secs_f64() / elapsed * 100.0;
            rows.push(vec![
                (*key).to_string(),
                format!("{}", format_duration(*duration)),
                // format_with_commas_f64(percent_runtime),
                format!("{:.2}%", percent_runtime),
            ]);
        }

        println!();
        print_formatted_table(&rows);
    }

    fn print_forecast_efficiency(&self) {
        let container = self.get_data(ProfilingDataPlugin);

        // Forecasting efficiency summary
        if let (Some(accepted), Some(forecasted)) = (
            container.get_named_count("accepted infection"),
            container.get_named_count("forecasted infection"),
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

impl ContextProfilingExt for Context {}

pub fn init(context: &mut Context) {
    _ = context.get_data_mut(ProfilingDataPlugin);
}

/// Prints a table with aligned columns, using the first row as a header.
/// The first column is left-aligned; remaining columns are right-aligned.
/// Automatically adjusts column widths and inserts a separator line.
pub fn print_formatted_table(rows: &[Vec<String>]) {
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
        let mut ctx = Context::new();

        ctx.increment_named_count("test_event");
        ctx.increment_named_count("test_event");
        ctx.increment_named_count("another_event");

        let data = ctx.get_data(ProfilingDataPlugin);
        assert_eq!(data.get_named_count("test_event"), Some(2));
        assert_eq!(data.get_named_count("another_event"), Some(1));
    }

    #[test]
    fn start_time_initialized_on_first_increment() {
        let mut ctx = Context::new();
        let data = ctx.get_data_mut(ProfilingDataPlugin);
        assert!(data.start_time.borrow().is_none());

        ctx.increment_named_count("first_event");

        let data = ctx.get_data(ProfilingDataPlugin);
        assert!(data.start_time.borrow().is_some());
    }

    #[test]
    fn print_named_counts_outputs_expected_format() {
        let mut ctx = Context::new();

        // Inject a fixed start time 1 second ago
        let data = ctx.get_data_mut(ProfilingDataPlugin);
        *data.start_time.borrow_mut() =
            Some(Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
        data.counts.borrow_mut().insert("event1", 5);

        ctx.print_named_counts(); // should print " event1  5  5.00 per second"
    }

    #[test]
    fn print_named_counts_computes_forecast_efficiency() {
        let mut ctx = Context::new();

        let data = ctx.get_data_mut(ProfilingDataPlugin);
        *data.start_time.borrow_mut() =
            Some(Instant::now().checked_sub(Duration::from_secs(2)).unwrap());
        data.counts.borrow_mut().insert("forecasted infection", 10);
        data.counts.borrow_mut().insert("accepted infection", 4);

        ctx.print_named_counts(); // should print "40.00% efficiency"
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
}
