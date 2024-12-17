use crate::intervention_manager::ContextInterventionExt;
use crate::parameters::Parameters;
use crate::population_loader::Alive;
use crate::transmission_manager::InfectiousStatusType;
use ixa::{
    define_person_property, define_person_property_with_default, define_rng, Context,
    ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, IxaError, PersonId,
};
use std::hash::Hash;

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash)]
pub enum FacemaskStatusType {
    None,
    Wearing,
}

define_person_property_with_default!(FacemaskStatus, FacemaskStatusType, FacemaskStatusType::None);

define_rng!(FacemaskRng);

/// Initialize the facemask intervention.
/// Assign facemask status to all people in the population and register the `FacemaskStatus` to relative transmission effects.
pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.register_intervention(
        InfectiousStatusType::Susceptible,
        FacemaskStatusType::Wearing,
        0.5,
    )?;
    context.register_intervention(
        InfectiousStatusType::Infectious,
        FacemaskStatusType::Wearing,
        0.25,
    )?;

    let population = context.query_people((Alive, true));
    for i in population {
        assign_facemask_status(context, i);
    }

    Ok(())
}

/// Assign facemask status to a person based on the global masking rate.
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
    use crate::intervention_manager::init as intervention_init;
    use crate::parameters::ParametersValues;
    use crate::transmission_manager::{
        init as transmission_init, InfectiousStatus, InfectiousStatusType,
    };
    use ixa::people::ContextPeopleExt;
    use root1d::toms748;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn setup(masking: f64, tmax: f64, reproduction_number: f64) -> Context {
        let params = ParametersValues {
            max_time: tmax,
            seed: 42,
            r_0: reproduction_number,
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
        let mut context = setup(1.0, 0.0, 2.0);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::Wearing);
    }

    #[test]
    fn test_assign_facemask_status_zero() {
        let mut context = setup(0.0, 0.0, 2.0);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::None);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_assign_facemask_status_largepop() {
        let mut context = setup(0.5, 0.0, 2.0);
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
        let mut context = setup(0.5, 0.0, 2.0);
        let population_size = 5000;
        for _ in 0..population_size {
            let _person_id = context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

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

    #[allow(clippy::cast_precision_loss)]
    fn epidemic_comparison_with_facemask(
        masking_rate: f64,
        masking_efficacy: f64,
        r_0: f64,
    ) -> f64 {
        let observed_r_0 = r_0 * (1.0 - (1.0 - masking_efficacy) * masking_rate);
        let theoretical_ratio = toms748(|x| 1.0 - x - f64::exp(-observed_r_0 * x), 0.0002, 1.)
            .root()
            .unwrap();

        let mut context = setup(masking_rate, 10.0, r_0);

        intervention_init(&mut context);

        let population_size: usize = 500;
        for _ in 0..population_size {
            let person_id = context.add_person(()).unwrap();
            assign_facemask_status(&mut context, person_id);
        }

        context
            .register_intervention(
                InfectiousStatusType::Infectious,
                FacemaskStatusType::Wearing,
                masking_efficacy,
            )
            .unwrap();

        transmission_init(&mut context);

        context.execute();

        let epidemic_size = context
            .query_people((InfectiousStatus, InfectiousStatusType::Recovered))
            .len();
        let observed_ratio = epidemic_size as f64 / population_size as f64;

        observed_ratio - theoretical_ratio
    }

    #[test]
    fn test_epidemic_comparison_with_split_facemask() {
        let masking_rate = 0.5;
        let masking_efficacy = 0.25;
        let r_0 = 25.0;

        let result = epidemic_comparison_with_facemask(masking_rate, masking_efficacy, r_0);
        assert!(result.abs() < 0.05);
    }

    #[test]
    fn test_epidemic_comparison_with_high_facemask() {
        let masking_rate = 0.9;
        let masking_efficacy = 0.25;
        let r_0 = 25.0;

        let result = epidemic_comparison_with_facemask(masking_rate, masking_efficacy, r_0);
        assert!(result.abs() < 0.05);
    }
}
