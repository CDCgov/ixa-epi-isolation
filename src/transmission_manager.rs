use ixa::{
    define_person_property, define_person_property_with_default, define_rng, Context,
    ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
    PersonPropertyChangeEvent,
};
use statrs::distribution::Poisson;

use crate::{contact::ContextContactExt, parameters::Parameters, population_loader::Alive};

// Define the possible infectious statuses for a person.
// These states refer to the person's infectiousness at a given time
// and are not related to the person's health status. How long an agent
// spends in the infectious compartment is determined entirely from their
// number of infection attempts and draws from the generation interval.
#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum InfectiousStatusType {
    Susceptible,
    Infectious,
    Recovered,
}

define_rng!(TransmissionRng);

define_person_property_with_default!(
    InfectiousStatus,
    InfectiousStatusType,
    InfectiousStatusType::Susceptible
);

/// Seeds initial infections at t = 0, and subscribes to
/// people becoming infectious to schedule their infection attempts.
pub fn init(context: &mut Context) {
    // Watch for changes in the InfectiousStatusType property.
    context.subscribe_to_event(
        move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
            handle_infectious_status_change(context, event);
        },
    );
    context.add_plan(0.0, |context| {
        seed_infections(context).expect("Unable to seed infections");
    });
}

/// This function seeds the initial infections in the population.
fn seed_infections(context: &mut Context) -> Result<(), IxaError> {
    // For now, we just pick a random person and make them infectious.
    // In the future, we may pick people based on specific person properties.
    let random_person_id = context.sample_person(TransmissionRng, (Alive, true))?;
    context.set_person_property(
        random_person_id,
        InfectiousStatus,
        InfectiousStatusType::Infectious,
    );
    Ok(())
}

// Called when a person's infectious status changes. Only considers people becoming infectious,
// and starts the process of scheduling their infection attempts sequentially.
fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    // Is the person going from S --> I?
    // We don't care about the other transitions here, but this function will still be triggered
    // because it watches for any change in InfectiousStatusType.
    if event.current == InfectiousStatusType::Infectious {
        // Get the number of infection attempts this person will have.
        let r_0 = context.get_global_property_value(Parameters).unwrap().r_0;
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let num_infection_attempts =
            context.sample_distr(TransmissionRng, Poisson::new(r_0).unwrap()) as usize;

        // Get generation interval parameters.
        let gi_params = context.sample_natural_history(event.person_id);
        // Start scheduling infection attempt events for this person.
        // People who have num_infection_attempts = 0 are still passed through this
        // logic but don't infect anyone. This is so that so that there is only one
        // place where we need to handle setting people to recovered.
        schedule_next_infection_attempt(
            context,
            event.person_id,
            gi_params,
            num_infection_attempts,
            // Since the agent has had no infection attempts yet,
            // they have had 0.0 of their infectious time pass.
            0.0,
        );
    }
}

/// Schedule the next infection attempt for the transmitter based on the generation
/// interval and the number of infection attempts remaining.
fn schedule_next_infection_attempt(
    context: &mut Context,
    transmitter_id: PersonId,
    gi_params: TriVLParams,
    num_infection_attempts_remaining: usize,
    last_infection_time_uniform: f64,
) {
    // If there are no more infection attempts remaining, set the person to recovered.
    if num_infection_attempts_remaining == 0 {
        context.set_person_property(
            transmitter_id,
            InfectiousStatus,
            InfectiousStatusType::Recovered,
        );
        return;
    }

    // Get the next infection attempt time.
    let (next_infection_time_unif, time_until_next_infection_attempt_gi) = get_next_infection_time(
        context,
        num_infection_attempts_remaining,
        last_infection_time_uniform,
        &gi_params,
    );

    // Schedule the infection attempt. The function `infection_attempt` (a) grabs a contact at
    // the time of the infection event to ensure that the contacts are current and (b) evaluates
    // whether the transmission event is successful.
    // After that, schedule the next infection attempt for this infected agent with
    // one fewer infection attempt remaining.
    context.add_plan(
        context.get_current_time() + time_until_next_infection_attempt_gi,
        move |context| {
            infection_attempt(context, transmitter_id)
                .expect("Error finding contact in infection attempt");
            // Schedule the next infection attempt for this infected agent
            // once the last infection attempt is over.
            schedule_next_infection_attempt(
                context,
                transmitter_id,
                gi_params,
                num_infection_attempts_remaining - 1,
                next_infection_time_unif,
            );
        },
    );
}

