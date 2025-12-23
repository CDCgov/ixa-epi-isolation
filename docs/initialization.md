# Simulation initializations

## Seeding initial infections
When the simulation is instantiated, all individuals are created in the susceptible compartment. Given the `initial_incidence` and `initial_recovered` parameters, a binomially distributed random number of incidence and recovered individuals are randomly sampled from the population. The transition of individuals to the infectious state, begin transmission within the remain susceptible population.

## Synthetic populations
A synthetic population is a structured `.csv` file which defines the population that will be simulated. Each row corresponds to an individual with the properties defined by the columns of the file: `age`, `homeId`, `schoolId`, `workplaceId`. `age` corresponds to the age of the individual. `homeId`, `schoolId`, and `workplaceId` corresponds to the home, school and workplace setting an individual belongs to. An individual must belong to a home setting, but does not need to belong to a school or workplace (this is indicated by an empty entry). An individual's community or census tract group is derived from the individual's `homeId`. For this model, the entrys for all setting IDs should be represented by 17 character structured numeric values. The first 11 characters of the string contain information about the state, county, and census tract following the FIPs format, and the remaining 6 characters define the group.

The synthetic population inherently defines the contact structure of the population, and the model is sensitive to this. We provide a script for generating example synthetic populations from census data. To create synthetic populations use `Rscript scripts/create_synthetic_population.R`. You can modifiy the parameters listed below to create additional synthetic populations. The parameterization below creates the recreates the file `input/people_test.csv` which is used in the base `input/input.json`.

```R
state_synth <- "WY"
year_synth <- 2023
population_size <- 1000
school_per_pop_ratio <- 0.002
work_per_pop_ratio <- 0.1
```
