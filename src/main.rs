use clap::Parser;
use ixa::{
    context::Context, error::IxaError, global_properties::ContextGlobalPropertiesExt,
    random::ContextRandomExt,
};
use parameters_loader::Parameters;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// path to the input file
    #[arg(short, long)]
    input_file: PathBuf,

    /// path to the output directory
    #[arg(short, long)]
    output_directory: PathBuf,
}

mod parameters_loader;

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    // read the global properties, setting them as parameters
    // propagate any errors that may arise with the ? operator
    parameters_loader::init_parameters(&mut context, &args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    // model tidyness -- random seed, automatic shutdown
    context.init_random(parameters.seed);
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
