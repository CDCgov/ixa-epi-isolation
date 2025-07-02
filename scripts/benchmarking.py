from abmwrappers import  wrappers
from abmwrappers.experiment_class import Experiment
import os
import time
import polars as pl 
def main():
    # Create the new Experiment and scenarios folder
    experiment = Experiment(
        experiments_directory="scripts/experiments",
        config_file="scripts/experiments/benchmarking/jul25-assessment/input/config.yaml",
    )

    wrappers.create_scenario_subexperiments(
        experiment
    )

    # Iterate over config files in the new scenarios directory
    # Create simulation data and store as parquet file in each scenario folder
    scenarios_dir = os.path.join(experiment.directory, "scenarios")

    for scenario in os.listdir(scenarios_dir):
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )

        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )

        wrappers.create_simulation_data(
            experiment=subexperiment,
            data_processing_fn=read_fn
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

def read_fn(outputs_dir):
    output_file_path = os.path.join(outputs_dir, "person_property_count.csv")
    if os.path.exists(output_file_path):
        df = pl.read_csv(output_file_path)
    else:
        raise FileNotFoundError(f"{output_file_path} does not exist.")

    # Select day and person count columns for ascertained cases
    df = (
        df.group_by(["t", "InfectionStatus"])
        .agg(pl.col("count").sum())
        .filter(
            (pl.col("InfectionStatus") == pl.lit("Recovered"))
            & (pl.col("InfectionStatus") == pl.lit("Symptomatic"))
        )
    )

    return df

main()