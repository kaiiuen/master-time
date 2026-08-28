//! Windows system-level time diagnostics.
//!
//! Collects hardware timer precision, OS clock discipline, W32Time state,
//! CPU load, power plan status, leap-second data, and network path info.
//! Uses raw FFI so it works without the `windows` crate feature juggling.
//! All functions degrade gracefully.

use std::process::Command;

/// Hardware timer / kernel clock data.
#[derive(Default, Clone)]
pub struct TimerInfo {
    pub qpc_frequency: String,
    pub qpc_resolution: String,
    pub qpc_value: String,
    pub timer_resolution: String,
    pub timer_resolution_min: String,
    pub timer_resolution_max: String,
    pub clock_adjustment: String,
    pub clock_increment: String,
    pub clock_disciplined: String,
    pub uptime: String,
    pub rtc_vs_os: String,
}

/// W32Time service state.
#[derive(Default, Clone)]
pub struct W32TimeInfo {
    pub raw: String,
    pub source: String,
    pub phase_offset: String,
    pub frequency: String,
    pub poll_interval: String,
    pub last_sync: String,
}

/// CPU / system load data.
#[derive(Default, Clone)]
pub struct LoadInfo {
    pub cpu_usage: String,
    pub context_switches: String,
    pub interrupts: String,
    pub warning: String,
}

/// Power plan status.
#[derive(Default, Clone)]
pub struct PowerInfo {
    pub active_plan: String,
    pub verdict: String,
}

// ---------------------------------------------------------------------------
// Raw FFI declarations (Windows).
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryPerformanceFrequency(freq: *mut i64) -> i32;
    fn QueryPerformanceCounter(count: *mut i64) -> i32;
    fn GetTickCount64() -> u64;
    fn GetSystemTimeAdjustment(enabled: *mut i32, increment: *mut u32, adjustment: *mut u32) -> i32;
    fn GetSystemTimeAsFileTime(ft: *mut i64);
    fn GetSystemTimes(idle: *mut u64, kernel: *mut u64, user: *mut u64) -> i32;
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: *const std::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[cfg(windows)]
#[repr(C)]
struct SystemTime {
    year: u16,
    month: u16,
    dow: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    millis: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemTime(st: *mut SystemTime);
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryTimerResolution(cur: *mut u32, min: *mut u32, max: *mut u32) -> i32;
}

#[cfg(windows)]
#[link(name = "powrprof")]
unsafe extern "system" {
    fn PowerGetActiveScheme(user: *const u8, scheme: *mut *mut u8) -> i32;
    fn LocalFree(ptr: *mut u8) -> *mut u8;
}

// ---------------------------------------------------------------------------
// Public collectors.
// ---------------------------------------------------------------------------

pub fn collect_timer_info() -> TimerInfo {
    let mut t = TimerInfo::default();

    if let Some(freq) = qpc_frequency() {
        t.qpc_frequency = format!("{freq} Hz");
        t.qpc_resolution = format!("{:.3} ns", 1e9 / freq as f64);
    } else {
        t.qpc_frequency = "unavailable".to_string();
        t.qpc_resolution = "unavailable".to_string();
    }
    t.qpc_value = qpc_value().map(|v| v.to_string()).unwrap_or_else(|| "unavailable".to_string());

    let (cur, min, max) = timer_resolution();
    t.timer_resolution = format!("{:.3} ms", cur as f64 / 10000.0);
    t.timer_resolution_min = format!("{:.3} ms", min as f64 / 10000.0);
    t.timer_resolution_max = format!("{:.3} ms", max as f64 / 10000.0);

    let (enabled, increment, adjustment) = system_time_adjustment();
    t.clock_increment = format!("{:.3} ms", increment as f64 / 10000.0);
    t.clock_adjustment = format!("{:.3} ms", adjustment as f64 / 10000.0);
    t.clock_disciplined = match enabled {
        Some(true) => "Disciplined (slewing)".to_string(),
        Some(false) => "Not disciplined".to_string(),
        None => "unavailable".to_string(),
    };

    t.uptime = uptime().map(format_duration).unwrap_or_else(|| "unavailable".to_string());
    t.rtc_vs_os = rtc_vs_os().unwrap_or_else(|| "unavailable".to_string());

    t
}

