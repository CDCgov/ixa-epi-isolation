use crate::parameters_loader::Parameters;
use ixa::{
    context::Context,
    define_person_property, define_person_property_with_default,
    error::IxaError,
    global_properties::ContextGlobalPropertiesExt,
    people::{ContextPeopleExt, PersonId},
};

use std::path::Path;
use std::path::PathBuf;
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

fn load_synth_population(context: &mut Context, synth_input_file: PathBuf) -> Result<(), IxaError>{
       let mut reader =
        csv::Reader::from_path(synth_input_file).expect("Failed to open file.");
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers().unwrap().clone();

    while reader.read_byte_record(&mut raw_record).unwrap() {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers)).expect("Failed to parse record.");
        create_person_from_record(context, &record)?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context
        .get_global_property_value(Parameters)
        .unwrap()
        .clone();

    load_synth_population(context, PathBuf::from(parameters.synth_population_file))?; 
    context.index_property(Age);
    context.index_property(CensusTract);
    Ok(())
}


#[cfg(test)]
mod test {
    use super::*;
    use tempfile::tempdir;
    use std::io::{Write, Read};
    use std::path::PathBuf;
    use std::fs::File;
    
    #[test]
    fn check_synth_file_error() {
        let mut context = Context::new();
        let temp_dir = tempdir().unwrap();
        let path = PathBuf::from(&temp_dir.path());
        let persisted_file = path.join("synth_pop_test.csv");
        let mut file = File::create(persisted_file).unwrap();
        file.write_all(b"
age,homeId
43,360930331020001
42,360930331020002").unwrap();
        let synth_file = path.join("synth_pop_test.csv");
        load_synth_population(&mut context, synth_file).unwrap();
        let age:u8 = 43;
        let tract:usize = 36093033102;

        assert_eq!(age, context.get_person_property(context.get_person_id(0), Age));
        assert_eq!(tract, context.get_person_property(context.get_person_id(0), CensusTract));
    }
}
