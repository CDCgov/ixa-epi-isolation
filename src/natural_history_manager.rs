use std::{fmt::Display, path::PathBuf, str::FromStr};

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

/// Read in input natural history parameters from CSVs, validate them as valid inputs,
/// and store them for later querying through the `ContextNaturalHistoryExt` trait.
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    // Read in the generation interval trajectories from a CSV file.
    let path = &context
        .get_global_property_value(Parameters)
        .unwrap()
        .gi_trajectories_file;
    let gi_trajectories = read_arbitrary_column_csv::<f64>(path)?;
    // Check that the trajectories are valid inverse CDFs.
    check_valid_cdf(&gi_trajectories, "GI")?;

    let natural_history_container = context.get_data_container_mut(NaturalHistory);
    natural_history_container.gi_trajectories = gi_trajectories;
    Ok(())
}

/// Read in a CSV file with an arbitrary number of columns that presumably represent a series.
/// File should have a header, but the header is ignored. Returns a vector of the series (vectors)
/// of type `T`.
fn read_arbitrary_column_csv<T: FromStr>(path: &PathBuf) -> Result<Vec<Vec<T>>, IxaError>
where
    <T as FromStr>::Err: Display,
{
    let mut trajectories = Vec::new();
    let mut reader = csv::Reader::from_path(path)?;

    for result in reader.records() {
        let record = result?;
        let trajectory = record
            .iter()
            .map(|x| {
                x.parse::<T>()
                    .map_err(|e| IxaError::IxaError(e.to_string()))
            })
            .collect::<Result<Vec<T>, _>>()?;
        trajectories.push(trajectory);
    }
    // The way that we've configured the CSV reader means that it will raise errors for an empty row, given that
    // the preceding rows had data (it also raises an error for a row with a different number of columns).
    // But, if all the rows are empty, it won't raise an error automatically.
    if trajectories.is_empty() {
        return Err(IxaError::IxaError(format!(
            "No data found in file: {}",
            path.display()
        )));
    }
    Ok(trajectories)
}

/// A series of checks that ensure that trajectory in a vector of trajectories are a valid CSV.
fn check_valid_cdf(trajectories: &[Vec<f64>], debug_parameter_name: &str) -> Result<(), IxaError> {
    trajectories
        .iter()
        .enumerate()
        .try_for_each(|(i, x)| -> Result<(), IxaError> {
            if x.iter().any(|&y| !(0.0..=1.0).contains(&y)) {
                return Err(IxaError::IxaError(format!(
                    "{debug_parameter_name} CDF trajectory {} contains values not in the range [0, 1].",
                    i + 1
                )));
            }
            if x.windows(2).any(|w| w[0] > w[1]) {
                return Err(IxaError::IxaError(format!(
                    "{debug_parameter_name} CDF trajectory {} is not strictly increasing.",
                    i + 1
                )));
            }
            // If we've made it this far, we know that if the first value is 1.0, all the rest are 1.0 too, and that's bad.
            #[allow(clippy::float_cmp)]
            if x[0] == 1.0 {
                return Err(IxaError::IxaError(format!(
                    "{debug_parameter_name} CDF trajectory {} cannot start at 1.0.",
                    i + 1
                )));
            }
            Ok(())
        })
}

define_person_property_with_default!(NaturalHistoryIdx, Option<usize>, None);

/// Provides a way to interact with natural history parameters. This includes setting a natural history
/// index for a person at the beginning of their infection and querying their natural history parameters
/// (ex., generation interval) throughout their infection.
pub trait ContextNaturalHistoryExt {
    /// Set the person property `NaturalHistoryIdx` that refers to the index of a natural history parameter set
    /// (generation interval, symptom onset time, symptom improvement time, viral load, etc.) that will be used
    /// throughout this person's infection. Indeces are chosen uniformly and randomly.
    fn set_natural_history_idx(&mut self, person_id: PersonId);

