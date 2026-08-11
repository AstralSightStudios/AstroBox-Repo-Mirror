use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ui::state::PersistedApp;

/// 持久化配置文件路径
const CONFIG_PATH: &str = "./simplefetch.config.json";
pub const CURRENT_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize)]
pub struct OnDiskConfig {
    pub version: u32,
    #[serde(default)]
    pub apps: Vec<PersistedApp>,
}

impl Default for OnDiskConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            apps: Vec::new(),
        }
    }
}

/// 从磁盘加载配置，文件缺失或损坏时返回默认值
pub fn load_config() -> OnDiskConfig {
    let path = Path::new(CONFIG_PATH);
    if !path.exists() {
        tracing::info!("persist: no config file at {} (first run)", CONFIG_PATH);
        return OnDiskConfig::default();
    }

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!("persist: failed to read {}: {}", CONFIG_PATH, err);
            return OnDiskConfig::default();
        }
    };

    match serde_json::from_str::<OnDiskConfig>(&text) {
        Ok(c) => {
            tracing::info!("persist: loaded config ({} apps)", c.apps.len());
            c
        }
        Err(err) => {
            tracing::warn!("persist: failed to parse {}: {}", CONFIG_PATH, err);
            OnDiskConfig::default()
        }
    }
}

/// 原子写入配置到磁盘
pub fn save_config(config: &OnDiskConfig) {
    let text = match serde_json::to_string_pretty(config) {
        Ok(t) => t,
        Err(err) => {
            tracing::error!("persist: failed to serialize config: {}", err);
            return;
        }
    };

    let tmp_path = format!("{}.tmp", CONFIG_PATH);
    if let Err(err) = fs::write(&tmp_path, &text) {
        tracing::error!("persist: failed to write {}: {}", tmp_path, err);
        return;
    }
    if let Err(err) = fs::rename(&tmp_path, CONFIG_PATH) {
        tracing::warn!(
            "persist: rename failed ({}), falling back to direct write",
            err
        );
        if let Err(err) = fs::write(CONFIG_PATH, &text) {
            tracing::error!("persist: direct write also failed: {}", err);
        }
    }
}
