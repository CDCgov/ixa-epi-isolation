### Generate ordinary differential equation compartmental model output ###
# Output from these ordinary differential equations will be used to compare
# to Ixa output for the purpose of integration testing; importantly, a single
# ODE simulation run may be used in multiple integration tests.

# Load necessary libraries
library(tidyverse)
library(deSolve)

file_path <- "tests/input"
if (!dir.exists(file_path)){
    dir.create(file_path)
}

# Define the SIR model function
sir_model <- function(time, state, parameters) {
  # Unpack state variables
  S <- state[1]
  I <- state[2]
  R <- state[3]

  # Unpack parameters
  beta <- parameters["beta"]
  gamma <- parameters["gamma"]

  # Calculate derivatives
  dS <- -beta * S * I / (S + I + R)
  dI <- beta * S * I / (S + I + R) - gamma * I
  dR <- gamma * I

  return(list(c(dS, dI, dR)))
}

# Define the SEIR model function
seir_model <- function(time, state, parameters) {
  # Unpack state variables
  S <- state[1]
  E <- state[2]
  I <- state[3]
  R <- state[4]

  # Unpack parameters
  beta <- parameters["beta"]
  eta <- parameters["eta"]
  gamma <- parameters["gamma"]

  # Calculate derivatives
  dS <- -beta * S * I / (S + E + I + R)
  dE <- beta * S * I / (S + E + I + R) - eta * E
  dI <- eta * E - gamma * I
  dR <- gamma * I

  return(list(c(dS, dE, dI, dR)))
}

# Define the SIR model function for a two-population metapopulation model
sir_model <- function(time, state, parameters) {
  # Unpack state variables
  S <- state[1]
  I <- state[2]
  R <- state[3]

  # Unpack parameters
  beta <- parameters["beta"]
  gamma <- parameters["gamma"]

  # Calculate derivatives
  dS <- -beta * S * I / (S + I + R)
  dI <- beta * S * I / (S + I + R) - gamma * I
  dR <- gamma * I

  return(list(c(dS, dI, dR)))
}

pop_size <- 50

# Initial conditions
initial_state_SIR <- c(S = pop_size - 1, I = 1, R = 0)
initial_state_SEIR <- c(S = pop_size - 1, E = 1, I = 0, R = 0)

# Define parameters
gamma <- 1 / 2
eta <- 1
beta <- 1.5
parameters_SIR <- c(beta = beta, gamma = gamma)
parameters_SEIR <- c(beta = beta, eta = eta, gamma = gamma)

# Time sequence for simulation (e.g., from day 0 to day 100)
time_sequence <- seq(0, 50, by = 1)

# Run the ODE solver
ode_results_SIR <- ode(
  y = initial_state_SIR,
  times = time_sequence,
  func = sir_model,
  parms = parameters_SIR
)

ode_results_df_SIR <- ode_results_SIR |>
  as.data.frame() |>
  rename(t = time, Susceptible = S, Infectious = I, Recovered = R) |>
  pivot_longer(
    cols = c(Susceptible, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  )

write.csv(
  x = ode_results_df_SIR,
  file = "tests/input/ode_results_SIR.csv",
  row.names = FALSE,
  na = ""
)

ode_results_SEIR <- ode(
  y = initial_state_SEIR,
  times = time_sequence,
  func = seir_model,
  parms = parameters_SEIR
)

ode_results_df_SEIR <- ode_results_SEIR |>
  as.data.frame() |>
  rename(t = time, Susceptible = S, Exposed = E, Infectious = I, Recovered = R) |>
  pivot_longer(
    cols = c(Susceptible, Exposed, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  )

write.csv(
  x = ode_results_df_SEIR,
  file = "tests/input/ode_results_SEIR.csv",
  row.names = FALSE,
  na = ""
)

# Additional ODE output that is needed
# SIR ODE output, R0 = 2 (beta = 1, mean duration infectiousness 2)