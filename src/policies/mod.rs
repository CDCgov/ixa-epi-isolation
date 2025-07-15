use ixa::{Context, IxaError};
use serde::{Deserialize, Serialize};

use crate::parameters::{ContextParametersExt, Params};

pub mod updated_guidance;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum Policies {
    // Struct contain policy parameters for isolation guidance
    // Post-isolation duration, isolation probability, and maximum isolation delay.
    UpdatedIsolationGuidance {
        post_isolation_duration: f64,
        isolation_probability: f64,
        isolation_delay_period: f64,
    },
}

pub fn validate_guidance_policy(guidance_policy: Option<Policies>) -> Result<(), IxaError> {
    match guidance_policy {
        None => (),
        Some(Policies::UpdatedIsolationGuidance {
            post_isolation_duration,
            isolation_probability,
            isolation_delay_period,
        }) => {
            if post_isolation_duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The post-isolation duration must be non-negative.".to_string(),
                ));
            }
            if !(0.0..=1.0).contains(&isolation_probability) {
                return Err(IxaError::IxaError(
                    "The isolation probability must be between 0 and 1, inclusive.".to_string(),
                ));
            }
            if isolation_delay_period < 0.0 {
                return Err(IxaError::IxaError(
                    "The isolation delay period must be non-negative.".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        guidance_policy, ..
    } = context.get_params();

    match guidance_policy {
        None => (),
        Some(Policies::UpdatedIsolationGuidance { .. }) => {
            updated_guidance::init(context)?;
        }
    }
    Ok(())
}
