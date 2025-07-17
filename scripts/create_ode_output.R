### Generate ordinary differential equation compartmental model output ###
# Output from these ordinary differential equations will be used to compare
# to Ixa output for the purpose of integration testing; importantly, a single
# ODE simulation run may be used in multiple integration tests.

### Define parameters ###
# single place in script for hard-coded parameters
pop_size <- 100
init_infect <- 1

gamma <- 1 / 2
eta <- 1
beta <- 1.5

max_time <- 50
time_step <- 1

# Load necessary libraries
library(tidyverse)
library(deSolve)

file_path <- "tests/input"
if (!dir.exists(file_path)) {
  dir.create(file_path)
}

# Define the SIR model function
sir_model <- function(time, state, parameters) {
  # Unpack state variables
  s <- state[1]
  i <- state[2]
  r <- state[3]

  # Unpack parameters
  beta <- parameters["beta"]
  gamma <- parameters["gamma"]

  # Calculate derivatives
  ds <- -beta * s * i / (s + i + r)
  di <- beta * s * i / (s + i + r) - gamma * i
  dr <- gamma * i

  list(c(ds, di, dr))
}

# Define the SEIR model function
seir_model <- function(time, state, parameters) {
  # Unpack state variables
  s <- state[1]
  e <- state[2]
  i <- state[3]
  r <- state[4]

  # Unpack parameters
  beta <- parameters["beta"]
  eta <- parameters["eta"]
  gamma <- parameters["gamma"]

  # Calculate derivatives
  ds <- -beta * s * i / (s + e + i + r)
  de <- beta * s * i / (s + e + i + r) - eta * e
  di <- eta * e - gamma * i
  dr <- gamma * i

  list(c(ds, de, di, dr))
}

# Initial conditions
initial_state_sir <- c(s = pop_size - init_infect, i = init_infect, r = 0)
initial_state_seir <- c(
  s = pop_size - init_infect, e = init_infect,
  i = 0, r = 0
)

# Create parameter vectors
parameters_sir <- c(beta = beta, gamma = gamma)
parameters_seir <- c(beta = beta, eta = eta, gamma = gamma)

# Time sequence for simulation output
time_sequence <- seq(0, max_time, by = time_step)

# Run the ODE solver for the base SIR model
ode_results_sir <- ode(
  y = initial_state_sir,
  times = time_sequence,
  func = sir_model,
  parms = parameters_sir
)

ode_results_df_sir <- ode_results_sir |>
  as.data.frame() |>
  rename(t = time, Susceptible = s, Infectious = i, Recovered = r) |>
  pivot_longer(
    cols = c(Susceptible, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  )

write.csv(
  x = ode_results_df_sir,
  file = "tests/input/ode_results_SIR.csv",
  row.names = FALSE,
  na = ""
)

# Run the ODE solver for the base SEIR model
ode_results_seir <- ode(
  y = initial_state_seir,
  times = time_sequence,
  func = seir_model,
  parms = parameters_seir
)

ode_results_df_seir <- ode_results_seir |>
  as.data.frame() |>
  rename(
    t = time, Susceptible = s, Exposed = e,
    Infectious = i, Recovered = r
  ) |>
  pivot_longer(
    cols = c(Susceptible, Exposed, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  )

write.csv(
  x = ode_results_df_seir,
  file = "tests/input/ode_results_SEIR.csv",
  row.names = FALSE,
  na = ""
)

# Run the ODE solver for the SIR model applicable to 1/4 of time spent in
# a unique household per person

# Create parameter vectors
parameters_sir_unique_hh <- c(beta = beta * 3/4, gamma = gamma)

ode_results_sir_unique_hh <- ode(
  y = initial_state_sir,
  times = time_sequence,
  func = sir_model,
  parms = parameters_sir_unique_hh
)

ode_results_df_sir_unique_hh <- ode_results_sir_unique_hh |>
  as.data.frame() |>
  rename(t = time, Susceptible = s, Infectious = i, Recovered = r) |>
  pivot_longer(
    cols = c(Susceptible, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  )

write.csv(
  x = ode_results_df_sir_unique_hh,
  file = "tests/input/ode_results_SIR_unique_hh.csv",
  row.names = FALSE,
  na = ""
)
