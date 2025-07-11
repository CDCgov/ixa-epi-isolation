import os
import time

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
import matplotlib.pyplot as plt


def main():
    # Create the new Experiment and scenarios folder
    experiment = Experiment(
        experiments_directory="scripts/experiments",
        config_file="scripts/experiments/benchmarking/jul25-assessment/input/config.yaml",
    )

    wrappers.create_scenario_subexperiments(experiment)

    # Iterate over config files in the new scenarios directory
    # Create simulation data and store as parquet file in each scenario folder
    scenarios_dir = os.path.join(experiment.directory, "scenarios")
    average_times = []
    for scenario in os.listdir(scenarios_dir):
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )

        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        start_time = time.time()
        subexperiment.run_step(
            data_processing_fn=read_fn, products=["simulations"]
        )
        end_time = time.time()
        average_time = (end_time - start_time) / subexperiment.replicates

        synth_pop = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["synth_population_file"]
        isolation_probability = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["intervention_policy_parameters"]["isolation_probability"]
        pop_size = pl.read_csv(synth_pop).height
        average_times.append(
            {
                "scenario": int(scenario.split("=")[-1]),
                "average_time": average_time,
                "pop_size": pop_size,
                "isolation_probability": isolation_probability,
            }
        )

        experiment.simulation_bundles.update(
            {scenario: subexperiment.simulation_bundles[0]}
        )
        wrappers.write_scenario_products(
            scenario=scenario,
            scenario_experiment=subexperiment,
            experiment_data_path=experiment.data_path,
            clean=False,
        )
    experiment.save()
    exp_output = experiment.read_results(filename="scenarios")
    exp_output = exp_output.join(
        pl.DataFrame(average_times), on="scenario", how="left"
    )
    exp_output.write_csv(os.path.join(experiment.directory, "experiment_runtime.csv"))

    plot_runtime(exp_output, experiment)


def read_fn(outputs_dir):
    output_file_path = os.path.join(outputs_dir, "person_property_count.csv")
    if os.path.exists(output_file_path):
        df = pl.read_csv(output_file_path)
    else:
        raise FileNotFoundError(f"{output_file_path} does not exist.")
    max_t = df["t"].max()
    df = (
        (
            df.group_by(["t", "InfectionStatus"])
            .agg(pl.col("count").sum())
            .filter((pl.col("InfectionStatus") == pl.lit("Recovered")))
        )
        .filter(pl.col("t") == max_t)
        .select(pl.col("count").alias("attack_rate"))
    )
    print(df.to_series().to_list()[0])
    return df

def plot_runtime(exp_output, experiment):
    unique_exp_output = exp_output.unique(subset=["pop_size", "isolation_probability"])
    plt.figure()
    for iso_prob in unique_exp_output["isolation_probability"].unique():
        subset = unique_exp_output.filter(pl.col("isolation_probability") == iso_prob)
        plt.plot(
            subset["pop_size"],
            subset["average_time"],
            marker="o",
            label=f"Isolation Probability: {iso_prob}"
        )
    plt.legend(title="Isolation Probability")
    plt.xscale("log")
    plt.xlabel("Population Size")
    plt.ylabel("Average Runtime")
    plt.title("Population Size vs Average Runtime")
    plt.grid(True)
    plt.tight_layout()
    plt.savefig(os.path.join(experiment.directory, "popsize_vs_avgtime.png"))
    plt.close()

main()
