# Reports
There are three types of reports generated in ixa-epi-isolation: incidence, prevalence, and transmission reports. All reports are defined in model input using the `ReportsParam` struct which contains the following attributes:
- `write` boolean value which if false indicates that the report will not be generate.
- `filename` optional string value for the filename of the report.
- `period` optional float value indicating the number of simulation days that occur between reports being recorded.

## Incidence Report

This report records the number of incident person property updates that occur over the simulation horizon. The person properties for which updates are tracked are `InfectionStatus`, `Symptoms`, and `Hospitalized`. Each tracked update is also aggregated by age. This report module maintains an internal map of with keys that are combinations of tracked person properties' values and ages and values that are counts of the number of times that record how many transitions have occurred over the current `period`. The internal map is updated using event subscriptions. After the data is recorded at the end of the period the values in the internal map are reset to zero.

The report structure has four columns:
- `t_upper` the time at which counts are recorded. It represents the last value for which counts of person properties are included.
- `age` report is stratified by age
- `event` the person property value of interested
- `count` the number of instances that an individual with `Age = age` updated a person property to have value equivalent to `event` in the period defined by `t_upper`

## Prevalence Report

This report records the number of people in the simulation with that have a combination of certain person property values over the simulation horizon. The person properties that are tracked are `InfectionStatus`, `Symptoms`, and `Hospitalized`. Each tracked person property is also aggregated by age. This report module maintains an internal map of with keys that are combinations of tracked person properties' values and ages and values that are counts of the number of people that currently have the given person property value. At the end of each `period` the results are recorded. The internal map is updated using event subscriptions.

The report structure has 6 columns:
- `t` the time at which counts are recorded.
- `age` Age person property value
- `symptoms` Symptoms person property value
- `infection_status` Infection status value
- `hospitalized` Hospitalized person property value
- `count` the number of individuals that have `Age = age`, `Symptoms = symtpoms`, `InfectionStatus = infection_status`, and `Hospitalized = hospitalized` at time `t`.

## Transmission Report

This report records each successful infection attempt. Event subscriptions are used to identify infection attempts, and information about each infection attempt is recorded as listed in the file structure below.

The report structure has 6 columns:
- `time` the time at which the infection attempt occurs
- `target_id` the `PersonId` who is subject to the infection attempt
- `infected_by` the `PersonId` who is attempting to infect another individual
- `infection_setting_type` the category of setting where the infection attempt occurred
- `infection_setting_id` the id of the setting where the infection attempt occurred
