use crate::parameters::Parameters;
use ixa::{
    context::Context,
    define_derived_property, define_person_property, define_person_property_with_default,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct PeopleRecord {
    age: u8,
    homeId: usize,
}

define_person_property!(Age, u8);
define_person_property!(HomeId, usize);
define_person_property_with_default!(Alive, bool, true);

define_derived_property!(CensusTract, usize, [HomeId], |home_id| { home_id / 10000 });

pub fn create_person_from_record(context: &mut Context, person_record: &PeopleRecord) -> PersonId {
    context
        .add_person(((Age, person_record.age), (HomeId, person_record.homeId)))
        .unwrap()
}

pub fn init(context: &mut Context) {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    let mut reader = csv::Reader::from_path(parameters.synth_population_file.clone()).unwrap();

    for result in reader.deserialize() {
        let record: PeopleRecord = result.expect("Failed to parse record.");
        create_person_from_record(context, &record);
    }
    context.index_property(Age);
    context.index_property(CensusTract);
}
