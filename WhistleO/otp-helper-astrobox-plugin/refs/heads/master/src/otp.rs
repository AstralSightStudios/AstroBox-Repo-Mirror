//! OTP 计算模块（TOTP / HOTP）
//!
//! 算法逻辑参考手表端 src/utils/otp.js

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// 解码 Base32 密钥（去除空白、连字符、验证长度和字符集）
pub fn decode_base32(secret: &str) -> Result<Vec<u8>, String> {
    let mut value: String = secret
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    value = value.replace('-', "").to_uppercase();

    if value.is_empty() {
        return Err("Base32 secret is required".to_string());
    }

    if !value.bytes().all(|b| BASE32_ALPHABET.contains(&b) || b == b'=') {
        return Err("Invalid Base32 secret".to_string());
    }

    value = value.trim_end_matches('=').to_string();

    if value.is_empty() {
        return Err("Invalid Base32 secret".to_string());
    }

    let remainder = value.len() % 8;
    if remainder == 1 || remainder == 3 || remainder == 6 {
        return Err("Invalid Base32 secret length".to_string());
    }

    let mut buffer: u64 = 0;
    let mut bits_left: u8 = 0;
    let mut bytes: Vec<u8> = Vec::new();

    for ch in value.chars() {
        let idx = BASE32_ALPHABET
            .iter()
            .position(|&b| b == ch as u8)
            .unwrap() as u64;
        buffer = (buffer << 5) | idx;
        bits_left += 5;

        while bits_left >= 8 {
            bytes.push(((buffer >> (bits_left - 8)) & 0xFF) as u8);
            bits_left -= 8;
        }
    }

    if bytes.is_empty() {
        return Err("Decoded Base32 secret is empty".to_string());
    }

    Ok(bytes)
}

fn counter_to_bytes(counter: u64) -> [u8; 8] {
    counter.to_be_bytes()
}

fn extract_code(hash: &[u8], digits: u32) -> String {
    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let binary = (((hash[offset] as u32 & 0x7f) << 24)
        | ((hash[offset + 1] as u32 & 0xff) << 16)
        | ((hash[offset + 2] as u32 & 0xff) << 8)
        | (hash[offset + 3] as u32 & 0xff)) as u64;

    let divisor = 10u64.pow(digits);
    let code = binary % divisor;
    format!("{:0digits$}", code, digits = digits as usize)
}

fn create_code_from_counter(
    secret_bytes: &[u8],
    counter: u64,
    digits: u32,
    algorithm: &str,
) -> Result<String, String> {
    let counter_bytes = counter_to_bytes(counter);
    match algorithm {
        "SHA1" => {
            let mut mac = HmacSha1::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC error: {:?}", e))?;
            mac.update(&counter_bytes);
            let hash = mac.finalize().into_bytes();
            Ok(extract_code(&hash, digits))
        }
        "SHA256" => {
            let mut mac = HmacSha256::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC error: {:?}", e))?;
            mac.update(&counter_bytes);
            let hash = mac.finalize().into_bytes();
            Ok(extract_code(&hash, digits))
        }
        "SHA512" => {
            let mut mac = HmacSha512::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC error: {:?}", e))?;
            mac.update(&counter_bytes);
            let hash = mac.finalize().into_bytes();
            Ok(extract_code(&hash, digits))
        }
        _ => Err(format!("Unsupported algorithm: {}", algorithm)),
    }
}

/// 生成 TOTP 验证码和剩余秒数
/// timestamp_secs 为 Unix 时间戳（秒）
pub fn generate_totp(
    secret: &str,
    digits: u32,
    period: u32,
    algorithm: &str,
    timestamp_secs: u64,
) -> Result<(String, u32), String> {
    let secret_bytes = decode_base32(secret)?;
    let counter = timestamp_secs / period as u64;
    let mut remaining = period - (timestamp_secs % period as u64) as u32;
    if remaining == 0 {
        remaining = period;
    }

    let code = create_code_from_counter(&secret_bytes, counter, digits, algorithm)?;
    Ok((format_code(&code), remaining))
}

/// 生成 HOTP 验证码
pub fn generate_hotp(secret: &str, digits: u32, algorithm: &str, counter: u64) -> Result<String, String> {
    let secret_bytes = decode_base32(secret)?;
    let code = create_code_from_counter(&secret_bytes, counter, digits, algorithm)?;
    Ok(format_code(&code))
}

/// 格式化验证码（6位中间加空格，8位中间加空格）
pub fn format_code(code: &str) -> String {
    match code.len() {
        6 => format!("{} {}", &code[..3], &code[3..]),
        8 => format!("{} {}", &code[..4], &code[4..]),
        _ => code.to_string(),
    }
}
