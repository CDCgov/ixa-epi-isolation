use std::path::PathBuf;

use ixa::{define_data_plugin, define_rng, Context, ContextRandomExt, IxaError, PersonId};
use serde::Deserialize;

use crate::{
    natural_history_parameter_manager::{
        ContextNaturalHistoryParameterExt, NaturalHistoryParameterLibrary,
    },
    parameters::{ContextParametersExt, Params, RateFnType},
};

use super::{rate_fn::InfectiousnessRateFn, ConstantRate, EmpiricalRate};

define_rng!(InfectiousnessRng);

struct RateFnContainer {
    rates: Vec<Box<dyn InfectiousnessRateFn>>,
}

#[derive(Debug)]
pub struct RateFn;

impl NaturalHistoryParameterLibrary for RateFn {
    fn library_size(&self, context: &Context) -> usize {
        context
            .get_data_container(RateFnPlugin)
            .unwrap()
            .rates
            .len()
    }
}

define_data_plugin!(
    RateFnPlugin,
    RateFnContainer,
    RateFnContainer { rates: Vec::new() }
);

pub trait InfectiousnessRateExt {
    fn add_rate_fn(&mut self, dist: impl InfectiousnessRateFn + 'static);
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn;
}

impl InfectiousnessRateExt for Context {
    fn add_rate_fn(&mut self, dist: impl InfectiousnessRateFn + 'static) {
        let container = self.get_data_container_mut(RateFnPlugin);
        container.rates.push(Box::new(dist));
    }

    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn {
        let id = self.get_parameter_id(RateFn, person_id);
        self.get_data_container(RateFnPlugin)
            .expect("Expected rate functions to be loaded.")
            .rates[id]
            .as_ref()
    }
}

#[allow(clippy::missing_panics_doc)]
/// Turn the information specified in the global parameter `infectiousness_rate_fn` into actual
/// infectiousness rate functions for the simulation.
/// # Errors
/// - If the parameters used to specify the rate functions are invalid
/// - If the file specified in the parameters cannot be read and turned into `EmpiricalRate` objects
pub fn load_rate_fns(context: &mut Context) -> Result<(), IxaError> {
    let rate_of_infection = context.get_params().infectiousness_rate_fn.clone();

    match rate_of_infection {
        RateFnType::Constant { rate, duration } => {
            context.add_rate_fn(ConstantRate::new(rate, duration)?);
        }
        RateFnType::EmpiricalFromFile { file, .. } => {
            add_rate_fns_from_file(context, file)?;
        }
    }

    context.register_parameter_id_assigner(RateFn, |context, _person_id| {
        let library_size = RateFn.library_size(context);
        context.sample_range(InfectiousnessRng, 0..library_size)
    })?;
    Ok(())
}

#[derive(Deserialize)]
pub struct EmpiricalRateFnRecord {
    id: u32,
    time: f64,
    value: f64,
}

fn add_rate_fns_from_file(context: &mut Context, file: PathBuf) -> Result<(), IxaError> {
    let Params {
        infectiousness_rate_fn,
        ..
    } = context.get_params();
    let &RateFnType::EmpiricalFromFile { scale, .. } = infectiousness_rate_fn else {
        unreachable!("This function should only be called for empirical rate functions");
    };
    let mut reader = csv::Reader::from_path(file)?;
    let mut reader = reader.deserialize();
    // Pop out the first record so we can initialize the vectors
    let record: EmpiricalRateFnRecord = reader.next().unwrap()?;
    let mut last_id = record.id;
    // Require that the first id is 1
    if last_id != 1 {
        return Err(IxaError::IxaError(format!(
            "First id in the file should be 1, got {last_id}."
        )));
    }
    let mut times = vec![record.time];
    let mut values = vec![record.value * scale];
    for record in reader {
        let record = record?;
        // For now, assume that we are only reading in empirical rate functions, so the code
        // below is pegged to the input format for empirical rate functions.
        if record.id == last_id {
            // Add to the current rate function
            times.push(record.time);
            values.push(record.value * scale);
        } else {
            // Take the last values of times and values and make them into a rate function
            let fcn = EmpiricalRate::new(times, values)?;
            context.add_rate_fn(fcn);
            // Check that the ids are contiguous
            if record.id != last_id + 1 {
                return Err(IxaError::IxaError(format!(
                    "Ids are not contiguous: expected {}, got {}",
                    last_id + 1,
                    record.id
                )));
            }
            last_id = record.id;
            // Start the new values off
            times = vec![record.time];
            values = vec![record.value * scale];
        }
    }
    // Add the last rate function in the CSV
    let fcn = EmpiricalRate::new(times, values)?;
    context.add_rate_fn(fcn);
    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::parameters::{GlobalParams, Params};

    use super::*;
    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt};
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
            .register_parameter_id_assigner(RateFn, |context, _person_id| {
                let container = context.get_data_container(RateFnPlugin).unwrap();
                let len = container.rates.len();
                context.sample_range(InfectiousnessRng, 0..len)
            })
            .unwrap();
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_random() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();

        let rate_fn = TestRateFn {};
        context.add_rate_fn(rate_fn);
        let rate_fns = context.get_data_container(RateFnPlugin).unwrap();
        assert_eq!(rate_fns.rates.len(), 1);

        assert_almost_eq!(context.get_person_rate_fn(person).rate(0.0), 1.0, 0.0);
    }

    #[test]
    fn test_load_rate_functions_constant() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
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
        let scale = 2.0;
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/two_rate_fns.csv"),
                scale,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        let rate_fns = context.get_data_container(RateFnPlugin).unwrap();
        // Make sure we load two rate functions as expected
        assert_eq!(rate_fns.rates.len(), 2);
        // Check that rate function 1 is what we expect it to be
        let rate_fn = rate_fns.rates[0].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 1.0 * scale, 0.0);
        assert_almost_eq!(rate_fn.rate(1.0), 2.0 * scale, 0.0);
        assert_almost_eq!(rate_fn.rate(2.0), 3.0 * scale, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 2.0, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(2.0), 4.0 * scale, 0.0);
        // Check that rate function 2 is what we expect it to be
        let rate_fn = rate_fns.rates[1].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 2.0 * scale, 0.0);
        assert_almost_eq!(rate_fn.rate(3.0), 2.0 * scale, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 3.0, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(3.0), 6.0 * scale, 0.0);
    }

    #[test]
    fn test_read_rate_function_discontiguous_ids() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/two_rate_fns_discontiguous_ids.csv"),
                scale: 1.0,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        let e = load_rate_fns(&mut context).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "Ids are not contiguous: expected 2, got 3".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the the ids are not contiguous. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!(
                "Expected an error. Instead, reading the rate functions passed with no errors."
            ),
        }
    }

    #[test]
    fn test_read_rate_function_id_starts_at_two() {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.0,
            initial_recovered: 0.0,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/rate_fn_id_starts_at_two.csv"),
                scale: 1.0,
            },
            symptom_progression_library: None,
            proportion_asymptomatic: 0.0,
            relative_infectiousness_asymptomatics: 0.0,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        let e = load_rate_fns(&mut context).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "First id in the file should be 1, got 2.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that the the ids do not start at 1. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!(
                "Expected an error. Instead, reading the rate functions passed with no errors."
            ),
        }
    }
}
