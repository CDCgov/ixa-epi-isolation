To run the experiment, read [how to set up your environment](/experiments/README.md).
Be sure to have the Azure credential `config.toml` at your root, or set `azure_batch: false`
in the `experiments/simple-fits/wyoming-incidence/input/config.yaml`. If using Azure Batch, it may be necessary to set `create_pool: true` and change the input values for `blob_container_name` and `pool_name`.

To generate the synthetic population file, in `scripts/create_synthetic_population.R` set

```{R}
state_synth <- "WY"
year_synth <- 2023
population_size <- 589000
```

To generate a synthetic population, execute:
```{shell}
docker build -t ixa-epi-isolation-r -f DockerfileR . && docker run --rm -v "$(pwd):/app" ixa-epi-isolation-r Rscript scripts/create_synthetic_population.R
```

(See https://docs.docker.com/desktop/features/wsl/ for more information about installing Docker and using it in WSL.)

Alternatively, the following command will generate a synthetic population, but this approach requires ensuring that the necessary R packages (tidyverse, tigris, sf, tidycensus, patchwork, and data.table) are installed:

```{shell}
Rscript scripts/create_synthetic_population.R
```

Running the make file should run all necessary steps, including downloading the target data, running the calibration, and generating the output report.
```{shell}
make -f experiments/simple-fits/wyoming-incidence/Makefile
```
