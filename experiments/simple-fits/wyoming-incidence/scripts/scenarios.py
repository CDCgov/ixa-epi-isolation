import os
import pickle
import subprocess
import polars as pl
import yaml
from abctools import abc_classes, abc_methods
from abmwrappers import utils, experiment_class, wrappers
from cfa_azure.clients import AzureClient
from scipy.stats import norm, uniform
import numpy as np
import json

import polars as pl
from abmwrappers import utils, wrappers
from abmwrappers.experiment_class import Experiment


def main():
    # Create the new Experiment and scenarios folder
    experiment = experiment_class.Experiment(
        img_file="experiments/simple-fits/wyoming-incidence/experiment_history.pkl",
        azure_batch = False,
        griddle_file = "experiments/simple-fits/wyoming-incidence/input/griddle.json",
        replicates = 1
    )
    print(experiment.azure_batch)
    local_directory = "input"
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
                "file": f"{local_directory}/{os.path.basename(symptom_params_file)}"
            }
        },
        "infectiousness_rate_fn": {
            "EmpiricalFromFile": {
                "file": f"{local_directory}/{os.path.basename(infectiousness_rate_file)}"
            }
        },
        "synth_population_file": f"{local_directory}/{os.path.basename(synth_pop_file)}",
    }
    
    wrappers.create_scenario_subexperiments(experiment)

    # Iterate over config files in the new scenarios directory
    # Create simulation data and store as parquet file in each scenario folder
    scenarios_dir = os.path.join(experiment.directory, "scenarios")
    parameters = []
    for scenario in os.listdir(scenarios_dir):
        print(f"Running scenario: {scenario}")
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )
        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        subexperiment.run_step(
            data_read_fn=read_hospitalizations_fn, products=["simulations"]
        )
        with open(experiment.griddle_file, "r") as fp:
            raw_griddle = json.load(fp)

        parameter_dict = {}
        for key in raw_griddle["parameters"].keys():
            flattened_dict = utils.flatten_dict(
                subexperiment.simulation_bundles[0].baseline_params[
                    subexperiment.scenario_key
                ]
            )
            for k in flattened_dict.keys():
                if "UpdatedIsolationGuidance" in k:
                    adherence = flattened_dict["guidance_policy>>>UpdatedIsolationGuidance>>>policy_adherence"]
                    if adherence == 0:
                        parameter_dict["Guidance"] = "No Guidance"
                        parameter_dict["Policy Adherence"] = adherence
                    else:
                        parameter_dict["Guidance"] = "Updated"
                        parameter_dict["Policy Adherence"] = adherence
                    break
                elif "PreviousIsolationGuidance" in k:
                    adherence = flattened_dict["guidance_policy>>>PreviousIsolationGuidance>>>policy_adherence"]
                    parameter_dict["Guidance"] = "Previous"
                    parameter_dict["Policy Adherence"] = adherence
                    break 
        parameter_dict["scenario"] = int(scenario.split("=")[-1])
        print(subexperiment.simulation_bundles[0].baseline_params[
                    subexperiment.scenario_key
                ])
        parameter_dict["burn_in_period"] = subexperiment.simulation_bundles[0].baseline_params[
                    subexperiment.scenario_key
                ]["burn_in_period"]
        parameters.append(parameter_dict)
        experiment.simulation_bundles.update(
            {scenario: subexperiment.simulation_bundles[0]}
        )
        wrappers.write_scenario_products(
            scenario=scenario,
            scenario_experiment=subexperiment,
            experiment_data_path=experiment.data_path,
            clean=False,
        )
    experiment.save()
    parameter_df = pl.DataFrame(parameters)
    exp_output = experiment.read_results(filename="scenarios")
    exp_output = exp_output.join(parameter_df, on="scenario")
    exp_output.write_csv(
        os.path.join(experiment.directory, "hospitalizations_wy.csv")
    )


def read_hospitalizations_fn(outputs_dir):
    data_path = os.path.join(outputs_dir, "incidence_report.csv")
    data = pl.read_csv(data_path)
    data = data.filter(pl.col("event") == "Hospitalized")
    data = data.group_by("t_upper").agg(pl.col("count").sum().alias("count"))
    return data

main()