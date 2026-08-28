//! Standalone, typed translations for the Master Time user interface.
//!
//! This module intentionally has no crate or UI-framework dependencies. Callers
//! can use [`TranslationCatalog::text`] for typed keys, or [`TranslationCatalog::text_for`]
//! when reading a persisted/remote key. Unknown locales and keys have safe,
//! deterministic fallbacks and never expose arbitrary input as UI text.

/// Languages shipped with the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    SimplifiedChinese,
    TraditionalChinese,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

impl Language {
    /// Maps common BCP-47 language identifiers to a supported language.
    ///
    /// Regional English and Chinese identifiers are accepted. Unsupported or
    /// malformed values deliberately fall back to English.
    pub fn from_locale(locale: &str) -> Self {
        let normalized = locale.trim().replace('_', "-").to_ascii_lowercase();
        match normalized.as_str() {
            "zh-cn" | "zh-sg" | "zh-hans" => Self::SimplifiedChinese,
            "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => Self::TraditionalChinese,
            "en" | "en-us" | "en-gb" | "en-au" | "en-ca" => Self::English,
            _ => Self::English,
        }
    }
}

/// Stable identifiers for every static label shown by the desktop UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    MasterTime,
    Time,
    Server,
    Network,
    Calibration,
    Diagnostics,
    GlobalServers,
    Settings,
    StartPolling,
    StopPolling,
    PollingActive,
    PollingStopped,
    Active,
    SyncStatus,
    Metrics,
    History,
    Stratum,
    Offset,
    RoundTripDelay,
    RootDistance,
    ConfiguredServers,
    ConfiguredProfiles,
    ServerProfilesValidated,
    NtpMeasurementPollingStatus,
    SyncPolicy,
    EligibleForCorrection,
    DisplayOnly,
    Unsafe,
    ClockCalibration,
    BeginCalibration,
    CalibrationWaitsForBoundary,
    TimeUntilBoundary,
    MarkBoundary,
    StopCalibration,
    Expected,
    Marked,
    Difference,
    Early,
    Late,
    GlobalServerCatalog,
    Search,
    All,
    UseServer,
    PollingIntervalSeconds,
    SetDraft,
    AllowedRangeSeconds,
    Theme,
    System,
    Light,
    Dark,
    Language,
    AlwaysOnTop,
    Apply,
    Cancel,
    LocalSettings,
    Uptime,
    CpuCount,
    CpuUtilization,
    CurrentOffset,
    Delay,
    Status,
    Synchronized,
    Uncertain,
    Unavailable,
    Error,
    MissingTranslation,
    NoServerConfigured,
    InvalidPollingInterval,
    InvalidServerProfile,
    ServerProfileAlreadyExists,
    ServerProfileNotFound,
    SelectAnotherServer,
    InvalidNtpResponse,
    NtpTransportFailed,
    NtpMeasurementFailed,
    UnsupportedNtpResponseMode,
    MissingTimestamp,
    ClockBeforeNtpEpoch,
    NtpTimestampOutOfRange,
    PollingWorkerPanicked,
    EmptyServerName,
    InvalidServerName,
    EmptyHostname,
    InvalidHostname,
    InvalidHostnameLabel,
    PollIntervalTooShort,
    PollIntervalTooLong,
    ActiveServerOutOfBounds,
}

