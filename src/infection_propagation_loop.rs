use core::f64;
use statrs::distribution::Binomial;

use crate::infectiousness_manager::{
    evaluate_forecast, get_forecast, infection_attempt, Forecast, InfectionContextExt,
    InfectionData, InfectionDataValue, InfectionStatus, InfectionStatusValue,
};
use crate::parameters::{ContextParametersExt, Params};
use crate::rate_fns::{load_rate_fns, InfectiousnessRateExt};
use crate::settings::{ContextSettingExt, ItineraryChangeEvent};
use ixa::{
    define_data_plugin, define_rng, plan::PlanId, trace, Context, ContextPeopleExt,
    ContextRandomExt, HashMap, IxaError, PersonId, PersonPropertyChangeEvent, PluginContext,
};

define_rng!(InfectionRng);

fn schedule_next_forecasted_infection(context: &mut Context, person: PersonId) {
    if let Some(Forecast {
        next_time,
        forecasted_total_infectiousness,
    }) = get_forecast(context, person)
    {
        let infection_plan = context.add_plan(next_time, move |context| {
            if evaluate_forecast(context, person, forecasted_total_infectiousness) {
                let _ = infection_attempt(context, person);
            }
            // Continue scheduling forecasts until the person recovers.
            schedule_next_forecasted_infection(context, person);
        });
        // The forecast plan is added to the data container for tracking
        context.add_forecast_plan(person, infection_plan);
    }
}

fn schedule_recovery(context: &mut Context, person: PersonId) {
    let infection_duration = context.get_person_rate_fn(person).infection_duration();
    let recovery_time = context.get_current_time() + infection_duration;
    context.add_plan(recovery_time, move |context| {
        trace!("Person {person} has recovered at {recovery_time}");
        context.recover_person(person);
        context.remove_forecast_plan(person);
    });
}

#[derive(Default)]
struct ForecastDataContainer {
    active_plans: HashMap<PersonId, PlanId>,
}

impl ForecastDataContainer {
    fn get_plan(&self, person_id: PersonId) -> Option<&PlanId> {
        self.active_plans.get(&person_id)
    }
    fn add_plan(&mut self, person_id: PersonId, plan_id: PlanId) -> Option<PlanId> {
        self.active_plans.insert(person_id, plan_id)
    }
    fn remove_plan(&mut self, person_id: PersonId) -> Option<PlanId> {
        self.active_plans.remove(&person_id)
    }
}

define_data_plugin!(
    ForecastDataPlugin,
    ForecastDataContainer,
    ForecastDataContainer::default()
);

trait ContextForecastInternalExt: PluginContext {
    /// Remove person from the data container `HashMap`
    fn remove_forecast_plan(&mut self, person_id: PersonId) {
        let container = self.get_data_mut(ForecastDataPlugin);
        container.remove_plan(person_id);
    }
    /// Add a new plan to the forecast data plugin and cancel any plan currently associated with that person
    fn add_forecast_plan(&mut self, person_id: PersonId, plan_id: PlanId) {
        let container = self.get_data_mut(ForecastDataPlugin);
        container.add_plan(person_id, plan_id);
    }
    /// Cancel forecast but keep infector in the data map
    fn cancel_forecast(&mut self, person_id: PersonId) {
        let container = self.get_data(ForecastDataPlugin);
        if let Some(active_plan) = container.get_plan(person_id) {
            self.cancel_plan(&active_plan.clone());
        }
    }
    /// Listen to itinerary changes and determine their effect on the current active forecast plans of potential infectors
    fn subscribe_to_itinerary_change(&mut self) {
        self.subscribe_to_event::<ItineraryChangeEvent>(move |context, event| {
            let container = context.get_data(ForecastDataPlugin);
            // Check for any active forecast associated with person
            let affected_infectors: Vec<PersonId> = container
                .active_plans
                .keys()
                .filter(|&infector| {
                    (context.is_contact(event.person_id, *infector) && event.increases_membership)
                        || (event.person_id == *infector
                            && event.previous_multiplier < event.current_multiplier)
                })
                .copied()
                .collect();
            
            // We have to re-evaluate any infectors and infectious contacts that may be new to the person with a changed itinerary,
            // as their itinerary potentially increased membership across some settings
            // and therefore potentially increased the infectiousness of the infector
            for infector in affected_infectors {
                // The infector is only removed from the data plugin once they have no more infectious potential
                // Otherwise, itinerary changes adding individuals to their settings could lead to infection attempts
                context.cancel_forecast(infector);
                schedule_next_forecasted_infection(context, infector);
            }
        });
    }
}
impl ContextForecastInternalExt for Context {}

