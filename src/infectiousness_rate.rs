use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, Context, ContextPeopleExt,
    ContextRandomExt, PersonId,
};
use ordered_float::OrderedFloat;

define_person_property_with_default!(TimeOfInfection, Option<OrderedFloat<f64>>, None);
define_person_property_with_default!(InfectiousnessRateId, Option<usize>, None);

pub trait InfectiousnessRate {
    fn get_rate(&self, t: f64) -> f64;
    fn max_rate(&self) -> f64;
    fn max_time(&self) -> f64;
    // There could be other stuff like get_inverse_cdf etc.
}

struct InfectiousnessRateContainer {
    rates: Vec<Box<dyn InfectiousnessRate>>,
}

define_data_plugin!(
    InfectiousnessRatePlugin,
    InfectiousnessRateContainer,
    InfectiousnessRateContainer {
        rates: Vec::new()
    }
);

define_rng!(InfectiousnessRateRng);
pub trait InfectiousnessRateExt {
    fn assign_infection_properties(&mut self, person_id: PersonId);
    fn add_infectiousness_function(&mut self, dist: Box<dyn InfectiousnessRate>);
    fn get_random_infectiousness_function(&mut self) -> usize;
    fn get_infection_rate(&self, index: usize, t: f64) -> f64;
    fn get_max_infection_rate(&self, index: usize) -> f64;
    fn get_max_infection_time(&self, index: usize) -> f64;
}

fn get_fn(context: &Context, index: usize) -> &dyn InfectiousnessRate {
    context
        .get_data_container(InfectiousnessRatePlugin)
        .unwrap()
        .rates[index]
        .as_ref()
}

impl InfectiousnessRateExt for Context {
    // This function should be called from the main loop whenever
    // someone is first infected. It assigns all their properties needed to
    // calculate intrinsic infectiousness
    fn assign_infection_properties(&mut self, person_id: PersonId) {
        let t = self.get_current_time();
        // Right people always get a random one, but we could make this more sophisticated
        let dist_id = self.get_random_infectiousness_function();
        self.set_person_property(person_id, TimeOfInfection, Some(OrderedFloat(t)));
        self.set_person_property(person_id, InfectiousnessRateId, Some(dist_id));
    }
    fn add_infectiousness_function(&mut self, dist: Box<dyn InfectiousnessRate>) {
        let container = self.get_data_container_mut(InfectiousnessRatePlugin);
        container.rates.push(dist);
    }
    fn get_random_infectiousness_function(&mut self) -> usize {
        let max = self
            .get_data_container_mut(InfectiousnessRatePlugin)
            .rates
            .len();
        self.sample_range(InfectiousnessRateRng, 0..max)
    }
    fn get_infection_rate(&self, index: usize, t: f64) -> f64 {
        get_fn(self, index).get_rate(t)
    }
    fn get_max_infection_rate(&self, index: usize) -> f64 {
        get_fn(self, index).max_rate()
    }
    fn get_max_infection_time(&self, index: usize) -> f64 {
        get_fn(self, index).max_time()
    }
}
