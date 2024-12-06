# Modeling the progression of symptoms from a SARS-CoV-2 infection

We need to model the clinical symptoms resulting from infection so that we can compare and calibrate
our model to observed burden. We propose a simple model of COVID-19 related symptoms and explain
the data we anticipate using to parameterize this model as well as any underlying assumptions.

## Health flow after infection
1. All people start as healthy. While we may add comorbidities in the future, comorbidities do not impact
an individual's underlying health status from COVID-19, which is what we are modeling here.
2. A proportion of individuals (`asymptomatic_probability`) never develop symptoms. To ensure they
are still tracked, they are labeled as `HealthStatus::Asymptomatic` when they become infectious and
are returned to `HealthStatus::Healthy` when they recover.
3. The remainder develop at least mild symptoms. Mild symptoms develop at some time after the agent
first becomes infectious based on the incubation period. The period before mild symptoms develop
is referred to as the `HealthStatus::Presymptomatic` phase. We use the COVID-19 incubation period
from Park et al., (2023).
4. Individuals who develop mild symptoms may develop severe symptoms. Those who do not develop severe
symptoms have symptom improvement at some time afterwards symptom onset. Symptom improvement times are
read from our isolation guidance modeling posteriors.
5. Individuals who develop severe symptoms (`hospitalization_probability`) develop symptoms some time
after symptom onset (`time_to_hospitalization`), and then their symptoms stay severe for some amount
of time (`hospitalization_period`).

## Implementation notes
- Calculating the incubation period times requires a custom PDF -- the user provides the parameters to
this PDF. We calculate this "derived" parameter in `mod parameters` and then store them in a data container
that can be queried in `mod infection_course_manager`.
- Symptom onset times are unrelated to an individual's viral load or symptom improvement time. Onset times
are taken from Park's distributions while improvement times are taken from our isolation guidance work and
are associated with that person also having a particular viral load/generation interval that we model in
the transmission model.
- We are potentially mixing data sources from different times in the COVID-19 pandemic. The majority of
participants in our isolation guidance work were infected during the Delta variant whereas the other parameter
estimates taken are mainly from the Omicron period.
