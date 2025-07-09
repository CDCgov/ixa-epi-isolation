import json
import os
import time

import polars as pl
import yaml
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment


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
        with open(config_path, "r") as f:
            experiment_dict = yaml.safe_load(f)

        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        start_time = time.time()
        wrappers.run_step_return_data(
            experiment=subexperiment, data_preprocessing_fn=read_fn
        )
        end_time = time.time()
        average_time = (end_time - start_time) / experiment_dict[
            "experiment_conditions"
        ]["replicates_per_particle"]

        sim_input = os.path.join(scenarios_dir, scenario, "input", "base.json")
        with open(sim_input, "r") as f:
            sim_data = json.load(f)
        pop_size = pl.read_csv(
            sim_data[experiment_dict["experiment_conditions"]["scenario_key"]][
                "synth_population_file"
            ]
        ).height
        average_times.append(
            {
                "scenario": int(scenario.split("=")[-1]),
                "average_time": average_time,
                "pop_size": pop_size,
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
    experiment.save(
        os.path.join(experiment.directory, "data", "experiment_history.pkl")
    )
    exp_output = experiment.parquet_from_path(
        os.path.join(experiment.directory, "data", "scenarios")
    )
    exp_output = exp_output.join(
        pl.DataFrame(average_times), on="scenario", how="left"
    )
    print(exp_output)


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


main()
