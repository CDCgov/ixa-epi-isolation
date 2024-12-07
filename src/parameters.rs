use std::hash::{DefaultHasher, Hasher};
use std::path::PathBuf;
use std::{fmt::Debug, hash::Hash};

use ixa::{define_data_plugin, define_global_property, error::IxaError, Context, PersonId};
use ixa::{define_rng, ContextGlobalPropertiesExt, ContextRandomExt};
use serde::{Deserialize, Serialize};
use statrs::distribution::{Continuous, Weibull};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub incubation_period: [f64; 3],
    pub generation_interval: f64,
    pub report_period: f64,
    pub synth_population_file: PathBuf,
    pub tri_vl_params_file: PathBuf,
    pub population_periodic_report: String,
}

define_rng!(NHParametersRng);

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct IsolationGuidanceParams {
    pub peak_time: f64,
    pub peak_magnitude: f64,
    pub proliferation_time: f64,
    pub clearance_time: f64,
    pub symptom_improvement_time: f64,
}

#[derive(Debug, Clone)]
pub struct TriVLParams {
    pub iso_guid_params: IsolationGuidanceParams,
    pub symptom_onset_time: f64,
}

define_data_plugin!(NaturalHistory, Vec<TriVLParams>, Vec::<TriVLParams>::new());

pub trait ContextParametersExt {
    fn sample_natural_history(&mut self, person_id: PersonId) -> TriVLParams;
}

impl ContextParametersExt for Context {
    fn sample_natural_history(&mut self, person_id: PersonId) -> TriVLParams {
        if let Some(nh) = self.get_data_container(NaturalHistory) {
            // Query a set of natural history parameters for this person
            let mut hasher = DefaultHasher::new();
            person_id.hash(&mut hasher);
            let idx = hasher.finish();
            nh[usize::try_from(idx).unwrap() % nh.len()].clone()
        } else {
            // Build the natural history dataset.
            let iso_guid_params = load_isolation_guidance_params(self);
            assemble_natural_history_params(self, iso_guid_params);
            // Now actually query a natural history for this person.
            self.sample_natural_history(person_id)
        }
    }
}

fn load_isolation_guidance_params(context: &Context) -> Vec<IsolationGuidanceParams> {
    let mut iso_guid_params: Vec<IsolationGuidanceParams> = Vec::new();
    // Read parameters from file.
    let path = context
        .get_global_property_value(Parameters)
        .unwrap()
        .tri_vl_params_file
        .clone();
    let mut reader = csv::Reader::from_path(path).expect("Tri VL file not found.");
    for result in reader.deserialize() {
        let record: IsolationGuidanceParams =
            result.expect("Failed to parse Tri VL parameters from file.");
        iso_guid_params.push(record);
    }
    iso_guid_params
}

#[allow(clippy::cast_precision_loss)]
fn assemble_natural_history_params(
    context: &mut Context,
    iso_guid_params: Vec<IsolationGuidanceParams>,
) {
    // We also need the incubation period distribution.
    // We use the COVID-19 incubation distribution from Park et al. (2023) PNAS.
    // This function is exactly what's plotted in Fig 2b of that paper.
    let parameters = context.get_global_property_value(Parameters).unwrap();
    let shape = parameters.incubation_period[0];
    let scale = parameters.incubation_period[1];
    let growth_rate = parameters.incubation_period[2];
    let weibull = Weibull::new(shape, scale).unwrap();
    let prob_incubation_period_times: Vec<f64> = (0..1000)
        .map(|t| {
            // Rescale the t values to be on the range of incubation times.
            // Looking at the density, NNH uses a max value of 23.0.
            let t = (f64::from(t)) * 23.0 / 1000.0;
            weibull.pdf(t) * f64::exp(growth_rate * t)
        })
        .collect();
    for iso_guid_param_set in iso_guid_params {
        // Since infectiousness cannot start before the symptom onset time, we place a constraint
        // on symptom onset times.
        let min_symptom_onset_time =
            -(iso_guid_param_set.peak_time - iso_guid_param_set.proliferation_time);
        let mut symptom_onset_time_sampled = min_symptom_onset_time;
        while symptom_onset_time_sampled < min_symptom_onset_time {
            symptom_onset_time_sampled =
                (context.sample_weighted(NHParametersRng, &prob_incubation_period_times) as f64)
                    * 23.0
                    / 1000.0;
        }
        context
            .get_data_container_mut(NaturalHistory)
            .push(TriVLParams {
                iso_guid_params: iso_guid_param_set,
                symptom_onset_time: symptom_onset_time_sampled,
            });
    }
}

fn validate_inputs(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.r_0 < 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number.".to_string(),
        ));
    }
    if parameters.generation_interval <= 0.0 {
        return Err(IxaError::IxaError(
            "The generation interval must be positive.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

#[cfg(test)]
mod test {
    use ixa::error::IxaError;

    use super::validate_inputs;
    use std::path::PathBuf;

    use crate::parameters::ParametersValues;

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            incubation_period: [1.5, 3.6, 0.15],
            generation_interval: 5.0,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("."),
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "r_0 must be a non-negative number.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that r_0 validation should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }

    #[test]
    fn test_validate_gi() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            incubation_period: [1.5, 3.6, 0.15],
            generation_interval: 0.0,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("."),
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let e = validate_inputs(&parameters).err();
        match e {
            Some(IxaError::IxaError(msg)) => assert_eq!(msg, "The generation interval must be positive.".to_string()),
            Some(ue) => panic!("Expected an error that the generation interval validation should fail. Instead got {:?}", ue.to_string()),
            None => panic!("Expected an error. Instead, validation passed with no errors."),
        }
    }
}
