use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::OnceLock;

use crate::path_pattern::StoreKind;

// 使用 String 作为设备 ID 的类型别名
pub type DeviceId = String;
pub type DeviceResourceId = u32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
#[serde(rename_all = "camelCase")]
pub enum DeviceResourceSource {
    Manual,
    Detected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DeviceResourceKind {
    GameRoot {
        store: StoreKind,
        path: String,
    },
    StoreAccount {
        store: StoreKind,
        #[serde(alias = "userId")]
        user_id: String,
    },
    GameInstallation {
        #[serde(alias = "rootId")]
        root_id: DeviceResourceId,
        store: StoreKind,
        #[serde(alias = "installDir")]
        install_dir: String,
        path: String,
        #[serde(
            default,
            alias = "storeGameId",
            skip_serializing_if = "Option::is_none"
        )]
        store_game_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
pub struct DeviceResource {
    pub id: DeviceResourceId,
    pub source: DeviceResourceSource,
    pub kind: DeviceResourceKind,
}

// 设备信息结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    #[serde(default)]
    pub resources: Vec<DeviceResource>,
    #[serde(default)]
    pub next_resource_id: DeviceResourceId,
}

impl Device {
    pub fn add_resource(
        &mut self,
        source: DeviceResourceSource,
        kind: DeviceResourceKind,
    ) -> DeviceResourceId {
        let next_after_existing = self
            .resources
            .iter()
            .map(|resource| resource.id)
            .max()
            .map_or(0, |id| id.saturating_add(1));
        let id = self.next_resource_id.max(next_after_existing);
        self.next_resource_id = self.next_resource_id.saturating_add(1);
        self.resources.push(DeviceResource { id, source, kind });
        id
    }

    pub fn resource(&self, id: DeviceResourceId) -> Option<&DeviceResource> {
        self.resources.iter().find(|resource| resource.id == id)
    }

    pub fn game_roots(&self) -> impl Iterator<Item = &DeviceResource> {
        self.resources
            .iter()
            .filter(|resource| matches!(resource.kind, DeviceResourceKind::GameRoot { .. }))
    }

    pub fn game_root_paths(&self) -> impl Iterator<Item = &str> {
        self.game_roots()
            .filter_map(|resource| match &resource.kind {
                DeviceResourceKind::GameRoot { path, .. } => Some(path.as_str()),
                _ => None,
            })
    }

    pub fn store_accounts(&self) -> impl Iterator<Item = &DeviceResource> {
        self.resources
            .iter()
            .filter(|resource| matches!(resource.kind, DeviceResourceKind::StoreAccount { .. }))
    }

    pub fn game_installations(&self) -> impl Iterator<Item = &DeviceResource> {
        self.resources
            .iter()
            .filter(|resource| matches!(resource.kind, DeviceResourceKind::GameInstallation { .. }))
    }
}

// 存储当前设备的静态变量，使用 OnceLock 确保只初始化一次
static CURRENT_DEVICE_ID: OnceLock<DeviceId> = OnceLock::new();

/// 获取当前设备的ID。
/// 首次调用时会生成 UUID 作为设备 ID。
/// 后续调用将返回缓存的设备ID。
pub fn get_current_device_id() -> &'static DeviceId {
    CURRENT_DEVICE_ID.get_or_init(|| machine_uid::get().expect("Failed to get machine ID"))
}

/// 获取当前系统的主机名
/// 如果无法获取，则返回"Unknown Device"作为默认值
pub fn get_system_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "Unknown Device".to_string())
}

impl Default for Device {
    fn default() -> Self {
        Self {
            id: machine_uid::get().expect("Failed to get machine ID"),
            name: get_system_hostname(),
            resources: Vec::new(),
            next_resource_id: 0,
        }
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_device_returns_consistent_info() {
        // 多次调用应返回相同的设备信息（在同一次运行中）
        let device1 = get_current_device_id();
        let device2 = get_current_device_id();
        assert_eq!(device1, device2);
        assert!(!device1.is_empty());
        println!("Device ID: {}", device1);
    }

    #[test]
    fn resource_ids_remain_stable_when_resource_details_change() {
        let mut device = Device {
            id: "device".to_string(),
            name: "Device".to_string(),
            resources: Vec::new(),
            next_resource_id: 0,
        };
        let id = device.add_resource(
            DeviceResourceSource::Manual,
            DeviceResourceKind::GameRoot {
                store: StoreKind::Other,
                path: "D:/Games".to_string(),
            },
        );

        let resource = device
            .resources
            .iter_mut()
            .find(|resource| resource.id == id)
            .unwrap();
        resource.kind = DeviceResourceKind::GameRoot {
            store: StoreKind::Other,
            path: "E:/Games".to_string(),
        };

        assert_eq!(device.resources[0].id, id);
        assert_eq!(device.next_resource_id, 1);
    }

    #[test]
    fn resource_fields_match_generated_binding_names() {
        let kind = DeviceResourceKind::GameInstallation {
            root_id: 7,
            store: StoreKind::Steam,
            install_dir: "Game".to_string(),
            path: "C:/Games/Game".to_string(),
            store_game_id: Some("42".to_string()),
        };

        let serialized = serde_json::to_value(kind).unwrap();
        assert_eq!(serialized.get("root_id"), Some(&serde_json::json!(7)));
        assert_eq!(
            serialized.get("install_dir"),
            Some(&serde_json::json!("Game"))
        );
        assert!(serde_json::from_value::<DeviceResourceKind>(serialized).is_ok());
        assert!(
            serde_json::from_value::<DeviceResourceKind>(serde_json::json!({
                "type": "gameInstallation",
                "rootId": 7,
                "store": "steam",
                "installDir": "Game",
                "path": "C:/Games/Game",
                "storeGameId": "42"
            }))
            .is_ok()
        );
    }
}
