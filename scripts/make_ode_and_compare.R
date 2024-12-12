### Exploratory script to compare Ixa results to ODEs

# Load necessary libraries
library(deSolve)
library(jsonlite)

set.seed(1234) # because stochastic sims to get duration infectiousness

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

# Get input params
input_params <- fromJSON("./input/input.json")
pop_size <- nrow(read.csv(
  file = input_params$epi_isolation.Parameters$synth_population_file
))

# Obtain via simulation an estimate of the mean duration of infectiousness

R0 <- input_params$epi_isolation.Parameters$r_0
gi <- input_params$epi_isolation.Parameters$generation_interval
reps <- 1e6

ixa_style_duration_I_fx <- function(R0, gi) {
  infect_attempts <- rpois(n = 1, lambda = R0)
  ifelse(test = infect_attempts == 0,
    yes = 0,
    no = max(rexp(n = infect_attempts, rate = 1 / gi))
  )
}

sim_I_dur_ixa <-
  replicate(n = reps, expr = ixa_style_duration_I_fx(R0 = R0, gi = gi))

mean_sim_I_dur_ixa <- mean(sim_I_dur_ixa)

# Initial conditions
initial_state <- c(S = pop_size - 1, I = 1, R = 0)

# Define parameters
gamma <- 1 / mean_sim_I_dur_ixa
beta <- input_params$epi_isolation.Parameters$r_0 * gamma
parameters <- c(beta = beta, gamma = gamma)

# Time sequence for simulation (e.g., from day 0 to day 100)
time_sequence <- seq(0, input_params$epi_isolation.Parameters$max_time,
  by = input_params$epi_isolation.Parameters$report_period
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
    names_to = "InfectiousStatus",
    values_to = "count"
  ) |>
  mutate(model = "ode_sir")

# make a model where there are multiple infectious compartments
# this is to change the distribution of duration of infectiousness

# Define the SIR model function
s3ir_model <- function(time, state, parameters) {
  # Unpack state variables
  S <- state[1]
  I1 <- state[2]
  I2 <- state[3]
  I3 <- state[4]
  R <- state[5]

  # Unpack parameters
  beta <- parameters["beta"]
  gamma <- parameters["gamma"] * 3

  # Calculate derivatives
  dS <- -beta * S * (I1 + I2 + I3) / (S + I1 + I2 + I3 + R)
  dI1 <- beta * S * (I1 + I2 + I3) / (S + I1 + I2 + I3 + R) - gamma * I1
  dI2 <- gamma * I1 - gamma * I2
  dI3 <- gamma * I2 - gamma * I3
  dR <- gamma * I3 # Change in recovered population

  return(list(c(dS, dI1, dI2, dI3, dR)))
}

# Initial conditions
initial_state_s3ir <- c(S = pop_size - 1, I1 = 1, I2 = 0, I3 = 0, R = 0)

# Run the ODE solver
ode_s3iR_results <- ode(
  y = initial_state_s3ir,
  times = time_sequence,
  func = s3ir_model,
  parms = parameters
)

ode_s3ir_results_df <- ode_s3iR_results |>
  as.data.frame() |>
  mutate(I = I1 + I2 + I3) |>
  select(!c(I1, I2, I3)) |>
  rename(t = time, Susceptible = S, Infectious = I, Recovered = R) |>
  pivot_longer(
    cols = c(Susceptible, Infectious, Recovered),
    names_to = "InfectiousStatus",
    values_to = "count"
  ) |>
  mutate(model = "ode_s3ir")

# Load simulation results from Ixa model
ixa_results_df <- read.csv(
  file = "./output/person_property_count_multi_rep.csv"
)

# combine ODE and stochastic results and make plots
ode_results_df <- ode_results_df |>
  mutate(ixa_rep = max(ixa_results_df$ixa_rep) + 1)
ode_s3ir_results_df <- ode_s3ir_results_df |>
  mutate(ixa_rep = max(ixa_results_df$ixa_rep) + 2)
results_df <- rbind(ixa_results_df, ode_results_df, ode_s3ir_results_df)

results_df |>
  ggplot(aes(x = t, y = count, group = paste(InfectiousStatus, ixa_rep))) +
  geom_line(
    aes(color = InfectiousStatus, linewidth = model, linetype = model)
  ) +
  xlab("Day") +
  ylab("Number of people") +
  scale_linetype_manual(values = c(1, 2, 3)) +
  scale_linewidth_manual(values = c(0.1, 1, 1)) +
  theme_minimal()

results_df |>
  filter(InfectiousStatus == "Infectious") |>
  ggplot(aes(x = t, y = count, group = paste(InfectiousStatus, ixa_rep))) +
  geom_line(
    aes(color = InfectiousStatus, linewidth = model, linetype = model)
  ) +
  xlab("Day") +
  ylab("Number of people") +
  scale_linetype_manual(values = c(1, 2, 3)) +
  scale_linewidth_manual(values = c(0.1, 1, 1)) +
  theme_minimal()

# simulate durations of infectiousness and plot cdfs for different assumptions

s3ir_duration_I_fx <- function(n_I_comp, mean_ixa) {
  sum(rexp(n = n_I_comp, rate = 1 / mean_ixa * n_I_comp))
}

sim_I_dur_ode_sir <-
  rexp(n = reps, rate = 1 / mean_sim_I_dur_ixa)

sim_I_dur_ode_s3ir <-
  replicate(n = reps, expr = s3ir_duration_I_fx(
    n_I_comp = 3,
    mean_ixa = mean_sim_I_dur_ixa
  ))

plot(ecdf(sim_I_dur_ixa), main = "CDF of duration of infectiousness")
plot(ecdf(sim_I_dur_ode_sir), col = "orange", add = TRUE)
plot(ecdf(sim_I_dur_ode_s3ir), col = "blue", add = TRUE)

mean(sim_I_dur_ixa)
mean(sim_I_dur_ode_sir)
mean(sim_I_dur_ode_s3ir)
