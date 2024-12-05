## ===============================#
## Setup ---------
## ===============================#

library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

## ===============================#
## Read person properties reports
## ===============================#

demographic_report <- readr::read_csv(file.path(
  "output",
  "person_demographics_count.csv"
))

infectious_report <- readr::read_csv(file.path(
  "output",
  "person_infectious_count.csv"
))

## ===============================#
## Plots
## ===============================#

# Demographics
demographic_report |>
  dplyr::filter(t == 0) |>
  group_by(Age) |>
  summarise(count = sum(count)) |>
  ggplot(aes(x = Age, y = count)) +
  geom_bar(stat = "identity") +
  xlab("Age") +
  ylab("Count at start of simulation")

# In the future, we will add a report of count of people by age group over time.

# Infection curve
ggplot2::ggplot() +
  geom_line(
    data = infectious_report,
    aes(x = t, y = count, color = InfectiousStatus)
  ) +
  xlab("Time") +
  ylab("Count")
