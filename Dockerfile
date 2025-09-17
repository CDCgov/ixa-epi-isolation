FROM rust:bullseye as rust-build
WORKDIR /app

RUN apt-get -y update
RUN apt-get install -y sudo

USER admin
COPY target/release/ .
COPY Cargo.lock .
COPY Cargo.toml .

# Second stage: Python with Java runtime
FROM python:3.10.17

# Configure environment variables
ENV PYTHONFAULTHANDLER=1 \
PYTHONUNBUFFERED=1 \
PYTHONHASHSEED=random \
PIP_NO_CACHE_DIR=off \
PIP_DISABLE_PIP_VERSION_CHECK=on \
PIP_DEFAULT_TIMEOUT=100 \
POETRY_NO_INTERACTION=1 \
POETRY_VIRTUALENVS_CREATE=false \
POETRY_CACHE_DIR='/var/cache/pypoetry' \
POETRY_HOME='/usr/local' \
POETRY_VERSION=1.8.4

# Install system dependencies and Poetry
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
RUN curl -sSL https://install.python-poetry.org | python3 -

# Verify Poetry installation
RUN poetry --version

# Set up Python application dependencies
WORKDIR /
COPY poetry.lock pyproject.toml ./
RUN poetry install --no-interaction --no-ansi

# Copy the Rust binary from the build stage
COPY --from=rust-build /app/ /app
