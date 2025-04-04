## ===============================================#
## Setup -----------------
## ===============================================#
library(tidyverse)
library(sjmisc)
source("scripts/functions.R")

num_trajectories <- 10
dt <- 0.2
max_time <- 28

posterior_sampled_path <- file.path(
  "input",
  "library_empirical_rate_parameters.csv"
)

sampled_params <- readr::read_csv(,
  file = posterior_sampled_path
)

sampled_params <- sampled_params |>
  # subsample
  dplyr::slice_sample(n = num_trajectories) |>
  dplyr::filter(symp_type_cat != 4) |>
  # for now, let's set some bogus infectiousness start times to shift us from
  # a space that is in terms of peak VL to in terms of time since infection
  # these times are with respect to when the peak viral load occurs
  # these are the shortest times + 1 day that allows the period
  # logVL > 0 to not be truncated
  dplyr::mutate(infectiousness_start_time = -(wp - tp + 1))

sampled_params <- sampled_params |>
  dplyr::select(
    tp, dp, wp, wr, infectiousness_start_time, si_time, symp_type_cat
  )

triangle_vl_wrt_infected_time <- function(
    tp, dp, wp, wr, infectiousness_start_time, max_time, dt) {
  curve <- pmax(triangle_vl(
    seq(infectiousness_start_time, infectiousness_start_time + max_time, dt),
    dp, tp, wp, wr
  ), 0)
  return(list(unnormalized_pdf_to_hazard(curve, dt)))
}

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

trajectories <- as_tibble(trajectories, .name_repair = "minimal") |>
  sjmisc::rotate_df()

# subsample
times <- seq(dt / 2, max_time, dt)
names(trajectories) <- times

trajectories <- trajectories |>
  dplyr::mutate(id = row_number()) |>
  pivot_longer(cols = -c("id"), names_to = "time", values_to = "value") |>
  dplyr::filter(value != 0 & !is.na(value) & is.finite(value))

write_csv(trajectories,
  file.path(
    "..", "ixa-epi-isolation", "input",
    "library_empirical_rate_fns.csv"
  ),
  col_names = TRUE
)
