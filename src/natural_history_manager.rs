use std::{fmt::Display, path::PathBuf, str::FromStr};

use ixa::{
    define_data_plugin, define_person_property, define_person_property_with_default, define_rng,
    Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
};
use splines::{Interpolation, Key, Spline};

use crate::parameters::Parameters;

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
        // There must be at least two values in each trajectory for linear interpolation.
        if trajectory.len() < 2 {
            return Err(IxaError::IxaError(format!(
                "File {} has fewer than 2 values per trajectory.",
                path.display()
            )));
        }
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
        #[allow(clippy::cast_precision_loss)]
        let times = (0..gi_trajectory.len())
            .map(|x| (x as f64) * dt)
            .collect::<Vec<f64>>();
        // Because we want to interpolate the *inverse* CDF, the CDF values are "x" and the time values are "y".
        interpolate(gi_trajectory, &times, gi_cdf_value_unif)
    }
}

/// An interpolation routine that expects a paired set of values `xs` and `ys` that represent samples
/// from a given function. The function is evaluated at a given x value `xp` using cubic spline interpolation
/// when there are at least two samples above and below `xp`. Otherwise, it uses linear extrapolation at the tails.
/// Assumes that the `xs` are sorted in ascending order.
fn interpolate(xs: &[f64], ys: &[f64], xp: f64) -> f64 {
    let upper_window_index_option = xs.iter().position(|&x| x > xp);
    // We need to check whether a point was found. If it wasn't, it means that all values
    // in `xs` are less than `xp`. We have to use an alternative extrapolation strategy.
    let upper_window_index = match upper_window_index_option {
        None => {
            // We are above the range of the `xs` samples, so we must extrapolate. We default to linear extrapolation
            // using the last two values in `xs`.
            let traj_len = xs.len();
            return linear_interpolation(
                xs[traj_len - 2],
                xs[traj_len - 1],
                ys[traj_len - 2],
                ys[traj_len - 1],
                xp,
            );
        }
        Some(i) => match i {
            // We only have one point in `xs` below `xp`, so we must also default to linear extrapolation.
            1 => return linear_interpolation(xs[0], xs[1], ys[0], ys[1], xp),
            // Else, we use cubic spline interpolation.
            // The index can never return 0 because we handle that case at the beginning of the function.
            _ => i,
        },
    };
    // If our interpolation point is in the range of the `xs` samples, we can use cubic spline interpolation.
    cubic_spline_interpolation(
        xs[(upper_window_index - 2)..=(upper_window_index + 1)]
            .try_into()
            .unwrap(),
        // Since `upper_window_index` is the third index in the trajectory, we need to subtract 2 to get the first index.
        ys[(upper_window_index - 2)..=(upper_window_index + 1)]
            .try_into()
            .unwrap(),
        xp,
    )
}

