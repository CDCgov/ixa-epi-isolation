//! A simple mechanism to count events during a simulation and report on total simple
//! mechanism to count events during a simulation and report on total accumulated counts
//! and per-second rates.
//!
//! ```
//! Event                   Count  Rate (per sec)
//! ---------------------------------------------
//! property progression   12,888         2428.00
//! recovery                9,091         1712.67
//! accepted infection      8,988         1693.27
//! forecasted infection   27,171         5118.81
//!
//! Infection Forecasting Efficiency: 33.08% (8,988 accepted of 27,171 forecasted)
//! ```
//!
//! This module provides an interface for collecting statistics on how frequently
//! certain events occur during a simulation. It is designed to track both the total
//! count of events and the event rate (count per second) over time.
//!
//! Although originally intended for measuring how often plans are processed,
//! the mechanism is general-purpose and can be used to track any discrete event
//! in the simulation, for example:
//!
//! - Monitoring the frequency of specific agent behaviors
//! - Tracking usage patterns of system resources
//! - Measuring throughput of scheduling or processing queues
//!
//! The mechanism is very simple:
//!
//! - **Count accumulation**: Keep track of how many times an event has occurred by calling
//!   `context.increment_named_count()` with the name of the event (a `&'static str`).
//! - **Rate estimation**: The first time `context.increment_named_count()` with **any** name
//!   the global start time is recorded. Display the count and computed rate (e.g., plans per
//!   second) with `context.print_named_counts()`.
//!
//!
//! The names `"forecasted infection"` and `"accepted infection"` are treated specially by
//! `print_named_counts()` in that the forecast efficiency, defined as
//!
//! ```ignore
//! accepted_forecast_count / forecast_count * 100.0
//! ```
//!
//! is also computed and printed.
//!
//! # Usage
//!
//! To use this module, increment the counter in your simulation loop, plan, or event handler.
//! Here is an example with
//!
//! ```rust
//! context.add_plan(next_time, move |context| {
//!
//!     // Increment the count for the "forecasted infection" event
//!     context.increment_named_count("forecasted infection");
//!
//!     if evaluate_forecast(context, person, forecasted_total_infectiousness) {
//!         if let Some(setting_id) = context.get_setting_for_contact(person) {
//!             let str_setting = setting_id.setting_type.get_name();
//!             let id = setting_id.id;
//!             if let Some(next_contact) =
//!                 infection_attempt(context, person, setting_id)
//!             {
//!                 // Increment the count for "accepted infection"
//!                 context.increment_named_count("accepted infection");
//!
//!                 context.infect_person(next_contact, Some(person), Some(str_setting), Some(id));
//!             }
//!         }
//!     }
//!     // Continue scheduling forecasts until the person recovers.
//!     schedule_next_forecasted_infection(context, person);
//! });
//! ```
//!

use ixa::{define_data_plugin, Context, HashMap, PluginContext};
use std::time::Instant;

#[derive(Default)]
struct ProfilingDataContainer {
    pub start_time: Option<Instant>,
    pub counts: HashMap<&'static str, usize>,
}

impl ProfilingDataContainer {
    pub fn get_named_count(&self, key: &'static str) -> Option<usize> {
        self.counts.get(&key).copied()
    }
}

define_data_plugin!(
    ProfilingDataPlugin,
    ProfilingDataContainer,
    ProfilingDataContainer::default()
);

pub trait ContextProfilingExt: PluginContext {
    fn increment_named_count(&mut self, key: &'static str) {
        let container = self.get_data_container_mut(ProfilingDataPlugin);
        if container.start_time.is_none() {
            container.start_time = Some(Instant::now());
        }
        container
            .counts
            .entry(key)
            .and_modify(|v| *v += 1)
            .or_insert(1);
    }

    fn print_named_counts(&self) {
        let container = self.get_data_container(ProfilingDataPlugin).unwrap();
        let elapsed = container.start_time.unwrap().elapsed().as_secs_f64();

        let mut rows = vec![
            // The header row
            vec![
                "Event".to_string(),
                "Count".to_string(),
                "Rate (per sec)".to_string(),
            ],
        ];

        // Collect data rows
        for (key, count) in &container.counts {
            #[allow(clippy::cast_precision_loss)]
            let rate = (*count as f64) / elapsed;
            rows.push(vec![
                (*key).to_string(),
                format_with_commas(*count),
                format!("{:.2}", rate),
            ]);
        }

        println!();
        print_formatted_table(&rows);

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
    _ = context.get_data_container_mut(ProfilingDataPlugin);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn increments_named_count_correctly() {
        let mut ctx = Context::new();

        ctx.increment_named_count("test_event");
        ctx.increment_named_count("test_event");
        ctx.increment_named_count("another_event");

        let data = ctx.get_data_container(ProfilingDataPlugin).unwrap();
        assert_eq!(data.get_named_count("test_event"), Some(2));
        assert_eq!(data.get_named_count("another_event"), Some(1));
    }

    #[test]
    fn start_time_initialized_on_first_increment() {
        let mut ctx = Context::new();
        let data = ctx.get_data_container_mut(ProfilingDataPlugin);
        assert!(data.start_time.is_none());

        ctx.increment_named_count("first_event");

        let data = ctx.get_data_container(ProfilingDataPlugin).unwrap();
        assert!(data.start_time.is_some());
    }

    #[test]
    fn print_named_counts_outputs_expected_format() {
        let mut ctx = Context::new();

        // Inject a fixed start time 1 second ago
        let data = ctx.get_data_container_mut(ProfilingDataPlugin);
        data.start_time = Some(Instant::now().checked_sub(Duration::from_secs(1)).unwrap());
        data.counts.insert("event1", 5);

        ctx.print_named_counts(); // should print " event1  5  5.00 per second"
    }

    #[test]
    fn print_named_counts_computes_forecast_efficiency() {
        let mut ctx = Context::new();

        let data = ctx.get_data_container_mut(ProfilingDataPlugin);
        data.start_time = Some(Instant::now().checked_sub(Duration::from_secs(2)).unwrap());
        data.counts.insert("forecasted infection", 10);
        data.counts.insert("accepted infection", 4);

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
}
