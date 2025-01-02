# A central disease natural history manager

## Overview

The natural history manager holds the quantities that define an agent's infection. These quantities
include the number of secondary infection attempts made by an infectious agent over their entire
duration of infectiousness, the agent's relative probability of transmission over time/their
generation interval distribution, and time of symptom development. The natural history manager
connects with other modules in the model by providing methods relevant to determine transmission
dynamics at the individual level. For instance, it determines the time to the next infection
attempt, which is required by the transmission manager.

Here we describe the methods in the natural history manager -- including their function signatures
and pseudocode -- and the format of an input CSV that specifies the natural history parameters. The
natural history manager randomly draws an agent's natural history parameters from an input CSV that
contains samples of the natural history parameters, thereby not making assumptions about the
underlying distribution of the parameters or their relationship to each other.

## Trait extension methods

The natural history manager trait extension provides methods for querying interpretable modeling-
relevant parameters and quantities needed for transmission. When an individual is infected and can
infect others, the transmission manager will schedule their next infection attempt using the trait
extension `time_to_next_infection_attempt`, provided by the natural history module. This method uses
the disease generation interval (GI) and [order statistics](./time-varying-infectiousness.md) to
calculate the time to the next infection attempt. If there are no more infection attempts, this
method returns `None`.

```rs:transmission.md
fn schedule_next_infection_attempt(
    context: &mut Context,
    transmitter_id: PersonId,
) {
    if let Some(delta_time) = context.get_time_to_next_infection_attempt(event.person_id) {
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

### Determining time to next infection attempt

The natural history manager calculates the time to next infection attempt using order statistics,
which requires the following quantities:

 1. The number of infection attempts remaining for this agent
 2. The timing of their last infection attempt
 3. The agent's disease generation interval distribution


For the `time_to_next_infection_attempt` method, the relevant parameters are the number of secondary
infection attempts and generation interval distribution (or an index that can be used to query the
generation interval and any other natural history parameters from an input CSV; more on this below).
We set these parameters as person properties in the `assign_natural_history(&mut Context, PersonId)`
function.

```rust
pub trait ContextNaturalHistoryExt {
    fn time_to_next_infection_attempt(&mut self, person_id: PersonId) -> Option<f64> {
        assign_natural_history(self, person_id);
        let infection_attempts_remaining = self.get_person_property(person_id, NumInfectionAttemptsRemaining).unwrap();
        if infection_attempts_remaining == 0 {
            return None;
        }
        let (last_infection_attempt_unif, last_infection_attempt_time) =
        self.get_person_property(person_id, LastInfectionAttemptTime);
        let next_infection_attempt_unif = get_next_infection_attempt_unif(infection_attempts_remaining,
            last_infection_attempt_unif);

        let next_infection_attempt_time = next_infection_attempt_gi(next_infection_attempt_unif, person_id);
        self.set_person_property(person_id, LastInfectionAttemptTime, (next_infection_attempt_unif, next_infection_attempt_time));
        next_infection_attempt_time - last_infection_attempt_time
    }
}

fn assign_natural_history(context: &mut Context, person_id: PersonId) {
    if context.get_person_property(NaturalHistoryIndex).is_some() {
        // Natural history index has already been set -- this is a repeat query of this function
        // just to check that the natural history parameters have been set, so the parameters should
        // not be reset. More on this below.
        return;
    }
    context.set_person_property(NumInfectionAttemptsRemaining, Some(sample_infection_attempts(context, person_id)));
    // LastInfectionAttemptTime is a tuple because it stores the last infection attempt time in both uniform
    // and generation interval/real time space.
    context.set_person_property(LastInfectionAttemptTime, (NotNan::new(0.0), NotNan::new(0.0)));
    context.set_person_property(NaturalHistoryIndex, Some(sample_natural_history_parameter_sets(context, person_id)));
}
```

## Initializing natural history parameters for an infected individual

When the `time_to_next_infection_attempt` method is called for the first time, it needs to set the
agent's natural history parameters.

The transmission manager would use the method as follows:

```rust
fn handle_infectious_status_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<InfectiousStatus>,
) {
    if event.current == InfectiousStatusType::Infectious {
        schedule_next_infection_attempt(
            context,
            event.person_id,
        );
    }
}

