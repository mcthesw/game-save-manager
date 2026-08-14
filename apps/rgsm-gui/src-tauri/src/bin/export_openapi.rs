use std::{env, fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let output = env::args_os().nth(1).map(PathBuf::from).ok_or_else(|| {
        anyhow::anyhow!("usage: export-openapi <output-path> <default-config-path>")
    })?;
    let default_config_output = env::args_os().nth(2).map(PathBuf::from).ok_or_else(|| {
        anyhow::anyhow!("usage: export-openapi <output-path> <default-config-path>")
    })?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let document = std::thread::Builder::new()
        .name("openapi-export".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(rgsm_lib::openapi_json)?
        .join()
        .map_err(|_| anyhow::anyhow!("OpenAPI exporter panicked"))??;
    fs::write(output, document)?;

    if let Some(parent) = default_config_output.parent() {
        fs::create_dir_all(parent)?;
    }
    let default_config = serde_json::to_string_pretty(&rgsm_core::config::Config::default())?;
    fs::write(
        default_config_output,
        format!(
            "// This file is generated from Rust Config::default(). Do not edit.\n\nimport type {{ Config }} from './generated/types.gen';\n\nexport const DEFAULT_CONFIG: Config = {default_config};\n"
        ),
    )?;
    Ok(())
}
