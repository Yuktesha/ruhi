//! 全域電源管理、螢幕節能關閉與 Win32 S3 硬體定時喚醒引擎 (`power_engine.rs`)
//!
//! 對應 Python 版 `_lib/power_engine.py` 與 `DJ_Next` 之 `power_win32.rs`。
//!
//! ### 核心功能
//! 1. **一鍵關閉螢幕節能 (`turn_off_screen`)**：
//!    發送 Win32 DPMS (`SC_MONITORPOWER = 2`) 或 macOS/Linux 顯示器斷電指令，
//!    預設帶有 `350ms` 放鍵防彈跳延遲（防止放開滑鼠左鍵或實體按鍵瞬間誤喚醒螢幕），並支援可選鎖定工作站 (`Win+L`)。
//! 2. **螢幕喚醒 (`wake_screen`)**：
//!    透過 `SetThreadExecutionState` 與微量游標位移訊號喚醒顯示器。
//! 3. **長任務防休眠鎖 (`prevent_screen_sleep` / `allow_screen_sleep`)**：
//!    在長時間批次轉檔、音訊混音或模型推論期間，阻止作業系統自動休眠或關閉顯示器。
//! 4. **BIOS / ACPI S3 硬體定時喚醒 (`HardwareWakeTimer`)**：
//!    使用 Win32 核心可等待計時器 (`CreateWaitableTimerW` + `SetWaitableTimer(..., fResume = 1)`)，
//!    可在系統進入 S3 睡眠後於指定秒數準時由硬體喚醒電腦。

use std::process::{Command, Stdio};
use std::time::Duration;

/// 關閉實體螢幕顯示器電源（具備放鍵防彈跳延遲與可選鎖定工作站）。
///
/// - `lock`: 若為 `true`，於關閉螢幕前先鎖定作業系統工作站 (`LockWorkStation`)。
/// - `delay_ms`: 放鍵防彈跳毫秒數（建議 `350` ms）。
pub fn turn_off_screen(lock: bool, delay_ms: u64) -> bool {
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }

    if lock {
        lock_workstation();
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND,
        };
        let res = PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER as usize, 2);
        return res != 0;
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("pmset")
            .arg("displaysleepnow")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let linux_cmds: &[&[&str]] = &[
            &["hyprctl", "dispatch", "dpms", "off"],
            &["swaymsg", "output * power off"],
            &["wlopm", "--off", "*"],
            &["xset", "dpms", "force", "off"],
        ];
        for cmd in linux_cmds {
            if let Ok(status) = Command::new(cmd[0])
                .args(&cmd[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                if status.success() {
                    return true;
                }
            }
        }
        false
    }
}

/// 鎖定作業系統工作站 (`Win+L` / `loginctl lock-session`)。
pub fn lock_workstation() -> bool {
    #[cfg(target_os = "windows")]
    {
        return Command::new("rundll32.exe")
            .args(["user32.dll,LockWorkStation"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[cfg(not(target_os = "windows"))]
    {
        for cmd in [
            &["loginctl", "lock-session"][..],
            &["xdg-screensaver", "lock"][..],
        ] {
            if let Ok(st) = Command::new(cmd[0])
                .args(&cmd[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                if st.success() {
                    return true;
                }
            }
        }
        false
    }
}

/// 喚醒實體螢幕顯示器。
pub fn wake_screen() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Power::{
            SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
        };
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{mouse_event, MOUSEEVENTF_MOVE};

        mouse_event(MOUSEEVENTF_MOVE, 0, 1, 0, 0);
        mouse_event(MOUSEEVENTF_MOVE, 0, -1, 0, 0);
        let prev =
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
        return prev != 0;
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("caffeinate")
            .args(["-u", "-t", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = Command::new("xset")
            .args(["dpms", "force", "on"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        true
    }
}

/// 設定或解除長時間任務防休眠鎖（防止轉檔、混音或推論期間系統自動進入睡眠或關閉顯示器）。
pub fn prevent_screen_sleep(enable: bool) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Power::{
            SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
        };
        let flags = if enable {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        return SetThreadExecutionState(flags) != 0;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = enable;
        true
    }
}

/// 恢復作業系統預設休眠與螢幕保護計時。
pub fn allow_screen_sleep() -> bool {
    prevent_screen_sleep(false)
}

/// 請求作業系統進入 S3 睡眠狀態 (`SetSuspendState`)。
pub fn enter_system_s3_sleep() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Power::SetSuspendState;
        return SetSuspendState(0, 0, 0) != 0;
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Windows 核心可等待硬體喚醒計時器 (`CreateWaitableTimerW` + `fResume = TRUE`)
#[derive(Debug, Default)]
pub struct HardwareWakeTimer {
    #[cfg(target_os = "windows")]
    handle: Option<isize>,
    pub armed_for_seconds: Option<u64>,
}

impl HardwareWakeTimer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 設定於 `seconds_from_now` 秒後由硬體喚醒系統（支援從 S3 睡眠喚醒）。
    pub fn arm_wake_timer(&mut self, seconds_from_now: u64) -> Result<(), String> {
        self.cancel();

        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Threading::SetWaitableTimer;

            #[link(name = "kernel32")]
            extern "system" {
                fn CreateWaitableTimerW(
                    lpTimerAttributes: *const core::ffi::c_void,
                    bManualReset: i32,
                    lpTimerName: *const u16,
                ) -> *mut core::ffi::c_void;
            }

            let handle = CreateWaitableTimerW(std::ptr::null(), 1, std::ptr::null());
            if handle.is_null() {
                return Err("CreateWaitableTimerW 呼叫失敗".to_string());
            }

            // 以 100 奈秒為單位之相對時間（負值代表自現在起算）
            let due_time_100ns: i64 = -((seconds_from_now.max(1) as i64) * 10_000_000);
            let ok = SetWaitableTimer(
                handle,
                &due_time_100ns as *const i64,
                0,
                None,
                std::ptr::null(),
                1, // fResume = TRUE
            );

            if ok == 0 {
                CloseHandle(handle);
                return Err(
                    "SetWaitableTimer(fResume=TRUE) 失敗，請確認電源計畫已允許喚醒計時器"
                        .to_string(),
                );
            }

            self.handle = Some(handle as isize);
            self.armed_for_seconds = Some(seconds_from_now);
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.armed_for_seconds = Some(seconds_from_now);
            Ok(())
        }
    }

    /// 取消已設定之硬體喚醒計時器並釋放核心控制代碼。
    pub fn cancel(&mut self) {
        #[cfg(target_os = "windows")]
        if let Some(h) = self.handle.take() {
            unsafe {
                use windows_sys::Win32::Foundation::CloseHandle;
                use windows_sys::Win32::System::Threading::CancelWaitableTimer;
                let ptr = h as *mut core::ffi::c_void;
                let _ = CancelWaitableTimer(ptr);
                let _ = CloseHandle(ptr);
            }
        }
        self.armed_for_seconds = None;
    }
}

impl Drop for HardwareWakeTimer {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_prevention_and_wake_timer_lifecycle() {
        assert!(prevent_screen_sleep(true));
        assert!(allow_screen_sleep());

        let mut timer = HardwareWakeTimer::new();
        assert!(timer.armed_for_seconds.is_none());
        let _ = timer.arm_wake_timer(3600);
        assert_eq!(timer.armed_for_seconds, Some(3600));
        timer.cancel();
        assert!(timer.armed_for_seconds.is_none());
    }
}
