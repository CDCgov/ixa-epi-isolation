### This script creates synthetic populations for use in integration tests.
### A single synthetic population may be used in multiple integration tests.

### This script is assumed to run from the root directory of the repo.

file_path <- "tests/input"
if (!dir.exists(file_path)) {
  dir.create(file_path)
}

### This script makes 3 synthentic population .csv files:
### simple_pop, unique_hh_pop, two_hh_pop

### Simple homogeneous mixing population: simple_pop ###
### synthetic population where all persons live in same household

pop_size <- 50

simple_pop <- data.frame(
  "age" = rep(1, times = pop_size),
  "homeId" = "10000000000",
  "schoolId" = NA,
  "workplaceId" = NA
)

write.csv(
  x = simple_pop,
  file = "tests/input/pop_simple.csv",
  row.names = FALSE,
  na = ""
)

### Unique household and shared census tract for all: unique_hh_pop ###
### synthetic population structured into as many households as there are people

unique_hh_pop <- data.frame(
  "age" = rep(1, times = pop_size),
  "homeId" = 1000000000000 + seq_len(pop_size),
  "schoolId" = NA,
  "workplaceId" = NA
)

write.csv(
  x = unique_hh_pop,
  file = "tests/input/pop_unique_hh.csv",
  row.names = FALSE,
  na = ""
)

### Population divided between two households: two_hh_pop ###
### synthetic population divided into two households (one shared census tract)

first_hh_size <- ceiling(pop_size / 2)
second_hh_size <- pop_size - first_hh_size

two_hh_pop <- data.frame(
  "age" = rep(1, times = pop_size),
  "homeId" = c(
    rep(1000000000001, times = first_hh_size),
    rep(1000000000002, times = second_hh_size)
  ),
  "schoolId" = NA,
  "workplaceId" = NA
)

write.csv(
  x = two_hh_pop,
  file = "tests/input/pop_two_hh.csv",
  row.names = FALSE,
  na = ""
)
