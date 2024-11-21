use crate::parameters_loader::Parameters;
use ixa::{
    context::Context,
    define_derived_property, define_person_property, define_person_property_with_default,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct PeopleRecord {
    age: u8,
    homeId: usize,
}

define_person_property!(Age, u8);
define_person_property!(HomeId, usize);
define_person_property_with_default!(Alive, bool, true);

define_derived_property!(CensusTract, usize, [HomeId], |home_id| { home_id / 10_000 });

pub fn create_new_person(context: &mut Context, person_record: &PeopleRecord) -> PersonId {
    let person = context
        .add_person(((Age, person_record.age),( HomeId, person_record.homeId)))
        .unwrap();
    person
}

pub fn init(context: &mut Context) {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    let record_dir = Path::new(file!()).parent().unwrap();

    let mut reader =
        csv::Reader::from_path(record_dir.join(parameters.synth_population_file)).unwrap();

    for result in reader.deserialize() {
        let record: PeopleRecord = result.expect("Failed to parse record.");
        create_new_person(context, &record);
    }
}
