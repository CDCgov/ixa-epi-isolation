pub mod curve_fitting;
pub use curve_fitting::linear_interpolation;

pub mod numeric_integrators;
pub use numeric_integrators::cumulative_trapezoid_integral;
pub use numeric_integrators::trapezoid_integral;
