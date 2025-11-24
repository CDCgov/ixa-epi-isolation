FROM rocker/r-ver:4.4

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libudunits2-dev \
    libcurl4-openssl-dev \
    libxml2-dev \
    libssl-dev \
    libproj-dev \
    proj-data \
    proj-bin \
    libgdal-dev \
    gdal-bin \
    && rm -rf /var/lib/apt/lists/*

# Install R packages (will use binaries if available)
RUN Rscript -e 'install.packages(c("tidyverse", "tigris", "tidycensus", "patchwork", "data.table"))'
WORKDIR /app
# CMD to be passed from command line
