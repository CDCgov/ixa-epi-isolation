use std::path::PathBuf;

use ixa::{
    define_data_plugin, define_rng, Context, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
};
use serde::Deserialize;

use crate::{
    infectiousness_manager::{InfectionData, InfectionDataValue},
    natural_history_parameter_manager::{
        ContextNaturalHistoryParameterExt, NaturalHistoryParameterLibrary,
    },
    parameters::{ContextParametersExt, RateFnType},
};

use super::{rate_fn::InfectiousnessRateFn, ConstantRate, EmpiricalRate};

define_rng!(InfectiousnessRng);

#[derive(Default)]
struct RateFnContainer {
    rates: Vec<Box<dyn InfectiousnessRateFn>>,
    asymptomatic_rates: Vec<Box<dyn InfectiousnessRateFn>>,
}

pub struct RateFn;

impl NaturalHistoryParameterLibrary for RateFn {
    fn library_size(&self, _context: &Context) -> usize {
        unreachable!("We manually specify the assignment function for RateFn, so this should never be called.")
    }
}

define_data_plugin!(RateFnPlugin, RateFnContainer, RateFnContainer::default());

fn add_rate_fn(
    rate_fns: &mut Vec<Box<dyn InfectiousnessRateFn>>,
    rate_fn: impl InfectiousnessRateFn + 'static,
) {
    rate_fns.push(Box::new(rate_fn));
}

pub trait InfectiousnessRateExt {
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn;
}

impl InfectiousnessRateExt for Context {
    /// Get the infectiousness rate function for a person.
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn {
        let container = self
            .get_data_container(RateFnPlugin)
            .expect("Expected rate functions to be loaded.");
        let id = self.get_parameter_id(RateFn, person_id);
        // Get if the person is symptomatic or not and chose rates accordingly if we have separate
        // asymptomatic rates loaded
        let separate_asymptomatic_rates = self.get_params().asymptomatic_rate_fn.is_some();
        let InfectionDataValue::Infectious { symptomatic, .. } =
            self.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infectious")
        };
        if symptomatic || !separate_asymptomatic_rates {
            // If the person is symptomatic, use the symptomatic rate functions
            container.rates[id].as_ref()
        } else {
            // If the person is asymptomatic, use the asymptomatic rate functions
            container.asymptomatic_rates[id].as_ref()
        }
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
    let asymptomatic_rate_of_infection = context.get_params().asymptomatic_rate_fn.clone();
    let container = context.get_data_container_mut(RateFnPlugin);

    // Load the base infectiousness rate functions
    append_based_on_rate_fn_type(rate_of_infection, &mut container.rates)?;

    // Load the asymptomatic infectiousness rate functions if they are specified
    if let Some(rate_of_infection) = asymptomatic_rate_of_infection {
        append_based_on_rate_fn_type(rate_of_infection, &mut container.asymptomatic_rates)?;
    }

    context.register_parameter_id_assigner(RateFn, |context, person_id| {
        // If the person is symptomatic, use the symptomatic rate functions, and if they are asymptomatic,
        // we use the asymptomatic rate functions if they are loaded.
        let InfectionDataValue::Infectious { symptomatic, .. } =
            context.get_person_property(person_id, InfectionData)
        else {
            panic!("Person {person_id} is not infectious")
        };
        let separate_asymptomatic_rates = context.get_params().asymptomatic_rate_fn.is_some();
        let container = context.get_data_container(RateFnPlugin).unwrap();
        if symptomatic || !separate_asymptomatic_rates {
            let len = container.rates.len();
            context.sample_range(InfectiousnessRng, 0..len)
        } else {
            // If there are asymptomatic rates loaded, use them
            let len = container.asymptomatic_rates.len();
            context.sample_range(InfectiousnessRng, 0..len)
        }
    })?;
    Ok(())
}

