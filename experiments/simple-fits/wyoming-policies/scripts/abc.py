import argparse
import os
from math import log

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import beta, uniform, norm, poisson


def main(config_file: str, keep: bool):
    prior = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {
                "scale": uniform(0.0, 0.2)
            }
        },
        "proportion_asymptomatic": beta(45, 55),
        "settings_properties": {
            "Home": {"alpha": uniform(0.0, 0.2)},
            "School": {"alpha": uniform(0.0, 0.2)},
            "Workplace": {"alpha": uniform(0.0, 0.2)},
        },
        "initial_recovered": beta(5, 15),
    }

    perturbation = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {
                "scale": norm(0.0, 0.01)
            }
        },
        "proportion_asymptomatic": norm(0.0, 0.02),
        "settings_properties": {
            "Home": {"alpha": norm(0.0, 0.01)},
            "School": {"alpha": norm(0.0, 0.01)},
            "Workplace": {"alpha": norm(0.0, 0.01)},
        },
        "initial_recovered": norm(0.0, 0.02),
    }

    # Initialize experiment object
    experiment = Experiment(
        experiments_directory="experiments",
        config_file=config_file,
        prior_distribution_dict=prior,
        perturbation_kernel_dict=perturbation,
    )

    if experiment.azure_batch:
        # Identifying file locations wihtin blob storage
        blob_experiment_directory = os.path.join(
            experiment.blob_container_name, experiment.sub_experiment_name
        )
        defaults = experiment.get_default_params()
        symptom_params_file = defaults["symptom_progression_library"][
            "EmpiricalFromFile"
        ]["file"]
        infectiousness_rate_file = defaults["infectiousness_rate_fn"][
            "EmpiricalFromFile"
        ]["file"]
        synth_pop_file = defaults["synth_population_file"]
        experiment.changed_baseline_params = {
            "symptom_progression_library": {
                "EmpiricalFromFile": {
                    "file": f"/{blob_experiment_directory}/{os.path.basename(symptom_params_file)}"
                }
            },
            "infectiousness_rate_fn": {
                "EmpiricalFromFile": {
                    "file": f"/{blob_experiment_directory}/{os.path.basename(infectiousness_rate_file)}"
                }
            },
            "synth_population_file": f"/{blob_experiment_directory}/{os.path.basename(synth_pop_file)}",
        }
        fps = [synth_pop_file, symptom_params_file, infectiousness_rate_file]
        use_existing = False
    else:
        fps = []
        use_existing = False

    # Run experiment object
    wrappers.run_abcsmc(
        experiment=experiment,
        distance_fn=hosp_lhood,
        data_read_fn=output_processing_function,
        files_to_upload=fps,
        use_existing_distances=use_existing,
        keep_all_sims = True
    )


def hosp_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    def poisson_lhood(model, data):
        return -log(poisson.pmf(model, data) + 1e-12)

    if "t" not in results_data.columns:
        joint_set = target_data.with_columns(
            pl.col("total_admissions")
            .map_elements(
                lambda x: poisson_lhood(0, x),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    else:
        # max_t_target = target_data.select(pl.col(["t", "total_admissions"])).sort("total_admissions", descending=True).select("t").to_series()[0]
        # max_t_result = results_data.select(pl.col(["t", "count"])).sort("count", descending=True).select("t").to_series()[0]
        # difference = max_t_result - max_t_target
        # results_data = results_data.with_columns(
        #     (pl.col("t") - difference).alias("t")
        # )
        joint_set = (
            results_data.select(pl.col(["t", "count"]))
            .join(
                target_data.select(pl.col(["t", "total_admissions"])),
                on="t",
                how="right",
            )
            .with_columns(pl.col("count").fill_null(strategy="zero"))
        )

        joint_set = joint_set.with_columns(
            pl.struct(["count", "total_admissions"])
            .map_elements(
                lambda x: poisson_lhood(x["count"], x["total_admissions"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    return joint_set.select(pl.col("negloglikelihood").sum()).item()


def output_processing_function(outputs_dir):
    fp = os.path.join(outputs_dir, "incidence_report.csv")

    try:
        df = pl.read_csv(fp)

        df = (
            df
            .filter(pl.col("event") == "Hospitalized")
            .group_by("t_upper")
            .agg(pl.sum("count"))
            .with_columns(pl.col("t_upper").cast(pl.Int64).alias("t"))
        )

        return df
    except:
        return pl.DataFrame()


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
        products=["distances", "simulations"],
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
argparser.add_argument(
    "-c",
    "--config-file",
    type=str,
    required=False,
    default="experiments/simple-fits/wyoming-policies/input/config.yaml",
)
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
