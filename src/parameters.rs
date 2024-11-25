use ixa::error::IxaError;
use std::fmt::Debug;
use std::path::PathBuf;

use ixa::define_global_property;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub infection_duration: f64,
    pub generation_interval: f64,
    pub report_period: f64,
    pub synth_population_file: PathBuf,
    pub population_periodic_report: String,
}
define_global_property!(Parameters, ParametersValues);
