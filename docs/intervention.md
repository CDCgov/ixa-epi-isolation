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


# Steps
Define some intervention and then actually implement the intervention trait for some object. To do this, we'll need to (1) register the intervention and (2) grab the associated value of the transmissiveness, as in the original API.

What is the intervention trait and where does the context trait extension appear? Three plugins (Transmission, intervention, contact), which interact through context.

## Identifying transmission modifiers
Relative transmission modifiers have elements that can either be affected by the innate transmissiveness or contact modules or can belong exclusively to those other modules. For example, facemasks modify the relative transmission potential of an infection attempt, but the decision to wear a facemask based on a person's risk category or symptom category is an intervention-level behavior that does not directly modify transmissiveness. However, symptoms may modify the efficacy of wearing a facemask, and this may need to be included in the registration. Therefore, in this module we have to understand how all potential modifiers interact with one another and extract only the explicit ways that they modify transmissiveness.

Per the respiratory disease isolation guidance, prevention entails staying up to date on vaccination, practicing good hygiene, and taking steps for cleaner air. The guidance targets mitigation of community spread, the prevention of further transmission, by recommending isolation at home and medication to reduce symptoms followed by more steps for cleaner air, enhanced hygiene practices, wearing a facemask, maintaining distance, and testing. The guidance states that such practices are more important for those 65 or older and those with weakened immune systems.

Comprehensive list of person property modifiers that are outlined in the isolation guidance:
<!-- In the final documentation focus on immediate proposed implementation -->
- Vaccination status
    (Person property that is registered to a function for vaccine efficacy)
- Vaccine efficacy
    (Function of person properties and time since vaccination to return modifier on immunity function)
- Anti-virals
    (Function of person properties to return modifier of symptom function)
- Covering coughs and sneezes
    (returns a float interacting with the symptoms function)
- Handwashing (might be distinct cdependning on interpretation of communicability)
- Maintaining distance
    (Returns a float)
- Wearing facemasks (adherence heterogeneity)
    (Person property that returns a float interacting with the symptoms function)
- Facemask efficacy
    (Registered float)

Elements related to these that are orthogonal to relative transmissiveness
- Uptake and adherence rates (policy/intervention manager)
- Testing (affect downstream behavior of individuals)
- Cleaning surfaces (different mechanism of spread)
- Isolation (changes contact rates)

Although some of these features may also affect compliance, symptoms, or other behaviors, we only consider here their direct effects on risk, infectiousness, and secondary modifier changes. Such secondary modifiers may include person properties, such as:
- Age (alters risk and interacts with vaccine/medication efficacy)
- Cross-protective immunity, either innate or acquired through vaccine or infection (alters risk)
- Symptoms (alters infectiousness, interacts with covering coughs, medication efficay, features of interaction location such as distance and outdoors, and facemask efficacy)
- Whether an individual is susceptible or infectious (determines whether risk or infectiousness is targeted for modification)

Or characteristics of the location of the interaction, such as:
- Home, work, school (location/type of interactions would alter the efficacy of air purification, facemask efficacy through time spent unmasked, maintaining distance)
- between random or known persons (this may only be a modifier of contact rate but could also modify facemaks adherence or maintaining distance)
- Purifying indoor air (HVAC filters or air purifiers)
- Opening windows for fresh air
- Gathering outdoors
- Outside and inside are properties of the location of interaction (these may be included indirectly by reducing transmissiveness by some amount due to time spent outdoors)

There are therefore three categories of infection risk modifiers - those that are dependent on a `pub enum` that defines the values of a `PersonProperty`, those that are functions of a `PersonId` and `context` state, and those that depend on where the interaction occurs. The third modifier could also alter the mapping or function of the other two, such as whether a person shows lower masking adherence at home.

The third may be better implemented in concert with get_contact, so that the location of interaction is tied with its effect on transmission probability.
