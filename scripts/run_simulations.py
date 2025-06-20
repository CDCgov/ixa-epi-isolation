import argparse
import os

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment


def main(config_file):
    experiment = Experiment(
        experiments_directory="experiments",
        config_file=config_file,
    )
    simulation_df = wrappers.create_simulation_data(
        experiment=experiment, 
        data_processing_fn=return_count_only
    )

    simulation_df.filter((pl.col("t") > 5) & (pl.col("t") < 10)).write_csv(
        os.path.join(experiment.data_path, "filtered_results.csv")
    )


def return_count_only(directory: str):
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
