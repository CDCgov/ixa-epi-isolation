To run the experiment, read [how to set up your environment](/experiments/README.md).
Be sure to have the Azure credential `config.toml` at your root, or set `azure_batch: false`
in the `experiments/simple-fits/recreate-synthetic/input/config.yaml`. If using Azure Batch, it may be necessary to set `create_pool: true` and change the input values for `blob_container_name` and `pool_name`.

The default values for generating the synthetic population file are set as follows in `scripts/create_synthetic_population.R`:

```{R}
state_synth <- "WY"
year_synth <- 2023
population_size <- 100000
```

To generate a synthetic population, execute:
```{shell}
docker build -t ixa-epi-isolation-r -f Dockerfile.R . && docker run --rm -v "$(pwd):/app" ixa-epi-isolation-r Rscript scripts/create_synthetic_population.R
```

(See https://docs.docker.com/desktop/features/wsl/ for more information about installing Docker and using it in WSL.)

Alternatively, the following command will generate a synthetic population, but this approach requires ensuring that the necessary R packages (tidyverse, tigris, sf, tidycensus, patchwork, and data.table) are installed:

```{shell}
Rscript scripts/create_synthetic_population.R
```

Be sure that `experiments/simple-fits/recreate-synthetic/input/base.json` correctly specifies the file name of the synthetic population generated in the previous step, e.g.
```{json}
{
  "epi_isolation.GlobalParams": {
    ...
    "synth_population_file": "input/synth_pop_people_WY_100000.csv",
    ...
    }
}
```

and `experiments/simple-fits/recreate-synthetic/input/config.yaml` e.g.
```{yaml}
...
local_path:
  target_data_file: input/synth_pop_people_WY_100000.csv
```

Finally:
```{shell}
cargo build --release
poetry run python experiments/simple-fits/recreate-synthetic/scripts/abc.py
poetry run quarto render experiments/simple-fits/recreate-synthetic/docs/output.qmd
```
