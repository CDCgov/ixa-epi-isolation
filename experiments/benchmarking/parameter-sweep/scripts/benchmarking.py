import json
import os

import polars as pl
from abmwrappers import utils, wrappers
from abmwrappers.experiment_class import Experiment


def main():
    # Create the new Experiment and scenarios folder
    experiment = Experiment(
        experiments_directory="experiments",
        config_file="experiments/benchmarking/parameter-sweep/input/config.yaml",
    )

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
        subexperiment.run_step(data_read_fn=read_fn, products=["simulations"])
        with open(experiment.griddle_file, "r") as fp:
            raw_griddle = json.load(fp)

        parameter_dict = {}
        for key in raw_griddle["parameters"].keys():
            flattened_dict = utils.flatten_dict(
                subexperiment.simulation_bundles[0].baseline_params[
                    subexperiment.scenario_key
                ]
            )
            parameter_dict[key] = flattened_dict[key]
        parameter_dict["scenario"] = int(scenario.split("=")[-1])
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
        os.path.join(experiment.directory, "experiment_runtime.csv")
    )


def read_fn(outputs_dir):
    profiling_file_path = os.path.join(outputs_dir, "profiling_data.json")
    with open(profiling_file_path, "r") as f:
        profiling_data = json.load(f)
    data = {
        "pop_size": profiling_data["execution_statistics"]["population"],
        "memory": profiling_data["execution_statistics"]["max_memory_usage"],
        "cpu_time": profiling_data["execution_statistics"]["cpu_time"],
        "wall_time": profiling_data["execution_statistics"]["wall_time"],
        "attack_rate": profiling_data["named_counts"][3]["count"]
        / profiling_data["execution_statistics"]["population"],
        "property_progressions": profiling_data["named_counts"][0]["count"],
        "forcasted_infections": profiling_data["named_counts"][4]["count"],
        "accepted_forecasts": profiling_data["named_counts"][1]["count"],
        "load_synth_pop": profiling_data["named_spans"][0]["duration"],
    }
    return pl.DataFrame([data])


main()
