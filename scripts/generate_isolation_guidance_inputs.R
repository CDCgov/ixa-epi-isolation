## ===============================================#
## Setup -----------------
## ===============================================#
library(tidyverse)
library(sjmisc)
source("scripts/functions.R")
set.seed(108)

# Jason's suggestion: add some time before infectiousness starts
# to get the time of infection. We can calibrate this time and use
# it to check our assumption that infectiousness starts at logVL > 0.
# The time we add is the disease latent period, and the latent period
# plus the time to symptom onset is the incubation period.
pre_infectiousness_mean <- 0.5
pre_infectiousness_std_dev <- 1
dt <- 0.2
max_time <- 28

posterior_sampled_path <- file.path(
  "input",
  "isolation_guidance_stan_parameters.csv"
)

sampled_params <- readr::read_csv(
  file = posterior_sampled_path
)

# Our isolation guidance parameters provide the viral load _with respect to_
# time since symptom. We need to convert times to be with respect to _infection_
# onset. For this reason, we need to estimate the time of infection.
# We do this by assuming that infection happens sometime before whenever we
# assume infectiousness starts (logVL > 0), a calibratable amount of time.
# Note that this time is the disease latent period, and the time from the start
# of the infection to symptom onset is the incubation period
# (this is -infection_start_time), so we can interpret these parameters
# epidemiologically.
# There core assumption here is really that infectiousness starts at logVL > 0.
# If that is not true, we should see the pre infectiousness period run up
# against 0 in the calibration, and we can adjust accordingly.
lognorm_with_constraints <- function(minimum, mu, sd) {
  # Generate a log-normal random variable with a minimum value
  repeat {
    x <- exp(rnorm(1, mean = log(mu), sd = sd))
    if (x > minimum) {
      return(x)
    }
  }
}

sampled_params <- sampled_params |>
  dplyr::mutate(minimum_incubation_period = tp - wp) |>
  dplyr::mutate(
    pre_infectiousness_period =
      purrr::map_dbl(
        minimum_incubation_period,
        lognorm_with_constraints,
        pre_infectiousness_mean,
        pre_infectiousness_std_dev
      )
  ) |>
  # wp is the proliferation period -- time from logVL = 0 to peak logVL
  # tp is the time of peak logVL wrt time since symptom onset
  # so wp - tp is the time from logVL = 0 to symptom onset, and -(wp - tp)
  # is the time at which infectiousness starts. We want to have the
  # infection start before that, so we subtract some time.
  dplyr::mutate(infection_start_time = tp - wp - pre_infectiousness_period)

# Our triangle_vl function from our isolation guidance work (copied and put in
# `functions.R` for now) is defined in terms of the time since symptom onset.
# This function is defined in terms of the time since infection.
triangle_vl_wrt_infected_time <- function(
    tp, dp, wp, wr, infection_start_time, max_time, dt) {
  curve <- pmax(triangle_vl(
    seq(infection_start_time, infection_start_time + max_time, dt),
    dp, tp, wp, wr
  ), 0)
  list(unnormalized_pdf_to_hazard(curve, dt))
}

# For every parameter set, make a vector of the viral load over time trajectory.
trajectories <- purrr::pmap_vec(
  list(
    tp = sampled_params$tp,
    dp = sampled_params$dp,
    wp = sampled_params$wp,
    wr = sampled_params$wr,
    infection_start_time = sampled_params$infection_start_time,
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
  dplyr::filter(is.finite(value)) |>
  # We can save space by removing zeros since the Rust code assumes that the
  # rate is 0 for values outside the time series.
  dplyr::filter(value != 0)

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
  # Same max si as is used in isolation guidance modeling
  dplyr::mutate(incubation_period = -infection_start_time, max_si = 28.0) |>
  dplyr::select(
    id, symp_type_cat,
    incubation_period, si_shape, si_scale, max_si
  )

# Get the dataframe into the right format for ixa epi's progression
# library reader
names(symptom_parameters) <- c(
  "id", "Symptom category", "Incubation period", "Weibull shape",
  "Weibull scale",
  "Weibull upper bound"
)

symptom_parameters <- symptom_parameters |>
  tidyr::pivot_longer(
    cols = -c("id"),
    names_to = "parameter_name",
    values_to = "parameter_value"
  ) |>
  dplyr::mutate(progression_type = "SymptomData") |>
  dplyr::select(id, progression_type, parameter_name, parameter_value)

write_csv(symptom_parameters,
  file.path(
    "..", "ixa-epi-isolation", "input",
    "library_symptom_parameters.csv"
  ),
  col_names = TRUE
)