/// Calculate the next infection time. Draws an increasing value of a uniform distribution
/// and passes it through the inverse CDF of the generation interval to get the next infection time.
fn get_next_infection_time(
    context: &mut Context,
    num_infection_attempts_remaining: usize,
    last_infection_time_uniform: f64,
    gi_params: &TriVLParams,
) -> (f64, f64) {
    // Draw the next uniform infection time using order statistics.
    // The first of n draws from U(0, 1) comes from Beta(1, n), so we pass a uniform
    // draw through the inverse CDF of Beta(1, n) to get the minimum time.
    #[allow(clippy::cast_precision_loss)]
    let mut next_infection_time_unif = 1.0
        - f64::powf(
            context.sample_range(TransmissionRng, 0.0..1.0),
            1.0 / num_infection_attempts_remaining as f64,
        );
    // We scale the uniform draw to be on the interval (last_infection_time_uniform, 1)
    // so that the next infection time is always greater than the last infection time.
    next_infection_time_unif = last_infection_time_uniform
        + next_infection_time_unif * (1.0 - last_infection_time_uniform);

    (
        next_infection_time_unif,
        tri_vl_gi_inverse_cdf(next_infection_time_unif, gi_params)
            - tri_vl_gi_inverse_cdf(last_infection_time_uniform, gi_params),
    )
}

