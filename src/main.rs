mod contact;
mod infection_propagation_loop;
mod infectiousness_manager;
mod parameters;
mod population_loader;
pub mod rate_fns;
pub mod settings;

use infection_propagation_loop::InfectiousStatus;
use ixa::runner::run_with_args;
use ixa::{ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt};
use parameters::Parameters;
use population_loader::{Age, CensusTract};

// You must run this with a parameters file:
// cargo run -- --config input/input.json
// Try enabling logs to see some output about infections:
// cargo run -- --config input/input.json --log-level=Trace -f | grep epi_isolation
fn main() {
    run_with_args(|context, _, _| {
        // Read the global properties.
        let parameters = context
            .get_global_property_value(Parameters)
            .unwrap()
            .clone();

        // Set the random seed.
        context.init_random(parameters.seed);

        // Report the number of people by age, census tract, and infectious status
        // every report_period.
        context.add_periodic_report(
            "person_property_count",
            parameters.report_period,
            (Age, CensusTract, InfectiousStatus),
        )?;

        // Load the synthetic population from the `synthetic_population_file`
        // specified in input.json.
        population_loader::init(context)?;
        context.index_property(Age);
        context.index_property(CensusTract);

        infection_propagation_loop::init(context);

        // Print out the parameters for debugging purposes for the user.
        println!("{parameters:?}");
        Ok(())
    })
    .unwrap();
}
