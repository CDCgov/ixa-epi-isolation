import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

sns.set_theme(style="whitegrid")

df = pd.read_csv("experiments/examples/parameter-sweep/experiment_runtime.csv")

# Plot Simulation runtime
plt.figure(figsize=(8, 6))
sns.scatterplot(
  data=df,
  x="pop_size",
  y="cpu_time",
  hue="attack_rate",
  palette="viridis"
)
plt.xscale("log")
plt.yscale("log")
plt.title("Simulation runtime")
plt.xlabel("Population Size (log scale)")
plt.ylabel("CPU Time in Seconds (log scale)")
plt.legend(title="Attack Rate")
plt.tight_layout()
plt.savefig('experiments/examples/parameter-sweep/runtime_by_population.png')

# Plot Simulation memory
plt.figure(figsize=(8, 6))
sns.scatterplot(
  data=df,
  x="pop_size",
  y="memory",
  hue="attack_rate",
  palette="viridis"
)
plt.xscale("log")
plt.yscale("log")
plt.title("Simulation Memory")
plt.xlabel("Population Size (log scale)")
plt.ylabel("Memory in Bytes (log scale)")
plt.legend(title="Attack Rate")
plt.tight_layout()
plt.savefig('experiments/examples/parameter-sweep/memory_by_population.png')