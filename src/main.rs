#[macro_use]
extern crate litcrypt;

use_litcrypt!();

mod system;

use std::collections::{HashMap, VecDeque};
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, Utc};
use eframe::egui;

const NTP_PORT: u16 = 123;

/// Seconds between the NTP epoch (1900-01-01) and the Unix epoch (1970-01-01).
const NTP_TIMESTAMP_DELTA: u64 = 2_208_988_800;

/// How often (in seconds) the background thread re-queries the NTP server.
const REFRESH_INTERVAL_SECS: u64 = 1;

/// How many recent ping samples to keep for the jitter sparkline.
const SPARKLINE_SAMPLES: usize = 60;

/// A server profile: a named NTP server with a hostname and an IP fallback.
#[derive(Clone)]
struct ServerProfile {
    name: String,
    hostname: String,
    ip: String,
}

impl ServerProfile {
    /// The default New Zealand Measurement Standards Laboratory profile.
    fn nz_default() -> Self {
        Self {
            name: lc!("NZ Measurement (MSL)").to_string(),
            hostname: lc!("pool.msltime.measurement.govt.nz").to_string(),
            ip: lc!("161.65.172.9").to_string(),
        }
    }
}

/// Geolocation info for an IP address (from a geolocation API).
#[derive(Clone, Default)]
struct GeoInfo {
    city: String,
    region: String,
    country: String,
    lat: f64,
    lon: f64,
    isp: String,
}

/// A known global NTP server for the Global Servers tab.
struct GlobalServer {
    name: &'static str,
    hostname: &'static str,
    ip: &'static str,
    strategy: &'static str,
    category: &'static str,
    notes: &'static str,
}

