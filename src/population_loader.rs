use crate::parameters::Parameters;
use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use serde::Deserialize;
use std::path::PathBuf;

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
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;

    let person_id = context.add_person((
        (Age, person_record.age),
        (HomeId, home_id.parse()?),
        (CensusTract, tract.parse()?),
    ))?;

    Ok(person_id)
}

fn load_synth_population(context: &mut Context, synth_input_file: PathBuf) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(synth_input_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers().unwrap().clone();

    while reader.read_byte_record(&mut raw_record).unwrap() {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers))?;
        create_person_from_record(context, &record)?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_global_property_value(Parameters).unwrap();

    load_synth_population(context, parameters.synth_population_file.clone())?;
    context.index_property(Age);
    context.index_property(CensusTract);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn persist_tmp_csv(content: &String) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    #[test]
    fn check_synth_file_tract() {
        let mut context = Context::new();
        let input = String::from("age,homeId\n43,360930331020001\n42,360930331020002");
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let tract = [36_093_033_102, 36_093_033_102];

        for id in 0..1 {
            let person_id = context.get_person_id(id);
            assert_eq!(age[id], context.get_person_property(person_id, Age));
            assert_eq!(
                tract[id],
                context.get_person_property(person_id, CensusTract)
            );
        }
    }

    #[test]
    #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
    fn check_invalid_census_tract() {
        let mut context = Context::new();
        let input = String::from("age,homeId\n43,360930331\n42,360930331020002");
        let synth_file = persist_tmp_csv(&input);
        let _ = load_synth_population(&mut context, synth_file);
    }
}
