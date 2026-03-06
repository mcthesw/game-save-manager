use std::borrow::Cow;

#[cfg(feature = "bundled-ludusavi-manifest")]
use rust_embed::Embed;

#[cfg(feature = "bundled-ludusavi-manifest")]
#[derive(Embed)]
#[folder = "resources/"]
#[include = "ludusavi_manifest.yaml"]
#[include = "ludusavi_manifest.meta.json"]
struct BundledResources;

#[cfg(feature = "bundled-ludusavi-manifest")]
fn load_bytes(path: &str) -> Option<Cow<'static, [u8]>> {
    BundledResources::get(path).map(|file| file.data)
}

#[cfg(not(feature = "bundled-ludusavi-manifest"))]
fn load_bytes(_path: &str) -> Option<Cow<'static, [u8]>> {
    None
}

fn load_utf8(path: &str) -> Option<Cow<'static, str>> {
    load_bytes(path).map(|bytes| match bytes {
        Cow::Borrowed(bytes) => Cow::Borrowed(
            std::str::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
        Cow::Owned(bytes) => Cow::Owned(
            String::from_utf8(bytes)
                .unwrap_or_else(|e| panic!("Embedded resource {path} is not valid UTF-8: {e}")),
        ),
    })
}

pub fn ludusavi_manifest_yaml() -> Option<Cow<'static, str>> {
    load_utf8("ludusavi_manifest.yaml")
}

pub fn ludusavi_manifest_meta_json() -> Option<Cow<'static, str>> {
    load_utf8("ludusavi_manifest.meta.json")
}

pub fn ludusavi_manifest_yaml_len() -> Option<u64> {
    load_bytes("ludusavi_manifest.yaml").map(|bytes| bytes.len() as u64)
}