    /// Estimate the value of the inverse generation interval (i.e., time since infection at which an infection
    /// attempt happens) from a CDF value (i.e., a value on 0 to 1 that represents the fraction of an individual's
    /// infectiousness that has passed) for a given person based on their set generation interval trajectory. Uses
    /// linear interpolation to estimate the time from the discrete CDF samples.
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
        // In this case, gi_cdf_value_unif will be 0.0. There are no points below 0.0 in a CDF, so interpolation
        // numerically will fairl. Instead, we return 0.0. This is the obvious value because it means that the person
        // has experienced none of their infectiousness at the start of their infection. It also ensures that if
        // GI CDF is 0.0 for some time after the start of infection, inverse_gi(\epsilon) - inverse_gi(0) = c > 0
        // even as \epsilon -> 0, which properly reproduces a CDF where an individual is not infectious immediately.
        if gi_cdf_value_unif == 0.0 {
            return 0.0;
        }
        // Grab the set GI trajectory for this person.
        let gi_index = self
            .get_person_property(person_id, NaturalHistoryIdx)
            .expect("No GI index set. Has this person been infected?");
        let natural_history_container = self
            .get_data_container(NaturalHistory)
            .expect("Natural history data container not initialized.");
        let gi_trajectory = &natural_history_container.gi_trajectories[gi_index];
        // Set up what we need for interpolation.
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

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError};
    use statrs::distribution::{ContinuousCDF, Exp};

    use crate::{
        natural_history_manager::{check_valid_cdf, init},
        parameters::{Parameters, ParametersValues},
        utils::ContextUtilsExt,
    };

    use super::{
        read_arbitrary_column_csv, ContextNaturalHistoryExt, NaturalHistory, NaturalHistoryIdx,
    };

    fn setup() -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0: 2.5,
            gi_trajectories_dt: 0.02,
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
    fn test_empty_csv() {
        let e = read_arbitrary_column_csv::<f64>(&PathBuf::from("./tests/data/empty.csv")).err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "No data found in file: ./tests/data/empty.csv".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that file should be empty. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error. Instead, read empty file with no errors."),
        }
    }

    #[test]
    fn test_automatic_column_number_detection() {
        let v = read_arbitrary_column_csv::<f64>(&PathBuf::from("./tests/data/three_columns.csv"))
            .unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].len(), 3);
    }

    #[test]
    fn test_input_out_of_cdf_range() {
        let bad_cdf = vec![vec![0.0, 0.5, 1.1]];
        let e = check_valid_cdf(&bad_cdf, "test").err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "test CDF trajectory 1 contains values not in the range [0, 1].".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that CDF values are not in the range [0, 1]. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error that CDF values are out of range. Instead, CDF validation passed with no error."),
        }
    }

    #[test]
    fn test_cdf_input_decreases() {
        let bad_cdf = vec![vec![0.0, 0.5, 0.4]];
        let e = check_valid_cdf(&bad_cdf, "test").err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "test CDF trajectory 1 is not strictly increasing.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that CDF values are not strictly increasing. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error that CDF values are not strictly increasing. Instead, CDF validation passed with no error."),
        }
    }

    #[test]
    fn test_cdf_input_flat() {
        let allowable_cdf = vec![vec![0.0, 0.0, 0.0]];
        check_valid_cdf(&allowable_cdf, "test").unwrap();
    }

    #[test]
    fn test_cdf_input_all_ones() {
        let bad_cdf = vec![vec![1.0, 1.0, 1.0]];
        let e = check_valid_cdf(&bad_cdf, "test").err();
        match e {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(msg, "test CDF trajectory 1 cannot start at 1.0.".to_string());
            }
            Some(ue) => panic!(
                "Expected an error that CDF values cannot start at 1.0. Instead got {:?}",
                ue.to_string()
            ),
            None => panic!("Expected an error that CDF values cannot start at 1.0. Instead, CDF validation passed with no error."),
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_natural_history_init() {
        // Check that the trajectory at this index is the toy GI we fed in -- CDF of exponential distribution
        // with rate 1.
        let mut context = setup();
        init(&mut context).unwrap();
        let gi_trajectory = &context
            .get_data_container(NaturalHistory)
            .unwrap()
            .gi_trajectories;
        assert_eq!(gi_trajectory.len(), 1);
        let cdf = |x| Exp::new(1.0).unwrap().cdf(x);
        let dt = context
            .get_global_property_value(Parameters)
            .unwrap()
            .gi_trajectories_dt;
        let expected_trajectory = (0..gi_trajectory[0].len())
            .map(|x| cdf(x as f64 * dt))
            .collect::<Vec<f64>>();
        let diff = gi_trajectory[0]
            .iter()
            .zip(expected_trajectory.iter())
            .map(|(x, y)| (x - y).abs())
            .collect::<Vec<f64>>();
        // Small differences due to R vs Rust's algorithm for calculating the value of the CDF at the tails.
        assert!(diff.iter().all(|&x| x < f64::EPSILON));
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
            Some(idx) => panic!("Wrong GI index set. Should be 0, but is {idx}."),
            None => panic!("Should not panic. NH index should be set."),
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_evaluate_inverse_generation_interval() {
        let mut context = setup();
        init(&mut context).unwrap();
        let dt = context
            .get_global_property_value(Parameters)
            .unwrap()
            .gi_trajectories_dt;
        let person_id = context.add_person(()).unwrap();
        context.set_natural_history_idx(person_id);
        // Check that a CDF value of 0.0 returns a time of 0.0.
        assert_eq!(
            context.evaluate_inverse_generation_interval(person_id, 0.0),
            0.0
        );
        // Check some values of the inverse CDF.
        let cdf = |x| Exp::new(1.0).unwrap().cdf(x);
        // No interpolation required because we pick an integer increment of dt.
        assert!(
            (context.evaluate_inverse_generation_interval(person_id, cdf(10.0 * dt)) - 10.0 * dt)
                .abs()
                < f64::EPSILON
        );
        // Interpolation required.
        // Test values for small, middle, and large dt.
        assert!(
            (context.evaluate_inverse_generation_interval(person_id, cdf(9.8 * dt))
                - context.linear_interpolation(
                    cdf(9.0 * dt),
                    cdf(10.0 * dt),
                    9.0 * dt,
                    10.0 * dt,
                    cdf(9.8 * dt)
                ))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (context.evaluate_inverse_generation_interval(person_id, cdf(159.5 * dt))
                - context.linear_interpolation(
                    cdf(159.0 * dt),
                    cdf(160.0 * dt),
                    159.0 * dt,
                    160.0 * dt,
                    cdf(159.5 * dt)
                ))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (context.evaluate_inverse_generation_interval(person_id, cdf(389.2 * dt))
                - context.linear_interpolation(
                    cdf(389.0 * dt),
                    cdf(390.0 * dt),
                    389.0 * dt,
                    390.0 * dt,
                    cdf(389.2 * dt)
                ))
            .abs()
                < f64::EPSILON
        );
    }
}
