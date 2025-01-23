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
global properties plugin's infrastructure. We focus on having these types be as generic as possible,
so that the user can change the value (for instance, changing an exponential distribution to a gamma
distribution) without impacting the parameter type or the way it is used in the code.

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
    // ... Because this is an enum, users can add any other distribution variants.
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
        "R_0": "Empirical([3.0])"
    }
}
```

In the case of the distribution for $R_0$, we are saying that all agents have the same value of
$R_0$ -- there is no stochasticity or person-to-person variation in $R_0$. Nevertheless, because we
have defined the `Distribution` type broadly, we can consider this special case. For our
`ProbabilityDistribution` type, we implement Rust's `rand` crate's `Distribution` trait. This
enables us to draw random samples from the distribution using Ixa's built-in `ContextRandomExt` that
expects a type that implements `Distribution<T>` for sampling. This lets us treat our custom enum
like we natively defined the distribution type in Rust without going through our enum first. To
reduce confusion, we adopt that the meaning of parameters in our custom `ProbabilityDistribution`
type is the same as in the `statrs` distribution of the same name.

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
to extract the raw `statrs` type (i.e., what they would have gotten if they called `Exp::new(1.0)`,
for instance) or to do other computations like getting the mean or variance of the distribution.

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
`Distribution` trait. The incubation period parameters could even be updated in the middle of the
simulation! Our `ProbabilityDistribution` type handles many use cases for natural history parameters
and more broadly being able to specify and sample from arbitrary distributions in our ABMs. This
type also serves as the building block for the next natural history parameter type we define:
time-varying parameters.

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

Concretely, a user may specify a time-varying parameter like so in their input JSON.

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
`LinearInterpolation` trait), draws samples from the corresponding distributions at those times, and
takes the weighted average of the sampled values on their time's distance to the given time. These
implementation details are abstracted from the calling module. However, there is one implementation
detail we expose to the user. In many cases, we estimate the value of a time-varying parameter at
always increasing times (i.e., when we require an agent's viral load and their time since infection
is always increasing). If we store the index of the last value at which the time-varying parameter
was queried, we know that our current value of that parameter must be at a greater index. We can use
this trick to speed up the process of searching for indeces.

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

For this type, we implement the `LinearInterpolation` trait that enables us to grab the indeces of
the values that window a particular value, and we implement a method that returns the inverse tuple.

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
    Distribution(ProbabilityDistribution, ProbabilityDistribution),
    LengthAndMagnitude(ProbabilityDistribution, ProbabilityDistribution, ProbabilityDistribution),
    Cdf(InvertibleTimeVaryingParameter, ProbabilityDistribution),
    NormalizedHazard(InvertibleTimeVaryingParameter, ProbabilityDistribution),
    AbsoluteHazard(InvertibleTimeVaryingParameter)
}
```

By convention, when modelers say "the generation interval is exponential", that means that the
generation interval _length_ is exponential and the magnitude over time is constant. We adopt the
same shorthand (the first enum variant listed above) and then a more complete option that allows the
user to separately specify the length and magnitude separately (the second option). In both cases,
the distribution for $R_0$ must also be specified, and that is the final parameter in each of these
variants. By tethering these two parameters together, we are ensuring that if the variant value
picked by the user requires a distribution for $R_0$, they actually must provide it otherwise the
model will not compile. If we were to separately specify the $R_0$ distribution, we would have to
check for the existence of that parameter (assuming a consistent name for it) when the model is
validating its parameters, deferring the error to later and making assumptions about code structure.

When the user cannot confine the generation interval to an analytical distribution (i.e., they have
empirical samples of the distribution over time), we allow multiple ways to specify the generation
interval -- as a CDF (requiring an invertible time varying parameter and a distribution of $R_0$),
a normalized hazard function which requires the same inputs, an absolute hazard function which does
not require a separate distribution of $R_0$ because it quantifies _absolute_ hazard, etc.
Regardless of the form of the generation interval, we use the generation interval to estimate the
time to the next infection attempt. Because there are multiple steps in this process, we define a
method in the `ContextNaturalHistoryExt` called `time_to_next_infection_attempt` that conducts this
routine according to our algorithm for calculating the
[time to the next infection attempt](./time_varying_infectiousness.md).

