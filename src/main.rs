use clap::Parser;
use ixa::{
    context::Context, error::IxaError, global_properties::ContextGlobalPropertiesExt,
    random::ContextRandomExt,
};
use std::path::PathBuf;

mod parameters;
use parameters::Parameters;
use parameters::ParametersValues;
mod population_loader;

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

mod periodic_report_population;
mod population_loader;

fn initialize(args: &Args) -> Result<Context, IxaError> {
    let mut context = Context::new();
    // read the global properties.
    context.load_global_properties(&args.input_file)?;
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();
    validate_inputs(&parameters)?;
    // model tidyness -- random seed, automatic shutdown
    context.init_random(parameters.seed);

    //initialize periodic report
    periodic_report_population::init(&mut context, &args.output_directory)?;

    // load the population from person record in input file
    population_loader::init(&mut context)?;

    context.add_plan(parameters.max_time, |context| {
        context.shutdown();
    });

    // load the population from person record in input file
    population_loader::init(&mut context)?;

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

#[cfg(test)]
mod test {
    use super::*;

    use crate::parameters::ParametersValues;

    #[test]
    fn test_validate_r_0() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: -1.0,
            infection_duration: 5.0,
            generation_interval: 5.0,
            report_period: 1.0,
            synth_population_file: ".".to_string(),
        };
        assert!(validate_inputs(&parameters).is_err());
    }

    #[test]
    fn test_validate_gi() {
        let parameters = ParametersValues {
            max_time: 100.0,
            seed: 0,
            r_0: 2.5,
            infection_duration: 5.0,
            generation_interval: 0.0,
            report_period: 1.0,
            synth_population_file: ".".to_string(),
        };
        assert!(validate_inputs(&parameters).is_err());
    }
}
