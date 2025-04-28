use ixa::{
    define_person_property, define_person_property_with_default, Context, ContextPeopleExt,
    IxaError,
};

use serde::Deserialize;
use std::path::PathBuf;

use crate::parameters::{ContextParametersExt, Params};
use crate::settings::{
    CensusTract, ContextSettingExt, Home, Itinerary, ItineraryEntry, School, Workplace,
};

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

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    let mut itinerary_person = Vec::<ItineraryEntry>::new();
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    if !workplace_string.is_empty() {
        itinerary_person
            .add_setting_to_itinerary::<Workplace>(context, workplace_string.parse()?)?;
    }
    if !school_string.is_empty() {
        itinerary_person.add_setting_to_itinerary::<School>(context, school_string.parse()?)?;
    }

    // Add the settings consistent across all individuals (Home, CensusTract) and rescale to 1.0
    itinerary_person
        .add_setting_to_itinerary::<Home>(context, home_id.parse()?)?
        .add_setting_to_itinerary::<CensusTract>(context, tract.parse()?)?;

    let person_id = context.add_person((Age, person_record.age))?;

    context.add_itinerary(person_id, itinerary_person)?;

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
    let Params {
        synth_population_file,
        ..
    } = context.get_params();
    load_synth_population(context, synth_population_file.clone())?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::{GlobalParams, ItineraryWriteFnType, RateFnType};
    use crate::settings::SettingId;
    use ixa::{ContextGlobalPropertiesExt, ContextPeopleExt};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn persist_tmp_csv(content: &String) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            initial_infections: 1,
            max_time: 100.0,
            seed: 0,
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            report_period: 1.0,
            synth_population_file: PathBuf::from("."),
            transmission_report_name: None,
            settings_properties: vec![],
            itinerary_fn_type: ItineraryWriteFnType::SplitEvenly,
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        context
    }

    #[test]
    fn check_synth_file_tract() {
        let mut context = setup();
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(1, context.query_people_count((Age, age[i])));
            assert_eq!(
                1,
                context
                    .get_setting_members::<Home>(SettingId::<Home>::new(home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(census_tract_id))
                .unwrap()
                .len()
        );
    }

    #[test]
    #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
    fn check_invalid_census_tract() {
        let mut context = setup();
        let input =
            String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
    }

    #[test]
    fn check_synth_file_school() {
        let mut context = setup();
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let school_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(1, context.query_people_count((Age, age[i])));
            assert_eq!(
                1,
                context
                    .get_setting_members::<School>(SettingId::<School>::new(school_id[i]))
                    .unwrap()
                    .len()
            );
            assert_eq!(
                1,
                context
                    .get_setting_members::<Home>(SettingId::<Home>::new(home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(census_tract_id))
                .unwrap()
                .len()
        );
    }

    #[test]
    fn check_synth_file_workplace() {
        let mut context = setup();
        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let workplace_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_current_population(), 2);

        for i in 0..1 {
            assert_eq!(1, context.query_people_count(((Age, age[i]),)));
            assert_eq!(
                1,
                context
                    .get_setting_members::<Workplace>(SettingId::<Workplace>::new(workplace_id[i]))
                    .unwrap()
                    .len()
            );
            assert_eq!(
                1,
                context
                    .get_setting_members::<Home>(SettingId::<Home>::new(home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members::<CensusTract>(SettingId::<CensusTract>::new(census_tract_id))
                .unwrap()
                .len()
        );
    }
}
