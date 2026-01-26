# Simulation Initialization

## Seeding Initial Conditions
When the simulation is instantiated, all individuals are created in the susceptible compartment. The model initializes infectious individuals partway through their infection, symptom, and hospitalization progressions at time 0. This is implemented using the negative simulation time feature of `ixa`. At time 0 recovered individuals are also seeded and transmission is enabled. Both the infectious and recovered seeding processes sample from the susceptible population and use a binomial distribution with success probability of `initial_incidence` and `initial_recovered`, respectively.

## Synthetic populations
A synthetic population is a structured `.csv` file which defines the population that will be simulated. Each row corresponds to an individual with the properties defined by the columns of the file: `age`, `homeId`, `schoolId`, `workplaceId`. `age` corresponds to the age of the individual. `homeId`, `schoolId`, and `workplaceId` correspond to the home, school and workplace setting an individual belongs to. An individual must belong to a home setting, but does not need to belong to a school or workplace (this is indicated by an empty entry). An individual's community or census tract is derived from the individual's `homeId`. The implementation in `population_loader.rs` adds all people to the model, assigns the age person property and setting itinerary to each individual. For this model, the entries for all setting IDs should be represented by 17 character structured numeric values. The first 11 characters of the string contain information about the state, county, and census tract following the FIPS format, and the remaining 6 characters define the group.

`scripts/create_synthetic_population.R` is a script for generating example synthetic populations from census data. You can modify the parameters listed below to create additional synthetic populations. The parameterization below recreates the file `input/people_test.csv` which is used in the base `input/input.json`.

```R
state_synth <- "WY"
year_synth <- 2023
population_size <- 1000
school_per_pop_ratio <- 0.002
work_per_pop_ratio <- 0.1
```
