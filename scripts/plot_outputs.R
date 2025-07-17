## ===============================#
## Setup ---------
## ===============================#

library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

## ===============================#
## Read person properties reports
## ===============================#

person_property_report <- readr::read_csv(file.path(
  "output",
  "person_property_report.csv"
))

person_property_count <- readr::read_csv(file.path(
  "output",
  "person_property_count.csv"
))

person_count_dif <- person_property_report |>
  group_by(t, InfectionStatus) |>
  summarize(count = sum(count), .groups = "drop") |>
  left_join(
    person_property_count |>
      group_by(t, InfectionStatus) |>
      summarize(count = sum(count), .groups = "drop")
  , by = c("t", "InfectionStatus")) |>
  mutate(difference = count.y - count.x)
## ===============================#
## Plots
## ===============================#

# Infectious curves
person_property_report |>
  group_by(t, InfectionStatus) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = InfectionStatus)) +
  xlab("Day") +
  ylab("Number of people")

# Symptom curves
person_property_report |>
  group_by(t, Symptoms) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = Symptoms)) +
  xlab("Day") +
  ylab("Number of people")
