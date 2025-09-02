use ixa::{Context, IxaError};
use serde::{Deserialize, Serialize};
use crate::parameters::{Params, ContextParametersExt};

pub mod transmission_report;
pub mod prevalence_report;
pub mod incidence_report;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ReportType {
    PrevalenceReport {
        name: String,
        period: f64,
    },
    IncidenceReport {
        name: String,
        period: f64,
    },
    TransmissionReport {
        name: String
    },
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        reports,
        ..
    } = context.get_params().clone();

    for report in reports.iter() {
        match report {
            ReportType::PrevalenceReport { name, period } => {      
                if period < &0.0 {
                    return Err(IxaError::IxaError(
                        "The prevalence report writing period must be non-negative.".to_string(),
                    ));
                }
                prevalence_report::init(context, name.as_str(), *period)?;
            },
            ReportType::IncidenceReport { name, period } => {      
                if period < &0.0 {
                    return Err(IxaError::IxaError(
                        "The incidence report writing period must be non-negative.".to_string(),
                    ));
                }
                incidence_report::init(context, name.as_str(), *period)?;
            },
            ReportType::TransmissionReport { name } => transmission_report::init(context, name.as_str())?
        }
    }
    Ok(())
}