```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn time_to_next_infection_attempt(
        &mut self,
        gi: GenerationInterval,
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
through the inverse CDF.

Internally, this method keeps track of:
1. The agent's remaining number of secondary infection attempts (or, if the absolute hazard function
was specified, remaining absolute infectiousness). Because our structure for the generation interval
is coupled to the number of secondary infection attempts, the calling code does not need to worry
about this quantity, and all calculations -- which depend on the variant value -- are abstracted
away from the transmission manager into the `time_to_next_infection_attempt` method.
2. The time of the last infection attempt, both as a uniform number and in absolute space, which is
kept track of as a person property. This also means that the `ContextNaturalHistoryExt` provides a
convenience method for resetting these internal parameters at the end of an infection to allow for
reinfection.

```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn reset_to_susceptible(&mut self, person: PersonId);
}
```

## Allowing for correlation between parameter values





We discuss the details of querying an agent's natural history parameters below when introducing the
format of the input CSV that contains these parameters. For now, know that the method
`get_parameter_value()` can return the person-specific parameter value. We need to call this method
twice in `time_to_next_infection_attempt()`: once to get the number of secondary infection attempts
(if not set for the person already, so called only on the first time
`time_to_next_infection_attempt()` is called), and once to get the person's disease generation
interval.

In the two cases, the return type of the parameter value is different. The number of secondary
infection attempts is a positive integer (but more generally a scalar) while the generation interval
CDF is a function (or samples of a function) that varies over time since infection. We also need to
use the two parameters differently. While we just want the value of the number of secondary
infection attempts for the person as a scalar, we need to use inverse transform sampling on the
generation interval CDF to convert an ordered uniform draw (i.e., the quantile by which a given
amount of infectiousness has passed) to a time. This entails inverting the generation interval CDF,
estimating the time at which the infectiousness quantile is reached, and returning a float.

```rs:natural_history_manager.rs
define_person_property_with_default!(NumInfectionAttemptsRemaining, Option<usize>, None);
define_person_property_with_default!(LastInfectionAttemptTime,
                                     (NotNan<f64>, NotNan<f64>),
                                     (NotNan::new(0.0), NotNan::new(0.0)));

