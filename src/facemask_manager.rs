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

    fn setup(masking: f64, tmax: f64, reproduction_number: f64, rand_seed: u64) -> Context {
        let params = ParametersValues {
            max_time: tmax,
            seed: rand_seed,
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
        let mut context = setup(1.0, 0.0, 2.0, 42);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::Wearing);
    }

    #[test]
    fn test_assign_facemask_status_zero() {
        let mut context = setup(0.0, 0.0, 2.0, 42);
        let person_id = context.add_person(()).unwrap();

        assign_facemask_status(&mut context, person_id);

        let facemask_status = context.get_person_property(person_id, FacemaskStatus);
        assert_eq!(facemask_status, FacemaskStatusType::None);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_assign_facemask_status_largepop() {
        let mut context = setup(0.5, 0.0, 2.0, 42);
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
        let mut context = setup(0.5, 0.0, 2.0, 42);
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

    // Evaluate transmission between two people simplified from transmission manager
    fn evaluate_transmission(
        context: &mut Context,
        contact_id: PersonId,
        transmitter_id: PersonId,
    ) -> bool {
        let relative_infectiousness =
            context.query_relative_transmission(transmitter_id, FacemaskStatus);
        let relative_risk = context.query_relative_transmission(contact_id, FacemaskStatus);
        let relative_transmission = relative_infectiousness * relative_risk;
        context.sample_range(FacemaskRng, 0.0..1.0) < relative_transmission
    }

    #[allow(clippy::cast_precision_loss)]
    fn two_person_epidemic_trial(
        relative_risk: f64,
        relative_infectiousness: f64,
        transmitter_mask: FacemaskStatusType,
        contact_mask: FacemaskStatusType,
        seed: u64,
    ) -> bool {
        // Setting R0 to 50 for guaranteed infection attempt
        let mut context = setup(0.5, 0.0, 2.0, seed);
        intervention_init(&mut context);

        let transmitter_id = context
            .add_person((
                (InfectiousStatus, InfectiousStatusType::Infectious),
                (FacemaskStatus, transmitter_mask),
            ))
            .unwrap();
        let contact_id = context
            .add_person((
                (InfectiousStatus, InfectiousStatusType::Susceptible),
                (FacemaskStatus, contact_mask),
            ))
            .unwrap();

        context
            .register_intervention(
                InfectiousStatusType::Infectious,
                FacemaskStatusType::Wearing,
                relative_infectiousness,
            )
            .unwrap();
        context
            .register_intervention(
                InfectiousStatusType::Susceptible,
                FacemaskStatusType::Wearing,
                relative_risk,
            )
            .unwrap();

        evaluate_transmission(&mut context, contact_id, transmitter_id)
    }

    #[allow(clippy::cast_precision_loss)]
    fn call_n_transmission_trials(
        n: usize,
        relative_risk: f64,
        relative_infectiousness: f64,
        transmitter_mask: FacemaskStatusType,
        contact_mask: FacemaskStatusType,
    ) {
        let infectiousness = match transmitter_mask {
            FacemaskStatusType::None => 1.0,
            FacemaskStatusType::Wearing => relative_infectiousness,
        };
        let risk = match contact_mask {
            FacemaskStatusType::None => 1.0,
            FacemaskStatusType::Wearing => relative_risk,
        };

        let mut successes: usize = 0;
        for seed in 0..n {
            if two_person_epidemic_trial(
                relative_risk,
                relative_infectiousness,
                transmitter_mask,
                contact_mask,
                seed.try_into().unwrap(),
            ) {
                successes += 1;
            }
        }
        let observed_rate = successes as f64 / n as f64;
        let theory_rate = risk * infectiousness;

        assert!((theory_rate - observed_rate).abs() < 0.01);
    }

    #[test]
    fn test_two_person_epidemic_trial() {
        call_n_transmission_trials(
            1000,
            0.25,
            0.75,
            FacemaskStatusType::Wearing,
            FacemaskStatusType::Wearing,
        );
        call_n_transmission_trials(
            1000,
            0.25,
            0.75,
            FacemaskStatusType::None,
            FacemaskStatusType::Wearing,
        );
        call_n_transmission_trials(
            1000,
            0.25,
            0.75,
            FacemaskStatusType::Wearing,
            FacemaskStatusType::None,
        );
        call_n_transmission_trials(
            1000,
            0.25,
            0.75,
            FacemaskStatusType::None,
            FacemaskStatusType::None,
        );
    }

    #[allow(clippy::cast_precision_loss)]
    fn epidemic_comparison_with_facemask(
        masking_rate: f64,
        masking_efficacy: f64,
        r_0: f64,
        seed: u64,
    ) {
        let observed_r_0 = r_0 * (1.0 - masking_efficacy * masking_rate);
        let theoretical_ratio = toms748(|x| 1.0 - x - f64::exp(-observed_r_0 * x), 0.0002, 1.0)
            .root()
            .unwrap();

        let mut context = setup(masking_rate, 10.0, r_0, seed);

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
                1.0 - masking_efficacy,
            )
            .unwrap();

        transmission_init(&mut context);

        context.execute();

        let epidemic_size =
            context.query_people_count((InfectiousStatus, InfectiousStatusType::Recovered));
        let observed_ratio = epidemic_size as f64 / population_size as f64;

        assert!((observed_ratio - theoretical_ratio).abs() < 0.025);
    }

    #[test]
    fn test_epidemic_comparison_with_facemask() {
        epidemic_comparison_with_facemask(0.8, 0.9, 10.0, 42);
        epidemic_comparison_with_facemask(0.0, 0.9, 2.8, 42);
    }
}
