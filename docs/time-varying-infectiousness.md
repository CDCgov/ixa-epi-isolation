# Time-varying infectiousness

## Motivation

Compartmental models of disease spread often assume people transition between compartments at a
constant rate, implying exponential waiting times between events. Nevertheless, epidemiological
studies have shown that this is not the case. People do not have a constant level of infectiousness
nor do they spend an exponentially-distributed amount of time as infectiousness. Instead,
infectiousness over the course of an individual's infection varies widely, and we would like a
general way to model whatever distribution may best represent an agent's infectiousness.

Even when trying to incorporate more realistic infectiousness distributions into compartmental models,
significant math manipulations are required. On the other hand, in agent-based models (ABMs), we can
instead draw infection attempts from any specified infectiousness distribution. We detail the process
for sampling an arbitrary number of infection attempts appropriately from an arbitrary infectiousness
curve. We describe an approach that provides great generalizability, enabling sampling when both the
underlying contact rate and infectiousness distribution change. We motivate our discussion with a
simple example of an infectious person who becomes hospitalized and takes an antiviral partially
through their infection.

Modeling general interventions and immunity are not the subject of this document.

## Assumptions

1. Rather than considering any arbitrary distribution for infectiousness over time (epidemiologically,
this is the generation interval), let us consider the uniform distribution from zero to one --
$\mathcal{U}(0, 1)$.
    - Using [inverse transform sampling](https://en.wikipedia.org/wiki/inverse_transform_sampling), a
uniformly-distributed random variable can be transformed to any distribution by passing samples through
the inverse cumulative distribution function (CDF).
    - Therefore, we need to only consider how to solve the appropriate math for $u \sim \mathcal{U}(0, 1)$.
    - We also require that the generation interval have an inverse CDF or we be able to approximate it (i.e.,
from empirical data).
2. Infectiousness over time is a relative quantity, so it is completely separate from the absolute degree
to which someone is infectious, represented by the total number of secondary infection attempts they have.
That quantity, denoted $C_i$ in the generality, is directly related to $R_0$.
3. We require $C_i$ ordered draws from the generation interval distribution. We schedule infection attempts
at these times.

## Central claim

We could just take $C_i$ draws from the generation interval (GI) and order them, scheduling an infection
attempt at each time drawn. However, we argue that we instead want to draw the minimum of our $C_i$ draws,
have an infection attempt, and _then_ schedule the next infection attempt -- drawing the second
smallest of our $C_i$ ordered draws from the GI. We argue that we want to use this approach because (a) it
provides a great deal of flexibility, allowing for many changes to the GI, and (b) because it is more
computationally efficient than alternatives.

However, wanting to sequentially schedule the infection attempts requires that we have a way of getting
the smallest of $C_i$ draws from the GI, the second smallest, the third smallest, etc. This is the problem
of obtaining ordered draws from a distribution, or having information about the distribution's
"[order statistics](https://en.wikipedia.org/wiki/Order_statistic)", and it is readily solvable for the
case of the uniform distribution.

### Why do we need order statistics?

Let us first explore why scheduling the next infection attempt after the last (i.e., sequentially
drawing infection attempt times from the GI) is the better approach compared to taking all the draws
from the GI at the beginning of an individual's infectiousness period and at once scheduling the infection
attempts based on those times.

1. **Changes in the number of infection attempts in the middle of an agent's infectious course.** Imagine
an agent dies while they are still infectious. Clearly, they cannot be infecting others. (Or, if the disease
in question is Ebola, there are well-defined processes by which they may still be infectious and those processes
should not be lumped with the infection generation interval).

    To ensure a dead person does not infect others, we would like to remove the plans where they infect others.
If we had pre-scheduled all infections, we would have had to store the plan IDs for all the infections in
`HashMap<PersonId, Vec<PlanId>>`. Then, each time we had executed one of these plans, we would have to have
removed it from the vector, so that the entry for each `PersonId` tells us the plans we have _left_ for a given
person. Then, when the agent dies, we could iterate through the remaining plans and cancel them.

    Clearly, this is computationally onerous, requiring us to store a potentially large `HashMap` and iterate
through a vector every time an infection attempt occurs. Alternatively, if we scheduled infection events
sequentially, we would only have to store the next infection attempt time. We could instead use a
`HashMap<PersonId, PlanId>`, removing the need to iterate through a vector and instead enabling us to
directly use the `.insert` method on the `HashMap`. More generally, this idea applies to any case where we need
to change the number of infection attempts part way through an infection course. sequentially scheduling
the attempts makes it easier to remove infection attempts in the future.

    Why not just check whether the agent is alive or not at the beginning of the infection attempt? If they are
not alive, simply skip the infection attempt. There are two reasons this is not the desired solution. First, once
an agent is no longer relevant in the simulation, it is much cleaner to handle that at the moment when they are no
longer involved in the simulation rather than keeping them around and continuously writing code that checks whether
the agent is still relevant. Ideally, there will be defined methods in ixa for the teardown of people. Secondly,
the points made in this example are not pertinent to just an agent no longer being alive: they are pertinent to
any changes in the number of infection attempts that may occur during the course of an agent's infection. Imagine
an agent becomes hospitalized partially through their infection course. This change in their setting reduces
the number of infection attempts they have because they are coming into contact with fewer people. Checking
an agent's setting at the beginning of an infection attempt becomes cumbersome for many different settings,
and canceling some of the remaining infection plans like described above remains inefficient. On the other hand,
sequentially scheduling infection attempts enables arbitrary changes to the effective contact rate to occur while
an agent is infectious and have them be updated in the simplest way possible.

2. **Rejection sampling from an arbitrary generation interval.** Let us consider a case where the infectiousness
distribution changes over time. Concretely, imagine that infectiousness is a function of the viral load,
and an agent gets an antiviral partially through their infection course. Depending on when the agent gets the
antiviral, they have a different reduction in viral load. Therefore, even if we know that an agent will
get an antiviral, we don't know their infectiousness distribution in advance.

    It is still straightforward to sample the appropriate infectiousness distribution without needing to change
the generation interval. We can just use [rejection sampling](https://en.wikipedia.org/wiki/Rejection_sampling).
In this case, we still sample infection attempt times from the pre-antiviral infectiousness distribution, and
we accept those times as actual infection attempts with probability $a(t) / g(t)$ where $a(t)$ is the
post-antiviral infectiousness distribution and $g(t)$ is the underlying pre-antiviral infectiousness
distribution. Note that $a(t)$ and $g(t)$ must be on an absolute scale in this example and not scaled to have
a unit integral. In the case where they are scaled, $g(t)$ can be rescaled to be $Mg(t)$ where $M = \max a(t)$.

    This general idea of rejection sampling is useful for other potential applications. Consider the case
where each person has a different generation interval. For instance, imagine that we have estimates of
infectiousness over time from a within-host viral kinetics model (once again assuming infectiousness is a
function of viral load). We have a different value of the generation interval for each person because our
model produces a posterior distribution of infectiousness over time, and we assign each person a draw from
this posterior distribution. Instead of sampling from the person-specific generation interval, we might
want to sample from one overall generation interval that's shared among all agents. This makes it easier for
us to abstract the sampling part of our code away from the `evaluate_transmission` part. In this case, we
would only accept a fraction of the sampled times as actual transmission events, a fraction that varies over
the infection time and depends on the infected individual's specific generation interval parameters. The setup
is the same as above. We would want to sample from our overall distribution ("proposal distribution") at
a faster rate than any of the person-specific distributions. Let us call the probability density of the
proposal distribution from which we sample at a faster rate $s(t)$, so that we sample from $Ms(t)$.
$M$ is a scalar that ensures that rate of sampling from $Y$ is always faster than the rate of the generation
interval. We can still recover the per-person infectious distribution: we only accept $g_i(t) / Ms(t)$ draws from $Y$.

    However, rejection sampling is inherently inefficient. It requires that we draw plans at a faster rate than events
actually happen, and then we need to evaluate at the time of the event draw whether we mean for something to actually
happen. Nevertheless, there are ways to improve efficiency. The trivial case for making the sampling rate faster than
the event rate is to have us sample at the maximum event rate, so that $s(t) = 1$ and $M = \max g(t)$. If
$g_i(t) << \max g(t)$ for nearly all $t$, (i.e., a disease where infectiousness is highly concentrated around a given time
or a case where there is significant per-person variability in when an agent has their maximum infectiousness) this is
particularly inefficient because we are rejecting the majority of samples. Instead, we may try making our proposal
distribution better fit our underlying distribution. We may make $s(t)$ a similar linear approximation for $g(t)$.

    However, this approximation is only possible if we sequentially sample infection attempts. If we sample all
infection attempts at the beginning of an agent's infection, we are locked into using a singular sampling rate. This
is by definition because we have sampled all attempts at once at the beginning of the infection course. Instead, we
want to update our sampling rate as we go throughout the infection course to best follow the generation interval
(whether we exactly follow the generation interval and don't use rejection sampling or still use a proposal distribution
is irrelevant to the mechanics). To update our sampling rate, we must sequentially sample infection attempts, calculating
the optimal sampling rate at each infection attempt. Thus, sequential sampling enables us to most generally model a
generation interval or infectiousness distribution that may change over the course of an agent's infection in a non
pre-defined way.

These examples underscore that sequentially sampling infection attempts rather than pre-scheduling enables
the modeler to consider changes in both the contact rate and the generation interval without needing to change
the transmission model. To this end, by sequentially sampling, we provide a modular transmission model that is
truly disentangled from the contact network and interventions present.

## Drawing ordered samples from a uniform distribution

We briefly describe the math behind order statistics -- in other words, the math needed to obtain the distribution
of the minimum of $n$ samples from a uniform distribution, the second smallest value of $n$ from the uniform, etc.
For further detail, the [Wikipedia](https://en.wikipedia.org/wiki/Order_statistic) page on order statistics is a
great reference as are various notes on the subject from
[advanced statistics courses](https://colorado.edu/amath/sites/default/files/attached-files/order_stats.pdf).

Let us begin by considering the minimum of $n$ draws from a uniform distribution. We denote the sorted
first of these draws as $X_{(1)}$, the sorted second as $X_{(2)}$, etc. Note that this is different from $X_1$
which is the first random draw from the distribution, unsorted. We are interested in the distribution of $X_{(1)}.
Let us consider the cumulative distribution function since we are working with a continuous random variable.

$$\mathbb{P}\{X_{(1)} \leq x\}$$

We know that if the minimum is below some value, $x$, that could mean that just the minimum is below $x$ or that
all $n$ of our random samples are below the minimum. There are too many values to enumerate, so let us consider
the opposite instead -- that $X_{(1)}$ is greater than some value $x$.

$$\mathbb{P}\{X_{(1)} \leq x\} = 1 - \mathbb{P}\{X_{(1)} > x\}$$

In this case, if $X_{(1)} \leq x$, we know that at least one of the sampled values is below $x$. Recall that
each of the samples is independent and identically-distributed.

$$= 1 - \mathbb{P}\{X_i > x\}^n$$

$$= 1 - (1 - F_X(x))^n$$

For a uniform distribution:

$$= 1 - (1 - x)^n$$

Careful inspection reveals that this is a beta distribution with $alpha = 1$ and $beta = n$. More generally,
one can show that the distribution of the $k$th ordered value of $n$ uniform draws is $\beta(k, n - 1 + k)$.

Therefore, we can draw a distribution for the timing of the first of $n$ infection attempts, the second of $n$
infection attempts, etc. from a uniform distribution using the Beta distribution.

However, we cannot just independently draw from the distributions for the ordered values. Instead, we must
update our distributions to be conditioned on the previous draws to ensure that are draws are always increasing.
In other words, if we draw from the $\beta(1, 5)$ and happen to get a large value, we need to take that into
account when drawing the second infection attempt time. It must be greater than the first value we drew, so we cannot
just independently take a draw from $\beta(2, 6)$ but rather $\beta(2, 6) | x_{(1)}$.

We can consider that our sorted infection attempt times are not independent by rephrasing the problem.
Once we have taken our first infection attempt, $x_{(1)}$, we can set this as the minimum of a new uniform
distribution, $\mathcal{U}(x_{(1)}, 1)$ from which we need to draw an infection attempt. However, because this
is a new distribution, we now want to draw the first of $n - 1$ infection attempts on this distribution.
We can do that by drawing the minimum of $n - 1$ infection attempts from $\mathcal{U}(0, 1)$, and
then we can scale that value to be on $(x_{(1)}, 1)$. In other words, we are using a trick where we shrink the
available uniform distribution with each infection attempt and ask what would be the _next_ infection
attempt on that distribution rather than attempting to do all the math on $\mathcal{U}(0, 1)$. Concretely,
if $x_{(1)}$ is on $(0, 1)$, and $x_{(2)}$ is also on $(0, 1)$ and we want to find $x'_{(2)}$ which is $x_{(2)}$
on $(x_{(1)}, 1)$, we use the following equation:

$$x'_{(2)} = x_{(1)} + x_{(2)} * (1 - x_{(1)})$$

## Workflow and approach

Finally, we describe the steps for taking sequential samples from an arbitrary generation interval.
These are the steps for simulating an agent's infectious period and their person-to-person transmission
events.

1. Draw a number of secondary infection attempts for the agent, $C_i$. This can be equal to $R_i$ if the sampling times
are from the generation interval exactly, or $C_i$ can be greater than $R_i$ if sampling from a proposal distribution
and rejecting some attempts to leave a total of $R_i$ infection attempts.
2. Draw the time for the first of $n$ remaining infection attempts of $\mathcal{U}(0, 1)$. $n = R_i - $ (the number
of infection attempts that have happened).
3. Scale the value on $\mathcal{U}(0, 1)$ to be on $\mathcal{U}(x_{(i)}, 1)$ where $x_{(i)}$ is the previous draw (the
greatest value of the uniform distribution seen before) and physically represents the proportion of the generation interval
that has occured so far.
4. Convert the uniform value to generation interval space by passing through the inverse CDF of the generation interval.
Schedule the infection attempt to occur at the modeled time, and wait to schedule the next infection attempt until the end
of the plan at that given time.
5. Repeat from step two until $n = 0$.