/// Curated list of major global time servers, grouped by category.
const GLOBAL_SERVERS: &[GlobalServer] = &[
    // ---- Corporate & Hyper-Scalers ----
    GlobalServer { name: "Google Public NTP", hostname: "time.google.com", ip: "216.239.35.0", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Anycast." },
    GlobalServer { name: "Google NTP 1", hostname: "time1.google.com", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Google NTP cluster." },
    GlobalServer { name: "Google NTP 2", hostname: "time2.google.com", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Google NTP cluster." },
    GlobalServer { name: "Google NTP 3", hostname: "time3.google.com", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Google NTP cluster." },
    GlobalServer { name: "Google NTP 4", hostname: "time4.google.com", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Google NTP cluster." },
    GlobalServer { name: "Amazon Time Sync", hostname: "time.aws.com", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Anycast." },
    GlobalServer { name: "Amazon Pool 0", hostname: "0.amazon.pool.ntp.org", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Amazon pool." },
    GlobalServer { name: "Amazon Pool 1", hostname: "1.amazon.pool.ntp.org", ip: "", strategy: "Leap smearing", category: "Corporate & Hyper-Scalers", notes: "Amazon pool." },
    GlobalServer { name: "Cloudflare Time", hostname: "time.cloudflare.com", ip: "162.159.200.1", strategy: "True UTC (NTS)", category: "Corporate & Hyper-Scalers", notes: "Supports Network Time Security (NTS)." },
    GlobalServer { name: "Microsoft", hostname: "time.windows.com", ip: "13.65.88.185", strategy: "Azure cluster", category: "Corporate & Hyper-Scalers", notes: "Windows OS default." },
    // ---- OS & Distribution Defaults ----
    GlobalServer { name: "Apple", hostname: "time.apple.com", ip: "17.253.20.45", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "macOS/iOS default." },
    GlobalServer { name: "Apple NTP 1", hostname: "time1.apple.com", ip: "", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "Apple NTP cluster." },
    GlobalServer { name: "Apple NTP 2", hostname: "time2.apple.com", ip: "", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "Apple NTP cluster." },
    GlobalServer { name: "Apple NTP 3", hostname: "time3.apple.com", ip: "", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "Apple NTP cluster." },
    GlobalServer { name: "Apple NTP 4", hostname: "time4.apple.com", ip: "", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "Apple NTP cluster." },
    GlobalServer { name: "Apple NTP 5", hostname: "time5.apple.com", ip: "", strategy: "Apple Anycast", category: "OS & Distribution Defaults", notes: "Apple NTP cluster." },
    GlobalServer { name: "Ubuntu", hostname: "ntp.ubuntu.com", ip: "91.189.91.157", strategy: "Canonical", category: "OS & Distribution Defaults", notes: "Canonical default NTP server." },
    GlobalServer { name: "Android", hostname: "time.android.com", ip: "", strategy: "Google NTP", category: "OS & Distribution Defaults", notes: "Google NTP service for Android." },
    GlobalServer { name: "GitHub Pool 0", hostname: "0.github.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "OS & Distribution Defaults", notes: "GitHub Enterprise default." },
    GlobalServer { name: "GitHub Pool 1", hostname: "1.github.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "OS & Distribution Defaults", notes: "GitHub Enterprise default." },
    // ---- National Laboratories & Government Standards (Stratum 1) ----
    GlobalServer { name: "NIST (USA)", hostname: "time.nist.gov", ip: "132.163.96.1", strategy: "Cesium Fountain", category: "National Laboratories & Government Standards (Stratum 1)", notes: "National Institute of Standards and Technology." },
    GlobalServer { name: "NIST A", hostname: "time-a.nist.gov", ip: "129.6.15.28", strategy: "Cesium Fountain", category: "National Laboratories & Government Standards (Stratum 1)", notes: "NIST, Gaithersburg, Maryland." },
    GlobalServer { name: "NIST B", hostname: "time-b.nist.gov", ip: "129.6.15.29", strategy: "Cesium Fountain", category: "National Laboratories & Government Standards (Stratum 1)", notes: "NIST, Gaithersburg, Maryland." },
    GlobalServer { name: "USNO Tick", hostname: "tick.usno.navy.mil", ip: "192.5.41.41", strategy: "GPS primary", category: "National Laboratories & Government Standards (Stratum 1)", notes: "U.S. Naval Observatory." },
    GlobalServer { name: "USNO Tock", hostname: "tock.usno.navy.mil", ip: "192.5.41.209", strategy: "GPS primary", category: "National Laboratories & Government Standards (Stratum 1)", notes: "U.S. Naval Observatory." },
    GlobalServer { name: "PTB 1 (Germany)", hostname: "ptbtime1.ptb.de", ip: "192.53.103.108", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "Physikalisch-Technische Bundesanstalt." },
    GlobalServer { name: "PTB 2 (Germany)", hostname: "ptbtime2.ptb.de", ip: "", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "Physikalisch-Technische Bundesanstalt." },
    GlobalServer { name: "NPL 1 (UK)", hostname: "ntp1.npl.co.uk", ip: "139.143.5.30", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "National Physical Laboratory." },
    GlobalServer { name: "NPL 2 (UK)", hostname: "ntp2.npl.co.uk", ip: "139.143.5.31", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "National Physical Laboratory." },
    GlobalServer { name: "NICT (Japan)", hostname: "ntp.nict.jp", ip: "", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "Operates five Stratum 1 servers." },
    GlobalServer { name: "IEN 1 (Italy)", hostname: "ntp1.ien.it", ip: "", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "Istituto Elettrotecnico Nazionale." },
    GlobalServer { name: "IEN 2 (Italy)", hostname: "ntp2.ien.it", ip: "", strategy: "Atomic clock", category: "National Laboratories & Government Standards (Stratum 1)", notes: "Istituto Elettrotecnico Nazionale." },
    // ---- NTP Pools ----
    GlobalServer { name: "Global Pool", hostname: "pool.ntp.org", ip: "anycast", strategy: "GeoDNS", category: "NTP Pools", notes: "Global pool." },
    GlobalServer { name: "Pool 0", hostname: "0.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Redundancy." },
    GlobalServer { name: "Pool 1", hostname: "1.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Redundancy." },
    GlobalServer { name: "Pool 2", hostname: "2.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Redundancy." },
    GlobalServer { name: "Pool 3", hostname: "3.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Redundancy." },
    GlobalServer { name: "US Pool", hostname: "us.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "Europe Pool", hostname: "europe.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "Asia Pool", hostname: "asia.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "Oceania Pool", hostname: "oceania.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "NZ Pool", hostname: "nz.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "AU Pool", hostname: "au.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "Brazil Pool", hostname: "br.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    GlobalServer { name: "South Africa Pool", hostname: "za.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "NTP Pools", notes: "Regional pool." },
    // ---- Vendor & Hardware Defaults ----
    GlobalServer { name: "Cisco SB 0", hostname: "0.ciscosb.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "Vendor & Hardware Defaults", notes: "Cisco devices." },
    GlobalServer { name: "Cisco SB 1", hostname: "1.ciscosb.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "Vendor & Hardware Defaults", notes: "Cisco devices." },
    GlobalServer { name: "Cisco SB 2", hostname: "2.ciscosb.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "Vendor & Hardware Defaults", notes: "Cisco devices." },
    GlobalServer { name: "Cisco SB 3", hostname: "3.ciscosb.pool.ntp.org", ip: "", strategy: "GeoDNS", category: "Vendor & Hardware Defaults", notes: "Cisco devices." },
    GlobalServer { name: "Netgear", hostname: "time.netgear.com", ip: "209.249.181.52", strategy: "Legacy", category: "Vendor & Hardware Defaults", notes: "Legacy vendor fallback." },
    GlobalServer { name: "D-Link", hostname: "time.dlink.com", ip: "195.113.144.238", strategy: "Legacy", category: "Vendor & Hardware Defaults", notes: "Legacy vendor fallback." },
];

/// Category filter for the Global servers tab.
#[derive(Clone, Copy, PartialEq)]
enum GlobalCategory {
    All,
    Corporate,
    OS,
    National,
    Pool,
    Vendor,
}

impl GlobalCategory {
    fn label(&self) -> &'static str {
        match self {
            GlobalCategory::All => "All",
            GlobalCategory::Corporate => "Corporate & Hyper-Scalers",
            GlobalCategory::OS => "OS & Distribution Defaults",
            GlobalCategory::National => {
                "National Laboratories & Government Standards (Stratum 1)"
            }
            GlobalCategory::Pool => "NTP Pools",
            GlobalCategory::Vendor => "Vendor & Hardware Defaults",
        }
    }
}

/// Supported UI languages.
#[derive(Clone, Copy, PartialEq)]
enum Language {
    English,
    ChineseSimplified,
    ChineseTraditional,
}

/// Supported UI themes.
#[derive(Clone, Copy, PartialEq)]
enum Theme {
    Light,
    Dark,
    Auto,
}

/// All user-facing strings, translated per language.
/// Clock synchronization status.
#[derive(Clone, Copy, PartialEq)]
enum ClockStatus {
    Synchronized,
    Unsynchronized,
    Estimated,
}

struct Strings {
    title: &'static str,
    always_on_top: &'static str,
    theme: &'static str,
    language: &'static str,
    server: &'static str,
    add: &'static str,
    edit: &'static str,
    remove: &'static str,
    active: &'static str,
    edit_profile: &'static str,
    name: &'static str,
    host: &'static str,
    ip: &'static str,
    save: &'static str,
    cancel: &'static str,
    waiting: &'static str,
    error: &'static str,
    deviation: &'static str,
    range: &'static str,
    jitter: &'static str,
    std_dev: &'static str,
    freq_error: &'static str,
    packet_loss: &'static str,
    clock_status: &'static str,
    stratum: &'static str,
    root_delay: &'static str,
    root_dispersion: &'static str,
    leap: &'static str,
    poll: &'static str,
    precision: &'static str,
    ref_id: &'static str,
    ref_time: &'static str,
    ref_time_na: &'static str,
    ping: &'static str,
    sent: &'static str,
    received: &'static str,
    requests: &'static str,
    utc: &'static str,
    local: &'static str,
    epoch: &'static str,
    origin: &'static str,
    receive: &'static str,
    transmit: &'static str,
    destination: &'static str,
    true_rtt: &'static str,
    true_offset: &'static str,
    ntp_version: &'static str,
    ntp_mode: &'static str,
    root_distance: &'static str,
    peer_state: &'static str,
    synchronized: &'static str,
    unsynchronized: &'static str,
    estimated: &'static str,
    leap_none: &'static str,
    leap_61: &'static str,
    leap_59: &'static str,
    leap_alarm: &'static str,
    stratum_unspec: &'static str,
    stratum_primary: &'static str,
    stratum_secondary: &'static str,
    stratum_unsync: &'static str,
    theme_light: &'static str,
    theme_dark: &'static str,
    theme_auto: &'static str,
    lang_english: &'static str,
    lang_chinese_simplified: &'static str,
    lang_chinese_traditional: &'static str,
    peer_sys: &'static str,
    unsafe_time: &'static str,
    ref_id_tooltip: &'static str,
    calibration: &'static str,
    cal_enable: &'static str,
    cal_countdown: &'static str,
    cal_mark: &'static str,
    cal_marked: &'static str,
    cal_hint: &'static str,
    net_identity: &'static str,
    server_ip: &'static str,
    your_ip: &'static str,
    server_loc: &'static str,
    your_loc: &'static str,
    coords: &'static str,
    isp: &'static str,
    peer_disp: &'static str,
    peer_warn: &'static str,
    tof: &'static str,
    kod_banner: &'static str,
    tab_time: &'static str,
    tab_server: &'static str,
    tab_network: &'static str,
    tab_calibration: &'static str,
    tab_settings: &'static str,
    time_format: &'static str,
    hour_12: &'static str,
    hour_24: &'static str,
    text_size: &'static str,
    size_small: &'static str,
    size_normal: &'static str,
    size_big: &'static str,
    drift_test: &'static str,
    drift_start: &'static str,
    drift_stop: &'static str,
    drift_duration: &'static str,
    drift_running: &'static str,
    drift_result: &'static str,
    drift_fast: &'static str,
    drift_slow: &'static str,
    drift_crystal: &'static str,
    hide_all: &'static str,
    show_all: &'static str,
    window_size: &'static str,
    fps: &'static str,
    visibility: &'static str,
    tab_global: &'static str,
    search: &'static str,
    hostname: &'static str,
    strategy: &'static str,
    category: &'static str,
    offset: &'static str,
    server_time: &'static str,
    device_time: &'static str,
    extensions: &'static str,
    extensions_none: &'static str,
    set_time: &'static str,
    set_time_launched: &'static str,
    set_time_failed: &'static str,
    set_time_na: &'static str,
}

impl Strings {
    fn for_language(lang: Language) -> Self {
        match lang {
            Language::English => Self::english(),
            Language::ChineseSimplified => Self::chinese_simplified(),
            Language::ChineseTraditional => Self::chinese_traditional(),
        }
    }

    fn english() -> Self {
        Self {
            title: "NTP Clock",
            always_on_top: "Always on top",
            theme: "Theme:",
            language: "Language:",
            server: "Server:",
            add: "Add",
            edit: "Edit",
            remove: "Remove",
            active: "Active: {} ({})",
            edit_profile: "Edit server profile",
            name: "Name:",
            host: "Host:",
            ip: "IP:",
            save: "Save",
            cancel: "Cancel",
            waiting: "Waiting for NTP response...",
            error: "Error: {}",
            deviation: "Deviation : {} (server vs system clock)",
            range: "Range     : {} .. {}",
            jitter: "Jitter          : {:.3} ms (ping std dev)",
            std_dev: "Std deviation   : {:.3} ms (offset stability)",
            freq_error: "Frequency error : {} (crystal drift)",
            packet_loss: "Packet loss     : {:.2}%",
            clock_status: "Clock status    : {}",
            stratum: "Stratum          : {} - {}",
            root_delay: "Root delay       : {:.3} ms",
            root_dispersion: "Root dispersion  : {:.3} ms",
            leap: "Leap indicator   : {} - {}",
            poll: "Poll interval    : {:.0} s",
            precision: "Precision        : {:.3} ms",
            ref_id: "Reference ID    : {}",
            ref_time: "Reference time  : {}",
            ref_time_na: "Reference time  : n/a",
            ping: "Ping      : {:.1} ms  (min {:.1} / avg {:.1} / max {:.1})",
            sent: "Sent      : {} total, {}",
            received: "Received  : {} total, {}",
            requests: "Requests  : {} sent, {} responses",
            utc: "UTC   : {}",
            local: "Local : {}",
            epoch: "Epoch : {}",
            origin: "Origin (T1)     : {}",
            receive: "Receive (T2)    : {}",
            transmit: "Transmit (T3)   : {}",
            destination: "Destination (T4): {}",
            true_rtt: "True RTT         : {:.3} ms",
            true_offset: "True offset      : {}",
            ntp_version: "NTP version      : {}",
            ntp_mode: "NTP mode         : {}",
            root_distance: "Root distance    : {:.3} ms",
            peer_state: "Peer state       : {}",
            synchronized: "Synchronized",
            unsynchronized: "Unsynchronized",
            estimated: "Estimated (fallback)",
            leap_none: "No warning",
            leap_61: "Last minute has 61 seconds",
            leap_59: "Last minute has 59 seconds",
            leap_alarm: "Alarm (clock not synchronized)",
            stratum_unspec: "Unspecified / Kiss-o'-Death",
            stratum_primary: "Primary reference (atomic clock / GPS)",
            stratum_secondary: "Secondary reference (stratum {})",
            stratum_unsync: "Unsynchronized",
            theme_light: "Light",
            theme_dark: "Dark",
            theme_auto: "Auto",
            lang_english: "English",
            lang_chinese_simplified: "简体中文",
            lang_chinese_traditional: "繁體中文",
            peer_sys: "Sys.Peer",
            unsafe_time: "⚠ Time unsafe (root distance > 1.5 s)",
            ref_id_tooltip: "Reference source: GNSS/GPS, CESM (caesium), RUBY (rubidium), or an upstream IP for stratum 2+.",
            calibration: "Calibration",
            cal_enable: "Beep at next minute",
            cal_countdown: "Next beep in {:.1} s",
            cal_mark: "Mark",
            cal_marked: "Marked at: {}",
            cal_hint: "Set your watch to the minute, wait for the beep, then press Mark.",
            net_identity: "Network identity & location",
            server_ip: "Server IP : {}",
            your_ip: "Your IP   : {}",
            server_loc: "Server location : {}",
            your_loc: "Your location : {}",
            coords: "Coordinates : {:.4}, {:.4}",
            isp: "ISP : {}",
            peer_disp: "Peer dispersion : {:.3} ms",
            peer_warn: "⚠ Time quality degrading (peer dispersion high)",
            tof: "Upstream : {:.1} ms | Downstream : {:.1} ms",
            kod_banner: "⚠ Server rejected request: {}",
            tab_time: "Time",
            tab_server: "Server",
            tab_network: "Network",
            tab_calibration: "Calibration",
            tab_settings: "Settings",
            time_format: "Time format:",
            hour_12: "12-hour",
            hour_24: "24-hour",
            text_size: "Text size:",
            size_small: "Small",
            size_normal: "Normal",
            size_big: "Big",
            drift_test: "Drift test",
            drift_start: "Start",
            drift_stop: "Stop",
            drift_duration: "Duration:",
            drift_running: "Measuring... {:.0}s remaining",
            drift_result: "Clock ran {:.1} ms {} over {:.0} s ({:+.1} ppm)",
            drift_fast: "fast",
            drift_slow: "slow",
            drift_crystal: "Estimated crystal error: {:+.1} ppm",
            hide_all: "Hide all",
            show_all: "Show all",
            window_size: "Window size: {} x {}",
            fps: "FPS: {:.0}",
            visibility: "Visibility",
            tab_global: "Global",
            search: "Search:",
            hostname: "Hostname",
            strategy: "Strategy",
            category: "Category",
            offset: "Offset",
            server_time: "Server time",
            device_time: "Device time",
            extensions: "Extensions",
            extensions_none: "None",
            set_time: "Set system time (elevated)",
            set_time_launched: "Elevation prompt launched.",
            set_time_failed: "Could not launch elevated process.",
            set_time_na: "No server time available yet.",
        }
    }

    fn chinese_simplified() -> Self {
        Self {
            title: "NTP 时钟",
            always_on_top: "始终置顶",
            theme: "主题：",
            language: "语言：",
            server: "服务器：",
            add: "添加",
            edit: "编辑",
            remove: "删除",
            active: "当前：{} ({})",
            edit_profile: "编辑服务器配置",
            name: "名称：",
            host: "主机：",
            ip: "IP：",
            save: "保存",
            cancel: "取消",
            waiting: "等待 NTP 响应...",
            error: "错误：{}",
            deviation: "偏差：{}（服务器与系统时钟）",
            range: "范围：{} .. {}",
            jitter: "抖动：{:.3} 毫秒（ping 标准差）",
            std_dev: "标准差：{:.3} 毫秒（偏移稳定性）",
            freq_error: "频率误差：{}（晶振漂移）",
            packet_loss: "丢包率：{:.2}%",
            clock_status: "时钟状态：{}",
            stratum: "层级：{} - {}",
            root_delay: "根延迟：{:.3} 毫秒",
            root_dispersion: "根分散：{:.3} 毫秒",
            leap: "闰秒指示：{} - {}",
            poll: "轮询间隔：{:.0} 秒",
            precision: "精度：{:.3} 毫秒",
            ref_id: "参考 ID：{}",
            ref_time: "参考时间：{}",
            ref_time_na: "参考时间：无",
            ping: "Ping：{:.1} 毫秒（最小 {:.1} / 平均 {:.1} / 最大 {:.1}）",
            sent: "发送：{} 总计，{}",
            received: "接收：{} 总计，{}",
            requests: "请求：{} 发送，{} 响应",
            utc: "UTC：{}",
            local: "本地：{}",
            epoch: "纪元：{}",
            origin: "起点 (T1)：{}",
            receive: "接收 (T2)：{}",
            transmit: "发送 (T3)：{}",
            destination: "终点 (T4)：{}",
            true_rtt: "真实 RTT：{:.3} 毫秒",
            true_offset: "真实偏移：{}",
            ntp_version: "NTP 版本：{}",
            ntp_mode: "NTP 模式：{}",
            root_distance: "根距离：{:.3} 毫秒",
            peer_state: "对等状态：{}",
            synchronized: "已同步",
            unsynchronized: "未同步",
            estimated: "估算（回退）",
            leap_none: "无警告",
            leap_61: "最后一分钟有 61 秒",
            leap_59: "最后一分钟有 59 秒",
            leap_alarm: "警报（时钟未同步）",
            stratum_unspec: "未指定 / Kiss-o'-Death",
            stratum_primary: "主参考（原子钟 / GPS）",
            stratum_secondary: "次参考（层级 {}）",
            stratum_unsync: "未同步",
            theme_light: "浅色",
            theme_dark: "深色",
            theme_auto: "自动",
            lang_english: "英语",
            lang_chinese_simplified: "简体中文",
            lang_chinese_traditional: "繁體中文",
            peer_sys: "系统对等",
            unsafe_time: "⚠ 时间不安全（根距离 > 1.5 秒）",
            ref_id_tooltip: "参考源：GNSS/GPS、CESM（铯）、RUBY（铷），或层级 2+ 的上游 IP。",
            calibration: "校准",
            cal_enable: "在下一分钟响铃",
            cal_countdown: "下次响铃在 {:.1} 秒",
            cal_mark: "标记",
            cal_marked: "标记时间：{}",
            cal_hint: "将手表设为该分钟，等待响铃，然后按标记。",
            net_identity: "网络身份与位置",
            server_ip: "服务器 IP：{}",
            your_ip: "您的 IP：{}",
            server_loc: "服务器位置：{}",
            your_loc: "您的位置：{}",
            coords: "坐标：{:.4}, {:.4}",
            isp: "ISP：{}",
            peer_disp: "对等分散：{:.3} 毫秒",
            peer_warn: "⚠ 时间质量下降（对等分散过高）",
            tof: "上行：{:.1} 毫秒 | 下行：{:.1} 毫秒",
            kod_banner: "⚠ 服务器拒绝请求：{}",
            tab_time: "时间",
            tab_server: "服务器",
            tab_network: "网络",
            tab_calibration: "校准",
            tab_settings: "设置",
            time_format: "时间格式：",
            hour_12: "12 小时",
            hour_24: "24 小时",
            text_size: "文字大小：",
            size_small: "小",
            size_normal: "正常",
            size_big: "大",
            drift_test: "漂移测试",
            drift_start: "开始",
            drift_stop: "停止",
            drift_duration: "时长：",
            drift_running: "测量中... 剩余 {:.0} 秒",
            drift_result: "时钟在 {:.0} 秒内{}了 {:.1} 毫秒（{:+.1} ppm）",
            drift_fast: "快",
            drift_slow: "慢",
            drift_crystal: "估算晶振误差：{:+.1} ppm",
            hide_all: "全部隐藏",
            show_all: "全部显示",
            window_size: "窗口大小：{} x {}",
            fps: "FPS：{:.0}",
            visibility: "可见性",
            tab_global: "全球",
            search: "搜索：",
            hostname: "主机名",
            strategy: "同步策略",
            category: "类别",
            offset: "偏移",
            server_time: "服务器时间",
            device_time: "设备时间",
            extensions: "扩展字段",
            extensions_none: "无",
            set_time: "设置系统时间（需提升权限）",
            set_time_launched: "已启动提升权限提示。",
            set_time_failed: "无法启动提升权限进程。",
            set_time_na: "尚无服务器时间。",
        }
    }

    fn chinese_traditional() -> Self {
        Self {
            title: "NTP 時鐘",
            always_on_top: "始終置頂",
            theme: "主題：",
            language: "語言：",
            server: "伺服器：",
            add: "新增",
            edit: "編輯",
            remove: "刪除",
            active: "目前：{} ({})",
            edit_profile: "編輯伺服器設定",
            name: "名稱：",
            host: "主機：",
            ip: "IP：",
            save: "儲存",
            cancel: "取消",
            waiting: "等待 NTP 回應...",
            error: "錯誤：{}",
            deviation: "偏差：{}（伺服器與系統時鐘）",
            range: "範圍：{} .. {}",
            jitter: "抖動：{:.3} 毫秒（ping 標準差）",
            std_dev: "標準差：{:.3} 毫秒（偏移穩定性）",
            freq_error: "頻率誤差：{}（晶振漂移）",
            packet_loss: "封包遺失率：{:.2}%",
            clock_status: "時鐘狀態：{}",
            stratum: "層級：{} - {}",
            root_delay: "根延遲：{:.3} 毫秒",
            root_dispersion: "根分散：{:.3} 毫秒",
            leap: "閏秒指示：{} - {}",
            poll: "輪詢間隔：{:.0} 秒",
            precision: "精確度：{:.3} 毫秒",
            ref_id: "參考 ID：{}",
            ref_time: "參考時間：{}",
            ref_time_na: "參考時間：無",
            ping: "Ping：{:.1} 毫秒（最小 {:.1} / 平均 {:.1} / 最大 {:.1}）",
            sent: "傳送：{} 總計，{}",
            received: "接收：{} 總計，{}",
            requests: "請求：{} 傳送，{} 回應",
            utc: "UTC：{}",
            local: "本機：{}",
            epoch: "紀元：{}",
            origin: "起點 (T1)：{}",
            receive: "接收 (T2)：{}",
            transmit: "傳送 (T3)：{}",
            destination: "終點 (T4)：{}",
            true_rtt: "真實 RTT：{:.3} 毫秒",
            true_offset: "真實偏移：{}",
            ntp_version: "NTP 版本：{}",
            ntp_mode: "NTP 模式：{}",
            root_distance: "根距離：{:.3} 毫秒",
            peer_state: "對等狀態：{}",
            synchronized: "已同步",
            unsynchronized: "未同步",
            estimated: "估算（回退）",
            leap_none: "無警告",
            leap_61: "最後一分鐘有 61 秒",
            leap_59: "最後一分鐘有 59 秒",
            leap_alarm: "警報（時鐘未同步）",
            stratum_unspec: "未指定 / Kiss-o'-Death",
            stratum_primary: "主參考（原子鐘 / GPS）",
            stratum_secondary: "次參考（層級 {}）",
            stratum_unsync: "未同步",
            theme_light: "淺色",
            theme_dark: "深色",
            theme_auto: "自動",
            lang_english: "英語",
            lang_chinese_simplified: "簡體中文",
            lang_chinese_traditional: "繁體中文",
            peer_sys: "系統對等",
            unsafe_time: "⚠ 時間不安全（根距離 > 1.5 秒）",
            ref_id_tooltip: "參考源：GNSS/GPS、CESM（銫）、RUBY（銣），或層級 2+ 的上游 IP。",
            calibration: "校準",
            cal_enable: "在下一分鐘響鈴",
            cal_countdown: "下次響鈴在 {:.1} 秒",
            cal_mark: "標記",
            cal_marked: "標記時間：{}",
            cal_hint: "將手錶設為該分鐘，等待響鈴，然後按標記。",
            net_identity: "網路身分與位置",
            server_ip: "伺服器 IP：{}",
            your_ip: "您的 IP：{}",
            server_loc: "伺服器位置：{}",
            your_loc: "您的位置：{}",
            coords: "座標：{:.4}, {:.4}",
            isp: "ISP：{}",
            peer_disp: "對等分散：{:.3} 毫秒",
            peer_warn: "⚠ 時間品質下降（對等分散過高）",
            tof: "上行：{:.1} 毫秒 | 下行：{:.1} 毫秒",
            kod_banner: "⚠ 伺服器拒絕請求：{}",
            tab_time: "時間",
            tab_server: "伺服器",
            tab_network: "網路",
            tab_calibration: "校準",
            tab_settings: "設定",
            time_format: "時間格式：",
            hour_12: "12 小時",
            hour_24: "24 小時",
            text_size: "文字大小：",
            size_small: "小",
            size_normal: "正常",
            size_big: "大",
            drift_test: "漂移測試",
            drift_start: "開始",
            drift_stop: "停止",
            drift_duration: "時長：",
            drift_running: "測量中... 剩餘 {:.0} 秒",
            drift_result: "時鐘在 {:.0} 秒內{}了 {:.1} 毫秒（{:+.1} ppm）",
            drift_fast: "快",
            drift_slow: "慢",
            drift_crystal: "估算晶振誤差：{:+.1} ppm",
            hide_all: "全部隱藏",
            show_all: "全部顯示",
            window_size: "視窗大小：{} x {}",
            fps: "FPS：{:.0}",
            visibility: "可見性",
            tab_global: "全球",
            search: "搜尋：",
            hostname: "主機名",
            strategy: "同步策略",
            category: "類別",
            offset: "偏移",
            server_time: "伺服器時間",
            device_time: "裝置時間",
            extensions: "擴展欄位",
            extensions_none: "無",
            set_time: "設定系統時間（需提升權限）",
            set_time_launched: "已啟動提升權限提示。",
            set_time_failed: "無法啟動提升權限程序。",
            set_time_na: "尚無伺服器時間。",
        }
    }
}

/// Format a translated template by replacing `{}` / `{:.N}` placeholders in order.
fn fmt(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();
    for arg in args {
        let bytes = result.as_bytes();
        let mut start: Option<(usize, usize)> = None;
        for i in 0..result.len() {
            if bytes[i] == b'{' {
                if i + 1 < result.len() && bytes[i + 1] == b'}' {
                    start = Some((i, i + 1));
                    break;
                }
                if i + 2 < result.len() && bytes[i + 1] == b':' && bytes[i + 2] == b'.'
                    && let Some(rel) = result[i..].find('}') {
                        start = Some((i, i + rel));
                        break;
                    }
            }
        }
        if let Some((s, e)) = start {
            result.replace_range(s..=e, arg);
        } else {
            break;
        }
    }
    result
}

/// A parsed NTP transmit timestamp.
struct NtpTimestamp {
    /// Whole seconds since the NTP epoch (1900-01-01).
    seconds: u64,
    /// Fractional seconds (0..1) as a 32-bit fraction.
    fraction: u32,
}

impl NtpTimestamp {
    /// Convert to Unix epoch seconds (whole seconds).
    fn to_unix_seconds(&self) -> u64 {
        self.seconds.saturating_sub(NTP_TIMESTAMP_DELTA)
    }

    /// Fractional part as milliseconds (0..1000).
    fn millis(&self) -> u64 {
        (self.fraction as u64 * 1000) >> 32
    }

    /// Unix epoch time as fractional seconds.
    fn to_unix_fractional(&self) -> f64 {
        self.to_unix_seconds() as f64 + self.millis() as f64 / 1000.0
    }

    /// NTP-format fractional seconds (seconds since 1900).
    fn to_ntp_fractional(&self) -> f64 {
        self.seconds as f64 + self.fraction as f64 / 4294967296.0
    }
}

/// NTP header fields describing the server's reference and synchronization state.
struct NtpHeader {
    /// Leap indicator (2-bit flag tracking scheduled leap seconds).
    leap_indicator: u8,
    /// Stratum: how close the server is to the ultimate reference clock.
    stratum: u8,
    /// Poll interval in seconds (2^poll).
    poll_interval_secs: f64,
    /// Root delay: cumulative round-trip latency back to the master clock (seconds).
    root_delay_secs: f64,
    /// Root dispersion: maximum error back to the master clock (seconds).
    root_dispersion_secs: f64,
    /// Precision of the server's system clock, in seconds (2^precision).
    precision_secs: f64,
    /// Reference ID: identifies the reference source (ASCII for stratum 1, IP otherwise).
    reference_id: String,
    /// Reference timestamp: when the server's clock was last set/corrected.
    reference_timestamp: Option<DateTime<Utc>>,
    /// NTP protocol version negotiated (from the response).
    version: u8,
    /// NTP mode of the response (3=client, 4=server, 5=broadcast).
    mode: u8,
    /// Receive timestamp (T2): when the request arrived at the server (NTP seconds).
    receive_ntp: f64,
}

/// Result of a single NTP query, including traffic, latency and clock offset.
struct QueryResult {
    timestamp: NtpTimestamp,
    header: NtpHeader,
    sent_bytes: u64,
    received_bytes: u64,
    /// Round-trip time in milliseconds.
    ping_ms: f64,
    /// Estimated system clock deviation vs the server, in seconds.
    offset_secs: f64,
    /// Origin timestamp (T1): when the request left the client (NTP seconds).
    origin_ntp: f64,
    /// Destination timestamp (T4): when the response arrived at the client (NTP seconds).
    destination_ntp: f64,
    /// True network round-trip delay: (T4 - T1) - (T3 - T2), in seconds.
    true_rtt_secs: f64,
    /// True clock offset: ((T2 - T1) + (T3 - T4)) / 2, in seconds.
    true_offset_secs: f64,
    /// Local (client) IP used for the exchange.
    local_ip: Option<String>,
    /// Resolved server IP.
    server_ip: Option<String>,
    /// Extension fields / MAC summary (NTS, auth, etc.).
    extensions: String,
}

/// Build a 48-byte NTP request packet (version 4, client mode).
fn build_request_packet() -> [u8; 48] {
    let mut packet = [0u8; 48];
    // LI=0 (2 bits), VN=4 (3 bits), Mode=3 (3 bits) => 0b001_000_11 = 0x23
    packet[0] = 0x23;
    packet
}

/// Parse the transmit timestamp (bytes 40..48) from an NTP response packet.
fn parse_transmit_timestamp(packet: &[u8]) -> NtpTimestamp {
    let seconds = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]);
    let fraction = u32::from_be_bytes([packet[44], packet[45], packet[46], packet[47]]);
    NtpTimestamp {
        seconds: seconds as u64,
        fraction,
    }
}

/// Parse a 64-bit NTP timestamp (32-bit seconds + 32-bit fraction) into fractional NTP seconds.
fn parse_ntp_fractional(bytes: &[u8]) -> f64 {
    let secs = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let frac = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    secs as f64 + frac as f64 / 4294967296.0
}

/// Current local time as fractional NTP seconds (seconds since 1900).
fn local_to_ntp_seconds() -> f64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    now + NTP_TIMESTAMP_DELTA as f64
}

/// Parse the NTP header fields (bytes 0..24) from a response packet.
fn parse_header(packet: &[u8]) -> NtpHeader {
    // Byte 0: LI (2 bits) | VN (3 bits) | Mode (3 bits).
    let leap_indicator = (packet[0] >> 6) & 0x3;
    let version = (packet[0] >> 3) & 0x7;
    let mode = packet[0] & 0x7;
    // Byte 1: Stratum.
    let stratum = packet[1];
    // Byte 2: Poll (signed 8-bit exponent of 2, in seconds).
    let poll = packet[2] as i8;
    let poll_interval_secs = 2f64.powi(poll as i32);
    // Byte 3: Precision (signed 8-bit exponent of 2, in seconds).
    let precision = packet[3] as i8;
    let precision_secs = 2f64.powi(precision as i32);
    // Bytes 4..8: Root Delay (signed 16.16 fixed point, seconds).
    let root_delay = i32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
    // Bytes 8..12: Root Dispersion (unsigned 16.16 fixed point, seconds).
    let root_dispersion = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);

    // Bytes 12..16: Reference ID. ASCII code for stratum 0 (KoD) and 1, IPv4 otherwise.
    let reference_id = if stratum == 0 || stratum == 1 {
        let bytes = [packet[12], packet[13], packet[14], packet[15]];
        bytes.iter().map(|&b| b as char).collect()
    } else {
        format!(
            "{}.{}.{}.{}",
            packet[12], packet[13], packet[14], packet[15]
        )
    };

    // Bytes 16..24: Reference timestamp (32-bit seconds + 32-bit fraction).
    let ref_seconds = u32::from_be_bytes([packet[16], packet[17], packet[18], packet[19]]);
    let ref_fraction = u32::from_be_bytes([packet[20], packet[21], packet[22], packet[23]]);
    let ref_unix = (ref_seconds as u64).saturating_sub(NTP_TIMESTAMP_DELTA);
    let ref_millis = (ref_fraction as u64 * 1000) >> 32;
    let reference_timestamp =
        DateTime::from_timestamp(ref_unix as i64, (ref_millis as u32) * 1_000_000);

    // Bytes 32..40: Receive timestamp (T2).
    let receive_ntp = parse_ntp_fractional(&packet[32..40]);

    NtpHeader {
        leap_indicator,
        stratum,
        poll_interval_secs,
        root_delay_secs: root_delay as f64 / 65536.0,
        root_dispersion_secs: root_dispersion as f64 / 65536.0,
        precision_secs,
        reference_id,
        reference_timestamp,
        version,
        mode,
        receive_ntp,
    }
}

/// Query the NTP server, measuring traffic, latency, clock offset and the 4 timestamps.
fn query_ntp(server: &str) -> Result<QueryResult, Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;

    let address = format!("{server}:{NTP_PORT}");
    let request = build_request_packet();

    // Connect the socket so the OS assigns a real source IP (local_addr()).
    socket.connect(address.as_str())?;

    // T1: origin timestamp (request leaves client).
    let origin_ntp = local_to_ntp_seconds();
    let start = Instant::now();
    socket.send(&request)?;

    let mut buf = [0u8; 1024];
    let len = socket.recv(&mut buf)?;
    let ping_ms = start.elapsed().as_secs_f64() * 1000.0;
    // T4: destination timestamp (response arrives at client).
    let destination_ntp = local_to_ntp_seconds();

    if len < 48 {
        return Err(format!("Short NTP response: {len} bytes").into());
    }

    let timestamp = parse_transmit_timestamp(&buf[..48]);
    let header = parse_header(&buf[..48]);
    // Parse any extension fields / MAC after the 48-byte header.
    let extensions = parse_extensions(&buf[..len]);

    // T3: transmit timestamp (response leaves server).
    let transmit_ntp = timestamp.to_ntp_fractional();
    // T2: receive timestamp (request arrives at server).
    let receive_ntp = header.receive_ntp;

    // True network round-trip delay: (T4 - T1) - (T3 - T2).
    let true_rtt_secs = (destination_ntp - origin_ntp) - (transmit_ntp - receive_ntp);
    // True clock offset: ((T2 - T1) + (T3 - T4)) / 2.
    let true_offset_secs = ((receive_ntp - origin_ntp) + (transmit_ntp - destination_ntp)) / 2.0;

    // Simple offset estimate (server transmit + one-way delay - local receipt).
    let local_receipt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let offset_secs = timestamp.to_unix_fractional() + (ping_ms / 1000.0) / 2.0 - local_receipt;

    let local_ip = socket.local_addr().ok().map(|a| a.ip().to_string());
    let server_ip = resolve_ip(server);

    Ok(QueryResult {
        timestamp,
        header,
        sent_bytes: request.len() as u64,
        received_bytes: len as u64,
        ping_ms,
        offset_secs,
        origin_ntp,
        destination_ntp,
        true_rtt_secs,
        true_offset_secs,
        local_ip,
        server_ip,
        extensions,
    })
}

/// Parse NTP extension fields / MAC after the 48-byte header.
/// Returns a human-readable summary (e.g. "1 ext field (NTS, 24 bytes)").
fn parse_extensions(packet: &[u8]) -> String {
    if packet.len() <= 48 {
        return "none".to_string();
    }
    let mut pos = 48;
    let mut fields = 0usize;
    let mut summary = String::new();
    while pos + 4 <= packet.len() {
        let field_type = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
        let len_field = u16::from_be_bytes([packet[pos + 2], packet[pos + 3]]);
        // NTP extension field length is in 4-byte words (RFC 7822).
        let field_len = (len_field as usize) * 4;
        if field_len < 4 || pos + field_len > packet.len() {
            break;
        }
        fields += 1;
        let type_name = match field_type {
            0x0104 => "NTS",
            0x0202 => "NTS Cookie",
            0x0203 => "NTS Cookie List",
            0x0204 => "NTS Server Negotiation",
            0x0205 => "NTS Error",
            0x0206 => "NTS Authenticator",
            0x0000 => "Padding",
            _ => "Unknown",
        };
        if !summary.is_empty() {
            summary.push_str(", ");
        }
        summary.push_str(&format!("{type_name} ({field_len} B)"));
        pos += field_len;
    }
    if fields == 0 {
        // Likely a MAC (last 20/24 bytes) rather than extension fields.
        let tail = packet.len() - 48;
        format!("MAC present ({tail} B)")
    } else {
        format!("{fields} ext field(s): {summary}")
    }
}

/// Resolve a hostname (or IP) to a string IP address.
fn resolve_ip(server: &str) -> Option<String> {
    if let Ok(ip) = server.parse::<std::net::IpAddr>() {
        return Some(ip.to_string());
    }
    (server, NTP_PORT)
        .to_socket_addrs()
        .ok()?
        .next()
        .map(|a| a.ip().to_string())
}

/// Query a free IP geolocation API. Empty IP queries the requester's public IP.
fn fetch_geo(ip: &str) -> Option<GeoInfo> {
    let url = if ip.is_empty() {
        lc!("http://ip-api.com/json/?fields=status,message,country,regionName,city,lat,lon,isp,query")
            .to_string()
    } else {
        let mut u = lc!("http://ip-api.com/json/").to_string();
        u.push_str(ip);
        u.push_str(&lc!("?fields=status,message,country,regionName,city,lat,lon,isp,query"));
        u
    };
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(5))
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v["status"].as_str() != Some("success") {
        return None;
    }
    Some(GeoInfo {
        city: v["city"].as_str().unwrap_or("").to_string(),
        region: v["regionName"].as_str().unwrap_or("").to_string(),
        country: v["country"].as_str().unwrap_or("").to_string(),
        lat: v["lat"].as_f64().unwrap_or(0.0),
        lon: v["lon"].as_f64().unwrap_or(0.0),
        isp: v["isp"].as_str().unwrap_or("").to_string(),
    })
}

/// Background thread: fetches geolocation for the client and server IPs once.
fn geo_worker(state: Arc<Mutex<ClockState>>) {
    loop {
        let server_ip = {
            let s = state.lock().unwrap();
            s.server_ip.clone()
        };
        if let Some(srv) = server_ip {
            // Client location comes from the public IP (query without an IP).
            let cg = fetch_geo("");
            let sg = fetch_geo(&srv);
            let mut s = state.lock().unwrap();
            s.client_geo = cg;
            s.server_geo = sg;
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Background thread: collects System-tab diagnostics once and caches them.
/// All the slow subprocess calls (w32tm, PowerShell, tracert) run here so the
/// UI thread never blocks.
fn system_worker(state: Arc<Mutex<ClockState>>) {
    let timer = system::collect_timer_info();
    let w32 = system::collect_w32time();
    let load = system::collect_load();
    let power = system::collect_power();
    let host = {
        let s = state.lock().unwrap();
        let idx = s.active_profile.min(s.profiles.len().saturating_sub(1));
        s.profiles[idx].hostname.clone()
    };
    let hops = system::traceroute(&host);
    let mut s = state.lock().unwrap();
    s.system_info = SystemInfo {
        timer,
        w32,
        load,
        power,
        hops,
        ready: true,
    };
}

/// Background thread: queries each global server once and stores its offset vs the local clock.
fn global_worker(state: Arc<Mutex<ClockState>>) {
    for gs in GLOBAL_SERVERS {
        if let Ok(qr) = query_ntp(gs.hostname) {
            let mut s = state.lock().unwrap();
            s.global_offsets
                .insert(gs.hostname.to_string(), qr.offset_secs * 1000.0);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Cached System-tab diagnostics, collected on a background thread so the
/// UI never blocks on slow subprocess calls (w32tm, PowerShell, tracert).
#[derive(Default)]
struct SystemInfo {
    timer: system::TimerInfo,
    w32: system::W32TimeInfo,
    load: system::LoadInfo,
    power: system::PowerInfo,
    hops: Vec<String>,
    ready: bool,
}

/// Shared state between the NTP background thread and the GUI.
struct ClockState {
    utc: Option<DateTime<Utc>>,
    local: Option<DateTime<Local>>,
    unix_seconds: Option<u64>,
    last_error: Option<String>,
    // Server profiles.
    profiles: Vec<ServerProfile>,
    active_profile: usize,
    // Network statistics (NTP server traffic only).
    total_sent: u64,
    total_received: u64,
    sent_rate: f64,
    received_rate: f64,
    request_count: u64,
    response_count: u64,
    // Ping / latency statistics.
    ping_ms: f64,
    ping_min: f64,
    ping_max: f64,
    ping_avg: f64,
    ping_count: u64,
    // Welford accumulator for ping jitter (standard deviation).
    ping_m2: f64,
    // Recent ping samples for the sparkline.
    ping_history: VecDeque<f64>,
    // System clock deviation vs the server.
    offset_secs: f64,
    offset_min: f64,
    offset_max: f64,
    // Welford accumulators for offset standard deviation.
    offset_count: u64,
    offset_mean: f64,
    offset_m2: f64,
    // Frequency error (PPM): rate of change of the clock offset.
    freq_ppm: f64,
    // Packet loss rate (percentage of dropped requests).
    packet_loss_pct: f64,
    // Clock status: synchronized / unsynchronized / estimated fallback.
    clock_status: ClockStatus,
    // NTP header fields (server reference info).
    leap_indicator: u8,
    stratum: u8,
    poll_interval_secs: f64,
    root_delay_secs: f64,
    root_dispersion_secs: f64,
    precision_secs: f64,
    reference_id: String,
    reference_timestamp: Option<DateTime<Utc>>,
    ntp_version: u8,
    ntp_mode: u8,
    // True time vs latency (4-timestamp formula).
    origin_ntp: f64,
    receive_ntp: f64,
    transmit_ntp: f64,
    destination_ntp: f64,
    true_rtt_secs: f64,
    true_offset_secs: f64,
    // Network identity / location.
    client_ip: Option<String>,
    server_ip: Option<String>,
    client_geo: Option<GeoInfo>,
    server_geo: Option<GeoInfo>,
    // Peer dispersion (grows with local drift when no successful exchange).
    peer_dispersion: f64,
    // Time-of-flight asymmetry.
    upstream_ms: f64,
    downstream_ms: f64,
    // Kiss-o'-Death detection.
    kod_code: Option<String>,
    // NTP extension fields / MAC summary.
    extensions: String,
    // Offsets (ms) of global servers vs the local clock, keyed by hostname.
    global_offsets: HashMap<String, f64>,
    // Advanced statistics.
    offset_history: VecDeque<f64>, // recent offsets (ms) for Allan/percentiles/MTE
    slew_count: u64,               // times offset exceeded the 128ms slew threshold
    // Clock filter: sliding window of (offset_secs, rtt_secs) samples.
    filter_window: VecDeque<(f64, f64)>,
    // Best (filtered) offset selected from the window, in seconds.
    filtered_offset_secs: f64,
    // Cached System-tab diagnostics (collected on a background thread).
    system_info: SystemInfo,
}

/// Background thread: continuously queries the active NTP server and updates state.
fn ntp_worker(state: Arc<Mutex<ClockState>>) {
    let mut prev_time = Instant::now();
    let mut prev_offset: Option<f64> = None;
    let mut prev_offset_time: Option<Instant> = None;

    loop {
        let start = Instant::now();

        // Read the active profile's hostname and IP.
        let (hostname, ip) = {
            let s = state.lock().unwrap();
            let idx = s.active_profile.min(s.profiles.len().saturating_sub(1));
            let p = &s.profiles[idx];
            (p.hostname.clone(), p.ip.clone())
        };

        // Try the hostname first, fall back to the IP if resolution fails.
        let result = match query_ntp(&hostname) {
            Ok(qr) => Ok(qr),
            Err(_) => query_ntp(&ip),
        };

        // Compute transfer rates over the time since the previous query.
        let elapsed = start.duration_since(prev_time).as_secs_f64().max(0.001);
        prev_time = start;

        {
            let mut s = state.lock().unwrap();
            // Every attempt counts as a request (for packet loss calculation).
            s.request_count += 1;

            match result {
                Ok(qr) => {
                    let unix = qr.timestamp.to_unix_seconds();
                    let millis = qr.timestamp.millis();

                    s.response_count += 1;
                    s.total_sent += qr.sent_bytes;
                    s.total_received += qr.received_bytes;
                    s.sent_rate = qr.sent_bytes as f64 / elapsed;
                    s.received_rate = qr.received_bytes as f64 / elapsed;

                    // Ping statistics + jitter (Welford's algorithm).
                    s.ping_ms = qr.ping_ms;
                    s.ping_count += 1;
                    if s.ping_count == 1 {
                        s.ping_min = qr.ping_ms;
                        s.ping_max = qr.ping_ms;
                        s.ping_avg = qr.ping_ms;
                        s.ping_m2 = 0.0;
                    } else {
                        s.ping_min = s.ping_min.min(qr.ping_ms);
                        s.ping_max = s.ping_max.max(qr.ping_ms);
                        let delta = qr.ping_ms - s.ping_avg;
                        s.ping_avg += delta / s.ping_count as f64;
                        s.ping_m2 += delta * (qr.ping_ms - s.ping_avg);
                    }

                    // Push to sparkline history (keep last N samples).
                    s.ping_history.push_back(qr.ping_ms);
                    if s.ping_history.len() > SPARKLINE_SAMPLES {
                        s.ping_history.pop_front();
                    }

                    // Clock deviation statistics + standard deviation (Welford).
                    s.offset_secs = qr.offset_secs;
                    s.offset_count += 1;
                    if s.offset_count == 1 {
                        s.offset_min = qr.offset_secs;
                        s.offset_max = qr.offset_secs;
                        s.offset_mean = qr.offset_secs;
                        s.offset_m2 = 0.0;
                    } else {
                        s.offset_min = s.offset_min.min(qr.offset_secs);
                        s.offset_max = s.offset_max.max(qr.offset_secs);
                        let delta = qr.offset_secs - s.offset_mean;
                        s.offset_mean += delta / s.offset_count as f64;
                        s.offset_m2 += delta * (qr.offset_secs - s.offset_mean);
                    }

                    // Advanced statistics: keep recent offsets (ms) and count slews.
                    s.offset_history.push_back(qr.offset_secs * 1000.0);
                    if s.offset_history.len() > 300 {
                        s.offset_history.pop_front();
                    }
                    if qr.offset_secs.abs() > 0.128 {
                        s.slew_count += 1;
                    }

                    // Clock filter: keep a sliding window of (offset, rtt) samples
                    // and select the best (lowest-RTT) offset as the filtered value.
                    s.filter_window.push_back((qr.offset_secs, qr.true_rtt_secs));
                    if s.filter_window.len() > 8 {
                        s.filter_window.pop_front();
                    }
                    s.filtered_offset_secs = clock_filter(&s.filter_window);

                    // Frequency error (PPM): rate of change of the clock offset.
                    if let (Some(po), Some(pt)) = (prev_offset, prev_offset_time) {
                        let dt = start.duration_since(pt).as_secs_f64();
                        if dt > 0.0 {
                            s.freq_ppm = (qr.offset_secs - po) / dt * 1e6;
                        }
                    }
                    prev_offset = Some(qr.offset_secs);
                    prev_offset_time = Some(start);

                    // Packet loss rate.
                    s.packet_loss_pct = if s.request_count > 0 {
                        (s.request_count - s.response_count) as f64 / s.request_count as f64 * 100.0
                    } else {
                        0.0
                    };

                    // Clock status.
                    s.clock_status = ClockStatus::Synchronized;

                    // NTP header fields.
                    s.leap_indicator = qr.header.leap_indicator;
                    s.stratum = qr.header.stratum;
                    s.poll_interval_secs = qr.header.poll_interval_secs;
                    s.root_delay_secs = qr.header.root_delay_secs;
                    s.root_dispersion_secs = qr.header.root_dispersion_secs;
                    s.precision_secs = qr.header.precision_secs;
                    s.reference_id = qr.header.reference_id.clone();
                    s.reference_timestamp = qr.header.reference_timestamp;
                    s.ntp_version = qr.header.version;
                    s.ntp_mode = qr.header.mode;

                    // True time vs latency (4-timestamp formula).
                    s.origin_ntp = qr.origin_ntp;
                    s.receive_ntp = qr.header.receive_ntp;
                    s.transmit_ntp = qr.timestamp.to_ntp_fractional();
                    s.destination_ntp = qr.destination_ntp;
                    s.true_rtt_secs = qr.true_rtt_secs;
                    s.true_offset_secs = qr.true_offset_secs;

                    // Network identity / location.
                    // Client IP is stable, so set it once. Server IP must always
                    // reflect the currently active server (it changes on profile switch).
                    if s.client_ip.is_none() {
                        s.client_ip = qr.local_ip.clone();
                    }
                    s.server_ip = qr.server_ip.clone();

                    // Peer dispersion resets to the server's dispersion on success.
                    s.peer_dispersion = qr.header.root_dispersion_secs;

                    // Time-of-flight asymmetry, corrected for the clock offset.
                    // Raw (T2 - T1) and (T4 - T3) are invalid across devices when the
                    // local clock is drifted, so subtract/add the true offset.
                    s.upstream_ms =
                        ((qr.header.receive_ntp - qr.origin_ntp) - qr.true_offset_secs) * 1000.0;
                    s.downstream_ms = ((qr.destination_ntp - qr.timestamp.to_ntp_fractional())
                        + qr.true_offset_secs)
                        * 1000.0;

                    // Kiss-o'-Death detection (stratum 0 => reference ID is a reason code).
                    s.kod_code = if qr.header.stratum == 0 {
                        Some(qr.header.reference_id.clone())
                    } else {
                        None
                    };
                    // NTP extension fields / MAC summary.
                    s.extensions = qr.extensions.clone();

                    match DateTime::from_timestamp(unix as i64, (millis as u32) * 1_000_000) {
                        Some(utc) => {
                            s.utc = Some(utc);
                            s.local = Some(utc.with_timezone(&Local));
                            s.unix_seconds = Some(unix);
                            s.last_error = None;
                        }
                        None => s.last_error = Some("Timestamp out of range".to_string()),
                    }
                }
                Err(e) => {
                    s.last_error = Some(e.to_string());
                    // Packet loss rate.
                    s.packet_loss_pct = if s.request_count > 0 {
                        (s.request_count - s.response_count) as f64 / s.request_count as f64 * 100.0
                    } else {
                        0.0
                    };
                    // Clock status: using stale data if we have any, else unsynchronized.
                    s.clock_status = if s.utc.is_some() {
                        ClockStatus::Estimated
                    } else {
                        ClockStatus::Unsynchronized
                    };
                    // Peer dispersion grows by the local clock's max drift (15 PPM)
                    // for every second without a successful exchange.
                    s.peer_dispersion += 15e-6 * elapsed;
                }
            }
        }

        thread::sleep(Duration::from_secs(REFRESH_INTERVAL_SECS));
    }
}

/// Format a byte count (storage measurement): B, KiB, MiB, GiB.
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// Format a transfer rate (network measurement) in bits per second.
fn format_bitrate(bytes_per_sec: f64) -> String {
    let bps = bytes_per_sec * 8.0;
    const KBPS: f64 = 1000.0;
    const MBPS: f64 = KBPS * 1000.0;
    const GBPS: f64 = MBPS * 1000.0;
    if bps >= GBPS {
        format!("{:.2} Gbps", bps / GBPS)
    } else if bps >= MBPS {
        format!("{:.2} Mbps", bps / MBPS)
    } else if bps >= KBPS {
        format!("{:.2} Kbps", bps / KBPS)
    } else {
        format!("{bps:.1} bps")
    }
}

/// Format a signed clock deviation (seconds) with a ± sign.
fn format_offset(secs: f64) -> String {
    let abs = secs.abs();
    if abs >= 1.0 {
        format!("{secs:+.3} s")
    } else {
        format!("{:+} ms", secs * 1000.0)
    }
}

/// Format a frequency error in parts per million with a sign.
fn format_ppm(ppm: f64) -> String {
    format!("{ppm:+.3} ppm")
}

/// Sample standard deviation from Welford's M2 accumulator.
fn std_dev(m2: f64, count: u64) -> f64 {
    if count > 1 {
        (m2 / (count - 1) as f64).sqrt()
    } else {
        0.0
    }
}

/// NTP-style clock filter: given a window of (offset_secs, rtt_secs) samples,
/// select the most trustworthy offset. Samples with the lowest round-trip time
/// are the most accurate (least network noise), so we take the median of the
/// lowest-RTT half of the window. Returns the filtered offset in seconds.
fn clock_filter(window: &VecDeque<(f64, f64)>) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    // Sort by RTT ascending; the best samples come first.
    let mut sorted: Vec<(f64, f64)> = window.iter().copied().collect();
    sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    // Keep the lowest-RTT half (at least 1 sample).
    let keep = (sorted.len() / 2).max(1);
    let best = &sorted[..keep];
    // Median of the kept offsets.
    let mut offsets: Vec<f64> = best.iter().map(|(o, _)| *o).collect();
    offsets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = offsets.len() / 2;
    if offsets.len().is_multiple_of(2) {
        (offsets[mid - 1] + offsets[mid]) / 2.0
    } else {
        offsets[mid]
    }
}

/// Human-readable description of the NTP leap indicator.
fn leap_text(s: &Strings, li: u8) -> &'static str {
    match li {
        0 => s.leap_none,
        1 => s.leap_61,
        2 => s.leap_59,
        _ => s.leap_alarm,
    }
}

/// Human-readable description of the NTP stratum.
fn stratum_text(s: &Strings, stratum: u8) -> String {
    match stratum {
        0 => s.stratum_unspec.to_string(),
        1 => s.stratum_primary.to_string(),
        2..=15 => s.stratum_secondary.replace("{}", &stratum.to_string()),
        _ => s.stratum_unsync.to_string(),
    }
}

/// Human-readable description of a Kiss-o'-Death code.
fn kod_description(code: &str) -> &'static str {
    match code {
        "RATE" => "Rate limit exceeded, back off",
        "DENY" => "Access denied",
        "RSTR" => "Access restricted",
        "UNSYNC" => "Server not synchronized",
        "INIT" => "Association initialized",
        "STEP" => "Step time change",
        "ACST" => "Access denied by ACL",
        "AUTH" => "Authentication failed",
        "DROP" => "Packet dropped",
        "NCST" => "No association",
        "NKEY" => "Key not found",
        "XKEY" => "Key expired",
        "TIME" => "Time changed",
        _ => "Unknown KoD code",
    }
}

/// Human-readable description of the NTP mode.
fn mode_text(mode: u8) -> &'static str {
    match mode {
        1 => "Symmetric active",
        2 => "Symmetric passive",
        3 => "Client",
        4 => "Server",
        5 => "Broadcast",
        6 => "NTP control",
        7 => "Private use",
        _ => "Reserved",
    }
}

/// Format an NTP-format timestamp (seconds since 1900) as a local time string.
fn format_ntp_local(ntp_secs: f64) -> String {
    let unix = ntp_secs - NTP_TIMESTAMP_DELTA as f64;
    let secs = unix.floor() as i64;
    let nanos = ((unix - unix.floor()) * 1e9) as u32;
    match DateTime::from_timestamp(secs, nanos) {
        Some(dt) => dt.with_timezone(&Local).format("%H:%M:%S%.3f").to_string(),
        None => "n/a".to_string(),
    }
}

/// Mask a value with asterisks of the same length (hides info without resizing).
fn mask(value: &str) -> String {
    "*".repeat(value.chars().count())
}

/// Draw a small sparkline of the given values.
fn draw_sparkline(ui: &mut egui::Ui, values: &VecDeque<f64>, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if values.len() < 2 {
        return;
    }
    let painter = ui.painter();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(1e-6);
    let n = values.len();
    let points: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + (i as f32 / (n - 1) as f32) * rect.width();
            let y = rect.bottom() - ((v - min) / range) as f32 * rect.height();
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0, 200, 150)),
    ));
}

/// Load a CJK-capable system font so Chinese text renders instead of "boxes of doom".
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Try common Windows CJK fonts in order of preference.
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc", // Microsoft YaHei
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf", // SimHei
        "C:\\Windows\\Fonts\\simsun.ttc", // SimSun
        "C:\\Windows\\Fonts\\Deng.ttf",   // DengXian
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("cjk".to_owned());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// Play a short beep. Uses the Windows console beep on Windows, and the
/// terminal bell character on other platforms.
fn beep() {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[console]::beep(1000, 300)"])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::io::Write;
        print!("\x07");
        let _ = std::io::stdout().flush();
    }
}

/// Background thread: waits for the next minute boundary, beeps once, then stops.
fn calibration_thread(state: Arc<Mutex<ClockState>>, running: Arc<AtomicBool>) {
    // Wait until we have a valid local time.
    let now = loop {
        if !running.load(Ordering::Relaxed) {
            return;
        }
        if let Some(local) = state.lock().unwrap().local {
            break local.timestamp() as f64 + local.timestamp_subsec_millis() as f64 / 1000.0;
        }
        thread::sleep(Duration::from_millis(100));
    };

    // Sleep until the next minute boundary, checking the stop flag.
    let secs_into_min = now % 60.0;
    let wait_ms = ((60.0 - secs_into_min) * 1000.0) as u64;
    let mut slept = 0u64;
    while slept < wait_ms && running.load(Ordering::Relaxed) {
        let step = 50u64.min(wait_ms - slept);
        thread::sleep(Duration::from_millis(step));
        slept += step;
    }

    if running.load(Ordering::Relaxed) {
        beep();
    }
    // One beep is enough; stop so the checkbox unchecks.
    running.store(false, Ordering::Relaxed);
}

/// Which profile-editing mode the UI is currently in.
#[derive(Clone, Copy, PartialEq)]
enum EditMode {
    None,
    New,
    Existing(usize),
}

/// The main UI tabs.
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Time,
    Server,
    Network,
    Calibration,
    System,
    Settings,
    Global,
}

/// UI text size.
#[derive(Clone, Copy, PartialEq)]
enum TextSize {
    Small,
    Normal,
    Big,
}

impl TextSize {
    fn zoom(&self) -> f32 {
        match self {
            TextSize::Small => 0.85,
            TextSize::Normal => 1.0,
            TextSize::Big => 1.25,
        }
    }
}

/// Per-item visibility for exposing/trackable info.
#[derive(Clone, Copy)]
struct Visibility {
    server_ip: bool,
    your_ip: bool,
    server_loc: bool,
    your_loc: bool,
    server_coords: bool,
    your_coords: bool,
    isp: bool,
    hostname: bool,
}

impl Default for Visibility {
    fn default() -> Self {
        Self {
            server_ip: true,
            your_ip: true,
            server_loc: true,
            your_loc: true,
            server_coords: true,
            your_coords: true,
            isp: true,
            hostname: true,
        }
    }
}

/// Result of a clock drift / crystal frequency test.
struct DriftResult {
    duration_secs: f64,
    drift_ms: f64,
    drift_ppm: f64,
}

/// Format a time as 12-hour or 24-hour.
fn format_time<Tz: chrono::TimeZone>(dt: &DateTime<Tz>, hour_24: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    if hour_24 {
        dt.format("%H:%M:%S").to_string()
    } else {
        dt.format("%I:%M:%S %p").to_string()
    }
}

/// Format a time with milliseconds (for the large main clock).
fn format_time_ms<Tz: chrono::TimeZone>(dt: &DateTime<Tz>, hour_24: bool) -> String
where
    Tz::Offset: std::fmt::Display,
{
    if hour_24 {
        dt.format("%H:%M:%S%.3f").to_string()
    } else {
        dt.format("%I:%M:%S%.3f %p").to_string()
    }
}

/// The egui application.
struct ClockApp {
    state: Arc<Mutex<ClockState>>,
    always_on_top: bool,
    theme: Theme,
    language: Language,
    edit_mode: EditMode,
    edit_name: String,
    edit_hostname: String,
    edit_ip: String,
    // Calibration.
    calibration_running: Arc<AtomicBool>,
    calibration_mark: Option<DateTime<Local>>,
    // Auto-resize: only for the first few frames so the user can resize freely.
    auto_resize_frames: u8,
    // UI state.
    current_tab: Tab,
    hour_24: bool,
    text_size: TextSize,
    visibility: Visibility,
    // Drift test.
    drift_running: bool,
    drift_start_server: Option<f64>,
    drift_start_local: Option<f64>,
    drift_duration: u64,
    drift_result: Option<DriftResult>,
    // Global servers tab.
    global_started: bool,
    global_search: String,
    global_category: GlobalCategory,
    // Set-system-time feedback message.
    set_time_msg: Option<String>,
}

impl ClockApp {
    fn new(state: Arc<Mutex<ClockState>>) -> Self {
        Self {
            state,
            always_on_top: false,
            theme: Theme::Dark,
            language: Language::English,
            edit_mode: EditMode::None,
            edit_name: String::new(),
            edit_hostname: String::new(),
            edit_ip: String::new(),
            calibration_running: Arc::new(AtomicBool::new(false)),
            calibration_mark: None,
            auto_resize_frames: 3,
            current_tab: Tab::Time,
            hour_24: true,
            text_size: TextSize::Normal,
            visibility: Visibility::default(),
            drift_running: false,
            drift_start_server: None,
            drift_start_local: None,
            drift_duration: 60,
            drift_result: None,
            global_started: false,
            global_search: String::new(),
            global_category: GlobalCategory::All,
            set_time_msg: None,
        }
    }
}

impl eframe::App for ClockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme.
        let dark = match self.theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::Auto => {
                dark_light::detect().unwrap_or(dark_light::Mode::Dark) == dark_light::Mode::Dark
            }
        };
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        ctx.set_zoom_factor(self.text_size.zoom());

        let s = Strings::for_language(self.language);
        let mut state = self.state.lock().unwrap();
        let local = state.local;
        let utc = state.utc;

        // ---- Header: program name + main time ----
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            // Row 1: program name (left) + FPS / window size (right).
            ui.horizontal(|ui| {
                ui.heading(s.title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let rect = ctx.screen_rect();
                    ui.label(fmt(
                        s.window_size,
                        &[
                            &format!("{:.0}", rect.width()),
                            &format!("{:.0}", rect.height()),
                        ],
                    ));
                    let fps = 1.0 / ctx.input(|i| i.stable_dt).max(1e-6);
                    ui.label(fmt(s.fps, &[&format!("{:.0}", fps)]));
                });
            });
            // Row 2: main time (left) + settings (right).
            if let Some(local) = local {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format_time_ms(&local, self.hour_24))
                            .size(40.0)
                            .strong(),
                    );
                    if let Some(utc) = utc {
                        ui.label(
                            egui::RichText::new(format!("UTC {}", format_time(&utc, self.hour_24)))
                                .size(16.0),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(s.theme);
                                egui::ComboBox::from_id_salt("theme_select")
                                    .selected_text(match self.theme {
                                        Theme::Light => s.theme_light,
                                        Theme::Dark => s.theme_dark,
                                        Theme::Auto => s.theme_auto,
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.theme,
                                            Theme::Light,
                                            s.theme_light,
                                        );
                                        ui.selectable_value(
                                            &mut self.theme,
                                            Theme::Dark,
                                            s.theme_dark,
                                        );
                                        ui.selectable_value(
                                            &mut self.theme,
                                            Theme::Auto,
                                            s.theme_auto,
                                        );
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.label(s.language);
                                egui::ComboBox::from_id_salt("lang_select")
                                    .selected_text(match self.language {
                                        Language::English => s.lang_english,
                                        Language::ChineseSimplified => s.lang_chinese_simplified,
                                        Language::ChineseTraditional => s.lang_chinese_traditional,
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.language,
                                            Language::English,
                                            s.lang_english,
                                        );
                                        ui.selectable_value(
                                            &mut self.language,
                                            Language::ChineseSimplified,
                                            s.lang_chinese_simplified,
                                        );
                                        ui.selectable_value(
                                            &mut self.language,
                                            Language::ChineseTraditional,
                                            s.lang_chinese_traditional,
                                        );
                                    });
                            });
                            ui.horizontal(|ui| {
                                ui.label(s.time_format);
                                ui.selectable_value(&mut self.hour_24, true, s.hour_24);
                                ui.selectable_value(&mut self.hour_24, false, s.hour_12);
                            });
                            ui.horizontal(|ui| {
                                ui.label(s.text_size);
                                ui.selectable_value(
                                    &mut self.text_size,
                                    TextSize::Small,
                                    s.size_small,
                                );
                                ui.selectable_value(
                                    &mut self.text_size,
                                    TextSize::Normal,
                                    s.size_normal,
                                );
                                ui.selectable_value(&mut self.text_size, TextSize::Big, s.size_big);
                            });
                            if ui
                                .checkbox(&mut self.always_on_top, s.always_on_top)
                                .changed()
                            {
                                let level = if self.always_on_top {
                                    egui::ViewportCommand::WindowLevel(
                                        egui::WindowLevel::AlwaysOnTop,
                                    )
                                } else {
                                    egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal)
                                };
                                ctx.send_viewport_cmd(level);
                            }
                        });
                    });
                });
            }
            ui.add_space(6.0);
        });

        let mut content_h = 0.0;
        egui::CentralPanel::default().show(ctx, |ui| {
            // Tab bar.
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Time, s.tab_time);
                ui.selectable_value(&mut self.current_tab, Tab::Server, s.tab_server);
                ui.selectable_value(&mut self.current_tab, Tab::Network, s.tab_network);
                ui.selectable_value(&mut self.current_tab, Tab::Calibration, s.tab_calibration);
                ui.selectable_value(&mut self.current_tab, Tab::System, "System");
                ui.selectable_value(&mut self.current_tab, Tab::Settings, s.tab_settings);
                ui.selectable_value(&mut self.current_tab, Tab::Global, s.tab_global);
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    match self.current_tab {
                        Tab::Time => {
                            if let Some(err) = &state.last_error {
                                ui.colored_label(egui::Color32::RED, fmt(s.error, &[err]));
                            }

                            let status_text = match state.clock_status {
                                ClockStatus::Synchronized => s.synchronized,
                                ClockStatus::Unsynchronized => s.unsynchronized,
                                ClockStatus::Estimated => s.estimated,
                            };
                            let drift_arrow = if state.freq_ppm > 0.0 { "↑" } else { "↓" };

                            egui::Grid::new("time_grid")
                                .num_columns(2)
                                .striped(true)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    match (&state.utc, &state.local) {
                                        (Some(utc), Some(local)) => {
                                            ui.label(s.utc);
                                            ui.label(
                                                utc.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                                            );
                                            ui.end_row();
                                            ui.label(s.local);
                                            ui.label(
                                                local.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                                            );
                                            ui.end_row();
                                            if let Some(unix) = state.unix_seconds {
                                                ui.label(s.epoch);
                                                ui.label(unix.to_string());
                                                ui.end_row();
                                            }
                                        }
                                        _ => {
                                            ui.label(s.waiting);
                                            ui.end_row();
                                        }
                                    }
                                    ui.label(s.deviation);
                                    ui.label(format_offset(state.offset_secs));
                                    ui.end_row();
                                    ui.label(s.range);
                                    ui.label(format!(
                                        "{} .. {}",
                                        format_offset(state.offset_min),
                                        format_offset(state.offset_max)
                                    ));
                                    ui.end_row();
                                    ui.label(s.jitter);
                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "{:.3} ms",
                                            std_dev(state.ping_m2, state.ping_count)
                                        ));
                                        draw_sparkline(
                                            ui,
                                            &state.ping_history,
                                            egui::vec2(120.0, 20.0),
                                        );
                                    });
                                    ui.end_row();
                                    ui.label(s.std_dev);
                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "{:.3} ms",
                                            std_dev(state.offset_m2, state.offset_count) * 1000.0
                                        ));
                                        draw_sparkline(
                                            ui,
                                            &state.offset_history,
                                            egui::vec2(120.0, 20.0),
                                        );
                                    });
                                    ui.end_row();
                                    ui.label(s.freq_error);
                                    ui.label(format!(
                                        "{} {}",
                                        format_ppm(state.freq_ppm),
                                        drift_arrow
                                    ));
                                    ui.end_row();
                                    ui.label(s.packet_loss);
                                    ui.label(format!("{:.2}%", state.packet_loss_pct));
                                    ui.end_row();
                                    ui.label(s.peer_disp);
                                    ui.label(format!("{:.3} ms", state.peer_dispersion * 1000.0));
                                    ui.end_row();
                                    if state.peer_dispersion > 0.1 {
                                        ui.colored_label(egui::Color32::RED, s.peer_warn);
                                        ui.end_row();
                                    }
                                    ui.label(s.clock_status);
                                    ui.label(status_text);
                                    ui.end_row();
                                    ui.label(s.origin);
                                    ui.label(format_ntp_local(state.origin_ntp));
                                    ui.end_row();
                                    ui.label(s.receive);
                                    ui.label(format_ntp_local(state.receive_ntp));
                                    ui.end_row();
                                    ui.label(s.transmit);
                                    ui.label(format_ntp_local(state.transmit_ntp));
                                    ui.end_row();
                                    ui.label(s.destination);
                                    ui.label(format_ntp_local(state.destination_ntp));
                                    ui.end_row();
                                    ui.label(s.true_rtt);
                                    ui.label(format!("{:.3} ms", state.true_rtt_secs * 1000.0));
                                    ui.end_row();
                                    ui.label(s.true_offset);
                                    ui.label(format_offset(state.true_offset_secs));
                                    ui.end_row();
                                    ui.label(s.tof);
                                    ui.label(format!(
                                        "{:.1} ms | {:.1} ms",
                                        state.upstream_ms, state.downstream_ms
                                    ));
                                    ui.end_row();
                                });

                            // Set system time from the NTP-derived value (elevated).
                            ui.add_space(8.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button(s.set_time).clicked() {
                                    if let Some(local) = state.local {
                                        let ts = local.format("%Y-%m-%d %H:%M:%S").to_string();
                                        let ok = system::set_system_time(&ts);
                                        self.set_time_msg = if ok {
                                            Some(s.set_time_launched.to_string())
                                        } else {
                                            Some(s.set_time_failed.to_string())
                                        };
                                    } else {
                                        self.set_time_msg = Some(s.set_time_na.to_string());
                                    }
                                }
                                if let Some(msg) = &self.set_time_msg {
                                    ui.label(msg);
                                }
                            });

                            // Live offset histogram (distribution of recent offsets).
                            ui.add_space(8.0);
                            ui.separator();
                            ui.label("Offset distribution (histogram):");
                            let hist: Vec<f64> = state.offset_history.iter().copied().collect();
                            if hist.len() >= 2 {
                                let min = hist.iter().cloned().fold(f64::INFINITY, f64::min);
                                let max = hist.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                                let range = (max - min).max(1e-6);
                                const BINS: usize = 24;
                                let mut counts = [0usize; BINS];
                                for v in &hist {
                                    let b = (((v - min) / range) * (BINS as f64 - 1.0))
                                        .round() as usize;
                                    counts[b.min(BINS - 1)] += 1;
                                }
                                let max_count = counts.iter().cloned().max().unwrap_or(1).max(1);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width().min(420.0), 60.0),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter();
                                let bar_w = rect.width() / BINS as f32;
                                for (i, c) in counts.iter().enumerate() {
                                    let h = (*c as f32 / max_count as f32) * rect.height();
                                    let x = rect.left() + i as f32 * bar_w;
                                    painter.rect_filled(
                                        egui::Rect::from_min_size(
                                            egui::pos2(x, rect.bottom() - h),
                                            egui::vec2(bar_w - 1.0, h),
                                        ),
                                        0.0,
                                        egui::Color32::from_rgb(0, 150, 200),
                                    );
                                }
                                ui.label(format!(
                                    "min {:.2} ms  |  max {:.2} ms  |  {} samples",
                                    min, max, hist.len()
                                ));
                            } else {
                                ui.weak("Collecting samples...");
                            }
                        }
                        Tab::Server => {
                            // Server profile selection / editing.
                            match self.edit_mode {
                                EditMode::None => {
                                    ui.horizontal(|ui| {
                                        ui.label(s.server);
                                        let mut new_active = state.active_profile;
                                        egui::ComboBox::from_id_salt("server_select")
                                            .selected_text(
                                                state.profiles[state.active_profile].name.clone(),
                                            )
                                            .show_ui(ui, |ui| {
                                                for (i, p) in state.profiles.iter().enumerate() {
                                                    ui.selectable_value(
                                                        &mut new_active,
                                                        i,
                                                        &p.name,
                                                    );
                                                }
                                            });
                                        state.active_profile = new_active;
                                        if ui.button(s.add).clicked() {
                                            self.edit_name.clear();
                                            self.edit_hostname.clear();
                                            self.edit_ip.clear();
                                            self.edit_mode = EditMode::New;
                                        }
                                        if ui.button(s.edit).clicked() {
                                            let i = state.active_profile;
                                            self.edit_name = state.profiles[i].name.clone();
                                            self.edit_hostname = state.profiles[i].hostname.clone();
                                            self.edit_ip = state.profiles[i].ip.clone();
                                            self.edit_mode = EditMode::Existing(i);
                                        }
                                        if ui.button(s.remove).clicked() {
                                            let can_remove = state.profiles.len() > 1;
                                            if can_remove {
                                                let idx = state.active_profile;
                                                state.profiles.remove(idx);
                                                if idx >= state.profiles.len() {
                                                    state.active_profile = state.profiles.len() - 1;
                                                }
                                            }
                                        }
                                    });
                                    let p = &state.profiles[state.active_profile];
                                    ui.label(fmt(s.active, &[&p.name, &p.hostname]));
                                }
                                EditMode::New | EditMode::Existing(_) => {
                                    ui.separator();
                                    ui.heading(s.edit_profile);
                                    ui.horizontal(|ui| {
                                        ui.label(s.name);
                                        ui.text_edit_singleline(&mut self.edit_name);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(s.host);
                                        ui.text_edit_singleline(&mut self.edit_hostname);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(s.ip);
                                        ui.text_edit_singleline(&mut self.edit_ip);
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button(s.save).clicked() {
                                            let name = self.edit_name.trim().to_string();
                                            let hostname = self.edit_hostname.trim().to_string();
                                            let ip = self.edit_ip.trim().to_string();
                                            if !name.is_empty()
                                                && (!hostname.is_empty() || !ip.is_empty())
                                            {
                                                match self.edit_mode {
                                                    EditMode::New => {
                                                        state.profiles.push(ServerProfile {
                                                            name,
                                                            hostname,
                                                            ip,
                                                        });
                                                        state.active_profile =
                                                            state.profiles.len() - 1;
                                                    }
                                                    EditMode::Existing(i) => {
                                                        state.profiles[i] =
                                                            ServerProfile { name, hostname, ip };
                                                    }
                                                    EditMode::None => {}
                                                }
                                                self.edit_mode = EditMode::None;
                                            }
                                        }
                                        if ui.button(s.cancel).clicked() {
                                            self.edit_mode = EditMode::None;
                                        }
                                    });
                                }
                            }

                            let root_distance =
                                state.root_dispersion_secs + state.root_delay_secs / 2.0;
                            let unsafe_flag = root_distance > 1.5;

                            egui::Grid::new("server_grid")
                                .num_columns(2)
                                .striped(true)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(s.stratum);
                                    ui.label(format!(
                                        "{} - {}",
                                        state.stratum,
                                        stratum_text(&s, state.stratum)
                                    ));
                                    ui.end_row();
                                    ui.label(s.root_delay);
                                    ui.label(format!("{:.3} ms", state.root_delay_secs * 1000.0));
                                    ui.end_row();
                                    ui.label(s.root_dispersion);
                                    ui.label(format!(
                                        "{:.3} ms",
                                        state.root_dispersion_secs * 1000.0
                                    ));
                                    ui.end_row();
                                    ui.label(s.leap);
                                    ui.label(format!(
                                        "{} - {}",
                                        state.leap_indicator,
                                        leap_text(&s, state.leap_indicator)
                                    ));
                                    ui.end_row();
                                    ui.label(s.poll);
                                    ui.label(format!("{:.0} s", state.poll_interval_secs));
                                    ui.end_row();
                                    ui.label(s.precision);
                                    ui.label(format!("{:.3} ms", state.precision_secs * 1000.0));
                                    ui.end_row();
                                    ui.label(s.ntp_version);
                                    ui.label(state.ntp_version.to_string());
                                    ui.end_row();
                                    ui.label(s.ntp_mode);
                                    ui.label(mode_text(state.ntp_mode));
                                    ui.end_row();
                                    ui.label(s.ref_id);
                                    ui.label(&state.reference_id)
                                        .on_hover_text(s.ref_id_tooltip);
                                    ui.end_row();
                                    ui.label(s.ref_time);
                                    match &state.reference_timestamp {
                                        Some(ts) => {
                                            ui.label(
                                                ts.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                                            );
                                        }
                                        None => {
                                            ui.label(s.ref_time_na);
                                        }
                                    }
                                    ui.end_row();
                                    ui.label(s.root_distance);
                                    ui.label(format!("{:.3} ms", root_distance * 1000.0));
                                    ui.end_row();
                                    if unsafe_flag {
                                        ui.colored_label(egui::Color32::RED, s.unsafe_time);
                                        ui.end_row();
                                    }
                                    ui.label(s.peer_state);
                                    ui.label(s.peer_sys);
                                    ui.end_row();
                                    ui.label(s.ping);
                                    ui.label(format!(
                                        "{:.1} ms  (min {:.1} / avg {:.1} / max {:.1})",
                                        state.ping_ms,
                                        state.ping_min,
                                        state.ping_avg,
                                        state.ping_max
                                    ));
                                    ui.end_row();
                                    ui.label(s.sent);
                                    ui.label(format!(
                                        "{} total, {}",
                                        format_bytes(state.total_sent),
                                        format_bitrate(state.sent_rate)
                                    ));
                                    ui.end_row();
                                    ui.label(s.received);
                                    ui.label(format!(
                                        "{} total, {}",
                                        format_bytes(state.total_received),
                                        format_bitrate(state.received_rate)
                                    ));
                                    ui.end_row();
                                    ui.label(s.requests);
                                    ui.label(format!(
                                        "{} sent, {} responses",
                                        state.request_count, state.response_count
                                    ));
                                    ui.end_row();
                                    ui.label(s.extensions);
                                    if state.extensions.is_empty() {
                                        ui.label(s.extensions_none);
                                    } else {
                                        ui.label(&state.extensions);
                                    }
                                    ui.end_row();
                                });
                        }
                        Tab::Network => {
                            // KoD warning banner with full code mapping.
                            if let Some(kod) = &state.kod_code {
                                let desc = kod_description(kod);
                                ui.colored_label(
                                    egui::Color32::RED,
                                    fmt(s.kod_banner, &[&format!("{kod} ({desc})")]),
                                );
                                ui.separator();
                            }

                            ui.horizontal(|ui| {
                                ui.heading(s.net_identity);
                                if ui.button(s.hide_all).clicked() {
                                    self.visibility = Visibility {
                                        server_ip: false,
                                        your_ip: false,
                                        server_loc: false,
                                        your_loc: false,
                                        server_coords: false,
                                        your_coords: false,
                                        isp: false,
                                        hostname: false,
                                    };
                                }
                                if ui.button(s.show_all).clicked() {
                                    self.visibility = Visibility::default();
                                }
                            });

                            // Table: header always visible, value hidden when toggled off.
                            let hostname = state.profiles[state.active_profile].hostname.clone();
                            let server_name = state.profiles[state.active_profile].name.clone();
                            let server_ip =
                                state.server_ip.clone().unwrap_or_else(|| "-".to_string());
                            let your_ip =
                                state.client_ip.clone().unwrap_or_else(|| "-".to_string());

                            egui::Grid::new("network_grid")
                                .num_columns(2)
                                .striped(true)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    // Hostname row.
                                    ui.label(s.active);
                                    let host_val = format!("{server_name} ({hostname})");
                                    if self.visibility.hostname {
                                        if ui.selectable_label(true, &host_val).clicked() {
                                            self.visibility.hostname = false;
                                        }
                                    } else if ui.selectable_label(false, mask(&host_val)).clicked()
                                    {
                                        self.visibility.hostname = true;
                                    }
                                    ui.end_row();
                                    // Server IP row.
                                    ui.label(s.server_ip);
                                    if self.visibility.server_ip {
                                        if ui.selectable_label(true, &server_ip).clicked() {
                                            self.visibility.server_ip = false;
                                        }
                                    } else if ui.selectable_label(false, mask(&server_ip)).clicked()
                                    {
                                        self.visibility.server_ip = true;
                                    }
                                    ui.end_row();
                                    // Your IP row.
                                    ui.label(s.your_ip);
                                    if self.visibility.your_ip {
                                        if ui.selectable_label(true, &your_ip).clicked() {
                                            self.visibility.your_ip = false;
                                        }
                                    } else if ui.selectable_label(false, mask(&your_ip)).clicked() {
                                        self.visibility.your_ip = true;
                                    }
                                    ui.end_row();
                                    // Server location row.
                                    ui.label(s.server_loc);
                                    if let Some(geo) = &state.server_geo {
                                        let loc = format!(
                                            "{}, {}, {}",
                                            geo.city, geo.region, geo.country
                                        );
                                        if self.visibility.server_loc {
                                            if ui.selectable_label(true, &loc).clicked() {
                                                self.visibility.server_loc = false;
                                            }
                                        } else if ui.selectable_label(false, mask(&loc)).clicked() {
                                            self.visibility.server_loc = true;
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                    ui.end_row();
                                    // Server coordinates row.
                                    ui.label(s.coords);
                                    if let Some(geo) = &state.server_geo {
                                        let c = format!("{:.4}, {:.4}", geo.lat, geo.lon);
                                        if self.visibility.server_coords {
                                            if ui.selectable_label(true, &c).clicked() {
                                                self.visibility.server_coords = false;
                                            }
                                        } else if ui.selectable_label(false, mask(&c)).clicked() {
                                            self.visibility.server_coords = true;
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                    ui.end_row();
                                    // Your location row.
                                    ui.label(s.your_loc);
                                    if let Some(geo) = &state.client_geo {
                                        let loc = format!(
                                            "{}, {}, {}",
                                            geo.city, geo.region, geo.country
                                        );
                                        if self.visibility.your_loc {
                                            if ui.selectable_label(true, &loc).clicked() {
                                                self.visibility.your_loc = false;
                                            }
                                        } else if ui.selectable_label(false, mask(&loc)).clicked() {
                                            self.visibility.your_loc = true;
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                    ui.end_row();
                                    // Your coordinates row.
                                    ui.label(s.coords);
                                    if let Some(geo) = &state.client_geo {
                                        let c = format!("{:.4}, {:.4}", geo.lat, geo.lon);
                                        if self.visibility.your_coords {
                                            if ui.selectable_label(true, &c).clicked() {
                                                self.visibility.your_coords = false;
                                            }
                                        } else if ui.selectable_label(false, mask(&c)).clicked() {
                                            self.visibility.your_coords = true;
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                    ui.end_row();
                                    // ISP row.
                                    ui.label(s.isp);
                                    if let Some(geo) = &state.client_geo {
                                        if self.visibility.isp && !geo.isp.is_empty() {
                                            if ui.selectable_label(true, &geo.isp).clicked() {
                                                self.visibility.isp = false;
                                            }
                                        } else if !geo.isp.is_empty() {
                                            if ui.selectable_label(false, mask(&geo.isp)).clicked()
                                            {
                                                self.visibility.isp = true;
                                            }
                                        } else {
                                            ui.label("-");
                                        }
                                    } else {
                                        ui.label("-");
                                    }
                                    ui.end_row();
                                });

                            // Advanced statistical confidence.
                            ui.add_space(8.0);
                            ui.separator();
                            ui.heading("Statistical Confidence");
                            let hist: Vec<f64> = state.offset_history.iter().copied().collect();
                            if hist.len() >= 2 {
                                let mut sorted = hist.clone();
                                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                                let pct = |q: f64| -> f64 {
                                    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
                                    sorted[idx.min(sorted.len() - 1)]
                                };
                                let mte = state.root_dispersion_secs * 1000.0
                                    + state.peer_dispersion * 1000.0
                                    + state.ping_m2.sqrt() * 2.0;
                                egui::Grid::new("stat_grid").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                                    ui.label("25th percentile"); ui.label(format!("{:.3} ms", pct(0.25))); ui.end_row();
                                    ui.label("50th percentile (median)"); ui.label(format!("{:.3} ms", pct(0.50))); ui.end_row();
                                    ui.label("75th percentile"); ui.label(format!("{:.3} ms", pct(0.75))); ui.end_row();
                                    ui.label("Max Time Error (MTE)"); ui.label(format!("{:.3} ms", mte)); ui.end_row();
                                    ui.label("Slew events (>128ms)"); ui.label(state.slew_count.to_string()); ui.end_row();
                                    ui.label("Samples"); ui.label(hist.len().to_string()); ui.end_row();
                                    ui.label("Filtered offset (best RTT)"); ui.label(format_offset(state.filtered_offset_secs)); ui.end_row();
                                });
                                // Allan deviation at tau = 1, 10, 100 samples.
                                ui.add_space(4.0);
                                ui.label("Allan deviation (offset stability):");
                                for tau in [1usize, 10, 100] {
                                    if hist.len() > tau * 2 {
                                        let mut sum = 0.0;
                                        let mut n = 0;
                                        for i in 0..(hist.len() - tau * 2) {
                                            let d = hist[i + 2 * tau] - 2.0 * hist[i + tau] + hist[i];
                                            sum += d * d;
                                            n += 1;
                                        }
                                        if n > 0 {
                                            let adev = (sum / (2.0 * n as f64)).sqrt();
                                            ui.label(format!("tau={tau}: {adev:.3} ms"));
                                        }
                                    }
                                }
                            } else {
                                ui.weak("Collecting samples...");
                            }
                        }
                        Tab::Calibration => {
                            ui.heading(s.calibration);
                            let mut armed = self.calibration_running.load(Ordering::Relaxed);
                            if ui.checkbox(&mut armed, s.cal_enable).changed() {
                                if armed {
                                    self.calibration_running.store(true, Ordering::Relaxed);
                                    let state = Arc::clone(&self.state);
                                    let running = Arc::clone(&self.calibration_running);
                                    thread::spawn(move || calibration_thread(state, running));
                                } else {
                                    self.calibration_running.store(false, Ordering::Relaxed);
                                }
                            }
                            if let Some(local) = state.local {
                                let now = local.timestamp() as f64
                                    + local.timestamp_subsec_millis() as f64 / 1000.0;
                                let secs_into_min = now % 60.0;
                                let wait = 60.0 - secs_into_min;
                                ui.label(fmt(s.cal_countdown, &[&format!("{:.1}", wait)]));
                            }
                            ui.label(s.cal_hint);
                            if ui.button(s.cal_mark).clicked() {
                                self.calibration_mark = state.local;
                            }
                            if let Some(mark) = self.calibration_mark {
                                ui.label(fmt(
                                    s.cal_marked,
                                    &[&mark.format("%Y-%m-%d %H:%M:%S%.3f").to_string()],
                                ));
                            }

                            // ---- Drift / crystal frequency test ----
                            ui.separator();
                            ui.heading(s.drift_test);
                            ui.horizontal(|ui| {
                                ui.label(s.drift_duration);
                                ui.selectable_value(&mut self.drift_duration, 30, "30 s");
                                ui.selectable_value(&mut self.drift_duration, 60, "60 s");
                                ui.selectable_value(&mut self.drift_duration, 120, "120 s");
                            });
                            ui.horizontal(|ui| {
                                if self.drift_running {
                                    if ui.button(s.drift_stop).clicked() {
                                        self.drift_running = false;
                                        self.drift_start_server = None;
                                        self.drift_start_local = None;
                                    }
                                } else if ui.button(s.drift_start).clicked()
                                    && let (Some(utc), Some(local)) = (state.utc, state.local) {
                                        self.drift_start_server = Some(
                                            utc.timestamp() as f64
                                                + utc.timestamp_subsec_millis() as f64 / 1000.0,
                                        );
                                        self.drift_start_local = Some(
                                            local.timestamp() as f64
                                                + local.timestamp_subsec_millis() as f64 / 1000.0,
                                        );
                                        self.drift_running = true;
                                        self.drift_result = None;
                                    }
                            });

                            // Side-by-side server vs device time (live during the test).
                            ui.horizontal(|ui| {
                                ui.label(s.server_time);
                                ui.label(s.device_time);
                            });
                            ui.horizontal(|ui| {
                                let server_str = state
                                    .utc
                                    .map(|u| u.format("%H:%M:%S%.3f").to_string())
                                    .unwrap_or_else(|| "-".to_string());
                                let device_str = state
                                    .local
                                    .map(|l| l.format("%H:%M:%S%.3f").to_string())
                                    .unwrap_or_else(|| "-".to_string());
                                ui.monospace(server_str);
                                ui.monospace(device_str);
                            });

                            // Update the running drift test.
                            if self.drift_running
                                && let (Some(ss), Some(sl)) =
                                    (self.drift_start_server, self.drift_start_local)
                                    && let Some(local) = state.local {
                                        let now_local = local.timestamp() as f64
                                            + local.timestamp_subsec_millis() as f64 / 1000.0;
                                        let elapsed = now_local - sl;
                                        let remaining = self.drift_duration as f64 - elapsed;
                                        if remaining > 0.0 {
                                            ui.label(fmt(
                                                s.drift_running,
                                                &[&format!("{:.0}", remaining)],
                                            ));
                                        } else {
                                            // Test complete: compute results.
                                            let server_now = state
                                                .utc
                                                .map(|u| {
                                                    u.timestamp() as f64
                                                        + u.timestamp_subsec_millis() as f64
                                                            / 1000.0
                                                })
                                                .unwrap_or(ss);
                                            let server_elapsed = server_now - ss;
                                            let local_elapsed = now_local - sl;
                                            let drift_secs = local_elapsed - server_elapsed;
                                            let drift_ppm = if server_elapsed > 0.0 {
                                                drift_secs / server_elapsed * 1e6
                                            } else {
                                                0.0
                                            };
                                            self.drift_result = Some(DriftResult {
                                                duration_secs: local_elapsed,
                                                drift_ms: drift_secs * 1000.0,
                                                drift_ppm,
                                            });
                                            self.drift_running = false;
                                            self.drift_start_server = None;
                                            self.drift_start_local = None;
                                        }
                                    }

                            if let Some(r) = &self.drift_result {
                                let fast_slow = if r.drift_ms >= 0.0 {
                                    s.drift_fast
                                } else {
                                    s.drift_slow
                                };
                                ui.label(fmt(
                                    s.drift_result,
                                    &[
                                        &format!("{:.1}", r.drift_ms.abs()),
                                        fast_slow,
                                        &format!("{:.0}", r.duration_secs),
                                        &format!("{:+.1}", r.drift_ppm),
                                    ],
                                ));
                                ui.label(fmt(s.drift_crystal, &[&format!("{:+.1}", r.drift_ppm)]));
                            }
                        }
                        Tab::Settings => {
                            ui.heading(s.tab_settings);
                            ui.label(s.visibility);
                            ui.horizontal(|ui| {
                                if ui.button(s.hide_all).clicked() {
                                    self.visibility = Visibility {
                                        server_ip: false,
                                        your_ip: false,
                                        server_loc: false,
                                        your_loc: false,
                                        server_coords: false,
                                        your_coords: false,
                                        isp: false,
                                        hostname: false,
                                    };
                                }
                                if ui.button(s.show_all).clicked() {
                                    self.visibility = Visibility::default();
                                }
                            });
                            ui.checkbox(&mut self.visibility.server_ip, s.server_ip);
                            ui.checkbox(&mut self.visibility.your_ip, s.your_ip);
                            ui.checkbox(&mut self.visibility.server_loc, s.server_loc);
                            ui.checkbox(&mut self.visibility.your_loc, s.your_loc);
                            ui.checkbox(&mut self.visibility.server_coords, s.coords);
                            ui.checkbox(&mut self.visibility.your_coords, s.coords);
                            ui.checkbox(&mut self.visibility.isp, s.isp);
                        }
                        Tab::Global => {
                            // Start the global probe worker the first time this tab is opened.
                            if !self.global_started {
                                self.global_started = true;
                                let gs = Arc::clone(&self.state);
                                thread::spawn(move || global_worker(gs));
                            }

                            ui.horizontal(|ui| {
                                ui.heading(s.tab_global);
                                if ui.button("⟳").on_hover_text("Refresh").clicked() {
                                    // Clear offsets and re-probe all global servers.
                                    state.global_offsets.clear();
                                    let gs = Arc::clone(&self.state);
                                    thread::spawn(move || global_worker(gs));
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(s.search);
                                ui.text_edit_singleline(&mut self.global_search);
                                ui.label(s.category);
                                egui::ComboBox::from_id_salt("global_cat")
                                    .selected_text(self.global_category.label())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::All,
                                            GlobalCategory::All.label(),
                                        );
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::Corporate,
                                            GlobalCategory::Corporate.label(),
                                        );
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::OS,
                                            GlobalCategory::OS.label(),
                                        );
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::National,
                                            GlobalCategory::National.label(),
                                        );
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::Pool,
                                            GlobalCategory::Pool.label(),
                                        );
                                        ui.selectable_value(
                                            &mut self.global_category,
                                            GlobalCategory::Vendor,
                                            GlobalCategory::Vendor.label(),
                                        );
                                    });
                            });
                            ui.separator();

                            let query = self.global_search.to_lowercase();
                            let cat = self.global_category.label();
                            egui::Grid::new("global_grid")
                                .num_columns(7)
                                .striped(true)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    ui.strong(s.name);
                                    ui.strong(s.hostname);
                                    ui.strong(s.ip);
                                    ui.strong(s.strategy);
                                    ui.strong(s.category);
                                    ui.strong(s.offset);
                                    ui.strong("");
                                    ui.end_row();

                                    for gs in GLOBAL_SERVERS {
                                        if self.global_category != GlobalCategory::All
                                            && gs.category != cat
                                        {
                                            continue;
                                        }
                                        if !query.is_empty()
                                            && !gs.name.to_lowercase().contains(&query)
                                            && !gs.hostname.to_lowercase().contains(&query)
                                        {
                                            continue;
                                        }
                                        ui.label(gs.name).on_hover_text(gs.notes);
                                        ui.label(gs.hostname);
                                        ui.label(gs.ip);
                                        ui.label(gs.strategy);
                                        ui.label(gs.category);
                                        let off = state.global_offsets.get(gs.hostname);
                                        match off {
                                            Some(o) => ui.label(format!("{:+.1} ms", o)),
                                            None => ui.label("..."),
                                        };
                                        if ui.button(s.add).clicked() {
                                            state.profiles.push(ServerProfile {
                                                name: gs.name.to_string(),
                                                hostname: gs.hostname.to_string(),
                                                ip: gs.ip.to_string(),
                                            });
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                        Tab::System => {
                            ui.heading("System Diagnostics");
                            ui.weak("Windows kernel, hardware timer, and OS clock state.");
                            ui.separator();

                            // All slow subprocess collection happens on a background
                            // thread; here we only read the cached results.
                            if !state.system_info.ready {
                                ui.spinner();
                                ui.label("Collecting system diagnostics...");
                                return;
                            }
                            let t = &state.system_info.timer;
                            let w = &state.system_info.w32;
                            let l = &state.system_info.load;
                            let p = &state.system_info.power;

                            // Section 1: Hardware timers.
                            ui.heading("Hardware Timers");
                            egui::Grid::new("sys_timer").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                                ui.label("QPC frequency"); ui.label(&t.qpc_frequency); ui.end_row();
                                ui.label("QPC resolution"); ui.label(&t.qpc_resolution); ui.end_row();
                                ui.label("QPC value"); ui.label(&t.qpc_value); ui.end_row();
                                ui.label("Timer resolution"); ui.label(&t.timer_resolution); ui.end_row();
                                ui.label("Timer min/max"); ui.label(format!("{} / {}", t.timer_resolution_min, t.timer_resolution_max)); ui.end_row();
                                ui.label("Clock adjustment"); ui.label(&t.clock_adjustment); ui.end_row();
                                ui.label("Clock increment"); ui.label(&t.clock_increment); ui.end_row();
                                ui.label("Clock discipline"); ui.label(&t.clock_disciplined); ui.end_row();
                                ui.label("System uptime"); ui.label(&t.uptime); ui.end_row();
                                ui.label("RTC vs OS"); ui.label(&t.rtc_vs_os); ui.end_row();
                            });
                            ui.add_space(8.0);

                            // Section 2: W32Time.
                            ui.heading("Windows Time Service (W32Time)");
                            egui::Grid::new("sys_w32").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                                ui.label("Source"); ui.label(if w.source.is_empty() { "n/a" } else { &w.source }); ui.end_row();
                                ui.label("Phase offset"); ui.label(if w.phase_offset.is_empty() { "n/a" } else { &w.phase_offset }); ui.end_row();
                                ui.label("Frequency"); ui.label(if w.frequency.is_empty() { "n/a" } else { &w.frequency }); ui.end_row();
                                ui.label("Poll interval"); ui.label(if w.poll_interval.is_empty() { "n/a" } else { &w.poll_interval }); ui.end_row();
                                ui.label("Last sync"); ui.label(if w.last_sync.is_empty() { "n/a" } else { &w.last_sync }); ui.end_row();
                            });
                            if !w.raw.is_empty() {
                                ui.collapsing("Raw w32tm output", |ui| {
                                    ui.monospace(&w.raw);
                                });
                            }
                            ui.add_space(8.0);

                            // Section 3: CPU load + power plan.
                            ui.heading("Load & Power");
                            egui::Grid::new("sys_load").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                                ui.label("CPU usage"); ui.label(&l.cpu_usage); ui.end_row();
                                ui.label("Context switches"); ui.label(&l.context_switches); ui.end_row();
                                ui.label("Interrupts"); ui.label(&l.interrupts); ui.end_row();
                                ui.label("Active power plan"); ui.label(&p.active_plan); ui.end_row();
                                ui.label("Power verdict"); ui.label(&p.verdict); ui.end_row();
                            });
                            if !l.warning.is_empty() {
                                ui.colored_label(egui::Color32::from_rgb(255, 180, 0), &l.warning);
                            }
                            ui.add_space(8.0);

                            // Section 4: Leap seconds (metrological calendar).
                            ui.heading("Leap Seconds");
                            egui::Grid::new("sys_leap").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                                ui.label("Last leap second"); ui.label(system::last_leap_second()); ui.end_row();
                                ui.label("Total since 1972"); ui.label(system::total_leap_seconds().to_string()); ui.end_row();
                            });
                            ui.collapsing("Full leap-second history", |ui| {
                                for (y, m, d) in system::leap_seconds() {
                                    ui.label(format!("{y:04}-{m:02}-{d:02}"));
                                }
                            });
                            ui.add_space(8.0);

                            // Section 5: Network path (traceroute).
                            ui.heading("Network Path");
                            let host = state.profiles[state.active_profile].hostname.clone();
                            ui.label(format!("Traceroute to {host}:"));
                            if state.system_info.hops.is_empty() {
                                ui.weak("No traceroute output (tracert unavailable or timed out).");
                            } else {
                                for hop in state.system_info.hops.iter().take(15) {
                                    ui.monospace(hop);
                                }
                            }
                        }
                    }
                    content_h = ui.min_rect().height();
                });
        });

        // Auto-resize only for the first few frames so the user can resize/maximize freely.
        if self.auto_resize_frames > 0 {
            self.auto_resize_frames -= 1;
            let max_h = (ctx.screen_rect().height() * 0.9).max(400.0);
            let desired_h = (content_h + 24.0).clamp(300.0, max_h);
            let desired = egui::vec2(640.0, desired_h);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired));
        }

        // Keep the UI repainting so the displayed time stays fresh.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn main() -> eframe::Result {
    let state = Arc::new(Mutex::new(ClockState {
        utc: None,
        local: None,
        unix_seconds: None,
        last_error: None,
        profiles: vec![ServerProfile::nz_default()],
        active_profile: 0,
        total_sent: 0,
        total_received: 0,
        sent_rate: 0.0,
        received_rate: 0.0,
        request_count: 0,
        response_count: 0,
        ping_ms: 0.0,
        ping_min: 0.0,
        ping_max: 0.0,
        ping_avg: 0.0,
        ping_count: 0,
        ping_m2: 0.0,
        ping_history: VecDeque::new(),
        offset_secs: 0.0,
        offset_min: 0.0,
        offset_max: 0.0,
        offset_count: 0,
        offset_mean: 0.0,
        offset_m2: 0.0,
        freq_ppm: 0.0,
        packet_loss_pct: 0.0,
        clock_status: ClockStatus::Unsynchronized,
        leap_indicator: 0,
        stratum: 0,
        poll_interval_secs: 0.0,
        root_delay_secs: 0.0,
        root_dispersion_secs: 0.0,
        precision_secs: 0.0,
        reference_id: String::new(),
        reference_timestamp: None,
        ntp_version: 0,
        ntp_mode: 0,
        origin_ntp: 0.0,
        receive_ntp: 0.0,
        transmit_ntp: 0.0,
        destination_ntp: 0.0,
        true_rtt_secs: 0.0,
        true_offset_secs: 0.0,
        client_ip: None,
        server_ip: None,
        client_geo: None,
        server_geo: None,
        peer_dispersion: 0.0,
        upstream_ms: 0.0,
        downstream_ms: 0.0,
        kod_code: None,
        extensions: String::new(),
        global_offsets: HashMap::new(),
        offset_history: VecDeque::new(),
        slew_count: 0,
        filter_window: VecDeque::new(),
        filtered_offset_secs: 0.0,
        system_info: SystemInfo::default(),
    }));

    // Spawn the NTP worker on a background thread so the UI never blocks.
    let worker_state = Arc::clone(&state);
    thread::spawn(move || ntp_worker(worker_state));

    // Spawn the geolocation worker (fetches client/server location once).
    let geo_state = Arc::clone(&state);
    thread::spawn(move || geo_worker(geo_state));

    // Spawn the System-tab diagnostics worker (slow subprocess calls run here).
    let sys_state = Arc::clone(&state);
    thread::spawn(move || system_worker(sys_state));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Master Clock",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(ClockApp::new(state)))
        }),
    )
}
