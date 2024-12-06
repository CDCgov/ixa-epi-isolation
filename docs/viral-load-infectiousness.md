# Infectiousness over time

## Overview

We assume that infectiousness over time varies with the viral load, similar to how we modeled infectiouness over time in our isolation guidance modeling response.
For now, we assume that infectiousness is proportional the log of the viral load (i.e., our base assumption in our isolation guidance work),
but we anticipate expanding to allow for other functional relationships, potentially even calibrating the functional form in our model.

For each person, we draw a set of viral load parameters and associated symptom improvement time from our isolation guidance posteriors.
We append to that a valid time to symptom onset modeled as described in Park et al., (2023) PNAS because our viral load parameters are all relative to the time of symptom onset.
For our ABM, we need to know infectiousness over time from when an agent gets infected, so we need an agent's incubation period to appropriately shift the triangle VL curve to be relative to when an agent was infected.

## Assumptions
1. We assume that there is no infectiousness below a log VL of 0.
2. We draw random samples from the set of triangle VL parameters for each agent, not requiring that there be as many parameter sets as there are agents.
However, for now, the parameter sets are person-specific.
We do this to enable being able to get the same parameter set for a person in multiple modules (we need symptom onset and improvement times in the health status module),
but this also means that for now an agent who is reinfected will have the exact same generation interval as in their previous infection unless we account for the number of previous infections when sampling parameter sets.
3. We do not allow for agents to be infectious before they are infected, and we do not truncate the triangle VL curve for any agents.
Because transmission starts `peak_time - proliferation_time`  days before symptom onset, this puts a constraint on the individual's incubation period.
We draw symptom onset times from the incubation period specified in Park et al., (2023) PNAS, and we constrain the drawn times to enforce that symptom onset happens after enough of the proliferation period has elapsed to ensure the agent experiences their full infectious course after getting infected.
4. We have not yet crossed the barrier of how to model transmission from asymptomatic individuals, considering that our VL model was fit to only symptomatic individuals.
One option is to still use the same model of infection but change the mean number of infection attempts such individuals have.
