library(tidyverse)
dir <- file.path("experiments", "simple-fits", "wyoming-constant-rate", "input")
wyoming_data <- read_csv(file.path(dir, "weekly_hospitalization_metrics_WY.csv"))

weekly_wyoming_covid_admissions <- wyoming_data |>
  mutate(date = as.Date(`Week Ending Date`)) |>
  rename(
    pediatric_admissions = `Weekly Total Pediatric COVID-19 Admissions`,
    adult_admissions = `Weekly Total Adult COVID-19 Admissions`,
    total_admissions = `Weekly Total COVID-19 Admissions`,
  ) |>
  select(date, pediatric_admissions, adult_admissions, total_admissions)

weekly_wyoming_covid_admissions |>
  write_csv(file.path(dir, "weekly_wyoming_covid_admissions.csv"))
weekly_wyoming_covid_admissions |>
  filter(date < as.Date("2021-04-15") & date > as.Date("2020-09-13")) |>
  mutate(t = 6 + date - min(date)) |>
  write_csv(file.path(dir, "target_data.csv"))
