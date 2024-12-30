# Time-varying infectiousness

## Motivation

Many epidemiological studies have shown that infectiousness varies widely over the course
of the individual's infection. We detail the process for sampling an arbitrary number of
infection attempts appropriately from an arbitrary infectiousness curve in an agent-based model (ABM).
We describe an approach that provides great generalizability, enabling sampling when both the
underlying contact rate and infectiousness distribution change over the course of an infection.
We motivate our discussion with a simple example of an infectious person who takes an
antiviral partially through their infection and is hospitalized.

## Assumptions

1. We describe the approach for sampling from an arbitrary infectiousness distribution for the uniform
distribution from zero to one -- $\mathcal{U}(0, 1)$.
    - Using [inverse transform sampling](https://en.wikipedia.org/wiki/inverse_transform_sampling), a
uniformly-distributed random variable can be transformed to any distribution by passing samples through
the inverse cumulative distribution function (CDF).
    - This requires that the infectiousness over time distribution has an inverse CDF, or we are
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

However, we cannot just independently draw the time of a next infection attempt from the corresponding Beta
distribution. Consider that we draw the first infection attempt time from $\beta(1, 5)$ and happen to get a large value.
We need to take that into account when drawing the second infection attempt time. The second infection
attempt time must be greater than the first, so we cannot just draw from $\beta(2, 6)$ but rather $\beta(2, 6) | x_{(1)}$.

We can set $x_{(1)}$ as the minimum of a new uniform
distribution, $\mathcal{U}(x_{(1)}, 1)$, from which we need to draw an infection attempt. Because this is a new distribution,
we want the first of $n - 1$ infection attempt times on this distribution. We can do that by drawing the
minimum of $n - 1$ infection attempts from $\mathcal{U}(0, 1)$, and scaling that value to be on $(x_{(1)}, 1)$.
In other words, we shrink the available uniform distribution with each infection attempt and ask what
would be the _next_ infection attempt on that distribution. Concretely, if $x_{(1)}$ is
on $(0, 1)$, and $x_{(2)}$ is also on $(0, 1)$, we use the following equation:

$$x_{(2)} := x_{(1)} + x_{(2)} * (1 - x_{(1)})$$

For further detail, the Wikipedia [page on order statistics](https://en.wikipedia.org/wiki/Order_statistic) is a
great reference as are various notes on the subject from
[advanced statistics courses](https://colorado.edu/amath/sites/default/files/attached-files/order_stats.pdf).

## Workflow

1. Draw a number of secondary infection attempts for the agent, $C_i$. This can be equal to $R_i$ if the sampling times
are from the generation interval exactly, or $C_i$ can be greater than $R_i$ if sampling from a
[proposal distribution](#rejection-sampling-from-an-arbitrary-generation-interval) and rejecting some attempts to leave
a total of $R_i$ infection attempts.
2. Draw the time for the first of $n$ remaining infection attempts of $\mathcal{U}(0, 1)$ by taking a random value from
$\beta(1, m)$. $m = C_i - $ (the number of infection attempts that have occurred).
3. Scale the value on $\mathcal{U}(0, 1)$ to be on $\mathcal{U}(x_{(i)}, 1)$ where $x_{(i)}$ is the previous draw.
4. Convert the uniform value to generation interval space by passing it through the inverse CDF of the generation interval,
and schedule the next infection attempt at the specified time. Wait until that time has occurred in the simulation before
proceeding.

    The result of passing the uniform time through the GI's inverse CDF is the time _since_ the agent first become
infectious at which the given $n$th infection attempt occurs.

5. Repeat from step two until $m = 0$.

## Why do we need order statistics?

We provide a detailed justification of why scheduling the next infection attempt after the last (i.e., sequentially
drawing infection attempt times from the GI) is the better approach compared to taking all the draws
from the GI at the beginning of an individual's infectiousness period and at once scheduling the infection
attempts based on those times.

### Changes in the number of infection attempts in the middle of an agent's infectious course

Imagine an agent dies while they are still infectious. They can no longer infect others. (Or, if the
disease in question is Ebola, there are well-defined processes by which they may still be infectious and
those processes should not be lumped with the infection generation interval.)

To ensure a dead person does not infect others, we would like to remove the plans where they infect others.
If we had pre-scheduled all infections, we would have had to store the plan IDs for all the infections in
`HashMap<PersonId, Vec<PlanId>>`. Then, each time we had executed one of these plans, we would have to
remove it from the vector, so that the entry for each `PersonId` tells us the plans we have _left_ for a given
person. Then, when the agent dies, we could iterate through the remaining plans and cancel them.

Clearly, this is cumbersome, requiring us to store a `HashMap` and iterate
through a vector every time an infection attempt occurs. Alternatively, if we scheduled infection events
sequentially, we would only have to store the next infection attempt time. We could instead use a
`HashMap<PersonId, PlanId>`, removing the need to iterate through a vector and instead enabling us to
directly use the `.insert` method on the `HashMap`.

More generally, this idea applies to any case where we need to change the number of infection attempts
partway through an infection course. Sequentially scheduling the attempts makes it possible to accomodate
changes to the number of infection attempts that may happen in the middle of an infection course.

Why not just check whether the agent is alive or not at the beginning of the infection attempt? If they are
not alive, skip the infection attempt. There are two reasons this is not the desired solution. First,
it is cleaner to handle removing an agent at the point at which they become no longer relevant rather than
continuously checking on whether they are alive. Secondly, there are other cases when the number
of infection attempts may change partially through an infection, such as being hospitalized and having
a lower contact rate. Sequentially scheduling infection attempts enables arbitrary changes to the
effective contact rate while an agent is infectious in the simplest way.

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
distribution.

However, rejection sampling is inherently inefficient. It requires that we draw plans at a faster rate than events
actually happen, and then we need to evaluate at the time of the plan whether we mean for something to actually
happen. Nevertheless, there are ways to improve efficiency. The trivial case of drawing plans at a faster rate
than events actually happen is to use the maximum event rate as the sampling rate for all $t$. But, if the
actual event rate is much smaller than the maximum for nearly all $t$, (i.e., a disease where infectiousness is highly
concentrated around a specific time or $a(t) << g(t)$), this is particularly inefficient because we are rejecting the majority of samples. Instead, we may try making our proposal distribution better fit our underlying distribution by making it, say, a linear approximation of $a(t)$.

This approach is only possible if we sequentially sample infection attempts. If we sample all
infection attempts at the beginning of an agent's infection, we are locked into using a singular sampling rate. Thus,
sequential sampling enables us to most generally model a generation interval or infectiousness distribution
that may change over the course of an agent's infection in a non pre-defined way.

These examples underscore that sequentially sampling infection attempts rather than pre-scheduling enables
the modeler to consider changes in both the contact rate and the generation interval without needing to change
the transmission model.
