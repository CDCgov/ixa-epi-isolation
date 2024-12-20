# Overview
Multiple factors determine the probability that an infection attempt is successful
during a given interaction. Although some of these factors are due to intrinsic 
transmitter infectiousness or contact susceptibility/risk, many factors are extrinsic 
modifiers that result in a relatively higher or lower transmission potential. In the 
`infection_manager.rs` module, we provide tools to register the effects of 
interventions that aim to lower realtive transmission, query the aggregate impact of 
these effects for particular individuals, and return these values through calls in 
`transmission_manager.rs` to modify transmission during infection attempts. 

Interventions that are aimed to lower the relative transmission potential of an 
infection attempts can do so by altering either infectiousness or risk, which are 
rarely symmetrical effects. Further, interventions can be represented in a 
`PersonProperty` or can be derived from current `context` state querying a particular 
`PersonId`. For example, the status of wearing a facemask may be modeled as a 
`PersonProperty` whereas individual vaccine efficacy may be a function of a person's 
time since vaccination. All of these effects are registered to some type ID and to 
`InfectiousStatusType`. 

In order to supply the transmission manager with these different modifications to 
transmission potential, the infection manager automatically detects all of the 
interventions relevant to a given person. These effects are combined additively by 
default, but may be changed through user-defined functions that alter the interaction 
between registered intervention effects. 

# API
## Register
In order to register a particular intervention, we need to relate an 
`InfectiousStatusType` and a generic to a float transmission modifier value. We 
access these values through a data plugin from `InterventionContainer` that assumes 
the generic has been Debugged to `String` and that all types are preceded by unique 
identifiers so that values don't overlap when calling the `String`.

```rust
struct InterventionContainer {
    intervention_map: HashMap<InfectiousStatusType, HashMap<String, f64>>,
}
```

These pieces then are provided to the register function which is a trait extension 
on context that generates (or modifies) an `InterventionPlugin`.

```rust
trait ContextInterventionExt {
    fn register_intervention<T: PersonProperty + std::fmt::Debug>(
        &mut self,
        t: T,
        infectious_status: InfectiousStatusType,
        i: impl Intervention<T::Value>,
    );
}
```

In order to register the nested `HashMap`, we `impl` a trait extension `Intervention` 
for vectors of generic, float Tuples. This function then returns the mapped type 
values.

```rust
trait Intervention<T> {
    fn map_intervention(self, property: String) -> HashMap<String, f64>;
}

impl<T: std::fmt::Debug> Intervention<T> for Vec<(T, f64)> {...}
```

## Query
Automatic detection of all the interventions applied to a person is necessary to remove multiple manual calls to query particular interventions, which would be error-prone and inflexible. To do this, we add

```rust
trait ContextInterventionExt {
    fn touch_registered_types(self) -> Vec<(TypeId, f64)>;
    fn register_intervention_calculator(self, f: Fn(Vec<(TypeId, f64)>) -> f64) -> f64
}
```

which access the stored current interventions and connect the aggregation calculator

