# A central natural history manager

## Overview

We provide an interface to read in user-specified natural history parameters, such as the generation
interval distribution, and query the value of natural history parameters in modules that manage an
individual's disease, like the transmission module. The goal of this module is to be flexible to
modeling any natural history parameters without making assumptions about their underlying and how
they vary over time.

Natural history parameters are those that describe an infection, and they include the generation
interval distribution (i.e., relative probability of transmitting disease over time since being
infected), the number of secondary infection attempts in total over the infection, viral load over
time, time to symptom onset/incubation period, and time from symptom onset to improvement. The
module expects samples of all of these parameters in the input CSV including a unique ID that ties
different parameters together to make one cohesive natural history parameter set that describes an
individual's infection. As such, all parameter samples with the same ID refer to the joint sample
from the natural history distributions. This choice allows for full flexibility over the natural
history parameter values and their relationship to each other.

## Data input format

The input CSV includes samples of the natural history parameters. The CSV expects parameters in a
long input format:

| id | weight | time | parameter | value |
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

The `id` is a unique identifier that marks a distinct sample of the natural history parameters. In
this example, `id = 0` describes the infection of an individual who has a symptomatic infection
because they have an incubation period and time to symptom improvement whereas `id = 1` describes
the infection of an individual who is asymptomatic because symptom-associated parameters are not
present in this natural history parameter set. This schema allows for describing different types of
infections in a single input file. The `weight` column describes the weight with which to sample
that particular infection archetype in the model. To add new parameters -- for instance, the viral
load -- add a row (or rows if the parameter varies in time) for each `id` for which this parameter
is relevant to the input CSV.

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
querying an individual's natural history parameters during their infection. To provide a value of a
natural history parameter that varies over time from the discrete samples fed as model inputs -- for
instance, calculating the viral load at a given time or estimating the inverse generation interval
from a uniform draw -- the natural history manager uses cubic spline interpolation.

## Assumptions
1. There are no requirements on the number of parameter sets fed to the model. Trajectory numbers
are assigned to people uniformly and randomly. The input CSV must contain enough trajectories that
they provide a representative sample of the underlying natural history parameters, but that choice
is up to the user.
2. For generation interval CDFs that are bounded -- meaning there is a maximum time since infection
at which a secondary infection can occur, the module assumes that the input CSV contains this
maximum time and CDF value.
