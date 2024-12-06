## =================================#
## Setup ---------------
## =================================#
library(tidyverse)
library(tigris)
library(tidycensus)
library(patchwork)

set.seed(1234)

state_synth <- "WY"
year_synth <- 2023
population_size <- 40

## =================================#
## Get population ---------------
## =================================#
pums_vars <- pums_variables |>
  filter(year == 2018, survey == "acs1") |>
  distinct(var_code, var_label, data_type, level)

person_variables <- c(
  "SPORDER", "SERIALNO", "PWGTP",
  "AGEP", "SEX", "PUMA", "REGION"
)
house_variables <- c("WGTP", "NP")

sample_pums <- get_pums(
  variables = c(person_variables, house_variables),
  state = state_synth,
  survey = "acs1",
  year = year_synth
)

household_pums <- sample_pums |>
  dplyr::select(SERIALNO, all_of(house_variables)) |>
  distinct()

## =================================#
## Create population ---------------
## =================================#
synth_pop_df <- tibble()
house_counter <- 0
while (nrow(synth_pop_df) < population_size) {
  house_counter <- house_counter + 1
  house_sample <- household_pums |>
    sample_n(1, weight = WGTP) |>
    left_join(sample_pums, by = (c("SERIALNO", "WGTP", "NP"))) |>
    mutate(house_number = house_counter)
  synth_pop_df <- bind_rows(synth_pop_df, house_sample)
}

## =================================#
## Recode and math GEO -----------
## =================================#
## For now, we will use PUMA codes
## instead of census tracts
pumas_st <- pumas(state = state_synth)
tracts_st <- tracts(state = state_synth)

synth_pop_region_df <- synth_pop_df |>
  left_join(
    pumas_st |>
      dplyr::select(STATEFP20, PUMACE20, INTPTLAT20, INTPTLON20),
    by = c("PUMA" = "PUMACE20")
  ) |>
  dplyr::select(-geometry) |>
  mutate(
    censusTractId = sprintf("%02d%09d", as.numeric(STATE), as.numeric(PUMA)),
    homeId = sprintf(
      "%02d%09d%06d",
      as.numeric(STATE), as.numeric(PUMA), house_number
    )
  )


## split pop in persons and regions
## People columns: age, homeId
people_df <- synth_pop_region_df |>
  dplyr::select(AGEP, homeId) |>
  dplyr::rename(age = AGEP)


## Region columns: region_id, lat, lon
region_df <- synth_pop_region_df |>
  dplyr::mutate(lat = as.numeric(INTPTLAT20), lon = as.numeric(INTPTLON20)) |>
  dplyr::select(censusTractId, lat, lon)

write_csv(
  region_df,
  file.path("input", sprintf("synth_pop_region_%s.csv", state_synth))
)
write_csv(
  people_df,
  file.path("input", sprintf("synth_pop_people_%s.csv", state_synth))
)
## =================================#
## Quick plot -----------
## =================================#
g1 <- ggplot(region_df) +
  aes(x = lon, y = lat) +
  geom_point()

g2 <- ggplot(pumas_st) +
  geom_sf() +
  theme_void()
g1 + g2
