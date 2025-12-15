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