/// Fits a line between two points and returns the value of the line at a given x-value `xp`.
/// Sensibly handles equal x-values by returning `y1` for `xp < x1`, `y2` for `xp > x2`, and
/// `(y1 + y2) / 2` for `xp = x1 = x2`.
fn linear_interpolation(x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64 {
    // At the tails of the CDF, the CDF moves very slowly, so with numerical imprecision, x2 may equal x1.
    #[allow(clippy::float_cmp)]
    if x2 == x1 {
        // The only sensible thing to return is the bounds.
        if xp < x1 {
            return y1;
        } else if xp > x2 {
            return y2;
        }
        return (y1 + y2) / 2.0;
    }
    (y2 - y1) / (x2 - x1) * (xp - x1) + y1
}

/// Fits a cubic spline between four points and returns the value of the spline at a given x-value `xp`.
/// Requires that `xp` be between the second and third ordered points in the `xs` array.
fn cubic_spline_interpolation(xs: &[f64; 4], ys: &[f64; 4], xp: f64) -> f64 {
    let spline_vec = xs
        .iter()
        .zip(ys.iter())
        .map(|(&x, &y)| Key::new(x, y, Interpolation::CatmullRom));
    let spline = Spline::from_iter(spline_vec);
    // Sampling from a clamped spline can return `None` if (a) there are fewer than four points,
    // or (b) the `xp` is outside the range of the spline. We've already accounted for (a) in our
    // `match` in `interpolation`, and we also ensure (b) can not happen with the way we search for
    // the upper window index that guarantees that `xp` is between the second and third points.
    spline.sample(xp).unwrap()
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError};
    use statrs::{
        assert_almost_eq,
        distribution::{ContinuousCDF, Exp},
    };

    use crate::{
        natural_history_manager::{
            check_valid_cdf, cubic_spline_interpolation, init, linear_interpolation,
        },
        parameters::{Parameters, ParametersValues},
    };

    use super::{
        interpolate, read_arbitrary_column_csv, ContextNaturalHistoryExt, NaturalHistory,
        NaturalHistoryIdx,
    };

    fn setup() -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0: 2.5,
            gi_trajectories_dt: 0.2,
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
            Some(ue) => panic!("Expected an error that file should be empty. Instead got {ue}."),
            None => panic!("Expected an error. Instead, read empty file with no errors."),
        }
    }

    #[test]
    fn test_at_least_two_columns() {
        let v =
            read_arbitrary_column_csv::<f64>(&PathBuf::from("./tests/data/one_column.csv")).err();
        match v {
            Some(IxaError::IxaError(msg)) => {
                assert_eq!(
                    msg,
                    "File ./tests/data/one_column.csv has fewer than 2 values per trajectory.".to_string()
                );
            }
            Some(ue) => panic!(
                "Expected an error that file should have at least two columns. Instead got {ue}."
            ),
            None => panic!("Expected an error that file does not have at least two columns. Instead, read file with one column with no errors."),
        }
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: CsvError(Error(UnequalLengths { pos: Some(Position { byte: 12, line: 3, record: 2 }), expected_len: 2, len: 3 }))"
    )]
    fn test_column_size_changes() {
        read_arbitrary_column_csv::<f64>(&PathBuf::from("./tests/data/column_size_changes.csv"))
            .unwrap();
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
                "Expected an error that CDF values are not in the range [0, 1]. Instead got {ue}."
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
                "Expected an error that CDF values are not strictly increasing. Instead got {ue}."
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
                "Expected an error that CDF values cannot start at 1.0. Instead got {ue}."
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
        #[allow(clippy::cast_precision_loss)]
        let expected_trajectory = (0..gi_trajectory[0].len())
            .map(|x| cdf(x as f64 * dt))
            .collect::<Vec<f64>>();
        let diff = gi_trajectory[0]
            .iter()
            .zip(expected_trajectory.iter())
            .map(|(x, y)| (x - y).abs())
            .collect::<Vec<f64>>();
        // Small differences due to R vs Rust's algorithm for calculating the value of the CDF at the tails.
        assert!(!diff.iter().any(|&x| x > f64::EPSILON));
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
    fn test_linear_interpolation_base() {
        // Test some basic linear interpolation.
        let result = linear_interpolation(1.0, 2.0, 3.0, 6.0, 1.25);
        assert_eq!(result, 3.75);
        let result = linear_interpolation(0.0, 0.2, 0.0, 0.1, 0.05);
        assert_eq!(result, 0.025);
        // Test a linear extrapolation.
        let result = linear_interpolation(0.9, 0.95, 3.0, 8.0, 0.99);
        // Huh? Why is the numeric error so big?
        assert_almost_eq!(result, 12.0, 100.0 * f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_linear_interpolation_x1_is_x2() {
        // Return average of y1 and y2 for x1 = x2 = xp.
        let result = linear_interpolation(0.99, 0.99, 0.0, 2.0, 0.99);
        assert_eq!(result, 1.0);
        // Return y1 for xp < x1.
        let result = linear_interpolation(0.99, 0.99, 0.0, 2.0, 0.98);
        assert_eq!(result, 0.0);
        // Return y2 for xp > x2.
        let result = linear_interpolation(0.99, 0.99, 0.0, 2.0, 0.999);
        assert_eq!(result, 2.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_cubic_spline_interpolation() {
        // Recover linear interpolation.
        let result = cubic_spline_interpolation(&[0.0, 1.0, 2.0, 3.0], &[4.0, 5.0, 6.0, 7.0], 1.5);
        assert_eq!(result, 5.5);
        // Recover a quadratic interpolation.
        let result = cubic_spline_interpolation(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 4.0, 9.0], 1.5);
        assert_eq!(result, 2.25);
        // Recover a cubic interpolation.
        let result = cubic_spline_interpolation(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 8.0, 27.0], 1.5);
        assert_eq!(result, 3.375);
        // Complicate it -- y = x^3 + x^2 + x + 1.
        let result =
            cubic_spline_interpolation(&[1.0, 2.0, 3.0, 4.0], &[4.0, 15.0, 40.0, 85.0], 2.5);
        assert_eq!(result, 25.375);

        // There may be cases where there is no change in the x values (CDF values)
        // but the y values (time values) change. Make sure that the curve fitting doesn't panic
        // and we properly recover interpolation.
        // Because of how we find the `upper_window_index`, the second and third values will always be
        // different, but the first and second or third and fourth values may be the same.
        let result = cubic_spline_interpolation(&[1.0, 1.0, 2.0, 2.0], &[4.0, 5.0, 6.0, 7.0], 1.5);
        assert_eq!(result, 5.5);
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn test_cubic_spline_interpolation_out_of_bounds() {
        // Just to test that the function behaves like we expect.
        cubic_spline_interpolation(&[0.0, 1.0, 2.0, 3.0], &[4.0, 5.0, 6.0, 7.0], 0.5);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_interpolation_conditions() {
        // We want to make sure that our interpolate function calls
        // linear interpolation and cubic spline interpolation as we expect.
        // Linear interpolation for only two values
        let result = interpolate(&[0.0, 1.0], &[0.0, 1.0], 0.5);
        assert_eq!(result, 0.5);
        // Linear extrapolation for any values outside the domain.
        let result = interpolate(&[0.0, 0.0, 1.0, 2.0], &[0.0, 1.0, 1.0, 2.0], 5.0);
        assert_eq!(result, 5.0);
        // Cubic spline interpolation for any values inside the domain.
        let result = interpolate(&[0.0, 0.0, 1.0, 2.0], &[0.0, 1.0, 1.0, 2.0], 0.5);
        assert_ne!(result, 1.0);
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
        // But, because the interpolation routine still runs, we can't check for exact equality.
        assert_almost_eq!(
            context.evaluate_inverse_generation_interval(person_id, cdf(10.0 * dt)),
            10.0 * dt,
            f64::EPSILON
        );
    }
}
