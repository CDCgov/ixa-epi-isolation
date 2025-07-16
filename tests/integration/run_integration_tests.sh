#!/bin/bash

echo "Building release version of ixa model..."

cargo build --release

echo "Generating populations..."

Rscript scripts/create_integration_test_pops.R

echo "Generating rate functions..."

Rscript scripts/create_integration_test_rate_fns.R

echo "Generating ODE output..."

Rscript scripts/create_ode_output.R

echo "Running simulations for the SIR-like model with small population size..."

poetry run python scripts/call_integration_test.py -f tests/integration/sir_small/input/config.yaml

echo "Running the simulations for the SEIR-like model..."

poetry run python scripts/call_integration_test.py -f tests/integration/seir/input/config.yaml

echo "Generating report with integration tests to be performed by visual inspection..."

Rscript -e  "rmarkdown::render('tests/integration/integration_tests_report.rmd')"

echo "Generating appendix on final size calculations..."

Rscript -e  "rmarkdown::render('tests/integration/final_size_appendix.rmd')"
