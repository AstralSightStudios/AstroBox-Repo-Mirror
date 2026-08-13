use std::sync::{OnceLock, RwLock};
use tracing::{info, warn};

const SETTINGS_FILE: &str = "api_settings.json";
const WEATHER_API_HOST: Option<&str> = option_env!("WEATHER_API_HOST");
const WEATHER_API_CLIENT_TYPE: Option<&str> = option_env!("WEATHER_API_CLIENT_TYPE");
const WEATHER_API_KEY: Option<&str> = option_env!("WEATHER_API_KEY");

/// 应用名称，用于服务端API
pub const APP_NAME: &str = "Eternal";

/// 快应用包名
pub const QA_PKG_NAME: &str = "moe.mcns.Eternal";

/// API服务器地址
pub const API_BASE_URL: &str = "https://api.b4qaq.cn";

fn default_bool_true() -> bool {
    true
}

/// 主Tab枚举
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    SyncData,
    CityManage,
    Background,
    Notice,
    Settings,
}

/// 公告信息
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NoticeInfo {
    pub id: u32,
    pub title: String,
    pub time: String,
    pub content: String,
    #[serde(default)]
    pub notice_type: String,
    #[serde(default)]
    pub pinned: bool,
}

/// 公告内容片段（支持文本、图片、二维码）
#[derive(Clone, Debug)]
pub enum NoticeSegment {
    Text { text: String },
    Image { url: String, alt: String },
    QrCode { url: String, alt: String },
}

/// 设备信息结构（字段名匹配设备API返回，允许非snake_case）
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
#[allow(non_snake_case)]
pub struct DeviceInfo {
    // 验证需要的字段
    pub product: String,
    pub deviceId: String,
    pub serial: String,
    // 屏幕信息
    pub screenWidth: u32,
    pub screenHeight: u32,
    pub screenShape: String,
    pub screenDensity: u32,
    // 设备基本信息
    pub deviceType: String,
    pub brand: String,
    pub model: String,
    // 蓝牙地址
    #[serde(default)]
    pub btAddr: String,
}

/// 城市信息
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CityInfo {
    pub name: String,
    pub lat: String,
    pub lon: String,
    #[serde(default)]
    pub adm1: String,
    #[serde(default)]
    pub adm2: String,
    #[serde(default)]
    pub country: String,
}

/// UI状态
pub struct UiState {
    pub root_element_id: Option<String>,
    pub current_tab: MainTab,
    pub settings_loaded: bool,

    // APIKey相关
    pub api_key: String,
    pub api_key_verified: bool,
    pub api_key_visible: bool, // APIKey是否可见（默认隐藏）
    pub device_info: Option<DeviceInfo>,
    pub server_device_info: Option<serde_json::Value>, // 从服务器获取的设备信息（含用量等）
    pub host_device_info: Option<(String, String)>, // 主机设备信息（name, addr），来自 device::get_connected_device_list()
    pub verification_status: VerificationStatus,

    // 同步设置
    pub sync_alerts_enabled: bool,
    pub selected_days: u32,

    // 城市管理
    pub city_list: Vec<CityInfo>,
    pub selected_city_index: Option<usize>,
    pub city_list_loading: bool, // 城市列表是否正在加载
    pub city_search_keyword: String, // 城市搜索关键词
    pub city_search_results: Vec<CityInfo>, // 搜索结果
    pub city_search_loading: bool, // 是否正在搜索
    pub city_search_range: String, // 搜索范围：world, china, japan
    pub city_search_number: u32, // 结果数量：5, 10, 15, 20
    pub search_results_expanded: bool, // 搜索结果是否展开

    // 选中位置信息
    pub selected_location_id: String,
    pub selected_location_name: String,
    pub selected_location_adm1: String,
    pub selected_location_adm2: String,
    pub selected_location_lat: String,
    pub selected_location_lon: String,

    // 同步状态
    pub last_sync_time_ms: u64,
    pub last_sync_location: String,
    pub sync_progress: SyncProgress,

    // 公告
    pub notice_list: Vec<NoticeInfo>,
    pub notice_loading: bool,

    // 背景图管理
    pub bg_supported: Vec<String>, // 支持自定义的全部天气背景名
    pub bg_installed: Vec<String>, // 当前已安装的背景名
    pub bg_loading: bool, // 是否正在加载背景信息
    pub bg_uploading: Option<String>, // 正在上传的背景名（None表示空闲）
    pub bg_layout_grid: bool, // 背景列表布局：true=网格，false=列表
    pub bg_upload: Option<BgUploadTask>, // 当前分块上传任务
    pub bg_deleting_all: bool, // 是否正在删除所有背景
    pub bg_chunk_size: usize, // 背景上传分片大小（base64字符数，4的倍数）：4096/8192/16384
    pub bg_pending_op: Option<String>, // 当前等待设备响应的操作类型（用于超时判断）
    pub bg_op_timer_id: Option<u64>, // 操作超时定时器ID
    pub pending_replace: Option<BgUploadTask>, // 替换场景：等旧文件删除后再开始上传的任务
}

/// 背景图分块上传任务
#[derive(Clone, Default)]
pub struct BgUploadTask {
    pub name: String,          // 天气背景名
    pub chunks: Vec<String>,   // base64 分片；非末片长度均为4的倍数
    pub total: usize,          // 总块数
    pub current: usize,      // 已发送块数（0-based，等于下一块索引）
    pub timer_id: Option<u64>, // 超时定时器ID
}

/// 同步进度状态
#[derive(Clone, Default)]
pub struct SyncProgress {
    pub syncing: bool,
    pub current_day: u32,
    pub total_days: u32,
    pub status_text: String,
}

