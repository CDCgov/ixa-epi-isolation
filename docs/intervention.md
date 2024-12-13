# Interventions

## Overview
Introducing interventions that affect the relative transmission potential in a given infection attempt, either through relative risk of the susceptible individual or through realtive infecitousness of the transmitter individual.

## Intervention manager
This trait extension on `Context` allows for querying population by infecitous status and intervention status. We have pursefully made the intervention type ambiguous, as an intervention of interest should be specified and enumerated in its own module.
We introduce the symmetrical function `query_relative_infectiousness` that uses a nested `HashMap` to ascertain the relative change in transmission potential. Taking a person ID, we obtain the intervention status and infectious status to query the float that determines risk. The `register_intervention` function then allows the nested `HashMap` to be created, registering the intervention relative transmission map beneath infectious status as a decision tree map within the intervention container.

The examples in the test section are implmented using specific cases from the facemask manager.

## Facemask Manager
Facemasks  are currently randomly assigned at a given maskiong rate specified in the parameters input JSON using the `assign_facemask_status` function. The `init` function registers facemask and infectious status types to their respective relative transmission potentials and then assigns individuals to either have `Wearing` or `None` for the facemask intervention.

## Impact on transmission
Currently, the relative transmission potential effet of interventions are deployed in the `evaluate_transmission` function of the transmission manager. Now, the probability of a successful transmission event depends on the additive relative transmission potential of the transmitter and contact as a result of the intervention.
