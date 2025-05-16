## Modifiers on total transmission
Before infecting a contact, an infectious individual must have a successfully scheduled infection
forecast and the contact must be identified as a successful infection attempt. Factors that change
the probability of accepting or rejecting either a forecast or scheduled infection attempt can
change over time, such as when an infectious individual changes their use of a facemask over the
course of infection. Modifiers on the total infectiousness of the infector and susceptibility of
the contact therefore need to be identified at runtime.

To include transmission modifiers, the `transmission_modifier_manager.rs` provides the capacity to
calculate a single `float` value, less than or equal to one, that represents the probability of
accepting a forecasst when calculated for an infectious individual and represents the probability
of infecting a contact given an infection attempt for a susceptible individual. These probabilities
are by design independent, as the rejection sampling hooks to the infection propagation loop occur
in two distinct locations (`evaluate_forecast` and `infeciton_attempt`, respectfully).

## Trait extension on `Context`
The `ContextTransmissionModifierExt` trait defines four key methods implemented for `Context` that
handle the storage, retrieval, and manipulation of transmission modifiers.

### `register_transmission_modifier_fn`

This method allows registering a custom function that computes a transmission modifier for a
specific infection status and person property. The function takes a Context and a PersonId as
inputs and returns a f64 value representing the relative modifier.

### `store_transmission_modifier_values`
This method registers a set of predefined modifier values for a specific infection status and
person property. It maps property values to floating-point modifiers (between 0.0 and 1.0) and
ensures that duplicate or invalid values are not allowed.

If a user wants to run an intervention parameterized with the efficacy $p_e$ of that intervention,
such as fitting the probability that a facemask successfully inhibits the transmission potential of
an infectious individual, then declaring that efficacy to `store_transmission_modifier_values` will
require converting it to a probability of forecast acceptance by storing $1.0 - p_e$

This method automatically calls `register_transmission_modifier_fn` and stores a lookup `HashMap`
for associating `PersonProperty` values with modifier `float`s. I nthat way, the user doesn't have
to register a transmission modifier funciton explicitly unless they have a property that is related
to transmission potential through an expression, such as the toy example provided in the tests that
increases the probability of accepting an infection attempt with `Age` of the susceptible contact.

### `register_transmission_aggregator`
This method registers an aggregator function for a specific infection status. The aggregator
combines multiple modifier functions into a single value, with the default behavior being the
product of all outputs from each modifier function.

### `get_modified_relative_total_transmission_person`
This method calculates the total modified transmission for a person by applying all registered
modifiers and aggregating their effects.

This function is called twice in the infection propagation loop, once during `evaluate_forecast`
to determine if a `Forecast` is accepted or not by modigfying the total infectiousness scale
factor, and once during `infection_attempt`, to determine if a contact is accepted as a successful
infection.
