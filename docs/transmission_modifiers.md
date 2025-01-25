# Overview
Many factors modify the probability of success of an infection attempt of an infectious individual.
These "transmission modifiers" can originate from the natural history of disease (cross-protection
from previous infection or vaccine) or from interventions aimed to reduce the overall probability of
transmission (facemasks). These modifiers can reduce the probability of infection given a reduction
in the transmissibility of the infectious individual, or the susceptibility of the individual being
exposed. Combined, these factors may modify the probability of infection. Transmission modifiers are
not strictly independent, nor is it convenient to repeatedly define their specific use cases, which
would require independent querying that is repetitive and error-prone. This module provides a
framework to incorporate transmission modifiers in a flexible and generalized manner, such that all
modifiers are detected and aggregated when assessing the relative transmission potential of an
infection attempt.

## Hook to transmission manager
In this module, we assume that an interaction between an infectious transmitter (I) and a
susceptible contact (S) has already occured. We alter the [transmission manager](transmission.md)
function `evaluate_transmission` that is conditioned on such an interaction. This is done using the
registered transmission modifiers of both I and S, which can be accessed through the
`query_infection_modifiers` function of the context trait extension
`ContextTransmissionModifierExt`.

```rust
fn query_infection_modifers(&mut self, transmitter_id: PersonId, contact_id: PersonId) -> f64 {
    self.compute_relative_transmission(transmitter_id) * self.compute_relative_transmission(contact_id)
}
```

We assume that the infectiousness of transmitter I is independent of, and therefore additive to, the
risk of infection of contact S.

The API hook to `evaluate_transmission` is independent of innate transmissiveness, which determines
the number and timing of infection attempts made by infectious individual I. We only account for the
`relative_transmission` change that alters probability of `transmission_success` given the attempt
as follows:

```rust
fn evaluate_transmission(context: &mut Context, contact_id: PersonId, transmitter_id: PersonId) {
    if context.get_person_property(contact_id, InfectiousStatus)
        == InfectiousStatusType::Susceptible
    {
        // Set the contact to infectious with probability additive relative transmission.
        if context.sample_bool(TransmissionRng, relative_transmission) {
            context.set_person_property(
                contact_id,
                InfectiousStatus,
                InfectiousStatusType::Infectious,
            );
        }
    }
}
```

# API
## Data plugin and Context trait extension
Transmission modifiers need to register their relative reduction on infectiousness (I) and risk (S).
We use a `HashMap` to relate `InfectiousStatusType` to transmission modifiers with `TypeId`. These
`TypeId` values then map to supplied relative transmission function that takes in `Context` and a
`PersonId`.

To relate multiple modifier functions to one another, we also map `InfectiousStatusType` to a
modifier aggregator function that determines how the transmission modifiers of a particular
`InfectiousStatusType` interact.

```rust
struct TransmissionModifierContainer {
    transmission_modifier_map: HashMap<InfectiousStatusType, HashMap<TypeId, Box<TransmissionModifierFn>>>,
    modifier_aggregator: HashMap<InfectiousStatusType, Box<TransmissionAggregatorFn>>,
}
```

`TransmissionModifierFn` and `TransmissionAggregatorFn` are types defined in the module that return
the `float` from a single modifier effect and from the total effect across modifiers, respectively,
for notational convenience.

```rust
type TransmissionModifierFn = dyn Fn(&Context, PersonId) -> f64;
type TransmissionAggregatorFn = dyn Fn(&Vec<(TypeId, f64)>) -> f64;
```

Other modules will implement particular instances of transmission modifiers and the
`transmission_manager` will call `query_infection_modifiers`, meaning that these modules will
require access to the `TransmissionModifierContainer`. We therefore require a trait extension that
includes the functionality to register modifiers and their aggregators, as well as call an
aggregator to compute the total relative transmission potential change due to the modifications
present for a given `PersonId`.

