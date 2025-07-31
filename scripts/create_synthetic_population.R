## =================================#
## Setup ---------------
## =================================#
library(tidyverse)
library(tigris)
library(tidycensus)
library(patchwork)
library(data.table)

set.seed(1234)

state_synth <- "WY"
year_synth <- 2023
population_size <- 1000000
school_per_pop_ratio <- 0.002
work_per_pop_ratio <- 0.1

pums_file <- sprintf("input/pums_%s_%d.csv", state_synth, year_synth)
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

if (!file.exists(pums_file)) {
  sample_pums <- get_pums(
    variables = c(person_variables, house_variables),
    state = state_synth,
    survey = "acs1",
    year = year_synth
  )
  write_csv(sample_pums, pums_file)
} else {
  sample_pums <- read_csv(pums_file)
}

household_pums <- sample_pums |>
  dplyr::select(SERIALNO, all_of(house_variables)) |>
  distinct()

## For now, we will use PUMA codes
## instead of census tracts
pumas_st <- pumas(state = state_synth, year = year_synth)
tracts_st <- tracts(state = state_synth, year = year_synth)

## =================================#
## Create schools -----------
## =================================#
puma_codes <- pumas_st$PUMACE20
n_schools <- ceiling(school_per_pop_ratio * population_size)

synth_school_df <- as_tibble(pumas_st) |>
  dplyr::sample_n(n_schools, replace = TRUE) |>
  dplyr::select(STATEFP20, PUMACE20, INTPTLAT20, INTPTLON20) |>
  mutate(
    census_tract_id = sprintf(
      "%02d%09d",
      as.numeric(STATEFP20), as.numeric(PUMACE20)
    ),
    school_id = sprintf(
      "%02d%09d%06d",
      as.numeric(STATEFP20), as.numeric(PUMACE20), row_number()
    )
  ) |>
  dplyr::mutate(
    lat = as.numeric(INTPTLAT20),
    lon = as.numeric(INTPTLON20)
  ) |>
  dplyr::select(school_id, lat, lon) |>
  mutate(enrolled = 0)

## =================================#
## Create workplaces -----------
## =================================#
n_workplaces <- ceiling(work_per_pop_ratio * population_size)

synth_workplace_df <- as_tibble(pumas_st) |>
  sample_n(n_workplaces, replace = TRUE) |>
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
      row_number()
    )
  ) |>
  dplyr::mutate(
    lat = as.numeric(INTPTLAT20),
    lon = as.numeric(INTPTLON20)
  ) |>
  dplyr::select(workplace_id, lat, lon) |>
  mutate(enrolled = 0)

## =================================#
## Create population ---------------
## =================================#
## WRK -> 1: Worked, 2: Not Worked, bb: unanswered
## SCH -> b: NA, 1: No, 2: Yes public, 3: Yes, private
start_time <- Sys.time()
synth_pop_list <- vector("list", population_size)
house_counter <- 1
list_counter <- 1
sampled_pop <- 0
household_sample_cap <- 2000
while (sampled_pop < population_size) {
  if (sampled_pop > 0) {
    households_remaining <- (population_size - sampled_pop) /
      (sampled_pop / house_counter)
    preferred_batch_size <- max(1, floor(0.95 * households_remaining))
    batch_size <- min(household_sample_cap, preferred_batch_size)
  } else {
    batch_size <- 30
  }
  house_sample <- household_pums |>
    sample_n(batch_size, weight = WGTP) |>
    mutate(house_number = house_counter:(house_counter + batch_size - 1)) |>
    left_join(sample_pums, by = (c("SERIALNO", "WGTP", "NP")))

  ## Assign workplaces
  work_id_list <- map_chr(house_sample$WRK, function(x) {
    if (x %in% c("1")) {
      sample(synth_workplace_df$workplace_id, size = 1)
    } else {
      NA
    }
  })

  school_id_list <- map_chr(house_sample$SCH, function(x) {
    if (x %in% c("2", "3")) {
      sample(synth_school_df$school_id, size = 1)
    } else {
      NA
    }
  })

  synth_pop_list[[list_counter]] <- house_sample |>
    mutate(
      school_id = school_id_list,
      workplace_id = work_id_list
    )
  sampled_pop <- sampled_pop + nrow(house_sample)
  house_counter <- house_counter + batch_size
  list_counter <- list_counter + 1
}

synth_pop_df <- data.table::rbindlist(synth_pop_list)
end_time <- Sys.time()
print(end_time - start_time)

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
  file.path(
    "input",
    sprintf(
      "synth_pop_region_%s_%d.csv",
      state_synth, population_size
    )
  ),
  na = ""
)
write_csv(
  people_df,
  file.path(
    "input",
    sprintf(
      "synth_pop_people_%s_%d.csv",
      state_synth, population_size
    )
  ),
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
