use flate2::read::GzDecoder;
use std::io::Read;
use url::Url;
use waki::bindings::wasi::http::{outgoing_handler, types as http_types};
use waki::bindings::wasi::io::streams::StreamError;

use super::state;

/// 构建认证请求头（用于需要认证的API）
pub fn auth_headers(include_content_type: bool) -> Result<Vec<(String, String)>, String> {
    let client_type = state::server_api_client_type()?;
    let api_key = state::server_api_key()?;

    let mut headers = vec![
        ("X-Client-Type".to_string(), client_type.to_string()),
        ("X-API-Key".to_string(), api_key.to_string()),
    ];

    if include_content_type {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    Ok(headers)
}

/// 构建基础请求头（不需要认证）
pub fn base_headers(include_content_type: bool) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if include_content_type {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    headers
}

/// 发送GET请求并返回JSON（需要认证）
pub fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let headers = auth_headers(false)?;
    let (status, body) = request_bytes("GET", url, &headers, None)?;
    tracing::info!("get_json status={}, body_len={}", status, body.len());
    log_body_preview("get_json", &body);
    parse_http_json_response(status, body)
}

/// 发送GET请求（不需要认证）
pub fn get_json_no_auth(url: &str) -> Result<serde_json::Value, String> {
    let headers = base_headers(false);
    let (status, body) = request_bytes("GET", url, &headers, None)?;
    tracing::info!("get_json_no_auth status={}, body_len={}", status, body.len());
    log_body_preview("get_json_no_auth", &body);
    parse_http_json_response(status, body)
}

/// 发送POST请求并返回JSON（需要认证）
pub fn post_json(url: &str, payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body = serde_json::to_vec(payload).map_err(|e| format!("请求序列化失败: {}", e))?;
    let headers = auth_headers(true)?;
    let (status, response_body) = request_bytes("POST", url, &headers, Some(&body))?;
    tracing::info!(
        "post_json status={}, body_len={}",
        status,
        response_body.len()
    );
    log_body_preview("post_json", &response_body);
    parse_http_json_response(status, response_body)
}

/// 发送POST请求（不需要认证）
pub fn post_json_no_auth(url: &str, payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body = serde_json::to_vec(payload).map_err(|e| format!("请求序列化失败: {}", e))?;
    let headers = base_headers(true);
    let (status, response_body) = request_bytes("POST", url, &headers, Some(&body))?;
    tracing::info!(
        "post_json_no_auth status={}, body_len={}",
        status,
        response_body.len()
    );
    log_body_preview("post_json_no_auth", &response_body);
    parse_http_json_response(status, response_body)
}

/// 发送POST请求并返回HTTP状态码和JSON（不需要认证）
/// 与 post_json_no_auth 不同：200和201均返回Ok，调用方可根据状态码区分处理
pub fn post_json_no_auth_with_status(
    url: &str,
    payload: &serde_json::Value,
) -> Result<(u16, serde_json::Value), String> {
    let body = serde_json::to_vec(payload).map_err(|e| format!("请求序列化失败: {}", e))?;
    let headers = base_headers(true);
    let (status, response_body) = request_bytes("POST", url, &headers, Some(&body))?;
    tracing::info!(
        "post_json_no_auth_with_status status={}, body_len={}",
        status,
        response_body.len()
    );
    log_body_preview("post_json_no_auth_with_status", &response_body);

    let decompressed = maybe_decompress(response_body)?;
    if decompressed.is_empty() {
        return Err("Empty response".to_string());
    }

    let json: serde_json::Value = serde_json::from_slice(&decompressed)
        .map_err(|e| format!("响应解析失败: {}", e))?;

    // 200 和 201 均视为成功，返回状态码供调用方判断
    if status == 200 || status == 201 {
        return Ok((status, json));
    }

    // 其他状态码提取错误信息
    if let Some(message) = json.get("message").and_then(|v| v.as_str()) {
        return Err(message.to_string());
    }
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        return Err(format!("接口错误: {}", code));
    }

    Err(format!("HTTP {}", status))
}

