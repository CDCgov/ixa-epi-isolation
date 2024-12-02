mod parameters;
mod periodic_report_population;
mod population_loader;

use clap::Parser;
use ixa::{
    context::Context, error::IxaError, global_properties::ContextGlobalPropertiesExt,
    people::ContextPeopleExt, random::ContextRandomExt,
};

use parameters::Parameters;
use std::path::PathBuf;

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

    #[arg(short, long, default_value = "false", default_missing_value = "true")]
    force_overwrite: bool,
}

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    // read the global properties.
    context.load_global_properties(&args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    // model tidyness -- random seed, automatic shutdown
    context.init_random(parameters.seed);

    //initialize periodic report
    periodic_report_population::init(&mut context, &args.output_directory, args.force_overwrite)?;

    // load the population from person record in input file
    population_loader::init(&mut context)?;
    context.index_property(Age);
    context.index_property(CensusTract);

    context.add_plan(parameters.max_time, |context| {
        context.shutdown();
    });
    // make it easy for the user to see what the parameters are
    println!("{parameters:?}");
    // if we've gotten to this point, nothing failed so return
    // context wrapped in Ok so that the user knows nothing failed
    Ok(context)
}

fn main() {
    let args = Args::parse();
    let mut context = initialize(&args).expect("Error initializing.");
    context.execute();
}
