use ixa::{
    define_person_property, define_person_property_with_default, Context, ContextPeopleExt,
    IxaError,
};

use serde::Deserialize;
use std::path::PathBuf;

use crate::parameters::ContextParametersExt;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct PeopleRecord<'a> {
    age: u8,
    homeId: &'a [u8],
    schoolId: &'a [u8],
    workplaceId: &'a [u8],
}

define_person_property!(Age, u8);
define_person_property_with_default!(Alive, bool, true);

define_person_property!(Household, usize);
define_person_property!(CensusTract, usize);
define_person_property!(SchoolId, Option<usize>);
define_person_property!(WorkplaceId, Option<usize>);

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    let school_id = if school_string.is_empty() {
        None
    } else {
        Some(school_string.parse()?)
    };

    let workplace_id = if workplace_string.is_empty() {
        None
    } else {
        Some(workplace_string.parse()?)
    };

    let _person_id = context.add_person((
        (Age, person_record.age),
        (Household, home_id.parse()?),
        (CensusTract, tract.parse()?),
        (SchoolId, school_id),
        (WorkplaceId, workplace_id),
    ))?;

    Ok(())
}

fn load_synth_population(context: &mut Context, synth_input_file: PathBuf) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(synth_input_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers))?;
        create_person_from_record(context, &record)?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let parameters = context.get_params();
    load_synth_population(context, parameters.synth_population_file.clone())
}

#[cfg(test)]
mod test {
    use super::*;
    use ixa::ContextPeopleExt;
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
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let tract = [36_093_033_102, 36_093_033_102];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(
                1,
                context.query_people_count((
                    (Age, age[i]),
                    (CensusTract, tract[i]),
                    (Household, home_id[i]),
                ))
            );
        }
    }

    #[test]
    #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
    fn check_invalid_census_tract() {
        let mut context = Context::new();
        let input =
            String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
    }

    #[test]
    fn check_synth_file_school() {
        let mut context = Context::new();
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let school_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(
                1,
                context.query_people_count((
                    (Age, age[i]),
                    (SchoolId, Some(school_id[i])),
                    (Household, home_id[i]),
                    (WorkplaceId, None)
                ))
            );
        }
    }

    #[test]
    fn check_synth_file_workplace() {
        let mut context = Context::new();
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let workplace_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(
                1,
                context.query_people_count((
                    (Age, age[i]),
                    (Household, home_id[i]),
                    (SchoolId, None),
                    (WorkplaceId, Some(workplace_id[i]))
                ))
            );
        }
    }
}
