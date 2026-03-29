use std::borrow::Cow;

#[cfg(feature = "bundled-manifest")]
use rust_embed::Embed;

#[cfg(feature = "bundled-manifest")]
#[derive(Embed)]
#[folder = "resources/"]
#[include = "ludusavi_manifest.yaml"]
#[include = "ludusavi_manifest.meta.json"]
struct BundledResources;

#[cfg(feature = "bundled-manifest")]
fn load_bytes(path: &str) -> Cow<'static, [u8]> {
    BundledResources::get(path)
        .unwrap_or_else(|| panic!("Missing embedded resource: {path}"))
        .data
}

#[cfg(feature = "bundled-manifest")]
fn load_utf8(path: &str) -> Cow<'static, str> {
    match load_bytes(path) {
        Cow::Borrowed(bytes) => Cow::Borrowed(
            std::str::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
        Cow::Owned(bytes) => Cow::Owned(
            String::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
    }
}

/// Whether this build includes the bundled Ludusavi manifest snapshot.
pub fn has_bundled_manifest() -> bool {
    cfg!(feature = "bundled-manifest")
}

#[cfg(feature = "bundled-manifest")]
pub fn ludusavi_manifest_yaml() -> Cow<'static, str> {
    load_utf8("ludusavi_manifest.yaml")
}

#[cfg(not(feature = "bundled-manifest"))]
pub fn ludusavi_manifest_yaml() -> Cow<'static, str> {
    Cow::Borrowed("")
}

#[cfg(feature = "bundled-manifest")]
pub fn ludusavi_manifest_meta_json() -> Cow<'static, str> {
    load_utf8("ludusavi_manifest.meta.json")
}

#[cfg(not(feature = "bundled-manifest"))]
pub fn ludusavi_manifest_meta_json() -> Cow<'static, str> {
    Cow::Borrowed("")
}

#[cfg(feature = "bundled-manifest")]
pub fn ludusavi_manifest_yaml_len() -> u64 {
    load_bytes("ludusavi_manifest.yaml").len() as u64
}

#[cfg(not(feature = "bundled-manifest"))]
pub fn ludusavi_manifest_yaml_len() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "bundled-manifest")]
    fn bundled_build_embeds_manifest_snapshot() {
        assert!(has_bundled_manifest());
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

    #[test]
    #[cfg(not(feature = "bundled-manifest"))]
    fn slim_build_omits_embedded_manifest() {
        assert!(!has_bundled_manifest());
        assert_eq!(ludusavi_manifest_yaml_len(), 0);
        assert!(ludusavi_manifest_yaml().is_empty());
        assert!(ludusavi_manifest_meta_json().is_empty());
    }
}