impl Key {
    /// Returns the stable, storage-safe identifier for this key.
    pub const fn id(self) -> &'static str {
        match self {
            Self::MasterTime => "master_time",
            Self::Time => "time",
            Self::Server => "server",
            Self::Network => "network",
            Self::Calibration => "calibration",
            Self::Diagnostics => "diagnostics",
            Self::GlobalServers => "global_servers",
            Self::Settings => "settings",
            Self::StartPolling => "start_polling",
            Self::StopPolling => "stop_polling",
            Self::PollingActive => "polling_active",
            Self::PollingStopped => "polling_stopped",
            Self::Active => "active",
            Self::SyncStatus => "sync_status",
            Self::Metrics => "metrics",
            Self::History => "history",
            Self::Stratum => "stratum",
            Self::Offset => "offset",
            Self::RoundTripDelay => "round_trip_delay",
            Self::RootDistance => "root_distance",
            Self::ConfiguredServers => "configured_servers",
            Self::ConfiguredProfiles => "configured_profiles",
            Self::ServerProfilesValidated => "server_profiles_validated",
            Self::NtpMeasurementPollingStatus => "ntp_measurement_polling_status",
            Self::SyncPolicy => "sync_policy",
            Self::EligibleForCorrection => "eligible_for_correction",
            Self::DisplayOnly => "display_only",
            Self::Unsafe => "unsafe",
            Self::ClockCalibration => "clock_calibration",
            Self::BeginCalibration => "begin_calibration",
            Self::CalibrationWaitsForBoundary => "calibration_waits_for_boundary",
            Self::TimeUntilBoundary => "time_until_boundary",
            Self::MarkBoundary => "mark_boundary",
            Self::StopCalibration => "stop_calibration",
            Self::Expected => "expected",
            Self::Marked => "marked",
            Self::Difference => "difference",
            Self::Early => "early",
            Self::Late => "late",
            Self::GlobalServerCatalog => "global_server_catalog",
            Self::Search => "search",
            Self::All => "all",
            Self::UseServer => "use_server",
            Self::PollingIntervalSeconds => "polling_interval_seconds",
            Self::SetDraft => "set_draft",
            Self::AllowedRangeSeconds => "allowed_range_seconds",
            Self::Theme => "theme",
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Language => "language",
            Self::AlwaysOnTop => "always_on_top",
            Self::Apply => "apply",
            Self::Cancel => "cancel",
            Self::LocalSettings => "local_settings",
            Self::Uptime => "uptime",
            Self::CpuCount => "cpu_count",
            Self::CpuUtilization => "cpu_utilization",
            Self::CurrentOffset => "current_offset",
            Self::Delay => "delay",
            Self::Status => "status",
            Self::Synchronized => "synchronized",
            Self::Uncertain => "uncertain",
            Self::Unavailable => "unavailable",
            Self::Error => "error",
            Self::MissingTranslation => "missing_translation",
            Self::NoServerConfigured => "no_server_configured",
            Self::InvalidPollingInterval => "invalid_polling_interval",
            Self::InvalidServerProfile => "invalid_server_profile",
            Self::ServerProfileAlreadyExists => "server_profile_already_exists",
            Self::ServerProfileNotFound => "server_profile_not_found",
            Self::SelectAnotherServer => "select_another_server",
            Self::InvalidNtpResponse => "invalid_ntp_response",
            Self::NtpTransportFailed => "ntp_transport_failed",
            Self::NtpMeasurementFailed => "ntp_measurement_failed",
            Self::UnsupportedNtpResponseMode => "unsupported_ntp_response_mode",
            Self::MissingTimestamp => "missing_timestamp",
            Self::ClockBeforeNtpEpoch => "clock_before_ntp_epoch",
            Self::NtpTimestampOutOfRange => "ntp_timestamp_out_of_range",
            Self::PollingWorkerPanicked => "polling_worker_panicked",
            Self::EmptyServerName => "empty_server_name",
            Self::InvalidServerName => "invalid_server_name",
            Self::EmptyHostname => "empty_hostname",
            Self::InvalidHostname => "invalid_hostname",
            Self::InvalidHostnameLabel => "invalid_hostname_label",
            Self::PollIntervalTooShort => "poll_interval_too_short",
            Self::PollIntervalTooLong => "poll_interval_too_long",
            Self::ActiveServerOutOfBounds => "active_server_out_of_bounds",
        }
    }
}

/// Text returned when an untrusted or stale key cannot be resolved.
pub const MISSING_TRANSLATION: &str = "Missing translation";

/// A language-bound translation catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranslationCatalog {
    language: Language,
}

impl TranslationCatalog {
    pub const fn new(language: Language) -> Self {
        Self { language }
    }

    pub const fn language(self) -> Language {
        self.language
    }

    /// Creates a catalog from a locale, falling back to English safely.
    pub fn for_locale(locale: &str) -> Self {
        Self::new(Language::from_locale(locale))
    }

