use std::path::PathBuf;

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError};
use serde::{Deserialize, Serialize};

use crate::parameters::{ContextParametersExt, RateFnType};

use super::{rate_fn::InfectiousnessRateFn, ConstantRate, EmpiricalRate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RateFnId(usize);

struct RateFnContainer {
    rates: Vec<Box<dyn InfectiousnessRateFn>>,
}

define_data_plugin!(
    RateFnPlugin,
    RateFnContainer,
    RateFnContainer { rates: Vec::new() }
);

define_rng!(InfectiousnessRateRng);

pub trait InfectiousnessRateExt {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>) -> RateFnId;
    fn get_random_rate_fn(&self) -> RateFnId;
    fn get_rate_fn(&self, index: RateFnId) -> &dyn InfectiousnessRateFn;
}

impl InfectiousnessRateExt for Context {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRateFn>) -> RateFnId {
        let container = self.get_data_container_mut(RateFnPlugin);
        container.rates.push(dist);
        RateFnId(container.rates.len() - 1)
    }

    fn get_random_rate_fn(&self) -> RateFnId {
        let max = self
            .get_data_container(RateFnPlugin)
            .expect("Expected rate functions to be loaded.")
            .rates
            .len();
        RateFnId(self.sample_range(InfectiousnessRateRng, 0..max))
    }

    fn get_rate_fn(&self, index: RateFnId) -> &dyn InfectiousnessRateFn {
        self.get_data_container(RateFnPlugin)
            .expect("Expected rate functions to be loaded.")
            .rates[index.0]
            .as_ref()
    }
}

/// Turn the information specified in the global parameter `infectiousness_rate_fn` into actual
/// infectiousness rate functions for the simulation.
/// # Errors
/// - If the parameters used to specify the rate functions are invalid
/// - If the file specified in the parameters cannot be read and turned into `EmpiricalRate` objects
pub fn load_rate_fns(context: &mut Context) -> Result<(), IxaError> {
    let rate_of_infection = context.get_params().infectiousness_rate_fn.clone();

    match rate_of_infection {
        RateFnType::Constant { rate, duration } => {
            context.add_rate_fn(Box::new(ConstantRate::new(rate, duration)?));
        }
        RateFnType::EmpiricalFromFile { file } => {
            add_rate_fns_from_file(context, file)?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct EmpiricalRateFnRecord {
    id: u32,
    time: f64,
    value: f64,
}

fn add_rate_fns_from_file(context: &mut Context, file: PathBuf) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(file)?;
    let mut reader = reader.deserialize::<EmpiricalRateFnRecord>();
    // Pop out the first record so we can initialize the vectors
    let record = reader.next().unwrap()?;
    let mut last_id = record.id;
    let mut times = vec![record.time];
    let mut values = vec![record.value];
    for record in reader {
        let record = record?;
        // For now, assume only empirical rate functions
        if record.id == last_id {
            // Add to the current rate function
            times.push(record.time);
            values.push(record.value);
        } else {
            // Take the last values of times and values and make them into a rate function
            if !times.is_empty() {
                let fcn = Box::new(EmpiricalRate::new(times, values)?);
                context.add_rate_fn(fcn);
                last_id = record.id;
            }
            // Start the new values off
            times = vec![record.time];
            values = vec![record.value];
        }
    }
    // Add the last rate function in the CSV
    let fcn = Box::new(EmpiricalRate::new(times, values)?);
    context.add_rate_fn(fcn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::parameters::{GlobalParams, ItineraryWriteFnType, Params};

    use super::*;
    use ixa::{Context, ContextGlobalPropertiesExt};
    use statrs::assert_almost_eq;

    struct TestRateFn;

    impl InfectiousnessRateFn for TestRateFn {
        fn rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn cum_rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn inverse_cum_rate(&self, _events: f64) -> Option<f64> {
            Some(1.0)
        }
        fn infection_duration(&self) -> f64 {
            1.0
        }
    }

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_random() {
        let mut context = init_context();

        let rate_fn = TestRateFn {};
        context.add_rate_fn(Box::new(rate_fn));

        let i = context.get_random_rate_fn();
        assert!(i.0 == 0);
        assert_almost_eq!(context.get_rate_fn(i).rate(0.0), 1.0, 0.0);
    }

    #[test]
    fn test_load_rate_functions_constant() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        let rate_fns = context.get_data_container(RateFnPlugin).unwrap();
        assert_eq!(rate_fns.rates.len(), 1);
        let rate_fn = rate_fns.rates[0].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 1.0, 0.0);
        assert_almost_eq!(rate_fn.rate(5.1), 0.0, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 5.0, 0.0);
    }

    #[test]
    fn test_read_rate_function_file_multiple_functions() {
        let mut context = Context::new();
        let file = PathBuf::from("./tests/data/two_rate_fns.csv");
        add_rate_fns_from_file(&mut context, file).unwrap();
        let rate_fns = context.get_data_container(RateFnPlugin).unwrap();
        // Make sure we load two rate functions as expected
        assert_eq!(rate_fns.rates.len(), 2);
        // Check that rate function 1 is what we expect it to be
        let rate_fn = rate_fns.rates[0].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 1.0, 0.0);
        assert_almost_eq!(rate_fn.rate(1.0), 2.0, 0.0);
        assert_almost_eq!(rate_fn.rate(2.0), 3.0, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 2.0, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(2.0), 4.0, 0.0);
        // Check that rate function 2 is what we expect it to be
        let rate_fn = rate_fns.rates[1].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 2.0, 0.0);
        assert_almost_eq!(rate_fn.rate(3.0), 2.0, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 3.0, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(3.0), 6.0, 0.0);
    }
}
