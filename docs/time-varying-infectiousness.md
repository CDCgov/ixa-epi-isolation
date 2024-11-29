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
from an arbitrary infectiousness curve. We describe why this approach provides great generalizability,
enabling sampling from an infectiousness curve that may change over time due to an antiviral.

Modeling interventions and immunity are not the subject of this document.

## Assumptions

1. Rather than considering any arbitrary distribution for infectiousness over time (epidemiologically,
this is the generation interval), let us consider the uniform distribution from zero to one --
$\mathcal{U}(0, 1)$.
    - Using [inverse transform sampling](https://en.wikipedia.org/wiki/inverse_transform_sampling), a
uniformally-distributed random variable can be transformed to any distribution by passing samples through
the inverse cumulative distribution function (CDF).
    - Therefore, we need to only consider how to solve the appropriate math for
$u \sim \mathcal{U}(0, 1)$.
2. Infectiousness over time is a relative quantity, so it is completely separate from the absolute degree
to which someone is infectious, represented by the total number of secondary infection attempts they have.
That quantity, denoted $C_i$, is directly related to $R_0$.
3. We require $C_i$ ordered draws from the generation interval distribution. We schedule infection attempts
at these times.

## Central claim

We could just take $C_i$ draws from the generation inteval (GI) and order them, scheduling an infection
attempt at each time drawn. However, we instead want to draw the minimum of our $C_i$ draws, have an
infection attempt, and _then_ schedule the next infection attempt, in other words drawing the second
smallest of our $C_i$ ordered draws from the GI. We want to use this approach because (a) it is the
most flexible, allowing for many changes to the GI, and (b) because it is more computationally
efficient than alternatives.

However, wanting to sequentially schedule the infection attempts requires that we have a way of getting
the smallest of $C_i$ draws from the GI, the second smallest, the third smallest, etc. This is the problem
of obtaining ordered draws from a distribution, or having information about the distribution's
"[order statistics](https://en.wikipedia.org/wiki/Order_statistic)", and it is readily solvable for the
case of the uniform distribution.

### Why do we need order statistics?

Let us first explore why scheduling the next infection attempt after the last (i.e., sequentially
drawing infection attempt times from the GI) is the better approach compared to taking all the draws
from the GI at the beginning of an individual's infectiousness period and scheduling all the infection
attempts based on those times.

1. **Changes in the number of infection attempts in the middle of an agent's infectious course.** Imagine
an agent dies while they are still infectious. Clearly, they cannot be infecting others. (Or, if the disease
in question is ebola, there are well-defined processes by which they may still be infectious and that process
should not be lumped with the infection generation interval).

    To ensure a dead person does not infect others, we would like to remove the plans where they infect others.
If we had pre-scheduled all infections, we would have had to store the plan IDs for all the infections in
`HashMap<PersonId, Vec<PlanId>>`. Then, each time we had executed one of these plans, we would have to have
removed it from the vector, so that the entry for each `PersonId` tells us the plans we have _left_ for a given
person. Then, we could iterate through the remaining plans and cancel them.

    Clearly, this is computationally onerous, requiring us to store a potentially large `HashMap` and iterate
through a vector every time an infection attempt occurs. Alternatively, if we scheduled infection events
sequentially, we would only have to store the next infection attempt time. We could instead use a
`HashMap<PersonId, PlanId>`, removing our need to iterate through a vector and instead enabling us to
directly use the `.insert` method on the `HashMap`. More generally, if we need to change the number of
infection attempts part way through an infection course, sequentially scheduling the attempts makes this easier.

    Why not just check whether the agent is alive or not at the beginning of the infection attempt? If they are
not alive, simply skip the infection attempt. There are two reasons this is not the desired solution. First, once
an agent is no longer relevant in the simulation, it is much cleaner to handle that at the moment when they are no
longer involved in the simulation rather than keeping them around and continuously writing code that checks whether
the agent is still relevant throughout the simulation. Ideally, there will be defined methods in ixa for the teardown
of people. Secondly, the points made in this example are not pertinent to just an agent no longer being alive: they
are pertinent to any changes in the number of infection attempts that may occur during the course of an agent's
infection. Imagine an agent becomes hospitalized partially through their infection course. This change in their
setting reduces the number of infection attempts they have because they are coming into contact with fewer people.
Checking an agent's setting at the beginning of an infection attempt becomes cumbersome for many different settings,
and canceling some of the remaining infection plans like described above remains inefficient. On the other hand,
sequentially scheduling infection attempts enables arbitrary changes to the effective contact rate to occur while
an agent is infectious and have them be updated in the simplest way possible.

2. **Rejection sampling from an arbitrary generation interval.** Let us consider that we may want to sample at a
time _faster_ than the infection attempts from the generation interval. Let us call this proposal distribution
from which we sample at a faster rate, $Y$ so that it has some probability density $s(t)$ and we sample from
$Ms(t)$. $M$ is a scalar that ensures that rate of sampling from $Y$ is always faster than the rate of the generation
interval. We can still recover the generation interval distribution: if $g(t)$ is the generation interval distribution,
we only accept $g(t) / Ms(t)$ draws from $Y$. This is the idea behind
[rejection sampling](https://en.wikipedia.org/wiki/Rejection_sampling). Note that even if we draw samples from a uniform
distribution and convert them to the generation interval, we can change the CDF that we use to convert to have sampling
occur at a faster rate. So, we are still working under the assumption that all draws are from a uniform distribution;
we are now interested into what distribution we should be transforming those draws (i.e., what inverse CDF to use).

    Why might this be helpful? Let us consider a few use cases. First, let us consider the case where each person
has a different generation interval. For instance, imagine that we have estimates of infectiousness over time
from a within-host viral kinetics model that assumes infectiousness is some function of the viral load. We have
a different value of the generation interval for each person because our model produces a posterior distribution
of infectiousness over time, and we assign each person a draw from this posterior distribution. Instead of sampling
from the person-specific generation interval, we might find the maximum of all these generation intervals and sample
from that. This makes it easier for us to abstract the sampling part of our code away from the `evaluate_transmission`
part. In this case, we would only accept a fraction of the sampled times as actual transmission events, a fraction
that varies over the infection time and depends on the infected individual's specific generation interval parameters.

    Secondly, continuing with the idea of infectiousness being a function of viral load, imagine that we are
simulating a case where antivirals become available to the population at some point in the outbreak. When an
individual gets an antiviral, their viral load will change, changing their infectiousness over time. Since each
person may get an antiviral at a different time in their infection and it may have a different effect on them,
rather than changing to sample from the viral load in the presence of an antiviral, we can keep on sampling from
the previous viral load function, but now only accepting a fraction of samples as infection attempts.

    However, rejection sampling is inherently inefficient. It requires that we draw plans at a faster rate than events
actually happen. Nevertheless, there are ways to improve efficiency. The requirement for rejection sampling is that
we must be sampling at a rate faster than the rate at which events actually happen. The trivial case of this
is to have us sample at the maximum rate, so that $s(t) = 1$ and $M = \max g(t)$. If $g(t) << \max g(t)$ for nearly
all $t$, (i.e., a disease where infectiousness is highly concentrated around a given time) this means we are sampling
at a much faster rate and creating many more events than we need to for most all $t$. This will slow down our simulation.
Instead, we may like to make $s(t)$ a similar linear approximation for $g(t)$, so that our proposal distribution is more
closely following our actual generation interval. Thus, continuing with the idea of people getting an antiviral that
adjusts their infectiousness, once an agent gets an antiviral, we can sample from our linear approximation of $g(t)$ that
is still always greater than $g(t)$. More generally, the rejection sampling here is a strategy for dealing in the most
arbitrary sense with

More generally

These examples underscore that sequentially sampling infection attempts rather than pre-scheduling enables
the modeler to consider changes in both the contact rate and the generation interval without needing to change
the transmission model. To this end, by sequentially sampling, we provide a modular transmission model that is
truly disentangled from the contact network and from interventions present.

## Drawing ordered samples from a uniform distribution

We briefly describe the math behind order statistics -- in other words, the math needed to obtain the distribution
of the minimum of $n$ samples from a uniform distribution, the second smallest value of $n$ from the uniform, etc.
For further detail, the [Wikipedia](https://en.wikipedia.org/wiki/Order_statistic) page on order statistics is a
great reference as are various notes on the subject from
[advanced statistics courses](https://colorado.edu/amath/sites/default/files/attached-files/order_stats.pdf).

Let us begin by considering the minimum of $n$ draws from a uniform distribution.

## Workflow and approach

Finally, we describe the steps for taking sequential samples from an arbitrary generation interval.
These are the steps for simulating an agent's infectious period and their person-to-person transmission
events.

1. Draw a number of secondary infection attempts for the agent, $C_i$. This can be equal to $R_i$ if the sampling times
are from the generation interval exactly. or, $C_i$ can be greater than $R_i$ if
2. Draw a beta
3.
