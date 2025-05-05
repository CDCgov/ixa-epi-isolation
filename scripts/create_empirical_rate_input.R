# generate empirical rate functions assuming different distributions
# for timing / duration of infectiousness

# create empirical rate function csv files that correspond to:
# 1) classic SIR model (exponentially distributed I, no latent period)
# 2) classic SEIR model (exponentially distributed E and I)

num_ids <- 1000 # number of "draws" of rate function to create
mean_duration_infectious <- 2 # mean number of time units infectious
beta_infectiousness <- 1.5 # expected onward transmissions per unit time
mean_duration_latent <- 1 # mean number of time units in latent

set.seed(456)

# when infectiousness is constant within infectious period
# we only need to set a start and end time

id <- seq_len(num_ids)
infectious_duration <- rexp(n = num_ids, rate= 1 / mean_duration_infectious)
latent_duration <- rexp(n = num_ids, rate= 1 / mean_duration_latent)

# set up for SIR

SIR_infectious_start_df <- data.frame(
    id,
    "time" = 0,
    "value" = beta_infectiousness)

SIR_infectious_end_df <- data.frame(
    id,
    "time" = round(infectious_duration, 4),
    "value" = beta_infectiousness)

SIR_infectious_df <- rbind(SIR_infectious_start_df, SIR_infectious_end_df)
SIR_infectious_df <- 
    SIR_infectious_df[order(SIR_infectious_df$id), ]

write.csv(x = SIR_infectious_df, file = "input/rate_fns_SIR.csv", row.names = FALSE)

# set up for SEIR

SEIR_infectious_start_df <- data.frame(
    id,
    "time" = latent_duration,
    "value" = beta_infectiousness)

SEIR_infectious_end_df <- data.frame(
    id,
    "time" = latent_duration + infectious_duration,
    "value" = beta_infectiousness)

SEIR_infectious_df <- rbind(
    SEIR_infectious_start_df,
    SEIR_infectious_end_df)
SEIR_infectious_df <- 
    SEIR_infectious_df[order(SEIR_infectious_df$id), ]

write.csv(x = SEIR_infectious_df, file = "input/rate_fns_SEIR.csv", row.names = FALSE)
