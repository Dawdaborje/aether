use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
pub enum IconType {
    #[default]
    Iconify,
    Lucide,
}

#[derive(Deserialize, Serialize)]
pub struct SettingTab {
    pub name: String,
    pub link: String,
    pub icon: String,
    pub icon_type: IconType,
    pub default: bool,
}

#[derive(Deserialize, Serialize)]
pub struct SettingGroup {
    pub name: String,
    pub tab: SettingTab,
    pub heading: Option<String>,
    pub icon: String,
    pub icon_type: IconType,
}

#[derive(Deserialize, Serialize, Default)]
pub enum FieldTypes {
    #[default]
    StringField,
    BooleanField,
    ListField,
    JSONField,
    NumberField,
    FloatField,
}

#[derive(Deserialize, Serialize, Default)]
pub struct SettingItem {
    pub label: String,
    pub value_boolean: Option<bool>,
    pub value_string: Option<String>,
    pub value_list: Option<Vec<String>>,
    pub value_json: String,
}
