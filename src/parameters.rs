use std::fmt::Debug;

use ixa::define_global_property;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParametersValues {
    pub population: usize,
    pub max_time: f64,
    pub seed: u64,
    pub r_0: f64,
    pub infection_duration: f64,
    pub generation_interval: f64,
    pub report_period: f64,
}
define_global_property!(Parameters, ParametersValues);
