# Load necessary libraries
library(tidyverse)
library(deSolve)
library(jsonlite)

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

# Get input params
SIR_input_params <- fromJSON("./input/input_simple_SIR.json")
pop_size <- nrow(read.csv(
  file = SIR_input_params$epi_isolation.GlobalParams$synth_population_file
))

# Initial conditions
initial_state <- c(S = pop_size - 1, I = 1, R = 0)
initial_state_SEIR <- c(S = pop_size - 1, E = 1, I = 0, R = 0)

# Define parameters
gamma <- 1 / 2
eta <- 1
beta <- 1.5
parameters <- c(beta = beta, gamma = gamma)
parameters_SEIR <- c(beta = beta, eta = eta, gamma = gamma)

# Time sequence for simulation (e.g., from day 0 to day 100)
time_sequence <- seq(0, SIR_input_params$epi_isolation.GlobalParams$max_time,
  by = SIR_input_params$epi_isolation.GlobalParams$report_period
)

# Run the ODE solver
ode_results <- ode(
  y = initial_state,
  times = time_sequence,
  func = sir_model,
  parms = parameters
)

ode_results_df <- ode_results |>
  as.data.frame() |>
  rename(t = time, Susceptible = S, Infectious = I, Recovered = R) |>
  pivot_longer(
    cols = c(Susceptible, Infectious, Recovered),
    names_to = "InfectionStatus",
    values_to = "count"
  ) |>
  mutate(model = "ode_sir", ixa_rep = 100)

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
  ) |>
  mutate(model = "ode_seir", ixa_rep = 100)

# TODO: make Gillespie compartmental models

# plot outputs

ixa_SEIR_df <- read.csv("./output/output_simple_SEIR.csv")

ixa_SEIR_df |>
  filter(InfectionStatus == "Recovered") |>
  ggplot(aes(x = t, y = count, group = paste(InfectionStatus, ixa_rep))) +
  geom_line(
    aes(color = InfectionStatus, linewidth = model, linetype = model)
  ) +
  xlab("Day") +
  ylab("Number of people") +
  scale_linetype_manual(values = c(1, 2, 3)) +
  scale_linewidth_manual(values = c(0.1, 1, 1)) +
  theme_minimal() + 
  geom_line(data = ode_results_df_SEIR |> filter(InfectionStatus == "Recovered"), col = "blue")


ixa_SIR_df <- read.csv("./output/output_simple_SIR.csv")

ixa_SIR_df |>
  filter(InfectionStatus == "Recovered") |>
  ggplot(aes(x = t, y = count, group = paste(InfectionStatus, ixa_rep))) +
  geom_line(
    aes(color = InfectionStatus, linewidth = model, linetype = model)
  ) +
  xlab("Day") +
  ylab("Number of people") +
  scale_linetype_manual(values = c(1, 2, 3)) +
  scale_linewidth_manual(values = c(0.1, 1, 1)) +
  theme_minimal() + 
  geom_line(data = ode_results_df |> filter(InfectionStatus == "Recovered"), col = "blue")
