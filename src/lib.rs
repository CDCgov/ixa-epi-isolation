// Re-export commonly used types at the crate root
pub use parameters::{HospitalizationParameters, Params, ProgressionLibraryType};
pub use population_loader::Age; // Re-export Age from its canonical location
pub use rate_fns::{load_rate_fns, ConstantRate, RateFn};
pub use symptom_progression::{SymptomValue, Symptoms}; // Module declarations
pub mod computed_statistics;
pub mod hospitalizations;
pub mod infection_propagation_loop;
pub mod infectiousness_manager;
pub mod interventions;
pub mod natural_history_parameter_manager;
pub mod parameters;
pub mod policies;
pub mod population_loader;
pub mod property_progression_manager;
pub mod rate_fns;
pub mod reports;
pub mod settings;
pub mod symptom_progression;
pub mod utils;

// Re-export common macros
pub use ixa::assert_almost_eq;
