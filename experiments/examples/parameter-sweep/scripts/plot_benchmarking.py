from math import log

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import polars as pl
import seaborn as sns
from scipy.stats import poisson

sns.set_theme(style="whitegrid")


def plot_hospitalizations(output_path, param_one, param_two):
    scenario_data = pd.read_csv(output_path)
    target_data = pd.read_csv(
        "experiments/examples/parameter-sweep/input/target_data.csv"
    )
    target_data["total_admissions"] = target_data["total_admissions"]
    # Get unique values for rows and columns
    val_one = sorted(scenario_data[param_one].unique())
    val_two = sorted(scenario_data[param_two].unique())

    fig, axes = plt.subplots(
        nrows=len(val_one),
        ncols=len(val_two),
        figsize=(
            6 * len(val_one),
            4 * len(val_two),
        ),
        sharex=True,
        sharey=True,
    )

    # If axes is 1D, make it 2D for consistency
    if len(val_one) == 1:
        axes = axes[np.newaxis, :]
    if len(val_two) == 1:
        axes = axes[:, np.newaxis]

    for i, one in enumerate(val_one):
        for j, two in enumerate(val_two):
            ax = axes[i, j]
            subset = scenario_data[
                (scenario_data[param_one] == one)
                & (scenario_data[param_two] == two)
            ]
            if subset.empty:
                ax.set_visible(False)
                continue
            subset["mean_hospitalizations"] = subset.groupby(
                ["t_upper", "scenario"], as_index=False
            )["count"].transform("mean")
            if (i, j) == (0, 0):
                sns.lineplot(
                    subset,
                    x="t_upper",
                    y="mean_hospitalizations",
                    hue="guidance_policy>>>UpdatedIsolationGuidance>>>policy_adherence",
                    units="scenario",
                    estimator=None,
                    ax=ax,
                    legend=True,
                )
            else:
                sns.lineplot(
                    subset,
                    x="t_upper",
                    y="mean_hospitalizations",
                    hue="guidance_policy>>>UpdatedIsolationGuidance>>>policy_adherence",
                    units="scenario",
                    estimator=None,
                    ax=ax,
                    legend=False,
                )
            # Plot target data as scatter
            ax.scatter(
                target_data["t"],
                target_data["total_admissions"],
                color="black",
                label="Target Data",
                zorder=10,
            )
            if (i, j) == (0, 0):
                ax.legend()
            ax.set_title(f"{param_one}={one}, {param_two}={two}")

    plt.tight_layout()

    plt.savefig(
        "experiments/examples/parameter-sweep/trajectories_mean.png",
        bbox_inches="tight",
    )


def plot_profiling(output_path):
    df = pd.read_csv(output_path)
    # Plot Simulation runtime
    plt.figure(figsize=(8, 6))
    sns.scatterplot(
        data=df,
        x="pop_size",
        y="cpu_time",
        hue="attack_rate",
        palette="viridis",
    )
    plt.xscale("log")
    plt.yscale("log")
    plt.title("Simulation runtime")
    plt.xlabel("Population Size (log scale)")
    plt.ylabel("CPU Time in Seconds (log scale)")
    plt.legend(title="Attack Rate")
    plt.tight_layout()
    plt.savefig(
        "experiments/examples/parameter-sweep/runtime_by_population.png"
    )

    # Plot Simulation memory
    plt.figure(figsize=(8, 6))
    sns.scatterplot(
        data=df, x="pop_size", y="memory", hue="attack_rate", palette="viridis"
    )
    plt.xscale("log")
    plt.yscale("log")
    plt.title("Simulation Memory")
    plt.xlabel("Population Size (log scale)")
    plt.ylabel("Memory in Bytes (log scale)")
    plt.legend(title="Attack Rate")
    plt.tight_layout()
    plt.savefig(
        "experiments/examples/parameter-sweep/memory_by_population.png"
    )


def hosp_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    def poisson_lhood(model, data):
        return -log(poisson.pmf(model, data) + 1e-12)

    if "t" not in results_data.columns:
        joint_set = target_data.with_columns(
            pl.col("total_admissions")
            .map_elements(
                lambda x: poisson_lhood(0, x),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    else:
        joint_set = (
            results_data.select(pl.col(["t", "count"]))
            .join(
                target_data.select(pl.col(["t", "total_admissions"])),
                on="t",
                how="right",
            )
            .with_columns(pl.col("count").fill_null(strategy="zero"))
        )
        print(joint_set)
        joint_set = joint_set.with_columns(
            pl.struct(["count", "total_admissions"])
            .map_elements(
                lambda x: poisson_lhood(x["count"], x["total_admissions"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    return joint_set.select(pl.col("negloglikelihood").sum()).item()


plot_hospitalizations(
    "experiments/examples/parameter-sweep/hospitalizations_asmp_home.csv",
    "proportion_asymptomatic",
    "settings_properties>>>Home>>>alpha",
)
