use serde::{Deserialize, Serialize};

pub const QUICK_APP_PACKAGE: &str = "com.shell.liangyi";
pub const HANDSHAKE: &str = "__hs__";
pub const HEARTBEAT: &str = "heartbeat";
pub const HEARTBEAT_ACK: &str = "heartbeatAck";
pub const REQUEST_SCREENSHOT_LIST: &str = "requestScreenshotList";
pub const REQUEST_SCREENSHOT_DATA: &str = "requestScreenshotData";
pub const SCREENSHOT_LIST_DATA: &str = "screenshotListData";
pub const SCREENSHOT_CHUNK_ACK: &str = "screenshotChunkAck";
pub const SCREENSHOT_CHUNK_START: &str = "screenshotChunkStart";
pub const SCREENSHOT_CHUNK_PART: &str = "screenshotChunkPart";
pub const SCREENSHOT_CHUNK_FINISH: &str = "screenshotChunkFinish";
pub const SCREENSHOT_SYNC_RESULT: &str = "screenshotSyncResult";
pub const SCREENSHOT_FETCH_SYNC_REQUEST: &str = "screenshotFetchSyncRequest";
pub const SCREENSHOT_FETCH_PROGRESS: &str = "screenshotFetchProgress";
pub const SCREENSHOT_FETCH_RESULT: &str = "screenshotFetchResult";
pub const REQUEST_RAW_LIST: &str = "requestRawList";
pub const REQUEST_RAW_DATA: &str = "requestRawData";
pub const RAW_LIST_DATA: &str = "rawListData";
pub const EXEC_COMMAND: &str = "execCommand";
pub const EXEC_ACK: &str = "execAck";
pub const EXEC_RESULT: &str = "execResult";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeeplinkActionRequest {
    pub action: String,
    #[serde(default)]
    pub cmd: String,
    #[serde(default)]
    pub panel: String,
    #[serde(default)]
    pub callback: String,
}

pub fn parse_deeplink_action_request(payload: &str) -> Result<DeeplinkActionRequest, String> {
    let payload = payload.trim();
    if payload.is_empty() {
        return Err("Deeplink data 为空".to_string());
    }

    if payload.starts_with('{') {
        let mut request = serde_json::from_str::<DeeplinkActionRequest>(payload)
            .map_err(|error| format!("Deeplink JSON 无效: {}", error))?;
        request.action = normalize_deeplink_action(&request.action);
        if request.action.is_empty() {
            return Err("Deeplink action 为空".to_string());
        }
        Ok(request)
    } else {
        Ok(DeeplinkActionRequest {
            action: normalize_deeplink_action(payload),
            cmd: String::new(),
            panel: String::new(),
            callback: String::new(),
        })
    }
}

fn normalize_deeplink_action(action: &str) -> String {
    action.trim().to_ascii_lowercase().replace('_', "-")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(rename = "sessionId", default, deserialize_with = "string_from_any")]
    pub session_id: String,
    #[serde(rename = "reqId", default, deserialize_with = "string_from_any")]
    pub req_id: String,
    #[serde(default)]
    pub cmd: String,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub exitcode: Option<i64>,
    #[serde(rename = "timedOut", default)]
    pub timed_out: bool,
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub screenshots: Vec<ScreenshotItem>,
    #[serde(rename = "shotId", default)]
    pub shot_id: String,
    #[serde(rename = "capturedAt", default)]
    pub captured_at: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub d: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(rename = "sentBytes", default)]
    pub sent_bytes: i64,
    #[serde(rename = "totalBytes", default)]
    pub total_bytes: i64,
    #[serde(rename = "rateKbps", default)]
    pub rate_kbps: f64,
    #[serde(default)]
    pub current: i64,
    #[serde(default)]
    pub done: i64,
    #[serde(default)]
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotItem {
    #[serde(rename = "shotId", default)]
    pub shot_id: String,
    #[serde(rename = "capturedAt", default)]
    pub captured_at: String,
    #[serde(rename = "capturedAtUnix", default)]
    pub captured_at_unix: i64,
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub source: String,
}

pub fn build_type_message(message_type: &str) -> String {
    serde_json::json!({ "type": message_type }).to_string()
}

pub fn build_request_screenshot_data(shot_id: &str) -> String {
    serde_json::json!({
        "type": REQUEST_SCREENSHOT_DATA,
        "shotId": shot_id,
        "chunkSize": 1536,
        "throttleMs": 12,
        "gcEvery": 4
    })
    .to_string()
}

pub fn build_request_raw_data(shot_id: &str) -> String {
    serde_json::json!({
        "type": REQUEST_RAW_DATA,
        "shotId": shot_id,
        "chunkSize": 1536,
        "throttleMs": 12,
        "gcEvery": 4
    })
    .to_string()
}

pub fn build_exec_command(req_id: &str, cmd: &str) -> String {
    serde_json::json!({
        "type": EXEC_COMMAND,
        "reqId": req_id,
        "cmd": cmd,
        "source": "astrobox-v2-plugin"
    })
    .to_string()
}

pub fn build_fetch_sync_request(
    session_id: &str,
    url: &str,
    screenshots: &[ScreenshotItem],
) -> String {
    let items = screenshots
        .iter()
        .map(|item| {
            serde_json::json!({
                "shotId": item.shot_id,
                "capturedAt": item.captured_at,
                "capturedAtUnix": item.captured_at_unix,
                "index": item.index
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "type": SCREENSHOT_FETCH_SYNC_REQUEST,
        "sessionId": session_id,
        "url": url,
        "screenshots": items
    })
    .to_string()
}

pub fn build_screenshot_chunk_ack(session_id: &str, phase: &str, index: i64, ok: bool) -> String {
    serde_json::json!({
        "type": SCREENSHOT_CHUNK_ACK,
        "sessionId": session_id,
        "phase": phase,
        "index": index,
        "ok": ok
    })
    .to_string()
}

pub fn build_handshake(count: i64) -> String {
    serde_json::json!({
        "type": HANDSHAKE,
        "count": count,
        "source": "astrobox-v2-plugin"
    })
    .to_string()
}

pub fn build_heartbeat_ack() -> String {
    serde_json::json!({
        "type": HEARTBEAT_ACK,
        "timestamp": current_unix_seconds()
    })
    .to_string()
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn string_from_any<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(text) => text,
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}
