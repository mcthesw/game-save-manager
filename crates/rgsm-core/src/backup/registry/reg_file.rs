use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::fmt::Write as _;

use super::{
    RegistryData, RegistryError, RegistryKeyEntry, RegistryValue, normalize_registry_path,
};

const REG_FILE_HEADER: &str = "Windows Registry Editor Version 5.00";

pub fn serialize_reg_file(data: &RegistryData) -> Result<Vec<u8>, RegistryError> {
    let text = serialize_reg_text(data)?;
    Ok(encode_utf16le_with_bom(&text))
}

pub fn deserialize_reg_file(bytes: &[u8]) -> Result<RegistryData, RegistryError> {
    let text = decode_reg_file_text(bytes)?;
    deserialize_reg_text(&text)
}

fn serialize_reg_text(data: &RegistryData) -> Result<String, RegistryError> {
    let root_key = normalize_registry_path(&data.root_key);
    if root_key.is_empty() {
        return Err(RegistryError::InvalidRegFile(
            "registry root key cannot be empty".to_string(),
        ));
    }

    let mut out = String::new();
    writeln!(out, "{REG_FILE_HEADER}\r").expect("writing to String cannot fail");

    for entry in &data.entries {
        writeln!(out).expect("writing to String cannot fail");
        writeln!(out, "[{}]\r", entry_key_path(&root_key, &entry.subkey))
            .expect("writing to String cannot fail");
        for value in &entry.values {
            writeln!(out, "{}\r", serialize_value(value)?).expect("writing to String cannot fail");
        }
    }

    Ok(out)
}

fn encode_utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(2 + text.len() * 2);
    bytes.extend_from_slice(&[0xff, 0xfe]);
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn decode_reg_file_text(bytes: &[u8]) -> Result<String, RegistryError> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_utf16_units(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_utf16_units(rest, u16::from_be_bytes);
    }

    String::from_utf8(bytes.to_vec())
        .map_err(|e| RegistryError::InvalidRegFile(format!("invalid UTF-8 registry file: {e}")))
}

