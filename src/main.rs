mod contact;
mod natural_history_manager;
mod parameters;
mod population_loader;
mod transmission_manager;
mod utils;

use clap::Args;
use ixa::runner::run_with_custom_args;
use ixa::{
    Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ContextReportExt,
    IxaError,
};
use std::path::PathBuf;
use transmission_manager::InfectiousStatus;

use crate::parameters::Parameters;
use crate::population_loader::{Age, CensusTract};

#[derive(Args, Debug)]
struct CustomArgs {
    ///Whether force overwrite of output files if they already exist
    #[arg(short = 'f', long)]
    force_overwrite: bool,
}

fn initialize() -> Result<Context, IxaError> {
    let mut context = run_with_custom_args(|context, args, custom_args: Option<CustomArgs>| {
        // Read the global properties.
        let custom_args = custom_args.unwrap();
        // Set the output directory and whether to overwrite the existing file.
        context
            .report_options()
            .directory(PathBuf::from(&args.output_dir))
            .overwrite(custom_args.force_overwrite);
        Ok(())
    })
    .unwrap();

    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    // Set the random seed.
    context.init_random(parameters.seed);

    // Read the generation interval trajectories from a CSV.
    // Eventually, also read the symptom onset/improvement times,
    // and any other disease parameters (like viral load over time) from CSVs.
    natural_history_manager::init(&mut context)?;

    // Set the output directory and whether to overwrite the existing file.
    context
        .report_options()
        .directory(PathBuf::from(&args.output_directory))
        .overwrite(args.force_overwrite);

    // Report the number of people by age, census tract, and infectious status
    // every report_period.
    context.add_periodic_report(
        "person_property_count",
        parameters.report_period,
        (Age, CensusTract, InfectiousStatus),
    )?;

    // Load the synthetic population from the `synthetic_population_file`
    // specified in input.json.
    population_loader::init(&mut context)?;
    context.index_property(Age);
    context.index_property(CensusTract);

    // Initialize the person-to-person transmission workflow.
    transmission_manager::init(&mut context);

    // Add a plan to shut down the simulation after `max_time`, regardless of
    // what else is happening in the model.
    context.add_plan(parameters.max_time, |context| {
        context.shutdown();
    });

    // Print out the parameters for debugging purposes for the user.
    println!("{parameters:?}");

    // Return context for execution.
    Ok(context)
}

fn main() {
    let mut context = initialize().expect("Error initializing.");
    context.execute();
}
