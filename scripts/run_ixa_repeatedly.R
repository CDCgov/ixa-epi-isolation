# this script provides a simple way to run a model in Ixa repeatedly
# note that there are issues with the path variables in RStudio,
# so run the script directly from a terminal

# function that changes the seed, runs the model, and gets the output
run_ixa_rep_fx <- function(ixa_rep){
  input_params$epi_isolation.Parameters$seed <- ixa_rep
  jsonlite::toJSON(x = input_params, pretty = TRUE, auto_unbox = TRUE) |>
    writeLines("./input/input.json")
  system("cargo run -- -i ./input/input.json -o ./output -f")
  infectious_report <- readr::read_csv(file.path(
    "output",
    "person_property_count.csv"
  )) |> dplyr::mutate(model = "ixa", ixa_rep = ixa_rep) |> 
    dplyr::select(!c(Age, CensusTract))
  return(infectious_report)
}

# read in the input JSON, of which the seed is one of the params
input_params <- jsonlite::fromJSON("./input/input.json")

# set number of reps
ixa_reps <- 50

# actually run the model and compile results
output <- lapply(X = seq_len(ixa_reps), FUN = run_ixa_rep_fx)

# turn results in a df
output_df <- do.call(rbind, output)

# export results
write.csv(x = output_df,
          file = "./output/person_property_count_multi_rep.csv",
          row.names = FALSE)