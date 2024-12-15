# Natural history model inputs

## Infectiousness over time

We provide a way for reading in a user-specified infectiousness over time distribution (generation interval)
and appropriately scheduling infection attempts based on the distribution. The user provides an input file
that contains samples from the cumulative distribution function (CDF) of the generation interval (GI) over
time at a specified $\Delta t$, describing the fraction of an individual's infectiousness that has passed
by a given time. The input data are assumed to have a format where the columns represent the times since
the infection attempt (so starting at $t = 0$) and the entries in each row describe the value of the GI
CDF. Each row represents a potential trajectory of the GI CDF.

People are assigned a trajectory number (row number) when they are infected. This allows for each person
to have a different GI CDF if each of the trajectories are different. However, that trajectory number will
be used for also drawing the person's other natural history characteristics, such as their symptom onset
and improvement times or viral load trajectory. This allows easily encoding correlation between natural
history parameters (the user provides input CSVs where the first row in each CSV is from a joint sample
of GI, symptom onset, symptom improvement, etc.) or allowing each of the parameters to be independent.

## Overall Assumptions
1. There are no requirements on the number of trajectories fed to the model. Trajectory numbers are assigned
to people uniformly and randomly. However, this means that an individual who is reinfected could have the exact
same infectiousness trajectory as their last infection.
2. There must be the same number of parameter sets for each parameter provided as an input CSV. For now, we are focusing
only on GI, but we will soon expand our work to also include symptom onset and symptom improvement times.
3. We have not yet crossed the barrier of how to separately treat individuals who are asymptomatic only. Are their
GIs drawn from a separate CSV? Should their $R_i$ just be multiplied by a scalar? Part of the reason we are deferring
this decision is because our previous isolation guidance work focused only on symptomatic individuals.
