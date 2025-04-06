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
static CURRENT_DEVICE: OnceLock<Device> = OnceLock::new();

/// 获取当前设备的信息。
/// 首次调用时会生成 UUID 作为设备 ID，并获取系统主机名作为设备名。
/// 后续调用将返回缓存的设备信息。
pub fn get_current_device() -> &'static Device {
    CURRENT_DEVICE.get_or_init(|| {
        let device_id = machine_uid::get().expect("Failed to get machine ID");
        let device_name = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "Unknown Device".to_string()); // 如果获取主机名失败，使用默认名称

        log::info!(
            "Generated current device info: id={}, name={}",
            device_id,
            device_name
        ); // 使用英文记录日志

        Device {
            id: device_id,
            name: device_name,
        }
    })
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_device_returns_consistent_info() {
        // 多次调用应返回相同的设备信息（在同一次运行中）
        let device1 = get_current_device();
        let device2 = get_current_device();
        assert_eq!(device1.id, device2.id);
        assert_eq!(device1.name, device2.name);
        assert!(!device1.id.is_empty());
        assert!(!device1.name.is_empty());
        println!("Device ID: {}", device1.id); // 打印以供查看
        println!("Device Name: {}", device1.name);
    }
}
