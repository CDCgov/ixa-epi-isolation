use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt,
    ContextRandomExt, PersonPropertyChangeEvent,
};

use statrs::distribution::Uniform;

use crate::{
    infectiousness_manager::InfectionStatusValue,
    interventions::ContextTransmissionModifierExt,
    parameters::{ContextParametersExt, Params},
    symptom_progression::{SymptomValue, Symptoms},
};

define_person_property_with_default!(MaskingStatus, bool, false);
define_person_property_with_default!(IsolatingStatus, bool, false);
define_rng!(PolicyRng);

pub fn init(context: &mut Context) {
    let &Params {
        post_isolation_intervention_duration,
        isolation_guidance_uptake_probability,
        ..
    } = context.get_params();

    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            MaskingStatus,
            &[(true, 0.0)],
        )
        .unwrap();
    context
        .store_transmission_modifier_values(
            InfectionStatusValue::Infectious,
            IsolatingStatus,
            &[(true, 0.0)],
        )
        .unwrap();

    event_subscriptions(
        context,
        post_isolation_intervention_duration,
        isolation_guidance_uptake_probability,
    );
}

fn event_subscriptions(
    context: &mut Context,
    post_isolation_duration: f64,
    isolation_guidance_uptake_probability: f64,
) {
    context.subscribe_to_event::<PersonPropertyChangeEvent<Symptoms>>(move |context, event| {
        match event.current {
            Some(SymptomValue::Presymptomatic) => (),
            Some(
                SymptomValue::Category1
                | SymptomValue::Category2
                | SymptomValue::Category3
                | SymptomValue::Category4,
            ) => {
                if context.sample_bool(PolicyRng, isolation_guidance_uptake_probability) {
                    let uniform = Uniform::new(0.0, 5.0).unwrap();
                    let delay_period = context.sample_distr(PolicyRng, uniform);
                    let t = context.get_current_time();
                    context.add_plan(t + delay_period, move |context| {
                        if context
                            .get_person_property(event.person_id, Symptoms)
                            .is_some()
                        {
                            context.set_person_property(event.person_id, IsolatingStatus, true);
                            trace!("Person {} is now isolating", event.person_id);
                        }
                    });
                }
            }
            None => {
                if context.get_person_property(event.person_id, IsolatingStatus) {
                    context.set_person_property(event.person_id, IsolatingStatus, false);
                    context.set_person_property(event.person_id, MaskingStatus, true);
                    trace!(
                        "Person {} is now wearing a mask and no longer isolating",
                        event.person_id
                    );
                    let t = context.get_current_time();
                    context.add_plan(t + post_isolation_duration, move |context| {
                        context.set_person_property(event.person_id, MaskingStatus, false);
                        trace!("Person {} is now not wearing a mask", event.person_id);
                    });
                }
            }
        }
    });
}
