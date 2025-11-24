library(tidyverse)
download.file(
  url = "https://data.cdc.gov/resource/aemt-mg7g.csv?jurisdiction=WY&$select=jurisdiction,week_end_date,total_admissions_pediatric_covid_confirmed,total_admissions_adult_covid_confirmed,total_admissions_all_covid_confirmed", # nolint: line_length_linter.
  destfile = file.path("input", "weekly_hospitalization_metrics_WY.csv")
)
dir <- file.path("experiments", "simple-fits", "wyoming-incidence", "input")
wyoming_data <- read_csv(
  file.path("input", "weekly_hospitalization_metrics_WY.csv")
)

weekly_admissions <- wyoming_data |>
  mutate(date = as.Date(week_end_date)) |>
  rename(
    pediatric_admissions = total_admissions_pediatric_covid_confirmed,
    adult_admissions = total_admissions_adult_covid_confirmed,
    total_admissions = total_admissions_all_covid_confirmed,
  ) |>
  select(date, pediatric_admissions, adult_admissions, total_admissions)

weekly_admissions |>
  write_csv(file.path(dir, "weekly_wyoming_covid_admissions.csv"))
weekly_admissions |>
  filter(date < as.Date("2021-03-25") & date > as.Date("2020-09-13")) |>
  mutate(t = 7 + date - min(date)) |>
  write_csv(file.path(dir, "target_data.csv"))
