import argparse
import os
from math import log

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import gamma, norm, poisson


def main(config_file: str, keep: bool):
    # Misspecified prior for scale that should be 1.0
    prior = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {"scale": gamma(a=1, scale=0.5)}
        }
    }

    perturbation = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {"scale": norm(0, 0.05)}
        }
    }

    # Initialize experiment object
    experiment = Experiment(
        experiments_directory="experiments",
        config_file=config_file,
        prior_distribution_dict=prior,
        perturbation_kernel_dict=perturbation,
    )

    # Run experiment object
    wrappers.run_abcsmc(
        experiment=experiment,
        distance_fn=ode_lhood,
        data_read_fn=output_processing_function,
        keep_all_sims=keep,
    )


def ode_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    def poisson_lhood(model, data):
        return -log(poisson.pmf(model, data))

    # upper precision bound for neg log, P(results) = 0
    if results_data.is_empty():
        return 750.0
    else:
        min_t_target = target_data.select(pl.col("t").min())

        target_data = (
            target_data.filter(pl.col("InfectionStatus") == "Infectious")
            .with_columns(pl.col("t") - min_t_target.item())
            .rename({"count": "target_count"})
        )

        joint_set = results_data.select(pl.col(["t", "result_count"])).join(
            target_data.select(pl.col(["t", "target_count"])), on="t"
        )

        joint_set = joint_set.with_columns(
            pl.struct(["result_count", "target_count"])
            .map_elements(
                lambda x: poisson_lhood(x["result_count"], x["target_count"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )

        return joint_set.select(pl.col("negloglikelihood").sum()).item()

def output_processing_function(outputdir: str) -> pl.DataFrame:
    """
    Read the output files and return a DataFrame with the results.
    """
    # Read the simulation results from the output directory
    sim_results = pl.read_csv(os.path.join(outputdir, "person_property_count.csv"))

    # Process the results to get the required format
    processed_results = (
        sim_results.filter(pl.col("InfectionStatus") == "Infectious")
        .group_by("t")
        .agg(pl.col("count").sum().alias("result_count"))
    )
    return processed_results

argparser = argparse.ArgumentParser()
argparser.add_argument("-x", "--execute", type=str, default="main")
argparser.add_argument("-c", "--config-file", type=str, required=False)
argparser.add_argument("-i", "--img-file", type=str, required=False)
argparser.add_argument(
    "-d",
    "--products-path",
    type=str,
    required=False,
    help="Output directory for products. Typically the data path of an experiment.",
)

argparser.add_argument(
    "--index",
    type=int,
    help="Simulation index to be called for writing and returning products",
)
argparser.add_argument(
    "--products",
    nargs="*",
    help="List of products to process (distances, simulations)",
    required=False,
)
argparser.add_argument(
    "--clean",
    action="store_true",
    help="Clean up raw output files after processing into products",
    required=False,
)
argparser.add_argument(
    "--keep",
    action="store_true",
    help="Keep all the simulation parquet parts from results",
    required=False,
)

args = argparser.parse_args()
if args.execute == "main":
    main(config_file=args.config_file, keep=args.keep)