/// Takes susceptible people from the population and changes them according to a provided `seed_fn`.
/// The total number of people seeded is distributed binomially according to the proportion to seed.
/// The proportion to seed is calibrated to the population size, not the current number of susceptibles.
/// This may result in the entire susceptible population being seeded with `seed_fn`
#[allow(clippy::cast_possible_truncation)]
fn query_susceptibles_and_seed(
    context: &mut Context,
    proportion_to_seed: f64,
    seed_fn: impl Fn(&mut Context, PersonId),
) {
    let binom = Binomial::new(
        proportion_to_seed,
        context.get_current_population().try_into().unwrap(),
    )
    .unwrap();
    let k: u64 = context.sample_distr(InfectionRng, binom);
    trace!(
        "Altering {k} susceptibles with a seeding function using proportion {proportion_to_seed}."
    );

    if k > 0 {
        let susceptibles = context.sample_people(
            InfectionRng,
            (InfectionStatus, InfectionStatusValue::Susceptible),
            k as usize,
        );
        for person in susceptibles {
            seed_fn(context, person);
        }
    }
}

fn seed_initial_infections(context: &mut Context, initial_incidence: f64) {
    query_susceptibles_and_seed(context, initial_incidence, |context, person_id| {
        trace!("Infecting person {person_id} as an initial infection.");
        context.infect_person(person_id, None, None, None);
    });
}

fn seed_initial_recovered(context: &mut Context, initial_recovered: f64) {
    query_susceptibles_and_seed(context, initial_recovered, |context, person_id| {
        trace!("Recovering person {person_id} as an initial recovered.");
        context.set_person_property(
            person_id,
            InfectionData,
            InfectionDataValue::Recovered {
                // If we choose to seed the population with people who are in various levels of "recovered"
                // and include waning immunity based on that, we could changes these values to reflect
                // when prior to simulation start an individual was actually infected/recovered.
                infection_time: f64::NAN,
                recovery_time: f64::NAN,
            },
        );
    });
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        initial_incidence,
        initial_recovered,
        ..
    } = context.get_params();

    load_rate_fns(context)?;

    context.add_plan(0.0, move |context| {
        if initial_incidence > 0.0 {
            seed_initial_infections(context, initial_incidence);
        }
        if initial_recovered > 0.0 {
            seed_initial_recovered(context, initial_recovered);
        }
    });

    // Subscribe to the person becoming infectious to trigger the infection propagation loop
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current != InfectionStatusValue::Infectious {
                return;
            }
            schedule_next_forecasted_infection(context, event.person_id);
            schedule_recovery(context, event.person_id);
        },
    );

    //context.subscribe_to_itinerary_change();

    Ok(())
}

#[cfg(test)]
mod test {
    use serde::{Deserialize, Serialize};
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use ixa::{
        define_person_property_with_default, Context, ContextGlobalPropertiesExt, ContextPeopleExt,
        ContextRandomExt, ExecutionPhase, IxaError, PersonId, PersonPropertyChangeEvent,
    };

    use statrs::{
        assert_almost_eq,
        distribution::{ContinuousCDF, Discrete, Poisson, Uniform},
    };