pub fn collect_w32time() -> W32TimeInfo {
    let mut w = W32TimeInfo::default();
    let output = Command::new("w32tm").args(["/query", "/status"]).output();
    let text = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => String::from_utf8_lossy(&o.stderr).to_string(),
        Err(_) => return w,
    };
    w.raw = text.clone();
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("Source:") {
            w.source = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("Phase Offset:") {
            w.phase_offset = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("Frequency:") {
            w.frequency = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("Poll Interval:") {
            w.poll_interval = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("Last Successful Sync Time:") {
            w.last_sync = v.trim().to_string();
        }
    }
    w
}

/// Query a Windows performance counter (e.g. "\System\Context Switches/sec")
/// via PowerShell's Get-Counter and return the cooked value.
fn perf_counter(counter: &str) -> Option<f64> {
    let script = format!(
        "(Get-Counter '{}').CounterSamples[0].CookedValue",
        counter.replace('\'', "''")
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().parse::<f64>().ok()
}

pub fn collect_load() -> LoadInfo {
    let ctx = perf_counter("\\System\\Context Switches/sec");
    let irq = perf_counter("\\Processor(_Total)\\Interrupts/sec");
    LoadInfo {
        cpu_usage: cpu_usage().map(|p| format!("{p:.1}%")).unwrap_or_else(|| "unavailable".to_string()),
        context_switches: ctx
            .map(|v| format!("{v:.0}/s"))
            .unwrap_or_else(|| "n/a".to_string()),
        interrupts: irq
            .map(|v| format!("{v:.0}/s"))
            .unwrap_or_else(|| "n/a".to_string()),
        warning: String::new(),
    }
}

pub fn collect_power() -> PowerInfo {
    let active_plan = active_power_plan().unwrap_or_else(|| "unavailable".to_string());
    let lower = active_plan.to_lowercase();
    let verdict = if lower.contains("high performance") || lower.contains("ultimate") {
        "Good: QPC/TSC stability".to_string()
    } else if lower.contains("power saver") || lower.contains("balanced") {
        "Caution: cores may be parked / TSC throttled".to_string()
    } else {
        "Unknown".to_string()
    };
    PowerInfo { active_plan, verdict }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn qpc_frequency() -> Option<u64> {
    let mut f: i64 = 0;
    if unsafe { QueryPerformanceFrequency(&mut f) } != 0 {
        Some(f as u64)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn qpc_frequency() -> Option<u64> {
    None
}

#[cfg(windows)]
fn qpc_value() -> Option<u64> {
    let mut c: i64 = 0;
    if unsafe { QueryPerformanceCounter(&mut c) } != 0 {
        Some(c as u64)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn qpc_value() -> Option<u64> {
    None
}

#[cfg(windows)]
fn timer_resolution() -> (u32, u32, u32) {
    let mut cur: u32 = 0;
    let mut min: u32 = 0;
    let mut max: u32 = 0;
    if unsafe { NtQueryTimerResolution(&mut cur, &mut min, &mut max) } == 0 {
        (cur, min, max)
    } else {
        (0, 0, 0)
    }
}

#[cfg(not(windows))]
fn timer_resolution() -> (u32, u32, u32) {
    (0, 0, 0)
}

#[cfg(windows)]
fn system_time_adjustment() -> (Option<bool>, u32, u32) {
    let mut enabled: i32 = 0;
    let mut increment: u32 = 0;
    let mut adjustment: u32 = 0;
    unsafe { GetSystemTimeAdjustment(&mut enabled, &mut increment, &mut adjustment) };
    (Some(enabled != 0), increment, adjustment)
}

#[cfg(not(windows))]
fn system_time_adjustment() -> (Option<bool>, u32, u32) {
    (None, 0, 0)
}

#[cfg(windows)]
fn uptime() -> Option<u64> {
    Some(unsafe { GetTickCount64() })
}

#[cfg(not(windows))]
fn uptime() -> Option<u64> {
    None
}

#[cfg(windows)]
fn rtc_vs_os() -> Option<String> {
    let mut rtc = SystemTime {
        year: 0,
        month: 0,
        dow: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        millis: 0,
    };
    unsafe { GetSystemTime(&mut rtc) };
    let rtc_secs = rtc.second as i64 + rtc.minute as i64 * 60 + rtc.hour as i64 * 3600;
    let mut ft: i64 = 0;
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    let os_secs = (ft / 10_000_000) - 11_644_473_600;
    let os_secs_of_day = os_secs.rem_euclid(86400);
    let diff = rtc_secs - os_secs_of_day;
    Some(format!("{diff:+} s (RTC vs OS clock of day)"))
}

#[cfg(not(windows))]
fn rtc_vs_os() -> Option<String> {
    None
}

#[cfg(windows)]
fn cpu_usage() -> Option<f64> {
    let mut idle: u64 = 0;
    let mut kernel: u64 = 0;
    let mut user: u64 = 0;
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let total = kernel + user;
    if total == 0 {
        return None;
    }
    Some(100.0 * (1.0 - idle as f64 / total as f64))
}

#[cfg(not(windows))]
fn cpu_usage() -> Option<f64> {
    None
}

#[cfg(windows)]
fn active_power_plan() -> Option<String> {
    let mut scheme: *mut u8 = std::ptr::null_mut();
    if unsafe { PowerGetActiveScheme(std::ptr::null(), &mut scheme) } != 0 {
        return None;
    }
    if scheme.is_null() {
        return None;
    }
    // GUID is 16 bytes; format it.
    let bytes = unsafe { std::slice::from_raw_parts(scheme, 16) };
    let d1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let d2 = u16::from_le_bytes([bytes[4], bytes[5]]);
    let d3 = u16::from_le_bytes([bytes[6], bytes[7]]);
    let s = format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        d1, d2, d3, bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    );
    unsafe { LocalFree(scheme) };
    Some(s)
}

#[cfg(not(windows))]
fn active_power_plan() -> Option<String> {
    None
}

fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{d}d {h:02}h {m:02}m {s:02}s")
}

// ---------------------------------------------------------------------------
// Leap seconds (bundled metrological calendar).
// ---------------------------------------------------------------------------

/// Leap-second dates (UTC) as (year, month, day). The last one is 2017-01-01.
pub fn leap_seconds() -> Vec<(i32, u32, u32)> {
    vec![
        (1972, 6, 30), (1972, 12, 31), (1973, 12, 31), (1974, 12, 31),
        (1975, 12, 31), (1976, 12, 31), (1977, 12, 31), (1978, 12, 31),
        (1979, 12, 31), (1981, 6, 30), (1982, 6, 30), (1983, 6, 30),
        (1985, 6, 30), (1987, 12, 31), (1989, 12, 31), (1990, 12, 31),
        (1992, 6, 30), (1993, 6, 30), (1994, 6, 30), (1995, 12, 31),
        (1997, 6, 30), (1998, 12, 31), (2005, 12, 31), (2008, 12, 31),
        (2012, 6, 30), (2015, 6, 30), (2016, 12, 31),
    ]
}

/// The last leap second date as a string.
pub fn last_leap_second() -> String {
    let list = leap_seconds();
    let (y, m, d) = *list.last().unwrap_or(&(0, 0, 0));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Total number of leap seconds inserted since 1972.
pub fn total_leap_seconds() -> usize {
    leap_seconds().len()
}

// ---------------------------------------------------------------------------
// Network path diagnostics (traceroute via `tracert`).
// ---------------------------------------------------------------------------

/// Run a traceroute to the given host and return the hop lines.
pub fn traceroute(host: &str) -> Vec<String> {
    let output = Command::new("tracert")
        .args(["-d", "-h", "15", "-w", "500", host])
        .output();
    let text = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return Vec::new(),
    };
    text.lines()
        .filter(|l| l.trim().starts_with(|c: char| c.is_ascii_digit()))
        .map(|l| l.trim().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Set system time (elevated).
// ---------------------------------------------------------------------------

/// Set the Windows system clock to the given local time string (e.g.
/// "2026-08-04 14:30:00"). Uses ShellExecuteW with the "runas" verb so the
/// OS shows a UAC elevation prompt. Returns true if the elevated process was
/// launched (not whether it succeeded, since it runs asynchronously).
pub fn set_system_time(local: &str) -> bool {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;

        // PowerShell command that sets the system date/time from the NTP value.
        let ps = format!(
            "Set-Date -Date '{}'",
            local.replace('\'', "''")
        );
        let cmd = format!("powershell.exe -NoProfile -Command \"{}\"", ps);
        let op: Vec<u16> = OsStr::new("runas").encode_wide().chain(Some(0)).collect();
        let file: Vec<u16> = OsStr::new("cmd.exe").encode_wide().chain(Some(0)).collect();
        let params: Vec<u16> = OsStr::new(&format!("/c {}", cmd)).encode_wide().chain(Some(0)).collect();
        let dir: Vec<u16> = OsStr::new("").encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                ptr::null(),
                op.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                dir.as_ptr(),
                0, // SW_HIDE
            )
        };
        // ShellExecuteW returns > 32 on success.
        result as i32 > 32
    }
    #[cfg(not(windows))]
    {
        let _ = local;
        false
    }
}
