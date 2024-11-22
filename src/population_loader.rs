use crate::parameters::Parameters;
use ixa::{
    context::Context,
    define_derived_property, define_person_property, define_person_property_with_default,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PeopleRecord {
    age: u8,
    homeId: usize,
}

define_person_property!(Age, u8);
define_person_property!(HomeId, usize);
define_person_property_with_default!(Alive, bool, true);

define_derived_property!(CensusTract, usize, [HomeId], |homeId| { homeId / 10_000 });

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<PersonId, IxaError> {
    let person_id =
        context.add_person(((Age, person_record.age), (HomeId, person_record.homeId)))?;
    Ok(person_id)
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    let mut reader =
        csv::Reader::from_path(parameters.synth_population_file).expect("Failed to open file.");

    for result in reader.deserialize() {
        let record: PeopleRecord = result.expect("Failed to parse record.");
        create_person_from_record(context, &record)?;
    }
    context.index_property(Age);
    context.index_property(CensusTract);
    Ok(())
}
