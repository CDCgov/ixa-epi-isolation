To run the experiment, read [how to set up your environment](experiments/README.md).
Be sure to have the Azure credential `config.toml` at your root, or set `azure_batch: false`
in the `config.yaml`.

To generate the synthetic population file, in `scripts/create_synthetic_population.R` set

```{R}
state_synth <- "WY"
year_synth <- 2023
population_size <- 100000
```

Then execute:
```{shell}
docker build -t ixa-epi-isolation-r -f Dockerfile.R . && docker run --rm -v "$(pwd):/app" ixa-epi-isolation-r Rscript scripts/create_synthetic_population.R
```
OR:

```{shell}
Rscript scripts/create_synthetic_population.R
```

Be sure to update `input/base.json`  with the generated file name e.g.
```{json}
{
  "epi_isolation.GlobalParams": {
    ...
    "synth_population_file": "input/synth_pop_people_WY_1000000.csv",
    ...
    }
}
```

and `input/config.yaml` e.g.
```{yaml}
...
local_path:
  target_data_file: input/synth_pop_people_WY_1000000.csv
```

Finally:
```{shell}
cargo build --release
poetry run python experiments/simple-fits/recreate-synthetic/scripts/abc.py
quarto render experiments/simple-fits/recreate-synthetic/docs/output.qmd
```
