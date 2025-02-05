use ixa::{Context, PersonId};

pub trait Setting {
    fn get_name(&self) -> String;
    fn get_members(&self, context: &Context, setting_id: usize) -> Vec<PersonId>;
    fn get_person_setting_id(&self, context: &Context, person_id: PersonId) -> usize;
    fn set_person_setting_id(&self, context: &mut Context, person_id: PersonId, setting_id: usize);
}

#[macro_export]
macro_rules! define_setting {
    ($name:ident) => {
        paste::paste! {
            ixa::define_person_property!( [<$name SettingId>], usize);

            pub struct $name;
            impl $crate::settings::Setting for $name {
                fn get_name(&self) -> String {
                    stringify!($name).to_string()
                }
                fn get_members(&self, context: &Context, setting_id: usize, ) -> Vec<ixa::PersonId> {
                    context.query_people(([<$name SettingId>], setting_id))
                }
                fn get_person_setting_id(&self, context: &Context, person_id: ixa::PersonId) -> usize {
                    context.get_person_property(person_id, [<$name SettingId>])
                }
                fn set_person_setting_id(&self, context: &mut Context, person_id: ixa::PersonId, setting_id: usize) {
                    context.set_person_property(person_id, [<$name SettingId>], setting_id);
                }
            }
        }
    }
}

pub trait ContextSettingExt {
    fn get_person_setting_id<T: Setting>(&self, person_id: PersonId, setting: T) -> usize;
    fn set_person_setting_id<T: Setting>(
        &mut self,
        person_id: PersonId,
        setting: T,
        setting_id: usize,
    );
    fn get_settings_members<T: Setting>(&self, setting: T, setting_id: usize) -> Vec<PersonId>;
}

impl ContextSettingExt for Context {
    fn get_person_setting_id<T: Setting>(&self, person_id: PersonId, setting: T) -> usize {
        setting.get_person_setting_id(self, person_id)
    }
    fn set_person_setting_id<T: Setting>(
        &mut self,
        person_id: PersonId,
        setting: T,
        setting_id: usize,
    ) {
        setting.set_person_setting_id(self, person_id, setting_id);
    }
    fn get_settings_members<T: Setting>(&self, setting: T, setting_id: usize) -> Vec<PersonId> {
        setting.get_members(self, setting_id)
    }
}
