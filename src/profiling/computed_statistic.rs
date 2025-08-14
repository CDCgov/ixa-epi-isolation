#[cfg(feature = "profiling")]
use super::profiling_data;
use super::ProfilingDataContainer;
use serde::Serialize;
use std::fmt::Display;

pub type CustomStatisticComputer =
    Box<dyn (Fn(&ProfilingDataContainer) -> ComputedValue) + Send + Sync>;
pub type CustomStatisticPrinter = Box<dyn (Fn(ComputedValue)) + Send + Sync>;

pub(super) struct ComputedStatistic {
    /// The label used for the statistic in the JSON report.
    pub label: &'static str,
    /// The computed value of the statistic.
    pub value: ComputedValue,
    /// The function used to compute the statistic.
    pub computer: CustomStatisticComputer,
    /// The function used to print the statistic to the console.
    pub printer: CustomStatisticPrinter,
}

/// The computed value of a statistic. The "computer" returns a value of this type.
#[derive(Clone, PartialEq, Serialize, Debug)]
pub enum ComputedValue {
    // The `None` case is for situations where the ability to compute a value is not guaranteed.
    None,
    USize(usize),
    Int(i64),
    Float(f64),
    Vec(Vec<ComputedValue>),
}

impl ComputedValue {
    pub fn is_none(&self) -> bool {
        self == &ComputedValue::None
    }
}

impl Default for ComputedValue {
    fn default() -> Self {
        Self::None
    }
}

impl Display for ComputedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputedValue::None => {
                write!(f, "")
            }

            ComputedValue::USize(value) => {
                write!(f, "{}", value)
            }

            ComputedValue::Int(value) => {
                write!(f, "{}", value)
            }

            ComputedValue::Float(value) => {
                write!(f, "{}", value)
            }

            ComputedValue::Vec(values) => {
                let formatted_values = values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>();
                write!(f, "[{}]", formatted_values.join(", "))
            }
        }
    }
}

#[cfg(feature = "profiling")]
pub fn add_computed_statistic(
    label: &'static str,
    computer: CustomStatisticComputer,
    printer: CustomStatisticPrinter,
) {
    let mut container = profiling_data();
    container.add_computed_statistic(label, computer, printer);
}
#[cfg(not(feature = "profiling"))]
pub fn add_computed_statistic(
    _label: &'static str,
    _computer: CustomStatisticComputer,
    _printer: CustomStatisticPrinter,
) {
}
