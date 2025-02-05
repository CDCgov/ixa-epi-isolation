use ixa::{
    define_data_plugin, define_person_property_with_default, define_rng, Context, ContextPeopleExt,
    ContextRandomExt, PersonId,
};

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
    InfectiousnessRateContainer { rates: Vec::new() }
);

define_rng!(InfectiousnessRateRng);
pub trait InfectiousnessRateExt {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRate>);
    fn assign_random_rate_fn(&mut self, person_id: PersonId);
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRate;
}

fn get_fn(context: &Context, index: usize) -> &dyn InfectiousnessRate {
    context
        .get_data_container(InfectiousnessRatePlugin)
        .unwrap()
        .rates[index]
        .as_ref()
}

impl InfectiousnessRateExt for Context {
    fn add_rate_fn(&mut self, dist: Box<dyn InfectiousnessRate>) {
        let container = self.get_data_container_mut(InfectiousnessRatePlugin);
        container.rates.push(dist);
    }
    fn assign_random_rate_fn(&mut self, person_id: PersonId) {
        let max = self
            .get_data_container_mut(InfectiousnessRatePlugin)
            .rates
            .len();
        let index = self.sample_range(InfectiousnessRateRng, 0..max);
        self.set_person_property(person_id, InfectiousnessRateId, Some(index));
    }
    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRate {
        let index = self
            .get_person_property(person_id, InfectiousnessRateId)
            .unwrap();
        get_fn(self, index)
    }
}