impl ContextNaturalHistoryExt for Context {
    fn time_to_next_infection_attempt(&mut self, person_id: PersonId) -> Option<f64> {
        let infection_attempts_remaining = match self.get_person_property(person_id,
                                                                    NumInfectionAttemptsRemaining) {
            None => {
                self.set_person_property(
                    person_id,
                    NumInfectionAttemptsRemaining,
                    Some(self.get_parameter_value(
                            person_id,
                            NaturalHistoryParameter::InfectionAttempts)
                         .get_value())
                );
                self.get_person_property(person_id, NumInfectionAttemptsRemaining).unwrap()
            },
            Some(0) => return None,
            Some(n) => n,
        };
        let (last_infection_attempt_quantile,
             last_infection_attempt_time) = self.get_person_property(
                                                    person_id,
                                                    LastInfectionAttemptTime
                                            );
        let next_infection_attempt_quantile = ordered_draw_uniform_distribution(
                                                    infection_attempts_remaining,
                                                    last_infection_attempt_quantile
                                              );
        let  next_infection_attempt_time = self.get_parameter_value(
                                                person_id,
                                                NaturalHistoryParameter::GenerationIntervalCDF)
                                           .estimate_at_point(next_infection_attempt_quantile,
                                                              true,
                                                              last_infection_attempt_quantile);
        self.set_person_property(
            person_id,
            LastInfectionAttemptTime,
            (NotNan::new(next_infection_attempt_quantile), NotNan::new(next_infection_attempt_time))
        );
        Some(next_infection_attempt_time - last_infection_attempt_time)
    }
}
```

We use the `get_value()` method for directly getting the value of a scalar parameter, and we use the
`estimate_at_point()` method for estimating the value of a time-varying parameter at a given point.
The `estimate_at_point()` method takes three arguments: the value at which we want to be doing the
estimation, whether we want to actually be estimating the _inverse_ of the function (in this case,
that is `true`), and a starting point. The starting point argument is useful because we often need
to get the value of a time-varying parameter over the course of an infection. Time since being
infected only ever increases, so we know that we will be querying the function at larger times.
Depending on the underlying type for which the trait is being implemented, knowing the starting
point for doing our estimation routine can help improve computational efficiency. We will explain
the details of implementing this trait below, but the abstractness of being able to tailor the
implementation of the trait to the type helps improve modularity.

### A general pattern for getting parameter values

The pattern used in the `time_to_next_infection_attempt()` method is a specific example of a more
general pattern: getting the parameter value with the `get_parameter_value()` method, and then using
the methods available in the `NaturalHistoryParameterValue` trait to extract the quantity relevant
for modeling disease spread at the individual level.

Similar to the use case described with the time to the next infection attempt, consider that a
person who is experiencing symptoms may test and quarantine if they test positive. If probability of
testing positive over time is specified as a natural history parameter, it can be queried at a given
time in `mod testing_manager`:

```rs:testing_manager.rs
fn handle_health_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<HealthStatus>,
   ) {
    if event.current == HealthStatusType::Symptomatic {
        let time_since_infected = context.get_current_time() - context.get_person_property(
                                                                    event.person_id,
                                                                    InfectionTime)
                                                                .unwrap();
        let antigen_positivity = context.get_parameter_value(
                                    event.person_id,
                                    NaturalHistoryParameter::AntigenPositivity)
                                 .estimate_at_point(time_since_infected, false, 0.0);

        if context.sample_bool(TestingRng, prob_test_positive) {
            context.set_person_property(
                event.person_id,
                QuarantineStatus,
                QuarantineStatusType::HouseholdQuarantine
            );
        }
    }
}
```

On the other hand, consider that `mod health_status_manager` wants to know when the agent starts
showing symptoms and change their clinical health status accordingly (imagine all agent present with
symptoms at some point for the purposes of this example):

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

        let incubation_time = context.get_parameter_value(
            event.person_id,
            NaturalHistoryParameter::IncubationPeriod)
            .get_value();
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

These two examples underscore our observations from the `time_to_next_infection_attempt()` use:
we must implement the `NaturalHistoryParameterValue` for a scalar return type and a type that lets
us estimate the value of a function at different times.

### Implementing the `NaturalHistoryParameterValue` trait for common return types

We have been purposely vague about the specific types for which we may implement the trait because
there are multiple possibilities depending on the use case. Consider the more specific natural
history manager described above where the user makes assumptions about the distributional form of
their parameters (ex., the incubation period is exponentially-distributed with a rate parameter
specified in the input file) and encodes those assumptions into their Rust implementation of the
natural history manager. They could implement the `NaturalHistoryParameterValue` trait for the
function type that enables them to randomly draw samples from their specified parameter
distributions. Then, the `get_value()` method would return a random sample from that distribution,
and the `estimate_at_point()` method could estimate the CDF at the specified value.

However, we are interested in the most general case where the user specifies samples of the natural
history parameters in an input CSV. In that case, for the natural history parameter types that do
not change over the course of the infection (like the incubation period or the number of secondary
infection attempts), the user provides in a scalar, and the `NaturalHistoryParameterValue` should be
implemented for that scalar type:

```rs:natural_history_manager.rs
impl NaturalHistoryParameterValue for f64 {
    fn get_value(&self) -> f64 {
        self
    }
    fn estimate_at_point(&self, x: f64, invert: bool, start: f64) -> f64 {
        unimplemented!();
    }
}
```

Since the `estimate_at_point()` method is only used for time-varying parameters, it does not make
sense in this case, so we do not implement it.

Comparatively, the most general way of storing values of a function that varies over time, such as
the generation interval CDF, is as a vector of times and a corresponding vector of function values:
`(times: Vec<f64>, values: Vec<f64>)`. We would implement the `NaturalHistoryParameterValue` trait
as follows:

```rs:natural_history_manager.rs
impl NaturalHistoryParameterValue for (Vec<f64>, Vec<f64>) {
    fn get_value(&self) -> f64 {
        unimplemented!();
    }
    fn estimate_at_point(&self, x: f64, invert: bool, start: f64) -> f64 {
        let (ts, vals) = self;
        // 1. Since the vectors are sorted, use binary search to get the indeces of the values in
        // the first vector that refer to the closest values less than and greater than the x value.
        // 2. Use linear interpolation to estimate the value at the given point based on the samples
        // in the second vector.
        if invert {
            let indeces: Range = find_window_indeces(vals, x, start);
            return linear_interpolation(vals[indeces], ts[indeces], x);
        }
        else {
            let indeces: Range = find_window_indeces(ts, x, start);
            return linear_interpolation(ts[indeces], cals[indeces], x);
        }
    }
}
```

## Storing natural history parameter values

We have motivated the `NaturalHistoryParameterValue` trait's utility from the perspective of having
parameters that take on different types but wanting consistent features across types to improve
modularity. However, the same challenge occurs when we think about wanting to consistently store
natural history parameter values, and the same solution is applicable. In particular, the trait
`NaturalHistoryParameterValue` is object safe, so we can store parameter values in a vector and
store that vector in a hash map entry referring to its parameter value:
`HashMap<NaturalHistoryParameter, Vec<Box<dyn NaturalHistoryParameterValue>>>`. The
`get_parameter_value()` method just queries this hash map, returning the value of the specified
natural history parameter for the specified person.

Although `get_parameter_value()` takes the person ID as an input, how does the method know which
parameter value should be returned for the person in question? We adopt the convention that each
person is assigned an index, the `NaturalHistoryIndex`. This index is used when taking a value from
the vector in the hash map for a specified natural history parameter. Indeces are assigned based on
weights. Together, this tells us the two features of the natural history data container that stores
the parameter values:

```rs:natural_history_manager.rs
#[derive(Default)]
struct NaturalHistoryContainer {
    weights: Vec<f64>,
    parameters: HashMap<NaturalHistoryParameter, Vec<Box<dyn NaturalHistoryParameterValue>>>
}

