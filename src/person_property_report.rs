use crate::{
    infectiousness_manager::{InfectionData, InfectionDataValue, InfectionStatus, InfectionStatusValue}, parameters::{ContextParametersExt, Params}, population_loader::{Age, Alive}, symptom_progression::{SymptomValue, Symptoms}
};
use ixa::{
    define_data_plugin, define_derived_property, define_report, info, report::{ContextReportExt, Report}, Context, ContextPeopleExt, ExecutionPhase, HashSet, HashSetExt, IxaError, PersonId, PersonProperty, PersonPropertyChangeEvent, Tabulator
};
use serde::{Deserialize, Serialize};
use std::{any::{Any, TypeId}, cell::RefCell, collections::HashMap, string::ToString};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct PersonPropertyReport {
    t: f64,
    alive: bool,
    age: u8,
    symptoms: Option<SymptomValue>,
    infection_status: Option<InfectionStatusValue>,
    count: u64,
}

define_report!(PersonPropertyReport);

define_derived_property!(
    ReportProperties,
    (bool, u8, Option<SymptomValue>, InfectionStatusValue),
    [Alive, Age, Symptoms, InfectionStatus],
    |alive, age, symptoms, infection_status| (alive, age, symptoms, infection_status));


fn update_property_change(
    context: &mut Context,
    event: PersonPropertyChangeEvent<ReportProperties>) 
 {
     let previous_vec = vec![
         event.previous.0.to_string(),
         event.previous.1.to_string(),
         format!("{:?}", event.previous.2),
         format!("{:?}", event.previous.3)
     ];
     let current_vec = vec![
         event.current.0.to_string(),
         event.current.1.to_string(),
         format!("{:?}", event.current.2),
         format!("{:?}", event.current.3)
     ];

    let mut report_container_mut = context
        .get_data_container_mut(PropertyReportDataPlugin);

     *report_container_mut
        .person_property_report_container
        .entry(current_vec).or_insert(0) += 1;

     *report_container_mut
        .person_property_report_container
        .entry(previous_vec).or_insert(0) -= 1;     

}

fn send_report_counts<T: Tabulator + Clone + 'static> (context: &mut Context, _tabulator: T) {
    let report_container = context.get_data_container(PropertyReportDataPlugin).unwrap();
    
    let mut writer = context.get_writer(TypeId::of::<T>());
    for (values, count) in report_container.person_property_report_container.iter() {
        let mut row = vec![context.get_current_time().to_string()];
        row.extend(values.to_owned());
        row.push(count.to_string());        
        writer.write_record(&row).expect("Failed to write row");
    }
}

struct PropertyReportDataContainer {
    person_property_report_container: HashMap<Vec<String>, usize>,
}

define_data_plugin!(
    PropertyReportDataPlugin,
    PropertyReportDataContainer,
    PropertyReportDataContainer {
        person_property_report_container: HashMap::new(),
    }
);

fn initialize_person_property_report<T: Tabulator + Clone + 'static>(
    context: &mut Context,
    file_name: &str,
    period: f64,
    tabulator: T) -> Result<(), IxaError>
{        
    // Count initial number of people per property status
    context.add_report_by_type_id(TypeId::of::<T>(), file_name)?;
    // Write the header
    {
        let mut writer = context.get_writer(TypeId::of::<T>());
        let columns = tabulator.get_columns();
        let mut header = vec!["t".to_string()];
        header.extend(columns);
        header.push("count".to_string());
        writer
            .write_record(&header)
            .expect("Failed to write header");
    }
        
    // Compute initial counts
    let map_counts: RefCell<HashMap<Vec<String>, usize>> = RefCell::new(HashMap::new());
    context.tabulate_person_properties(&tabulator,  |context, values, count| {
        map_counts.borrow_mut().insert(values.to_vec(), count);            
    });
    let report_container = context.get_data_container_mut(PropertyReportDataPlugin);
    report_container.person_property_report_container = map_counts.borrow().clone();
    
    context.add_periodic_plan_with_phase(
        period,
        move |context: &mut Context| {
            send_report_counts(context, tabulator.clone());
        },
        ExecutionPhase::Last,
    );
    

    context.subscribe_to_event::<PersonPropertyChangeEvent<ReportProperties>>(|context, event| {       
        update_property_change(
            context,
            event,
        );
    });
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    let person_property_report_name = parameters.person_property_report_name.clone();
    let report_period = parameters.report_period.clone();
    match person_property_report_name {
        Some(path_name) => {
            initialize_person_property_report(
                context,                
                path_name.as_ref(), report_period,
                (Alive, Age, Symptoms, InfectionStatus),
            )?;
        }
        None => {
            info!("No property report name provided. Skipping report creation");
        }
    }
    Ok(())
}