fn decode_utf16_units(
    bytes: &[u8],
    read_unit: fn([u8; 2]) -> u16,
) -> Result<String, RegistryError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(RegistryError::InvalidRegFile(
            "UTF-16 registry file has an odd byte length".to_string(),
        ));
    }

    let units = bytes
        .chunks_exact(2)
        .map(|chunk| read_unit([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|e| RegistryError::InvalidRegFile(format!("invalid UTF-16 registry file: {e}")))
}

fn entry_key_path(root_key: &str, subkey: &str) -> String {
    let subkey = normalize_registry_path(subkey);
    if subkey.is_empty() {
        root_key.to_string()
    } else {
        format!("{root_key}\\{subkey}")
    }
}

fn serialize_value(value: &RegistryValue) -> Result<String, RegistryError> {
    let (name, body) = match value {
        RegistryValue::Sz { name, data } => (
            serialize_value_name(name),
            format!("\"{}\"", escape_string(data)),
        ),
        RegistryValue::ExpandSz { name, data } => (
            serialize_value_name(name),
            format!("hex(2):{}", format_hex_bytes(&string_to_reg_sz_bytes(data))),
        ),
        RegistryValue::MultiSz { name, data } => (
            serialize_value_name(name),
            format!("hex(7):{}", format_hex_bytes(&multi_sz_to_bytes(data))),
        ),
        RegistryValue::Dword { name, data } => {
            (serialize_value_name(name), format!("dword:{data:08x}"))
        }
        RegistryValue::Qword { name, data } => (
            serialize_value_name(name),
            format!("hex(b):{}", format_hex_bytes(&data.to_le_bytes())),
        ),
        RegistryValue::Binary { name, data } => {
            let bytes = BASE64.decode(data)?;
            (
                serialize_value_name(name),
                format!("hex:{}", format_hex_bytes(&bytes)),
            )
        }
    };

    Ok(format!("{name}={body}"))
}

fn serialize_value_name(name: &str) -> String {
    if name.is_empty() {
        "@".to_string()
    } else {
        format!("\"{}\"", escape_string(name))
    }
}

fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn string_to_reg_sz_bytes(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in value.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn multi_sz_to_bytes(values: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        for unit in value.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn format_hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn deserialize_reg_text(text: &str) -> Result<RegistryData, RegistryError> {
    let lines = logical_lines(text);
    let mut iter = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with(';'));

    let Some(header) = iter.next() else {
        return Err(RegistryError::InvalidRegFile(
            "registry file is empty".to_string(),
        ));
    };
    if header != REG_FILE_HEADER {
        return Err(RegistryError::InvalidRegFile(format!(
            "missing {REG_FILE_HEADER} header"
        )));
    }

    let mut root_key: Option<String> = None;
    let mut current_entry: Option<RegistryKeyEntry> = None;
    let mut entries = Vec::new();

    for line in iter {
        if let Some(key) = parse_key_header(line)? {
            if let Some(entry) = current_entry.take() {
                entries.push(entry);
            }

            let normalized_key = normalize_registry_path(&key);
            let root = root_key.get_or_insert_with(|| normalized_key.clone());
            let subkey = relative_subkey(root, &normalized_key)?;
            current_entry = Some(RegistryKeyEntry {
                subkey,
                values: Vec::new(),
            });
            continue;
        }

        let Some(entry) = current_entry.as_mut() else {
            return Err(RegistryError::InvalidRegFile(
                "value appears before any registry key header".to_string(),
            ));
        };
        entry.values.push(parse_value_line(line)?);
    }

    if let Some(entry) = current_entry {
        entries.push(entry);
    }

    let root_key =
        root_key.ok_or_else(|| RegistryError::InvalidRegFile("missing key header".to_string()))?;

    Ok(RegistryData {
        format_version: 1,
        root_key,
        entries,
    })
}

fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim_end();
        if trimmed.ends_with('\\') {
            current.push_str(trimmed.trim_end_matches('\\').trim_end());
            continue;
        }

        current.push_str(line.trim());
        lines.push(std::mem::take(&mut current));
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

fn parse_key_header(line: &str) -> Result<Option<String>, RegistryError> {
    if !line.starts_with('[') {
        return Ok(None);
    }
    if !line.ends_with(']') {
        return Err(RegistryError::InvalidRegFile(format!(
            "invalid registry key header: {line}"
        )));
    }
    if line.starts_with("[-") {
        return Err(RegistryError::UnsupportedRegValueType(
            "key deletion entries are not supported".to_string(),
        ));
    }

    Ok(Some(line[1..line.len() - 1].to_string()))
}

fn relative_subkey(root: &str, key: &str) -> Result<String, RegistryError> {
    if key == root {
        return Ok(String::new());
    }

    let prefix = format!("{root}\\");
    key.strip_prefix(&prefix)
        .map(ToString::to_string)
        .ok_or_else(|| {
            RegistryError::InvalidRegFile(format!(
                "registry key '{key}' is outside root key '{root}'"
            ))
        })
}

fn parse_value_line(line: &str) -> Result<RegistryValue, RegistryError> {
    let (name, body) = parse_value_assignment(line)?;
    if let Some(rest) = body.strip_prefix('"') {
        let (data, trailing) = parse_quoted_after_open_quote(rest)?;
        ensure_blank(trailing, "string value")?;
        return Ok(RegistryValue::Sz { name, data });
    }

    let lower = body.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("dword:") {
        let data = u32::from_str_radix(hex, 16).map_err(|e| {
            RegistryError::InvalidRegFile(format!("invalid DWORD value '{body}': {e}"))
        })?;
        return Ok(RegistryValue::Dword { name, data });
    }

    if lower.starts_with("hex") {
        return parse_hex_value(name, body);
    }

    Err(RegistryError::UnsupportedRegValueType(body.to_string()))
}

fn parse_value_assignment(line: &str) -> Result<(String, &str), RegistryError> {
    if let Some(rest) = line.strip_prefix('@') {
        let rest = rest.trim_start();
        let Some(body) = rest.strip_prefix('=') else {
            return Err(RegistryError::InvalidRegFile(format!(
                "invalid default value assignment: {line}"
            )));
        };
        return Ok((String::new(), body.trim_start()));
    }

    let Some(rest) = line.strip_prefix('"') else {
        return Err(RegistryError::InvalidRegFile(format!(
            "invalid registry value name: {line}"
        )));
    };
    let (name, trailing) = parse_quoted_after_open_quote(rest)?;
    let trailing = trailing.trim_start();
    let Some(body) = trailing.strip_prefix('=') else {
        return Err(RegistryError::InvalidRegFile(format!(
            "missing '=' after registry value name: {line}"
        )));
    };

    Ok((name, body.trim_start()))
}

fn parse_quoted_after_open_quote(input: &str) -> Result<(String, &str), RegistryError> {
    let mut escaped = false;
    let mut out = String::new();

    for (index, ch) in input.char_indices() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Ok((out, &input[index + ch.len_utf8()..])),
            _ => out.push(ch),
        }
    }

    Err(RegistryError::InvalidRegFile(
        "unterminated quoted string".to_string(),
    ))
}

