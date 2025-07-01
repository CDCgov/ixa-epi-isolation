import os
import subprocess
import time
import polars as pl
import yaml
import json


def run_simulation_for_synth_pop(synth_pop, isolation_probability):
    input_file_path = "input/input.json"
    if not os.path.exists(input_file_path):
        raise FileNotFoundError(
            f"The required input file '{input_file_path}' does not exist."
        )

    with open(input_file_path, "r") as file:
        input_data = json.load(file)
        # Access and modify elements of input_data
        input_data["epi_isolation.GlobalParams"]["synth_population_file"] = synth_pop
        input_data["epi_isolation.GlobalParams"]["intervention_policy_parameters"]["isolation_probability"] = isolation_probability
        # Save the updated input_data dictionary to input_benchmark.json
        with open(
            "input/input_benchmark.json", "w"
        ) as output_file:
            json.dump(input_data, output_file, indent=2)
    ixa_command = [
        os.path.join("target", "release", "epi-isolation"),
        "-c",
        "input/input_benchmark.json",
        "-o",
        "output/benchmarking",
        "-f",
    ]

    subprocess.run(ixa_command, check=True)


def time_simulation(synth_pop, policy_input):
    start_time = time.time()
    run_simulation_for_synth_pop(synth_pop, policy_input)
    end_time = time.time()
    elapsed_time = end_time - start_time
    return elapsed_time

def get_attack_rate(outputs_dir):
    output_file_path = os.path.join(outputs_dir, "person_property_count.csv")
    if os.path.exists(output_file_path):
        df = pl.read_csv(output_file_path)
    else:
        raise FileNotFoundError(f"{output_file_path} does not exist.")
    max_t = df["t"].max()
    df = (
        df.group_by(["t", "InfectionStatus"])
        .agg(pl.col("count").sum())
        .filter(
            (pl.col("InfectionStatus") == pl.lit("Recovered"))
        )
    ).filter(pl.col("t") == max_t).select(pl.col("count").alias("attack_rate"))
    return df.to_series().to_list()[0]

def main():
    # define grid parameters. You will need to modify the run simulation for
    # synth pop function if you change these parameters 
    synth_pop = [
        "input/synth_pop_people_WY_1000.csv",
        "input/synth_pop_people_WY_10000.csv"
    ]
    isolation_probability_input = [0.0,1.0]
    results = []
    for isolation_probability in isolation_probability_input:
        for pop in synth_pop:
            # run the simulation for a number of replications to understand the variability
            for i in range(1):
                elapsed_time = time_simulation(pop, isolation_probability)
                num_rows = sum(1 for _ in open(pop)) - 1
                ar = get_attack_rate("output/benchmarking")/num_rows
                results.append(
                    {
                        "synth_pop": pop,
                        "iteration": i + 1,
                        "elapsed_time": elapsed_time,
                        "num_rows": num_rows,
                        "isolation_probability": isolation_probability,
                        "attack_rate": ar
                    }
                )
                print(
                    f"Simulation for {pop} iteration {i} completed in {elapsed_time:.2f} seconds with {num_rows} rows."
                )

    # Convert results to a DataFrame
    df = pl.DataFrame(results)
    df.write_csv(
        "output/benchmarking/ixa_epi_isolation_runtime.csv"
    )


main()