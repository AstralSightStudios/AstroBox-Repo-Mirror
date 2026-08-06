//! 设备通信模块：封装设备检查、应用启动、消息发送等操作
//!
//! 参考 Daymatter-AstroBox-Plugin 的通信模式：
//! check_device -> resolve_pkg -> launch -> wait -> send

use crate::astrobox::psys_host::{device, interconnect, register, thirdpartyapp};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 手表端认证器应用包名（默认值）
const DEFAULT_AUTH_PKG_NAME: &str = "com.whistleo.otp";
/// 目标应用名称关键词（用于在手表应用列表中查找）
const TARGET_APP_NAME_KEYWORD: &str = "OTP";
/// 设备缓存有效期（秒）
const CACHE_TTL_SECONDS: u64 = 30;
/// 启动应用后等待时间（秒）
const LAUNCH_WAIT_SECONDS: u64 = 2;

/// 设备信息缓存
struct DeviceCache {
    addr: String,
    pkg_name: String,
    cached_at: Instant,
}

static CACHE: Mutex<Option<DeviceCache>> = Mutex::new(None);

/// 获取首个已连接设备的地址
pub async fn check_device() -> Option<String> {
    // 优先使用缓存
    if let Some(cache) = get_cache() {
        return Some(cache.addr);
    }

    let devices = device::get_connected_device_list().await;
    devices.into_iter().next().map(|d| d.addr)
}

/// 从手表应用列表中解析目标快应用包名
pub async fn resolve_pkg_name(addr: &str) -> Option<String> {
    // 优先使用缓存
    if let Some(cache) = get_cache() {
        return Some(cache.pkg_name);
    }

    let apps = thirdpartyapp::get_thirdparty_app_list(addr).await.ok()?;

    // 优先按应用名关键词匹配
    if let Some(app) = apps.iter().find(|a| a.app_name.contains(TARGET_APP_NAME_KEYWORD)) {
        tracing::info!("resolve_pkg_name: found by name '{}'", app.package_name);
        update_cache(addr, &app.package_name);
        return Some(app.package_name.clone());
    }

    // 其次按默认包名匹配
    if let Some(app) = apps.iter().find(|a| a.package_name == DEFAULT_AUTH_PKG_NAME) {
        tracing::info!("resolve_pkg_name: found by pkg '{}'", app.package_name);
        update_cache(addr, &app.package_name);
        return Some(app.package_name.clone());
    }

    tracing::warn!("resolve_pkg_name: not found, using default");
    update_cache(addr, DEFAULT_AUTH_PKG_NAME);
    Some(DEFAULT_AUTH_PKG_NAME.to_string())
}

/// 启动手表端认证器应用并等待就绪
pub async fn launch_and_wait(addr: &str, pkg_name: &str) -> Result<(), String> {
    ensure_registered(addr, pkg_name).await;

    let apps = thirdpartyapp::get_thirdparty_app_list(addr)
        .await
        .map_err(|()| "获取手表应用列表失败".to_string())?;

    let app = apps
        .into_iter()
        .find(|a| a.package_name == pkg_name)
        .ok_or_else(|| format!("手表上未找到认证器应用 ({})", pkg_name))?;

    thirdpartyapp::launch_qa(addr, &app, "pages/home")
        .await
        .map_err(|()| "启动应用失败".to_string())?;

    tracing::info!("launch_and_wait: app launched, waiting {}s...", LAUNCH_WAIT_SECONDS);
    std::thread::sleep(Duration::from_secs(LAUNCH_WAIT_SECONDS));
    tracing::info!("launch_and_wait: wait done");

    Ok(())
}

/// 发送消息到手表端认证器应用
pub async fn send_to_watch(addr: &str, pkg_name: &str, payload: &str) -> Result<(), String> {
    ensure_registered(addr, pkg_name).await;

    match interconnect::send_qaic_message(addr, pkg_name, payload).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let err_detail = format!("{:?}", e);
            tracing::error!("send_to_watch FAILED addr={}, pkg={}, err={}", addr, pkg_name, err_detail);
            // 发送失败时清除缓存，下次重新查询
            clear_cache();
            Err(format!("发送失败(addr={}, pkg={}): {}", addr, pkg_name, err_detail))
        }
    }
}

/// 注册 interconnect 接收通道
pub async fn ensure_registered(addr: &str, pkg_name: &str) {
    match register::register_interconnect_recv(addr, pkg_name).await {
        Ok(()) => {
            tracing::info!("interconnect registered: addr={}, pkg={}", addr, pkg_name);
        }
        Err(err) => {
            let raw = format!("{:?}", err);
            let lower = raw.to_lowercase();
            if lower.contains("already") || lower.contains("exists") || lower.contains("duplicate") {
                tracing::info!("interconnect already registered: addr={}, pkg={}", addr, pkg_name);
            } else {
                tracing::warn!("register_interconnect_recv failed: {}", raw);
            }
        }
    }
}

/// 获取缓存的设备信息（如果未过期）
fn get_cache() -> Option<DeviceCache> {
    let cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.as_ref().and_then(|c| {
        if c.cached_at.elapsed().as_secs() < CACHE_TTL_SECONDS {
            Some(DeviceCache {
                addr: c.addr.clone(),
                pkg_name: c.pkg_name.clone(),
                cached_at: c.cached_at,
            })
        } else {
            None
        }
    })
}

/// 更新设备缓存
fn update_cache(addr: &str, pkg_name: &str) {
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = Some(DeviceCache {
        addr: addr.to_string(),
        pkg_name: pkg_name.to_string(),
        cached_at: Instant::now(),
    });
}

/// 清除设备缓存
fn clear_cache() {
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
}