    /// Looks up a compile-time-known key.
    pub const fn text(self, key: Key) -> &'static str {
        match self.language {
            Language::English => english(key),
            Language::SimplifiedChinese => simplified_chinese(key),
            Language::TraditionalChinese => traditional_chinese(key),
        }
    }

    /// Resolves an external key without rendering the input on failure.
    pub fn text_for(self, id: &str) -> &'static str {
        ALL_KEYS
            .iter()
            .find(|key| key.id() == id)
            .map_or(MISSING_TRANSLATION, |key| self.text(*key))
    }
}

/// Alias useful to callers that prefer the shorter catalog name.
pub type Catalog = TranslationCatalog;

const ALL_KEYS: &[Key] = &[
    Key::MasterTime,
    Key::Time,
    Key::Server,
    Key::Network,
    Key::Calibration,
    Key::Diagnostics,
    Key::GlobalServers,
    Key::Settings,
    Key::StartPolling,
    Key::StopPolling,
    Key::PollingActive,
    Key::PollingStopped,
    Key::Active,
    Key::SyncStatus,
    Key::Metrics,
    Key::History,
    Key::Stratum,
    Key::Offset,
    Key::RoundTripDelay,
    Key::RootDistance,
    Key::ConfiguredServers,
    Key::ConfiguredProfiles,
    Key::ServerProfilesValidated,
    Key::NtpMeasurementPollingStatus,
    Key::SyncPolicy,
    Key::EligibleForCorrection,
    Key::DisplayOnly,
    Key::Unsafe,
    Key::ClockCalibration,
    Key::BeginCalibration,
    Key::CalibrationWaitsForBoundary,
    Key::TimeUntilBoundary,
    Key::MarkBoundary,
    Key::StopCalibration,
    Key::Expected,
    Key::Marked,
    Key::Difference,
    Key::Early,
    Key::Late,
    Key::GlobalServerCatalog,
    Key::Search,
    Key::All,
    Key::UseServer,
    Key::PollingIntervalSeconds,
    Key::SetDraft,
    Key::AllowedRangeSeconds,
    Key::Theme,
    Key::System,
    Key::Light,
    Key::Dark,
    Key::Language,
    Key::AlwaysOnTop,
    Key::Apply,
    Key::Cancel,
    Key::LocalSettings,
    Key::Uptime,
    Key::CpuCount,
    Key::CpuUtilization,
    Key::CurrentOffset,
    Key::Delay,
    Key::Status,
    Key::Synchronized,
    Key::Uncertain,
    Key::Unavailable,
    Key::Error,
    Key::MissingTranslation,
    Key::NoServerConfigured,
    Key::InvalidPollingInterval,
    Key::InvalidServerProfile,
    Key::ServerProfileAlreadyExists,
    Key::ServerProfileNotFound,
    Key::SelectAnotherServer,
    Key::InvalidNtpResponse,
    Key::NtpTransportFailed,
    Key::NtpMeasurementFailed,
    Key::UnsupportedNtpResponseMode,
    Key::MissingTimestamp,
    Key::ClockBeforeNtpEpoch,
    Key::NtpTimestampOutOfRange,
    Key::PollingWorkerPanicked,
    Key::EmptyServerName,
    Key::InvalidServerName,
    Key::EmptyHostname,
    Key::InvalidHostname,
    Key::InvalidHostnameLabel,
    Key::PollIntervalTooShort,
    Key::PollIntervalTooLong,
    Key::ActiveServerOutOfBounds,
];

macro_rules! catalog {
    ($language:ident, $($key:ident => $text:expr),+ $(,)?) => {
        const fn $language(key: Key) -> &'static str {
            match key { $(Key::$key => $text,)+ }
        }
    };
}

