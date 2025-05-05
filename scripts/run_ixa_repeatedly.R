# this script provides a simple way to run a model in Ixa repeatedly
# note that there are issues with the path variables in RStudio,
# so run the script directly from a terminal

# function that changes the seed, runs the model, and gets the output
# hard-coded that seed is in row 4 of the input json
run_ixa_rep_fx <- function(ixa_rep) {
  print(paste("currently running replicate", ixa_rep, "of", ixa_reps, sep = " "))
  input_json[4] <- gsub("\\d+", ixa_rep, input_json[4])
  writeLines(input_json, args[1])
  system(paste("cargo run -- -c", args[1], "-o ./output -f"))
  infectious_report <- readr::read_csv(file.path(
    "output",
    "person_property_count.csv"
  ), show_col_types = FALSE) |>
    dplyr::mutate(model = "ixa", ixa_rep = ixa_rep) |>
    dplyr::select(!c(Age))
  return(infectious_report)
}

# get the name of the JSON to use and the name of the output

args <- commandArgs(trailingOnly = TRUE)

# read in the input JSON, of which the seed is a component

input_json <- readLines(args[1])

# set number of reps
ixa_reps <- 50

# actually run the model and compile results
output <- lapply(X = seq_len(ixa_reps), FUN = run_ixa_rep_fx)

# turn results in a df
output_df <- do.call(rbind, output)

# export results
write.csv(
  x = output_df,
  file = args[2],
  row.names = FALSE
)
