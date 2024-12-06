use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default, define_rng,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId, PersonPropertyChangeEvent},
    random::ContextRandomExt,
};
use statrs::distribution::{ContinuousCDF, Exp, Poisson};

use crate::parameters::Parameters;
use crate::{contact::ContextContactExt, population_loader::Alive};

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

pub fn init(context: &mut Context) {
    // Watch for changes in the InfectiousStatusType property.
    context.subscribe_to_event(
        move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
            handle_infectious_status_change(context, event);
        },
    );
    context.add_plan(0.0, |context| {
        seed_infections(context);
    });
}

/// This function seeds the initial infections in the population.
fn seed_infections(context: &mut Context) {
    // For now, we just pick a random person and make them infectious.
    // In the future, we may pick people based on specific person properties.
    let alive_people = context.query_people((Alive, true));
    let random_person_id = context.sample_range(TransmissionRng, 0..alive_people.len());
    context.set_person_property(
        alive_people[random_person_id],
        InfectiousStatus,
        InfectiousStatusType::Infectious,
    );
}

fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    // Is the person going from S --> I?
    // We don't care about the other transitions here, but this function will still be triggered
    // because it watches for any change in InfectiousStatusType.
    if event.current == InfectiousStatusType::Infectious {
        assert_eq!(event.previous, InfectiousStatusType::Susceptible);
        // Get the number of infection attempts this person will have.
        let r_0 = context.get_global_property_value(Parameters).unwrap().r_0;
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let num_infection_attempts =
            context.sample_distr(TransmissionRng, Poisson::new(r_0).unwrap()) as usize;

        // Start scheduling infection attempt events for this person.
        // People who have num_infection_attempts = 0 are still passed through this
        // logic but don't infect anyone. This is so that so that there is only one
        // place where we need to handle setting people to recovered.
        schedule_next_infection_attempt(
            context,
            event.person_id,
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

    assert!(next_infection_time_unif > last_infection_time_uniform);

    (
        next_infection_time_unif,
        gi_inverse_cdf(context, next_infection_time_unif)
            - gi_inverse_cdf(context, last_infection_time_uniform),
    )
}

/// The inverse CDF of the generation interval distribution.
/// This function is used to calculate the time until the next infection attempt.
fn gi_inverse_cdf(context: &Context, uniform_draw: f64) -> f64 {
    let gi = context
        .get_global_property_value(Parameters)
        .unwrap()
        .generation_interval;
    // In the future, we will generalize the use of an exponential distribution to
    // an arbitrary distribution with a defined inverse CDF.
    Exp::new(1.0 / gi).unwrap().inverse_cdf(uniform_draw)
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
        parameters::{Parameters, ParametersValues},
        population_loader::Alive,
        transmission_manager::schedule_next_infection_attempt,
    };

    use super::{init, InfectiousStatus, InfectiousStatusType};
    use ixa::{
        context::Context, define_data_plugin, global_properties::ContextGlobalPropertiesExt,
        people::ContextPeopleExt, random::ContextRandomExt, PersonId, PersonPropertyChangeEvent,
    };
    use statrs::distribution::{ContinuousCDF, Exp};

    fn setup(r_0: f64) -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0,
            infection_duration: 0.1,
            generation_interval: 3.0,
            report_period: 1.0,
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

    define_data_plugin!(RecordedInfectionTimes, Vec<f64>, Vec::<f64>::new());

    #[test]
    fn test_kolmogorov_smirnov_reproduce_gi() {
        // We would like to test that our use of order statistics
        // has not changed the distribution of the generation interval.
        // We do this by recording the infection times and then comparing to
        // the generation interval distribution. We use the Kolmogorov-Smirnov
        // test (KS test) to compare the two distributions the infection times
        // (empirical) distribution and the generation interval (theoretical)
        // distribution.

        // We need to get some parameters out of context for comparison in the KS test.
        // Initialize `gi`. It will always be set by the proper value from context
        // in the for loop below.
        let mut gi = 0.0;
        // We want an infected person to infect others, and we want to record those
        // infection times. We need to do this multiple times with different seeds
        // to get a good estimate of the distribution of infection times.
        let n: u32 = 2000;
        let infection_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        for seed in 0..n {
            let mut context = setup(5.0);
            if seed == 0 {
                // Get the parameters we need out of context.
                gi = context
                    .get_global_property_value(Parameters)
                    .unwrap()
                    .generation_interval;
            }
            context.init_random(seed.into());
            // We only need two people in the population: a transmitter and a contact.
            // This is because we set the person who becomes infected back to susceptible
            // after every infection attempt below in our record_infection_times function.
            // This lets the transmitter repetively infect this person.
            let transmitter_id = context.add_person(()).unwrap();
            // We add the contact to the context _after_ we choose the transmitter (happens in init).
            init(&mut context);
            let infection_times_clone = Rc::clone(&infection_times);
            context.subscribe_to_event({
                move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
                    record_infection_times(context, event, transmitter_id, &infection_times_clone);
                }
            });
            context.add_plan(0.0, move |context| {
                context.add_person(()).unwrap();
            });
            // Finally, we need one more helper function. The contact experiences a full infectious course.
            // Even though we are setting their `InfectiousStatus` back to susceptible, there is an event
            // generated when their status changes to infectious. This event triggers the rest of the infectious
            // course, which ends in the person being labeled as recovered. If they are labeled as recovered,
            // they cannot get infected and we do not record their infection time again. So, we need to revert
            // recovery.
            context.subscribe_to_event(
                move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
                    if event.current == InfectiousStatusType::Recovered
                        && event.person_id != transmitter_id
                    {
                        context.set_person_property(
                            event.person_id,
                            InfectiousStatus,
                            InfectiousStatusType::Susceptible,
                        );
                    }
                },
            );
            context.execute();
        }

        check_ks_stat(&mut infection_times.borrow_mut(), |x| {
            Exp::new(1.0 / gi).unwrap().cdf(x)
        });
    }

    fn record_infection_times(
        context: &mut Context,
        event: PersonPropertyChangeEvent<InfectiousStatus>,
        transmitter_id: PersonId,
        infection_times: &Rc<RefCell<Vec<f64>>>,
    ) {
        // We only want to track the infection time if the agent is getting infected.
        // We also want to make sure that we are tracking the contact, not the contact potentially
        // infecting the transmitter.
        if event.current == InfectiousStatusType::Infectious && event.person_id != transmitter_id {
            let current_time = context.get_current_time();
            infection_times.borrow_mut().push(current_time);
            // However, we need to make sure this person stays a potential infectious contact.
            // If this person becomes infectious, we cannot guarantee that they will be infected
            // again at the next infectious time (for instance, they may have immunity).
            // So, if we try to infect them again, nothing will happen and we won't observe the
            // infection time. This is more likely to happen as there are more infections, so this
            // would negatively bias our estimates of the GI.
            context.set_person_property(
                event.person_id,
                InfectiousStatus,
                InfectiousStatusType::Susceptible,
            );
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn infection_attempts_end_time(n_attempts: usize, n_iter: u32) {
        // Using some math, we can test that the observed time of the end of the simulation
        // is what we expect it to be given the number of infection attempts.
        // Concretely, with more infection attempts, we expect the simulation to end later.
        // In uniform space, the last infection time occurs at Beta(n_attempts, 1).
        // We can observe the end time for many iterations of n_attempts infections and compare
        // using the KS test.

        let mut end_times = Vec::<f64>::new();
        // In this test, the value of r_0 used to set up context is meaningless because we manually
        // set the number of infection attempts below.
        let context = setup(1.0);
        let params = context
            .get_global_property_value(Parameters)
            .unwrap()
            .clone();
        for seed in 0..n_iter {
            let mut context = setup(1.0);
            context.init_random(seed.into());
            let transmitter_id = context.add_person(()).unwrap();
            // Create a person who will be the only contact, but have them be dead so they can't be infected.
            // Instead, `get_contact` will return None.
            let only_contact = context.add_person((Alive, false)).unwrap();
            // Schedule the infection attempts.
            schedule_next_infection_attempt(&mut context, transmitter_id, n_attempts, 0.0);
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
        check_ks_stat(&mut end_times, |x| {
            let gi_cdf = Exp::new(1.0 / params.generation_interval).unwrap().cdf(x);
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
