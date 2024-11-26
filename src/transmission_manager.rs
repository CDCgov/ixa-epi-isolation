use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default, define_rng,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId, PersonPropertyChangeEvent},
    random::ContextRandomExt,
};
use statrs::distribution::{ContinuousCDF, Exp, Poisson};

use crate::contact::ContextContactExt;
use crate::parameters::Parameters;

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

/// This function evaluates whether a transmission event is successful based on the characteristics
/// of the transmitter and the contact. For now, we assume all transmission events are sucessful.
/// However, in the future, the success of a transmission event may depend on person-level interventions,
/// such as whether an agent is wearing a mask. For this reason, we pass the transmitter as well to this
/// function. This will allow us to evaluate the success of a transmission event based on the properties of
/// both the transmitter and the contact in the future.
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

/// This function is called when an infected agent has an infection event scheduled.
/// This function includes identifying a potential contact to infect, evaluating whether
/// the transmission event is successful, and scheduling the next infection event.
fn infection_attempt(
    context: &mut Context,
    transmitter_id: PersonId,
    num_infection_attempts_remaining: usize,
    infection_time_uniform: f64,
) -> Result<(), IxaError> {
    // This is a method from a trait extension implemented in `mod contact`.
    // As long as the method returns a contact id, it can use any underlying sampling strategy
    // to obtain that contact, and that strategy can be separately implemented in `mod contact`
    // without changing the logic here.
    let contact_id = context.get_contact(transmitter_id)?;

    // If there are no contacts to infect, do nothing.
    if let Some(contact_id) = contact_id {
        // We evaluate transmission in its own function because there will be eventually
        // be intervention-based logic that determines whether a transmission event is successful.
        evaluate_transmission(context, contact_id, transmitter_id);
    }

    // Schedule the next infection attempt for this infected agent.
    schedule_next_infection_attempt(
        context,
        transmitter_id,
        num_infection_attempts_remaining - 1,
        infection_time_uniform,
    );
    Ok(())
}

struct InfectionTimes {
    uniform: f64,
    gi: f64,
}

/// Calculate the next infection time. Draws an increasing value of a uniform distribution
/// and passes it through the inverse CDF of the generation interval to get the next infection time.
fn get_next_infection_time(
    context: &mut Context,
    last_infection_time_uniform: f64,
) -> InfectionTimes {
    // FOR NOW, we are using placeholder math to guarantee the infection times are always increasing.
    // This is not order statistics. This will be corrected at a later time.
    let next_infection_time_unif =
        context.sample_range(TransmissionRng, last_infection_time_uniform..1.0);

    // In the future, we will generalize the use of an exponential distribution to
    // an arbitrary distribution with a defined inverse CDF.
    let gi = context
        .get_global_property_value(Parameters)
        .unwrap()
        .generation_interval;
    let next_infection_time_gi = Exp::new(1.0 / gi)
        .unwrap()
        .inverse_cdf(next_infection_time_unif);

    InfectionTimes {
        uniform: next_infection_time_unif,
        gi: next_infection_time_gi,
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
    } else {
        // Schedule the next infection attempt.
        // Get the next infection attempt time.
        let next_infection_times = get_next_infection_time(context, last_infection_time_uniform);

        // Schedule the infection attempt. This function (a) grabs a contact at the time of the infection event
        // to ensure that the contacts are current and (b) evaluates whether the transmission event is successful.
        context.add_plan(
            context.get_current_time() + next_infection_times.gi,
            move |context| {
                infection_attempt(
                    context,
                    transmitter_id,
                    num_infection_attempts_remaining,
                    next_infection_times.uniform,
                )
                .expect("Error finding contact in infection attempt");
            },
        );
    }
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
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
            // last_infection_time_uniform is relative to when the agent became infectious.
            // Basically, it details the fraction of the agent's infectiousness that has passed.
            0.0,
        );
    }
}

pub fn init(context: &mut Context) {
    // Watch for changes in the InfectiousStatusType property.
    context.subscribe_to_event(
        move |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
            handle_infectious_status_change(context, event);
        },
    );
}

#[cfg(test)]
mod test {
    use crate::{parameters::Parameters, parameters::ParametersValues, population_loader::Alive};

    use super::{infection_attempt, init, InfectiousStatus, InfectiousStatusType};
    use ixa::{
        context::Context, global_properties::ContextGlobalPropertiesExt, people::ContextPeopleExt,
        random::ContextRandomExt,
    };

    fn setup(r_0: f64) -> Context {
        let p_values = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0,
            infection_duration: 0.1,
            generation_interval: 3.0,
            report_period: 1.0,
            synth_population_file: ".".to_string(),
        };
        let mut context = Context::new();
        context.init_random(p_values.seed);
        context.set_global_property_value(Parameters, p_values);
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

        context.set_person_property(
            person_id,
            InfectiousStatus,
            InfectiousStatusType::Infectious,
        );
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

    #[allow(clippy::cast_precision_loss)]
    fn variable_number_of_infection_attempts(n_attempts: usize, n_iter: i32) {
        // Using some math, we can test that the observed time of the end of the simulation
        // is what we expect it to be given the number of infection attempts.

        // In our current naive implementation, the expected end time of the simulation is
        // the sum of the exponential inverse_cdf evaluated at each draw of the uniform distribution.

        let mut sum_end_times = 0.0;
        // In this test, e value of r_0 used to set up context is meaningless because we manually
        // set the number of infection attempts below.
        let context = setup(1.0);
        let params = context
            .get_global_property_value(Parameters)
            .unwrap()
            .clone();
        for seed in 0..n_iter {
            let mut context = setup(1.0);
            context.init_random(seed.try_into().unwrap());
            let transmitter_id = context.add_person(()).unwrap();
            // Create a person who will be the only contact, but have them be dead so they can't be infected.
            // Instead, `get_contact` will return None.
            let only_contact = context.add_person((Alive, false)).unwrap();
            // Since we call infection_attempt directly, we need to add one to n_attempts. Concretely,
            // infection_attempt finds a contact and conducts an infection immediately, not scheduling a plan to do so.
            // Instead, the next infection attempt is scheduled on a plan. Since the goal of this test is to evaluate
            // the timing of when infection events occur, to test the timing of n_attempts infection events, we need to
            // call infection_attempt n_attempts + 1 times.
            infection_attempt(&mut context, transmitter_id, n_attempts + 1, 0.0).unwrap();
            context.execute();
            sum_end_times += context.get_current_time();
            assert_eq!(
                context.get_person_property(transmitter_id, InfectiousStatus),
                InfectiousStatusType::Recovered
            );
            assert_eq!(
                context.get_person_property(only_contact, InfectiousStatus),
                InfectiousStatusType::Susceptible
            );
        }

        // Expected time elapsed comes from the memorylessness of the exponential distribution.
        let expected_time_elapsed =
            params.generation_interval * (n_attempts as f64) * ((n_attempts as f64) + 1.0) / 2.0;

        println!(
            "Expected time elapsed: {}, Observed time elapsed: {}",
            expected_time_elapsed,
            sum_end_times / f64::from(n_iter)
        );

        assert!(((sum_end_times / f64::from(n_iter)) - expected_time_elapsed).abs() < 0.1);
    }

    #[test]
    fn test_variable_number_of_infection_attempts() {
        variable_number_of_infection_attempts(1, 1000);
        variable_number_of_infection_attempts(2, 1000);
        variable_number_of_infection_attempts(3, 5000);
    }
}
