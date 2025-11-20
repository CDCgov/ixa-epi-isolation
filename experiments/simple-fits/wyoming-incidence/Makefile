python := poetry run python
BUILD := target/release/epi-isolation
DIR := experiments/simple-fits/wyoming-incidence
TARGET_DATA := $(DIR)/input/target_data.csv
TARGET_WRITER := $(DIR)/scripts/data_cleaning.R
ABC := $(DIR)/scripts/abc.py
CONFIG := $(DIR)/input/config.yaml
HISTORY := $(DIR)/data/experiment_history.pkl
MD := $(DIR)/docs/output.qmd
REPORT := $(DIR)/docs/output.pdf

.PHONY: all
all: run write

$(BUILD): Cargo.toml src/*.rs
	cargo build --release

$(TARGET_DATA): $(TARGET_WRITER)
	Rscript $(<)

$(HISTORY): $(ABC) $(TARGET_DATA) $(BUILD) $(CONFIG)
	$(python) $(<)

run: $(HISTORY)

$(REPORT): $(MD) $(HISTORY)
	poetry run quarto render $(MD)

write: $(REPORT)
