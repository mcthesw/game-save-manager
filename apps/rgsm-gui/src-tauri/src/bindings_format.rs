//! Post-processing for the specta-generated `bindings.ts`.
//!
//! The default specta TypeScript emitter can leave trailing whitespace on some
//! lines, which then fails the frontend lint/format checks. Stripping it here as
//! an export formatter keeps the generated file clean without a Node toolchain
//! dependency at build time.

use std::path::Path;

/// Remove trailing spaces/tabs from every line of the generated file in place,
/// preserving the original `\n` / `\r\n` line endings.
pub fn strip_trailing_whitespace(path: &Path) -> std::io::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let mut output = String::with_capacity(content.len());

    for line in content.split_inclusive('\n') {
        let (body, ending) = line
            .strip_suffix("\r\n")
            .map(|body| (body, "\r\n"))
            .or_else(|| line.strip_suffix('\n').map(|body| (body, "\n")))
            .unwrap_or((line, ""));
        output.push_str(body.trim_end_matches([' ', '\t']));
        output.push_str(ending);
    }

    if output != content {
        std::fs::write(path, output)?;
    }

    Ok(())
}
