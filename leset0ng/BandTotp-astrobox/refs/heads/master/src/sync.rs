use astrobox_ng_wit::astrobox::psys_host::{device, dialog, interconnect, thirdpartyapp};
use serde::Serialize;

/// Parsed TOTP entry, matching the Android `TOTPInfo` data class.
#[derive(Serialize, Clone, Debug)]
pub struct TotpInfo {
    pub name: String,
    pub usr: String,
    pub key: String,
    pub algorithm: String,
    pub digits: u32,
    pub period: u32,
}

/// The payload shape sent to the wearable: `{ "list": [...] }`.
#[derive(Serialize, Clone, Debug)]
pub struct SyncPayload {
    pub list: Vec<TotpInfo>,
}

impl SyncPayload {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"list":[]}"#.to_string())
    }
}

/// Parse a single `otpauth://totp/...` URI into a `TotpInfo`.
pub fn parse_totp_uri(totp_uri: &str) -> Option<TotpInfo> {
    let trimmed = normalize_uri_candidate(totp_uri);
    if trimmed.is_empty() {
        return None;
    }

    let uri = url::Url::parse(&trimmed).ok()?;

    if uri.scheme() != "otpauth" || uri.host_str() != Some("totp") {
        return None;
    }

    let label = percent_decode(uri.path().trim_start_matches('/'));
    if label.trim().is_empty() {
        return None;
    }

    let (issuer_from_path, account) = match label.split_once(':') {
        Some((issuer, account)) if !account.trim().is_empty() => {
            (issuer.trim().to_string(), account.trim().to_string())
        }
        _ => (String::new(), label.trim().to_string()),
    };

    let secret = get_query_param(&uri, "secret")?;
    let issuer_from_query = get_query_param(&uri, "issuer");
    let name = issuer_from_query
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            if issuer_from_path.is_empty() {
                None
            } else {
                Some(issuer_from_path)
            }
        })
        .unwrap_or_else(|| account.clone());
    let algorithm = get_query_param(&uri, "algorithm").unwrap_or_else(|| "SHA1".to_string());
    let digits = get_query_param(&uri, "digits")
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let period = get_query_param(&uri, "period")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    Some(TotpInfo {
        name,
        usr: account,
        key: secret,
        algorithm,
        digits,
        period,
    })
}

fn get_query_param(uri: &url::Url, key: &str) -> Option<String> {
    uri.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

/// Parse a text block into a list of valid TOTP entries.
pub fn parse_totp_text(text: &str) -> Vec<TotpInfo> {
    extract_totp_uri_candidates(text)
        .into_iter()
        .filter_map(|candidate| parse_totp_uri(&candidate))
        .collect()
}

fn extract_totp_uri_candidates(text: &str) -> Vec<String> {
    let normalized = text.replace(r"\/", "/").replace("&amp;", "&");
    let mut candidates = Vec::new();

    for line in normalized.lines() {
        let line = line.trim();
        if line.starts_with("otpauth://totp/") {
            candidates.push(line.to_string());
        }
    }

    let mut rest = normalized.as_str();
    while let Some(start) = rest.find("otpauth://totp/") {
        let candidate_start = &rest[start..];
        let end = candidate_start
            .char_indices()
            .find_map(|(idx, ch)| is_uri_delimiter(ch).then_some(idx))
            .unwrap_or(candidate_start.len());
        candidates.push(candidate_start[..end].to_string());
        rest = &candidate_start[end..];
    }

    let mut unique = Vec::new();
    for candidate in candidates {
        let candidate = normalize_uri_candidate(&candidate);
        if !candidate.is_empty() && !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn is_uri_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>' | ',' | ')' | ']' | '}')
}

fn normalize_uri_candidate(candidate: &str) -> String {
    candidate
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '<' | '>' | ',' | ')' | ']' | '}'))
        .to_string()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[idx + 1]), hex_value(bytes[idx + 2])) {
                output.push((hi << 4) | lo);
                idx += 3;
                continue;
            }
        }
        output.push(bytes[idx]);
        idx += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Pick a plain-text file and return its contents as a string.
pub async fn pick_totp_file() -> Result<String, String> {
    let config = dialog::PickConfig {
        read: true,
        copy_to: None,
    };
    let filter = dialog::FilterConfig {
        multiple: false,
        extensions: vec![],
        default_directory: "".to_string(),
        default_file_name: "".to_string(),
    };

    let result = dialog::pick_file(&config, &filter).await;
    String::from_utf8(result.data).map_err(|e| format!("file is not valid UTF-8: {}", e))
}

/// Return the first currently connected wearable device, if any.
pub async fn first_connected_device() -> Result<device::DeviceInfo, String> {
    let devices = device::get_connected_device_list().await;
    devices.into_iter().next().ok_or_else(|| "no connected device".to_string())
}

/// Launch the BandTOTP quick app on the first connected device.
pub async fn launch_band_totp(app_pkg: &str) -> Result<String, String> {
    let dev = first_connected_device().await?;
    let apps = thirdpartyapp::get_thirdparty_app_list(&dev.addr)
        .await
        .map_err(|()| "device returned error while listing apps".to_string())?;

    let app = apps
        .into_iter()
        .find(|a| a.package_name == app_pkg || a.app_name == app_pkg)
        .ok_or_else(|| format!("quick app '{}' not found on {}", app_pkg, dev.name))?;

    thirdpartyapp::launch_qa(&dev.addr, &app, "pages/index")
        .await
        .map_err(|()| "failed to launch quick app".to_string())?;

    Ok(format!("opened {} on {}", app_pkg, dev.name))
}

/// Send the prepared JSON payload to the wearable quick app.
pub async fn send_payload(payload: &str, app_pkg: &str) -> Result<String, String> {
    let dev = first_connected_device().await?;
    interconnect::send_qaic_message(&dev.addr, app_pkg, payload)
        .await
        .map_err(|()| "device rejected message".to_string())?;

    Ok(format!("sent {} bytes to {}", payload.len(), dev.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_totp_uri_without_issuer_prefix() {
        let entry = parse_totp_uri(
            "otpauth://totp/alice%40example.com?secret=ABC123&issuer=Example",
        )
        .expect("URI without issuer prefix should parse");

        assert_eq!(entry.name, "Example");
        assert_eq!(entry.usr, "alice@example.com");
        assert_eq!(entry.key, "ABC123");
    }

    #[test]
    fn extracts_totp_uris_embedded_in_file_text() {
        let text = r#"
        exported = [
          "otpauth://totp/GitHub:alice?secret=AAA111&issuer=GitHub",
          "otpauth://totp/Bob?secret=BBB222&issuer=Mail"
        ]
        "#;

        let entries = parse_totp_text(text);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "GitHub");
        assert_eq!(entries[0].usr, "alice");
        assert_eq!(entries[1].name, "Mail");
        assert_eq!(entries[1].usr, "Bob");
    }

    #[test]
    fn does_not_duplicate_line_with_trailing_punctuation() {
        let text = "otpauth://totp/GitHub:alice?secret=AAA111&issuer=GitHub,";

        let entries = parse_totp_text(text);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "GitHub");
        assert_eq!(entries[0].usr, "alice");
    }
}
