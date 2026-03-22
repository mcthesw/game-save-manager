use rust_embed::Embed;

#[derive(Embed)]
#[folder = "resources/"]
#[include = "ludusavi_manifest.yaml"]
#[include = "ludusavi_manifest.meta.json"]
struct BundledResources;

fn load_bytes(path: &str) -> std::borrow::Cow<'static, [u8]> {
    BundledResources::get(path)
        .unwrap_or_else(|| panic!("Missing embedded resource: {path}"))
        .data
}

fn load_utf8(path: &str) -> std::borrow::Cow<'static, str> {
    match load_bytes(path) {
        std::borrow::Cow::Borrowed(bytes) => std::borrow::Cow::Borrowed(
            std::str::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
        std::borrow::Cow::Owned(bytes) => std::borrow::Cow::Owned(
            String::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
    }
}

pub fn ludusavi_manifest_yaml() -> std::borrow::Cow<'static, str> {
    load_utf8("ludusavi_manifest.yaml")
}

pub fn ludusavi_manifest_meta_json() -> std::borrow::Cow<'static, str> {
    load_utf8("ludusavi_manifest.meta.json")
}

pub fn ludusavi_manifest_yaml_len() -> u64 {
    load_bytes("ludusavi_manifest.yaml").len() as u64
}

#[cfg(test)]
mod tests {
    use super::{ludusavi_manifest_meta_json, ludusavi_manifest_yaml, ludusavi_manifest_yaml_len};

    #[test]
    fn bundled_ludusavi_manifest_files_are_embedded() {
        assert!(ludusavi_manifest_yaml_len() > 0);

        let manifest_yaml = ludusavi_manifest_yaml();
        let manifest_meta = ludusavi_manifest_meta_json();

        let parsed_yaml: serde_yaml::Value = serde_yaml::from_str(manifest_yaml.as_ref())
            .expect("bundled manifest should be valid YAML");
        let parsed_meta: serde_json::Value = serde_json::from_str(manifest_meta.as_ref())
            .expect("bundled manifest metadata should be valid JSON");

        assert!(matches!(parsed_yaml, serde_yaml::Value::Mapping(_)));
        assert!(parsed_meta.is_object());
    }
}
