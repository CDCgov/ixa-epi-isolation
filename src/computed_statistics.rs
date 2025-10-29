//! This module contains the definitions of the "custom" computed statistics that are registered
//! with the profiling mechanism for printing to the console and / or writing to a JSON file at
//! the end of the simulation.
//!
//! Each custom statistic needs two functions:
//! - A _boxed_ "computer" function that takes a reference to the `ProfilingData` and computes
//!   a value.
//! - A _boxed_ "printer" function that takes the computed value and prints it to the console.
//!
//! The "computer" gets an immutable reference to all counts and spans and to the start time, data
//! members of the `ProfilingData` struct:
//!
//! ```rust, ignore
//! pub struct ProfilingData {
//!     pub start_time: Option<Instant>,
//!
//!     // A map from the count label to the value of the count
//!     pub counts: HashMap<&'static str, usize>,
//!
//!     // A map from the span label to its duration and count as a tuple
//!     pub spans: HashMap<&'static str, (Duration, usize)>,
//! }
//! ```
//!
//! The computer function should not have side effects, and the printer function should only have
//! side effects if it prints to the console.
//!
//! To register a custom statistic, use the `add_computed_statistic` function from the
//! `profiling` module. This function takes the label, description, computer function, and printer
//! function as arguments.
//!
//! ```rust, ignore
//! use crate::profiling::add_computed_statistic;
//!
//! add_computed_statistic(
//!     "infection forecasting efficiency",
//!     "The percentage of forecasted infections that were accepted.",
//!     forecasting_efficiency_computer,
//!     forecasting_efficiency_printer,
//! );
//! ```
//!

use crate::profiling::{
    add_computed_statistic, CustomStatisticComputer, CustomStatisticPrinter, ProfilingData,
};

/// The name of the distinguished accepted infection label. You don't need to make a constant
/// for this, but it can keep you from introducing a bug because of a typo in the label.
/// You would use the constant everywhere in your code instead of the string literal.
pub const ACCEPTED_INFECTION_LABEL: &str = "accepted infection attempt";
/// The name of the distinguished forecasted infection label.
pub const FORECASTED_INFECTION_LABEL: &str = "forecasted infection";

/// The function that knows how to compute the forecasting efficiency from the
/// data collected in `ProfilingData`.
///
/// - This could just as easily be a closure, but making it a function can make
///   things a lot more readable.
/// - This function returns an option to accommodate statistics that are only
///   conditionally computable.
fn forecasting_efficiency_computer(statistics: &ProfilingData) -> Option<f64> {
    if let (Some(&accepted), Some(&forecasted)) = (
        statistics.counts.get(&ACCEPTED_INFECTION_LABEL),
        statistics.counts.get(&FORECASTED_INFECTION_LABEL),
    ) {
        #[allow(clippy::cast_precision_loss)]
        let efficiency = (accepted as f64) / (forecasted as f64) * 100.0;
        Some(efficiency)
    } else {
        None
    }
}

/// The function that knows how to display the forecasting efficiency on the console.
/// This could just as easily be a closure if you prefer. The type inside the option
/// returned by the computer and argument of the printer function must coincide.
fn forecasting_efficiency_printer(efficiency: f64) {
    println!("Infection Forecasting Efficiency: {:.2}%", efficiency);
}

/// Initializes the custom computed statistics. This function is called from the `main` function
/// in `src/main.rs`.
pub fn init() {
    // Don't forget to box the functions. The compiler will infer the type--you don't need
    // to specify it.
    let computer: CustomStatisticComputer<f64> = Box::new(forecasting_efficiency_computer);
    let printer: CustomStatisticPrinter<f64> = Box::new(forecasting_efficiency_printer);

    // The label and description are used in the JSON report.
    let label = "infection forecasting efficiency";
    let description = "The percentage of forecasted infections that were accepted.";

    // Use the free function in the `profiling` module to register the statistic.
    add_computed_statistic(label, description, computer, printer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiling::{get_profiling_data, print_named_counts};
    use std::time::{Duration, Instant};

    #[test]
    fn print_named_counts_computes_forecast_efficiency() {
        {
            let mut data = get_profiling_data();
            data.start_time = Some(Instant::now().checked_sub(Duration::from_secs(2)).unwrap());
            data.counts.insert(FORECASTED_INFECTION_LABEL, 10);
            data.counts.insert(ACCEPTED_INFECTION_LABEL, 4);
        }
        print_named_counts(); // should print "40.00% efficiency"
    }
}
