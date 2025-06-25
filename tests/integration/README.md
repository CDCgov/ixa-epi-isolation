# Running integration tests

Currently, the integration tests are performed by visual inspection of figures that are produced with commentary in an HTML report. To generate the report, run (from the root of the repo) `bash tests/integration/run_integration_tests.sh`.

That bash script will generate inputs (by running `scripts/create_integration_test_pops.R`, `scripts/create_integration_test_rate_fns.R`, and `scripts/create_ode_output.R`) that are saved at `tests/input`. Then, the ABM simulations themselves are run in Ixa and the simulation output are saved in the respective sub-directory in `tests/integration`; for example, for the ABM that uses typical SIR assumptions, the simulation output are stored in `tests/integration/sir/data/simulations`. Finally, the HTML report is generated and can be found at `tests/integration/integration_tests_report.html`.

The following actions are required before running the bash script:

- Ensure that a release version of the model has been built and is located at `target/release`. If necessary, run `cargo --build release`.
- Ensure the following R packages are installed: `tidyverse`, `deSolve`, `arrow`, and `rmarkdown`.
- Run `poetry install`.