define_data_plugin!(NaturalHistory, NaturalHistoryContainer, NaturalHistoryContainer::default());
```

Index assignment is lazy, so the `get_parameter_value()` method always calls the
`assign_natural_history` function as its first step. This function checks if an agent's natural
history index has been set already, and if not assigns one.

```rs:natural_history_manager.rs
define_person_property_with_default!(NaturalHistoryIndex, Option<usize>, None);

fn assign_natural_history(context: &mut Context, person_id: PersonId) -> usize {
    if let Some(idx) = context.get_person_property(person_id, NaturalHistoryIndex) {
        return idx;
    }

    let weights = context.get_data_container(NaturalHistoryParameters).unwrap().weights;
    let idx = context.sample_weighted(NaturalHistoryRng, weights);

    context.set_person_property(
        person_id,
        NaturalHistoryIndex,
        Some(idx)
    );
    idx
}
```

Lazy assignment of indeces improves modularity: imagine that we pick one method that explicitly sets
the natural history index, or even if it is done by the user in one of the modules (natural history
index is a person property and directly modifiable in any module). Let us say that method is the
`time_to_next_infection_attempt()` method though it can be any method. Since all natural history
parameters are queried based on the index, this structure would require that the
`time_to_next_infection_attempt()` method is called _before_ any other methods that require the
person's natural history parameter index. Even if the natural history parameter index is only
required once an agent becomes infectious, this requires that the transmission manager's event
listeners for the `Susceptible --> Infectious` transition occur before any other module's. Not only
does this reduce modularity, but it makes the model structure prone to bugs and dependency issues.

To ensure that an agent's natural history parameters are not being changed in the middle of their
infection (i.e., a subsequent call to `get_parameter_value` and therefore `assign_natural_history()`
does not change the natural history parameters if they have already been set for this agent's
infection), the method only sets the properties if they are `None` -- in other words, not set
previously. In a future immunity manager, when an individual is returned to being susceptible, their
parameters can be reset: the `NaturalHistoryIndex` can be set to back to `None`, the
`LastInfectionAttemptTime` to `(NotNan::new(0.0), NotNan::new(0.0))`, and `NumInfectionAttempts` to
`None`.

This gives us the following structure for the `get_parameter_value()` method:

```rs:natural_history_manager.rs
impl ContextNaturalHistoryExt for Context {
    fn get_parameter_value(
        &mut self,
        person_id: PersonId,
        parameter: NaturalHistoryParameter
       ) -> impl NaturalHistoryParameterValue {
        let index = assign_natural_history(self, person_id);
        let container = self.get_data_container(NaturalHistory).unwrap();
        container.get(&parameter)[index]
       }
}
```

`get_parameter_value()` requires a mutable reference to `Context` because it calls
`assign_natural_history()`, which potentially sets a person property.



## Application to isolation guidance at the community level

For our model of isolation guidance, we apply the natural history manager and the functionality
provided by it as follows:

1. We have samples of an agent's generation interval over time, an associated symptom improvement
time, viral load over time, and antigen positivity over time. We can use values from the literature
to produce consistent estimates of an individual's incubation period given their other natural
history parameters. We want to read all of these parameters in, maintaining their correlations to
each other, so that when an agent becomes infected, they are assigned a natural history parameter
set that specifies all of these values.
    - We tested various different assumptions about the functional form of the generation interval
    in our modeling, and we would like to do the same in our agent-based model. We only have to
    change the parameter values in the input CSV to achieve this.
2. We need to use the values of the natural history parameters for an agent in not only scheduling
their infectious attempts but changing their clinical symptoms. We query their incubation period
and symptom improvement time natural history parameters and get their values for use in the model
module that manages clinical symptoms.
3. For modeling the previous guidance, we need to query an agent's probability of testing positive
at the time when their symptoms appear.

The generic nature of how we manage, store, and query natural history parameters both through the
`ContextNaturalHistoryExt` trait extension on `Context` and the `NaturalHistoryParameterValue` trait
directly enable these required features.