catalog!(english,
    MasterTime => "Master Time", Time => "Time", Server => "Server", Network => "Network", Calibration => "Calibration", Diagnostics => "Diagnostics", GlobalServers => "Global Servers", Settings => "Settings", StartPolling => "Start polling", StopPolling => "Stop polling", PollingActive => "Polling active", PollingStopped => "Polling stopped", Active => "Active", SyncStatus => "Sync status", Metrics => "Metrics", History => "History", Stratum => "Stratum", Offset => "Offset", RoundTripDelay => "Round-trip delay", RootDistance => "Root distance", ConfiguredServers => "Configured servers", ConfiguredProfiles => "configured profile(s)", ServerProfilesValidated => "Server profiles are validated before they enter application state.", NtpMeasurementPollingStatus => "NTP measurement and polling status", SyncPolicy => "Sync policy", EligibleForCorrection => "eligible for correction", DisplayOnly => "display only", Unsafe => "unsafe", ClockCalibration => "Clock calibration", BeginCalibration => "Begin calibration", CalibrationWaitsForBoundary => "Calibration waits for the next minute boundary.", TimeUntilBoundary => "Time until boundary: {seconds} seconds", MarkBoundary => "Mark boundary", StopCalibration => "Stop calibration", Expected => "Expected", Marked => "Marked", Difference => "Difference", Early => "early", Late => "late", GlobalServerCatalog => "Global server catalog", Search => "Search", All => "All", UseServer => "Use server", PollingIntervalSeconds => "Polling interval (seconds)", SetDraft => "Set draft", AllowedRangeSeconds => "Allowed range: {minimum}–{maximum} seconds", Theme => "Theme", System => "System", Light => "Light", Dark => "Dark", Language => "Language", AlwaysOnTop => "Always on top", Apply => "Apply", Cancel => "Cancel", LocalSettings => "Local settings", Uptime => "Uptime", CpuCount => "CPU count", CpuUtilization => "CPU utilization", CurrentOffset => "Current offset", Delay => "Delay", Status => "Status", Synchronized => "Synchronized", Uncertain => "Uncertain", Unavailable => "Unavailable", Error => "Error", MissingTranslation => MISSING_TRANSLATION, NoServerConfigured => "No server is configured", InvalidPollingInterval => "Polling interval must be an integer", InvalidServerProfile => "invalid server profile", ServerProfileAlreadyExists => "server profile already exists", ServerProfileNotFound => "server profile {index} not found", SelectAnotherServer => "select another server before removing the active server", InvalidNtpResponse => "invalid NTP response", NtpTransportFailed => "NTP transport failed", NtpMeasurementFailed => "could not calculate NTP measurement", UnsupportedNtpResponseMode => "unsupported NTP response mode {mode}; expected server mode", MissingTimestamp => "missing {name} timestamp", ClockBeforeNtpEpoch => "local clock is before the NTP epoch", NtpTimestampOutOfRange => "local time cannot be represented as an NTP timestamp", PollingWorkerPanicked => "polling worker thread panicked", EmptyServerName => "server name cannot be empty", InvalidServerName => "server name contains a control character", EmptyHostname => "hostname cannot be empty", InvalidHostname => "hostname contains invalid characters", InvalidHostnameLabel => "hostname label is invalid", PollIntervalTooShort => "poll interval {actual:?} is shorter than minimum {minimum:?}", PollIntervalTooLong => "poll interval {actual:?} is longer than maximum {maximum:?}", ActiveServerOutOfBounds => "active server index {index} is invalid for {server_count} servers"
);