```rust
trait ContextTransmissionModifierExt {
    fn register_transmission_modifier<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(&mut self, infectious_status: InfectiousStatusType, person_property: T, instance_dict: Vec<(T::Value, f64)>);
    fn register_transmission_aggregator(&mut self, infectious_status: InfectiousStatusType, agg_function: Box<TransmissionAggregatorFn>);
    fn compute_relative_transmission(&mut self, person_id: PersonId) -> f64;
    fn query_infection_modifiers(&mut self, transmitter_id: PersonId, contact_id: PersonId) -> f64;
}
```

## Modifier registration
To `register_transmission_modifier`, the user must specify the `InfectiousStatusType` and
`PersonProperty` of interest. The user then provides a `modifier_key` that associates
`PersonProperty` values to `float`s. The `modifier_key` is then passed into a closure of type
`TransmissionModifierFn` to return the effect of the modifier on relative transmission potential.

```rust
fn register_transmission_modifier<T: PersonProperty + 'static + std::cmp::Eq + std::hash::Hash>(
    &mut self,
    infectious_status: InfectiousStatusType,
    person_property: T,
    modifier_key: &Vec<(T::Value, f64)>,
    ) {
        let mut transmission_modifier_map = HashMap::new();
        transmission_modifier_map.insert(
            TypeId::of::<T>(),
            Box::new(move |context: &mut Context, person_id: PersonId| -> f64 {
                let property_val = context.get_person_property(person_id, person_property);

                for item in modifier_key {
                    if item.0 == property_val {
                        return item.1;
                    }
                }
                return 1.0;
            })
        );

        let transmission_modifier_container = self.get_data_container_mut(TransmissionModifierPlugin);

        // mismatched types: instance_map is of type {closure} but expected {dyn Fn(...)}
        transmission_modifier_container
        .transmission_modifier_map
        .insert(infectious_status, transmission_modifier_map);

}
```

The closure hard-coded into the registration looks up values from the `modifier_key` and is the
simplest case to incorporate direct effects on relative transmission. As an example, consider
including the effect of risk cateogry on susceptiblity. Because we adhere to using a `TypeId` for
mapping, and because we don't want to conflate multiple instances of abstract types, we must define
a public `enum` and associate it with a `PersonProperty`. We then register this function by
providing a vector of values from the `enum` and their associated change effect on relative
transmission. We only do so for the `InfectiousStatusType::Susceptible` case, as `RiskCategory`
would only apply to risk, not infecitousness, in such a module.

```rust
pub enum RiskCategoryType {
    Low,
    Medium,
    High,
};

define_person_property_with_default!(RiskCategory, RiskCategoryType, RiskCategoryType::Low);

...

context.register_transmission_modifier(
    infectious_status: InfectiousStatusType::Susceptible,
    person_property: RiskCategory,
    modifier_key: &vec![(RiskCategoryType::Low, 0.5), (RiskCateogryType::Medium, 0.8)],
    )
```

The `RiskCategoryType::High` does not need to be registered if innate transmissiveness is
parameterized to the susceptibility of the high risk group. The default for the closure reading the
`modifier_key` is 1.0, meaning there is no change in the relative risk for unregistered types. This
functional format is acceptable because we `define_person_property_with_default!`, and don't allow
`PersonId`s without a `RiskCategory` to also default to one. Without a default for the
`PersonProperty`, all individuals would be effectively in the `RiskCateogryType::High` group in this
example. Analogous cases without a default `PersonProperty` value will need to consider this effect
by either treating unregistered cases as a default or by registering all values.

## Computation
### Aggregator functions
In the `TransmissionModifierContainer`, we implement a method for running the aggregator function
for some reference to a `Vec` of transmission modifiers using a specified `InfectiousStatusType`
key. We then assign a default method for the `modifier_aggregator` that assumes each effect is
independent, which is the most common anticipated assumption.

```rust
impl TransmissionModifierContainer {
    fn run_aggregator(&self, infectious_status: InfectiousStatusType, modifiers: &Vec<(TypeId, f64)>) -> f64{
        self.modifier_aggregator
            .get(&infectious_status)
            .unwrap_or(&Self::default_aggregator())
            (modifiers)
    }

    fn default_aggregator() -> Box<TransmissionAggregatorFn> {
        Box::new(|modifiers: &Vec<(TypeId, f64)>| -> f64 {
            let mut aggregate_effects = 1.0;

            for (_, effect) in modifiers {
                aggregate_effects *= effect;
            }

            aggregate_effects
        })
    }
}
```

