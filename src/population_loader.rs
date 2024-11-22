use crate::parameters_loader::Parameters;
use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct PeopleRecord<'a> {
    age: u8,
    homeId: &'a [u8],
}

define_person_property!(Age, u8);
define_person_property!(HomeId, usize);
define_person_property_with_default!(Alive, bool, true);

define_person_property!(CensusTract, usize);

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<PersonId, IxaError> {
    let tract: usize = String::from_utf8(person_record.homeId[..11].to_owned())
        .expect("Home id should have 11 digits for tract + home id")
        .parse()
        .expect("Could not parse census tract");    
    let home_id: usize = String::from_utf8(person_record.homeId.to_owned())
        .expect("Could not read home id")
        .parse()
        .expect("Could not read home id");
    
    let person_id =
        context.add_person(((Age, person_record.age), (HomeId, home_id), (CensusTract, tract)))?;
    Ok(person_id)
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    let mut reader =
        csv::Reader::from_path(parameters.synth_population_file).expect("Failed to open file.");
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers().unwrap().clone();

    while reader.read_byte_record(&mut raw_record).unwrap() {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers)).expect("Failed to parse record.");
        create_person_from_record(context, &record)?;
    }
    context.index_property(Age);
    context.index_property(CensusTract);
    Ok(())
}
