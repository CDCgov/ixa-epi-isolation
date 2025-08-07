from abmwrappers.experiment_class import Experiment
from abmwrappers import plotting, utils

import seaborn as sns
import polars as pl
import matplotlib.pyplot as plt
import pickle
import os

fp = "experiments/simple-fits/wyoming-constant-rate/data/experiment_history.pkl"
download=False
if download:
    client, blob_container_name, job_prefix = utils.initialize_azure_client("experiments/simple-fits/wyoming-constant-rate/input/azure_config.yaml", "simple-fits")
    client.download_file(
        src_path=os.path.join(
            "wyoming-constant-rate", "experiment_history.pkl"
        ),
        dest_path=fp,
        container_name=blob_container_name,
    )
set_experiment_name=False
if set_experiment_name:
    # Load the data from the compressed pickle file
    with open(fp, "rb") as f:
        data = pickle.load(f)
    data.update({
        "super_experiment_name": "simple-fits",
        "sub_experiment_name": "wyoming-constant-rate"
    })
    # Save the data to a compressed pickle file
    with open(fp, "wb") as f:
        pickle.dump(data, f)


plot_trajectories=True
plot_distros = False
if plot_distros or plot_trajectories:
    experiment = Experiment(
        img_file=fp
    )

def output_processing_function(df: pl.DataFrame) -> pl.DataFrame:
    df = (
        df.with_columns((pl.col("time") / 7.0).ceil().cast(pl.Int64).alias("week"))
        .group_by("week")
        .agg(pl.len().alias("count"))
        .with_columns((pl.col("week") * 7 - 1).alias("t"))
    )

    return df

if plot_distros:
    plotting.plot_posterior_distribution_2d(experiment)
    plotting.plot_posterior_distribution_2d(experiment, visualization_methods_marginal=["histogram", "density"], visualization_methods=["density"])
    plotting.plot_posterior_distribution(experiment, visualization_methods=["histogram", "density"], facet_by=["parameter", "step"], include_previous_steps=True)
    plotting.plot_posterior_distribution(experiment, visualization_methods=["histogram", "density"], facet_by=["parameter"], include_priors=True)

if plot_trajectories:
    # With acces to raw_output
    if not experiment.azure_batch:
        hospitalization_data=experiment.read_results(filename="hospital_incidence_report", data_read_fn = output_processing_function)
        distances = experiment.read_results(filename="distances")
        best_sims = distances.sort("distance").head(9).join(hospitalization_data, on ="simulation", how = "inner")
    else:
        hospital_data = experiment.read_results(filename="simulations",input_dir="wyoming-constant-rate/data")
        distances = experiment.read_results(filename="distances",input_dir="wyoming-constant-rate/data")
        best_sims=distances.sort("distance").head(30).join(hospital_data, on="simulation", how="inner")
        all_sims=distances.sort("distance").join(hospital_data, on="simulation", how="inner")

    fig, axes = plt.subplots(nrows=1,ncols=2)

    ax=sns.lineplot(best_sims, x="t", y = "count", units="simulation", estimator=None, hue = "distance", ax=axes[1])
    sns.scatterplot(experiment.target_data, x = "t", y="total_admissions", ax=axes[1])
    ax.set(xlabel="Report day after Sep 13, 2020", ylabel="Total Wyoming hospital admissions")
    ax=sns.scatterplot(experiment.target_data, x = "t", y="total_admissions", ax=axes[0])
    ax.set(xlabel="Report day after Sep 13, 2020", ylabel="Total Wyoming hospital admissions")
    sns.lineplot(all_sims, x="t", y = "count", units="simulation", estimator=None, hue = "distance", ax=axes[0])
    plt.show()
