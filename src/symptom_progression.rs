use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, Context, ContextPeopleExt,
    ContextRandomExt, PersonId, PersonPropertyChangeEvent,
};
use serde::Serialize;

use crate::infectiousness_manager::{InfectionStatus, InfectionStatusValue};

define_rng!(SymptomRng);

#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum SymptomValue {
    Category1,
    Category2,
    Category3,
    Category4,
}

define_person_property_with_default!(ClinicalSymptoms, Option<SymptomValue>, None);

struct ClinicalCategoryContainer {
    categories: Vec<SymptomValue>,
    recovery_distributions: Vec<f64>,
    incubation_distributions: Vec<f64>,
    weights: Vec<f64>,
}

define_data_plugin!(
    ClinicalCategoryPlugin,
    ClinicalCategoryContainer,
    ClinicalCategoryContainer {
        categories: Vec::new(),
        recovery_distributions: Vec::new(),
        incubation_distributions: Vec::new(),
        weights: Vec::new(),
    }
);

pub trait ClinicalCategoryExt {
    fn add_category(
        &mut self,
        category: SymptomValue,
        recovery_distribution: f64,
        incubation_distribution: f64,
        weight: f64,
    ) -> usize;
}

impl ClinicalCategoryExt for Context {
    fn add_category(
        &mut self,
        category: SymptomValue,

        recovery_distribution: f64,
        incubation_distribution: f64,
        weight: f64,
    ) -> usize {
        let container = self.get_data_container_mut(ClinicalCategoryPlugin);
        container.categories.push(category);
        container.recovery_distributions.push(recovery_distribution);
        container
            .incubation_distributions
            .push(incubation_distribution);
        container.weights.push(weight);

        container.categories.len() - 1
    }
}

pub fn init(context: &mut Context) {
    context.add_category(SymptomValue::Category1, 10.0, 5.0, 0.1);
    context.add_category(SymptomValue::Category2, 10.0, 5.0, 0.4);
    context.add_category(SymptomValue::Category3, 10.0, 5.0, 0.3);
    context.add_category(SymptomValue::Category4, 10.0, 5.0, 0.2);
    // Save disease data for a person somewhere after infection
    // If symptomatic, choose one of the categories and make a plan to stop being symptomatic
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current == InfectionStatusValue::Infectious {
                schedule_symptoms(context, event.person_id);
            }
        },
    );
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<ClinicalSymptoms>| {
            if let Some(category) = event.current {
                schedule_recovery(context, event.person_id, category);
            }
        },
    );
}

fn schedule_recovery(context: &mut Context, person: PersonId, category: SymptomValue) {
    // Need to call symptom duration from a data plugin
    let container = context.get_data_container(ClinicalCategoryPlugin).unwrap();

    let index = container
        .categories
        .iter()
        .position(|&c| c == category)
        .expect("Category not found in ClinicalCategoryPlugin container");

    let symptom_duration = container.recovery_distributions[index];
    context.add_plan(
        context.get_current_time() + symptom_duration,
        move |context| {
            context.set_person_property(person, ClinicalSymptoms, None);
        },
    );
}

fn schedule_symptoms(context: &mut Context, person: PersonId) {
    // Need to call incubation period from disease data plugin
    let container = context.get_data_container(ClinicalCategoryPlugin).unwrap();
    let index = context.sample_weighted(SymptomRng, container.weights.as_slice());

    let category = container.categories[index];
    let incubation_period = container.incubation_distributions[index];

    context.add_plan(
        context.get_current_time() + incubation_period,
        move |context| {
            context.set_person_property(person, ClinicalSymptoms, Some(category));
        },
    );
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::{ClinicalCategoryExt, ClinicalCategoryPlugin, SymptomValue};
    use crate::{
        parameters::{GlobalParams, RateFnType},
        rate_fns::load_rate_fns,
        Params,
    };

    use ixa::{Context, ContextGlobalPropertiesExt, ContextRandomExt};

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 3,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_add_category() {
        let mut context = setup();
        //FAILURE: PeoplePlugin is not initialized; make sure you add a person before accessing properties
        let category = context.add_category(SymptomValue::Category1, 1.0, 2.0, 1.0);
        assert_eq!(category, 0);
        assert_eq!(
            context
                .get_data_container(ClinicalCategoryPlugin)
                .unwrap()
                .categories[0],
            SymptomValue::Category1
        );
        assert_eq!(
            context
                .get_data_container(ClinicalCategoryPlugin)
                .unwrap()
                .recovery_distributions[0],
            1.0
        );
        assert_eq!(
            context
                .get_data_container(ClinicalCategoryPlugin)
                .unwrap()
                .incubation_distributions[0],
            2.0
        );
    }
}
