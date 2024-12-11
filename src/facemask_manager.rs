use crate::intervention_manager::{ContextInterventionExt, FacemaskStatus, FacemaskStatusType};
use crate::parameters::Parameters;
use crate::population_loader::Alive;
use crate::transmission_manager::InfectiousStatusType;
use ixa::{
    define_rng, Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, PersonId,
};

define_rng!(FacemaskRng);

pub fn init(context: &mut Context) {
    context.register_intervention(
        InfectiousStatusType::Susceptible,
        FacemaskStatusType::None,
        1.0,
    );
    context.register_intervention(
        InfectiousStatusType::Susceptible,
        FacemaskStatusType::Wearing,
        0.5,
    );
    context.register_intervention(
        InfectiousStatusType::Infectious,
        FacemaskStatusType::None,
        1.0,//make 1 default?
    );
    context.register_intervention(
        InfectiousStatusType::Infectious,
        FacemaskStatusType::Wearing,
        0.25,
    );

    let population = context.query_people((Alive, true));
    for i in population {
        assign_facemask_status(context, i);
    }
}

fn assign_facemask_status(context: &mut Context, person_id: PersonId) {
    let masking_rate = context
        .get_global_property_value(Parameters)
        .unwrap()
        .masking_rate;

    let mask_uniform_draw = context.sample_range(FacemaskRng, 0.0..1.0);
    if mask_uniform_draw < masking_rate {
        context.set_person_property(person_id, FacemaskStatus, FacemaskStatusType::Wearing);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::ParametersValues;
    use ixa::people::ContextPeopleExt;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn setup(masking: f64) -> Context {
        let params = ParametersValues {
            max_time: 10.0,
            seed: 42,
            r_0: 2.0,
            infection_duration: 0.1,
            generation_interval: 3.0,
            report_period: 1.0,
            masking_rate: masking,
            synth_population_file: PathBuf::from("."),
            population_periodic_report: String::new(),
        };
        let mut context = Context::new();
        context.init_random(params.seed);
        context
            .set_global_property_value(Parameters, params)
            .unwrap();
        context
    }

    #[test]
    fn test_assign_facemask_status_guaranteed() {
        let mut context = setup(1.0);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::Wearing);
    }

    #[test]
    fn test_assign_facemask_status_zero() {
        let mut context = setup(0.0);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::None);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_assign_facemask_status_largepop() {
        let mut context = setup(0.5);
        let mut facemask_counts = HashMap::new();
        let population_size = 5000;

        for _ in 0..population_size {
            let person_id = context.add_person(()).unwrap();
            assign_facemask_status(&mut context, person_id);
            let facemask_status = context.get_person_property(person_id, FacemaskStatus);
            *facemask_counts.entry(facemask_status).or_insert(0) += 1;
        }

        let wearing_count =
            context.query_people_count((FacemaskStatus, FacemaskStatusType::Wearing));
        let none_count = context.query_people_count((FacemaskStatus, FacemaskStatusType::None));

        assert_eq!(wearing_count + none_count, population_size);
        assert!(wearing_count > 0);
        assert!(none_count > 0);
        assert!(
            ((wearing_count as f64) / (population_size as f64)
                - context
                    .get_global_property_value(Parameters)
                    .unwrap()
                    .masking_rate)
                .abs()
                < 0.05
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_init() {
        let mut context = setup(0.5);
        let population_size = 5000;
        for _ in 0..population_size {
            let _person_id = context.add_person(()).unwrap();
        }

        init(&mut context);

        let wearing_count =
            context.query_people_count((FacemaskStatus, FacemaskStatusType::Wearing));
        let none_count = context.query_people_count((FacemaskStatus, FacemaskStatusType::None));

        assert_eq!(wearing_count + none_count, population_size);
        assert!(wearing_count > 0);
        assert!(none_count > 0);
        assert!(
            ((wearing_count as f64) / (population_size as f64)
                - context
                    .get_global_property_value(Parameters)
                    .unwrap()
                    .masking_rate)
                .abs()
                < 0.05
        );
    }
}
