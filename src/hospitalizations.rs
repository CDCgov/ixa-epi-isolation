use ixa::{define_derived_property, define_person_property_with_default, define_rng, Context, ContextPeopleExt, ContextRandomExt, PersonId, PersonPropertyChangeEvent};
use statrs::distribution::Exp;

use crate::{define_setting_category, parameters::ContextParametersExt, population_loader::Age, settings::{ContextSettingExt, ItineraryEntry, ItineraryModifiers, SettingId}, symptom_progression::{SymptomValue, Symptoms}};

define_setting_category!(Hospital);
define_person_property_with_default!(
    Hospitalized,
    bool,
    false
);

define_rng!(HospitalizationRng);

// pub struct HospitalAgeGroups {
//     pub age_minimum: u8,
//     pub age_maximum: u8,
//     pub probabilty: f64,
// }

trait ContextHospitalizationInternalExt {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
    ) -> Result<(), ixa::IxaError>;
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
    ) -> Result<(), ixa::IxaError>;
    fn setup_hospitalization_event_sequence(
        &mut self,
    );
    fn get_hospital_id(&self, person_id: PersonId) -> Option<usize>;
    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
    ) -> bool;
}

impl ContextHospitalizationInternalExt for Context {
    fn modify_hospitalization(
        &mut self,
        person_id: PersonId,
        hospitalized: bool,
    ) -> Result<(), ixa::IxaError> {
        // Modify the hospitalization status of a person
        self.set_person_property(person_id, Hospitalized, hospitalized);
        if hospitalized{
            if let Some(hospital_id) = self.get_hospital_id(person_id) {
                let itinerary = vec![ItineraryEntry::new(
                    SettingId::new(Hospital, hospital_id),
                    1.0,
                )];
                self.modify_itinerary(person_id, ItineraryModifiers::ReplaceWith {itinerary})?;
            }
        } else {
            self.remove_modified_itinerary(person_id)?;
        }
        Ok(())

    }
    fn plan_hospital_arrival(
        &mut self,
        person_id: PersonId,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // evaluate hospitalization risk
        // plan a delay to enter the hospital
        if self.evaluate_hospitalization_risk(person_id) {
            if let Some(ref hospitalization_parameters) = self.get_params().hospitalization_parameters {
                // Get the hospital ID for the person                    // Plan the arrival at the hospital
                    let exp = Exp::new(hospitalization_parameters.mean_delay_to_hospitalization).unwrap();
                    let delay_to_hospitalization = self.sample_distr(HospitalizationRng, exp);
                    self.add_plan(
                        self.get_current_time() + delay_to_hospitalization, 
                        move |context| {
                            context.modify_hospitalization(
                                person_id,
                                true,
                            ).unwrap();
                        },
                    );
                    return Ok(())
                }
            }
            Ok(())
    }
    fn plan_hospital_departure(
        &mut self,
        person_id: PersonId,
    ) -> Result<(), ixa::IxaError> {
        // get hospital parameters
        // select a duration of hospitalization
        // plan to leave the hospital after the duration
        if self.evaluate_hospitalization_risk(person_id) {
            if let Some(ref hospitalization_parameters) = self.get_params().hospitalization_parameters {
                // Get the hospital ID for the person                    // Plan the arrival at the hospital
                let exp = Exp::new(hospitalization_parameters.mean_duration_of_hospitalization).unwrap();
                let hospitalization_duration = self.sample_distr(HospitalizationRng, exp);
                self.add_plan(
                    self.get_current_time() + hospitalization_duration, 
                    move |context| {
                        context.modify_hospitalization(
                            person_id,
                            false,
                        ).unwrap();
                    },
                );
                return Ok(())
            }
        }
        Ok(())
    }

    fn setup_hospitalization_event_sequence(&mut self) {
        // Initialize any necessary sequences or properties related to hospitalization
        self.subscribe_to_event(
            move |context, event: PersonPropertyChangeEvent<Symptoms>| {
                match event.current{
                    Some(SymptomValue::Presymptomatic) => {context.plan_hospital_arrival(
                        event.person_id
                    ).unwrap();},
                    _ => {}
                }
            },
        );
    }

    fn get_hospital_id(&self, person_id: PersonId) -> Option<usize> {
        // Retrieve the hospital ID for a given person
        Some(0)// Placeholder logic
    }

    fn evaluate_hospitalization_risk(
        &mut self,
        person_id: PersonId,
    ) -> bool {
        // Evaluate the risk of hospitalization based on the person's age
        if let Some(ref hospitalization_parameters) = self.get_params().hospitalization_parameters {
            let age = self.get_person_property(person_id, Age);
            if age >= 65 {
                return self.sample_bool(HospitalizationRng, hospitalization_parameters.probability_by_age[2]);
            } else if age >= 18 {
                return self.sample_bool(HospitalizationRng, hospitalization_parameters.probability_by_age[1]);
            } else {
                return self.sample_bool(HospitalizationRng, hospitalization_parameters.probability_by_age[0]);
            }
        }
        false
    }

}