/// 解析HTTP JSON响应
fn parse_http_json_response(status: u16, body: Vec<u8>) -> Result<serde_json::Value, String> {
    let body = maybe_decompress(body)?;
    if body.is_empty() {
        return Err("Empty response".to_string());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("响应解析失败: {}", e))?;

    if status == 200 {
        return Ok(json);
    }

    if let Some(message) = json.get("message").and_then(|v| v.as_str()) {
        return Err(message.to_string());
    }
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        return Err(format!("接口错误: {}", code));
    }

    Err(format!("HTTP {}", status))
}

/// 发送HTTP请求
fn request_bytes(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    tracing::info!("request_bytes method={}, url={}", method, url);
    let url = Url::parse(url).map_err(|e| e.to_string())?;

    let header_entries: Vec<(String, Vec<u8>)> = if headers.is_empty() {
        Vec::new()
    } else {
        headers
            .iter()
            .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
            .collect()
    };

    let http_headers = http_types::Headers::from_list(&header_entries)
        .map_err(|e| format!("Headers::from_list failed: {:?}", e))?;
    let req = http_types::OutgoingRequest::new(http_headers);

    let http_method = match method {
        "POST" => http_types::Method::Post,
        "GET" => http_types::Method::Get,
        _ => return Err(format!("unsupported method: {}", method)),
    };

    req.set_method(&http_method)
        .map_err(|()| "failed to set method".to_string())?;

    let scheme = match url.scheme() {
        "https" => http_types::Scheme::Https,
        _ => http_types::Scheme::Http,
    };
    req.set_scheme(Some(&scheme))
        .map_err(|()| "failed to set scheme".to_string())?;

    let authority = url.authority();
    req.set_authority(Some(authority))
        .map_err(|()| "failed to set authority".to_string())?;

    let path = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    };
    req.set_path_with_query(Some(&path))
        .map_err(|()| "failed to set path".to_string())?;

    let options = http_types::RequestOptions::new();
    let outgoing_body = req
        .body()
        .map_err(|_| "outgoing request write failed".to_string())?;
    let maybe_stream = if let Some(body) = body {
        let stream = outgoing_body
            .write()
            .map_err(|_| "open body writer failed".to_string())?;
        stream
            .blocking_write_and_flush(body)
            .map_err(|e| format!("write body failed: {:?}", e))?;
        drop(stream);
        None
    } else {
        None
    };
    http_types::OutgoingBody::finish(outgoing_body, maybe_stream)
        .map_err(|_| "finish body failed".to_string())?;

    let future_response =
        outgoing_handler::handle(req, Some(options)).map_err(|e| format!("{:?}", e))?;
    let incoming_response = match future_response.get() {
        Some(result) => result.map_err(|()| "response already taken".to_string())?,
        None => {
            let pollable = future_response.subscribe();
            pollable.block();
            future_response
                .get()
                .ok_or_else(|| "response not available".to_string())?
                .map_err(|()| "response already taken".to_string())?
        }
    }
    .map_err(|e| format!("{:?}", e))?;

    let status = incoming_response.status();
    tracing::info!("request_bytes status={}", status);
    let incoming_body = incoming_response
        .consume()
        .map_err(|_| "missing body".to_string())?;
    let input_stream = incoming_body
        .stream()
        .map_err(|_| "failed to open body stream".to_string())?;

    let mut body = Vec::new();
    loop {
        match input_stream.blocking_read(1024 * 64) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Err(StreamError::Closed) => break,
            Err(e) => return Err(format!("read body failed: {:?}", e)),
        }
    }

    Ok((status, body))
}

fn log_body_preview(tag: &str, body: &[u8]) {
    if body.is_empty() {
        tracing::info!("{} body_preview: <empty>", tag);
        return;
    }
    let preview_len = body.len().min(400);
    let preview = String::from_utf8_lossy(&body[..preview_len]);
    tracing::info!("{} body_preview_utf8: {}", tag, preview);
}

fn maybe_decompress(body: Vec<u8>) -> Result<Vec<u8>, String> {
    if body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b {
        tracing::info!("detected gzip body, decompressing...");
        let mut decoder = GzDecoder::new(&body[..]);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip decompress failed: {}", e))?;
        return Ok(out);
    }

    Ok(body)
}