use serde::{Deserialize, Serialize};
use specta::Type;

use crate::device::DeviceResourceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameDeviceBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_ids: Option<Vec<DeviceResourceId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_ids: Option<Vec<DeviceResourceId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_ids: Option<Vec<DeviceResourceId>>,
}

impl GameDeviceBinding {
    pub fn is_explicit(&self) -> bool {
        self.root_ids.is_some() || self.account_ids.is_some() || self.installation_ids.is_some()
    }
}
