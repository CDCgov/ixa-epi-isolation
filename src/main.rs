mod contact;
mod parameters;
mod population_loader;
mod transmission_manager;

use ixa::runner::run_with_args;
use ixa::{
    ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt,
    ContextReportExt,
};
use transmission_manager::InfectiousStatus;

use crate::parameters::Parameters;
use crate::population_loader::{Age, CensusTract};

fn main() {
    
    run_with_args(|context, args, _| {
        // Read the global properties.
        // Set the output directory and whether to overwrite the existing file.
        context
            .report_options()
            .directory(args.output_dir.unwrap())
            .overwrite(args.force_overwrite);

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

        // Initialize the person-to-person transmission workflow.
        transmission_manager::init(context);

        // Add a plan to shut down the simulation after `max_time`, regardless of
        // what else is happening in the model.
        context.add_plan(parameters.max_time, |context| {
            context.shutdown();
        });

        // Print out the parameters for debugging purposes for the user.
        println!("{parameters:?}");
        Ok(())
    }).unwrap();
}

