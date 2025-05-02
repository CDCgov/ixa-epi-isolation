mod contact;
mod infection_propagation_loop;
mod infectiousness_manager;
mod parameters;
mod population_loader;
mod property_progression_manager;
pub mod rate_fns;
mod settings;
mod symptom_progression;
mod transmission_report;
pub mod utils;

use infectiousness_manager::InfectionStatus;
use ixa::runner::run_with_args;
use ixa::{ContextPeopleExt, ContextRandomExt, ContextReportExt};
use parameters::{ContextParametersExt, Params};
use population_loader::Age;

// You must run this with a parameters file:
// cargo run -- --config input/input.json
// Try enabling logs to see some output about infections:
// cargo run -- --config input/input.json --log-level=Trace -f | grep epi_isolation
fn main() {
    run_with_args(|context, _, _| {
        // Read the global properties.
        let &Params {
            max_time,
            seed,
            report_period,
            ..
        } = context.get_params();

        // Set the random seed.
        context.init_random(seed);

        // Add a plan to shut down the simulation after `max_time`, regardless of
        // what else is happening in the model.
        context.add_plan(max_time, |context| {
            context.shutdown();
        });

        // Report the number of people by age, census tract, and infectious status
        // every report_period.
        context.add_periodic_report(
            "person_property_count",
            report_period,
            (Age, InfectionStatus),
        )?;

        settings::init(context)?;

        // Load the synthetic population from the `synthetic_population_file`
        // specified in input.json.
        population_loader::init(context)?;
        context.index_property(Age);

        infection_propagation_loop::init(context)?;
        transmission_report::init(context)?;
        symptom_progression::init(context)?;

        Ok(())
    })
    .unwrap();
}
