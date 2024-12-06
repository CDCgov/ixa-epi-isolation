use ixa::{
    define_person_property, define_person_property_with_default, Context, ContextPeopleExt,
    PersonPropertyChangeEvent,
};

use crate::transmission_manager::{InfectiousStatus, InfectiousStatusType};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum HealthStatusType {
    Healthy,
    Asymptomatic,
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
                handle_infection_starting(context, event);
            } else if event.current == InfectiousStatusType::Recovered {
                assert_eq!(event.previous, InfectiousStatusType::Infectious);
                handle_infection_ending(context, event);
            }
        },
    );
    // We also need to watch for changes to health status to trigger when to plan the person's next health status change.
    context.subscribe_to_event(|_context, event: PersonPropertyChangeEvent<HealthStatus>| {
        // We only care about the cases of asymptomatic to mild, and mild to severe.
        if event.current == HealthStatusType::Mild {
            assert_eq!(event.previous, HealthStatusType::Asymptomatic);
            // Schedule the person to potentially become severely symptomatic at some point in the future.
        } else if event.current == HealthStatusType::Severe {
            assert_eq!(event.previous, HealthStatusType::Mild);
            // Schedule the person to recover at some point in the future.
        }
    });
}

fn handle_infection_starting(
    _context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    if event.current == InfectiousStatusType::Infectious {
        // Determine whether the person stays asymptomatic.

        // If not, schedule them to become mildly symptomatic at some point in the future.
    }
}

fn handle_infection_ending(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    if event.current == InfectiousStatusType::Recovered {
        // This case is simple -- health status is only coupled to infectious status when a person is asymptomatic.
        // Improvement from other symptom courses
        if context.get_person_property(event.person_id, HealthStatus)
            == HealthStatusType::Asymptomatic
        {
            context.set_person_property(event.person_id, HealthStatus, HealthStatusType::Healthy);
        }
    }
}
