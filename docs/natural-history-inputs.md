# A central natural history manager

## Overview
We provide a way to read in user-specified natural history parameters (infectiousness over time/the
generation interval, viral load over time, etc.) and parametrizing transmission based on these parameters.
In the future, the input CSV can be expanded to also include symptom onset and improvement times as part of
the natural history parameter set.

A natural history parameter set consists of a set of values of all the natural history parameters: the
generation interval over time, the viral load, the symptom onset and improvement times, the time to
hospitalization, etc. There may be correlations between these parameters -- so that higher viral loads
are associated with longer times to symptom improvement -- or there may not be. By having the user specify
a CSV file which contains all of these parameter sets, not only does this mean that the user determines
the distribution of each of these parameters outside of Ixa, but the user is also able to impose any
meaningful correlations between parameters.

## Data input format

The input CSV includes samples of the natural history parameters. For now, we are interested in a single
natural history parameter: the generation interval CDF over time. Data must be input in a long format, as
such:

| id | time | gi_cdf |
| --- | --- | --- |
| 0 | 0.0 | 0.0 |
| 0 | 1.0 | 0.5 |
| 0 | 2.0 | 1.0 |
| 1 | 0.0 | 0.0 |
| 1 | 1.0 | 0.3 |
| ... | ... | ... |

The `id` is an identifier that marks a distinct sample from the natural history parameters at a given `time`
since the person was first infected. The `gi_cdf` describes the fraction of infectiousness that has occured
in this parameter set at time t. Additional columns like `viral_load` and other time-varying quantities
can easily be added in this input schema.

In the future, we may also add variables like `symptom_onset_time` and `symptom_improvement_time`. These
variables are constant in time, so the same value will be present for all rows of a given `id` value.
If a person is asymptomatic, their `symptom_onset_time` and `symptom_improvement_time` should be NA.
This choice allows for one input CSV that includes both symptomatics and asymptomatics, and the user
can specify different GIs for symptomatics and asymptomatics with great flexibility.

Finally, this input schema naturally lends itself to adequately modeling per-person variability in
the natural history parameters. The user can specify as many different natural history parameter
sets (potentially where each parameter set has completely different values for all parameters or
where each parameter set has, say, the same GI but different symptom onset times) as needed to properly
reproduce the underlying parameter distribution, and the model will sample from these values accordingly.

Note that this input structure also means that each parameter set can have parameter samples at
different times. This could be useful if synthesizing various clinical datasets that each record the
viral load at different times and a user wants to include the raw data in the model without doing
any pre-processing, though it is worth noting that the module is set up to use cubic spline
interpolation for intermediary values (more on this below.)

## Implementation

People are assigned an `id` when they are infected. This `id` refers to the natural history parameter
set that will define their infection (for now, just the GI, but this will be expanded to include
symptom onset and improvement times). `id`s are assigned uniformly and randomly, which allows for
directly reproducing the variability present in these inputs in what is actually simulated.

`mod natural_history_manager` provides a trait extension `ContextNaturalHistoryExt` that enables a user
to query an individual's various natural history parameters during their infection. First, the user's
natural history index must be set. We assume that it is set when an agent is first infected in
`mod transmission_manager`. This implicitly means that all other modules that require knowing the agent's
natural history parameter index (for instance, the future `mod health_status_manager` which requires
knowing an agent's symptom onset and improvement times) must have their event callbacks for the
`InfectiousStatusValue::Susceptible --> InfectiousStatusValue::Infectious` transition happen
_after_ `mod transmission_manager`'s callback for the same transition.

As of now, the `ContextNaturalHistoryExt` trait provides a method for querying the value of the
inverse generation interval (i.e., returning a time from a uniform value) for scheduling
[infection attempts](time-varying-infectiousness.md). The method takes the person doing the infecting
as an input to query their natural history index and then use the particular generation interval CDF
trajectory associated with that index. Since the generation interval samples that are provided are discrete,
we use cubic spline interpolation (defaulting to linear interpolation at the tails) to estimate the value
of the inverse CDF from the continuous uniform value provided as an input to the method. In the future,
we may also add methods for querying the viral load, and they could use the same sort of interpolation
features.

## Assumptions
1. There are no requirements on the number of parameter sets fed to the model. Trajectory numbers are assigned
to people uniformly and randomly. A user must provide enough trajectories that they provide a representative
sample of the underlying natural history parameters, but that choice is up to the user.
2. There must be the same number of values for each parameter provided in the input CSV. In other words, a user
cannot provide 1000 GI trajectories but only 10 symptom improvement times. The user must ensure that all parameter
sets are complete and do that either via assuming independent draws between parameter values or imposing a correlation
between parameter values. In other words, the burden of constructing reasonable parameter sets falls on the user
rather than being implicitly baked into the model.
3. We plan on introducing clinical symptoms in the same input CSV and being managed as another natural history
parameter even though these outcomes are not necessarily only a function of the disease natural history. In other
words, social determinants of health and other properties may also influence an agent's clinical outcomes from disease.
Does this current input structure make sense, or is the inability to disentangle clinical outcomes from infectiousness
dangerous? For our isolation guidance work, the two are inexplicably linked, and I cannot think of a way to disentangle
them.
