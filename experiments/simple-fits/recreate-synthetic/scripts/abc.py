import argparse
import os
from math import log

import polars as pl
from abmwrappers import utils, wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import norm, poisson, uniform

# import seaborn as sns
# import matplotlib.pyplot as plt


def main(config_file: str, keep: bool):
    prior = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {
                "scale": uniform(0.0, 0.2),
            }
        },
        "proportion_asymptomatic": uniform(0.0, 1.0),
        "settings_properties": {
            "Home": {"alpha": uniform(0.0, 0.2)},
            "School": {"alpha": uniform(0.0, 0.2)},
        },
    }

    perturbation = {
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {
                "scale": norm(0.0, 0.01),
            }
        },
        "proportion_asymptomatic": norm(0.0, 0.05),
        "settings_properties": {
            "Home": {"alpha": norm(0.0, 0.01)},
            "School": {"alpha": norm(0.0, 0.01)},
        },
    }

    # Initialize experiment object
    experiment = Experiment(
        experiments_directory="experiments",
        config_file=config_file,
        prior_distribution_dict=prior,
        perturbation_kernel_dict=perturbation,
    )

    # Make the synthetic data for calibration testing
    synthetic_data_folder = os.path.join(experiment.data_path, "target")
    os.makedirs(synthetic_data_folder, exist_ok=True)
    cmd = utils.write_default_cmd(
        input_file=experiment.default_params_file,
        output_dir=synthetic_data_folder,
        exe_file=experiment.exe_file,
        model_type=experiment.model_type,
    )

    utils.run_model_command_line(cmd, model_type=experiment.model_type)
    experiment.target_data = output_processing_function(synthetic_data_folder)
    experiment.target_data.write_csv(
        os.path.join(synthetic_data_folder, "target_data.csv")
    )

    # sns.scatterplot(experiment.target_data, x = "t", y="count",hue="pediatric")
    # plt.show()
    # print(sdfg)
    if experiment.azure_batch:
        # Identifying file locations wihtin blob storage
        blob_experiment_directory = os.path.join(
            experiment.blob_container_name, experiment.sub_experiment_name
        )
        defaults = experiment.get_default_params()
        symptom_params_file = defaults["symptom_progression_library"][
            "EmpiricalFromFile"
        ]["file"]
        synth_pop_file = defaults["synth_population_file"]
        infectiousness_file = defaults["infectiousness_rate_fn"][
            "EmpiricalFromFile"
        ]["file"]
        experiment.changed_baseline_params = {
            "symptom_progression_library": {
                "EmpiricalFromFile": {
                    "file": f"/{blob_experiment_directory}/{os.path.basename(symptom_params_file)}"
                }
            },
            "synth_population_file": f"/{blob_experiment_directory}/{os.path.basename(synth_pop_file)}",
            "infectiousness_rate_fn": {
                "EmpiricalFromFile": {
                    "file": f"/{blob_experiment_directory}/{os.path.basename(infectiousness_file)}",
                }
            },
        }
        fps = [synth_pop_file, symptom_params_file, infectiousness_file]
        use_existing = False
    else:
        fps = []
        use_existing = False

    # Run experiment object
    wrappers.run_abcsmc(
        experiment=experiment,
        distance_fn=hosp_lhood,
        data_processing_fn=output_processing_function,
        files_to_upload=fps,
        use_existing_distances=use_existing,
        keep_all_sims=keep,
    )


def hosp_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    def poisson_lhood(model, data):
        # Probability of data value given the model value as expectation
        return -log(poisson.pmf(data, model + 1e-6) + 1e-12)

    # This doesn't work because of apply groups per key in abctools
    # The distance value is generated, but the empty data frame leads to an empty eval list
    if "t" not in results_data.columns or results_data.is_empty():
        joint_set = target_data.with_columns(
            pl.col("count")
            .map_elements(
                lambda x: poisson_lhood(0, x),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    else:
        joint_set = (
            results_data.select(pl.col(["t", "count", "pediatric"]))
            .rename({"count": "model_count"})
            .join(
                target_data.select(pl.col(["t", "count", "pediatric"])),
                on=["t", "pediatric"],
                how="right",
            )
            .with_columns(pl.col("model_count").fill_null(strategy="zero"))
        )
        joint_set = joint_set.with_columns(
            pl.struct(["model_count", "count"])
            .map_elements(
                lambda x: poisson_lhood(x["model_count"], x["count"]),
                return_dtype=pl.Float64,
            )
            .alias("negloglikelihood")
        )
    return joint_set.select(pl.col("negloglikelihood").sum()).item()


def output_processing_function(outputs_dir):
    fp = os.path.join(outputs_dir, "person_property_count.csv")
    default_df = pl.DataFrame({"t": 0, "pediatric": False, "count": 0})

    df = pl.read_csv(fp, raise_if_empty=False)

    if not df.is_empty():
        df = (
            df.with_columns((pl.col("age") < 18).alias("pediatric"))
            .filter(pl.col("hospitalized") == "true")
            .group_by("t", "pediatric")
            .agg(pl.sum("count"))
        )

    if df.is_empty():
        return default_df
    else:
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
argparser.add_argument(
    "-c",
    "--config-file",
    type=str,
    required=False,
    default="experiments/simple-fits/recreate-synthetic/input/config.yaml",
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
