import argparse
import os
from math import log

import polars as pl
from abmwrappers import wrappers
from abmwrappers.experiment_class import Experiment
from scipy.stats import beta, uniform, norm, poisson
import pickle


experiment = Experiment(img_file="./experiments/simple-fits/distance-test/data/experiment_history.pkl")
steps = len(experiment.tolerance_dict)
print(experiment)

import seaborn as sns
import matplotlib.pyplot as plt
import polars as pl

max_target = 450

# For azure implementations, use default blob container read:
if experiment.azure_batch:
    simulations = experiment.read_results(filename="simulations", verbose = False)
    distances = experiment.read_results(filename="distances",verbose=False)
# For local implementations, account for relative path of docs:
else:
    simulations = experiment.read_results(filename="simulations", input_dir ="../data")
    distances = experiment.read_results(filename="distances", input_dir ="../data")

max_t_target = experiment.target_data.select(pl.col(["t", "total_admissions"])).sort("total_admissions", descending=True).select("t").to_series()[0]
# max_t_result = simulations.select(pl.col(["t", "count"])).sort("count", descending=True).select("t").to_series()[0]
# difference = max_t_result - max_t_target
# results_data = results_data.with_columns(
#     (pl.col("t") - difference).alias("t")
# )
# Adjust the "t" column for each unique value of "t" in simulations
# unique_sim_values = simulations.select("simulation").unique().to_series()

# simulations = (
#     simulations.filter(pl.col("simulation") < 1500)
#     .group_by("simulation")
#     .apply(
#         lambda group: group.with_columns(
#             (pl.col("t") - (group.select(pl.col(["t", "count"]))
#                             .sort("count", descending=True)
#                             .select("t")
#                             .to_series()[0] - max_t_target)).alias("t")
#         )
#     )
# )

# Combine all adjusted results back into a single DataFrame
# simulations = pl.concat(adjusted_results)

simulations = simulations.with_columns((pl.col("t") - 100).alias("t"))
posterior_sims=distances.sort("distance").filter(pl.col("distance")<max_target).join(simulations, on="simulation", how="inner")
posterior_sims=posterior_sims.filter(pl.col("simulation")<1500)
print(f"Showing {posterior_sims.select(pl.n_unique('simulation')).item()} accepted simulations from last step below threshold {max_target}")
sns.lineplot(posterior_sims, x="t", y="count",hue="distance", units="simulation", estimator=None)
sns.scatterplot(experiment.target_data, x = "t", y="total_admissions",zorder=10)
plt.show()

sns.lineplot(posterior_sims, x="t", y="count", estimator="median", errorbar=lambda x: (x.quantile(0.025), x.quantile(0.975)))
sns.lineplot(posterior_sims, x="t", y="count", estimator="median", errorbar=lambda x: (x.quantile(0.25), x.quantile(0.75)))
# sns.lineplot(posterior_sims, x="t", y="count",hue="distance", units="simulation", estimator=None)
sns.scatterplot(experiment.target_data, x = "t", y="total_admissions",zorder=10)
plt.show()


# # Path to the pickle file
pickle_file_path = 'experiments/simple-fits/distance-test/data/experiment_history.pkl'

# Load the pickle file
with open(pickle_file_path, 'rb') as file:
    data = pickle.load(file)

# print(data)
# # Print the loaded data
# print(data["skinny_bundles"][0]["distances"])
# print(data["skinny_bundles"][1]["distances"])
# print(data["skinny_bundles"][2]["distances"])

# import matplotlib.pyplot as plt

# # Extract the distance column
# # distances = [entry["distance"] for entry in data["skinny_bundles"][0]["accepted"]]

# # Create a boxplot for the distances
# plt.figure(figsize=(8, 5))
# for i in range(0, 1):
#     plt.boxplot(data["skinny_bundles"][i]["distances"]["distance"], vert=True, patch_artist=True, positions=[i])
# plt.title('Particle distances')
# plt.xlabel('Index (i)')
# plt.ylabel('Distance')
# plt.xticks(range(0, 1), [str(i) for i in range(0, 1)])
# plt.grid(True, linestyle='--', alpha=0.7)
# plt.show()