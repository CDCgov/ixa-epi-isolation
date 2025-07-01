use ixa::{Context, IxaError};
use serde::{Deserialize, Serialize};

use crate::parameters::{ContextParametersExt, Params};

pub mod updated_guidance;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum GuidancePolicies {
    Updated,
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        guidance_policy, ..
    } = context.get_params();

    match guidance_policy {
        None => (),
        Some(GuidancePolicies::Updated) => updated_guidance::init(context)?,
    }
    Ok(())
}
