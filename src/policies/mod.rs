use ixa::{Context, IxaError};
use serde::{Deserialize, Serialize};

use crate::parameters::{ContextParametersExt, Params};

pub mod previous_guidance;
pub mod updated_guidance;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum Policies {
    // Struct contain policy parameters for isolation guidance
    // Post-isolation duration, isolation probability, and maximum isolation delay.
    UpdatedIsolationGuidance {
        // duration an individual follows post-isolation precautions
        // in this implementation this refers to masking
        post_isolation_duration: f64,
        // probability an individual follows the isolation guidance policy
        policy_adherence: f64,
        // delay from symptom onset to when an individual starts following the isolation guidance policy
        isolation_delay_period: f64,
    },
    PreviousIsolationGuidance {
        // the minimum duration from symptom onset of the policy if the individuals has
        // a positive test results
        overall_policy_duration: f64,
        // the minimum required duration of isolation for individuals with mild symptoms
        mild_symptom_isolation_duration: f64,
        // the minimum required duration of isolation for individuals with moderate symptoms
        moderate_symptom_isolation_duration: f64,
        // the delay between an individual's first negative test and the subsequent retest
        delay_to_retest: f64,
        // probability an individual follows the isolation guidance policy
        policy_adherence: f64,
        // delay from symptom onset to when an individual starts following the isolation guidance policy
        isolation_delay_period: f64,
        // the sensitivity of the test used to determine if an individual is infectious
        test_sensitivity: f64,
    },
}

pub fn validate_guidance_policy(guidance_policy: Option<Policies>) -> Result<(), IxaError> {
    match guidance_policy {
        None => (),
        Some(Policies::UpdatedIsolationGuidance {
            post_isolation_duration,
            policy_adherence,
            isolation_delay_period,
        }) => {
            if post_isolation_duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The post-isolation duration must be non-negative.".to_string(),
                ));
            }
            if !(0.0..=1.0).contains(&policy_adherence) {
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
        Some(Policies::PreviousIsolationGuidance {
            overall_policy_duration,
            mild_symptom_isolation_duration,
            moderate_symptom_isolation_duration,
            delay_to_retest,
            policy_adherence,
            isolation_delay_period,
            test_sensitivity,
        }) => {
            if overall_policy_duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The duration from symptom onset must be non-negative.".to_string(),
                ));
            }
            if mild_symptom_isolation_duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The mild symptom isolation duration must be non-negative.".to_string(),
                ));
            }
            if moderate_symptom_isolation_duration < 0.0 {
                return Err(IxaError::IxaError(
                    "The moderate symptom isolation duration must be non-negative.".to_string(),
                ));
            }
            if delay_to_retest < 0.0 {
                return Err(IxaError::IxaError(
                    "The negative test isolation duration must be non-negative.".to_string(),
                ));
            }
            if !(0.0..=1.0).contains(&policy_adherence) {
                return Err(IxaError::IxaError(
                    "The isolation probability must be between 0 and 1, inclusive.".to_string(),
                ));
            }
            if isolation_delay_period < 0.0 {
                return Err(IxaError::IxaError(
                    "The isolation delay period must be non-negative.".to_string(),
                ));
            }
            if !(0.0..=1.0).contains(&test_sensitivity) {
                return Err(IxaError::IxaError(
                    "The test sensitivity must be between 0 and 1, inclusive.".to_string(),
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
        Some(Policies::PreviousIsolationGuidance { .. }) => {
            previous_guidance::init(context)?;
        }
    }
    Ok(())
}
