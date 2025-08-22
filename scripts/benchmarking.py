import os
import time

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment


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
    average_times = []
    for scenario in os.listdir(scenarios_dir):
        config_path = os.path.join(
            scenarios_dir, scenario, "input", "config.yaml"
        )

        subexperiment = Experiment(
            experiments_directory=experiment.directory,
            config_file=config_path,
        )
        start_time = time.time()
        subexperiment.run_step(data_read_fn=read_fn, products=["simulations"])
        end_time = time.time()
        average_time = (end_time - start_time) / subexperiment.replicates

        synth_pop = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["synth_population_file"]

        infectiousness_scale = subexperiment.simulation_bundles[
            0
        ].baseline_params[subexperiment.scenario_key][
            "infectiousness_rate_fn"
        ]["EmpiricalFromFile"]["scale"]

        home_alpha = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["settings_properties"]["Home"]["alpha"]

        workplace_alpha = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["settings_properties"]["Workplace"]["alpha"]

        school_alpha = subexperiment.simulation_bundles[0].baseline_params[
            subexperiment.scenario_key
        ]["settings_properties"]["School"]["alpha"]

        censustract_alpha = subexperiment.simulation_bundles[
            0
        ].baseline_params[subexperiment.scenario_key]["settings_properties"][
            "CensusTract"
        ]["alpha"]

        pop_size = pl.read_csv(synth_pop).height
        average_times.append(
            {
                "scenario": int(scenario.split("=")[-1]),
                "average_time": average_time,
                "pop_size": pop_size,
                "infectiousness_scale": infectiousness_scale,
                "home_alpha": home_alpha,
                "workplace_alpha": workplace_alpha,
                "school_alpha": school_alpha,
                "censustract_alpha": censustract_alpha,
            }
        )

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
    exp_output = experiment.read_results(filename="scenarios")
    exp_output = exp_output.join(
        pl.DataFrame(average_times), on="scenario", how="left"
    )
    exp_output.write_csv(
        os.path.join(experiment.directory, "experiment_runtime.csv")
    )


def read_fn(outputs_dir):
    output_file_path = os.path.join(outputs_dir, "person_property_count.csv")
    if os.path.exists(output_file_path):
        df = pl.read_csv(output_file_path)
    else:
        raise FileNotFoundError(f"{output_file_path} does not exist.")
    max_t = df["t"].max()
    df = (
        (
            df.group_by(["t", "InfectionStatus"])
            .agg(pl.col("count").sum())
            .filter((pl.col("InfectionStatus") == pl.lit("Recovered")))
        )
        .filter(pl.col("t") == max_t)
        .select(pl.col("count").alias("attack_rate"))
    )
    print(df.to_series().to_list()[0])
    return df


main()
