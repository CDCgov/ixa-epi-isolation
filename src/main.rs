use std::path::PathBuf;

use ixa::{define_global_property, run_with_args, Context};

pub mod infectiousness_rate;
mod infection_propagation_loop;
mod infectiousness_setup;
mod population_loader;
pub mod settings;
pub mod contact;

define_global_property!(SynthPopulationFile, PathBuf);
define_global_property!(InitialInfections, usize);
define_global_property!(PopulationSize, usize);
define_global_property!(HouseholdSize, usize);
define_global_property!(RecoveryTime, f64);
define_global_property!(GlobalTransmissibility, f64);
define_global_property!(Alpha, f64);

fn main() {
    run_with_args(|context: &mut Context, args, _| {
        if args.config.is_none() {
            panic!("You need to run the model with a config file, for example `cargo run -- --config src/params.json`");
        }
        population_loader::init(context)?;
        infectiousness_setup::init(context);
        infection_propagation_loop::init(context);
        Ok(())
    })
    .unwrap();
}