/// The inverse CDF of the triangle VL curve defined in Kissler et al., (2023) Nat Comms.
/// The triangle VL curve is a piecewise linear function that increases linearly
/// over the "proliferation time" from 0 to a peak value and then decreases linearly
/// back to 0 over the "clearance time". The peak value occurs at a peak time
/// relative to the time of symptom onset. Although the triangle VL curve has range
/// from (-\infty, `peak_magnitude`], we assume that an individual has no infectiousness
/// if the curve is below 0.
fn tri_vl_gi_inverse_cdf(uniform_draw: f64, gi_params: &TriVLParams) -> f64 {
    // The CDF is 0 on the interval (0, infection_start_time]. Rather than returning the
    // infection_start_time when the uniform_draw is 0, we must return 0.0. This is because we only
    // ever call tri_vl_gi_inverse_cdf(time + dt) - tri_vl_gi_inverse_cdf(time) since we are
    // interested in calculating the time until the next infection attempt. But, if we returned
    // infection_start_time for time = 0, for dt --> 0, we would return a value --> 0. This means
    // that there is no delay to the start of the infectious period. Instead, if we return 0.0
    // for time = 0.0, for time + dt, we will return approximately infection_start_time + d(gi).
    // For all subsequent calls of time != 0.0, tri_vl_gi_inverse_cdf(time + dt) - tri_vl_gi_inverse_cdf(time)
    // correctly gives the time until the next infection attempt.
    if uniform_draw == 0.0 {
        return 0.0;
    }
    // I calculate the start and end times of the infectious period based on when the triangle VL
    // curve is above zero.
    let iso_guid_params = &gi_params.iso_guid_params;
    let infection_start_time = iso_guid_params.peak_time - iso_guid_params.proliferation_time
        + gi_params.symptom_onset_time;
    let infection_end_time =
        iso_guid_params.peak_time + iso_guid_params.clearance_time + gi_params.symptom_onset_time;

    // I calculate the CDF using geometric rules of a triangle's area because it is easier for me
    // to visualize what is going on rather than manually calculating the integral of the triangle VL
    // curve and then inverting it.
    let triangle_area =
        0.5 * iso_guid_params.peak_magnitude * (infection_end_time - infection_start_time);
    // CDFs must have unit integral, so because we are inverting the CDF, we multiply
    // the uniform draw by the area of the triangle that we would have used to normalize
    // its integral into a CDF.
    let cdf_absolute_space = uniform_draw * triangle_area;
    // Since the triangle VL curve is piecewise linear, there are two different expressions
    // for the area depending on whether we are on the left or right side of the peak.
    let left_half_area = 0.5 * iso_guid_params.peak_magnitude * iso_guid_params.proliferation_time;
    if cdf_absolute_space <= left_half_area {
        // We calculate the base of a triangle (i.e., calculating the $t$ value that lead to the
        // observed CDF value) with area given by the CDF value assuming that the triangle starts
        // at the infection_start_time and extends left towards the peak. We can calculate the
        // height of the triangle by knowing that the triangle VL curve increases linearly with
        // slope = peak_magnitude / proliferation_time. This gives us the expression
        // A = 0.5 * t * t * peak_magnitude / proliferation_time. We solve for t:
        let extra_time = f64::sqrt(
            cdf_absolute_space * 2.0 * iso_guid_params.proliferation_time
                / iso_guid_params.peak_magnitude,
        );
        infection_start_time + extra_time
    } else {
        // To make it possible to still use our triangle area approach, we consider the area remaining
        // in the entire triangle VL curve. We want to calculate the base of that triangle because it is the time
        // remaining until the end of the infectious period. We use the same approach as above but know
        // that the slope of the decrease is peak_magnitude / clearance_time.
        let time_until_end = f64::sqrt(
            (triangle_area - cdf_absolute_space) * 2.0 * iso_guid_params.clearance_time
                / iso_guid_params.peak_magnitude,
        );
        infection_end_time - time_until_end
    }
}

/// This function is called when an infected agent has an infection event scheduled.
/// This function includes identifying a potential contact to infect, evaluating whether
/// the transmission event is successful, and scheduling the next infection event.
/// The method to identify a contact is from the trait extension `ContextContactExt`,
/// and as long as it returns a valid contact id, it can use any sampling strategy.
/// In other words, this code does not depend on the sampling logic, but it also doesn't
/// check any characteristics of the contact.
/// Errors
/// - If there is only one person in the population.
fn infection_attempt(context: &mut Context, transmitter_id: PersonId) -> Result<(), IxaError> {
    // `get_contact`? returns Option<PersonId>. If the option is None, there are no valid
    // contacts to infect, so do nothing.
    if let Some(contact_id) = context.get_contact(transmitter_id)? {
        // We evaluate transmission in its own function because there will be eventually
        // be intervention-based logic that determines whether a transmission event is successful.
        evaluate_transmission(context, contact_id, transmitter_id);
    }

    Ok(())
}

