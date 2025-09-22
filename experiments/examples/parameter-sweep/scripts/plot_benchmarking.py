import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns

sns.set_theme(style="whitegrid")


def plot_hospitalizations(output_path):
    scenario_data = pd.read_csv(output_path)

    # Get unique values for rows and columns
    proportion_asymptomatic_vals = sorted(
        scenario_data["proportion_asymptomatic"].unique()
    )
    home_alpha_vals = sorted(
        scenario_data["settings_properties>>>Home>>>alpha"].unique()
    )

    fig, axes = plt.subplots(
        nrows=len(proportion_asymptomatic_vals),
        ncols=len(home_alpha_vals),
        figsize=(
            6 * len(home_alpha_vals),
            4 * len(proportion_asymptomatic_vals),
        ),
        sharex=True,
        sharey=True,
    )

    # If axes is 1D, make it 2D for consistency
    if len(proportion_asymptomatic_vals) == 1:
        axes = axes[np.newaxis, :]
    if len(home_alpha_vals) == 1:
        axes = axes[:, np.newaxis]

    for i, proportion_asymptomatic in enumerate(proportion_asymptomatic_vals):
        for j, home_alpha in enumerate(home_alpha_vals):
            ax = axes[i, j]
            subset = scenario_data[
                (
                    scenario_data["proportion_asymptomatic"]
                    == proportion_asymptomatic
                )
                & (
                    scenario_data["settings_properties>>>Home>>>alpha"]
                    == home_alpha
                )
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
            ax.set_title(
                f"proportion_asymptomatic={proportion_asymptomatic}, home_alpha={home_alpha}"
            )

    plt.tight_layout()

    plt.savefig(
        "experiments/examples/parameter-sweep/trajectories_mean_grid_asmpy_alpha.png",
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


plot_hospitalizations(
    "experiments/examples/parameter-sweep/hospitalizations_asmp_home.csv"
)
