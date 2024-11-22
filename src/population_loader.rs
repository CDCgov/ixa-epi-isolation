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
#[allow(non_snake_case)]
pub struct PeopleRecord {
    age: u8,
    homeId: usize,
}

define_person_property!(Age, u8);
define_person_property!(HomeId, usize);
define_person_property_with_default!(Alive, bool, true);

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
const CENSUS_MAX: usize = 1e15 as usize;

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
const CENSUS_MIN: usize = 1e14 as usize;

define_derived_property!(CensusTract, usize, [HomeId], |home_id| {
    if (CENSUS_MIN..CENSUS_MAX).contains(&home_id) {
        home_id / 10_000
    } else {
        0 //Err(IxaError::IxaError(String::from("Census tract invalid from homeId")))
    }
});

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

#[cfg(test)]

mod tests {
    use super::*;
    use ixa::{context::Context, people::ContextPeopleExt, random::ContextRandomExt};

    #[test]
    #[allow(clippy::inconsistent_digit_grouping)]
    fn test_create_person_from_record() {
        let mut context = Context::new();
        context.init_random(0);
        let record = PeopleRecord {
            age: 42,
            homeId: 36_09_30_33102_0005,
        };
        let person_id = create_person_from_record(&mut context, &record).unwrap();
        assert_eq!(context.get_person_property(person_id, Age), 42);
        assert_eq!(
            context.get_person_property(person_id, HomeId),
            36_09_30_33102_0005
        );
        assert!(context.get_person_property(person_id, Alive));
        assert_eq!(
            context.get_person_property(person_id, CensusTract),
            36_09_30_33102
        );
    }
}
