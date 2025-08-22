import os
import time

import griddler
import polars as pl
from abmwrappers import wrappers
from abmwrappers import utils
from abmwrappers.experiment_class import Experiment
import json


def main():
    # Create the new Experiment and scenarios folder
    experiment = Experiment(
        experiments_directory="scripts/experiments",
        config_file="scripts/experiments/benchmarking/parameter-sweep/input/config.yaml",
    )

    wrappers.create_scenario_subexperiments(experiment)

    # Iterate over config files in the new scenarios directory
    # Create simulation data and store as parquet file in each scenario folder
    scenarios_dir = os.path.join(experiment.directory, "scenarios")
    parameters = []
    for scenario in os.listdir(scenarios_dir):
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )
        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        subexperiment.run_step(data_read_fn=read_fn, products=["simulations"])
        with open(experiment.griddle_file, "r") as fp:
            raw_griddle = json.load(fp)
        print(type(raw_griddle))
        print(raw_griddle["parameters"].keys())
        parameter_dict = {}
        for key in raw_griddle["parameters"].keys():
            print(subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key])
            flattened_dict = utils.flatten_dict(subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key])
            parameter_dict[key] = flattened_dict[key]
        parameter_dict["scenario"] = int(scenario.split("=")[-1])
        parameters.append(parameter_dict)
        # print(griddler.parse(raw_griddle))
        
        # for par_set in griddler.parse(raw_griddle)():
        #     print(par_set)

        # synth_pop = subexperiment.simulation_bundles[0].baseline_params[
        #     subexperiment.scenario_key
        # ]["synth_population_file"]

        # infectiousness_scale = subexperiment.simulation_bundles[
        #     0
        # ].baseline_params[subexperiment.scenario_key][
        #     "infectiousness_rate_fn"
        # ]["EmpiricalFromFile"]["scale"]

        # home_alpha = subexperiment.simulation_bundles[0].baseline_params[
        #     subexperiment.scenario_key
        # ]["settings_properties"]["Home"]["alpha"]

        # workplace_alpha = subexperiment.simulation_bundles[0].baseline_params[
        #     subexperiment.scenario_key
        # ]["settings_properties"]["Workplace"]["alpha"]

        # school_alpha = subexperiment.simulation_bundles[0].baseline_params[
        #     subexperiment.scenario_key
        # ]["settings_properties"]["School"]["alpha"]

        # censustract_alpha = subexperiment.simulation_bundles[
        #     0
        # ].baseline_params[subexperiment.scenario_key]["settings_properties"][
        #     "CensusTract"
        # ]["alpha"]

        # average_times.append(
        #     {
        #         "scenario": int(scenario.split("=")[-1]),
        #         "average_time": average_time,
        #         # "pop_size": pop_size,
        #         # "infectiousness_scale": infectiousness_scale,
        #         # "home_alpha": home_alpha,
        #         # "workplace_alpha": workplace_alpha,
        #         # "school_alpha": school_alpha,
        #         "censustract_alpha": censustract_alpha,
        #     }
        # )

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
        os.path.join(experiment.directory, "experiment_runtime.csv")
    )


def read_fn(outputs_dir):
    profiling_file_path = os.path.join(outputs_dir, "profiling_data.json")
    with open(profiling_file_path, "r") as f:
        profiling_data = json.load(f)
    pop_size = profiling_data["execution_statistics"]["population"]
    memory = profiling_data["execution_statistics"]["max_memory_usage"]
    cpu_time = profiling_data["execution_statistics"]["cpu_time"]
    wall_time = profiling_data["execution_statistics"]["wall_time"]
    
    attack_rate = profiling_data["named_counts"][3]["count"]/pop_size
    prop_prog = profiling_data["named_counts"][0]["count"]
    forcasted = profiling_data["named_counts"][4]["count"]
    accepted = profiling_data["named_counts"][1]["count"]

    data = {
        "pop_size": pop_size,
        "memory": memory,
        "cpu_time": cpu_time,
        "wall_time": wall_time,
        "attack_rate": attack_rate,
        "property_progressions": prop_prog,
        "forcasted_infections": forcasted,
        "accepted_forecasts": accepted,
    }
    df = pl.DataFrame([data])
    return df

main()
