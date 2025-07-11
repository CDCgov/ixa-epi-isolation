#[cfg(feature = "profiling")]
use super::profiling_data;
use super::ProfilingDataContainer;
use serde::Serialize;
use std::fmt::Display;

pub type CustomStatisticComputer<T> =
    Box<dyn (Fn(&ProfilingDataContainer) -> Option<T>) + Send + Sync>;
pub type CustomStatisticPrinter<T> = Box<dyn (Fn(T)) + Send + Sync>;

pub(super) enum ComputedStatisticFunctions {
    USize {
        computer: CustomStatisticComputer<usize>,
        printer: CustomStatisticPrinter<usize>,
    },
    Int {
        computer: CustomStatisticComputer<i64>,
        printer: CustomStatisticPrinter<i64>,
    },
    Float {
        computer: CustomStatisticComputer<f64>,
        printer: CustomStatisticPrinter<f64>,
    },
}

impl ComputedStatisticFunctions {
    /// A type erased way to compute a statistic.
    pub(super) fn compute(&self, container: &ProfilingDataContainer) -> Option<ComputedValue> {
        match self {
            ComputedStatisticFunctions::USize { computer, .. } => {
                computer(container).map(ComputedValue::USize)
            }
            ComputedStatisticFunctions::Int { computer, .. } => {
                computer(container).map(ComputedValue::Int)
            }
            ComputedStatisticFunctions::Float { computer, .. } => {
                computer(container).map(ComputedValue::Float)
            }
        }
    }

    /// A type erased way to print a statistic.
    pub(super) fn print(&self, value: ComputedValue) {
        match value {
            ComputedValue::USize(value) => {
                let ComputedStatisticFunctions::USize { printer, .. } = self else {
                    unreachable!()
                };
                (printer)(value);
            }
            ComputedValue::Int(value) => {
                let ComputedStatisticFunctions::Int { printer, .. } = self else {
                    unreachable!()
                };
                (printer)(value);
            }
            ComputedValue::Float(value) => {
                let ComputedStatisticFunctions::Float { printer, .. } = self else {
                    unreachable!()
                };
                (printer)(value);
            }
        }
    }
}

pub(super) struct ComputedStatistic {
    /// The label used for the statistic in the JSON report.
    pub label: &'static str,
    /// The computed value of the statistic.
    pub value: Option<ComputedValue>,
    /// The two functions used to compute the statistic and to print it to the console.
    pub functions: ComputedStatisticFunctions,
}

// This trick makes it so client code can _use_ `ComputableType` but not _implement_ it.
mod sealed {
    pub(super) trait SealedComputableType {}
}
#[allow(private_bounds)]
pub trait ComputableType: sealed::SealedComputableType
where
    Self: Sized,
{
    // This method is only callable from within this crate.
    #[allow(private_interfaces)]
    fn new_functions(
        computer: CustomStatisticComputer<Self>,
        printer: CustomStatisticPrinter<Self>,
    ) -> ComputedStatisticFunctions;
}
impl sealed::SealedComputableType for usize {}
impl ComputableType for usize {
    #[allow(private_interfaces)]
    fn new_functions(
        computer: CustomStatisticComputer<Self>,
        printer: CustomStatisticPrinter<Self>,
    ) -> ComputedStatisticFunctions {
        ComputedStatisticFunctions::USize { computer, printer }
    }
}
impl sealed::SealedComputableType for i64 {}
impl ComputableType for i64 {
    #[allow(private_interfaces)]
    fn new_functions(
        computer: CustomStatisticComputer<Self>,
        printer: CustomStatisticPrinter<Self>,
    ) -> ComputedStatisticFunctions {
        ComputedStatisticFunctions::Int { computer, printer }
    }
}
impl sealed::SealedComputableType for f64 {}
impl ComputableType for f64 {
    #[allow(private_interfaces)]
    fn new_functions(
        computer: CustomStatisticComputer<Self>,
        printer: CustomStatisticPrinter<Self>,
    ) -> ComputedStatisticFunctions {
        ComputedStatisticFunctions::Float { computer, printer }
    }
}

/// The computed value of a statistic. The "computer" returns a value of this type.
#[derive(Copy, Clone, PartialEq, Serialize, Debug)]
pub(super) enum ComputedValue {
    USize(usize),
    Int(i64),
    Float(f64),
}

impl Display for ComputedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputedValue::USize(value) => {
                write!(f, "{}", value)
            }

            ComputedValue::Int(value) => {
                write!(f, "{}", value)
            }

            ComputedValue::Float(value) => {
                write!(f, "{}", value)
            }
        }
    }
}

#[cfg(feature = "profiling")]
pub fn add_computed_statistic<T: ComputableType>(
    label: &'static str,
    computer: CustomStatisticComputer<T>,
    printer: CustomStatisticPrinter<T>,
) {
    let mut container = profiling_data();
    container.add_computed_statistic(label, computer, printer);
}
#[cfg(not(feature = "profiling"))]
pub fn add_computed_statistic<T: ComputableType>(
    _label: &'static str,
    _computer: CustomStatisticComputer<T>,
    _printer: CustomStatisticPrinter<T>,
) {
}
