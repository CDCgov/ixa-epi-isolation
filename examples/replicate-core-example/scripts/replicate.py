import argparse

import matplotlib.pyplot as plt
import polars as pl
import seaborn as sns
from abmwrappers.experiment_class import Experiment


def main(config_file, verbose):
    # Instantiate the experiment object
    experiment = Experiment(
        experiments_directory="", config_file=config_file, verbose=verbose
    )

    # Run the step included in the config file (Experiment knows to provide random seeds for a single particle with no SMC commands)
    experiment.run_step(data_filename="person_property_count.csv")

    # Read resutls to a temporary data frame for plotting
    results = experiment.read_results(
        "person_property_count.csv", data_read_fn=infection_status_over_time
    )

    # Plot the results
    if experiment.verbose:
        print("Making plot of infection compartment class counts over time.")
    sns.lineplot(
        results,
        x="t",
        y="count",
        units="simulation",
        estimator=None,
        hue="infection_status",
    )
    plt.show()


def infection_status_over_time(input_df: pl.DataFrame) -> pl.DataFrame:
    out = input_df.group_by(["t", "infection_status"]).agg(pl.sum("count"))
    return out


parser = argparse.ArgumentParser()
parser.add_argument(
    "-c",
    "--config-file",
    type=str,
    default="examples/replicate-core-example/input/config.yaml",
    help="Path to the configuration file for instantiating the experiment class object",
)
parser.add_argument(
    "-v",
    "--verbose",
    action="store_true",
    help="Verbose argument for experiment class object",
)
args = parser.parse_args()

main(config_file=args.config_file, verbose=args.verbose)
