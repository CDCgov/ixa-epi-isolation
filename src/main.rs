mod hospitalizations;
mod infection_propagation_loop;
mod infectiousness_manager;
mod interventions;
mod natural_history_parameter_manager;
mod parameters;
mod policies;
mod population_loader;
mod profiling;
mod property_progression_manager;
pub mod rate_fns;
mod settings;
mod symptom_progression;
pub mod reports;
pub mod utils;

use ixa::runner::run_with_args;
use ixa::{ContextPeopleExt, ContextRandomExt};
use parameters::{ContextParametersExt, Params};
use population_loader::Age;

use crate::profiling::{print_profiling_data, ProfilingContextExt};

// You must run this with a parameters file:
// cargo run -- --config input/input.json
// Try enabling logs to see some output about infections:
// cargo run -- --config input/input.json --log-level epi_isolation=Trace -f
fn main() {
    let mut context = run_with_args(|context, _, _| {
        // Read the global properties.
        let &Params { max_time, seed, .. } = context.get_params();

        // Set the random seed.
        context.init_random(seed);

        // Add a plan to shut down the simulation after `max_time`, regardless of
        // what else is happening in the model.
        context.add_plan(max_time, |context| {
            context.shutdown();
        });

        settings::init(context);

        // Load the synthetic population from the `synthetic_population_file`
        // specified in input.json.
        population_loader::init(context)?;
        context.index_property(Age);

        infection_propagation_loop::init(context)?;
        reports::init(context)?;
        symptom_progression::init(context)?;
        policies::init(context)?;
        hospitalizations::init(context);

        Ok(())
    })
    .unwrap();

    // Write the profiling data and context's execution statistics to a JSON file.
    context.write_profiling_data();
    print_profiling_data();
}
