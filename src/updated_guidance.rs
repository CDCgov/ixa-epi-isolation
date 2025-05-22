use ixa::{
    define_person_property_with_default, trace, Context, ContextPeopleExt,
    PersonPropertyChangeEvent,
};
use serde::{Deserialize, Serialize};

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    symptom_progression::{SymptomData, SymptomValue, Symptoms},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Masking {
    None,
    Wearing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum Isolating {
    None,
    Participating,
}

define_person_property_with_default!(MaskingStatus, Masking, Masking::None);
define_person_property_with_default!(IsolatingStatus, Isolating, Isolating::None);

pub fn init(context: &mut Context) {
    // let params = context.get_params();
    // load_progressions(context, params.symptom_progression_library.clone())?;
    let parameter_names = vec![
        "Symptom category".to_string(),
        "Incubation period".to_string(),
        "Weibull shape".to_string(),
        "Weibull scale".to_string(),
        "Weibull upper bound".to_string(),
    ];
    let parameters = vec![1.0, 5.0, 2.0, 3.0, 4.0];
    SymptomData::register(context, parameter_names, parameters).unwrap();
    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            MaskingStatus,
            &[(Masking::Wearing, 0.0)],
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
                    Isolating::Participating,
                );
                trace!("Person {} is now isolating", event.person_id);
            }
            None | Some(SymptomValue::Presymptomatic) => (),
        }
    });
    context.subscribe_to_event::<PersonPropertyChangeEvent<IsolatingStatus>>(
        move |context, event| {
            let t = context.get_current_time();
            let symptom_duration = 5.0;
            // let symptom_duration = SymptomData::get_symptom_duration(context, event.person_id);
            match event.current {
                Isolating::Participating => {
                    context.add_plan(t + symptom_duration, move |context| {
                        context.set_person_property(
                            event.person_id,
                            MaskingStatus,
                            Masking::Wearing,
                        );
                        context.set_person_property(
                            event.person_id,
                            IsolatingStatus,
                            Isolating::None,
                        );
                        trace!(
                            "Person {} is now wearing a mask and not isolating",
                            event.person_id
                        );
                    });
                }
                Isolating::None => (),
            }
        },
    );
    context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
        move |context, event| {
            let t = context.get_current_time();
            match event.current {
                Masking::Wearing => {
                    context.add_plan(t + 5.0, move |context| {
                        context.set_person_property(event.person_id, MaskingStatus, Masking::None);
                        trace!("Person {} is now not wearing a mask", event.person_id);
                    });
                }
                Masking::None => (),
            }
        },
    );
    //------------------------------------------------------------------------------------------------------------
    // The second sequence of subscriptions uses the symptom progression event to determine when to start and stop
    // interventions following symptom progression
    // context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(move |context, event| {
    //     match event.current {
    //         Some(SymptomValue::Presymptomatic) => (),
    //         Some(
    //             SymptomValue::Category1
    //             | SymptomValue::Category2
    //             | SymptomValue::Category3
    //             | SymptomValue::Category4,
    //         ) => {
    //             context.set_person_property(event.person_id, IsolatingStatus, Isolating::Participating);
    //             trace!("Person {} is now isolating", event.person_id);
    //         }
    //         None => {
    //             // how does this work with seeding?
    //             context.set_person_property(event.person_id, IsolatingStatus, Isolating::None);
    //             context.set_person_property(event.person_id, MaskingStatus, Masking::Wearing);
    //             trace!("Person {} is now wearing a mask and no longer isolating", event.person_id);
    //             let t = context.get_current_time();
    //             context.add_plan(t + 5.0, move |context| {
    //                 context.set_person_property(event.person_id, MaskingStatus, Masking::None);
    //                 trace!("Person {} is now not wearing a mask", event.person_id);
    //             });
    //         },
    //     }
    // });
}
