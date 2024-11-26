use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default, define_rng,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId, PersonPropertyChangeEvent},
    random::ContextRandomExt,
};
use statrs::distribution::{ContinuousCDF, Exp, Poisson};

use crate::contact::QueryContacts;
use crate::parameters::Parameters;

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum InfectiousStatus {
    Susceptible,
    Infectious,
    Recovered,
}

define_rng!(TransmissionRng);

define_person_property_with_default!(
    InfectiousStatusType,
    InfectiousStatus,
    InfectiousStatus::Susceptible
);

fn evaluate_transmission(context: &mut Context, contact_id: PersonId, _transmittee_id: PersonId) {
    // for now, we assume all transmission events are sucessful
    // we have the contact_id as an argument to this fcn so that person-person pair transmission potential
    // based on interventions can be evaluated to determine whether this transmission event is actually successful
    if matches!(
        context.get_person_property(contact_id, InfectiousStatusType),
        InfectiousStatus::Susceptible
    ) {
        context.set_person_property(
            contact_id,
            InfectiousStatusType,
            InfectiousStatus::Infectious,
        );
    }
}

fn infection_attempt(
    context: &mut Context,
    transmitee_id: PersonId,
    gi: f64,
    num_infection_attempts_remaining: f64,
    next_infection_time_unif: f64,
) -> Result<(), IxaError> {
    // if the person still has people left to infect
    if num_infection_attempts_remaining >= 1.0 {
        // this is a method from a trait extention implemented in the contact manager
        // as long as this method returns a contact id, it can use any underlying sampling strategy
        let contact_id = context.get_contact(transmitee_id)?;
        // evaluate transmission is its own function because there will be
        // intervention-based logic there eventually
        // if there are no contacts, to infect, do nothing
        if let Some(contact_id) = contact_id {
            evaluate_transmission(context, contact_id, transmitee_id);
        }
        // schedule the subsequent infection attempt for this infected agent,
        // which happens at a greater value of the GI
        schedule_next_infection_attempt(
            context,
            transmitee_id,
            gi,
            num_infection_attempts_remaining - 1.0,
            next_infection_time_unif,
        );
    }
    // if no more infection attempts remaining, set the person to recovered
    else {
        context.set_person_property(
            transmitee_id,
            InfectiousStatusType,
            InfectiousStatus::Recovered,
        );
    }
    Ok(())
}

fn get_next_infection_time_unif(context: &mut Context, last_infection_time_unif: f64) -> f64 {
    // this is NOT order statistics
    // we are just playing around to make a point
    // we will update the math to be correct later
    context.sample_range(TransmissionRng, last_infection_time_unif..1.0)
}

fn get_next_infection_time_from_gi(
    _context: &mut Context,
    gi: f64,
    next_infection_time_unif: f64,
) -> f64 {
    // this will be properly stored later
    //let gi = context.get_data_container(generation_interval).unwrap();
    // the math here is wrong as well -- we are not implementing order statistics
    // there is no guarantee that these infection attempts are sequential draws from the GI distribution
    // this will be fixed with real math later
    Exp::new(1.0 / gi)
        .unwrap()
        .inverse_cdf(next_infection_time_unif)
}

fn schedule_next_infection_attempt(
    context: &mut Context,
    transmitee_id: PersonId,
    gi: f64,
    num_infection_attempts_remaining: f64,
    last_infection_time_unif: f64,
) {
    // get next infection attempt time
    let next_infection_time_unif = get_next_infection_time_unif(context, last_infection_time_unif);
    let next_infection_time_gi =
        get_next_infection_time_from_gi(context, gi, next_infection_time_unif);
    // schedule the infection attempt: this grabs a contact at the time of the infection event
    // to make sure the contacts are based on who is alive at that time and evaluates whether the
    // transmission event is successful
    context.add_plan(
        context.get_current_time() + next_infection_time_gi,
        move |context| {
            infection_attempt(
                context,
                transmitee_id,
                gi,
                num_infection_attempts_remaining,
                next_infection_time_unif,
            )
            .expect("Error finding contact in infection attempt");
        },
    );
}

fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatusType>,
    r_0: f64,
    gi: f64,
) {
    // is the person going from S --> I?
    // we don't care about the other cases here
    if matches!(event.previous, InfectiousStatus::Susceptible) {
        // get the number of infection attempts this person will have
        // ok to use unwrap here because we pass inputs (r_0) through a validator
        let num_infection_attempts =
            context.sample_distr(TransmissionRng, Poisson::new(r_0).unwrap());
        // start scheduling infection attempt events for this person
        // people who have num_infection_attempts = 0 are still passed through this logic so that
        // they are set to recovered
        schedule_next_infection_attempt(context, event.person_id, gi, num_infection_attempts, 0.0);
    }
}

pub fn init(context: &mut Context) {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    context.subscribe_to_event(
        move |context, event: PersonPropertyChangeEvent<InfectiousStatusType>| {
            handle_infectious_status_change(
                context,
                event,
                parameters.r_0,
                parameters.generation_interval,
            );
        },
    );
}

#[cfg(test)]
mod test {
    use crate::{parameters::ParametersValues, population_loader::Alive};

    use super::*;
    use ixa::{context::Context, random::ContextRandomExt};

    fn set_up(r_0: f64) -> Context {
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
        // set a super small r_0 so that the person has a very low probability of infecting others
        let mut context = set_up(0.000_000_000_000_000_01);
        init(&mut context);
        let person_id = context.add_person(()).unwrap();

        context.set_person_property(
            person_id,
            InfectiousStatusType,
            InfectiousStatus::Infectious,
        );
        context.execute();

        assert_eq!(
            context.get_person_property(person_id, InfectiousStatusType),
            InfectiousStatus::Recovered
        );
    }

    #[test]
    fn test_infection_attempts() {
        // use a super big r_0 so that the probability of the person having
        // zero secondary infections is extremely low
        let mut context = set_up(50.0);
        init(&mut context);
        let person_id = context.add_person(()).unwrap();
        let contact = context.add_person(()).unwrap();
        // set person to infectious
        context.set_person_property(
            person_id,
            InfectiousStatusType,
            InfectiousStatus::Infectious,
        );
        // let person infect others
        context.execute();
        // check that the person is now recovered
        assert!(context.get_current_time() > 0.0);
        assert_eq!(
            context.get_person_property(person_id, InfectiousStatusType),
            InfectiousStatus::Recovered
        );
        // check that the person is now recovered
        assert_eq!(
            context.get_person_property(contact, InfectiousStatusType),
            InfectiousStatus::Recovered
        );
    }

    fn variable_number_of_infection_attempts(n_attempts: f64, n_iter: i32) {
        // infection attempt times are drawn from an exponential distribution
        // if we fix the number of infection attempts, we can calculate the distribution of
        // where the infection attempt times should be -- a time we can get because the simulation
        // ends at the end of the last infection attempt (agent becomes recovered)
        // in our current naive implementation, we model the mean last infection
        // attempt as occuring at inverse_cdf(1 - (1 / 2) ^ n) where n is the number of infection attempts
        let mut sum_end_times = 0.0;
        // r_0 not in this test is meaningless
        let context = set_up(1.0);
        let params = context
            .get_global_property_value(Parameters)
            .unwrap()
            .clone();
        for seed in 0..n_iter {
            let mut context = set_up(1.0);
            context.init_random(seed.try_into().unwrap());
            let transmitee_id = context.add_person(()).unwrap();
            // so we can get a contact, but the contact is a dummy
            let only_contact = context.add_person((Alive, false)).unwrap();
            infection_attempt(
                &mut context,
                transmitee_id,
                params.generation_interval,
                n_attempts,
                0.0,
            )
            .unwrap();
            context.execute();
            sum_end_times += context.get_current_time();
            assert_eq!(
                context.get_person_property(transmitee_id, InfectiousStatusType),
                InfectiousStatus::Recovered
            );
            assert_eq!(
                context.get_person_property(only_contact, InfectiousStatusType),
                InfectiousStatus::Susceptible
            );
        }
        // TODO: figure out way to check math for n_attempts > 1
        let expected_time_elapsed = f64::powf(params.generation_interval, n_attempts);

        assert!(((sum_end_times / f64::from(n_iter)) - expected_time_elapsed).abs() < 0.1);
    }

    #[test]
    fn test_variable_number_of_infection_attempts() {
        variable_number_of_infection_attempts(1.0, 1000);
    }
}
