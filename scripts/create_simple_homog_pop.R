# This script creates simple synthetic populations of arbitrary size n.

n <- 50

# generate synthetic population where all persons live in same household
# this corresponds to simplest mass-action homogeneous mixing assumption

simple_people <- data.frame(
  "age" = rep(1, times = n),
  "homeId" = "10000000000",
  "schoolId" = NA,
  "workplaceId" = NA
)

write.csv(
  x = simple_people,
  file = "./input/simple_people.csv",
  row.names = FALSE,
  na = ""
)

# generate synthetic population where all persons live alone
# (i.e., a unique household for each person)
# but everyone is in the same census tract

unique_hh_people <- data.frame(
  "age" = rep(1, times = n),
  "homeId" = 1000000000000 + seq_len(n),
  "schoolId" = NA,
  "workplaceId" = NA
)

write.csv(
  x = unique_hh_people,
  file = "./input/unique_hh_people.csv",
  row.names = FALSE,
  na = ""
)

