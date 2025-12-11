import os

import polars as pl
from abmwrappers import experiment_class, wrappers
from abmwrappers.experiment_class import Experiment


def main(scenario_replicates: int = 50):
    # Create the new Experiment and scenarios folder
    # Move the pickle file produced during calibration out of the data folder
    # the pickle file produced from the scenarios will live in the data folder
    experiment = experiment_class.Experiment(
        img_file="experiments/simple-fits/wyoming-incidence/experiment_history.pkl",
        azure_batch=False,
        griddle_file="experiments/simple-fits/wyoming-incidence/input/griddle.json",
        replicates=1,
    )
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

    wrappers.create_scenario_subexperiments(
        experiment=experiment,
        sample_posterior=True,
        n_samples=scenario_replicates,
    )

    # Iterate over config files in the new scenarios directory
    # Create simulation data and store as parquet file in each scenario folder
    scenarios_dir = os.path.join(experiment.directory, "scenarios")
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

        wrappers.write_scenario_products(
            scenario=scenario,
            scenario_experiment=subexperiment,
            experiment_data_path=experiment.data_path,
            clean=False,
        )
        experiment.simulation_bundles.update(
            {scenario: subexperiment.simulation_bundles[0]}
        )
    experiment.save()


def read_hospitalizations_fn(outputs_dir):
    data_path = os.path.join(outputs_dir, "incidence_report.csv")
    data = pl.read_csv(data_path)
    data = data.filter(pl.col("event") == "Hospitalized")
    data = data.group_by("t_upper").agg(pl.col("count").sum().alias("count"))
    return data


main()
