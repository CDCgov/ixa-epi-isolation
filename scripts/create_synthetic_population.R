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
population_size <- 10000
school_per_pop_ratio <- 0.0005
work_per_pop_ratio <- 0.1

## data on number of public and private schools by state are available in
## National Center for Education Statistics Digest of Education Statistics
## tables at
## https://nces.ed.gov/programs/digest/d23/tables/dt23_216.70.asp
## https://nces.ed.gov/programs/digest/d23/tables/dt23_205.80.asp

pums_file <- sprintf("input/pums_%s_%d.csv", state_synth, year_synth)
## =================================#
## Get population ---------------
## =================================#

# pums_variables is a data set included in tidycensus
pums_vars <- pums_variables |>
  filter(year == year_synth, survey == "acs1") |>
  distinct(var_code, var_label, data_type, level)

person_variables <- c(
  "SPORDER", "SERIALNO", "PWGTP",
  "AGEP", "SEX", "PUMA",
  "SCH", "SCHG", "WRK"
)

house_variables <- c("WGTP", "NP")

pums_vars[pums_vars$var_code %in% c(person_variables, house_variables), ]

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

## Make crosswalk between PUMAs and census tracts
tract_puma_crosswalk_file <- "input/2020_Census_Tract_to_2020_PUMA.csv"
if (!file.exists(tract_puma_crosswalk_file)) {
  download.file(
    url = "https://www2.census.gov/geo/docs/maps-data/data/rel2020/2020_Census_Tract_to_2020_PUMA.txt", # nolint: line_length_linter.
    destfile = tract_puma_crosswalk_file
  )
}

tracts_by_puma <- read_csv(tract_puma_crosswalk_file) |>
  mutate(puma_id = paste0(STATEFP, PUMA5CE)) |>
  mutate(tract_id = paste0(STATEFP, COUNTYFP, TRACTCE)) |>
  group_by(puma_id) |>
  summarise(tracts = list(tract_id))

## Get spatial and other basic data on tracts for the state
tracts_st <- tracts(state = state_synth, year = year_synth)

## =================================#
## Create schools -----------
## =================================#
## Each schools is randomly assigned to a census tract
n_schools <- ceiling(school_per_pop_ratio * population_size)

synth_school_df <- as_tibble(tracts_st) |>
  dplyr::slice_sample(n = n_schools, replace = TRUE) |>
  dplyr::select(GEOID, INTPTLAT, INTPTLON) |>
  dplyr::mutate(school_id = paste0(GEOID, sprintf("%06d", row_number()))) |>
  dplyr::mutate(lat = as.numeric(INTPTLAT), lon = as.numeric(INTPTLON)) |>
  dplyr::select(school_id, lat, lon) |>
  mutate(enrolled = 0)

## =================================#
## Create workplaces -----------
## =================================#
## Each workplace is randomly assigned to a census tract
n_workplaces <- ceiling(work_per_pop_ratio * population_size)

synth_workplace_df <- as_tibble(tracts_st) |>
  dplyr::slice_sample(n = n_workplaces, replace = TRUE) |>
  dplyr::select(GEOID, INTPTLAT, INTPTLON) |>
  dplyr::mutate(workplace_id = paste0(GEOID, sprintf("%06d", row_number()))) |>
  dplyr::mutate(lat = as.numeric(INTPTLAT), lon = as.numeric(INTPTLON)) |>
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

  ## Assign workplaces (randomly within state)
  work_id_list <- map_chr(house_sample$WRK, function(x) {
    if (x %in% c("1")) {
      sample(synth_workplace_df$workplace_id, size = 1)
    } else {
      NA
    }
  })

  ## Assign schools (randomly within state)
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
## sample census tract for each house_number based on PUMA

house_puma_df <- synth_pop_df |>
  dplyr::select(house_number, STATE, PUMA) |>
  distinct() |>
  mutate(puma_id = paste0(STATE, PUMA)) |>
  left_join(tracts_by_puma, by = c("puma_id")) |>
  mutate(tract_id = map_chr(tracts, sample, size = 1)) |>
  select(-tracts)

synth_pop_region_df <- synth_pop_df |>
  left_join(
    house_puma_df |> select(-puma_id),
    by = c("house_number", "STATE", "PUMA")
  ) |>
  left_join(
    tracts_st |>
      select(COUNTYFP, TRACTCE, GEOID, INTPTLAT, INTPTLON),
    by = c("tract_id" = "GEOID")
  ) |>
  mutate(home_id = paste0(tract_id, sprintf("%06d", house_number)))


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
  dplyr::mutate(lat = as.numeric(INTPTLAT), lon = as.numeric(INTPTLON)) |>
  dplyr::select(tract_id, lat, lon) |>
  rename(censusTractId = tract_id)

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

g2 <- ggplot(tracts_st) +
  geom_sf() +
  theme_void()
