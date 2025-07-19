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
  "person_property_count.csv"
))

## ===============================#
## Plots
## ===============================#

# Infectious curves
x <- person_property_report |>
  group_by(t, InfectionStatus) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = InfectionStatus)) +
  xlab("Day") +
  ylab("Number of people")
print(x)
# Symptom curves
x <- person_property_report |>
  group_by(t, Symptoms) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = Symptoms)) +
  xlab("Day") +
  ylab("Number of people")
print(x)
# Symptom curves
x <- person_property_report |>
  group_by(t, Hospitalized) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = Hospitalized)) +
  xlab("Day") +
  ylab("Number of people")

print(x)
