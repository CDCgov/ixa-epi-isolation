use ixa::context::Context;
use ixa::define_data_plugin;
use ixa::define_rng;
use ixa::random::ContextRandomExt;
use statrs::distribution::{ContinuousCDF, Exp};
define_rng!(InfectionRandomId);
define_data_plugin!(InfectionPlugin, Vec<f64>, Vec::new());

#[allow(clippy::cast_precision_loss)]
fn schedule_next_infection(
    context: &mut Context,
    generation_interval: impl ContinuousCDF<f64, f64> + 'static,
    last_infection_time_unif: f64,
    infections_remaining: usize,
) {
    // use inverse transform sampling to take a draw from a
    // beta with alpha = 1 (k = 1, minimum value), beta = infections_remaining
    // (number of draws of uniform)
    // which is the distribution of the minimum of n draws of a uniform
    let minimum_uniform_draw = f64::powf(
        context.sample_range(InfectionRandomId, 0.0..1.0),
        1.0 / infections_remaining as f64,
    );

    // draw the next value greater than the current by shrinking the uniform distribution
    // by "what's left"
    let next_infection_time_unif = 1.0 - minimum_uniform_draw * (1.0 - last_infection_time_unif);
    // use the inverse CDF of the generation interval to get the next infection time
    let next_infection_time = generation_interval.inverse_cdf(next_infection_time_unif);
    context.add_plan(next_infection_time, move |context| {
        let time = context.get_current_time();
        context.get_data_container_mut(InfectionPlugin).push(time);
        if infections_remaining > 1 {
            schedule_next_infection(
                context,
                generation_interval,
                next_infection_time_unif,
                infections_remaining - 1,
            );
        }
    });
}

fn run_infection_simulation(
    n_infections: usize,
    generation_interval: impl ContinuousCDF<f64, f64> + 'static,
    seed: u64,
) -> Vec<f64> {
    let mut context = Context::new();
    context.init_random(seed);
    schedule_next_infection(&mut context, generation_interval, 0.0, n_infections);
    context.execute();
    context.get_data_container(InfectionPlugin).unwrap().clone()
}

#[allow(clippy::cast_precision_loss)]
pub fn init() {
    let generation_interval = Exp::new(1.0 / 3.0).unwrap();
    let n_infections = 5;
    let n_simulations = 100; // Generate infection time samples
    let mut infection_times = Vec::new();
    for sim in 0..n_simulations {
        infection_times.append(&mut run_infection_simulation(
            n_infections,
            generation_interval,
            sim,
        ));
        println!(
            "{:?}",
            run_infection_simulation(n_infections, generation_interval, sim,)
        );
    }
    infection_times.sort_by(|a, b| a.partial_cmp(b).unwrap()); // Compare with CDF
    let mut max_cdf_deviation = 0.0;
    let mut gi_cdf_values = Vec::new();
    for (i, infection_time) in infection_times.iter().enumerate() {
        let cdf_value = generation_interval.cdf(*infection_time);
        gi_cdf_values.push(cdf_value);
        max_cdf_deviation = f64::max(
            max_cdf_deviation,
            (cdf_value - (i as f64 / (infection_times.len()) as f64)).abs(),
        );
    }
    println!("{max_cdf_deviation}");
}
