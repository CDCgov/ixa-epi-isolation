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
  group_by(t, infection_status) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = infection_status)) +
  xlab("Day") +
  ylab("Number of people")
print(x)
# Symptom curves
x <- person_property_report |>
  filter(!is.na(symptoms)) |>
  group_by(t, symptoms) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = symptoms)) +
  xlab("Day") +
  ylab("Number of people")
print(x)
# Symptom curves
x <- person_property_report |>
  filter(hospitalized == TRUE) |>
  group_by(t, hospitalized) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = hospitalized)) +
  xlab("Day") +
  ylab("Number of people")

print(x)
