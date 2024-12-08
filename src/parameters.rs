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
    pub report_period: f64,
    pub synth_population_file: PathBuf,
    pub tri_vl_params_file: PathBuf,
    pub population_periodic_report: String,
}

fn validate_inputs(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.r_0 < 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number.".to_string(),
        ));
    }
    if parameters.incubation_period[0] <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period scale must be positive.".to_string(),
        ));
    }
    if parameters.incubation_period[1] <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period shape must be positive.".to_string(),
        ));
    }
    Ok(())
}

define_rng!(NHParametersRng);

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IsolationGuidanceParams {
    pub peak_time: f64,
    pub peak_magnitude: f64,
    pub proliferation_time: f64,
    pub clearance_time: f64,
    pub symptom_improvement_time: f64,
}

#[derive(Debug, Clone, PartialEq)]
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
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
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
        // Truncate the distribution to only consider values greater than the minimum symptom onset time.
        // Do this by calculating the index of the minimum symptom onset time in the distribution.
        let min_idx = (f64::ceil(f64::max(min_symptom_onset_time * 1000.0 / 23.0, 0.0))) as usize;
        let symptom_onset_time_sampled = (context
            .sample_weighted(NHParametersRng, &prob_incubation_period_times[min_idx..])
            as f64)
            * 23.0
            / 1000.0
            + min_symptom_onset_time;
        assert!(symptom_onset_time_sampled >= min_symptom_onset_time);
        context
            .get_data_container_mut(NaturalHistory)
            .push(TriVLParams {
                iso_guid_params: iso_guid_param_set,
                symptom_onset_time: symptom_onset_time_sampled,
            });
    }
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

#[cfg(test)]
mod test {
    use ixa::{
        error::IxaError, Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt,
    };

    use super::{validate_inputs, Parameters};
    use std::path::PathBuf;

    use crate::parameters::{ContextParametersExt, ParametersValues};

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            incubation_period: [1.5, 3.6, 0.15],
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
    fn test_assemble_nh_params() {
        // Do we get the same value for the same person in two separate contexts?
        let mut context1 = Context::new();
        let mut context2 = Context::new();
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 108,
            r_0: 2.0,
            incubation_period: [1.5, 3.6, 0.15],
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("./tests/data/tri_vl_params.csv"),
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        context1.init_random(108);
        context2.init_random(108);
        context1
            .set_global_property_value(Parameters, parameters.clone())
            .unwrap();
        context2
            .set_global_property_value(Parameters, parameters.clone())
            .unwrap();
        let person1 = context1.add_person(()).unwrap();
        let person2 = context2.add_person(()).unwrap();

        let nh_sample1 = context1.sample_natural_history(person1);
        let nh_sample2 = context2.sample_natural_history(person2);
        assert_eq!(nh_sample1, nh_sample2);

        // These values should not change when run again.
        assert_eq!(nh_sample1, context1.sample_natural_history(person1));
        assert_eq!(nh_sample2, context2.sample_natural_history(person2));
    }
}
