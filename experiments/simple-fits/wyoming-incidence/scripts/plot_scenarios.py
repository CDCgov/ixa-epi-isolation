from math import log

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import polars as pl
import seaborn as sns
from scipy.stats import poisson

sns.set_theme(style="whitegrid")


def plot_hospitalizations_simple(output_path):
    scenario_data = pd.read_csv(output_path)
    target_data = pd.read_csv(
        "experiments/simple-fits/wyoming-incidence/input/target_data.csv"
    )
    scenario_data["burn_in_period"] = (scenario_data["burn_in_period"] // 7) * 7
    scenario_data["t_adjusted"] = (scenario_data["t_upper"] + scenario_data["burn_in_period"]).astype(int)
    scenario_data = scenario_data[scenario_data["t_adjusted"] >= 0]
    
    scenario_data["mean_hospitalizations"] = scenario_data.groupby(
        ["t_upper", "scenario"], as_index=False
    )["count"].transform("mean")
    # Create a new figure and axis
    plt.figure(figsize=(10, 6))
    # Create subplots for each level of Policy Adherence
    policy_adherence_levels = scenario_data["Policy Adherence"].unique()
    policy_adherence_levels = sorted(policy_adherence_levels[policy_adherence_levels != 0])
    num_levels = len(policy_adherence_levels)
    fig, axes = plt.subplots(
        nrows=1,
        ncols=num_levels,
        figsize=(10, 6 * num_levels),
        sharex=True,
        sharey=True,
    )

    if num_levels == 1:
        axes = [axes]  # Ensure axes is iterable for a single subplot

    for ax, level in zip(axes, policy_adherence_levels):
        subset = scenario_data[(scenario_data["Policy Adherence"] == level) | (scenario_data["Policy Adherence"] == 0)]
        sns.lineplot(
            subset,
            x="t_adjusted",
            y="count",
            hue="Guidance",
            units="scenario",
            estimator=None,
            ax=ax,
            legend=True,
        )
        sns.scatterplot(target_data, x="t", y="total_admissions", ax=ax, zorder=10, label = "Target Data", legend = True)
        ax.set_title(f"Policy Adherence: {level}")
        ax.set_xlabel("Time")
        ax.set_ylabel("Hospitalizations")

    plt.show()
    
    fig, axes = plt.subplots(
        nrows=1,
        ncols=num_levels,
        figsize=(10, 6 * num_levels),
        sharex=True,
        sharey=True,
    )

    if num_levels == 1:
        axes = [axes]  # Ensure axes is iterable for a single subplot

    
    for ax, level in zip(axes, policy_adherence_levels):
        subset = scenario_data[(scenario_data["Policy Adherence"] == level) | (scenario_data["Policy Adherence"] == 0)]
        sns.lineplot(
            subset,
            x="t_adjusted",
            y="count",
            hue="Guidance",
            estimator="median",
            errorbar=lambda x: (x.quantile(0.25), x.quantile(0.75)),
            ax = ax,
            legend=True,  # Show legend for this plot
        )
        sns.lineplot(
            subset,
            x="t_adjusted",
            y="count",
            hue="Guidance",
            estimator="median",
            errorbar=lambda x: (x.quantile(0.025), x.quantile(0.975)),
            ax = ax,
            legend=False,  # Show legend for this plot
        )

        sns.scatterplot(target_data, x="t", y="total_admissions", ax = ax, zorder=10, label = "Target Data", legend = True)
        ax.set_title(f"Policy Adherence: {level}")
        ax.set_xlabel("Time")
        ax.set_ylabel("Hospitalizations")
    
    plt.show()
    
    subset = scenario_data[scenario_data["Policy Adherence"] == 0]
    sns.lineplot(
        subset,
        x="t_adjusted",
        y="count",
        hue="Guidance",
        estimator="median",
        errorbar=lambda x: (x.quantile(0.25), x.quantile(0.75)),
        legend=True,  # Show legend for this plot
    )
    sns.lineplot(
        subset,
        x="t_adjusted",
        y="count",
        hue="Guidance",
        estimator="median",
        errorbar=lambda x: (x.quantile(0.025), x.quantile(0.975)),
        legend=False,  # Show legend for this plot
    )

    sns.scatterplot(target_data, x="t", y="total_admissions", zorder=10, label = "Target Data", legend = True)
    plt.title("Model Calibration")
    plt.xlabel("Time")
    plt.ylabel("Hospitalizations")
    
    plt.show()
    


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

plot_hospitalizations_simple(
    "experiments/simple-fits/wyoming-incidence/hospitalizations_wy.csv",
)

# data = pl.read_csv("output/incidence_report.csv")
# data = data.filter(pl.col("event") == "Hospitalized")
# data = data.group_by("t_upper").agg(pl.col("count").sum().alias("count"))

# plt.figure(figsize=(10, 6))
# sns.lineplot(data=data, x="t_upper", y="count")
# plt.xlabel("Time (t_upper)")
# plt.ylabel("Count")
# plt.title("Hospitalizations over Time")
# plt.show()