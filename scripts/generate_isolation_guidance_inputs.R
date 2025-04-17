## ===============================================#
## Setup -----------------
## ===============================================#
library(tidyverse)
library(sjmisc)
source("scripts/functions.R")

dt <- 0.2
max_time <- 28

posterior_sampled_path <- file.path(
  "input",
  "isolation_guidance_stan_parameters.csv"
)

sampled_params <- readr::read_csv(,
  file = posterior_sampled_path
)

# Our isolation guidance parameters provide the viral load _with respect to_
# time since symptom. We need to convert times to be with respect to _infection_
# onset.
# Todo (kzs9): Use Sang Woo Park (PNAS, 2022) incubation period and
# proliferation times to develop a principled way to get incubation
# periods consistent with proliferation times.
# For now, we use a bogus incubation period. We know that the incubation
# period must be longer than proliferation period - time of symptom onset.
# We add one as a buffer, and we negate to be wrt to symptom onset.
sampled_params <- sampled_params |>
  dplyr::mutate(infectiousness_start_time = -(wp - tp + 1))

# Our triangle_vl function from our isolation guidance work (copied and put in
# `functions.R` for now) is defined in terms of the time since symptom onset.
# This function is defined in terms of the time since infection.
triangle_vl_wrt_infected_time <- function(
    tp, dp, wp, wr, infectiousness_start_time, max_time, dt) {
  curve <- pmax(triangle_vl(
    seq(infectiousness_start_time, infectiousness_start_time + max_time, dt),
    dp, tp, wp, wr
  ), 0)
  return(list(unnormalized_pdf_to_hazard(curve, dt)))
}

# For every parameter set, make a vector of the viral load over time trajectory.
trajectories <- purrr::pmap_vec(
  list(
    tp = sampled_params$tp,
    dp = sampled_params$dp,
    wp = sampled_params$wp,
    wr = sampled_params$wr,
    infectiousness_start_time = sampled_params$infectiousness_start_time,
    max_time = max_time,
    dt = dt
  ), triangle_vl_wrt_infected_time,
  .progress = TRUE
)

# Prepare the data for long format.
# Turn the list of vectors into a matrix.
trajectories <- as_tibble(trajectories, .name_repair = "minimal") |>
  sjmisc::rotate_df()
times <- seq(dt / 2, max_time, dt) # Add the times for each value
names(trajectories) <- times # As column names

trajectories <- trajectories |>
  dplyr::mutate(id = row_number()) |>
  pivot_longer(cols = -c("id"), names_to = "time", values_to = "value") |>
  # Due to numerical instability, it is possible to have the rate go to infinity
  # at the end of a timeseries -- we need to remove these.
  dplyr::filter(is.finite(value))

write_csv(trajectories,
  file.path(
    "..", "ixa-epi-isolation", "input",
    "library_empirical_rate_fns.csv"
  ),
  col_names = TRUE
)

sampled_params <- sampled_params |>
  dplyr::mutate(
    si_scale =
      calculate_weibull_scale(
        sampled_params$si_beta_0_exponentiated,
        sampled_params$si_beta_wr,
        sampled_params$wr,
        sampled_params$wr_mean,
        sampled_params$wr_std
      )
  )

symptom_parameters <- sampled_params |>
  dplyr::mutate(id = row_number()) |>
  dplyr::select(id, symp_type_cat, si_shape, si_scale)

write_csv(symptom_parameters,
  file.path(
    "..", "ixa-epi-isolation", "input",
    "library_symptom_parameters.csv"
  ),
  col_names = TRUE
)