fn ensure_blank(trailing: &str, context: &str) -> Result<(), RegistryError> {
    if trailing.trim().is_empty() {
        Ok(())
    } else {
        Err(RegistryError::InvalidRegFile(format!(
            "unexpected trailing data after {context}: {trailing}"
        )))
    }
}

fn parse_hex_value(name: String, body: &str) -> Result<RegistryValue, RegistryError> {
    let Some((kind, bytes_text)) = body.split_once(':') else {
        return Err(RegistryError::InvalidRegFile(format!(
            "invalid hex registry value: {body}"
        )));
    };
    let bytes = parse_hex_bytes(bytes_text)?;
    match kind.to_ascii_lowercase().as_str() {
        "hex" => Ok(RegistryValue::Binary {
            name,
            data: BASE64.encode(bytes),
        }),
        "hex(2)" => Ok(RegistryValue::ExpandSz {
            name,
            data: decode_reg_sz_bytes(&bytes)?,
        }),
        "hex(7)" => Ok(RegistryValue::MultiSz {
            name,
            data: decode_multi_sz_bytes(&bytes)?,
        }),
        "hex(b)" => {
            let data = bytes
                .as_slice()
                .try_into()
                .map(u64::from_le_bytes)
                .map_err(|_| {
                    RegistryError::InvalidRegFile(format!(
                        "QWORD value must contain 8 bytes, got {}",
                        bytes.len()
                    ))
                })?;
            Ok(RegistryValue::Qword { name, data })
        }
        other => Err(RegistryError::UnsupportedRegValueType(other.to_string())),
    }
}

fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, RegistryError> {
    input
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            u8::from_str_radix(part, 16).map_err(|e| {
                RegistryError::InvalidRegFile(format!("invalid hex byte '{part}': {e}"))
            })
        })
        .collect()
}

fn decode_reg_sz_bytes(bytes: &[u8]) -> Result<String, RegistryError> {
    let mut units = decode_utf16le_bytes(bytes)?;
    while units.last() == Some(&0) {
        units.pop();
    }
    String::from_utf16(&units)
        .map_err(|e| RegistryError::InvalidRegFile(format!("invalid REG_SZ data: {e}")))
}

fn decode_multi_sz_bytes(bytes: &[u8]) -> Result<Vec<String>, RegistryError> {
    let units = decode_utf16le_bytes(bytes)?;
    let mut values = Vec::new();
    let mut start = 0;

    for (index, unit) in units.iter().enumerate() {
        if *unit != 0 {
            continue;
        }
        if index == start {
            break;
        }
        values.push(String::from_utf16(&units[start..index]).map_err(|e| {
            RegistryError::InvalidRegFile(format!("invalid REG_MULTI_SZ data: {e}"))
        })?);
        start = index + 1;
    }

    Ok(values)
}

