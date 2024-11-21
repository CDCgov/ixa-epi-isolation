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
    #[arg(short, long)]
    input_file: PathBuf,
    #[arg(short, long)]
    output_directory: PathBuf,
    #[arg(short, long)]
    person_report: String,
}

mod parameters_loader;

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    parameters_loader::init_parameters(&mut context, &args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    context.init_random(parameters.seed);
    context.add_plan(parameters.max_time, |context| {
        context.shutdown();
    });
    println!("{parameters:?}");
    Ok(context)
}

fn main() {
    let args = Args::parse();
    let mut context = initialize(&args).expect("Error initializing.");
    context.execute();
}
