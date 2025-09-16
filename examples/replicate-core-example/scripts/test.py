## GOALS
## 1) What´s the minimal version of this package that I can use to run experiments using a list of
## parameters values
from abmwrappers.experiment_class import Experiment
from scipy.stats import uniform

## 1) Define the experiments:
## - Seed N = 30
## - initial_infected: Uniform(20, 80)

## Guido Note:
## I would like to have more control of parameters, default files, and other.
## seed = uniform(30, 1, 100000)
## initial_infected = uniform(30, 20, 80)
## guidance_policy: [None, updated]

## Define a config file for abmwrappers
experiment_config_file = "examples/replicate-core-example/input/config_initial.yaml"

## experiments_directory is assuming you are in root. "" -> root directory
experiment = Experiment(config_file = experiment_config_file,
                        experiments_directory = "")

experiment.priors = dict(initial_incidence = uniform(0.2, 0.2))
experiment.n_particles = 10
experiment.replicates = 1

## 2) Create the input files for such experiments
## Run step: initialize simulation history,
## - runs each index in the simulation
## - processes the data

## Prior distribution it's confusing. Shouldn´t it be parameters? or parameters' distribution?
## initial_simbundle: sample parameters, and create inputs? 
experiment_bundle = experiment.initialize_simbundle()
experiment.get_default_value("initial_recovered")
## How do we update or provide a data frame to run
## experiment_bundle.update(expand(experiment_bundle.inputs, guidance_policy_vector))

## 3) Run the experiments with ixa
## Data file name and data read function shuoldn´t be absolutely necessary. 

experiment.write_inputs(experiments.inputs)
experiment.run_inputs(data_filename = "person_property_count.csv", compress = False)



## 4) Collect the data based on some gathering function

## 5) Plot
