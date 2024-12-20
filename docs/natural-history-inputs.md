# A central natural history manager

## Overview

We provide an interface to read in user-specified natural history parameters, such as the generation
interval distribution, and query the value of natural history parameters in modules that manage an
individual's disease, like the transmission module.

The goal of this module is to be flexible to modeling any natural history parameter that follows an
arbitrary underlying distribution and may vary over time. The user provides a CSV that contains the
parameters that define an individual's infection. This includes the generation interval distribution
(i.e., relative probability of transmitting disease over time since being infected), viral load
(also varies over time), incubation period (one value per infection), and time from symptom onset
to improvement (also one value per infection). The user provides samples of all of these parameters
in the input CSV including a unique ID that ties different parameters together to make one cohesive
natural history parameter set that describes an individual's infection. As such, all parameter
samples with the same ID refer to the joint sample from the natural history distributions. This
choice allows for the user to have full flexibility over the natural history parameter values and
their relationship to each other.

## Data input format

The input CSV includes samples of the natural history parameters. The CSV expects parameters in a
long input format:

| id | weight | time | parameter | value |
| --- | --- | --- | --- | --- |
| 0 | 0.5 | 0.0 | GenerationIntervalCDF | 0.0 |
| 0 | 0.5 | 1.0 | GenerationIntervalCDF | 0.5 |
| 0 | 0.5 | 2.0 | GenerationIntervalCDF | 1.0 |
| 0 | 0.5 | NA | IncubationPeriod | 6 |
| 0 | 0.5 | NA | TimeToSymptomImprovement | 7 |
| 1 | 0.5 | 0.0 | GenerationIntervalCDF | 0.0 |
| 1 | 0.5 | 1.0 | GenerationIntervalCDF | 0.8 |
| 1 | 0.5 | 2.0 | GenerationIntervalCDF | 1.0 |
| ... | ... | ... | ... | ... |

The `id` is a unique identifier that marks a distinct sample of the natural history parameters. In
this example, `id = 0` describes the infection of an individual who has a symptomatic infection
because they have an incubation period and time to symptom improvement whereas `id = 1` describes
the infection of an individual who is asymptomatic. This schema allows for the user to describe
different types of infections in a single input file. The `weight` column describes the weight
with which to sample that particular infection archetype in the model. A user can add new parameters
-- for instance, the viral load -- by adding a row (or rows if the parameter varies in time) to the
input CSV.

This input structure has two implications for time-varying parameters. First, there may be multiple
parameters that vary over time, but they do not need to have values at the same time (for instance,
the viral load and generation interval CDF for the same `id` could have samples at different time).
Second, a time-varying parameter may have different time values across different `id`s.

## Implementation

People are assigned an `id` lazily when they are infected. This `id` refers to the natural history
parameter set that will define their infection. `id`s are assigned randomly with frequency
proportional to their `weight` value. This allows for the simulation to directly reproduce the
variability present in the inputs.

The natural history manager provides a trait extension `ContextNaturalHistoryExt` that enables a
user to query an individual's natural history parameters during their infection. To provide a value
of a natural history parameter that varies over time from the discrete samples fed as model inputs
-- for instance, calculating the viral load at a given time or estimating the inverse generation
interval from a uniform draw -- the natural history manager uses cubic spline interpolation.

## Assumptions
1. There are no requirements on the number of parameter sets fed to the model. Trajectory numbers
are assigned to people uniformly and randomly. A user must provide enough trajectories that they
provide a representative sample of the underlying natural history parameters, but that choice is up
to the user.
