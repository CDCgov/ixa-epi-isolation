# Modeling an individual's clinical disease manifestation

We use the automated property progression manager in defined in `mod clinical_status_manager` to
model an individual's symptoms as relevant for isolation guidance. We model two person properties
related to symptoms -- an individual's "isolation guidance" symptoms and their actual disease
severity/how badly they are feeling the symptoms.

An individual's "isolation guidance" symptoms entail what category they are in and whether they are
"improved".
```rust
#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum IsolationGuidanceSymptomValue {
    NoSymptoms,
    Category1,
    Category2,
    Category3,
    Category4,
    Improved
}

define_person_property_with_default!(
    IsolationGuidanceSymptom,
    Option<IsolationGuidanceSymptomValue>,
    None
);
```

Isolation guidance symptoms are separate from an individual's actual disease severity. This is how
"badly" an individual is feeling the effects of the disease. This is the kind of person property
that we would use to determine whether a person seeks treatment, medical attention, hospitalization,
etc. In a future iteration, we may add some concept of "hospital beds" as a resource by region and
use the value of this property along with the hospitalization rate and whether hospital beds are
available to determine whether a person can be hospitalized.

```rust
#[derive(PartialEq, Copy, Clone, Debug, Serialize)]
pub enum DiseaseSeverityValue {
    Mild,
    Moderate,
    Severe,
}

define_person_property_with_default!(
    DiseaseSeverity,
    Option<DiseaseSeverityValue>,
    None
);
```

In reality, the two person properties are correlated. Someone likely does not have severe symptoms
if they are in category four, for example. We will introduce a general way to model correlation
between person properties in the future. For now, we conceptualize the progressions of the two
person properties using different implementations of the `PropertyProgression` trait. For the case
of an individual's isolation guidance symptoms, we know the progression from our isolation guidance
modeling and have empirical values to feed into the model. Comparatively, f the case of an
individual's disease severity, we will use a simple Markov chain model to model the progression.

The case of an empirical symptom progression -- in other words, having a defined set of states and
times between those states -- that we implement it as a generic for any type `T`.

```rust
pub struct EmpiricalProgression<T: PartialEq + Copy> {
    states: Vec<T>,
    time_to_next: Vec<f64>,
}

impl<T: PartialEq + Copy> PropertyProgression for EmpiricalProgression<T> {
    type Value = T;
    fn next(&self, _context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        let mut iter = self.states.iter().enumerate();
        while let Some((_, status)) = iter.next() {
            if status == last {
                return iter
                    .next()
                    .map(|(i, next)| (*next, self.time_to_next[i - 1]));
            }
        }
        None
    }
}
```

Here, `states` provides a sequence of values for the person property in question, and `time_to_next`
tells us the time between successive values. `states` is always one element longer than
`time_to_next`. For example, let us consider the following empirical progression:

```rust
let p = EmpiricalProgression{ states: vec![Some(IsolationGuidanceSymptomValue::Presymptomatic),
                                           Some(IsolationGuidanceSymptomValue::Category1),
                                           Some(IsolationGuidanceSymptomValue::Improved)],
                              time_to_next: vec![2.0, 3.0]}
```

This means that the person spends 2.0 time units as presymptomatic and then transitions to category
1 where they spend 3.0 time units before transitioning to improved. We register this empirical
progression with the associated person property to ensure the progression is automated:

```rust
use clinical_status_manager::ContextPropertyProgressionExt;

context.register_property_progression(IsolationGuidanceSymptom, p);
```

We might register similar progressions for categories 2, 3, 4, and never symptomatic. In the future,
we will introduce a way of including correlations between natural history parameters, so that if
a person is assigned a rate function characteristic of category 1, they are also assigned an
associated symptom progression.

On the other hand, for our disease severity property, this empirical way of implementing its
progression no longer makes much sense. Let's consider the data that we may have to parameterize
this quantity: perhaps we have the fraction of individuals who make it from mild to moderate, and
the fraction of individuals who make it from moderate to severe. We may also have a distribution of
the amount of time an individual spends in each of these states. This setup does not lend itself to
explicit empirical progressions but rather a Markov chain:

```rust
pub struct DiseaseSeverityProgression {
    pub mild_to_moderate: f64,
    pub moderate_to_severe: f64,
    pub mild_time: f64,
    pub moderate_time: f64,
    pub severe_time: f64,
}

impl PropertyProgression for DiseaseSeverityProgression {
    type Value = Option<DiseaseSeverityValue>;
    fn next(&self, context: &Context, last: &Self::Value) -> Option<(Self::Value, f64)> {
        match last {
            Some(DiseaseSeverityValue::Mild) => {
                // With some probability, the person moves to moderate, otherwise they recover
                if context.sample_bool(SymptomRng, self.mild_to_moderate) {
                    Some((Some(DiseaseSeverityValue::Moderate), context.sample_distr(SymptomRng, Exp::new(1.0 / self.mild_time).unwrap())))
                } else {
                    Some((Some(DiseaseSeverityValue::Recovered), context.sample_distr(SymptomRng, Exp::new(1.0 / self.mild_time).unwrap())))
                }
            },
            Some(DiseaseSeverityValue::Moderate) => {
                // With some probability, the person moves to severe, otherwise they recover
                if context.sample_bool(SymptomRng, self.moderate_to_severe) {
                    Some((Some(DiseaseSeverityValue::Severe), context.sample_distr(SymptomRng, Exp::new(1.0 / self.moderate_time).unwrap())))
                } else {
                    Some((Some(DiseaseSeverityValue::Recovered), context.sample_distr(SymptomRng, Exp::new(1.0 / self.moderate_time).unwrap())))
                }
            },
            Some(DiseaseSeverityValue::Severe) => Some((Some(DiseaseSeverityValue::Recovered), context.sample_distr(SymptomRng, Exp::new(1.0 / self.severe_time).unwrap()))),
            Some(DiseaseSeverityValue::Recovered) | None => None,
        }
    }
}
```

We can use values from the rich COVID literature to parameterize this progression. This shows how
by only requiring that we implement the trait `PropertyProgression`, we can define a wide array of
different ways in which a person property may progress through states.
