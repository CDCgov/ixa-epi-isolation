import argparse
import os
import subprocess

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment


def main(config_file):
    rate_fns_path = "tests/data/rate_fns_exp_I.csv"
    if not os.path.exists(rate_fns_path):
        # These should create ALL of the rates, populations, and ODE output, not just the rate_fns_SIR.csv
        subprocess.Popen("Rscript scripts/create_integration_test_rate_fns.R".split(), stdout=subprocess.PIPE)
        subprocess.Popen("Rscript scripts/create_integration_test_pops.R".split(), stdout=subprocess.PIPE)

        if False:
            raise NotImplementedError("")
            input = experiment.default_params_file
            # In many cases - params from ODE have to hava fxnl relationship with the base JSON file
            subprocess.run(f"Rscript scripts/create_ode_outputs.R {input}".split())

    experiment = Experiment(
        experiments_directory="tests",
        config_file=config_file,
        changed_baseline_params={
            "infectiousness_rate_fn": {
                "EmpiricalFromFile": {
                    "file": rate_fns_path
                }
            }
        }
    )
    simulation_df = wrappers.create_simulation_data(
        experiment=experiment, 
        data_processing_fn=return_infection_count
    )

    print(simulation_df)

def return_infection_count(directory: str):
    file_path = os.path.join(directory, "person_property_count.csv")
    if os.path.exists(file_path):
        df = pl.read_csv(file_path)
    else:
        raise FileNotFoundError(f"Expected file not found: {file_path}")

    df = df.group_by(["t", "InfectionStatus"]).agg(pl.col("count").sum())

    return df

def return_raw_count(directory: str):
    file_path = os.path.join(directory, "person_property_count.csv")
    if os.path.exists(file_path):
        df = pl.read_csv(file_path)
    else:
        raise FileNotFoundError(f"Expected file not found: {file_path}")

    return df


parser = argparse.ArgumentParser()
parser.add_argument(
    "-f",
    "--config-file",
    type=str,
    required=True,
    help="Path to the configuration file.",
)
args = parser.parse_args()
main(config_file=args.config_file)
