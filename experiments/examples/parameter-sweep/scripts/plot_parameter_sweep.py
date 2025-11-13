import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns

sns.set_theme(style="whitegrid")


def plot_hospitalizations(output_path, param_one, param_two, param_three):
    scenario_data = pd.read_csv(output_path)
    scenario_data = scenario_data.rename(columns={"t_upper": "Time"})
    # Rename columns for clarity
    scenario_data = scenario_data.rename(
        columns={
            param_one: "home_alpha",
            param_two: "school_alpha",
            param_three: "workplace_alpha",
        }
    )
    param_one = param_one.replace(
        "settings_properties>>>Home>>>alpha", "home_alpha"
    )
    param_two = param_two.replace(
        "settings_properties>>>School>>>alpha", "school_alpha"
    )
    param_three = param_three.replace(
        "setting_properties>>>Workplace>>>alpha", "workplace_alpha"
    )

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
            subset["Infections"] = subset.groupby(
                ["Time", "scenario"], as_index=False
            )["count"].transform("mean")

            # Increase font size for axes
            ax.tick_params(axis="both", labelsize=16)
            ax.set_title(f"{param_one}={one}, {param_two}={two}", fontsize=18)
            ax.set_xlabel(ax.get_xlabel(), fontsize=16)
            ax.set_ylabel(ax.get_ylabel(), fontsize=16)
            # Plot lines with increased thickness
            if (i, j) == (0, 0):
                sns.lineplot(
                    subset,
                    x="Time",
                    y="Hospitalized",
                    hue=param_three,
                    units="scenario",
                    estimator=None,
                    ax=ax,
                    legend=True,
                    linewidth=3,
                )
            else:
                sns.lineplot(
                    subset,
                    x="Time",
                    y="Hospitalized",
                    hue=param_three,
                    units="scenario",
                    estimator=None,
                    ax=ax,
                    legend=False,
                    linewidth=3,
                )
            if (i, j) == (0, 0):
                ax.legend(fontsize=14)
    plt.tight_layout()
    # Increase font size for the whole figure
    plt.savefig(
        "experiments/examples/parameter-sweep/parameter_sweep.png",
        bbox_inches="tight",
        dpi=150,
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


plot_hospitalizations(
    "experiments/examples/parameter-sweep/parameter_sweep.csv",
    "settings_properties>>>Home>>>alpha",
    "settings_properties>>>School>>>alpha",
    "setting_properties>>>Workplace>>>alpha",
)
