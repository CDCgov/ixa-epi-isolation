use ixa::{
    define_data_plugin, define_person_property, define_person_property_with_default, define_rng,
    Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
};

use crate::{parameters::Parameters, utils::ContextUtilsExt};

define_rng!(NaturalHistorySamplerRng);

#[derive(Default)]
struct NaturalHistoryParameters {
    gi_trajectories: Vec<Vec<f64>>,
}

define_data_plugin!(
    NaturalHistory,
    NaturalHistoryParameters,
    NaturalHistoryParameters::default()
);

/// Read in input natural history parameter inputs and assemble valid parameter sets.
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Read in the generation interval trajectories from a CSV file.
    let path = &context
        .get_global_property_value(Parameters)
        .unwrap()
        .gi_trajectories_file;
    let mut vec_trajectories = Vec::new();
    let mut reader = csv::Reader::from_path(path)?;

    for result in reader.records() {
        let record = result?;
        let gi = record
            .iter()
            .map(|x| x.parse::<f64>().unwrap())
            .collect::<Vec<f64>>();
        vec_trajectories.push(gi);
    }

    let natural_history_container = context.get_data_container_mut(NaturalHistory);
    natural_history_container.gi_trajectories = vec_trajectories;
    Ok(())
}

define_person_property_with_default!(NaturalHistoryIdx, Option<usize>, None);

pub trait ContextNaturalHistoryExt {
    /// Set a person property that is a random index that refers to a natural history parameter set
    /// (generation interval, symptom onset time, symptom improvement time, viral load, etc.).
    fn set_natural_history_idx(&mut self, person_id: PersonId);
    /// Estimate the inverse generation interval (i.e., time since infection at which an infection attempt happens)
    /// from a CDF value (i.e., a value on 0 to 1 that represents the fraction of an individual's infectiousness
    /// that has passed) for a given person based on their generation interval trajectory. Uses linear interpolation
    /// to estimate the time from the discrete CDF samples.
    fn evaluate_inverse_generation_interval(
        &self,
        person_id: PersonId,
        gi_cdf_value_unif: f64,
    ) -> f64;
}

impl ContextNaturalHistoryExt for Context {
    fn set_natural_history_idx(&mut self, person_id: PersonId) {
        let num_trajectories = self
            .get_data_container(NaturalHistory)
            .expect("Natural history data container not initialized.")
            .gi_trajectories
            .len();
        let gi_index = self.sample_range(NaturalHistorySamplerRng, 0..num_trajectories);
        self.set_person_property(person_id, NaturalHistoryIdx, Some(gi_index));
    }

    fn evaluate_inverse_generation_interval(
        &self,
        person_id: PersonId,
        gi_cdf_value_unif: f64,
    ) -> f64 {
        // Let's first deal with the corner case -- the person is experiencing their first infection attempt.
        // Linear interpolation will fail because it would try to query a value of the CDF smaller than 0.0.
        // Instead, we return 0.0. This is the sensible value because it means that the person has
        // experienced none of their infectiousness at the start of their infection.
        if gi_cdf_value_unif == 0.0 {
            return 0.0;
        }
        // Grab the GI trajectory for this person.
        let gi_index = self
            .get_person_property(person_id, NaturalHistoryIdx)
            .expect("No GI index set. Has this person been infected?");
        let natural_history_container = self
            .get_data_container(NaturalHistory)
            .expect("Natural history data container not initialized.");
        let gi_trajectory = &natural_history_container.gi_trajectories[gi_index];
        let dt = self
            .get_global_property_value(Parameters)
            .unwrap()
            .gi_trajectories_dt;
        // The trajectories are provided in the form of the CDF of the GI over time in increments of `dt`. Since the
        // CDF is an always increasing function, the index of the first value greater than the uniform draw gives us
        // our entry point to figure out the values between which we interpolate.
        let upper_window_index = gi_trajectory
            .iter()
            .position(|&x| x - gi_cdf_value_unif > 0.0)
            .unwrap();
        // Because we want to interpolate the *inverse* CDF, the CDF values are "x" and the time values are "y".
        #[allow(clippy::cast_precision_loss)]
        self.linear_interpolation(
            gi_trajectory[upper_window_index - 1],
            gi_trajectory[upper_window_index],
            (upper_window_index - 1) as f64 * dt,
            upper_window_index as f64 * dt,
            gi_cdf_value_unif,
        )
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt};

    use crate::{
        natural_history_manager::init,
        parameters::{Parameters, ParametersValues},
    };

    use super::{ContextNaturalHistoryExt, NaturalHistoryIdx};

    fn setup() -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0: 2.5,
            gi_trajectories_dt: 0.1,
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            gi_trajectories_file: PathBuf::from("./tests/data/gi_trajectory.csv"),
        };
        let mut context = Context::new();
        context.init_random(params.seed);
        context
            .set_global_property_value(Parameters, params)
            .unwrap();
        context
    }

    #[test]
    fn test_set_natural_history_idx() {
        let mut context = setup();
        init(&mut context).unwrap();
        let person_id = context.add_person(()).unwrap();
        context.set_natural_history_idx(person_id);
        let gi_index = context.get_person_property(person_id, NaturalHistoryIdx);
        match gi_index {
            Some(0) => (),
            Some(idx) => panic!("Wrong GI index set. Should be 1, but is {idx}."),
            None => panic!("Should not panic. NH index should be set."),
        }
    }

    #[test]
    fn test_evaluate_inverse_generation_interval() {
        let mut context = setup();
        init(&mut context).unwrap();
        let person_id = context.add_person(()).unwrap();
        context.set_natural_history_idx(person_id);
        context.evaluate_inverse_generation_interval(person_id, 0.5);
    }
}
