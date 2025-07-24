from math import floor

import matplotlib.pyplot as plt
import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import gamma, norm

# Misspecified prior for scale that should be 1.0
prior = {
    "infectiousness_rate_fn": {
        "EmpiricalFromFile": {"scale": gamma(a=1, scale=0.5)}
    }
}

perturbation = {
    "infectiousness_rate_fn": {"EmpiricalFromFile": {"scale": norm(0, 0.05)}}
}

config_file = "experiments/examples/scale/input/config.yaml"

experiment = Experiment(
    experiments_directory="experiments",
    config_file=config_file,
    prior_distribution_dict=prior,
    perturbation_kernel_dict=perturbation,
)

distances = pl.read_parquet("experiments/examples/scale/data/distances")
results = pl.read_parquet("experiments/examples/scale/data/simulations")

# Apply experiment.step_from_index(simulation)
results = results.with_columns(
    pl.col("simulation")
    .map_elements(
        lambda x: experiment.step_from_index(x), return_dtype=pl.Int32
    )
    .alias("step")
)
distances = distances.with_columns(
    pl.col("simulation")
    .map_elements(
        lambda x: experiment.step_from_index(x), return_dtype=pl.Int32
    )
    .alias("step")
)
target_data = experiment.target_data
min_t_target = target_data.select(pl.col("t").min())

target_data = target_data.filter(
    pl.col("InfectionStatus") == "Infectious"
).with_columns(pl.col("t") - min_t_target.item())


# PLot target data over simulations for each tolerance step
# Add histograms of distances for the same steps below the line and scatter plots
fig, axes = plt.subplots(2, 2, sharey="row", figsize=(10, 10))
axes = axes.flatten()

for i, step in enumerate([0, 2]):
    # Line and scatter plots
    print(step)
    ax = axes[i]
    step_results = results.filter(pl.col("step") == step)

    for simulation in step_results["simulation"].unique():
        sim_data = step_results.filter(
            pl.col("simulation") == simulation
        ).sort("t")
        ax.plot(
            sim_data["t"], sim_data["result_count"], alpha=0.05, color="blue"
        )

    ax.scatter(
        target_data["t"],
        target_data["count"],
        label="Target",
        color="red",
        s=50,
        zorder=3000,
    )
    ax.set_title(f"Step {step}")
    ax.legend()

    # Histogram of distances
    ax_hist = axes[i + 2]
    step_distances = distances.filter(
        (pl.col("step") == step) & (pl.col("distance") < 200)
    )
    ax_hist.hist(step_distances["distance"], bins=30, color="gray", alpha=0.7)
    ax_hist.set_title(f"Distances Histogram Step {step}")
    ax_hist.set_xlabel("Distance")
    ax_hist.set_ylabel("Frequency")

plt.tight_layout()
plt.show()
