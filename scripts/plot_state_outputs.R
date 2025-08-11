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
  "1000000",
  "person_property_count.csv"
))

person_property_incidence <- readr::read_csv(file.path(
  "output",
  "1000000",
  "incidence_person_property_count.csv"
))

## ===============================#
## Plots ------------
## ===============================#
pop_data <- person_property_report |>
  group_by(t, infection_status) |>
  summarise(count = sum(count), .groups = "drop")
pop_size <- sum(pop_data[pop_data$t == 0, "count"])
max_inf <- sum(pop_data[
  pop_data$t == max(pop_data$t) &
    pop_data$infection_status == "Recovered", "count"
])

## ===============================#
## Infection curves ------------
## ===============================#
inf_prevalence <- person_property_report |>
  group_by(t, infection_status) |>
  summarize(count = sum(count), .groups = "drop")

inf_incidence <- person_property_incidence |>
  filter(property == "InfectionStatus") |>
  group_by(t, property_value) |>
  summarize(count = sum(count), .groups = "drop")
inf_report <- left_join(inf_incidence, inf_prevalence,
  by = c("t" = "t", "property_value" = "infection_status"),
  suffix = c("_incidence", "_prevalence")
) |>
  gather(key = "output", value = "count", -c(t, property_value))


inf_report |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = property_value, linetype = output)) +
  xlab("Day") +
  ylab("Number of people") +
  ggtitle(sprintf(
    "R0 = %.2f - Population = %d",
    -log(1 - max_inf / pop_size) / (max_inf / pop_size), pop_size
  ))

## ===============================#
## Hospitalizations ------------
## ===============================#
hosp_prevalence <- person_property_report |>
  group_by(t, hospitalized) |>
  summarize(hospital_count = sum(count), .groups = "drop") |>
  filter(hospitalized == TRUE) |>
  select(-hospitalized)

hosp_incidence <- person_property_incidence |>
  filter(property == "Hospitalized", property_value == "true") |>
  group_by(t) |>
  summarize(hospital_count = sum(count))
hosp_report <- left_join(hosp_incidence, hosp_prevalence,
  by = c("t"),
  suffix = c("_incidence", "_prevalence")
) |>
  gather(key = "output", value = "count", -t)


hosp_report |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = output)) +
  xlab("Day") +
  ylab("Number of people hospitalized") +
  ggtitle(sprintf(
    "R0 = %.2f - Population = %d",
    -log(1 - max_inf / pop_size) / (max_inf / pop_size), pop_size
  ))
