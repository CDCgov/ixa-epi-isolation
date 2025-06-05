import argparse
import os
from math import log

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import norm, poisson, uniform


def main(config_file: str):
    experiment_params_prior_dist = {
        "settings_properties>>>Home>>>alpha": uniform(0.0, 1.0),
    }
    perturbation_kernels = {
        "settings_properties>>>Home>>>alpha": norm(0, 0.015),
    }

    experiment = Experiment(
        experiments_directory="experiments", config_file=config_file
    )
    wrappers.split_scenarios_into_subexperiments(experiment, scenario_key="epi_isolation.GlobalParams")

    scenarios_dir = os.path.join(experiment.directory, "scenarios")
    for scenario in os.listdir(scenarios_dir):
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )
        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        subexperiment.initialize_simbundle()

        _simulation_data_frame = wrappers.create_simulation_data(
            experiment = subexperiment, 
            data_processing_fn = data_processing_fn
        )
        experiment.simulation_bundles.update({scenario: subexperiment.simulation_bundles[0]})

        wrappers.write_scenario_products_to_data(        
            scenario=scenario,
            scenario_experiment=subexperiment,
            experiment_data_path=experiment.data_path,
            clean=True,
        )

# ----
# Distance function section
# ----


def poisson_lhood(model, data):
    return -log(poisson.pmf(data, model + 0.001))


def distance_pois_lhood(results_data: pl.DataFrame, target_data: pl.DataFrame):
    if results_data.is_empty():
        return 750.0
    else:
        min_t_target = target_data.select(pl.col("t").min())

        target_data = target_data.with_columns(
            pl.col("t") - min_t_target.item()
        ).rename({"count": "target_count"})

        min_t_results = results_data.select(pl.col("t").min())
        results_data = results_data.with_columns(
            pl.col("t") - min_t_results.item()
        ).rename({"count": "result_count"})

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


# ----
# Clean up data
# ----


def data_processing_fn(directory: str):
    file_path = os.path.join(directory, "person_property_count.csv")
    if os.path.exists(file_path):
        df = pl.read_csv(file_path)
    else:
        raise FileNotFoundError(f"Expected file not found: {file_path}")

    return df


parser = argparse.ArgumentParser()
parser.add_argument(
    "-i",
    "--input-file",
    type=str,
    required=True,
    help="Path to the input configuration file.",
)
args = parser.parse_args()
main(config_file=args.input_file)
