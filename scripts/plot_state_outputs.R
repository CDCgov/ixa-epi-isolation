## ===============================#
## Setup ---------
## ===============================#

library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

## ===============================#
## Read reports--------
## ===============================#

person_property_report <- readr::read_csv(file.path(
  "output",
  "10000",
  "person_property_count.csv"
))

## ===============================#
## Plots ------------
## ===============================#
pop_data <- person_property_report |>
  group_by(t, InfectionStatus) |>
  summarise(count = sum(count), .groups = "drop")
pop_size <- sum(pop_data[pop_data$t == 0, "count"])
max_inf <- sum(pop_data[pop_data$t == max(pop_data$t) & pop_data$InfectionStatus == "Recovered","count"])

# Infectious curves
person_property_report |>
  group_by(t, InfectionStatus) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = InfectionStatus)) +
  xlab("Day") +
  ylab("Number of people") +
  ggtitle(sprintf("R0 = %.2f - Population = %d", -log(1 - max_inf/pop_size)/(max_inf/pop_size), pop_size))

