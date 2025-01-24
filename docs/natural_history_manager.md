# Modeling with individual-level natural history parameters

## Overview

An agent-based model (ABM) of human-to-human disease transmission requires natural history
parameters. For instance, the transmission module requires the disease generation interval to
schedule when an infected agent has their infection attempts; the clinical health status module
requires the incubation period to schedule when an infected agent becomes symptomatic; and the
testing module requires the respiratory viral load at a given time since infection to evaluate
whether an agent may test positive at a given time. Here, we introduce the `natural_history_manager`
module which provides an interface for easily specifying natural history parameters and methods
that enable computing derived quantities relevant to modeling disease at the individual-level from
these parameters.

## Types and traits provided by the natural history manager

Unlike most managers that provide a trait extension on `Context` with methods that other modules
call, this manager's main contribution is different natural history parameter types on which traits
have been implemented that enable computing quantities relevant to modeling disease at the
individual-level. This manager also provides a trait extension on `Context` called the
`ContextNaturalHistoryExt` that contains convenience methods for common multi-step calculations on
natural history parameters. The custom types defined in this module are all deserializable, so the
user can specify natural history parameters in an input JSON file, taking advantage of the existing
global properties plugin's infrastructure (in particular, parameter validation at read-in time). We
focus on having these types be as generic as possible, so that the user can change the value (for
instance, changing an exponential distribution to a gamma distribution) without impacting the
parameter type or the way it is used in the code.

### Time-invariant parameters: An example from specifying the disease incubation period

