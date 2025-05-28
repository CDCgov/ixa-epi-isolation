use ixa::{
    define_person_property_with_default, trace, Context, ContextPeopleExt,
    PersonPropertyChangeEvent,
};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    symptom_progression::{SymptomData, SymptomValue, Symptoms},
    property_progression_manager::{load_progressions, ContextPropertyProgressionExt, Progression},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);

pub fn init(context: &mut Context) {
    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            MaskingStatus,
            &[(true, 0.0)],
        )
        .unwrap();
    context.store_transmission_modifier_values(
        InfectionStatusValue::Infectious,
        IsolatingStatus,
        &[(true, 0.0)],
    )
        .unwrap();

    event_subscriptions(context);
}

fn event_subscriptions(context: &mut Context) {
    // The first sequence of subscriptions uses the person property change event looking at the intervention
    // to determine when to start and stop following interventions.
    context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(move |context, event| {
        match event.current {
            Some(
                SymptomValue::Category1
                | SymptomValue::Category2
                | SymptomValue::Category3
                | SymptomValue::Category4,
            ) => {
                context.set_person_property(
                    event.person_id,
                    IsolatingStatus,
                    true,
                );
                trace!("Event current: {:?}, Person ID: {}", event.current, event.person_id);
                trace!("Person {} is now isolating", event.person_id);
            }
            None | Some(SymptomValue::Presymptomatic) => {
                trace!("Event current: {:?}, Person ID: {}", event.current, event.person_id);
            },
        }
    });
    context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
        move |context, event| {
            let t = context.get_current_time();
            let symptom_duration = 5.0;
            // if let Some(progressions) = context.get_property_progressions() {
                
            // }
            match event.current {
                true => {
                    context.add_plan(t + symptom_duration, move |context| {
                        context.set_person_property(
                            event.person_id,
                            IsolatingStatus,
                            false,
                        );
                        trace!(
                            "Person {} is no longer isolating",
                            event.person_id
                        );
                    });
                }
                false => {context.set_person_property(
                    event.person_id,
                    MaskingStatus,
                    true,
                );
                    trace!("Person {} is now wearing a mask", event.person_id);
                },
            }
        },
    );
    context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
        move |context, event| {
            let t = context.get_current_time();
            match event.current {
                true => {
                    context.add_plan(t + 5.0, move |context| {
                        context.set_person_property(event.person_id, MaskingStatus, false);
                        trace!("Person {} is now not wearing a mask", event.person_id);
                    });
                }
                false => (),
            }
        },
    );
}
