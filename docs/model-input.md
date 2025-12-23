# Model Input
The model's behavior is defined by several input parameters. They are defined and possible values are listed below.

#### `seed`
The seed of the model's random number generator

#### `max_time`
The time the simulation terminates. Any plans scheduled later than `max_time` will not occur. If all plans are completed before `max_time` occurs, the simulation will terminate.

#### `synth_population_file`
Path to the synthetic population file. This file informs the underlying population characteristics and contact structure. See [simulation initialization documentation](initialization.md) for more detail.

#### `initial_incidence`
The proportion of people that begin the simulation in the infectious state. See [simulation initialization documentation](initialization.md) for more detail.

#### `initial_recovered`
The proportion of people that begin the simulation in the recovered state. See [simulation initialization documentation](initialization.md) for more detail.

#### `infectious_rate_fn`
A library of infection rates assigned to individual when they become infectious. Possible values are `EmpiricalFromFile`, which requires a file of rates and a numeric scale value, and `Constant`, which requires a rate and duration See [transmission documentation](transmission.md) for more detail.

#### `proportion_asymptomatic`
The proportion of infected individuals who do not develop symptoms

#### `relative_infectiousness_asymptomatics`
Asymptomatic people are modeled as less infectious than symptomatic people. This parameter is the multiplier applied to modify an individuals transmission.

#### `symptom_progression_library`
This optional parameter is type `ProgressionLibraryType`.
/// A library of symptom progressions
pub symptom_progression_library: Option<ProgressionLibraryType>,

#### `hospitalization_parameters`
This parameter struct has three components:
- `mean_duration_of_hospitalization` mean of the exponential distribution which generates an individual's hospital durations
- `mean_delay_to_hospitalization` mean of the exponential distribution which generates an individual's  delay from symptom onset to hospital
- `age_groups` dictionary defining age buckets and the corresponding probability of hospitalization given moderate symptoms. The age value key defines the lower bound of the age bucket. The noninclusive upper bound of the age bucket is next age key value.

See the [hospitalization documentation](hospitalization.md) for more details

#### `setting_properties`

This parameter struct defines a map of `CoreSettingsTypes` and `SettingProperties`. There must be alignment between the settings enumerated in this struct and the settings that are declared in the model instantiation. With each setting type, the following attributes must be defined in the `SettingProperties`:
- `alpha` parameter informing density dependent transmission in the setting. Density dependent transmission is a multiplier on an individual's infectiousness that takes the form $(N-1)^\alpha$ where $N$ is the number of individuals in the setting.
- `itinerary_specification` parameter used to define the proportion of time an individual spend in the setting

See the [settings documentation](settings.md) for more details.

#### `guidance_policy`
This optional parameter takes a `Policies` type. The two types of policies each with specific attribute parameter listed below
- `UpdateIsolationGuidance`
    - `policy_adherence` the proportion of individual that follow the policy when symptomatic
    - `post_isolation_duration` the duration an individual follows post-isolation precautions
    - `isolation_delay_period` mean of the exponential distribution which generates an individual's  delay from symptom onset to beginning isolation
- `PreviousIsolationGuidance`
    - `overall_policy_duration` the minimum duration from symptom onset of the policy if the individuals has a positive test results
    - `mild_symptom_isolation_duration` the minimum required duration of isolation for individuals with mild symptoms
    - `moderate_symptom_isolation_duration` the minimum required duration of isolation for individuals with moderate symptoms
    - `delay_to_retest` the delay between an individual's first negative test and the subsequent retest
    - `policy_adherence` probability an individual follows the isolation guidance policy conditional on symptom duration > isolation_delay_period
    - `isolation_delay_period` delay from symptom onset to when an individual starts following the isolation guidance policy
    `test_sensitivity` sensitivity of the test used to determine if an individual is infected

See the [intervention policy documentation](intervention-policies.md) for more details.

#### `facemask_parameter`
This optional parameter struct has a single parameter `facemask_efficacy` which is a multiplier on an individuals infectiousness associated with using a facemask.

See the [intervention policy documentation](intervention-policies.md) for more details.

### `prevalence_report`
This is defined by a `ReportParams` struct and creates the report indicating the number of individuals in infectious, symptomatic, and hospitalized compartments each day of the simulation.

See the [reports documentation](reports.md) for more details.

### `incidence_report`
This is defined by a `ReportParams` struct and creates the report indicating the number of incident transitions of the infectious, symptomatic, and hospitalized progressions each day of the simulation.

See the [reports documentation](reports.md) for more details.

### `transmission_report`
This is defined by a `ReportParams` struct and creates the report tracking the individuals and location of each accepted infection attempt.

See the [reports documentation](reports.md) for more details.
