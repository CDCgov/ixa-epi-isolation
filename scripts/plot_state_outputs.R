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

incidence_counts <- person_property_incidence |>
  group_by(t_upper, event) |>
  summarize(count = sum(count), .groups = "drop")

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

inf_report <- left_join(
  inf_prevalence, incidence_counts,
  by = c("t" = "t_upper", "infection_status" = "event"),
  suffix = c("_prevalence", "_incidence")
) |>
  gather(key = "output", value = "count", -c(t, infection_status))


inf_report |>
  ggplot(aes(x = t, y = count)) +
  geom_line(aes(color = infection_status, linetype = output)) +
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
  filter(hospitalized == TRUE) |>
  group_by(t) |>
  summarize(hospitalized = sum(count), .groups = "drop")

hosp_incidence <- person_property_incidence |>
  filter(event == "Hospitalized") |>
  group_by(t_upper) |>
  summarize(hospitalized = sum(count))

hosp_report <- left_join(hosp_prevalence, hosp_incidence,
  by = c("t" = "t_upper"),
  suffix = c("_prevalence", "_incidence")
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
