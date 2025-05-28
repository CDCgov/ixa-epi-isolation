## Modifiers on total transmission
Before infecting a contact, an infectious individual must have a successfully scheduled infection
forecast and the contact must be identified as a potential infectee. Factors that change
the probability of that someone will become infected during a scheduled infection attempt can
change over time, such as when an infectious individual changes their use of a facemask over the
course of infection. Modifiers on the total infectiousness of the infector and susceptibility of
the contact therefore need to be accounted for during the infection propagation loop.

To include transmission modifiers, the `transmission_modifier_manager.rs` provides the capacity to
calculate a single `float` value, less than or equal to one. This singular value, depending on the
person it is calculated for, can either represent the probability of accepting a forecast for an
infectious individual or represent the probability of infecting a contact given an infection
attempt for a susceptible individual. These probabilities are by design independent, as the
rejection sampling hooks to the infection propagation loop occur in two distinct locations
(`evaluate_forecast` and `infeciton_attempt`, respectfully).

## Trait extension on `Context`
The `ContextTransmissionModifierExt` trait defines four key methods implemented for `Context` that
handle the storage, retrieval, and manipulation of transmission modifiers.

### `store_transmission_modifier_values`
This method registers a set of predefined modifier values for a specific infection status and
person property. This method accepts a map of property values to floating-point modifiers (between
0.0 and 1.0) and ensures that duplicate or invalid values are not allowed.

All values default to 1.0 unless specified otherwise because we are concerned with relative
transmission potential as compared to the base case, which is the maximum possible total
infectiousness.

This method automatically calls `register_transmission_modifier_fn` and stores a lookup `HashMap`
for associating `PersonProperty` values with modifier `float`s. Thus, the user doesn't have
to register a transmission modifier function explicitly.

If a user wants to run an intervention parameterized with the efficacy $p_e$ of that intervention,
(e.g. the probability that a facemask successfully inhibits an individual's infectgiousness) then
declaring that efficacy to `store_transmission_modifier_values` will require converting it to a
probability of forecast acceptance by storing $1.0 - p_e$.

For example, in order to register the impact of facemasks for infectious individuals using
the efficacy of preventing infection, one would call

```rust
context
    .store_transmission_modifier_values(
        InfectionStatusValue::Infectious,
        MaskingStatus,
        &[(Masking::Wearing, 1.0 - facemask_efficacy)],
    )
    .unwrap();
```

In this case, calling `get_relative_total_transmission` on someone who is
`Susceptible` or somone who is `Infectious` but is not currently wearing a facemask will yield
an unmodified transmission potential (susceptibility or infectiousness multiplied by 1.0).pre-commit -m

### `register_transmission_modifier_fn`
This method allows registering a custom function that computes a transmission modifier for a
specific infection status and person property. The function takes `Context` and the `PersonId` as
inputs and returns a f64 value representing the relative modifier.

This generalized method is exposed to the user in case an intervention is related to transmission
potential through an expression, such as the toy example provided in the tests that increases the
probability of accepting an infection attempt with `Age` of the susceptible contact. In most use
cases, however, the intervention will be a look up key registered implicitly when calling
`store_transmission_modifier_values`.

### `register_transmission_modifier_aggregator`
Modifier functions each return an effect on relative transmission potential, which have to be
combined in some way to yield a single float value for relative total infectiousness or
susceptibility. Therefore, the user will need to specify a way to aggregate a vector of modifier
outputs into a single effect.

This method registers an aggregator function for a specific infection status. The aggregator
combines multiple modifier functions into a single value, with the default behavior being the
product of all outputs from each modifier function.

### `get_relative_total_transmission`
This method calculates the total modified transmission for a person by applying all registered
modifiers and aggregating their effects according to the registered aggregator.

This function is called twice in the infection propagation loop, once during `evaluate_forecast`
to determine if a `Forecast` is accepted or not by modifying the total infectiousness scale
factor, and once during `infection_attempt`, to determine if a contact is accepted as a successful
infection.