fn schedule_next_infection_attempt(
    context: &mut Context,
    transmitter_id: PersonId,
) {
    if let Some(delta_time) = context.get_time_to_next_infection_attempt(event.person_id) {
        context.add_plan(
            context.get_current_time() + delta_time,
            move |context| {
                infection_attempt(context, transmitter_id)
                    .expect("Error finding contact in infection attempt");
                // Schedule the next infection attempt for this infected agent
                // once the last infection attempt is over.
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
workflow, independent of the mathematical calculations required to obtain the times.

### Querying clinical-relevant natural history parameters

The natural history trait extension provides methods to query an individual's time to symptom onset,
improvement, hospitalization, or even symptoms experienced at a given time. By having one trait
extension manage both transmission-relevant natural history parameters (e.g., number of secondary
infection attempts) and clinical-relevant natural history parameters (incubation period), we can
allow for correlations between the two. For instance, positive relationship between an agent's
duration of symptoms and number of secondary infection attempts.

These methods also need to call `assign_natural_history()` to ensure that the natural history
parameters relevant to these methods, namely the incubation period and time to symptom onset, are
set. Since `assign_natural_history()` does not know its calling function, it should set all the
natural history parameters (or an index that can be used to query them as mentioned above), and
each natural history querying method should call it so to ensure that all natural history parameters
are set. If, for example, the `time_to_symptom_onset` method below does not call the
`assign_natural_history` method, that would require that the natural history had already been
assigned and that the `time_to_next_infection_attempt` method had been called prior to the
`time_to_symptom_onset` method. This would require that the transmission manager's event listeners
were triggered prior to the clinical symptoms event manager's. That structure introduces subtle
dependencies between modules, making them loss modular and prone to bugs.

To ensure that an agent's natural history parameters are not being changed in the middle of their
infection (i.e., a subsequent call to `assign_natural_history()` does not change the natural history
parameters if they have already been set for this agent's infection), the method only sets the
properties if they have not been set before. This choice allows for resetting the parameters prior
to re-infection: in a future immunity manager, when an individual is returned to being susceptible,
their `NaturalHistoryIndex` can be reset to `None`.

```rust
pub trait ContextNaturalHistoryExt {
    fn time_to_symptom_onset(&mut self, person_id: PersonId) -> f64 {
        assign_natural_history(self, person_id);
        sample_incubation_time(self, person_id)
    }
    fn time_to_symptom_improvement(&mut self, person_id: PersonId) ->
        assign_natural_history_idx(self, person_id);
        sample_symptom_recovery_time(self, person_id)
    }
}
```

### Querying biomarker-relevant natural history parameters

ABMs are often used to model the impact of disease testing programs where the probability of an
individual testing positive is a function of their viral load. Therefore, it is necessary to have a
method to query the viral load at a given time.

```rust
pub trait ContextNaturalHistoryExt {
    fn viral_load(&mut self, person_id: PersonId, time: f64) -> f64 {
        // Assign natural history parameter set if they don't exist already.
        assign_natural_history(self, person_id);
        sample_viral_load(person_id, time)
    }
}
```

## Input data

The natural history manager must know the values and distributions of the natural history
parameters. The most general natural history manager takes an input CSV of samples of the natural
history parameters. The `sample_{parameter}` functions referenced above sample from the values in
the CSV for a given parameter, assigning parameter indeces with a given `weight` and then sampling
from the values present for the person's assigned natural history index at a given time.

The input CSV is in long format and contains all the natural history parameters relevant to the
model.

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

The `index` is a unique identifier that marks a distinct sample of the natural history parameters.
Agents are assigned an index when infectious, and this is stored in the person property
`NaturalHistoryIndex`. In this example, `index = 0` describes the infection of an individual who has
a symptomatic infection because they have incubation period and time to symptom improvement values
in their parameter set whereas `index = 1` describes the infection of an individual who is
asymptomatic because symptom-associated parameters are not present in their natural history
parameter set. This schema allows for describing different types of infections in a single input
file. The `weight` column describes the weight with which to sample that particular infection
archetype in the model. To add new parameters -- for instance, the viral load -- add a row (or rows
if the parameter varies in time) for each `index` value for which this parameter is relevant to the
input CSV.

This input structure has two implications for time-varying parameters. First, there may be multiple
parameters that vary over time, but they do not need to have values at the same time (for instance,
the viral load and generation interval CDF for the same `index` could have samples at different
time). Second, a time-varying parameter may have different time values across different `index`s.
We use linear interpolation to estimate the value of a time-varying parameter at a continuous time
value in the model from the samples provided at discrete times in the input CSV.

## Application to isolation guidance

To model isolation guidance at the community level, we need to:

1. Read natural history parameters for symptomatic agents including the generation interval
distribution, symptom onset time, symptom improvement time, and viral load over time.
2. When an agent becomes infected, assign a natural history parameter set that consists of a
generation interval, symptom onset time, symptom improvement time, and viral load. This parameter
set should be a joint posterior sample from the Stan model, so that all parameters are related,
meaning that the generation interval distribution is associated with particular values of the
symptom onset time and improvement time.
3. We tested different generation intervals in our isolation guidance modeling, so we should be able
to easily swap between generation intervals in our ABM to examine how assumptions about
infectiousness over time change our results.
3. (For the current guidance) Label individuals as isolating while they are experiencing symptoms,
and have their infectiousness and contacts changed accordingly. Label individuals as in
post-isolation precautions for five days after their symptoms improve, and have their infectiousness
and contacts changed accordingly.
4. (For the previous guidance) Simulate individuals getting a COVID test when they first start
experiencing symptoms with test positivity as a function of their viral load.

The structure described in this natural history manager enables each of these requirements.
