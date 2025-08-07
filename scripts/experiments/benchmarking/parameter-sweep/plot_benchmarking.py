import os
import matplotlib.pyplot as plt
import polars as pl
def plot_runtime(exp_output):
    df = pl.read_csv(exp_output)
    unique_exp_output = df.filter(pl.col("pop_size").is_not_null() & pl.col("infectiousness_scale").is_not_null())
    print(unique_exp_output)
    plt.figure()
    for scale in unique_exp_output["infectiousness_scale"].unique():
        subset = unique_exp_output.filter(pl.col("infectiousness_scale") == scale)
        plt.scatter(
            subset["pop_size"],
            subset["average_time"],
            marker="o",
            label=f"Infectiousness Scale: {scale}"
        )
    plt.legend(title="Infectiousness Scale")
    plt.xscale("log")
    plt.yscale("log")
    plt.xlabel("Population Size")
    plt.ylabel("Average Runtime")
    plt.title("Population Size vs Average Runtime")
    plt.grid(True)
    plt.tight_layout()
    plt.savefig(os.path.join(os.path.dirname(exp_output), "popsize_vs_avgtime.png"))
    plt.close()

plot_runtime("scripts/experiments/benchmarking/parameter-sweep/experiment_runtime.csv")