catalog!(simplified_chinese,
    MasterTime => "主时间", Time => "时间", Server => "服务器", Network => "网络", Calibration => "校准", Diagnostics => "诊断", GlobalServers => "全球服务器", Settings => "设置", StartPolling => "开始轮询", StopPolling => "停止轮询", PollingActive => "轮询进行中", PollingStopped => "轮询已停止", Active => "当前", SyncStatus => "同步状态", Metrics => "指标", History => "历史记录", Stratum => "层级", Offset => "偏移量", RoundTripDelay => "往返延迟", RootDistance => "根距离", ConfiguredServers => "已配置的服务器", ConfiguredProfiles => "个已配置的配置文件", ServerProfilesValidated => "服务器配置文件在进入应用状态前会经过验证。", NtpMeasurementPollingStatus => "NTP 测量和轮询状态", SyncPolicy => "同步策略", EligibleForCorrection => "可进行校正", DisplayOnly => "仅显示", Unsafe => "不安全", ClockCalibration => "时钟校准", BeginCalibration => "开始校准", CalibrationWaitsForBoundary => "校准等待下一个整分钟边界。", TimeUntilBoundary => "距离边界还有：{seconds} 秒", MarkBoundary => "标记边界", StopCalibration => "停止校准", Expected => "预期", Marked => "标记时间", Difference => "差值", Early => "提前", Late => "延迟", GlobalServerCatalog => "全球服务器目录", Search => "搜索", All => "全部", UseServer => "使用服务器", PollingIntervalSeconds => "轮询间隔（秒）", SetDraft => "设置草稿", AllowedRangeSeconds => "允许范围：{minimum}–{maximum} 秒", Theme => "主题", System => "系统", Light => "浅色", Dark => "深色", Language => "语言", AlwaysOnTop => "始终置顶", Apply => "应用", Cancel => "取消", LocalSettings => "本地设置", Uptime => "运行时间", CpuCount => "CPU 数量", CpuUtilization => "CPU 使用率", CurrentOffset => "当前偏移量", Delay => "延迟", Status => "状态", Synchronized => "已同步", Uncertain => "不确定", Unavailable => "不可用", Error => "错误", MissingTranslation => "缺少翻译", NoServerConfigured => "未配置服务器", InvalidPollingInterval => "轮询间隔必须是整数", InvalidServerProfile => "服务器配置文件无效", ServerProfileAlreadyExists => "服务器配置文件已存在", ServerProfileNotFound => "找不到服务器配置文件 {index}", SelectAnotherServer => "请先选择其他服务器，再移除当前服务器", InvalidNtpResponse => "NTP 响应无效", NtpTransportFailed => "NTP 传输失败", NtpMeasurementFailed => "无法计算 NTP 测量结果", UnsupportedNtpResponseMode => "不支持的 NTP 响应模式 {mode}；应为服务器模式", MissingTimestamp => "缺少 {name} 时间戳", ClockBeforeNtpEpoch => "本地时钟早于 NTP 纪元", NtpTimestampOutOfRange => "本地时间无法表示为 NTP 时间戳", PollingWorkerPanicked => "轮询工作线程发生崩溃", EmptyServerName => "服务器名称不能为空", InvalidServerName => "服务器名称包含控制字符", EmptyHostname => "主机名不能为空", InvalidHostname => "主机名包含无效字符", InvalidHostnameLabel => "主机名标签无效", PollIntervalTooShort => "轮询间隔 {actual:?} 短于最小值 {minimum:?}", PollIntervalTooLong => "轮询间隔 {actual:?} 长于最大值 {maximum:?}", ActiveServerOutOfBounds => "活动服务器索引 {index} 对 {server_count} 台服务器无效"
);

