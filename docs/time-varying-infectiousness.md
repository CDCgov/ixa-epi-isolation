# Time-varying infectiousness

## Motivation

Many epidemiological studies have shown that infectiousness varies widely over the course
of the individual's infection. Agent-based models (ABMs) are able to simulate events
that occur from any underlying probability distribution. We detail the process for sampling
an arbitrary number of infection attempts appropriately from an arbitrary infectiousness curve.
We describe an approach that provides great generalizability, enabling sampling when both the
underlying contact rate and infectiousness distribution change. We motivate our discussion with a
simple example of an infectious person who becomes hospitalized and takes an antiviral partially
through their infection.

## Assumptions

1. We describe the approach for the uniform distribution from zero to one -- $\mathcal{U}(0, 1)$.
    - Using [inverse transform sampling](https://en.wikipedia.org/wiki/inverse_transform_sampling), a
uniformly-distributed random variable can be transformed to any distribution by passing samples through
the inverse cumulative distribution function (CDF).
    - This requires that the infectiousness over time distribution has an inverse CDF or we are
    able to approximate it (i.e., from empirical data).
2. Infectiousness over time is a relative quantity, so it is completely separate from the absolute degree
to which someone is infectious, represented by the total number of secondary infection attempts they have.
That quantity, denoted $C_i$ in the generality, is directly related to $R_0$.
3. We require $C_i$ ordered draws from the infectiousness over time/generation interval distribution (time
from an individual becoming infected to their secondary contacts becoming infected). We schedule infection
attempts at these drawn times.

## Overall Methodology

We could just take $C_i$ draws from the generation interval (GI) and order them, scheduling an infection
attempt at each time drawn. However, we argue (in detail [below](#why-do-we-need-order-statistics)) that
it is more robust to draw the minimum of our $C_i$ draws, evaluate an infection attempt at that time,
and _then_ schedule the next infection attempt -- drawing the second smallest of our $C_i$ ordered draws
from the GI.

Drawing the smallest, the second smallest, the third smallest, etc. of a set number of random draws
from a distribution is the problem of having the distribution's
[order statistics](https://en.wikipedia.org/wiki/Order_statistic)", and it is readily solvable for the
case of the uniform distribution.

## Drawing ordered samples from a uniform distribution

Let us begin by considering the minimum of $n$ draws from a uniform distribution. We denote the sorted
first of these draws as $X_{(1)}$, the sorted second as $X_{(2)}$, etc. Note that this is different from $X_1$
which is the unsorted first random draw from the distribution, unsorted. We are interested in the distribution of
$X_{(1)}$. Let us consider the cumulative distribution function of $X_{(1)}$ since it is a continuous random variable.

$$\mathbb{P} \\{X_{(1)} \leq x \\}$$

We know that if the minimum is below some value, $x$, that could mean that just the minimum is below $x$ or that
all $n$ of our random samples are below the minimum. There are too many values to enumerate, so let us consider
the opposite instead -- that $X_{(1)}$ is greater than some value $x$.

$$\mathbb{P} \\{X_{(1)} \leq x \\} = 1 - \mathbb{P} \\{X_{(1)} > x \\}$$

In this case, if $X_{(1)} > x$, we know that all of sampled values are at least above $x$. Recall that
each of the samples is independent and identically-distributed.

$$= 1 - \mathbb{P} \\{X_i > x \\}^n$$

$$= 1 - (1 - F_X(x))^n$$

For a uniform distribution:

$$= 1 - (1 - x)^n$$

This is the CDF for a Beta distribution with $alpha = 1$ and $beta = n$. More generally, the distribution
of the $k$th infection attempt from $n$ total infection attempts is $\beta(k, n - 1 + k)$.

However, we cannot just independently sample from these beta distributions. Instead, we must update the distributions
to be conditioned on the previously drawn times to ensure that our infection attempt times are always increasing.
In other words, if we draw the first infection attempt time from $\beta(1, 5)$ and happen to get a large value, we need
to take that into account when drawing the second infection attempt time. It must be greater than the first
value we drew, so we cannot just independently take a draw from $\beta(2, 6)$ but rather $\beta(2, 6) | x_{(1)}$.

We can consider that the second infection time is not independent from the first, the third not independent from
the first and the second, etc. by rephrasing the problem. Once we have taken our first infection attempt, $x_{(1)}$,
we know that any subsequent infection attempt times must be greater. We can set $x_{(1)}$ as the minimum of a new uniform
distribution, $\mathcal{U}(x_{(1)}, 1)$, from which we need to draw an infection attempt. Because this is a new distribution,
we want the first of $n - 1$ infection attempt times on this distribution. We can do that by drawing the
minimum of $n - 1$ infection attempts from $\mathcal{U}(0, 1)$, and scaling that value to be on $(x_{(1)}, 1)$.
In other words, we are using a trick where we shrink the available uniform distribution with each infection
attempt and ask what would be the _next_ infection attempt on that distribution. Concretely, if $x_{(1)}$ is
on $(0, 1)$, and $x_{(2)}$ is also on $(0, 1)$ and we want to find $x'_{(2)}$ which is $x_{(2)}$ on $(x_{(1)}, 1)$,
we use the following equation:

$$x'_{(2)} = x_{(1)} + x_{(2)} * (1 - x_{(1)})$$

For further detail, the Wikipedia [page on order statistics](https://en.wikipedia.org/wiki/Order_statistic) is a
great reference as are various notes on the subject from
[advanced statistics courses](https://colorado.edu/amath/sites/default/files/attached-files/order_stats.pdf).

## Workflow

1. Draw a number of secondary infection attempts for the agent, $C_i$. This can be equal to $R_i$ if the sampling times
are from the generation interval exactly, or $C_i$ can be greater than $R_i$ if sampling from a
[proposal distribution](#rejection-sampling-from-an-arbitrary-generation-interval) and rejecting some attempts to leave
a total of $R_i$ infection attempts.
2. Draw the time for the first of $n$ remaining infection attempts of $\mathcal{U}(0, 1)$ by taking a random value from
$\beta(1, m)$. $m = C_i - $ (the number of infection attempts that have occured).
3. Scale the value on $\mathcal{U}(0, 1)$ to be on $\mathcal{U}(x_{(i)}, 1)$ where $x_{(i)}$ is the previous draw.
4. Convert the uniform value to generation interval space by passing it through the inverse CDF of the generation interval,
and schedule the next infection attempt at the specified time. Wait until that time has occured in the simulation before
proceeding.

    The result of passing the uniform time through the GI's inverse CDF is the time _since_ the agent first become
infectious at which the given $n$th infection attempt occurs. To determine the amount of time _elapsed_ until the next
infection attempt, given that the agent is currently at their $n-1$th infection attempt, schedule the next infection
attempt to occur in how much ever time remains until that infection attempt from the last attempt. In other words, subtract
the calculated time from the time since the agent became infectious of the current infection attempt, and schedule the next
infection attempt to occur in that much time.

5. Repeat from step two until $m = 0$.

## Why do we need order statistics?

We provide a detailed justification of why scheduling the next infection attempt after the last (i.e., sequentially
drawing infection attempt times from the GI) is the better approach compared to taking all the draws
from the GI at the beginning of an individual's infectiousness period and at once scheduling the infection
attempts based on those times.

### Changes in the number of infection attempts in the middle of an agent's infectious course

Imagine an agent dies while they are still infectious. Clearly, they cannot be infecting others. (Or, if the
disease in question is Ebola, there are well-defined processes by which they may still be infectious and
those processes should not be lumped with the infection generation interval.)

To ensure a dead person does not infect others, we would like to remove the plans where they infect others.
If we had pre-scheduled all infections, we would have had to store the plan IDs for all the infections in
`HashMap<PersonId, Vec<PlanId>>`. Then, each time we had executed one of these plans, we would have to
remove it from the vector, so that the entry for each `PersonId` tells us the plans we have _left_ for a given
person. Then, when the agent dies, we could iterate through the remaining plans and cancel them.

Clearly, this is computationally onerous and cumbersome, requiring us to store a `HashMap` and iterate
through a vector every time an infection attempt occurs. Alternatively, if we scheduled infection events
sequentially, we would only have to store the next infection attempt time. We could instead use a
`HashMap<PersonId, PlanId>`, removing the need to iterate through a vector and instead enabling us to
directly use the `.insert` method on the `HashMap`.

More generally, this idea applies to any case where we need to change the number of infection attempts
part way through an infection course. Sequentially scheduling the attempts makes it possible to accomodate
changes to the number of infection attempts that may happen in the middle of an infection course.

Why not just check whether the agent is alive or not at the beginning of the infection attempt? If they are
not alive, simply skip the infection attempt. There are two reasons this is not the desired solution. First,
it is cleaner to handle removing an agent at the point at which they become no longer relevant rather than
writing code that continuously checks for an agent's relevance. Secondly, there are other cases when the number
of infection attempts may need to change partially through an infection. Imagine an agent becomes hospitalized
after becoming infected. If contact rates are lower in the hospital, the number of contacts the agent may
decrease. Clearly, explictly checking an agent's setting, alive status, etc. at the beginning of an infection
attempt becomes not only cumbersome for many different settings but does not encourage modular separation
of the transmission workflow from other parts of the model. On the other hand, sequentially scheduling infection
attempts enables arbitrary changes to the effective contact rate to occur while an agent is infectious and have
them be updated in the simplest way possible.

### Rejection sampling from an arbitrary generation interval

Let us consider a case where the infectiousness distribution changes over time. Concretely, imagine that
infectiousness is a function of the viral load, and an agent gets an antiviral partially through their
infection course. Depending on when the agent gets the antiviral, they have a different reduction in viral
load. Therefore, even if we know that an agent will get an antiviral, we don't know their infectiousness
distribution in advance.

It is still straightforward to sample the appropriate infectiousness distribution without needing to change
the generation interval. We can use [rejection sampling](https://en.wikipedia.org/wiki/Rejection_sampling).
In this case, we still sample infection attempt times from the pre-antiviral infectiousness distribution, and
we accept those sampled times as actual infection attempts with probability $a(t) / g(t)$ where $a(t)$ is the
post-antiviral infectiousness distribution and $g(t)$ is the underlying pre-antiviral infectiousness
distribution. Note that $a(t)$ and $g(t)$ must be on an absolute scale in this example and not scaled to have
a unit integral. In the case where they are scaled, $g(t)$ can be rescaled to be $Mg(t)$ where $M = \max a(t)$.

This general idea of rejection sampling is useful for other applications. Consider the case
where each person has a different generation interval. For instance, imagine that we have estimates of
infectiousness over time from a within-host viral kinetics model (once again assuming infectiousness is a
function of viral load). We have a different value of the generation interval for each person because our
model produces a posterior distribution of infectiousness over time, and we assign each person a draw from
this posterior distribution. Instead of sampling from the person-specific generation interval, we might
want to sample from one overall generation interval that's shared among all agents. This makes it easier for
us to abstract the sampling part of our code away from the `evaluate_transmission` part. In this case, we
would only accept a fraction of the sampled times as actual transmission events, a fraction that varies over
the infection time and depends on the infected individual's specific generation interval parameters.

However, rejection sampling is inherently inefficient. It requires that we draw plans at a faster rate than events
actually happen, and then we need to evaluate at the time of the plan whether we mean for something to actually
happen. Nevertheless, there are ways to improve efficiency. So far, we have focused on the trivial case for making
the sampling rate faster than the event rate: sampling at the maximum event rate, so that $s(t) = 1$ and $M = \max g(t)$. If
$g_i(t) << \max g(t)$ for nearly all $t$ (i.e., a disease where infectiousness is highly concentrated around a given time
or a case where there is significant per-person variability in when an agent has their maximum infectiousness), this is
particularly inefficient because we are rejecting the majority of samples. Instead, we may try making our proposal
distribution better fit our underlying distribution. We may make $s(t)$ a similar linear approximation for $g(t)$.

However, this approximation is only possible if we sequentially sample infection attempts. If we sample all
infection attempts at the beginning of an agent's infection, we are locked into using a singular sampling rate. This
is by definition because we have sampled all attempts at once at the beginning of the infection course. Instead, we
want to update our sampling rate as we go throughout the infection course to best follow the generation interval
(whether we exactly follow the generation interval and don't use rejection sampling or still use a proposal distribution
is irrelevant). To update our sampling rate, we must sequentially sample infection attempts, calculating
the optimal sampling rate at each infection attempt. Thus, sequential sampling enables us to most generally model a
generation interval or infectiousness distribution that may change over the course of an agent's infection in a non
pre-defined way.

These examples underscore that sequentially sampling infection attempts rather than pre-scheduling enables
the modeler to consider changes in both the contact rate and the generation interval without needing to change
the transmission model. To this end, by sequentially sampling, we provide a modular transmission model that is
truly disentangled from the contact network and interventions present.
