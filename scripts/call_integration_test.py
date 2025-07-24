import argparse
import os
import subprocess

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment


def main(config_file, verbose):
    rate_fns_path = "tests/input/rate_fns_exp_I.csv"
    if not os.path.exists(rate_fns_path):
        # These should create ALL of the rates, populations, and ODE output, not just the rate_fns_SIR.csv
        subprocess.run(
            "Rscript scripts/create_integration_test_rate_fns.R".split()
        )
        subprocess.run(
            "Rscript scripts/create_integration_test_pops.R".split()
        )
        subprocess.run("Rscript scripts/create_ode_output.R".split())

    experiment = Experiment(
        experiments_directory="tests", config_file=config_file
    )
    experiment.run_step(
        data_filename="person_property_count.csv"
    )

parser = argparse.ArgumentParser()
parser.add_argument(
    "-f",
    "--config-file",
    type=str,
    required=True,
    help="Path to the configuration file.",
)
parser.add_argument("-v", "--verbose", action="store_true")
args = parser.parse_args()
main(config_file=args.config_file, verbose=args.verbose)
