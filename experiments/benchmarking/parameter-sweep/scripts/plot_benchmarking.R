library(tidyverse)
ggplot2::theme_set(ggplot2::theme_classic())

df <- readr::read_csv(
  "experiments/benchmarking/parameter-sweep/experiment_runtime.csv"
)


x <- ggplot(
  data = df,
  mapping = aes(
    x = pop_size, y = cpu_time,
    color = attack_rate,
  )
) +
  geom_point() +
  theme_bw() +
  ggtitle("Simulation runtime") +
  scale_x_log10() +
  scale_y_log10() +
  labs(
    x = "Population Size (log scale)",
    y = "CPU Time in Seconds (log scale)",
    color = "Attack Rate"
  )
print(x)

x <- ggplot(
  data = df,
  mapping = aes(
    x = pop_size, y = memory,
    color = attack_rate,
  )
) +
  geom_point() +
  theme_bw() +
  ggtitle("Simulation Memory") +
  scale_x_log10() +
  scale_y_log10() +
  labs(
    x = "Population Size (log scale)",
    y = "Memory in Bytes (log scale)",
    color = "Attack Rate"
  )
print(x)
