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
    // this is a method from a trait extention implemented in the contact manager
    // we need to provide the transmitee id to ensure we do not just randomly sample that
    // an agent infects themselves
    // as long as this method returns a contact id, it can use any sampling strategy
    // or logic to get there
    let contact_id = context.get_contact(transmitee_id)?;
    // evaluate transmission is its own function because there will be logic there eventually
    // surrounding interventions
    // if there are no contacts, to infect, do nothing
    if let Some(contact_id) = contact_id {
        evaluate_transmission(context, contact_id, transmitee_id);
    }
    // schedule the subsequent infection attempt for this infected agent,
    // which happens at a greater value of the GI
    if num_infection_attempts_remaining > 1.0 {
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
    use super::*;
    use ixa::{context::Context, people::PersonPropertyChangeEvent, random::ContextRandomExt};

    #[test]
    fn test_person_with_no_infection_attempts() {
        let mut context = Context::new();
        let person_id = context.add_person(()).unwrap();
        let gi = 5.0;
        let num_infection_attempts_remaining = 0.0;
        let next_infection_time_unif = 0.5;

        infection_attempt(
            &mut context,
            person_id,
            gi,
            num_infection_attempts_remaining,
            next_infection_time_unif,
        )
        .unwrap();

        assert_eq!(
            context.get_person_property(person_id, InfectiousStatusType),
            InfectiousStatus::Recovered
        );
    }

    #[test]
    fn test_transition() {
        let mut context = Context::new();
        let person_id = context.add_person(()).unwrap();
        let gi = 5.0;
        let num_infection_attempts_remaining = 3.0;
        let next_infection_time_unif = 0.5;

        infection_attempt(
            &mut context,
            person_id,
            gi,
            num_infection_attempts_remaining,
            next_infection_time_unif,
        )
        .unwrap();

        assert_eq!(
            context.get_person_property(person_id, InfectiousStatusType),
            InfectiousStatus::Infectious
        );
    }

    #[test]
    fn test_handle_infectious_status_change() {
        let mut context = Context::new();
        let person_id = context.add_person(()).unwrap();
        let event = PersonPropertyChangeEvent {
            person_id,
            previous: InfectiousStatus::Susceptible,
            current: InfectiousStatus::Infectious,
        };
        let r_0 = 2.0;
        let gi = 5.0;

        handle_infectious_status_change(&mut context, event, r_0, gi);

        // Check if the person has scheduled infection attempts
        let num_infection_attempts =
            context.sample_distr(TransmissionRng, Poisson::new(r_0).unwrap());
        assert!(num_infection_attempts > 0.0);
    }
}
