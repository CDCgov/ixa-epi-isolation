### This script creates empirical rate functions for use in integration tests.
### A single set of rate functions may be used in multiple integration tests.

### This script is assumed to run from the root directory of the repo.

### Empirical rate functions stipulate the timing and intensity of
### infectiousness.

### This script makes 3 sets of empirical rate functions (saved as .csv files):
### rate_fns_exp_I, rate_fns_exp_E_exp_I, rate_fn_triangle

set.seed(456)

### Exponentially distributed infectious period: rate_fns_exp_I ###
### empirical rate functions that correspond to SIR or SIS assumptions

num_ids <- 1000 # number of "draws" of rate function to create
mean_duration_infectious <- 2 # mean number of time units infectious
beta_infectiousness <- 1.5 # expected onward transmissions per unit time

# when infectiousness is constant within infectious period
# we only need to set a start and end time

id <- seq_len(num_ids)
infectious_duration <- rexp(n = num_ids, rate = 1 / mean_duration_infectious)

rate_fns_exp_I_start_df <- data.frame(
    "id" = id,
    "time" = 0,
    "value" = beta_infectiousness)

rate_fns_exp_I_end_df <- data.frame(
    "id" = id,
    "time" = infectious_duration,
    "value" = beta_infectiousness)

rate_fns_exp_I <- rbind(rate_fns_exp_I_start_df, rate_fns_exp_I_end_df)
rate_fns_exp_I <- rate_fns_exp_I[order(rate_fns_exp_I$id), ]

write.csv(x = rate_fns_exp_I,
    file = "tests/data/rate_fns_exp_I.csv",
    row.names = FALSE)

### Exponentially distributed latent and infectious periods:
### rate_fns_exp_E_exp_I ###
### empirical rate functions that correspond to SEIR or SEIS assumptions

mean_duration_latent <- 1 # mean number of time units in latent period

latent_duration <- rexp(n = num_ids, rate = 1 / mean_duration_latent)

rate_fns_exp_E_exp_I_start_df <- data.frame(
    "id" = id,
    "time" = latent_duration,
    "value" = beta_infectiousness)

rate_fns_exp_E_exp_I_end_df <- data.frame(
    "id" = id,
    "time" = latent_duration + infectious_duration,
    "value" = beta_infectiousness)

rate_fns_exp_E_exp_I <- rbind(rate_fns_exp_E_exp_I_start_df,
rate_fns_exp_E_exp_I_end_df)
rate_fns_exp_E_exp_I <- rate_fns_exp_E_exp_I[order(rate_fns_exp_E_exp_I$id), ]

write.csv(x = rate_fns_exp_E_exp_I,
    file = "tests/data/rate_fns_exp_E_exp_I.csv",
    row.names = FALSE)

### Empirical rate function for "triangle" infectiousness: rate_fn_triangle ###
### empirical rate function where infectiousness starts 1 time unit after
### infection, peaks 0.5 time units later, and ends 1.5 time units after that.
### peak value of beta is 3, so the area under the curve is the same as in
### the above two examples
### the mean latent and infectious periods are the same as above, but
### this rate function is different insofar as 1) intensity of infectiousness is
### time-varying, and 2) the same rate function is applied to all infections

rate_fn_triangle <- data.frame(
    "id" = 1,
    "time" = c(1, 1.5, 3),
    "value" = c(0, 3, 0))

write.csv(x = rate_fn_triangle, file = "tests/data/rate_fn_triangle.csv",
row.names = FALSE)
