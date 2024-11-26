mod parameters;
mod population_loader;

use clap::Parser;
use ixa::{
    context::Context, error::IxaError, global_properties::ContextGlobalPropertiesExt,
    people::ContextPeopleExt, random::ContextRandomExt, report::ContextReportExt,
};

use parameters::Parameters;
use std::path::PathBuf;

use crate::population_loader::{Age, CensusTract};

use crate::parameters::Parameters;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the input file.
    #[arg(short, long)]
    input_file: PathBuf,

    /// Path to the output directory.
    #[arg(short, long)]
    output_directory: PathBuf,

    #[arg(short, long, default_value = "false", default_missing_value = "true")]
    force_overwrite: bool,
}

mod periodic_report_population;
mod contact;
mod population_loader;
mod transmission_manager;

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    // Read the global properties.
    context.load_global_properties(&args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    // Initialize the base random seed.
    context.init_random(parameters.seed);

    // set the output directory and overwrite the existing file
    context
        .report_options()
        .directory(PathBuf::from(&args.output_directory))
        .overwrite(args.force_overwrite);

    //initialize periodic report
    context.add_periodic_report(
        &parameters.population_periodic_report,
        parameters.report_period,
        (Age, CensusTract),
    )?;

    // load the population from person record in input file
    population_loader::init(&mut context)?;
    context.index_property(Age);
    context.index_property(CensusTract);

    // Initialize person-to-person transmission workflow.
    // This watches for agents going from susceptible to infectious,
    // and schedules transmission events for them according to the
    // disease parameters.
    transmission_manager::init(&mut context);

    // Add a plan to shut down the simulation after the maximum time.
    context.add_plan(parameters.max_time, |context| {
        context.shutdown();
    });

    // Make it easy for the user to see what parameters were loaded and will be
    // used to run the model.
    println!("{parameters:?}");

    // No errors raised: return context for execution.
    Ok(context)
}

fn main() {
    let args = Args::parse();
    let mut context = initialize(&args).expect("Error initializing.");
    context.execute();
}
