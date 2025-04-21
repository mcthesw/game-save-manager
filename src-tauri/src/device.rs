use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::OnceLock;

// 使用 String 作为设备 ID 的类型别名
pub type DeviceId = String;

// 设备信息结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Type)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
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
}
