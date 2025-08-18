library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

df <- readr::read_csv("scripts/experiments/benchmarking/parameter-sweep/experiment_runtime.csv")


x <- ggplot(
    data = df,
    mapping = aes(
        x = average_time, y = attack_rate,
        color = as.factor(infectiousness_scale),
    )
) +
    geom_point() +
    theme_bw() +
    facet_grid(~ school_alpha, scales = "free") +
    ggtitle("Incidence Report") +
    scale_x_log10() +
    scale_y_log10() +
    labs(
        x = "Average Time (log scale)",
        y = "Attack rate (log scale)",
        color = "Census Tract Alpha"
    )
print(x)

# Fit a linear regression model for each unique pop_size and print the summary
unique_pop_sizes <- unique(df$pop_size)
for (ps in unique_pop_sizes) {
    cat("Results for pop_size =", ps, "\n")
    df_sub <- df %>% filter(pop_size == ps)
    df_sub <- df_sub %>%
        mutate(across(
            c(average_time, censustract_alpha, home_alpha, infectiousness_scale, workplace_alpha, school_alpha),
            ~ scale(.)[, 1],
            .names = "scaled_{.col}"
        ))
    print(head(df_sub))
    model <- lm(
        scaled_average_time ~ scaled_censustract_alpha + scaled_home_alpha + scaled_infectiousness_scale + scaled_workplace_alpha + scaled_school_alpha,
        data = df_sub
    )
    print(summary(model))
    cat("\n-----------------------------\n")
}

