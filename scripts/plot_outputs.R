## ===============================#
## Setup ---------
## ===============================#

library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

## ===============================#
## Read person properties reports
## ===============================#

infectious_report <- readr::read_csv(file.path(
  "output",
  "person_property_count.csv"
))

## ===============================#
## Plots
## ===============================#

# Infectious curves
infectious_report |>
  group_by(t, InfectiousStatus) |>
  summarise(count = sum(count), .groups = "drop") |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = InfectiousStatus)) +
  xlab("Day") +
  ylab("Number of people")