/// 验证状态
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    NotStarted,
    CheckingDevice,
    GettingAPIKey,
    GettingDeviceInfo,
    WaitingPayment,
    VerifyingPayment,
    Verified,
    Failed,
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        let state = UiState {
            root_element_id: None,
            current_tab: MainTab::SyncData,
            settings_loaded: false,

            api_key: String::new(),
            api_key_verified: false,
            api_key_visible: false, // 默认隐藏APIKey
            device_info: None,
            server_device_info: None,
            host_device_info: None,
            verification_status: VerificationStatus::NotStarted,

            sync_alerts_enabled: default_bool_true(),
            selected_days: 7,

            city_list: Vec::new(),
            selected_city_index: None,
            city_list_loading: false,
            city_search_keyword: String::new(),
            city_search_results: Vec::new(),
            city_search_loading: false,
            city_search_range: String::new(),
            city_search_number: 10,
            search_results_expanded: true,

            selected_location_id: String::new(),
            selected_location_name: String::new(),
            selected_location_adm1: String::new(),
            selected_location_adm2: String::new(),
            selected_location_lat: String::new(),
            selected_location_lon: String::new(),

            last_sync_time_ms: 0,
            last_sync_location: String::new(),
            sync_progress: SyncProgress::default(),

            notice_list: Vec::new(),
            notice_loading: false,

            bg_supported: Vec::new(),
            bg_installed: Vec::new(),
            bg_loading: false,
            bg_uploading: None,
            bg_layout_grid: true,
            bg_upload: None,
            bg_deleting_all: false,
            bg_chunk_size: 16 * 1024, // 默认 16K
            bg_pending_op: None,
            bg_op_timer_id: None,
            pending_replace: None,
        };
        RwLock::new(state)
    })
}

/// 获取API基础地址（优先使用环境变量，否则使用默认地址）
pub fn server_api_base() -> &'static str {
    WEATHER_API_HOST
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(API_BASE_URL)
}

pub fn server_api_client_type() -> Result<&'static str, String> {
    let client_type = WEATHER_API_CLIENT_TYPE
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "WEATHER_API_CLIENT_TYPE 未配置".to_string())?;

    Ok(client_type)
}

pub fn server_api_key() -> Result<&'static str, String> {
    let api_key = WEATHER_API_KEY
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "WEATHER_API_KEY 未配置".to_string())?;

    Ok(api_key)
}

/// 存储的设置结构
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredApiSettings {
    #[serde(default = "default_bool_true")]
    sync_alerts_enabled: bool,
    #[serde(default)]
    selected_days: u32,
    #[serde(default)]
    selected_city_index: Option<usize>,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    city_list: Vec<CityInfo>,
    #[serde(default)]
    city_search_range: String,
    #[serde(default = "default_search_number")]
    city_search_number: u32,
    #[serde(default = "default_chunk_size")]
    bg_chunk_size: usize,
}

fn default_search_number() -> u32 {
    10
}

fn default_chunk_size() -> usize {
    16 * 1024
}

pub fn load_api_settings_once() {
    let should_load = {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.settings_loaded {
            false
        } else {
            state.settings_loaded = true;
            true
        }
    };

    if !should_load {
        return;
    }

    let has_api_key = match std::fs::read_to_string(SETTINGS_FILE) {
        Ok(content) => match serde_json::from_str::<StoredApiSettings>(&content) {
            Ok(stored) => {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.sync_alerts_enabled = stored.sync_alerts_enabled;
                state.selected_days = if stored.selected_days == 0 {
                    14
                } else {
                    stored.selected_days
                };
                state.selected_city_index = stored.selected_city_index;
                state.api_key = stored.api_key.clone();
                state.city_list = stored.city_list;

                // 加载搜索设置
                if !stored.city_search_range.is_empty() {
                    state.city_search_range = stored.city_search_range;
                }
                if stored.city_search_number > 0 {
                    state.city_search_number = stored.city_search_number;
                }
                // 加载背景分片大小（只允许 4K/8K/16K）
                if matches!(stored.bg_chunk_size, 4096 | 8192 | 16384) {
                    state.bg_chunk_size = stored.bg_chunk_size;
                }

                // 如果有APIKey，标记为已验证
                if !state.api_key.is_empty() {
                    state.api_key_verified = true;
                    state.verification_status = VerificationStatus::Verified;
                }

                if state.selected_city_index.is_none() && !state.city_list.is_empty() {
                    state.selected_city_index = Some(0);
                }
                info!("loaded api settings from disk");
                !stored.api_key.is_empty() // 返回是否有APIKey
            }
            Err(e) => {
                warn!("failed to parse api settings: {}", e);
                false
            }
        },
        Err(e) => {
            warn!("api settings not loaded: {}", e);
            false
        }
    };

    // 如果有APIKey，异步获取设备信息
    if has_api_key {
        wit_bindgen::block_on(async move {
            // 延迟一点确保UI已初始化
            std::thread::sleep(std::time::Duration::from_millis(100));
            super::event_handler::fetch_device_info_from_server();
        });
    }
}

pub fn save_all_settings() -> Result<(), String> {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stored = StoredApiSettings {
        sync_alerts_enabled: state.sync_alerts_enabled,
        selected_days: state.selected_days,
        selected_city_index: state.selected_city_index,
        api_key: state.api_key.clone(),
        city_list: state.city_list.clone(),
        city_search_range: state.city_search_range.clone(),
        city_search_number: state.city_search_number,
        bg_chunk_size: state.bg_chunk_size,
    };

    let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(SETTINGS_FILE, content).map_err(|e| e.to_string())?;
    Ok(())
}
