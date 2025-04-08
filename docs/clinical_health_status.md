# Modeling an individual's clinical disease manifestation

We use the automated property progression manager in defined in `mod property_progression_manager`
to model an individual's symptoms as relevant for isolation guidance. This manager automates the
progression of an individual through a set of person properties: this manager is helpful for
modeling symptoms because an individual moves through various symptom states throughout the course
of their infection. For our particular example of isolation guidance, we model an individual's
"isolation guidance".

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
Since

We model the progression of an individual through isolation guidance symptom values using an
implementation of the `PropertyProgression` trait (the trait that must be implemented to register a
progression with the property progression manager). We feed in the symptom improvement times
generated from the Stan model (each associated with a particular infectiousness curve) and imputed
incubation periods based on the Stan-estimated proliferation time. Since these are paired values
that do not necessarily follow a well-known probability distribution, we think about modeling the
progression through symptoms as based of empirical values for both the states and the times.

This idea of an empirical progression is generic enough that we implement it for any type `T` that
can be held as a person property. We provide this struct in the ixa-epi crate because it is so
useful for multiple applications, but a user can always implement their own version of the
`PropertyProgression` trait on any struct that makes sense based on however it is best to store
information about their particular property and use case.

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
                              time_to_next: vec![2.0, 3.0] };
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

The abstract nature of the property progression manager is that its use case is not limited to
clinical symptoms but can be expanded to even include someone's state when following an individual-
level policy or any sort of progression where an individual moves through states. Likewise, the
manager only requires an implementation of the `PropertyProgression` trait, so a user can figure out
what struct works best for their particular use case (for instance, if it is not the
`EmpiricalProgression` struct) and implement accordingly.