    use crate::{
        define_setting_category,
        infection_propagation_loop::{
            init, schedule_next_forecasted_infection, schedule_recovery, seed_initial_infections,
            seed_initial_recovered, ContextForecastInternalExt, InfectionStatus,
            InfectionStatusValue,
        },
        infectiousness_manager::{
            max_total_infectiousness_multiplier, InfectionContextExt, InfectionData,
            InfectionDataValue,
        },
        interventions::ContextTransmissionModifierExt,
        parameters::{
            ContextParametersExt, CoreSettingsTypes, GlobalParams, ItinerarySpecificationType,
            Params, RateFnType,
        },
        rate_fns::{load_rate_fns, InfectiousnessRateExt},
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, ItineraryModifiers, SettingId,
            SettingProperties, Workplace,
        },
    };

    define_setting_category!(HomogeneousMixing);

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = vec![ItineraryEntry::new(
            SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context(
        seed: u64,
        rate: f64,
        alpha: f64,
        duration: f64,
        initial_recovered: f64,
    ) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_incidence: 0.1, // 10% of the population
            initial_recovered,
            max_time: 100.0,
            seed,
            infectiousness_rate_fn: RateFnType::Constant { rate, duration },
            settings_properties: HashMap::from([
                (
                    CoreSettingsTypes::Home,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::Workplace,
                    SettingProperties {
                        alpha: 0.5,
                        itinerary_specification: Some(ItinerarySpecificationType::Constant {
                            ratio: 1.0,
                        }),
                    },
                ),
                (
                    CoreSettingsTypes::CensusTract,
                    SettingProperties {
                        alpha: 0.5,
                        // Itinerary is specified in the `set_homogeneous_mixing_itinerary` function
                        // so we do not need to set it here.
                        itinerary_specification: None,
                    },
                ),
            ]),
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        // We also set up a homogenous mixing itinerary so that when we don't call `settings::init`,
        // we still have people in settings.
        context
            .register_setting_category(
                &HomogeneousMixing,
                SettingProperties {
                    alpha,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 1.0,
                    }),
                },
            )
            .unwrap();
        context
    }

    #[test]
    fn test_seed_initial_conditions() {
        let mut context = setup_context(0, 1.0, 1.0, 5.0, 0.0);
        let initial_infected = context.add_person(()).unwrap();
        seed_initial_infections(&mut context, 1.0);
        assert_eq!(
            context.get_person_property(initial_infected, InfectionStatus),
            InfectionStatusValue::Infectious
        );

        let initial_recovered = context.add_person(()).unwrap();
        seed_initial_recovered(&mut context, 1.0);
        assert_eq!(
            context.get_person_property(initial_recovered, InfectionStatus),
            InfectionStatusValue::Recovered
        );
    }

    #[test]
    fn test_seed_initial_conditions_empty() {
        let mut context = setup_context(0, 1.0, 1.0, 5.0, 0.0);
        let person = context.add_person(()).unwrap();
        seed_initial_infections(&mut context, 0.0);
        assert_eq!(
            context.get_person_property(person, InfectionStatus),
            InfectionStatusValue::Susceptible
        );

        seed_initial_recovered(&mut context, 0.0);
        assert_eq!(
            context.get_person_property(person, InfectionStatus),
            InfectionStatusValue::Susceptible
        );
    }

    fn seed_initial_conditions_binomial(incidence: f64, recovered: f64, pop_size: i32) -> usize {
        let mut context = setup_context(0, 1.0, 1.0, 5.0, 0.0);
        for _ in 0..pop_size {
            context.add_person(()).unwrap();
        }
        seed_initial_infections(&mut context, incidence);
        seed_initial_recovered(&mut context, recovered);
        context.query_people_count((InfectionStatus, InfectionStatusValue::Infectious))
    }

    #[test]
    fn test_binomial_incidence() {
        let reps = 1000;
        let incidence = 0.01;
        let recovered = 0.0;
        let pop_size = 100;

        let mut infections = 0;
        for _ in 0..reps {
            let i = seed_initial_conditions_binomial(incidence, recovered, pop_size);
            infections += i;
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed = infections as f64 / (reps as f64 * pop_size as f64);
        assert_almost_eq!(incidence, observed, 0.01);
    }

    #[test]
    fn test_binomial_incidence_and_recovery() {
        let reps = 1000;
        let incidence = 0.01;
        let recovered = 0.99;
        let pop_size = 100;

        let mut infections = 0;
        for _ in 0..reps {
            let i = seed_initial_conditions_binomial(incidence, recovered, pop_size);
            infections += i;
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed = infections as f64 / (reps as f64 * pop_size as f64);
        assert_almost_eq!(incidence, observed, 0.01);
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context(42, 1.0, 1.0, 5.0, 0.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        // At the end of 0.0, we should have some seeded infections and recovereds
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(!context
                    .query_people((InfectionStatus, InfectionStatusValue::Infectious))
                    .is_empty());
                assert!(!context
                    .query_people((InfectionStatus, InfectionStatusValue::Recovered))
                    .is_empty());
            },
            ExecutionPhase::Last,
        );
    }

    #[test]
    fn test_zero_rate_no_infections() {
        let mut context = setup_context(0, 0.0, 1.0, 5.0, 0.1);

        // Add people -- a lot so we can show that no new infections are added
        for _ in 0..1000 {
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        // We're going to extract out the number of initial infections and recovered
        let num_initial_infections = Rc::new(RefCell::new(0));
        let num_initial_infections_clone = Rc::clone(&num_initial_infections);

        let num_initial_recovered = Rc::new(RefCell::new(0));
        let num_initial_recovered_clone = Rc::clone(&num_initial_recovered);

        context.add_plan(0.0, move |context| {
            // Count the number of initial infections and recovered actually created from the binomial
            // sampling
            *num_initial_infections_clone.borrow_mut() = context
                .query_people((InfectionStatus, InfectionStatusValue::Infectious))
                .len();
            *num_initial_recovered_clone.borrow_mut() = context
                .query_people((InfectionStatus, InfectionStatusValue::Recovered))
                .len();
        });

        // We want to count the number of new infections that are created to ensure this is equal to
        // the number of initial infections seeded.
        let num_new_infections = Rc::new(RefCell::new(0));
        let num_new_infections_clone = Rc::clone(&num_new_infections);

        context.subscribe_to_event(
            move |_context, event: PersonPropertyChangeEvent<InfectionStatus>| {
                if event.current == InfectionStatusValue::Infectious {
                    *num_new_infections_clone.borrow_mut() += 1;
                }
            },
        );

        context.execute();

        // Make sure that the only people who pass through infectious are those that we seeded
        // as the initial infectious
        assert_eq!(
            *num_new_infections.borrow(),
            *num_initial_infections.borrow()
        );

        // And that recovereds is equal to the initial infectious (who have recovered) + recovered
        assert_eq!(
            context.query_people_count((InfectionStatus, InfectionStatusValue::Recovered)),
            *num_initial_infections.borrow() + *num_initial_recovered.borrow(),
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum InfectiousnessProportion {
        None,
        Partial,
    }
    define_person_property_with_default!(
        InfectiousnessProportionStatus,
        InfectiousnessProportion,
        InfectiousnessProportion::None
    );

    pub const INFECTIOUS_PARTIAL: f64 = 0.7;

    #[test]
    fn test_number_timing_infections_one_time_unit() {
        // Does one infectious person generate the number of infections as expected by the rate?
        // We're going to run many simulations that each start with one infectious and one
        // susceptible person. The susceptible person gets moved back to susceptible when becoming
        // infected, so this is really a setup where there is no susceptible depletion/an
        // infinitely large starting population. We stop the simulation at the end of 1.0 time units
        // and compare the number of infected people to the infectious rate.
        // We're also going to check the times at which they are infected. In this test simulation,
        // we are using a constant hazard of infection, and we only record infection times that are
        // within 1.0 time units, so we expect the timing of infection attempts to follow U(0, 1).
        // First, we should not expect to observe an exponential distribution because we may observe
        // multiple infection attempts in the same experiment, not just the first. This also helps
        // provide intuition for why we expect a uniform distribution -- if the first infection
        // attempt happens quickly, that increases the chance we see another in 1.0 time units, and
        // because there is basically this compensating relationship between the time and the number
        // of events, they "cancel" each other out to give a uniform distribution (handwavingly).
        let num_sims: u64 = 20_000;
        let rate = 1.5;
        let alpha = 0.42;
        let duration = 5.0;
        // We need the total infectiousness multiplier for the person.
        let mut total_infectiousness_multiplier = None;
        let mut modifier = 1.0;
        // Where we store the infection times.
        let infection_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let num_infected = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let infection_times_clone = Rc::clone(&infection_times);
            let num_infected_clone = Rc::clone(&num_infected);
            let mut context = setup_context(seed, rate, alpha, duration, 0.0);

            context
                .store_transmission_modifier_values(
                    InfectionStatusValue::Infectious,
                    InfectiousnessProportionStatus,
                    &[(InfectiousnessProportion::Partial, INFECTIOUS_PARTIAL)],
                )
                .unwrap();

            // We only run the simulation for 1.0 time units.
            context.add_plan_with_phase(1.0, ixa::Context::shutdown, ExecutionPhase::Last);
            // Add a a person who will get infected.
            let p1 = context.add_person(()).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
            // We don't want infectious people beyond our index case to be able to transmit, so we
            // have to do setup on our own since just calling `init` will trigger a watcher for
            // people becoming infectious that lets them transmit.
            load_rate_fns(&mut context).unwrap();
            // Add our infectious fellow.
            let infectious_person = context
                .add_person((
                    InfectiousnessProportionStatus,
                    InfectiousnessProportion::Partial,
                ))
                .unwrap();
            set_homogeneous_mixing_itinerary(&mut context, infectious_person).unwrap();

            context.infect_person(infectious_person, None, None, None);
            // Get the total infectiousness multiplier for comparison to total number of infections.
            if total_infectiousness_multiplier.is_none() {
                total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                    &context,
                    infectious_person,
                ));
            }
            modifier = context.get_relative_total_transmission(infectious_person);
            // Add a watcher for when people are infected to record the infection times.
            context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
                move |context, event| {
                    if event.current == InfectionStatusValue::Infectious {
                        let current_time = context.get_current_time();
                        infection_times_clone.borrow_mut().push(current_time);
                        // Reset the person to susceptible.
                        if event.person_id != infectious_person {
                            *num_infected_clone.borrow_mut() += 1;
                            context.set_person_property(
                                event.person_id,
                                InfectionData,
                                InfectionDataValue::Susceptible,
                            );
                        }
                    }
                },
            );
            // Setup is now over -- onto actually letting our infectious fellow infect others.
            schedule_next_forecasted_infection(&mut context, infectious_person);
            context.execute();
        }

        #[allow(clippy::cast_precision_loss)]
        let avg_number_infections = *num_infected.borrow() as f64 / num_sims as f64;
        assert_almost_eq!(
            avg_number_infections,
            modifier * rate * total_infectiousness_multiplier.unwrap(),
            0.05
        );
        assert_almost_eq!(modifier, INFECTIOUS_PARTIAL, 0.0);
        // Check whether the times at when people are infected fall uniformly on [0, 1].
        check_ks_stat(&mut infection_times.borrow_mut(), |x| {
            Uniform::new(0.0, 1.0).unwrap().cdf(x)
        });
    }

    fn check_ks_stat(times: &mut [f64], theoretical_cdf: impl Fn(f64) -> f64) {
        // Sort the empirical times to make an empirical CDF.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // KS stat is the maximum observed CDF deviation.
        let ks_stat = times
            .iter()
            .enumerate()
            .map(|(i, time)| {
                #[allow(clippy::cast_precision_loss)]
                let empirical_cdf_value = (i as f64) / (times.len() as f64);
                let theoretical_cdf_value = theoretical_cdf(*time);
                (empirical_cdf_value - theoretical_cdf_value).abs()
            })
            .reduce(f64::max)
            .unwrap();

        assert_almost_eq!(ks_stat, 0.0, 0.01);
    }

    #[test]
    fn test_schedule_recovery() {
        // Create a simulation with an infected person and schedule their recovery.
        let mut context = setup_context(0, 0.0, 1.0, 5.0, 0.0);
        load_rate_fns(&mut context).unwrap();
        let person = context.add_person(()).unwrap();
        seed_initial_infections(&mut context, 1.0);
        // For later, we need to get the recovery time from the rate function.
        let recovery_time = context.get_person_rate_fn(person).infection_duration();
        schedule_recovery(&mut context, person);
        context.execute();
        // Make sure person is recovered.
        assert_eq!(
            context.get_person_property(person, InfectionData),
            InfectionDataValue::Recovered {
                infection_time: 0.0,
                recovery_time
            }
        );
        // Make sure nothing has happened after person is recovered.
        assert_almost_eq!(context.get_current_time(), recovery_time, 0.0);
    }

    #[test]
    fn test_location_infections() {
        // Does one infectious person generate the number of infections as expected in different
        // settings? We're going to run many simulations that each start with one infectious and three
        // susceptible person. Each susceptible person belongs in one of three setting types
        // and the infectious person is in all three settings. The simulation ends after the
        // first person is infected. The location of this infection is records. We compare the number of
        // infected people in each setting to the expected proportion defined by the ratios. We examine
        // seven scenarios of ratios for the infectious individual.
        let num_sims: u64 = 1000;
        let rate = 1.5;
        let alpha = 0.42;

        // ratios is a matrix of ratio values for the three settings. The first value in each row
        // corresponds to the home setting, the second to the census tract setting, and the third to
        // the workplace setting.
        let ratios = [
            [0.0, 0.0, 0.5],
            [0.0, 0.5, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 0.5, 0.0],
            [0.5, 0.0, 0.5],
            [0.0, 0.5, 0.5],
            [0.5, 0.5, 0.5],
        ];
        for ratio in ratios {
            // We add home workplace and census tract settings to context
            // in the test setup for this unit test.
            // We need the total infectiousness multiplier for the person.
            let sum_of_ratio: f64 = ratio.iter().sum();
            let mut total_infectiousness_multiplier = None;
            // Where we store the infection counts.
            let num_infected_home = Rc::new(RefCell::new(0usize));
            let num_infected_censustract = Rc::new(RefCell::new(0usize));
            let num_infected_workplace = Rc::new(RefCell::new(0usize));

            for seed in 0..num_sims {
                let num_infected_home_clone = Rc::clone(&num_infected_home);
                let num_infected_cenustract_clone = Rc::clone(&num_infected_censustract);
                let num_infected_workplace_clone = Rc::clone(&num_infected_workplace);
                let mut context = setup_context(seed, rate, alpha, 5.0, 0.0);
                crate::settings::init(&mut context);

                // Add a a person who will get infected.
                let infectious_person = context.add_person(()).unwrap();
                let person_home = context.add_person(()).unwrap();
                let person_censustract = context.add_person(()).unwrap();
                let person_workplace = context.add_person(()).unwrap();
                let itinerary_all = vec![
                    ItineraryEntry::new(SettingId::new(Home, 0), ratio[0]),
                    ItineraryEntry::new(SettingId::new(CensusTract, 0), ratio[1]),
                    ItineraryEntry::new(SettingId::new(Workplace, 0), ratio[2]),
                ];
                let itinerary_home = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
                let itinerary_censustract =
                    vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0)];
                let itinerary_workplace =
                    vec![ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0)];
                context
                    .add_itinerary(infectious_person, itinerary_all)
                    .unwrap();
                context.add_itinerary(person_home, itinerary_home).unwrap();
                context
                    .add_itinerary(person_censustract, itinerary_censustract)
                    .unwrap();
                context
                    .add_itinerary(person_workplace, itinerary_workplace)
                    .unwrap();

                // We don't want infectious people beyond our index case to be able to transmit, so we
                // have to do setup on our own since just calling `init` will trigger a watcher for
                // people becoming infectious that lets them transmit.
                load_rate_fns(&mut context).unwrap();

                context.infect_person(infectious_person, None, None, None);
                // Get the total infectiousness multiplier for comparison to total number of infections.
                if total_infectiousness_multiplier.is_none() {
                    total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                        &context,
                        infectious_person,
                    ));
                }
                // Add a watcher for when people are infected to record their infection settings.
                context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
                    move |context, event| {
                        if event.current == InfectionStatusValue::Infectious {
                            // Reset the person to susceptible.
                            if event.person_id == person_home {
                                *num_infected_home_clone.borrow_mut() += 1;
                            } else if event.person_id == person_censustract {
                                *num_infected_cenustract_clone.borrow_mut() += 1;
                            } else if event.person_id == person_workplace {
                                *num_infected_workplace_clone.borrow_mut() += 1;
                            }
                            context.shutdown();
                        }
                    },
                );
                // Setup is now over -- onto actually letting our infectious fellow infect others.
                schedule_next_forecasted_infection(&mut context, infectious_person);
                context.execute();
            }
            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_home = *num_infected_home.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(avg_number_infections_home, ratio[0] / sum_of_ratio, 0.05);
            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_censustract =
                *num_infected_censustract.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(
                avg_number_infections_censustract,
                ratio[1] / sum_of_ratio,
                0.05
            );
            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_workplace =
                *num_infected_workplace.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(
                avg_number_infections_workplace,
                ratio[2] / sum_of_ratio,
                0.05
            );
        }
    }

    // Scenario for three person household with random intervention
    // 1. Person 1 is infectious and lives with a community of other people who are not yet infected
    // 2. Person 1 wears a mask that is 100% effective at periodic intervals during the day for some proportion of time
    // 3. No subsequent infectious individuals contribute to transmission

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
    pub enum Masking {
        None,
        Wearing,
    }
    define_person_property_with_default!(MaskingStatus, Masking, Masking::None);

    // Function to return a vector of infection times for each secondary case created according to the scenario described above
    // This is identical to the test_number_timing_infections_one_time_unit function, but with the addition of a facemask modifier
    // and making community size greater than 1 allows for testing alpha with calculating the total infectiousness multiplier
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn run_masking_community_scenario(
        seed: u64,
        rate: f64,
        alpha: f64,
        duration: f64,
        masking_proportion: f64,
        mask_changes: f64,
        community_size: usize,
    ) -> Vec<(usize, f64)> {
        let mut context = setup_context(seed, rate, alpha, duration, 0.0);
        load_rate_fns(&mut context).unwrap();

        // Initialize the infectious person
        let infectious_person = context.add_person(()).unwrap();
        context.infect_person(infectious_person, None, None, None);
        context.add_plan(0.0, move |context| {
            schedule_next_forecasted_infection(context, infectious_person);
            schedule_recovery(context, infectious_person);
        });

        // Set up the intervention of facemask and register a perfect facemask modifier
        context
            .store_transmission_modifier_values(
                InfectionStatusValue::Infectious,
                MaskingStatus,
                &[(Masking::Wearing, 0.0)],
            )
            .unwrap();

        // Switch back and forth between masking or not masking every `masking_duration`. If `mask_changes` is 10.0,
        // this results in one tenth of the infectious period spent masking is done before the infector switches to unmasked.
        // If masking duration is 0, meaning no intervention is applied, this is skipped.
        let masking_duration = duration * masking_proportion / mask_changes;
        let nonmasking_duration = duration * (1.0 - masking_proportion) / mask_changes;

        if masking_duration > 0.0 {
            context.subscribe_to_event::<PersonPropertyChangeEvent<MaskingStatus>>(
                move |context, event| {
                    let t = context.get_current_time();
                    match event.current {
                        Masking::None => {
                            context.add_plan(t + nonmasking_duration, move |context| {
                                context.set_person_property(
                                    event.person_id,
                                    MaskingStatus,
                                    Masking::Wearing,
                                );
                            });
                        }
                        Masking::Wearing => {
                            context.add_plan(t + masking_duration, move |context| {
                                context.set_person_property(
                                    event.person_id,
                                    MaskingStatus,
                                    Masking::None,
                                );
                            });
                        }
                    }
                },
            );
            // Plan first switch to putting on the mask
            context.add_plan(0.0, move |context| {
                context.set_person_property(infectious_person, MaskingStatus, Masking::Wearing);
            });
        }

        // Set up the household
        set_homogeneous_mixing_itinerary(&mut context, infectious_person).unwrap();
        for _ in 0..community_size - 1 {
            let cohabitant = context.add_person(()).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, cohabitant).unwrap();
        }

        // Listen for infection events
        // We do not schedule the next forecasted infection for the secondary cases, as we want to see how many
        // secondary cases are created by only the initial infectious person before they recover
        let secondary_infection_times = Rc::new(RefCell::new(vec![]));
        let secondary_infection_times_clone: Rc<RefCell<Vec<_>>> =
            Rc::clone(&secondary_infection_times);
        context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
            move |context, event| {
                if event.current == InfectionStatusValue::Infectious {
                    let period_id =
                        (context.get_current_time() * mask_changes / duration).trunc() as usize;
                    secondary_infection_times_clone
                        .borrow_mut()
                        .push((period_id, context.get_current_time()));

                    let InfectionDataValue::Infectious { infected_by, .. } =
                        context.get_person_property(event.person_id, InfectionData)
                    else {
                        panic!("{} is not infected.", event.person_id);
                    };
                    let infector_masking =
                        context.get_person_property(infected_by.unwrap(), MaskingStatus);
                    assert_eq!(infector_masking, Masking::None);
                    assert_eq!(infected_by.unwrap(), infectious_person);
                    context.set_person_property(
                        event.person_id,
                        InfectionData,
                        InfectionDataValue::Susceptible,
                    );
                }
            },
        );

        // Shut down manually to avoid infinite plan loop from masking application
        context.add_plan(duration, ixa::Context::shutdown);

        context.execute();

        // Infection times are returned here for convenience but the usize masking period is used in the test below
        let returned_vec = secondary_infection_times.borrow().clone();
        returned_vec
    }

    #[test]
    #[allow(
        clippy::cast_lossless,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    fn test_community_masking_one_infector() {
        let rate = 2.0;
        let alpha = 0.1;
        let duration = 5.0;
        let masking_proportion = 0.4;
        let mask_changes = 10.0;
        let community_size = 5;
        let n_reps = 5_000;

        let mut cases_count_sizes = vec![];

        // Check that the number of secondary cases is as expected
        let expected_cases = (1.0 - masking_proportion)
            * rate
            * duration
            * ((community_size - 1) as f64).powf(alpha);
        let mut case_count_distribution = [0; 10];

        // Run the simulation
        for seed in 0..n_reps {
            let infection_times = run_masking_community_scenario(
                seed,
                rate,
                alpha,
                duration,
                masking_proportion,
                mask_changes,
                community_size,
            );
            cases_count_sizes.push(infection_times.len());

            // Check that infection counts in each nonmasking period are Poisson distributed
            // First, we find how many cases were generated during each period where the infector was unmasked
            let mut cases_per_masking_period = vec![0; mask_changes as usize];
            for (period_id, _) in infection_times {
                cases_per_masking_period[period_id] += 1;
            }

            // Then, we get the distribution of case counts per period, which is tracked across all experiments
            for &count in &cases_per_masking_period {
                if count < 10 {
                    case_count_distribution[count] += 1;
                }
            }
        }

        // Finally, we check that the distribution of case counts is approximately Poisson distributed using the pmf
        let poisson_dist = Poisson::new(expected_cases / mask_changes).unwrap();
        for (i, &counts) in case_count_distribution.iter().enumerate() {
            let poisson_prob = poisson_dist.pmf(i as u64);
            let empirical_prob = counts as f64 / (mask_changes * n_reps as f64);
            assert_almost_eq!(poisson_prob, empirical_prob, 0.005);
        }

        // And we compare the overall case count average across experiments
        let average_case_count = cases_count_sizes.iter().sum::<usize>() as f64 / n_reps as f64;
        assert_almost_eq!(expected_cases, average_case_count, expected_cases / 100.0);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_proportion_infected_recovered() {
        // If we start with 1000 people 100 times, we should see that the proportion of people
        // who are initialized as infectious and recovered follow the expected proportions.
        let mut num_initial_infections = 0;
        let mut num_initial_recovered = 0;
        let num_people = 1000;
        let num_sims = 10;
        let mut initial_incidence = None;
        let initial_recovered = 0.1; // 10% of people should be recovered
        for seed in 0..num_sims {
            let mut context = setup_context(seed, 0.0, 0.0, 1.0, initial_recovered);
            if initial_incidence.is_none() {
                // If we don't have an initial incidence, get it
                initial_incidence = Some(context.get_params().initial_incidence);
            }
            context.init_random(seed);
            // Add our people
            for _ in 0..num_people {
                context.add_person(()).unwrap();
            }
            init(&mut context).unwrap();
            // Add a plan to shutdown after the seeding so we can count infected and recovereds
            context.add_plan(0.0, ixa::Context::shutdown);
            context.execute();
            // Count number of initial infections and recovereds
            num_initial_infections +=
                context.query_people_count((InfectionStatus, InfectionStatusValue::Infectious));
            num_initial_recovered +=
                context.query_people_count((InfectionStatus, InfectionStatusValue::Recovered));
        }
        // Check that the proportion of people is close to the expected proportion
        assert_almost_eq!(
            num_initial_infections as f64 / (num_people * num_sims) as f64,
            initial_incidence.unwrap(),
            0.01
        );
        assert_almost_eq!(
            num_initial_recovered as f64 / (num_people * num_sims) as f64,
            initial_recovered,
            0.01
        );
    }

    fn setup_transmission_settings_context(seed: u64) -> Context {
        let mut context = Context::new();

        // Create a single itinerary for a group of people
        // Itinerary is pslit 50% at home and 50% at work, but at home transmission potential is density-dependent
        let parameters = Params {
            ..Default::default()
        };

        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context
            .register_setting_category(
                &Home,
                SettingProperties {
                    alpha: 1.0,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();
        context
            .register_setting_category(
                &Workplace,
                SettingProperties {
                    alpha: 0.0,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 0.5,
                    }),
                },
            )
            .unwrap();

        context.init_random(seed);
        load_rate_fns(&mut context).unwrap();
        context.subscribe_to_itinerary_change();

        context
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_forecast_during_self_itinerary_modification() {
        let n_replicates = 1000;
        let successes = Rc::new(RefCell::new(0));

        for seed in 0..n_replicates {
            let successes_clone = Rc::clone(&successes);
            let mut context = setup_transmission_settings_context(seed);

            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
                ItineraryEntry::new(SettingId::new(Workplace, 1), 0.5),
            ];

            let infector = context.add_person(()).unwrap();
            context.infect_person(infector, None, None, None);
            context.add_itinerary(infector, itinerary.clone()).unwrap();

            for _ in 0..5 {
                let contact = context.add_person(()).unwrap();
                context.add_itinerary(contact, itinerary.clone()).unwrap();
            }

            // Create a quasi-infinite pool of susceptibles by reverting status
            context.subscribe_to_event(
                move |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
                    if event.current == InfectionStatusValue::Infectious {
                        *successes_clone.borrow_mut() += 1;
                        context.set_person_property(
                            event.person_id,
                            InfectionData,
                            InfectionDataValue::Susceptible,
                        );
                    }
                },
            );

            // Initialize the schedule for forecasts of infector prior to modifying itinerary
            schedule_next_forecasted_infection(&mut context, infector);

            // Modify itinerary of infector to increase infectiousness due to isolating at home with alpha > 0
            context.add_plan(0.0, move |context| {
                context
                    .modify_itinerary(infector, ItineraryModifiers::RestrictTo { setting: &Home })
                    .unwrap();
            });

            context.execute();
        }

        // Rate of 5.0 = 1.0*(6 - 1)^1.0 in the home
        // For a duration of 5 days, this should yield 25 infections on average.
        let avg_successes = *successes.borrow() as f64 / n_replicates as f64;
        assert_almost_eq!(avg_successes, 25.0, 0.5);
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
    fn test_forecast_during_contact_itinerary_modification() {
        let n_replicates = 1000;
        let successes = Rc::new(RefCell::new(0));
        for seed in 0..n_replicates {
            let successes_clone = Rc::clone(&successes);
            let mut context = setup_transmission_settings_context(seed);

            let itinerary = vec![
                ItineraryEntry::new(SettingId::new(Home, 1), 0.5),
                ItineraryEntry::new(SettingId::new(Workplace, 1), 0.5),
            ];

            let infector = context.add_person(()).unwrap();
            context.infect_person(infector, None, None, None);
            context.add_itinerary(infector, itinerary.clone()).unwrap();

            // Introduce contacts that will have a new itinerary as observed by the infector (rejoining a `Home` setting and increasing infectiousness)
            for _ in 0..5 {
                let contact = context.add_person(()).unwrap();
                context.add_itinerary(contact, itinerary.clone()).unwrap();
                // Modify itinerary of each contact
                context
                    .modify_itinerary(contact, ItineraryModifiers::Exclude { setting: &Home })
                    .unwrap();
                // Plan to remove the modified itinerary after the forecast is calculated
                context.add_plan(0.0, move |context| {
                    context.remove_modified_itinerary(contact).unwrap();
                });
            }

            // Create a quasi-infinite pool of susceptibles by reverting status
            context.subscribe_to_event(
                move |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
                    if event.current == InfectionStatusValue::Infectious {
                        *successes_clone.borrow_mut() += 1;
                        context.set_person_property(
                            event.person_id,
                            InfectionData,
                            InfectionDataValue::Susceptible,
                        );
                    }
                },
            );

            // Initialize the schedule for forecasts of infector prior to modifying itineraries through `context.execute()``
            schedule_next_forecasted_infection(&mut context, infector);

            context.execute();
        }

        // Rate of 3.0 = 0.5*(6 - 1)^1.0 + 0.5*(6 - 1)^0.0 across both settings
        // For a duration of 5 days, this should yield 15 infections on average.
        // Because 8.2% of all infectors never initiate a scheduled forecast (dpois(0, 0.5*5.0=2.5)), there is a decrease.
        let avg_successes = *successes.borrow() as f64 / n_replicates as f64;
        assert_almost_eq!(avg_successes, 15.0 * (1.0 - 0.082), 0.5);
    }
}
