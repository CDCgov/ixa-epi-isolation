## ===============================#
## Setup ---------
## ===============================#

library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

## ===============================#
## Read person properties reports
## ===============================#

infections <- readr::read_csv(file.path(
  "person_property_count.csv"
))

## ===============================#
## Plots
## ===============================#

# Infectious curves
infections |>
  group_by(t, InfectionStatus) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = InfectionStatus)) +
  xlab("Day") +
  ylab("Number of people")
