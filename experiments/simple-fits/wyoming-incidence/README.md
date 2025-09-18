To run the experiment, read [how to set up your environment](experiments/README.md).
Be sure to have the Azure credential `config.toml` at your root, or set `azure_batch: false`
in the `config.yaml`.

Create the `input/weekly_hospitalization_metrics_WY.csv` by exporting the data from source using
the [NHSN website](https://data.cdc.gov/Public-Health-Surveillance/Weekly-United-States-Hospitalization-Metrics-by-Ju/aemt-mg7g/data_preview)
and filtering for results from Wyoming. The data will be post-processed in the `data_cleaning.R` script of the experiment.

To generate the synthetic population file, in `scripts/create_synthetic_population.R` set

```{R}
state_synth <- "WY"
year_synth <- 2023
population_size <- 589000
```

Then execute

```{shell}
Rscript scripts/create_synthetic_population.R
make -f experiments/simple-fits/wyoming-incidence/Makefile
```
