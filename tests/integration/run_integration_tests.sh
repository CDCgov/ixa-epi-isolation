#!/bin/bash

echo "Running the simulations for the SIR-like model..."

poetry run python scripts/call_integration_test.py -f tests/integration/sir/input/config.yaml

echo "Running the simulations for the SEIR-like model..."

poetry run python scripts/call_integration_test.py -f tests/integration/seir/input/config.yaml

echo "Generating report with integration tests to be performed by visual inspection..."

Rscript -e  "rmarkdown::render('tests/integration/integration_tests_report.rmd')"
