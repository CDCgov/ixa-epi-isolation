mod contact;
mod parameters;
mod population_loader;
mod transmission_manager;

use clap::Parser;
use ixa::{
    context::Context, error::IxaError, global_properties::ContextGlobalPropertiesExt,
    people::ContextPeopleExt, random::ContextRandomExt, report::ContextReportExt,
};
use std::path::PathBuf;

use crate::parameters::Parameters;
use crate::population_loader::{Age, CensusTract};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// path to the input file
    #[arg(short, long)]
    input_file: PathBuf,

    /// path to the output directory
    #[arg(short, long)]
    output_directory: PathBuf,

    /// whether force overwrite of output files if they already exist
    #[arg(short, long, default_value = "false", default_missing_value = "true")]
    force_overwrite: bool,
}

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    // Read the global properties.
    context.load_global_properties(&args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    // Set the random seed.
    context.init_random(parameters.seed);

    // Set the output directory and whether to overwrite the existing file.
    context
        .report_options()
        .directory(PathBuf::from(&args.output_directory))
        .overwrite(args.force_overwrite);

    // Report the number of people by age and census tract every report_period.
    context.add_periodic_report(
        "person_property_count",
        parameters.report_period,
        (Age, CensusTract),
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

    // Return context for exectuion
    Ok(context)
}

fn main() {
    let args = Args::parse();
    let mut context = initialize(&args).expect("Error initializing.");
    context.execute();
}
