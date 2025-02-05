use std::path::PathBuf;

use ixa::{define_global_property, run_with_args, Context};

mod infection_propagation_loop;
pub mod infectiousness_rate;
mod infectiousness_setup;
mod population_loader;
pub mod settings;

define_global_property!(SynthPopulationFile, PathBuf);
define_global_property!(InitialInfections, usize);
define_global_property!(PopulationSize, usize);
define_global_property!(HouseholdSize, usize);
define_global_property!(RecoveryTime, f64);
// This is an example of some global parameter you'd use when
// setting up the infectiousness rate functions for individuals
define_global_property!(TransmissibilityFactor, f64);
define_global_property!(Alpha, f64);

fn main() {
    run_with_args(|context: &mut Context, args, _| {
        assert!(args.config.is_some(), "You need to run the model with a config file, for example `cargo run -- --config input/input.json`");

        population_loader::init(context)?;
        infectiousness_setup::init(context);
        infection_propagation_loop::init(context);
        Ok(())
    })
    .unwrap();
}
