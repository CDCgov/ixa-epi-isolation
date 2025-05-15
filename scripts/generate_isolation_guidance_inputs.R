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
# plus the time from infectiousness onset to symptom onset is the
# disease incubation period.
log_pre_infectiousness_mean <- 0.5
log_pre_infectiousness_std_dev <- 1
dt <- 0.2
max_time <- 28

posterior_sampled_path <- file.path(
  "input",
  "isolation_guidance_stan_parameters.csv"
)

sampled_params <- readr::read_csv(
  file = posterior_sampled_path
)

# Our isolation guidance parameters provide the viral load with respect to
# time since _symptom onset_. We need to convert times to be with respect to
# _infection_ onset because in our ABM we need to start forecasting
# infectiousness the moment the agent becomes infectious rather than when they
# start showing symptoms.
# For this reason, we need to estimate the time of infection.
# We do this by assuming that the actual infection event happens sometime
# before whenever we assume infectiousness starts (logVL > 0).
# This time is calibratable and is biologically the disease latent period.
# The time from the start of the infection to symptom onset is the incubation
# period (this is -infection_start_time), so we can interpret these parameters
# and relationships among them epidemiologically.
# The core assumption here is really that infectiousness starts at logVL > 0.
# If that is not true, when we do model calibration of the pre infectiousness
# period, we should see the posterior for the pre infectiousness period run up
# against 0, and we can adjust the assumption that logVL > 0 marks the start of
# infectiousness accordingly, making the start of infectiousness at a lower
# logVL instead.
lognorm_with_constraints <- function(minimum, mu, sd) {
  # Generate a log-normal random variable with a minimum value
  repeat {
    l <- rlnorm(1, meanlog = mu, sdlog = sd)
    if (l > minimum) {
      return(l)
    }
  }
}

sampled_params <- sampled_params |>
  dplyr::mutate(minimum_latent_period = tp - wp) |>
  dplyr::mutate(
    latent_period =
      purrr::map_dbl(
        minimum_latent_period,
        lognorm_with_constraints,
        log_pre_infectiousness_mean,
        log_pre_infectiousness_std_dev
      )
  ) |>
  # wp is the proliferation period -- time from logVL = 0 to peak logVL
  # tp is the time of peak logVL wrt time since symptom onset
  # so wp - tp is the time from logVL = 0 to symptom onset, and -(wp - tp)
  # is the time at which infectiousness starts. We want to have the
  # infection start before that, so we subtract some time.
  dplyr::mutate(infection_start_time = tp - wp - latent_period)

stopifnot(
  all(sampled_params$infection_start_time <= 0)
)

# Our triangle_vl function from our isolation guidance work (copied and put in
# `functions.R` for now) is defined in terms of the time since symptom onset.
# This function is defined in terms of the time since infection.
triangle_vl_wrt_infected_time <- function(
    tp, dp, wp, wr, infection_start_time, max_time, dt) {
  curve <- pmax(triangle_vl(
    seq(infection_start_time, infection_start_time + max_time, dt),
    dp, tp, wp, wr
  ), 0)
  # The triangle vl curves tell us the rate of transmission over time, so we
  # treat them alone as the hazard rate.
  # TODO (kzs9): allow for adding a viral_load_to_infectiousness function here
  # which transforms the viral load to an infectiousness function using our
  # isolation guidance assumptions for the different functional forms of
  # infectiousness.
  # When we do that, we also need to figure out a more mature way to handle
  # the start of infectiousness. Do we stick with saying that infectiousness
  # starts at logVL > 0 even though our other curves are no longer just the
  # the logVL? Or, do we pick some threshold based on the specific viral load
  # to infectiousness function we are using?
  list(curve)
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
times <- seq(0, max_time, dt) # Add the times for each value
names(trajectories) <- times # As column names

trajectories <- trajectories |>
  dplyr::mutate(id = row_number()) |>
  pivot_longer(cols = -c("id"), names_to = "time", values_to = "value") |>
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