fn append_based_on_rate_fn_type(
    rate_fn_type: RateFnType,
    rate_fns: &mut Vec<Box<dyn InfectiousnessRateFn>>,
) -> Result<(), IxaError> {
    match rate_fn_type {
        RateFnType::Constant { rate, duration } => {
            add_rate_fn(rate_fns, ConstantRate::new(rate, duration)?);
        }
        RateFnType::EmpiricalFromFile { file, scale, .. } => {
            add_empirical_rate_fns_from_file(file, scale, rate_fns)?;
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

fn add_empirical_rate_fns_from_file(
    file: PathBuf,
    scale: f64,
    rate_fns: &mut Vec<Box<dyn InfectiousnessRateFn>>,
) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(file)?;
    let mut reader = reader.deserialize::<EmpiricalRateFnRecord>();
    // Pop out the first record so we can initialize the vectors
    let record = reader.next().unwrap()?;
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
            add_rate_fn(rate_fns, fcn);
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
    add_rate_fn(rate_fns, fcn);
    Ok(())
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{
        infectiousness_manager::InfectionContextExt,
        natural_history_parameter_manager::ContextNaturalHistoryParameterExt,
        parameters::{ContextParametersExt, GlobalParams, Params, RateFnType},
        rate_fns::{
            load_rate_fns,
            rate_fn_storage::{add_rate_fn, InfectiousnessRng, RateFnPlugin},
            InfectiousnessRateExt, InfectiousnessRateFn, RateFn,
        },
    };

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError};
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
            .register_parameter_id_assigner(RateFn, |context, _| {
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
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.0,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        let person = context.add_person(()).unwrap();
        context.infect_person(person, None, None, None);

        let rate_fn = TestRateFn {};
        let container = context.get_data_container_mut(RateFnPlugin);
        add_rate_fn(&mut container.rates, rate_fn);
        let rate_fns = context.get_data_container(RateFnPlugin).unwrap();
        assert_eq!(rate_fns.rates.len(), 1);

        assert_almost_eq!(context.get_person_rate_fn(person).rate(0.0), 1.0, 0.0);
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
            symptom_progression_library: None,
            fraction_asymptomatic: 0.0,
            asymptomatic_rate_fn: None,
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
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/two_rate_fns.csv"),
                scale,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.0,
            asymptomatic_rate_fn: None,
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
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/two_rate_fns_discontiguous_ids.csv"),
                scale: 1.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.0,
            asymptomatic_rate_fn: None,
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
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::EmpiricalFromFile {
                file: PathBuf::from("./tests/data/rate_fn_id_starts_at_two.csv"),
                scale: 1.0,
            },
            symptom_progression_library: None,
            fraction_asymptomatic: 0.0,
            asymptomatic_rate_fn: None,
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

    #[test]
    fn test_asymptomatics_get_normal_rates_when_no_asymptomatic_rates_loaded() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            // All people are asymptomatic
            fraction_asymptomatic: 1.0,
            asymptomatic_rate_fn: None,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        let person = context.add_person(()).unwrap();
        context.infect_person(person, None, None, None);
        let rate_fn = context.get_person_rate_fn(person);
        // Since we have no asymptomatic rates loaded, we should get the normal rates
        assert_almost_eq!(rate_fn.rate(0.0), 1.0, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 5.0, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(5.0), 5.0, 0.0);
    }

    #[test]
    fn test_asymptomatics_get_asympomatic_rates_when_loaded() {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            symptom_progression_library: None,
            // All people are asymptomatic
            fraction_asymptomatic: 1.0,
            asymptomatic_rate_fn: Some(RateFnType::Constant {
                rate: 0.5,
                duration: 2.5,
            }),
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: HashMap::new(),
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context.init_random(context.get_params().seed);
        load_rate_fns(&mut context).unwrap();
        let person = context.add_person(()).unwrap();
        context.infect_person(person, None, None, None);
        let rate_fn = context.get_person_rate_fn(person);
        // Since we have no asymptomatic rates loaded, we should get the normal rates
        assert_almost_eq!(rate_fn.rate(0.0), 0.5, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 2.5, 0.0);
        assert_almost_eq!(rate_fn.cum_rate(2.5), 1.25, 0.0);
    }
}
