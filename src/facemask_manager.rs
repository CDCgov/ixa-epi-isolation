use ixa::{
    Context, 
    define_rng
};
use crate::transmission_manager::InfectiousStatusType;
use crate::intervention_manager::{
    ContextInterventionExt, 
    FacemaskStatusType
};

define_rng!(TransmissionRng);

pub fn init(context: &mut Context) {
    context.register_intervention(
        InfectiousStatusType::Susceptible,
        FacemaskStatusType::None,
        1.0,
    );
    context.register_intervention(
        InfectiousStatusType::Susceptible,
        FacemaskStatusType::Wearing,
        0.5,
    );
    context.register_intervention(
        InfectiousStatusType::Infectious,
        FacemaskStatusType::None,
        1.0,
    );
    context.register_intervention(
        InfectiousStatusType::Infectious,
        FacemaskStatusType::Wearing,
        0.25,
    );
}
