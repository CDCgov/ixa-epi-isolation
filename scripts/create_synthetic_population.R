##=================================#
## Setup ---------------
##=================================#
library(tidyverse)
library(tigris)
library(tidycensus)

state_synth <- "WY"
year_synth <- 2023
population_size <- 100

##=================================#
## Get population ---------------
##=================================#
pums_vars <- pums_variables |>
  filter(year == 2018, survey == "acs1") |>
  distinct(var_code, var_label, data_type, level)

person_variables <- c("SPORDER","SERIALNO", "PWGTP", "AGEP", "SEX", "PUMA", "REGION")
house_variables <- c("WGTP", "NP")

sample_pums <- get_pums(
  variables = c(person_variables, house_variables),
  state = state_synth,
  survey = "acs1",
  year = year_synth)

household_pums <- sample_pums |>
  dplyr::select(SERIALNO, all_of(house_variables)) |>
  distinct()
