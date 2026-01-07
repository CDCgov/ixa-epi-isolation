python := poetry run python
BUILD := target/release/epi-isolation
DIR := experiments/simple-fits/wyoming-incidence
TARGET_DATA := $(DIR)/input/target_data.csv
TARGET_WRITER := $(DIR)/scripts/data_cleaning.R
POP_BUILDER := scripts/create_synthetic_population.R
ABC := $(DIR)/scripts/abc.py
SCENARIO := $(DIR)/scripts/scenarios.py
CONFIG := $(DIR)/input/config.yaml
POP := input/synth_pop_people_WY_589000.csv
ABC_HISTORY := $(DIR)/experiment_history.pkl
SCENARIO_HISTORY := $(DIR)/data/experiment_history.pkl
MD := $(DIR)/docs/output.qmd
REPORT := $(DIR)/docs/output.pdf

.PHONY: all
all: run write

$(BUILD): Cargo.toml src/*.rs
	cargo build --release

$(TARGET_DATA): $(TARGET_WRITER)
	Rscript $(<)

$(POP): $(POP_BUILDER)
	Rscript $(<)

$(ABC_HISTORY): $(ABC) $(TARGET_DATA) $(BUILD) $(CONFIG) $(POP)
	$(python) $(<)

run: $(ABC_HISTORY)

$(SCENARIO_HISTORY): $(SCENARIO) $(ABC_HISTORY)
	$(python) $(<)

run: $(SCENARIO_HISTORY)

$(REPORT): $(MD) $(HISTORY)
	poetry run quarto render $(MD)

write: $(REPORT)