catalog!(traditional_chinese,
    MasterTime => "主時間", Time => "時間", Server => "伺服器", Network => "網路", Calibration => "校準", Diagnostics => "診斷", GlobalServers => "全球伺服器", Settings => "設定", StartPolling => "開始輪詢", StopPolling => "停止輪詢", PollingActive => "輪詢進行中", PollingStopped => "輪詢已停止", Active => "目前", SyncStatus => "同步狀態", Metrics => "指標", History => "歷史記錄", Stratum => "階層", Offset => "偏移量", RoundTripDelay => "往返延遲", RootDistance => "根距離", ConfiguredServers => "已設定的伺服器", ConfiguredProfiles => "個已設定的設定檔", ServerProfilesValidated => "伺服器設定檔在進入應用程式狀態前會經過驗證。", NtpMeasurementPollingStatus => "NTP 測量與輪詢狀態", SyncPolicy => "同步策略", EligibleForCorrection => "可進行校正", DisplayOnly => "僅顯示", Unsafe => "不安全", ClockCalibration => "時鐘校準", BeginCalibration => "開始校準", CalibrationWaitsForBoundary => "校準等待下一個整分鐘邊界。", TimeUntilBoundary => "距離邊界還有：{seconds} 秒", MarkBoundary => "標記邊界", StopCalibration => "停止校準", Expected => "預期", Marked => "標記時間", Difference => "差值", Early => "提前", Late => "延遲", GlobalServerCatalog => "全球伺服器目錄", Search => "搜尋", All => "全部", UseServer => "使用伺服器", PollingIntervalSeconds => "輪詢間隔（秒）", SetDraft => "設定草稿", AllowedRangeSeconds => "允許範圍：{minimum}–{maximum} 秒", Theme => "主題", System => "系統", Light => "淺色", Dark => "深色", Language => "語言", AlwaysOnTop => "永遠置頂", Apply => "套用", Cancel => "取消", LocalSettings => "本機設定", Uptime => "執行時間", CpuCount => "CPU 數量", CpuUtilization => "CPU 使用率", CurrentOffset => "目前偏移量", Delay => "延遲", Status => "狀態", Synchronized => "已同步", Uncertain => "不確定", Unavailable => "無法使用", Error => "錯誤", MissingTranslation => "缺少翻譯", NoServerConfigured => "未設定伺服器", InvalidPollingInterval => "輪詢間隔必須是整數", InvalidServerProfile => "伺服器設定檔無效", ServerProfileAlreadyExists => "伺服器設定檔已存在", ServerProfileNotFound => "找不到伺服器設定檔 {index}", SelectAnotherServer => "請先選擇其他伺服器，再移除目前的伺服器", InvalidNtpResponse => "NTP 回應無效", NtpTransportFailed => "NTP 傳輸失敗", NtpMeasurementFailed => "無法計算 NTP 測量結果", UnsupportedNtpResponseMode => "不支援的 NTP 回應模式 {mode}；應為伺服器模式", MissingTimestamp => "缺少 {name} 時間戳", ClockBeforeNtpEpoch => "本機時鐘早於 NTP 紀元", NtpTimestampOutOfRange => "本機時間無法表示為 NTP 時間戳", PollingWorkerPanicked => "輪詢工作執行緒發生當機", EmptyServerName => "伺服器名稱不可為空白", InvalidServerName => "伺服器名稱包含控制字元", EmptyHostname => "主機名稱不可為空白", InvalidHostname => "主機名稱包含無效字元", InvalidHostnameLabel => "主機名稱標籤無效", PollIntervalTooShort => "輪詢間隔 {actual:?} 短於最小值 {minimum:?}", PollIntervalTooLong => "輪詢間隔 {actual:?} 長於最大值 {maximum:?}", ActiveServerOutOfBounds => "目前伺服器索引 {index} 對 {server_count} 台伺服器無效"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_typed_key_is_present_in_all_catalogs() {
        for key in ALL_KEYS {
            if *key == Key::MissingTranslation {
                continue;
            }
            assert_ne!(
                TranslationCatalog::new(Language::English).text(*key),
                MISSING_TRANSLATION
            );
            assert_ne!(
                TranslationCatalog::new(Language::SimplifiedChinese).text(*key),
                MISSING_TRANSLATION
            );
            assert_ne!(
                TranslationCatalog::new(Language::TraditionalChinese).text(*key),
                MISSING_TRANSLATION
            );
        }
    }

    #[test]
    fn locales_and_unknown_values_fail_safe() {
        assert_eq!(Language::from_locale("zh_CN"), Language::SimplifiedChinese);
        assert_eq!(Language::from_locale("zh-TW"), Language::TraditionalChinese);
        assert_eq!(Language::from_locale("xx-YY"), Language::English);
        let catalog = TranslationCatalog::for_locale("unknown");
        assert_eq!(catalog.text(Key::Settings), "Settings");
        assert_eq!(catalog.text_for("not_a_key"), MISSING_TRANSLATION);
        assert_eq!(
            catalog.text_for("<script>alert(1)</script>"),
            MISSING_TRANSLATION
        );
    }

    #[test]
    fn keys_round_trip_through_external_identifiers() {
        let catalog = TranslationCatalog::new(Language::TraditionalChinese);
        assert_eq!(catalog.text_for(Key::Diagnostics.id()), "診斷");
        assert_eq!(
            catalog.text_for(Key::NtpTransportFailed.id()),
            "NTP 傳輸失敗"
        );
    }
}
