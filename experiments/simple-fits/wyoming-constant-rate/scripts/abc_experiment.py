import argparse
import os
from math import log

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import gamma, norm, poisson, beta


def main(config_file: str, keep: bool):
    # Misspecified prior for scale that should be 1.0
    prior = {
        "infectiousness_rate_fn": {
            "Constant": {
                "rate": gamma(a=1, scale=0.4),
                "duration": gamma(a=10, scale=0.5),
            }
        },
        "proportion_asymptomatic": beta(3, 3)
    }

    perturbation = {
        "infectiousness_rate_fn": {
            "Constant": {"rate": norm(0, 0.01), "duration": norm(0, 0.05)}
        },
        "proportion_asymptomatic": norm(0, 0.01)
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
        distance_fn=hosp_lhood,
        data_read_fn=output_processing_function,
    )


def hosp_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    def poisson_lhood(model, data):
        return -log(poisson.pmf(model, data)+1e-12)

    # upper precision bound for neg log, P(results) = 0
    if results_data.is_empty():
        return 750.0
    else:
        print(results_data.sort("t"))
        joint_set = results_data.select(pl.col(["t", "count"])).join(
            target_data.select(pl.col(["t", "total_admissions"])),
            on="t",
            how="right",
        ).with_columns(
            pl.col("count").fill_null(strategy="zero")
        )

        joint_set = joint_set.with_columns(
            pl.struct(["count", "total_admissions"])
            .map_elements(
                lambda x: poisson_lhood(x["count"], x["total_admissions"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
        print(joint_set.sort("t"))
        return joint_set.select(pl.col("negloglikelihood").sum()).item()


def output_processing_function(outputs_dir):
    fp = os.path.join(outputs_dir, "hospital_incidence_report.csv")

    if os.path.exists(fp):
        df = pl.read_csv(fp)
    else:
        raise FileNotFoundError(f"{fp} does not exist.")

    df = (
        df.with_columns((pl.col("time") / 7.0).ceil().cast(pl.Int64).alias("week"))
        .group_by("week")
        .agg(pl.len().alias("count"))
        .with_columns((pl.col("week") * 7 - 1).alias("t"))
    )

    return df

def task(
    simulation_index: int,
    img_file: str,
    clean: bool = False,
    products_path: str = None,
    products: list = None,
):
    experiment = Experiment(img_file=img_file)
    experiment.run_index(
        simulation_index=simulation_index,
        distance_fn=hosp_lhood,
        data_read_fn=output_processing_function,
        products=products,
        products_output_dir=products_path,
        clean=clean,
    )

def gather(
    img_file: str,
    products_path: str,
):
    wrappers.update_abcsmc_img(img_file, products_path)


argparser = argparse.ArgumentParser()
argparser.add_argument("-x", "--execute", type=str, default="main")
argparser.add_argument("-c", "--config-file", type=str, required=False, default="experiments/simple-fits/wyoming-constant-rate/input/config.yaml")
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
elif args.execute == "gather":
    gather(img_file=args.img_file, products_path=args.products_path)
elif args.execute == "run":
    task(
        simulation_index=args.index,
        img_file=args.img_file,
        clean=args.clean,
        products_path=args.products_path,
        products=args.products,
    )
