To run the experiment, read [how to set up your environment](experiments/README.md).
Be sure to have the Azure credential `config.toml` at your root, or set `azure_batch: false`
in the `config.yaml`.

To generate the synthetic population file, in `scripts/create_synthetic_population.R` set

```{R}
state_synth <- "WY"
year_synth <- 2023
population_size <- 100000
```

Then execute

```{shell}
Rscript scripts/create_synthetic_population.R
poetry run python experiments/simple-fits/recreate-synthetic/scripts/abc.py
quarto render experiments/simple-fits/recreate-synthetic/docs/output.qmd
```
