use std::{
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
};

use ixa::{define_global_property, IxaError};
use serde::{Deserialize, Serialize};
use statrs::distribution::{Continuous, Weibull};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub incubation_period_shape: f64,
    pub incubation_period_scale: f64,
    pub growth_rate_incubation_period: f64,
    pub report_period: f64,
    pub synth_population_file: PathBuf,
    pub tri_vl_params_file: PathBuf,
}

fn validate_inputs(parameters: &ParametersValues) -> Result<(), IxaError> {
    if parameters.r_0 < 0.0 {
        return Err(IxaError::IxaError(
            "r_0 must be a non-negative number.".to_string(),
        ));
    }
    if parameters.incubation_period_shape <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period shape parameter must be positive.".to_string(),
        ));
    }
    if parameters.incubation_period_scale <= 0.0 {
        return Err(IxaError::IxaError(
            "The incubation period scale parameter must be positive.".to_string(),
        ));
    }
    Ok(())
}

define_global_property!(Parameters, ParametersValues, validate_inputs);

define_rng!(NHParametersRng);

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
    /// Return a set of natural history parameters for a person.
    /// Natural history parameters include the isolation guidance-calculated
    /// parameters (triangle viral load parameters + symptom improvement time) and a valid
    /// symptom onset time drawn from Park et al. (2023) PNAS.
    /// Guarantees that the same person will always have the same natural history parameters.
    fn sample_natural_history(&mut self, person_id: PersonId) -> TriVLParams;
}

impl ContextParametersExt for Context {
    fn sample_natural_history(&mut self, person_id: PersonId) -> TriVLParams {
        // If the natural history dataset has already been built, just query it.
        if let Some(nh) = self.get_data_container(NaturalHistory) {
            // Query a set of natural history parameters for this person.
            // We require that we return the same natural history dataset for a given person because
            // we need the natural history parameters multiple times in different modules, so we need
            // to call this function multiple times for the same person. As a result, we use the hash of
            // the person ID to pick a person-specific index from the dataframe.
            // Note that one unintended consequence of this setup for now is that the person's natural
            // history parameters would not change if they were to get reinfected.
            // In the future, we could add the number of previous infections to the hash of the person ID
            // when we get the actual parameter set.
            let mut hasher = DefaultHasher::new();
            person_id.hash(&mut hasher);
            let idx = hasher.finish();
            // Since we do not require that the natural history dataset be the same size as the population,
            // we take the modulo of the index to ensure that we always have a valid index.
            nh[usize::try_from(idx).unwrap() % nh.len()].clone()
        } else {
            // Build the natural history dataset as it has not been queried before.
            // This only happens in the tests for the transmission manager where
            // we do not call `make_derived_parameters` (we cannot access this function in another module)
            // prior to calling `sample_natural_history`.
            make_derived_parameters(self).expect(
                "Error assembling isolation guidance parameters from specified input file.",
            );
            // Now actually query a natural history for this person.
            self.sample_natural_history(person_id)
        }
    }
}

/// Load the isolation guidance parameters from the specified input file.
/// Assumes the parameters are stored in a CSV file with the following columns:
/// peak time, peak magnitude, proliferation time, clearance time, symptom improvement time.
fn load_isolation_guidance_params(
    path: &PathBuf,
) -> Result<Vec<IsolationGuidanceParams>, IxaError> {
    let mut iso_guid_params: Vec<IsolationGuidanceParams> = Vec::new();
    let mut reader = csv::Reader::from_path(path)?;
    for result in reader.deserialize() {
        let record: IsolationGuidanceParams = result?;
        iso_guid_params.push(record);
    }
    Ok(iso_guid_params)
}

