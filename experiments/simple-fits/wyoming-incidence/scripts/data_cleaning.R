library(tidyverse)
dir <- file.path("experiments", "simple-fits", "wyoming-incidence", "input")
wyoming_data <- read_csv(file.path("input", "weekly_hospitalization_metrics_WY.csv"))

weekly_admissions <- wyoming_data |>
  mutate(date = as.Date(`Week Ending Date`)) |>
  rename(
    pediatric_admissions = `Weekly Total Pediatric COVID-19 Admissions`,
    adult_admissions = `Weekly Total Adult COVID-19 Admissions`,
    total_admissions = `Weekly Total COVID-19 Admissions`,
  ) |>
  select(date, pediatric_admissions, adult_admissions, total_admissions)

weekly_admissions |>
  write_csv(file.path(dir, "weekly_wyoming_covid_admissions.csv"))
weekly_admissions |>
  filter(date < as.Date("2021-03-25") & date > as.Date("2020-09-13")) |>
  mutate(t = 7 + date - min(date)) |>
  write_csv(file.path(dir, "target_data.csv"))