We define a default aggregator so that the user does not need to specify all or even any
aggregators in order to `run_aggregator` in `compute_relative_transmission`. To register
non-default functions, the user provides the `InfectiousStatusType` and a boxed
`TransmissionAggregatorFn`.

```rust
fn register_transmission_aggregator(
        &mut self,
        infectious_status: InfectiousStatusType,
        agg_function: Box<TransmissionAggregatorFn>
    ) {
    let transmission_modifier_container = self.get_data_container_mut(TransmissionModifierPlugin);

    transmission_modifier_container
    .modifier_aggregator
    .insert(&infectious_status, agg_function);
}
```

### Facemask translation to effect example
From the `default_aggregator`, we can see that all transmission modifier effects directly change
relative infectiousness and risk. We always return `effect = 1.0 - transmission_reduction` within
the `TransmissionModifierFn` of a modifier that isn't parameterized with a direct effect.

This is particularly relevant for interventions, which are frequently parmaeterized in terms of
their efficacy, or relative reduction in transmission potential. For example, if a facemask
transmission modifier is parameterized with the reduction in transmission `facemask_efficacy` upon
wearing a facemask, we ensure that we return the actual effect upon registering the transmission
modifier relative effects.

```rust
pub enum FacemaskStatusType {
    Wearing,
    None,
};

define_person_property_with_default!(FacemaskStatus, FacemaskStatusType, FacemaskStatusType::None);

// Include 1.0 - efficacy from parameters as input to modifier keys
context.register_transmission_modifier(
    infectious_status: InfectiousStatusType::Susceptible,
    person_property: FacemaskStatus,
    modifier_key: &vec![(FacemaskStatusType::Wearing, 1.0 - parameters.facemask_risk_efficacy)],
    )

context.register_transmission_modifier(
    infectious_status: InfectiousStatusType::Infectious,
    person_property: FacemaskStatus,
    modifier_key: &vec![(FacemaskStatusType::Wearing, 1.0 - parameters.facemask_infectious_efficacy)],
    )
```

### Hook to aggregator from PersonId
To calculate the total relative transmission potential change for an individual during an infection
attempt, `Context` and the `PersonId` are supplied to `compute_relative_transmission`, which
accepts a `PersonId` and does not require prior knowledge of their `InfectiousStatusType`.

```rust
fn compute_relative_transmission(&mut self, person_id: PersonId) -> f64 {
    let infectious_status =self.get_person_property(person_id, InfectiousStatus);

    let mut registered_modifiers = Vec::new();
    let transmission_modifier_plugin = self.get_data_container(TransmissionModifierPlugin).unwrap();
    let transmission_modifier_map = transmission_modifier_plugin
        .transmission_modifier_map
        .get(&infectious_status)
        .unwrap();

    for (t, f) in transmission_modifier_map {
        registered_modifiers.push((*t, f(self, person_id)));
    }

    transmission_modifier_plugin.run_aggregator(infectious_status, &registered_modifiers)
}
```

We use automatic iteration through all the modifications applied to a person, which is necessary to
remove multiple manual calls that would otherwise query particular transmission modifiers.

# Modifier scope
Transmission modifiers are not the only module that alter the potential for transmission. They
are strictly defined as data managers that hold the relative effects on transmission from the
properties of individuals interacting during an infection attempt. This is in contrast with the
steps to determine where that infection attempt occurs, who is target by the attempt, and how the
`PersonProperties` are actually assigned.

For example, facemasks modify the relative transmission potential of an infection attempt. The
decision to wear a facemask based on a person's risk category or symptom category is an
intervention-level behavior that does not directly modify relative transmission potential, meaning
that such choices are excluded from this module. In contrast, symptoms may also modify the efficacy
of wearing a facemask, which is a higher order modification that would need to be accounted for in
the overall changes to relative transmission potential caused by facemasks through a
`TransmissionAggregatorFn` defined and registered in the symptoms module.
