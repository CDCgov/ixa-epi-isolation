# generate simple, unstructured synthetic population of arbitrary size
n <- 1e3

simple_people <- data.frame(
  "age" = rep(1, times = n),
  "homeId" = "10000000000"
)

write.csv(
  x = simple_people,
  file = "./input/simple_people.csv",
  row.names = FALSE
)
