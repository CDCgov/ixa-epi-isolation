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
`ContextNaturalHistoryExt` that contains convenience methods for common multistep calculations on
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
have defined the `Distribution` type broadly, we can consider this special case. For the type, we
implement a getter method that returns a type that implements Rust's `statrs`'s `Distribution`
trait. This enables us to draw random samples from the distribution and treat it like we natively
defined the distribution type in Rust without going through our enum first. To reduce confusion, we
adopt that the meaning of parameters in our custom `ProbabilityDistribution` type is the same as in
the associated `statrs` type.

```rs:natural_history_manager.rs
use statrs::distribution::{Exp, Gamma, Empirical};

impl ProbabilityDistribution {
    pub fn get(&self) -> impl Distribution<f64> {
        match self {
            ProbabilityDistribution::Exp(lambda) => Exp::new(lambda).unwrap(),
            ProbabilityDistribution::Gamma(a, b) => Gamma::new(a, b).unwrap(),
            ProbabilityDistribution::Empirical(vals) => Empirical::from_iter(vals).unwrap()
        }
    }
}
```

If a user wanted to use this type to draw a random sample in a module, say to draw the time at which
to set the person's health status to symptomatic once the agent becomes infected, and the incubation
period distribution were specified in the input JSON file as described above, they would do the
following:

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
                                .incubation_period.get()
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

The modular structure lends itself to the user being able to easily change the underlying
distribution type (i.e., switching out a gamma for, say, a lognormal) in the input JSON with no
change to the code and only needing to make sure their new distribution can be coerced into a Rust
type that implements the `Distribution` trait. Our `ProbabilityDistribution` type handles many cases
of natural history parameters and more broadly being able to specify and sample from arbitrary
distributions in our ABMs. This type also serves as the building block for the next natural history
parameter type we define: time-varying parameters.

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
implemented in most ABMs: concretely, we argue that the value of a time-varying parameter at a given
time may itself be a random distribution rather than being a fixed value. However, based on what we
have introduced so far, this type does not allow for the user to specify viral load _trajectories_
(i.e., one agent is assigned an entire set of deterministic viral loads and another agent has a
different specified set). While a user could effectively hijack this current type to do that by
implementing their own sampling method that provides the desired behavior, we devise a general
solution to this problem below exploiting infrastructure we have to build to allow for correlated
parameter values.

With a time-varying parameter, we want a method to get the value of the parameter at a given time.
This process requires more steps, so we define a convenience method in a trait extension on
context that helps us do this.

### Implementing new methods on subtypes: An example from using the disease generation interval



## Allowing for correlation between parameter values






```rs:natural_history_manager.rs
pub trait ContextNaturalHistoryExt {
    fn get_parameter_value(
        &mut self,
        person_id: PersonId,
        parameter: NaturalHistoryParameter
    ) -> impl NaturalHistoryParameterValue;

    fn time_to_next_infection_attempt(&mut self, person_id: PersonId) -> Option<f64>;
}

pub trait NaturalHistoryParameterValue {
    fn get_value(&self) -> f64;
    fn estimate_at_point(&self, x: f64, invert: bool, start: f64) -> f64;
}
```

We implement the `NaturalHistoryParameterValue` trait for the common return types of natural history
parameter values (details below). At a high level, these methods provide the post-processing needed
to use natural history parameter values in an individual-level model.

### Querying transmission-relevant quantities

The natural history manager provides a method for querying the time to the next infection attempt
called by the transmission manager. Since our
[algorithm for calculating the time to the next infection attempt](./time_varying_infectiousness.md)
relies on an individual's natural history parameters, we want the `time_to_next_infection_attempt`
method to be in the `ContextNaturalHistoryExt` trait extension. This structure also simplifies the
code in the transmission manager, improving modularity. Adopting the convention that the method
returns `None` if there are no more infection attempts, the transmission manager would call the
method as follows:

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
    if let Some(delta_time) = context.time_to_next_infection_attempt(event.transmitter_id) {
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
workflow, independent of the mathematical calculations to obtain the times. This helps improve
modularity, even giving the user the option to use their own `time_to_next_infection_attempt` that
uses a custom sampling strategy.

We use the [order statistics](./time_varying_infectiousness.md) sampling strategy to calculate the
time to the next infection attempt. This means that the `time_to_next_infection_attempt` method
requires the following:
1. The individual's disease generation interval,
2. The number of secondary infection attempts remaining, and
3. The time of the last infection attempt.

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

## Input format for natural history parameters

The natural history manager must know the values and distributions of the natural history
parameters. The most general natural history manager takes an input CSV of samples of the natural
history parameters. It reads in this CSV, stores it in the `NaturalHistory` data plugin, and then
queries the relevant values for a given person on demand.

We expect the input CSV to take on the following long format easily conducive to adding new natural
history parameters:

| index | weight | time | parameter | value |
| --- | --- | --- | --- | --- |
| 0 | 0.45 | 0.0 | GenerationIntervalCDF | 0.0 |
| 0 | 0.45 | 1.0 | GenerationIntervalCDF | 0.5 |
| 0 | 0.45 | 2.0 | GenerationIntervalCDF | 1.0 |
| 0 | 0.45 | NA | IncubationPeriod | 6 |
| 0 | 0.45 | NA | TimeToSymptomImprovement | 7 |
| 1 | 0.4 | 0.0 | GenerationIntervalCDF | 0.0 |
| 1 | 0.4 | 1.0 | GenerationIntervalCDF | 0.8 |
| 1 | 0.4 | 2.0 | GenerationIntervalCDF | 1.0 |
| 2 | 0.15 | 0.0 | GenerationIntervalCDF | 0.1 |
| ... | ... | ... | ... | ... |

This CSV contains the values of all natural history parameters that the natural history manager
knows about. Parameters are specified by an index, the same index that an agent is assigned
(proportional to the specified weight) when their natural history parameters are queried. Grouping
parameters by indeces allows for implicit correlations and dependencies between parameters. For
instance, in this example `index = 0` describes the infection of an individual who has a symptomatic
infection because they have an incubation period and time to symptom improvement. Both are required
to specify a symptomatic infection. Likewise, the generation interval CDF for this person can be
specific to them being a symptomatic individual. Implicitly, all values of a parameter for a given
index represent a joint sample from the data generation process for the natural history of
infectious agents. On the other hand, the agent who is assigned `index = 1` has an asymptomatic
infection because these parameters are not present in their parameter set. Therefore, this input
schema allows for describing different types of infections in a single file. Simultaneously, the
`weight` column allows for each of the different infection types to be sampled proportional to their
occurrence in observational data (or even for this quantity to be calibrated). To add new parameters
-- for instance, the viral load -- add a row (or rows if the parameter varies in time) for each
`index` value for which this parameter is relevant (i.e., infection type) to the input CSV.

This input structure has two implications for time-varying parameters. First, there may be multiple
parameters that vary over time, but they do not need to have values at the same time (for instance,
the viral load and generation interval CDF for the same `index` could have samples at different
time). Second, a time-varying parameter may have different time values across different `index`s.
Despite this variety in types of inputs, all parameters can be stored and queried the same
through the natural history manager and by using the `NaturalHistoryParameterValue` trait.

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
