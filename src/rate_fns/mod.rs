pub mod rate_fn;
pub use rate_fn::{InfectiousnessRateFn, ScaledRateFn};
pub mod rate_fn_storage;
pub use rate_fn_storage::{load_rate_fns, InfectiousnessRateExt, RateFn};

pub mod constant_rate;
pub use constant_rate::ConstantRate;

pub mod empirical_rate;
pub use empirical_rate::EmpiricalRate;
