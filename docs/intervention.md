# Current API

## Overview
Introducing interventions that affect the relative transmission potential in a given infection attempt, either through relative risk of the susceptible individual or through realtive infecitousness of the transmitter individual.

## Intervention manager
This trait extension on `Context` allows for querying population by infecitous status and intervention status. We have pursefully made the intervention type ambiguous, as an intervention of interest should be specified and enumerated in its own module.
We introduce the symmetrical function `query_relative_infectiousness` that uses a nested `HashMap` to ascertain the relative change in transmission potential. Taking a person ID, we obtain the intervention status and infectious status to query the float that determines risk. The `register_intervention` function then allows the nested `HashMap` to be created, registering the intervention relative transmission map beneath infectious status as a decision tree map within the intervention container.

## Facemask Manager
Facemasks  are currently randomly assigned at a given maskiong rate specified in the parameters input JSON using the `assign_facemask_status` function. The `init` function registers facemask and infectious status types to their respective relative transmission potentials and then assigns individuals to either have `Wearing` or `None` for the facemask intervention.

## Impact on transmission
Currently, the relative transmission potential effet of interventions are deployed in the `evaluate_transmission` function of the transmission manager. Now, the probability of a successful transmission event depends on the additive relative transmission potential of the transmitter and contact as a result of the intervention.

# Proposed API
## Intervention manager
We want to be able to register multiple interventions simultaneously without interference or requiring excessive calls to the same functions with single intervention inputs. The multiple interventions should therefore allow the user to specify how they interact to impact relative transmission and, crucially, be callable as vectors into manager functions. The interventions should be able to be specified as either modifying transmissiveness (e.g. facemasks), contact rate (e.g. isolation), or possibly both (e.g. physical distance).

All variants of a particular intervention Type will be registered simultaneously, modelled on the query API. It may also be the case that we want some derived property of the current context to map to an effect on transmission, so a closure input option will be added to the register function. In order to retreive these registrations, we'll need functions that likewise accept vectors of intervention Type ID's.

Calculating the effect of nested intervention combinations on transmission should depend on a vector of interventions, not a singly specified intervention type. This can be handled by a second register function that determines the relationship between a vector of `Vec<(TypeId, f64)>` tuples. This calculation function should be external to `query_relative_transmission` so that the probability of successful transmission is independent of the infection attempt.

## Facemasks
Individuals wear facemasks (of any form) at some base rate or wear masks according to markers of disease progression with probabilities that follow qualitative guidance. We therefore want to assign masking in a way that is user-specified, as a single function or through some policy manager.