fn decode_utf16le_bytes(bytes: &[u8]) -> Result<Vec<u16>, RegistryError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(RegistryError::InvalidRegFile(format!(
            "UTF-16 value has odd byte length {}",
            bytes.len()
        )));
    }

    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry_data() -> RegistryData {
        RegistryData {
            format_version: 1,
            root_key: "REGISTRY:HKEY_CURRENT_USER/Software/RGSM Test".to_string(),
            entries: vec![
                RegistryKeyEntry {
                    subkey: String::new(),
                    values: vec![
                        RegistryValue::Sz {
                            name: String::new(),
                            data: "default value".to_string(),
                        },
                        RegistryValue::ExpandSz {
                            name: "InstallDir".to_string(),
                            data: "%USERPROFILE%\\Game".to_string(),
                        },
                        RegistryValue::MultiSz {
                            name: "Profiles".to_string(),
                            data: vec!["alpha".to_string(), "beta".to_string()],
                        },
                        RegistryValue::Dword {
                            name: "Enabled".to_string(),
                            data: 1,
                        },
                        RegistryValue::Qword {
                            name: "Ticks".to_string(),
                            data: 9_999_999_999,
                        },
                        RegistryValue::Binary {
                            name: "Blob".to_string(),
                            data: BASE64.encode([0, 1, 2, 0xff]),
                        },
                    ],
                },
                RegistryKeyEntry {
                    subkey: "Child".to_string(),
                    values: vec![RegistryValue::Sz {
                        name: "Quoted".to_string(),
                        data: "a \"quote\" and slash \\".to_string(),
                    }],
                },
            ],
        }
    }

    #[test]
    fn registry_data_reg_file_roundtrip() {
        let original = sample_registry_data();
        let bytes = serialize_reg_file(&original).unwrap();
        assert!(bytes.starts_with(&[0xff, 0xfe]));

        let parsed = deserialize_reg_file(&bytes).unwrap();
        assert_eq!(
            parsed.root_key,
            "HKEY_CURRENT_USER\\Software\\RGSM Test".to_string()
        );
        assert_eq!(parsed.entries, original.entries);
    }

    #[test]
    fn parses_utf8_reg_file_with_continued_hex_lines() {
        let text = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\RGSM Test]
"Blob"=hex:00,01,\
  02,ff
"Enabled"=dword:0000002a
"Name"="Game"
"Path"=hex(2):25,00,55,00,53,00,45,00,52,00,50,00,52,00,4f,00,46,00,49,00,4c,00,45,00,25,00,00,00
"Profiles"=hex(7):61,00,00,00,62,00,00,00,00,00
"Ticks"=hex(b):ff,e3,0b,54,02,00,00,00
"#;

        let parsed = deserialize_reg_file(text.as_bytes()).unwrap();
        assert_eq!(parsed.root_key, "HKEY_CURRENT_USER\\Software\\RGSM Test");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(
            parsed.entries[0].values,
            vec![
                RegistryValue::Binary {
                    name: "Blob".to_string(),
                    data: BASE64.encode([0, 1, 2, 0xff]),
                },
                RegistryValue::Dword {
                    name: "Enabled".to_string(),
                    data: 42,
                },
                RegistryValue::Sz {
                    name: "Name".to_string(),
                    data: "Game".to_string(),
                },
                RegistryValue::ExpandSz {
                    name: "Path".to_string(),
                    data: "%USERPROFILE%".to_string(),
                },
                RegistryValue::MultiSz {
                    name: "Profiles".to_string(),
                    data: vec!["a".to_string(), "b".to_string()],
                },
                RegistryValue::Qword {
                    name: "Ticks".to_string(),
                    data: 9_999_999_999,
                },
            ]
        );
    }
}
