use ixa::{
    define_person_property_with_default, define_rng, trace, Context, ContextPeopleExt,
    ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent,
};
use statrs::distribution::Exp;

use crate::{
    define_setting_category,
    parameters::{ContextParametersExt, HospitalizationParameters, Params},
    population_loader::Age,
    settings::{
        ContextSettingExt, ItineraryEntry, ItineraryModifiers, SettingId, SettingProperties,
    },
    symptom_progression::{SymptomValue, Symptoms},
};

define_setting_category!(Hospital);
define_person_property_with_default!(Hospitalized, bool, false);

define_rng!(HospitalizationRng);

// pub struct HospitalAgeGroups {
//     pub age_minimum: u8,
//     pub age_maximum: u8,
//     pub probabilty: f64,
// }

pub trait ContextHospitalizationExt {
    fn get_hospital_id(&self) -> usize;
}

impl ContextHospitalizationExt for Context {
    fn get_hospital_id(&self) -> usize {
        self.get_hospital_id_internal().unwrap_or(0)
    }
}

trait ContextHospitalizationInternalExt {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError>;
    fn setup_hospitalization_event_sequence(
        &mut self,
        hospitalization_parameters: HospitalizationParameters,
    );
    fn get_hospital_id_internal(&self) -> Option<usize>;
    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> bool;
}

impl ContextHospitalizationInternalExt for Context {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError> {
        // Modify the hospitalization status of a person
        // depending on the person property value being set activate or deactivate the hospitalization itinerary
        self.set_person_property(person_id, Hospitalized, hospitalized);
        if hospitalized {
            if let Some(hospital_id) = self.get_hospital_id_internal() {
                let itinerary = vec![ItineraryEntry::new(
                    SettingId::new(Hospital, hospital_id),
                    1.0,
                )];
                self.modify_itinerary(person_id, ItineraryModifiers::ReplaceWith { itinerary })?;
                trace!("Person {person_id} is hospitalized at hospital {hospital_id}");
            }
        } else {
            self.remove_modified_itinerary(person_id)?;
            trace!("Person {person_id} is discharged from the hospital");
        }
        Ok(())
    }
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // evaluate hospitalization risk
        // plan a delay to enter the hospital
        if self.evaluate_hospitalization_risk(person_id, hospitalization_parameters.clone()) {
            // Get the hospital ID for the person                    // Plan the arrival at the hospital
            let exp = Exp::new(hospitalization_parameters.mean_delay_to_hospitalization).unwrap();
            let delay_to_hospitalization = self.sample_distr(HospitalizationRng, exp);
            trace!(
                "Planning hospital arrival for person {person_id} at {}",
                self.get_current_time() + delay_to_hospitalization
            );
            self.add_plan(
                self.get_current_time() + delay_to_hospitalization,
                move |context| {
                    context.modify_hospitalization(person_id, true).unwrap();
                },
            );
            return Ok(());
        }
        Ok(())
    }
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // select a duration of hospitalization
        // plan to leave the hospital after the duration
        trace!("Planning hospital departure for person {person_id}");
        let exp = Exp::new(hospitalization_parameters.mean_duration_of_hospitalization).unwrap();
        let hospitalization_duration = self.sample_distr(HospitalizationRng, exp);
        self.add_plan(
            self.get_current_time() + hospitalization_duration,
            move |context| {
                context.modify_hospitalization(person_id, false).unwrap();
            },
        );
        Ok(())
    }

    fn setup_hospitalization_event_sequence(
        &mut self,
        hospitalization_parameters: HospitalizationParameters,
    ) {
        // Clone hospitalization_parameters for each closure to avoid move errors
        let hospitalization_parameters_for_symptoms = hospitalization_parameters.clone();
        let hospitalization_parameters_for_hospitalized = hospitalization_parameters.clone();

        // Subscribe to individuals being presymptomatic to plan if/when they enter the hospital
        self.subscribe_to_event(move |context, event: PersonPropertyChangeEvent<Symptoms>| {
            let hospitalization_parameters = hospitalization_parameters_for_symptoms.clone();
            if let Some(SymptomValue::Presymptomatic) = event.current {
                context
                    .plan_hospital_arrival(event.person_id, hospitalization_parameters.clone())
                    .unwrap();
            }
        });
        // Subscribe to individuals being hospitalized to plan when they leave the hospital
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<Hospitalized>| {
                let hospitalization_parameters =
                    hospitalization_parameters_for_hospitalized.clone();
                if event.current {
                    context
                        .plan_hospital_departure(
                            event.person_id,
                            hospitalization_parameters.clone(),
                        )
                        .unwrap();
                }
            },
        );
    }

    fn get_hospital_id_internal(&self) -> Option<usize> {
        // Retrieve the hospital ID for a given person
        Some(0)
    }

    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
        hospitalization_parameters: HospitalizationParameters,
    ) -> bool {
        // Evaluate the risk of hospitalization based on the person's age
        let age = self.get_person_property(person_id, Age);
        if age >= 65 {
            return self.sample_bool(
                HospitalizationRng,
                hospitalization_parameters.probability_by_age[2],
            );
        } else if age >= 18 {
            return self.sample_bool(
                HospitalizationRng,
                hospitalization_parameters.probability_by_age[1],
            );
        }
        self.sample_bool(
            HospitalizationRng,
            hospitalization_parameters.probability_by_age[0],
        )
    }
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context
        .register_setting_category(
            &Hospital,
            SettingProperties {
                alpha: 0.0,
                itinerary_specification: None,
            },
        )
        .unwrap();
    let Params {
        hospitalization_parameters,
        ..
    } = context.get_params();
    if let Some(hospitalization_parameters) = hospitalization_parameters {
        context.setup_hospitalization_event_sequence(hospitalization_parameters.clone());
    } else {
        return Err(IxaError::IxaError("No hospitalization parameters provided. They are required for the hospitalization policy.".to_string()));
    }
    Ok(())
}
