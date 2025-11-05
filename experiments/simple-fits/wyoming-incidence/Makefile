python := poetry run python
BUILD := target/release/epi-isolation
DIR := experiments/simple-fits/wyoming-incidence

.PHONY: all
all: run write

$(BUILD): Cargo.toml src/*.rs
	cargo build --release

TARGET_DATA := $(DIR)/input/target_data.csv
RAW_DATA_FILE := input/weekly_hospitalization_metrics_WY.csv
TARGET_WRITER := $(DIR)/scripts/data_cleaning.R

$(TARGET_DATA): $(TARGET_WRITER) $(RAW_DATA_FILE)
	Rscript $(<)

run: $(HISTORY)
ABC := $(DIR)/scripts/abc.py
CONFIG := $(DIR)/input/config.yaml
HISTORY := $(DIR)/data/experiemnt_history.pkl

$(HISTORY): $(ABC) $(TARGET_DATA) $(BUILD) $(CONFIG)
	$(python) $(<)

write: $(REPORT)
MD := $(DIR)/docs/output.qmd
REPORT := $(DIR)/docs/output.pdf

$(REPORT): $(MD) $(HISTORY)
	quarto render $(MD)