#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
/// Assemble the natural history parameters for each person in the simulation.
/// Takes the isolation guidance parameters and concatenates each set with a valid symptom onset time.
/// The symptom onset time is sampled from the incubation period distribution in Park et al. (2023) PNAS,
/// truncated to only consider that do not force the triangle viral load infectiousness to start before the
/// symptom onset time.
fn assemble_natural_history_params(
    context: &mut Context,
    iso_guid_params: Vec<IsolationGuidanceParams>,
) -> Result<(), IxaError> {
    // We also need the incubation period distribution.
    // We use the COVID-19 incubation distribution from Park et al. (2023) PNAS.
    // This function is exactly what's plotted in Fig 2b of that paper.
    // Concretely, it is a Weibull distribution times an exponential growth curve.
    let parameters = context.get_global_property_value(Parameters).unwrap();
    let weibull = Weibull::new(
        parameters.incubation_period_shape,
        parameters.incubation_period_scale,
    )
    .unwrap();
    let prob_incubation_period_times: Vec<f64> = (0..1000)
        .map(|t| {
            // Rescale the t values to be on the range of incubation times.
            // Looking at the density, NNH uses a max value of 23.0.
            // Because we place a constraint on symptom onset times to make them
            // valid based on the siolation guidance parameters, by truncating this distribution
            // at 23.0 days, we are automatically rejecting some potential parameter sets.
            let t = (f64::from(t)) * 23.0 / 1000.0;
            weibull.pdf(t) * f64::exp(parameters.growth_rate_incubation_period * t)
        })
        .collect();
    for iso_guid_param_set in iso_guid_params {
        // Since infectiousness cannot start before the symptom onset time, we place a constraint
        // on symptom onset times.
        let min_symptom_onset_time =
            -(iso_guid_param_set.peak_time - iso_guid_param_set.proliferation_time);
        // Because we truncate the distribution at 23.0 days, we are implicitly rejecting parameter
        // sets that would require symptom onset times to be greater than 23 days.
        if min_symptom_onset_time < 23.0 - (23.0 / 1000.0) {
            // Truncate the distribution to only consider values greater than the minimum symptom onset time.
            // Do this by calculating the index of the minimum symptom onset time in the distribution, and then
            // sampling from the distribution starting at that index.
            let min_idx =
                (f64::ceil(f64::max(min_symptom_onset_time * 1000.0 / 23.0, 0.0))) as usize;
            let symptom_onset_time_sampled_idx =
                context.sample_weighted(NHParametersRng, &prob_incubation_period_times[min_idx..]);
            // Convert the sampled index back to a time value, accounting for the truncation of the distribution which
            // really shifted all of our indices by the minimum symptom onset time.
            let symptom_onset_time_sampled =
                (symptom_onset_time_sampled_idx as f64) * 23.0 / 1000.0 + min_symptom_onset_time;
            assert!(symptom_onset_time_sampled >= min_symptom_onset_time);
            context
                .get_data_container_mut(NaturalHistory)
                .push(TriVLParams {
                    iso_guid_params: iso_guid_param_set,
                    symptom_onset_time: symptom_onset_time_sampled,
                });
        }
    }
    if context.get_data_container(NaturalHistory).is_none() {
        return Err(IxaError::IxaError(
            "No valid natural history parameter sets could be generated.".to_string(),
        ));
    }
    Ok(())
}

/// Read in input natural history parameter inputs and assemble valid parameter sets.
pub fn make_derived_parameters(context: &mut Context) -> Result<(), IxaError> {
    // Read in the isolation guidance parameters from the specified input file.
    let path = &context
        .get_global_property_value(Parameters)
        .unwrap()
        .tri_vl_params_file;
    let iso_guid_params = load_isolation_guidance_params(path)?;
    // Assemble a vector of natural history parameters by also sampling symptom onset times
    // conditioned on the isolation guidance parameters.
    assemble_natural_history_params(context, iso_guid_params)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use ixa::IxaError;

    use super::{validate_inputs, Parameters};
    use std::path::PathBuf;

    use crate::parameters::{make_derived_parameters, ContextParametersExt, ParametersValues};

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            incubation_period_shape: 1.5,
            incubation_period_scale: 3.6,
            growth_rate_incubation_period: 0.15,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("."),
            synth_population_file: PathBuf::from("."),
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
            incubation_period_shape: 1.5,
            incubation_period_scale: 3.6,
            growth_rate_incubation_period: 0.15,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("./tests/data/tri_vl_params.csv"),
            synth_population_file: PathBuf::from("."),
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

    #[test]
    fn test_reject_bad_params() {
        // Do we get the same value for the same person in two separate contexts?
        let mut context = Context::new();
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 108,
            r_0: 2.0,
            incubation_period_shape: 1.5,
            incubation_period_scale: 3.6,
            growth_rate_incubation_period: 0.15,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("./tests/data/tri_vl_params_bad.csv"),
            synth_population_file: PathBuf::from("."),
        };
        context
            .set_global_property_value(Parameters, parameters)
            .unwrap();
        context.init_random(108);
        let e = make_derived_parameters(&mut context).err();

        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "No valid natural history parameter sets could be generated.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that parameter set assembly should fail. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, assembled parameter sets with no errors."),
        }
    }
}
