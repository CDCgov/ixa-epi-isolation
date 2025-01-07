# Overview
Infection attempts are not always successful (i.e. result in transmission). This module provides a framework to incorporate the many factors that modify the probability of infection attempt success. In this module, we assume that an interaction between an infectious transmitter (I) and a susceptible contact (S) has already occured and we want to `evaluate_transmission` using the registered modifiers of both I and S.

```rust
fn evaluate_transmission(context: &mut Context, contact_id: PersonId, transmitter_id: PersonId) {
    if context.get_person_property(contact_id, InfectiousStatus)
        == InfectiousStatusType::Susceptible
    {
        let relative_transmission = query_modifiers(&context, transmitter_id, contact_id)
        let transmission_success =
            context.sample_range(TransmissionRng, 0.0..1.0) < relative_transmission;

        // Set the contact to infectious with probability additive relative transmission.
        if transmission_success {
            context.set_person_property(
                contact_id,
                InfectiousStatus,
                InfectiousStatusType::Infectious,
            );
        }
    }
}
```

The API hook to `evaluate_transmission` is independent of innate transmissiveness, which determines the number and timing of infection attempts made by infecitous individual I. 

## Modifier scope
In this module, we ignore behaviors and modifications not directly relevant to changing the transmission potential of an infection attempt. The separation of contact selection and contact rates is apparent, but other distinctions may be less clear.
For example, facemasks modify the relative transmission potential of an infection attempt. The decision to wear a facemask based on a person's risk category or symptom category is an intervention-level behavior that does not directly modify relative transmission potential, meaning that such choices are excluded from this module. In contrast, symptoms may also modify the efficacy of wearing a facemask, which is a higher order modification that would need to be accounted for in the changes to relative transmission potential caused by facemasks. 
Compare this higher order modification to the instance in which an individual may be less likely to wear a mask at home, or may wear it for less time. This is a modifier created by the location of the infection attempt, and is thus separate from the relative transmission modifiers module.

# API
## InterventionContainer
In order to obtain the total effect of all transmission modifiers acting on the relative transmission potential between I and S during an infection attempt, we need to map relative infectiousness and risk, respectively, to `PersonProperty` values. We are concerned with two categories of transmission modifiers: values stored in the `enum` underlying a `PersonProperty` that map to a float directly, and functions of a `PersonId` and `context` state that return a float. Because the former is a special case of the latter, we store both in a nested `HashMap` of a data container

```rust
struct InterventionContainer {
    intervention_map: HashMap<InfectiousStatusType, HashMap<TypeId, Box<InterventionFn>>>,
    aggregator: HashMap<InfectiousStatusType, Box<AggregatorFn>>,
}
```

`InterventionFn` and `AggregateFn` are types defined in the module that return the `float` from a single modification effect and from the total effect across modifications, respectively. 

```rust
type InterventionFn = dyn Fn(&Context, PersonId) -> f64;
type AggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;
```

In the `InterventionContainer`, we implement a method for running the aggregator function for some reference to a `Vec` of interventions using a specified `InfectiousStatusType` key.
We then want to assign a default method for the `aggregator` that assumes each effect is independent, which is the most common anticipated assumption.

```rust
impl InterventionContainer {
    fn run_aggregator(&self, infectious_status: InfectiousStatusType, interventions: &Vec<(TypeId, f64)>) -> f64{
        self.aggregator
            .get(&infectious_status)
            .unwrap_or(&Self::default_aggregator())
            (interventions)
    }

    fn default_aggregator() -> Box<AggregatorFn> {
        Box::new(|interventions: &Vec<(TypeId, f64)>| -> f64 {
            let mut aggregate_effects = 1.0;
    
            for (_, effect) in interventions {
                aggregate_effects *= effect;
            }
    
            aggregate_effects
        })
    }
}
```

Finally ,we `define_data_plugin!` to use the data container with `Context`.

## Registration and computation
To connect the `InterventionContainer` to the current `Context` state, we require a trait extension that builds in functionality to register interventions and their aggregators, as well as call the aggregator to compute the total relative transmission potential change due to the modifications for a given individual.

```rust
trait ContextTransmissionModifierExt {
    fn register_intervention<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(&mut self, infectious_status: InfectiousStatusType, person_property: T, instance_dict: Vec<(T::Value, f64)>);
    fn register_aggregator(&mut self, agg_functions: Vec<(InfectiousStatusType, Box<AggregatorFn>)>);
    fn compute_intervention(&mut self, person_id: PersonId) -> f64;
}
```

To `register_intervention` for some instance of a modifier, the user must specify the `InfectiousStatus` and `PersonProperty` of interest and then provide an `instance_dict` that associates `PersonProperty` values to `float`s. These inputs are supplied to a default closure that obtains the realtive trasnmission potential modification through direct value association. We will want to abstract out this closure so that a generic function of type `InterventionFn` can be supplied instead.

```rust
fn register_intervention<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(&mut self, infectious_status: InfectiousStatusType, person_property: T, instance_dict: Vec<(T::Value, f64)>) {

    let mut instance_map = HashMap::new();
    instance_map.insert(TypeId::of::<T>(), move |context: &mut Context, person_id| -> f64 {
        let property_val = context.get_person_property(person_id, person_property);
        
        for item in instance_dict {
            if property_val == item.0 {
                return item.1;
            }
        } 
        // Return a default 1.0 (no relative change if unregistered)
        return 1.0;
    });

    let intervention_container = self.get_data_container_mut(InterventionPlugin);

    // mismatched types: instance_map is of type {closure} but expected {dyn Fn(...)}
    intervention_container
    .intervention_map
    .insert(infectious_status, instance_map);

}
```

To register the `aggregator` functions, because we only can only have as many aggregators as there are `InfectiousStatusType` values, the user provides a `Vec` of all aggregator functions desired to the register function. 

```rust
fn register_aggregator(&mut self, agg_functions: Vec<(InfectiousStatusType, Box<AggregatorFn>)>) {
    let intervention_container = self.get_data_container_mut(InterventionPlugin);

    for item in agg_functions {
        intervention_container
        .aggregator
        .insert(item.0, item.1);
    }
}
```

We already defined the default aggregator through `impl InterventionContainer` so that the user does not need to specify all or even any aggregators in order to `run_aggregator` in `compute_intervention`. 

To calculate the total relative trasnmission potential change for an individual during an infection attempt, `Context` and the `PersonId` are supplied to `compute_intervention`, which is agnostic of the `InfectiousStatus` of the `PersonId` supplied.

```rust
fn compute_intervention(&mut self, person_id: PersonId) -> f64 {
    let infectious_status =self.get_person_property(person_id, InfectiousStatus);

    let mut registered_interventions: Vec<(TypeId, f64)> = Vec::new();
    let intervention_plugin = self.get_data_container(InterventionPlugin).unwrap();
    let intervention_map = intervention_plugin
        .intervention_map
        .get(&infectious_status)
        .unwrap();

    for (t, f) in intervention_map {
        registered_interventions.push((*t, f(self, person_id)));
    }

    intervention_plugin.run_aggregator(infectious_status, &registered_interventions)
}
```
We use automatic detection of all the modifications applied to a person, which is necessary to remove multiple manual calls to query particular interventions that would otherwise be error-prone and inflexible.

## Querying
We provide the simple public function to query all the relative transmission modifiers for both S and I during the infection attempt

```rust

pub fn query_modifers(context: &Context, transmitter_id: PersonId, contact_id: PersonId) -> f64 {
    context.compute_intervention(transmitter_id) * context.compute_intervention(contact_id)
}
```
