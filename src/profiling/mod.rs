//! This module provides a lightweight profiling interface for simulations, tracking
//! event counts and measuring elapsed time for named operations (“spans”). It supports:
//!
//! - **Event counting** – Track how often named events occur during a run.
//! - **Rate calculation** – Compute rates (events per second) since the first count.
//! - **Span timing** – Measure time intervals between span creation and reporting.
//! - **Efficiency reporting** – Report forecasting efficiency when "forecasted infection" and
//!   "accepted infection" counts are available.
//!
//! The functionality of this module is gated behind the `profiling` feature, which is enabled by
//! default. Disabling this feature leaves the public API defined but with an empty implementation,
//! which is optimized away by the compiler, allowing the programmer to leave profiling code
//! throughout their model with zero cost when profiling is disabled.
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

mod data;
mod display;
mod file;

use crate::parameters::{ContextParametersExt, Params};
pub use data::*;
pub use display::*;
use file::write_profiling_data_to_file;
use ixa::{error, Context, ContextReportExt};
use std::path::Path;
#[cfg(feature = "profiling")]
use std::time::Instant;

// "Magic" constants used in this module
/// The distinguished total measured time label.
#[cfg(feature = "profiling")]
const TOTAL_MEASURED: &str = "Total Measured";
/// The name of the distinguished accepted infection label
#[cfg(feature = "profiling")]
const ACCEPTED_INFECTION_LABEL: &str = "accepted infection";
/// The name of the distinguished forecasted infection label
#[cfg(feature = "profiling")]
const FORECASTED_INFECTION_LABEL: &str = "forecasted infection";
#[cfg(feature = "profiling")]
const NAMED_SPANS_HEADERS: &[&str] = &["Span Label", "Count", "Duration", "% runtime"];
#[cfg(feature = "profiling")]
const NAMED_COUNTS_HEADERS: &[&str] = &["Event Label", "Count", "Rate (per sec)"];

pub struct Span {
    #[cfg(feature = "profiling")]
    label: &'static str,
    #[cfg(feature = "profiling")]
    start_time: Instant,
}

impl Span {
    fn new(#[allow(unused_variables)] label: &'static str) -> Self {
        Self {
            #[cfg(feature = "profiling")]
            label,
            #[cfg(feature = "profiling")]
            start_time: Instant::now(),
        }
    }
}

#[cfg(feature = "profiling")]
impl Drop for Span {
    fn drop(&mut self) {
        let mut container = profiling_data();
        container.close_span(self);
    }
}

/// Writes the execution statistics for the context and all profiling data
/// to a JSON file.
pub trait ProfilingContextExt: ContextParametersExt + ContextReportExt {
    fn write_profiling_data(&mut self) {
        let (mut prefix, directory, overwrite) = {
            let report_options = self.report_options();
            (
                report_options.file_prefix.clone(),
                report_options.output_dir.clone(),
                report_options.overwrite,
            )
        };

        let execution_statistics = self.get_execution_statistics();
        let Params {
            profiling_data_path,
            ..
        } = self.get_params();

        if profiling_data_path.is_none() {
            error!("no profiling data path specified");
            return;
        }

        prefix.push_str(profiling_data_path.as_ref().unwrap());
        let profiling_data_path = directory.join(prefix);
        let profiling_data_path = Path::new(&profiling_data_path);

        if !overwrite && profiling_data_path.exists() {
            error!(
                "profiling output file already exists: {}",
                profiling_data_path.display()
            );
            return;
        }

        write_profiling_data_to_file(profiling_data_path, execution_statistics)
            .expect("could not write profiling data to file");
    }
}
impl ProfilingContextExt for Context {}
