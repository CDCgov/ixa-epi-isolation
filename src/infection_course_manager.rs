use ixa::{
    define_person_property, define_person_property_with_default, define_rng, Context,
    ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, PersonPropertyChangeEvent,
};
use statrs::distribution::Exp;

use crate::{
    parameters::{ContextParametersExt, Parameters},
    transmission_manager::{InfectiousStatus, InfectiousStatusType},
};

define_rng!(HealthStatusRng);

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum HealthStatusType {
    Healthy,
    Asymptomatic,
    Presymptomatic,
    Mild,
    Severe,
}

define_person_property_with_default!(HealthStatus, HealthStatusType, HealthStatusType::Healthy);

/// Watches for people becoming infected and updates their health status.
/// Functions independently from `InfectiousStatus`.
pub fn init(context: &mut Context) {
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectiousStatus>| {
            if event.current == InfectiousStatusType::Infectious {
                assert_eq!(event.previous, InfectiousStatusType::Susceptible);
                handle_infection_starting(context, event);
            } else if event.current == InfectiousStatusType::Recovered {
                assert_eq!(event.previous, InfectiousStatusType::Infectious);
                handle_infection_ending(context, event);
            }
        },
    );
    // We also need to watch for changes to health status to trigger when to plan the person's next health status change.
    context.subscribe_to_event(|context, event: PersonPropertyChangeEvent<HealthStatus>| {
        // We only care about the cases of asymptomatic to mild, and mild to severe.
        if event.current == HealthStatusType::Mild {
            assert_eq!(event.previous, HealthStatusType::Presymptomatic);
            // Schedule the person to either recover or become severely symptomatic.
            handle_mild_symptoms(context, event);
        } else if event.current == HealthStatusType::Severe {
            assert_eq!(event.previous, HealthStatusType::Mild);
            // Schedule the person to recover from their severe symptoms
            handle_severe_symptoms(context, event);
        }
    });
}

/// Chooses whether the person never develops symptoms, or develops mild symptoms.
fn handle_infection_starting(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    // Determine whether the person ever develops symptoms
    if context.sample_bool(
        HealthStatusRng,
        context
            .get_global_property_value(Parameters)
            .unwrap()
            .asymptomatic_probability,
    ) {
        context.set_person_property(
            event.person_id,
            HealthStatus,
            HealthStatusType::Asymptomatic,
        );
    } else {
        context.set_person_property(
            event.person_id,
            HealthStatus,
            HealthStatusType::Presymptomatic,
        );
        // Schedule the person to become mildly symptomatic at some point in the future.
        // Grab a random sample from our pre-calculated incubation period times.
        let incubation_time = context.sample_incubation_period_time();
        context.add_plan(
            context.get_current_time() + incubation_time,
            move |context| {
                context.set_person_property(event.person_id, HealthStatus, HealthStatusType::Mild);
            },
        );
    }
}

/// Makes sure that the person's health status gets updated when they recover if they were asymptomatic.
fn handle_infection_ending(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    // This case is simple -- health status is only coupled to infectious status when a person is asymptomatic only.
    // Improvement from other symptom courses is not related to infectious status and managed elsewhere.
    // However, we need to check whether the person truly
    if context.get_person_property(event.person_id, HealthStatus) == HealthStatusType::Asymptomatic
    {
        context.set_person_property(event.person_id, HealthStatus, HealthStatusType::Healthy);
    }
}

/// Schedule the person to recover or become severely symptomatic at some point in the future.
fn handle_mild_symptoms(context: &mut Context, event: PersonPropertyChangeEvent<HealthStatus>) {
    let parameters = context.get_global_property_value(Parameters).unwrap();
    if context.sample_bool(
        HealthStatusRng,
        context
            .get_global_property_value(Parameters)
            .unwrap()
            .hospitalization_infection_probability,
    ) {
        // In the future, we will replace this with a real distribution based on NNH's parameter estimates.
        let time_to_severe = context.sample_distr(
            HealthStatusRng,
            Exp::new(parameters.time_to_hospitalization).unwrap(),
        );
        context.add_plan(
            context.get_current_time() + time_to_severe,
            move |context| {
                context.set_person_property(
                    event.person_id,
                    HealthStatus,
                    HealthStatusType::Severe,
                );
            },
        );
    } else {
        // If the person doesn't become severely symptomatic, schedule them to recover.
        // We will use symptom improvement times from our isolation guidance modeling work,
        // so for now, the Exponential distribution is just a placeholder here.
        let symptom_improvement_time =
            context.sample_distr(HealthStatusRng, Exp::new(1.0).unwrap());
        context.add_plan(
            context.get_current_time() + symptom_improvement_time,
            move |context| {
                context.set_person_property(
                    event.person_id,
                    HealthStatus,
                    HealthStatusType::Healthy,
                );
            },
        );
    }
}

/// Schedule the person to recover at some point in the future.
fn handle_severe_symptoms(context: &mut Context, event: PersonPropertyChangeEvent<HealthStatus>) {
    let parameters = context.get_global_property_value(Parameters).unwrap();
    // In the future, we will replace this with a real distribution based on NNH's parameter estimates.
    let time_to_recovery = context.sample_distr(
        HealthStatusRng,
        Exp::new(parameters.hospitalization_duration).unwrap(),
    );
    context.add_plan(
        context.get_current_time() + time_to_recovery,
        move |context| {
            context.set_person_property(event.person_id, HealthStatus, HealthStatusType::Healthy);
        },
    );
}
