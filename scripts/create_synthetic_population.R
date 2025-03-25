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
population_size <- 4000
school_per_pop_ratio <- 0.002
work_per_pop_ratio <- 0.1
## =================================#
## Get population ---------------
## =================================#
pums_vars <- pums_variables |>
  filter(year == 2018, survey == "acs1") |>
  distinct(var_code, var_label, data_type, level)

person_variables <- c(
  "SPORDER", "SERIALNO", "PWGTP",
  "AGEP", "SEX", "PUMA", "REGION",
  "SCH", "SCHG", "WRK"
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

## For now, we will use PUMA codes
## instead of census tracts
pumas_st <- pumas(state = state_synth)
tracts_st <- tracts(state = state_synth)

## =================================#
## Create schools -----------
## =================================#
puma_codes <- pumas_st$PUMACE20
synth_school_df <- tibble()
n_schools <- ceiling(school_per_pop_ratio * population_size)
for (n in seq_len(n_schools)) {
  school_tmp <- as_tibble(pumas_st)
  school_tmp <- school_tmp |>
    dplyr::sample_n(1) |>
    dplyr::select(STATEFP20, PUMACE20, INTPTLAT20, INTPTLON20) |>
    mutate(
      census_tract_id = sprintf(
        "%02d%09d",
        as.numeric(STATEFP20), as.numeric(PUMACE20)
      ),
      school_id = sprintf(
        "%02d%09d%06d",
        as.numeric(STATEFP20), as.numeric(PUMACE20), n
      )
    ) |>
    dplyr::rename(
      lat = INTPTLAT20,
      lon = INTPTLON20
    ) |>
    dplyr::select(school_id, lat, lon) |>
    mutate(enrolled = 0)
  synth_school_df <- bind_rows(synth_school_df, school_tmp)
}

## =================================#
## Create workplaces -----------
## =================================#
synth_workplace_df <- tibble()
n_workplaces <- ceiling(work_per_pop_ratio * population_size)
for (n in seq_len(n_workplaces)) {
  work_tmp <- as_tibble(pumas_st)
  work_tmp <- work_tmp |>
    dplyr::sample_n(1) |>
    dplyr::select(STATEFP20, PUMACE20, INTPTLAT20, INTPTLON20) |>
    mutate(
      census_tract_id = sprintf(
        "%02d%09d",
        as.numeric(STATEFP20),
        as.numeric(PUMACE20)
      ),
      workplace_id = sprintf(
        "%02d%09d%06d",
        as.numeric(STATEFP20),
        as.numeric(PUMACE20),
        n
      )
    ) |>
    dplyr::rename(
      lat = INTPTLAT20,
      lon = INTPTLON20
    ) |>
    dplyr::select(workplace_id, lat, lon) |>
    mutate(enrolled = 0)
  synth_workplace_df <- bind_rows(synth_workplace_df, work_tmp)
}

## =================================#
## Create population ---------------
## =================================#
## WRK -> 1: Worked, 2: Not Worked, bb: unanswered
## SCH -> b: NA, 1: No, 2: Yes public, 3: Yes, private
synth_pop_df <- tibble()
house_counter <- 0
while (nrow(synth_pop_df) < population_size) {
  house_counter <- house_counter + 1
  house_sample <- household_pums |>
    sample_n(1, weight = WGTP) |>
    left_join(sample_pums, by = (c("SERIALNO", "WGTP", "NP"))) |>
    mutate(house_number = house_counter)
  ## Assign schools

  ## Assign workplaces
  work_id_list <- map_chr(house_sample$WRK, function(x) {
    if (x %in% c("1")) {
      return(sample(synth_workplace_df$workplace_id, size = 1))
    } else {
      return(NA)
    }
  })

  school_id_list <- map_chr(house_sample$SCH, function(x) {
    if (x %in% c("2", "3")) {
      return(sample(synth_school_df$school_id, size = 1))
    } else {
      return(NA)
    }
  })
  synth_pop_df <- bind_rows(
    synth_pop_df,
    house_sample |>
      mutate(
        school_id = school_id_list,
        workplace_id = work_id_list
      )
  )
}

## =================================#
## Recode and math GEO -----------
## =================================#
synth_pop_region_df <- synth_pop_df |>
  left_join(
    pumas_st |>
      dplyr::select(STATEFP20, PUMACE20, INTPTLAT20, INTPTLON20),
    by = c("PUMA" = "PUMACE20")
  ) |>
  dplyr::select(-geometry) |>
  mutate(
    census_tract_id = sprintf(
      "%02d%09d",
      as.numeric(STATE), as.numeric(PUMA)
    ),
    home_id = sprintf(
      "%02d%09d%06d",
      as.numeric(STATE), as.numeric(PUMA), house_number
    )
  )


## split pop in persons and regions
## People columns: age, homeId
people_df <- synth_pop_region_df |>
  dplyr::select(AGEP, home_id, school_id, workplace_id) |>
  dplyr::rename(
    age = AGEP,
    homeId = home_id,
    schoolId = school_id,
    workplaceId = workplace_id
  )


## Region columns: region_id, lat, lon
region_df <- synth_pop_region_df |>
  dplyr::mutate(lat = as.numeric(INTPTLAT20), lon = as.numeric(INTPTLON20)) |>
  dplyr::select(census_tract_id, lat, lon) |>
  rename(censusTractId = census_tract_id)

## =================================#
## Write outputs -----------
## =================================#
write_csv(
  region_df,
  file.path("input", sprintf("synth_pop_region_%s.csv", state_synth)),
  na = ""
)
write_csv(
  people_df,
  file.path("input", sprintf("synth_pop_people_%s.csv", state_synth)),
  na = ""
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