/// Evaluates whether a transmission event is successful based on the characteristics
/// of the transmitter and the contact. For now, we assume all transmission events are sucessful.
/// In the future, the success of a transmission event may depend on person-level interventions,
/// such as whether either agent is wearing a mask. For this reason, we pass the transmitter as well.
fn evaluate_transmission(context: &mut Context, contact_id: PersonId, _transmitter_id: PersonId) {
    if context.get_person_property(contact_id, InfectiousStatus)
        == InfectiousStatusType::Susceptible
    {
        context.set_person_property(
            contact_id,
            InfectiousStatus,
            InfectiousStatusType::Infectious,
        );
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, path::PathBuf, rc::Rc};

    use crate::{
        parameters::{ContextParametersExt, IsolationGuidanceParams, Parameters, ParametersValues},
        population_loader::Alive,
        transmission_manager::{
            schedule_next_infection_attempt, tri_vl_gi_inverse_cdf, TriVLParams,
        },
    };

    use super::{init, InfectiousStatus, InfectiousStatusType};
    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, PersonId,
        PersonPropertyChangeEvent,
    };

    fn setup(r_0: f64) -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0,
            incubation_period_shape: 1.5,
            incubation_period_scale: 3.6,
            growth_rate_incubation_period: 0.15,
            report_period: 1.0,
            tri_vl_params_file: PathBuf::from("./tests/data/tri_vl_params.csv"),
            synth_population_file: PathBuf::from("."),
        };
        let mut context = Context::new();
        context.init_random(params.seed);
        context
            .set_global_property_value(Parameters, params)
            .unwrap();
        context
    }

    #[test]
    fn test_transitions() {
        // Use a small r_0 so that the person has a low probability of infecting others,
        // and we do not trigger the `get_contact` function -- which errors out in the case
        // of a population size of 1.
        let mut context = setup(0.000_000_000_000_000_01);
        init(&mut context);
        let person_id = context.add_person(()).unwrap();

        context.execute();

        assert_eq!(
            context.get_person_property(person_id, InfectiousStatus),
            InfectiousStatusType::Recovered
        );
    }

    #[test]
    fn test_infection_attempts() {
        // Use a big r_0, so that the probability of the person having
        // zero secondary infections is extremely low.
        // This lets us check that the other person in the population is infected.
        let mut context = setup(50.0);
        init(&mut context);
        let person_id = context.add_person(()).unwrap();
        let contact = context.add_person(()).unwrap();

        // Set person to infectious, triggering the event watcher set up in the init function.
        context.set_person_property(
            person_id,
            InfectiousStatus,
            InfectiousStatusType::Infectious,
        );

        // Execute the model logic.
        context.execute();

        // Check that the transmitter is now recovered.
        assert!(context.get_current_time() > 0.0);
        assert_eq!(
            context.get_person_property(person_id, InfectiousStatus),
            InfectiousStatusType::Recovered
        );

        // Check that their contact is also recovered.
        assert_eq!(
            context.get_person_property(contact, InfectiousStatus),
            InfectiousStatusType::Recovered
        );
    }

    fn tri_vl_gi_cdf(time: f64, gi_params: &TriVLParams) -> f64 {
        let iso_guid_params = &gi_params.iso_guid_params;
        // We use the same triangle area based approach to calculate the CDF of the GI.
        // We calculate the total area because we must normalize the CDF to have unit integral.
        let tri_area = 0.5
            * iso_guid_params.peak_magnitude
            * (iso_guid_params.clearance_time + iso_guid_params.proliferation_time);
        // Depending on whether the time is before or after the peak, we calculate the area differently.
        let gi_cdf = if time < iso_guid_params.peak_time + gi_params.symptom_onset_time {
            // Calculate the time from the triangle starting point to the current time.
            let extra_time = time - iso_guid_params.peak_time + iso_guid_params.proliferation_time
                - gi_params.symptom_onset_time;
            // Calculate the area of the triangle that would be formed by having a base equal to the extra time
            // and the corresponding height along the triangle VL curve.
            0.5 * extra_time * extra_time * iso_guid_params.peak_magnitude
                / iso_guid_params.proliferation_time
        } else {
            // Calculate the time remaining to the end of the infectious period (triangle crosses below 0 again).
            let extra_time = iso_guid_params.peak_time
                + gi_params.symptom_onset_time
                + iso_guid_params.clearance_time
                - time;
            // Calculate the residual area of a triangle with base equal to the extra time and the corresponding height.
            tri_area
                - 0.5 * extra_time * extra_time * iso_guid_params.peak_magnitude
                    / iso_guid_params.clearance_time
        };
        gi_cdf / tri_area
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_tri_vl_inverse_cdf() {
        let gi_params = TriVLParams {
            iso_guid_params: IsolationGuidanceParams {
                peak_time: 1.2,
                peak_magnitude: 8.0,
                proliferation_time: 1.0,
                clearance_time: 7.0,
                symptom_improvement_time: 6.0,
            },
            symptom_onset_time: 5.0,
        };
        let iso_guid_params = &gi_params.iso_guid_params;
        // Let's check some times to make sure that the inverse CDF is correct.
        assert_eq!(tri_vl_gi_inverse_cdf(0.0, &gi_params), 0.0);
        // Small values are always greater than the infection start time.
        assert!(
            tri_vl_gi_inverse_cdf(1e-12, &gi_params)
                > iso_guid_params.peak_time - iso_guid_params.proliferation_time
                    + gi_params.symptom_onset_time
        );
        // Infections end at the infection end time.
        assert_eq!(
            tri_vl_gi_inverse_cdf(1.0, &gi_params),
            iso_guid_params.peak_time
                + iso_guid_params.clearance_time
                + gi_params.symptom_onset_time
        );
        // Inverse CDF of the CDF should give the identity function.
        // There seems to be some numerical error here.
        assert!(
            (tri_vl_gi_cdf(tri_vl_gi_inverse_cdf(0.5, &gi_params), &gi_params) - 0.5).abs()
                < f64::EPSILON
        );
        assert!(
            (tri_vl_gi_cdf(tri_vl_gi_inverse_cdf(0.25, &gi_params), &gi_params) - 0.25).abs()
                < f64::EPSILON
        );
        assert!(
            (tri_vl_gi_cdf(tri_vl_gi_inverse_cdf(0.75, &gi_params), &gi_params) - 0.75).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn test_kolmogorov_smirnov_reproduce_gi() {
        // We would like to test that our use of order statistics
        // has not changed the distribution of the generation interval.
        // We do this by recording the infection times and then comparing to
        // the generation interval distribution. We use the Kolmogorov-Smirnov
        // test (KS test) to compare the two distributions the infection times
        // (empirical) distribution and the generation interval (theoretical)
        // distribution.

        // We want an infected person to infect others, and we want to record those
        // infection times. We need to do this multiple times with different seeds
        // to get a good estimate of the distribution of infection times.
        let n: u32 = 2000;
        let infection_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let mut gi_params: Option<TriVLParams> = None;
        for seed in 0..n {
            let mut context = setup(5.0);
            // We only need two people in the population: a transmitter and a contact.
            // This is because we set the person who becomes infected back to susceptible
            // after every infection attempt below in our record_infection_times function.
            // This lets the transmitter repetively infect this person.
            let transmitter_id = context.add_person(()).unwrap();
            // We add the contact to the context _after_ we choose the transmitter (happens in init).
            // By having only one person in the population when we choose the transmitter, we guarantee
            // that that one person is the transmitter. This lets us keep the transmitter id, which we
            // need below.

            // Initially, we set the seed to be the same across all simulations. Later, we
            // change the seed to be different across runs. Since symptom onset time is also randomly chosen,
            // to ensure we can compare times across runs, we need to have the same random number generator
            // across simulations. We manually call `context.sample_natural_history` which sets the parameters
            // for a given person, and then we can set the seed back to vary across runs.
            context.init_random(108);
            gi_params = Some(context.sample_natural_history(transmitter_id));
            context.init_random(seed.into());
            init(&mut context);
            let infection_times_clone = Rc::clone(&infection_times);
            context.subscribe_to_event({
                move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
                    record_infection_times(context, event, transmitter_id, &infection_times_clone);
                }
            });
            context.add_plan(0.0, |context| {
                context.add_person(()).unwrap();
            });
            context.execute();
        }
        let gi_params = gi_params.unwrap();
        check_ks_stat(&mut infection_times.borrow_mut(), |time| {
            tri_vl_gi_cdf(time, &gi_params)
        });
    }

    fn record_infection_times(
        context: &mut Context,
        event: PersonPropertyChangeEvent<InfectiousStatus>,
        transmitter_id: PersonId,
        infection_times: &Rc<RefCell<Vec<f64>>>,
    ) {
        // We want to make sure that we are tracking the contact, not the contact potentially
        // infecting the transmitter.
        if event.person_id != transmitter_id {
            // We only want to track the infection time if the agent is getting infected.
            if event.current == InfectiousStatusType::Infectious {
                let current_time = context.get_current_time();
                infection_times.borrow_mut().push(current_time);
            }
            // However, we need to make sure this person stays a potential infectious contact.
            if event.current != event.previous {
                context.set_person_property(
                    event.person_id,
                    InfectiousStatus,
                    InfectiousStatusType::Susceptible,
                );
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn infection_attempts_end_time(n_attempts: usize, n_iter: u32) {
        // We can test that the observed time of the end of the simulation
        // is what we expect it to be given the number of infection attempts.
        // Concretely, with more infection attempts, we expect the simulation to end later.
        // In uniform space, the last infection time occurs at Beta(n_attempts, 1).
        // We can observe the end time for many iterations of n_attempts infections and compare
        // using the KS test.

        let mut end_times = Vec::<f64>::new();
        let gi_params = TriVLParams {
            iso_guid_params: IsolationGuidanceParams {
                peak_time: 1.2,
                peak_magnitude: 8.0,
                proliferation_time: 1.0,
                clearance_time: 7.0,
                symptom_improvement_time: 6.0,
            },
            symptom_onset_time: 5.0,
        };
        for seed in 0..n_iter {
            // In this test, the value of r_0 used to set up context is meaningless because we manually
            // set the number of infection attempts and GI params.
            let mut context = setup(1.0);
            context.init_random(seed.into());
            let transmitter_id = context.add_person(()).unwrap();
            // Create a person who will be the only contact, but have them be dead so they can't be infected.
            // Instead, `get_contact` will return None.
            let only_contact = context.add_person((Alive, false)).unwrap();
            // Schedule the infection attempts.
            schedule_next_infection_attempt(
                &mut context,
                transmitter_id,
                gi_params.clone(),
                n_attempts,
                0.0,
            );
            context.execute();
            end_times.push(context.get_current_time());
            assert_eq!(
                context.get_person_property(transmitter_id, InfectiousStatus),
                InfectiousStatusType::Recovered
            );
            assert_eq!(
                context.get_person_property(only_contact, InfectiousStatus),
                InfectiousStatusType::Susceptible
            );
        }

        // The theoretical CDF is calculated by taking the CDF of the GI
        // and passing that through the CDF of Beta(n_attempts, 1).
        check_ks_stat(&mut end_times, |time| {
            let gi_cdf = tri_vl_gi_cdf(time, &gi_params);
            // Inverse CDF of Beta(n_attempts, 1)
            f64::powf(gi_cdf, n_attempts as f64)
        });
    }

    fn check_ks_stat(times: &mut [f64], theoretical_cdf: impl Fn(f64) -> f64) {
        // Sort the empirical times to make an empirical CDF.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // KS stat is the maximum observed CDF deviation.
        let ks_stat = times
            .iter()
            .enumerate()
            .map(|(i, time)| {
                #[allow(clippy::cast_precision_loss)]
                let empirical_cdf_value = (i as f64) / (times.len() as f64);
                let theoretical_cdf_value = theoretical_cdf(*time);
                (empirical_cdf_value - theoretical_cdf_value).abs()
            })
            .reduce(f64::max)
            .unwrap();

        assert!(ks_stat < 0.01);
    }

    #[test]
    fn test_variable_number_of_infection_attempts() {
        infection_attempts_end_time(1, 10000);
        infection_attempts_end_time(2, 10000);
        infection_attempts_end_time(3, 10000);
    }
}