The first type we define is `ProbabilityDistribution`. At the most general level, any natural
history parameter that does not vary over time (i.e., there is one value of that parameter drawn for
the entirety of an agent's infection) is a sample from a probability distribution. We want a type
that allows the user to specify the distributional shape for a parameter as a variant of an
overarching type:

```rs:natural_history_manager.rs
#[derive(Deserialize)]
pub enum ProbabilityDistribution {
    Exp(f64),
    Gamma(f64, f64),
    Empirical(Vec<f64>),
    // ... Because this is an enum, users can add other distribution variants.
}
```

We can also add a similar type called `ProbabilityDistributionDiscrete` that is the same but only
for discrete distributions (`Poisson`, `NegBin`, `Empirical(Vec<usize>)`).

A user may specify time-invariant natural history parameters in the input JSON as follows:

```json
{
    "epi_isolation.Parameters": {
        "incubation_period": "Gamma(1.0, 3.0)",
        "time_to_symptom_improvement": "Empirical([1.0, 2.0])",
        "R_0": "Empirical([3])"
    }
}
```

In the case of the distribution for $R_0$, we are saying that all agents have the same value of
$R_0$ -- there is no stochasticity or person-to-person variation in $R_0$. Nevertheless, because we
have defined the `ProbabilityDistributionDiscrete` type broadly, we can consider this special case
within our defined parameter types. For our `ProbabilityDistribution` type, we implement Rust's
`rand` crate's `Distribution` trait. This enables us to draw random samples from the distribution
using Ixa's built-in `ContextRandomExt` that expects a type that implements `Distribution<T>` for
sampling. This lets us treat our custom enum like we natively defined the distribution type in Rust
without going through our enum first. To reduce confusion, we adopt that the meaning of parameters
in our custom `ProbabilityDistribution` type is the same as in the `statrs` distribution of the same
name.

```rs:natural_history_manager.rs
use rand::distribution::Distribution
use statrs::distribution::{Exp, Gamma, Empirical}

impl Distribution<f64> for ProbabilityDistribution {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> f64 {
        match self {
            ProbabilityDistribution::Exp(lambda) => rng.sample(Exp::new(lambda).unwrap()),
            ProbabilityDistribution::Gamma(a, b) => rng.sample(Gamma::new(a, b).unwrap()),
            ProbabilityDistribution::Empirical(vals) => rng.sample(Empirical::from_iter(vals).unwrap())
        }
    }
}
```

The user can also implement their own traits on the `ProbabilityDistribution` type if they needed
to do other computations like getting the mean or variance of the distribution.

To draw a random sample in a module with this type, say to draw the time at which to set the
person's health status to symptomatic once the agent becomes infected based on the incubation period
(which we assume is specified in the input JSON file as described above and set as a global
property), the user would do the following:

```rs:health_status_manager.rs
fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
   ) {
    if event.current == InfectiousStatusType::Infectious {
        context.set_person_property(
            event.person_id,
            HealthStatus,
            HealthStatusType::Presymptomatic);

        let incubation_per = context.get_global_property_value(Parameters).unwrap()
                                .incubation_period;
        let incubation_time = context.sample_distr(HealthStatusRng, incubation_per);
        context.add_plan(
            context.get_current_time() + incubation_time,
            move |context| {
                context.set_person_property(
                    event.person_id,
                    HealthStatus,
                    HealthStatusType::Symptomatic);
            },
        );
    }
}
```

The modular structure lends itself to being able to easily change the underlying distribution type
(i.e., switching out a gamma for, say, a lognormal) in the input JSON with no change to the code and
only needing to make sure the new distribution can be coerced into a Rust type that implements the
`Distribution` trait. The incubation period distribution could even be updated in the middle of the
simulation by just changing the global properties! Our `ProbabilityDistribution` type handles many
use cases for natural history parameters and more broadly being able to specify and sample from
arbitrary distributions in our ABMs. This type also serves as the building block for the next
natural history parameter type we define: time-varying parameters.

### Time-varying parameters: An example from specifying the respiratory viral load

Time-varying parameters are those where the value of a natural history parameter changes
over time, such as the respiratory viral load which changes over time since infection. In the most
general sense, a time-varying parameter is a set of distributions each associated with a time:

```rs:natural_history_manager.rs
#[derive(Deserialize)]
pub struct TimeVaryingParameter (
    pub Vec<f64>,
    pub Vec<ProbabilityDistribution>
)
```

Concretely, a user may specify a time-varying parameter in their input JSON as follows:

```json
{
    "epi_isolation.Parameters": {
        "respiratory_viral_load": "TimeVaryingParameter([0.0, 1.0, 2.0], [Empirical[3.0], Gamma(2.0, 5.0), Exp(1.0)])"
    }
}
```

Because our custom `TimeVaryingParameter` type builds off our existing infrastructure for the
`ProbabilityDistribution` type, we hold a more general idea of a time-varying parameter than is
generally implemented in most ABMs: concretely, we allow for the value of a time-varying parameter
at a given time to itself be a random distribution rather than being a fixed value. However, based
on what we have introduced so far, this type alone does not allow for the user to specify viral load
_trajectories_ (i.e., one agent is assigned an entire set of deterministic viral loads and another
agent has a different specified set). While a user could effectively hijack this current type to do
that by implementing their own sampling method that provides the desired behavior, we devise a
general solution to this problem below exploiting infrastructure we have to build to allow for
correlated parameter values.

With a time-varying parameter, we want a method to get the value of the parameter at a given time.
This process requires more steps, so we define a convenience method in a trait extension on
`Context` that helps us do this. Because we will soon define other cases of time-varying parameters,
we make these methods take any type that implement a trait we define called `LinearInterpolation`
(described below).

```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn estimate_parameter_at_value(
        &mut self,
        person: PersonId,
        p: &impl LinearInterpolation,
        value: f64) -> f64 {
            self.estimate_parameter_at_time_greater(p, person, time, 0)
        }

    fn estimate_parameter_at_value_greater(
        &mut self,
        person: PersonId,
        p: &impl LinearInterpolation,
        value: f64,
        last_index_used: usize) -> f64
}
```

We explain [below](#allowing-for-correlation-between-parameter-values) why we require the person as
an argument and why we must take a mutable reference to `Context` -- in brief, this is so that we
can allow for correlations between parameter values.

At a high-level, this method finds the nearest times and indeces both less than and greater than the
provided `time` from the time vector in the time-varying parameter (implemented as part of the
`LinearInterpolation` trait), draws samples from the corresponding distributions at those times
(leveraging our existing infrastructure for the `ProbabilityDistribution` type), and takes the
average of the sampled values weighted by distance from the samples' times to the given time. These
implementation details are abstracted from the calling module by being wrapped up into the two
estimation methods presented above. However, there is one implementation detail we expose to the
user. In many cases, we estimate the value of a time-varying parameter at always increasing times
(i.e., when we require an agent's viral load and their time since infection is always increasing).
If we store the index of the last value at which the time-varying parameter was queried, we know
that our current value of that parameter must be at a greater index. We can use this trick to speed
up the process of searching for indeces.

There is a special case of a time-varying parameter: one that is invertible, meaning that we can
instead run an estimation routine on the inverse of the parameter, such as estimating the time at
which the viral load (or any other time-varying parameter) equals a given value (rather than
estimating the viral load at a given time). While use cases for this type on its own are rare, it
is critical for allowing different ways of specifying the generation interval (described in the
[next section](#implementing-new-methods-on-subtypes-an-example-from-using-the-disease-generation-interval)).

```rs:natural_history_manager.rs
#[derive(Deserialize)]
pub struct InvertibleTimeVaryingParameter (
    pub Vec<f64>,
    pub Vec<f64>
)
```

For this type, we implement the `LinearInterpolation` trait described above that contains the same
functions to enable us to grab the indeces of the values in the first vector in the tuple that
window a provided value, and we implement a new method that returns the inverse tuple.

```rs:natural_history_manager.rs
impl InvertibleTimeVaryingParameter {
    pub fn invert(&self) -> InvertibleTimeVaryingParameter {
        InvertibleTimeVaryingParameter(self.1.clone(), self.0.clone())
    }
}
```

### Implementing new methods on subtypes: An example from using the disease generation interval

One particular natural history parameter that is critical to almost all disease models and which
requires in-depth manipulation to obtain values relevant to modeling disease at the individual-level
is the disease generation interval. First, the generation interval can be specified in multiple
ways: in a simple disease model, the user may want to simply say the generation interval follows
a given distribution (exponential or gamma), allowing for direct recovery of a differential equation
compartmental model. In more complicated models, the user may want to directly specify the
generation interval as a set of values. We create an enum called `GenerationInterval` which contains
different variants to represent the multiple ways a user may specify the generation interval.

```rs:natural_history_manager.rs
#[derive(Deserialize)]
pub enum GenerationInterval {
    Distribution(ProbabilityDistribution, ProbabilityDistributionDiscrete),
    LengthAndMagnitude(ProbabilityDistribution, ProbabilityDistribution, ProbabilityDistributionDiscrete),
    Cdf(InvertibleTimeVaryingParameter, ProbabilityDistributionDiscrete),
    NormalizedHazard(InvertibleTimeVaryingParameter, ProbabilityDistributionDiscrete),
    AbsoluteHazard(InvertibleTimeVaryingParameter)
}
```

By convention, when modelers say "the generation interval is exponential", that means that the
generation interval _length_ is exponential and the magnitude over time is constant. We adopt the
same shorthand (the first enum variant listed above) and then a more complete option that allows the
user to separately specify the length and magnitude separately (the second option). In both cases,
the distribution for $R_0$ must also be specified, and that is the final parameter in each of these
variants (a discrete distribution). By tethering the generation interval and $R_0$ to be specified
together, we are ensuring that if the `GenerationInterval` variant mode picked by the user requires
a distribution for $R_0$, they actually must provide it. If we were to separately specify the $R_0$
distribution as its own optional natural history parameter, we would have to check for the existence
of that parameter (assuming a consistent name for it) when the model is validating its parameters,
deferring the error to later and making assumptions about code structure.

When the user cannot confine the generation interval to an analytical distribution (i.e., they have
empirical samples of the distribution over time), we allow multiple ways to specify the generation
interval -- as a CDF (requiring an invertible time varying parameter and a distribution of $R_0$),
a normalized hazard function which requires the same inputs, an absolute hazard function which does
not require a separate distribution of $R_0$ because it quantifies _absolute_ hazard, etc.
Regardless of the form of the generation interval, we use it to estimate the time to the next
infection attempt. We implement the methods required to conduct this calculation in the trait
`GenerationIntervalComputations` and we provide an implementation of this trait on the
`GenerationInterval` type. Because there are multiple steps in this process, we define a method in
the `ContextNaturalHistoryExt` called `time_to_next_infection_attempt` that conducts this routine
according to our algorithm for calculating the
[time to the next infection attempt](./time_varying_infectiousness.md).

```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn time_to_next_infection_attempt(
        &mut self,
        gi: &impl GenerationIntervalComputations,
        person: PersonId) -> Option<f64>;
}
```

We require the person because the time to the next infection attempt relies on person properties
like the agent's last infection time, number of secondary infection attempts remaining, etc. We
adopt the convention that the method returns `None` if there are no more infection attempts. The
method would be used in the transmission manager as follows:

```rs:transmission_manager.rs
fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
   ) {
    if event.current == InfectiousStatusType::Infectious {
        schedule_next_infection_attempt(
            context,
            event.person_id);
    }
}

fn schedule_next_infection_attempt(
    context: &mut Context,
    transmitter_id: PersonId,
   ) {
    let gi = context.get_global_property_value(Parameters).unwrap().generation_interval;
    if let Some(delta_time) = context.time_to_next_infection_attempt(gi, event.transmitter_id) {
        context.add_plan(
            context.get_current_time() + delta_time,
            move |context| {
                infection_attempt(context, transmitter_id)
                    .expect("Error finding contact in infection attempt");
                schedule_next_infection_attempt(
                    context,
                    transmitter_id,
                );
            },
        );
    }
}
```

By abstracting the computation for getting the next infection attempt time into the natural history
manager, the transmission manager's syntax is focused entirely on the person-to-person transmission
workflow, independent of the mathematical calculations to obtain the times. This syntax points more
clearly to what the transmission manager is actually doing and helps improve modularity between the
mathematical details and the logic flow of the model. At a high level, our
`time_to_next_infection_attempt` method uses [order statistics](./time_varying_infectiousness.md)
to calculate the time to the next infection attempt. The method:

1. Returns `None` if there are no infection attempts remaining.
2. Draws the next ordered uniform value based on the number of infection attempts remaining.
3. Estimates the time at which the next infection attempt occurs by passing the uniform value
through the inverse generation interval CDF.

Internally, this method keeps track of:
1. The agent's remaining number of secondary infection attempts (or, if the absolute hazard function
was specified, remaining absolute infectiousness). Because our structure for the generation interval
is coupled to the number of secondary infection attempts, the calling code does not need to worry
about this quantity, and all calculations -- which depend on the variant value -- are abstracted
away from the transmission manager into the `time_to_next_infection_attempt` method which checks for
the variant value of the `GenerationInterval` type and computes quantites accordingly.
2. The time of the last infection attempt, both as a uniform number and in absolute space, which is
kept track of as a person property. This also means that the `ContextNaturalHistoryExt` provides a
convenience method for resetting these internal parameters at the end of an infection to allow for
reinfection.

```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn reset_to_susceptible(&mut self, person: PersonId);
}
```

Finally, there are two implications of this structure we have outlined. First, since the generation
interval is required as an input to the `time_to_next_infection_attempt` method, a modeler can have
the generation interval switch based on the infection setting or other model properties. In other
words, the specification of parameters and computation done with them are modular. Secondly, this
structure also lends itself to generalizability to modeling multiple co-circulating pathogens: the
modeler chooses the relevant generation interval based on the infecting pathogen rather than their
being one global generation interval.

## Allowing for correlation between parameter values

Natural history parameters can be correlated. For instance, a specific generation interval may be
associated with a particular number of secondary infection attempts, or a given value of the
generation interval may be associated with a particular time to symptom improvement. We want to
allow for an input format that lets us include paired natural history parameter values. First, let
us describe what the input format for such a parameter may be. Returning to the idea of there being
different possible respiratory viral load trajectories for each agent, it makes sense to specify
these as a vector. If we want each trajectory to be associated with a particular incubation period
distribution, we would also specify those values as a vector.

```json
{
    "epi_isolation.Parameters": {
        "respiratory_viral_load": [
            "TimeVaryingParameter([0.0, 1.0], [Empirical[1.0], Empirical[2.0])",
            "TimeVaryingParameter([0.0, 1.0], [Empirical[1.5], Empirical[2.5])",
            "TimeVaryingParameter([1.0, 2.0], [Empirical[1.0], Empirical[2.0])"],
        "incubation_period": [
            "Empirical[3.0]",
            "Empirical[2.5]",
            "Empirical[4.2]"],
    }
}
```

By itself, this input structure suggests a noteworthy feature: different trajectories of a
time-varying parameter can have different time values. This is a helpful feature for most closely
mimicking real-world data that may have measurements of a time-varying parameter at different times
per person. By ensuring that the input format can allow for the complexities of real-world data, we
remove the need for spurious pre-processing, which in this case would be standardizing all samples
to have values at the same time. On the other hand, this input structure also has a potential
downside: at the current time, it does not allow for samples in the vector to be weighted. In other
words, we cannot provide a weight that tells the sampler "take the first value 40% of the time and
the second and the third each 20%". However, a user could implement the necessary traits on a new
type that also contained weighting information if their use case required that.

This changes the type of the respiratory viral load natural history parameter to now be
`Vec<TimeVaryingParameter>` and the type of the incubation period natural history parameter to be
`Vec<ProbabilityDistribution>`. So far, we have implemented all the traits we need to compute
quantites from the parameter values on the raw types, not vectors of the types. However, we want to
modularly allow the user to modify their input file to have vectors of these types without needing
to change the way their code is structured, so we implement these traits for the vectors of the
types as well. Because the methods that use these times just require that they implement specific
traits (i.e., the methods are not tied to particular types), this enables the modularity we desire.

Concretely, we implement the `Distribution` trait for `Vec<ProbabilityDistribution`, the
`LinearInterpolation` trait for `Vec<TimeVaryingParameter>` and
`Vec<InvertibleTimeVaryingParameter>`, and we implement `GenerationIntervalComputation` on
`Vec<GenerationInterval>`. These traits still need to act on a particular instance of the underlying
type, so we introduce a new concept -- a person property called the `NaturalHistoryIndex`. In brief,
this index tells us which item in the vector to use as that person's particular value of a natural
history parameter. This index is used consistently across all natural history parameters for which a
vector of values is specified, thereby allowing for paired/joint natural history parameter values.
Indeces are assigned lazily when any one of the methods in `ContextNaturalHistoryExt` is called, so
this technical detail is entirely abstracted from the calling code. This is why all the methods
in the `ContextNaturalHistoryExt` require the person as an input: they use the value of the person's
`NaturalHistoryIndex` to conduct the computations in the traits we've defined on the particular
value at the associated index.

```rs:natural_history_manager.rs
define_person_property_with_default!(NaturalHistoryIndex, Option<usize>, None);

fn assign_natural_history(context: &mut Context, person_id: PersonId) -> usize {
    if let Some(idx) = context.get_person_property(person_id, NaturalHistoryIndex) {
        return idx;
    }

    // Assign index based on the length of the vectors of the natural history parameters defined in
    // this module.
    let idx = ...;

    context.set_person_property(
        person_id,
        NaturalHistoryIndex,
        Some(idx)
    );
    idx
}
```

Lazy assignment of indeces (i.e., assignment on demand by the natural history manager) improves
modularity: imagine that we pick one method that explicitly sets the natural history index, or even
if it is done by the user in one of the modules (natural history index is a person property and
directly modifiable in any module). Let us say that method is the `time_to_next_infection_attempt()`
method though it can be any method. Since all natural history parameters are queried based on the
index, this structure would require that the `time_to_next_infection_attempt()` method is called
_before_ any other methods that require the person's natural history parameter index. Even if the
natural history parameter index is only required once an agent becomes infectious, this requires
that the transmission manager's event listeners for the `Susceptible --> Infectious` transition
occur before any other module's. Not only does this reduce modularity, but it makes the model
structure prone to bugs and dependency issues.

To ensure that an agent's natural history parameters are not being changed in the middle of their
infection (i.e., a subsequent call to any of the methods in the `ContextNaturalHistoryExt` that then
call `assign_natural_history()` does not change the natural history parameters if they have already
been set for this agent's infection), the method only sets the properties if they are `None` -- in
other words, not set previously. In a future immunity manager, when an individual is returned to
being susceptible, their natural history parameters can be reset: the `NaturalHistoryIndex` can be
set to back to `None` and any other person properties the natural history manager controls/sets for
the `time_to_next_infection_attempt` method can be reset. This is done in the
`reset_to_susceptible(&mut self, person: PersonId)` method in the `ContextNaturalHistoryExt`
referenced [above](#implementing-new-methods-on-subtypes-an-example-from-using-the-disease-generation-interval).

Finally, because assigning a natural history index changes a person property, all methods that do
so require a mutable reference to `Context`. This explains why all the methods described above in
`ContextNaturalHistoryExt` take a mutable reference to `Context`.
