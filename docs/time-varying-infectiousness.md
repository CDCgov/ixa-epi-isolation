# Time-varying infectiousness

## Motivation

Compartmental models of disease spread often assume people transition between compartments at a
constant rate, implying exponential waiting times between events. This assumption is particularly
error-prone when considering the time an individual spends infectious and the time between their
subsequent infection events -- i.e., when they are infectious enough to be actively transmitting.
Epidemiological studies often show that infectiousness over the course of an individual's infection
varies widely, and we would like a general way to model whatever distribution may best represent an
agent's infectiousness. Even when trying to incorporate more realistic infectiousness distributions
in compartmental models, significant math manipulations are required. On the other hand, in agent-
based models (ABMs), we can instead draw infection attempts from any specified infectiousness
distribution. We detail the process for sampling an arbitrary number of infection attempts appropriately
from an arbitrary infectiousness curve. We describe how we may extend this framework to also including
arbitrary interventions, immunity, and even antivirals that may change the infectiousness curve over time.

## Overview



##
