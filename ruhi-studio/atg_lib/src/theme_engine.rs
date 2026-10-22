//! # `theme_engine` — 深度系統布景主題與暗黑模式動態換膚引擎
//!
//! 對應 Python 版 `_lib/theme_engine.py`，提供：
//! 1. **語意調色盤 (`SemanticPalette`)**：「專業暗色 (`StudioDark`)」與「溫潤宣紙淺色 (`RicePaperLight`)」雙標準調色盤。
//! 2. **Windows 登錄檔即時探測**：透過 Win32 Registry API 讀取 `AppsUseLightTheme` 與 `DWM\AccentColor`，毫秒級零子程序開銷。
//! 3. **HSL 色彩空間動態微調 (`adjust_hsl`)**：自動由系統強調色推導懸停色 (`accent_hover`) 與選取底色 (`bg_selected`)。
//! 4. **Windows 10/11 DWM 沉浸式標題列 (`apply_win32_dwm_titlebar`)**：
//!    - 透過 `GetAncestor(hwnd, GA_ROOT)` 穿透子視窗鎖定頂層 HWND。
//!    - 同步注入 `DWMWA_USE_IMMERSIVE_DARK_MODE (20/19)`、`DWMWA_CAPTION_COLOR (35)` 與 `DWMWA_TEXT_COLOR (36)`。
//! 5. **系統主題變更監聽器 (`SystemThemeWatcher`)**：支援「跟隨系統 (`FollowSystem`)」模式下零延遲即時換膚。

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// 布景主題偏好設定模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// 自動跟隨作業系統深淺色與強調色
    #[default]
    #[serde(alias = "follow_system", alias = "auto")]
    System,
    /// 專業暗色模式 (Studio Dark)
    #[serde(alias = "studio_dark")]
    Dark,
    /// 溫潤宣紙淺色模式 (Rice-Paper Light)
    #[serde(alias = "rice_paper_light")]
    Light,
}

impl ThemePreference {
    /// 從設定檔字串解析主題偏好
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_lowercase().as_str() {
            "dark" | "studio_dark" => Self::Dark,
            "light" | "rice_paper_light" => Self::Light,
            _ => Self::System,
        }
    }

    /// 轉為設定檔儲存用標準代碼
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// 切換至下一個明暗主題（供 `Ctrl+T` 快捷鍵使用）
    pub fn toggle_dark_light(&self, current_is_dark: bool) -> Self {
        if current_is_dark {
            Self::Light
        } else {
            Self::Dark
        }
    }
}

/// 24-bit RGB 色彩結構，支援 Hex 轉換、HSL 明度/飽和度調整與 Win32 `COLORREF` (BGR) 轉換。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorRgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// 由 `#RRGGBB` 或 `RRGGBB` 十六進位字串解析色彩
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        let h = hex_str.trim().trim_start_matches('#');
        if h.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    /// 轉換為 `#RRGGBB` 格式字串
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// 轉換為 Windows DWM / GDI `COLORREF` (`0x00BBGGRR`)
    pub const fn to_win32_colorref(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
    }

    /// 在 HSL 色彩空間調整明度 (`l_delta`) 與飽和度 (`s_delta`)
    pub fn adjust_hsl(self, l_delta: f32, s_delta: f32) -> Self {
        let (h, s, l) = rgb_to_hsl(self.r, self.g, self.b);
        let new_l = (l + l_delta).clamp(0.05, 0.95);
        let new_s = (s + s_delta).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, new_s, new_l);
        Self { r, g, b }
    }

    /// 轉換為 `egui::Color32`
    #[cfg(feature = "ui")]
    pub const fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgb(self.r, self.g, self.b)
    }
}

/// 語意調色盤：涵蓋背景、表面、輸入框、前景文字、強調色、邊框與狀態色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticPalette {
    pub is_dark: bool,
    pub bg_main: ColorRgb,
    pub bg_surface: ColorRgb,
    pub bg_elevated: ColorRgb,
    pub bg_input: ColorRgb,
    pub bg_hover: ColorRgb,
    pub bg_selected: ColorRgb,
    pub row_alt: ColorRgb,
    pub fg_main: ColorRgb,
    pub fg_bright: ColorRgb,
    pub fg_muted: ColorRgb,
    pub fg_disabled: ColorRgb,
    pub accent: ColorRgb,
    pub accent_hover: ColorRgb,
    pub accent_fg: ColorRgb,
    pub border: ColorRgb,
    pub border_focus: ColorRgb,
    pub success: ColorRgb,
    pub warning: ColorRgb,
    pub error: ColorRgb,
}

impl SemanticPalette {
    /// 專業暗色調色盤 (`StudioDark`) — 對應 `_lib/theme_engine.py` `DARK_PALETTE`
    pub const fn studio_dark() -> Self {
        Self {
            is_dark: true,
            bg_main: ColorRgb::new(0x18, 0x1A, 0x1F),
            bg_surface: ColorRgb::new(0x21, 0x25, 0x2B),
            bg_elevated: ColorRgb::new(0x2C, 0x31, 0x3A),
            bg_input: ColorRgb::new(0x14, 0x16, 0x1A),
            bg_hover: ColorRgb::new(0x32, 0x38, 0x42),
            bg_selected: ColorRgb::new(0x26, 0x4F, 0x78),
            row_alt: ColorRgb::new(0x1D, 0x20, 0x26),
            fg_main: ColorRgb::new(0xE6, 0xEA, 0xEF),
            fg_bright: ColorRgb::new(0xFF, 0xFF, 0xFF),
            fg_muted: ColorRgb::new(0x8B, 0x94, 0x9E),
            fg_disabled: ColorRgb::new(0x48, 0x4F, 0x58),
            accent: ColorRgb::new(0x3B, 0x82, 0xF6),
            accent_hover: ColorRgb::new(0x60, 0xA5, 0xFA),
            accent_fg: ColorRgb::new(0xFF, 0xFF, 0xFF),
            border: ColorRgb::new(0x30, 0x36, 0x3D),
            border_focus: ColorRgb::new(0x58, 0xA6, 0xFF),
            success: ColorRgb::new(0x3F, 0xB9, 0x50),
            warning: ColorRgb::new(0xD2, 0x99, 0x22),
            error: ColorRgb::new(0xF8, 0x51, 0x49),
        }
    }

    /// 溫潤宣紙淺色調色盤 (`RicePaperLight`) — 對應 `_lib/theme_engine.py` `LIGHT_PALETTE`
    pub const fn rice_paper_light() -> Self {
        Self {
            is_dark: false,
            bg_main: ColorRgb::new(0xF6, 0xF5, 0xF0),
            bg_surface: ColorRgb::new(0xFF, 0xFF, 0xFF),
            bg_elevated: ColorRgb::new(0xEE, 0xEC, 0xE4),
            bg_input: ColorRgb::new(0xFF, 0xFF, 0xFF),
            bg_hover: ColorRgb::new(0xE5, 0xE2, 0xD9),
            bg_selected: ColorRgb::new(0xCE, 0xE3, 0xFA),
            row_alt: ColorRgb::new(0xF2, 0xF0, 0xE8),
            fg_main: ColorRgb::new(0x1F, 0x23, 0x28),
            fg_bright: ColorRgb::new(0x00, 0x00, 0x00),
            fg_muted: ColorRgb::new(0x57, 0x60, 0x6A),
            fg_disabled: ColorRgb::new(0x8C, 0x95, 0x9F),
            accent: ColorRgb::new(0x09, 0x69, 0xDA),
            accent_hover: ColorRgb::new(0x05, 0x50, 0xAE),
            accent_fg: ColorRgb::new(0xFF, 0xFF, 0xFF),
            border: ColorRgb::new(0xD0, 0xD7, 0xDE),
            border_focus: ColorRgb::new(0x09, 0x69, 0xDA),
            success: ColorRgb::new(0x1A, 0x7F, 0x37),
            warning: ColorRgb::new(0x9A, 0x67, 0x00),
            error: ColorRgb::new(0xCF, 0x22, 0x2E),
        }
    }

    /// 套用自訂或作業系統強調色，自動推導懸停色、焦點邊框與選取底色
    pub fn with_accent(mut self, sys_accent: ColorRgb) -> Self {
        if self.is_dark {
            let accent = sys_accent.adjust_hsl(0.08, 0.0);
            self.accent = accent;
            self.accent_hover = accent.adjust_hsl(0.12, 0.05);
            self.border_focus = accent;
            self.bg_selected = sys_accent.adjust_hsl(-0.20, -0.15);
        } else {
            let accent = sys_accent.adjust_hsl(-0.05, 0.0);
            self.accent = accent;
            self.accent_hover = accent.adjust_hsl(-0.10, 0.05);
            self.border_focus = accent;
            self.bg_selected = sys_accent.adjust_hsl(0.32, -0.10);
        }
        self
    }

    /// 根據主題偏好與系統設定產生最終調色盤
    pub fn resolve(pref: ThemePreference, use_system_accent: bool) -> Self {
        let is_dark = match pref {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => is_system_dark_mode(),
        };
        let base = if is_dark {
            Self::studio_dark()
        } else {
            Self::rice_paper_light()
        };
        if use_system_accent {
            if let Some(accent) = get_system_accent_color() {
                return base.with_accent(accent);
            }
        }
        base
    }
}

/// RGB (0..=255) 轉 HSL (0.0..=1.0)
pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) * 0.5;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - rf).abs() < f32::EPSILON {
        let mut h = (gf - bf) / d;
        if gf < bf {
            h += 6.0;
        }
        h / 6.0
    } else if (max - gf).abs() < f32::EPSILON {
        ((bf - rf) / d + 2.0) / 6.0
    } else {
        ((rf - gf) / d + 4.0) / 6.0
    };

    (h, s, l)
}

/// HSL (0.0..=1.0) 轉 RGB (0..=255)
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s.abs() < f32::EPSILON {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }

    fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 0.5 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let r = (hue_to_rgb(p, q, h + 1.0 / 3.0) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    let g = (hue_to_rgb(p, q, h) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (hue_to_rgb(p, q, h - 1.0 / 3.0) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (r, g, b)
}

/// 偵測作業系統目前是否處於深色模式 (Dark Mode)
pub fn is_system_dark_mode() -> bool {
    #[cfg(target_os = "windows")]
    {
        if let Some(val) = read_hkcu_dword(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            "AppsUseLightTheme",
        ) {
            return val == 0;
        }
        true
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return text.trim().eq_ignore_ascii_case("dark");
        }
        false
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        true
    }
}

/// 讀取 Windows 系統個人化強調色 (System Accent Color)
pub fn get_system_accent_color() -> Option<ColorRgb> {
    #[cfg(target_os = "windows")]
    {
        // 優先讀取 DWM AccentColor (ABGR DWORD)
        if let Some(abgr) = read_hkcu_dword("Software\\Microsoft\\Windows\\DWM", "AccentColor") {
            let r = (abgr & 0xFF) as u8;
            let g = ((abgr >> 8) & 0xFF) as u8;
            let b = ((abgr >> 16) & 0xFF) as u8;
            if !(r == 0 && g == 0 && b == 0) {
                return Some(ColorRgb::new(r, g, b));
            }
        }
        // 備援讀取 ColorizationColor (ARGB DWORD)
        if let Some(argb) =
            read_hkcu_dword("Software\\Microsoft\\Windows\\DWM", "ColorizationColor")
        {
            let r = ((argb >> 16) & 0xFF) as u8;
            let g = ((argb >> 8) & 0xFF) as u8;
            let b = (argb & 0xFF) as u8;
            if !(r == 0 && g == 0 && b == 0) {
                return Some(ColorRgb::new(r, g, b));
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn read_hkcu_dword(subkey: &str, value_name: &str) -> Option<u32> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };

    let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let val_w: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_w.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        if status != ERROR_SUCCESS {
            return None;
        }

        let mut data: u32 = 0;
        let mut data_len: u32 = std::mem::size_of::<u32>() as u32;
        let mut reg_type: u32 = 0;
        let q_status = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            (&mut data as *mut u32).cast::<u8>(),
            &mut data_len,
        );
        RegCloseKey(hkey);

        if q_status == ERROR_SUCCESS && reg_type == REG_DWORD {
            Some(data)
        } else {
            None
        }
    }
}

/// 系統外觀即時監聽器：定時向作業系統查詢深淺色與強調色是否變更。
#[derive(Debug, Clone)]
pub struct SystemThemeWatcher {
    poll_interval: Duration,
    last_check: Instant,
    last_is_dark: bool,
    last_accent: Option<ColorRgb>,
}

impl Default for SystemThemeWatcher {
    fn default() -> Self {
        Self::new(Duration::from_millis(1200))
    }
}

impl SystemThemeWatcher {
    pub fn new(poll_interval: Duration) -> Self {
        Self {
            poll_interval,
            last_check: Instant::now(),
            last_is_dark: is_system_dark_mode(),
            last_accent: get_system_accent_color(),
        }
    }

    /// 檢查系統深淺色或強調色是否發生變更；若有變更則回傳 `true`。
    pub fn poll_changed(&mut self) -> bool {
        if self.last_check.elapsed() < self.poll_interval {
            return false;
        }
        self.last_check = Instant::now();
        let cur_dark = is_system_dark_mode();
        let cur_accent = get_system_accent_color();
        if cur_dark != self.last_is_dark || cur_accent != self.last_accent {
            self.last_is_dark = cur_dark;
            self.last_accent = cur_accent;
            true
        } else {
            false
        }
    }

    pub fn last_is_dark(&self) -> bool {
        self.last_is_dark
    }

    pub fn last_accent(&self) -> Option<ColorRgb> {
        self.last_accent
    }
}

/// 為指定 Win32 視窗控制代碼 (`HWND`) 套用 Windows 10/11 DWM 沉浸式深色標題列與自訂色彩。
///
/// 嚴格遵循 ATG DWM 規範：
/// 1. 透過 `GetAncestor(hwnd, GA_ROOT)` 取得頂層外框視窗。
/// 2. 注入 `DWMWA_USE_IMMERSIVE_DARK_MODE` (20 與舊版 19)。
/// 3. 在 Windows 11 同步注入 `DWMWA_CAPTION_COLOR (35)` 與 `DWMWA_TEXT_COLOR (36)`。
pub fn apply_win32_dwm_titlebar(hwnd: isize, palette: &SemanticPalette) -> bool {
    if hwnd == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOT};

        let raw_hwnd = hwnd as HWND;
        let root_hwnd = {
            let anc = GetAncestor(raw_hwnd, GA_ROOT);
            if anc.is_null() {
                raw_hwnd
            } else {
                anc
            }
        };

        let dark_val: i32 = if palette.is_dark { 1 } else { 0 };
        // DWMWA_USE_IMMERSIVE_DARK_MODE = 20 (Win10 2004+ / Win11), 19 (Win10 1809)
        let _ = DwmSetWindowAttribute(
            root_hwnd,
            20,
            (&dark_val as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            root_hwnd,
            19,
            (&dark_val as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );

        // Windows 11 DWMWA_CAPTION_COLOR (35) & DWMWA_TEXT_COLOR (36)
        let caption_colorref: u32 = palette.bg_main.to_win32_colorref();
        let text_colorref: u32 = palette.fg_main.to_win32_colorref();
        let _ = DwmSetWindowAttribute(
            root_hwnd,
            35,
            (&caption_colorref as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            root_hwnd,
            36,
            (&text_colorref as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        true
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = palette;
        false
    }
}

/// 自動尋找目前程序所屬的活動頂層視窗並套用 DWM 沉浸式標題列色調。
pub fn apply_win32_dwm_titlebar_for_current_process(palette: &SemanticPalette) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let hwnd = GetForegroundWindow();
        if !hwnd.is_null() {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == GetCurrentProcessId() {
                return apply_win32_dwm_titlebar(hwnd as isize, palette);
            }
        }
        false
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = palette;
        false
    }
}

/// 將 `SemanticPalette` 與縮放倍率套用至 `egui::Context`
#[cfg(feature = "ui")]
pub fn apply_palette_to_egui(ctx: &egui::Context, palette: &SemanticPalette, ui_scale: f32) {
    let mut visuals = if palette.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    visuals.dark_mode = palette.is_dark;
    visuals.panel_fill = palette.bg_main.to_egui();
    visuals.window_fill = palette.bg_surface.to_egui();
    visuals.faint_bg_color = palette.row_alt.to_egui();
    visuals.extreme_bg_color = palette.bg_input.to_egui();
    visuals.code_bg_color = palette.bg_elevated.to_egui();

    visuals.override_text_color = Some(palette.fg_main.to_egui());
    visuals.hyperlink_color = palette.accent.to_egui();
    visuals.warn_fg_color = palette.warning.to_egui();
    visuals.error_fg_color = palette.error.to_egui();

    let border_stroke = egui::Stroke::new(1.0_f32, palette.border.to_egui());
    let focus_stroke = egui::Stroke::new(1.5_f32, palette.border_focus.to_egui());

    visuals.window_stroke = border_stroke;
    visuals.selection.bg_fill = palette.bg_selected.to_egui();
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, palette.fg_bright.to_egui());

    // 非互動狀態元件 (Non-interactive)
    visuals.widgets.noninteractive.bg_fill = palette.bg_surface.to_egui();
    visuals.widgets.noninteractive.weak_bg_fill = palette.bg_main.to_egui();
    visuals.widgets.noninteractive.bg_stroke = border_stroke;
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0_f32, palette.fg_main.to_egui());

    // 靜止互動元件 (Inactive)
    visuals.widgets.inactive.bg_fill = palette.bg_elevated.to_egui();
    visuals.widgets.inactive.weak_bg_fill = palette.bg_surface.to_egui();
    visuals.widgets.inactive.bg_stroke = border_stroke;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, palette.fg_main.to_egui());

    // 滑鼠懸停元件 (Hovered)
    visuals.widgets.hovered.bg_fill = palette.bg_hover.to_egui();
    visuals.widgets.hovered.weak_bg_fill = palette.bg_hover.to_egui();
    visuals.widgets.hovered.bg_stroke = focus_stroke;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, palette.fg_bright.to_egui());

    // 點擊啟動元件 (Active)
    visuals.widgets.active.bg_fill = palette.accent.to_egui();
    visuals.widgets.active.weak_bg_fill = palette.accent_hover.to_egui();
    visuals.widgets.active.bg_stroke = focus_stroke;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, palette.accent_fg.to_egui());

    // 展開選單元件 (Open)
    visuals.widgets.open.bg_fill = palette.bg_elevated.to_egui();
    visuals.widgets.open.weak_bg_fill = palette.bg_surface.to_egui();
    visuals.widgets.open.bg_stroke = focus_stroke;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, palette.fg_bright.to_egui());

    ctx.set_visuals(visuals);

    // 同步調整字體級距與控制項間距，達成雙層生命週期等比放大
    let scale = ui_scale.clamp(0.75, 2.50);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0 * scale, 6.0 * scale);
    style.spacing.button_padding = egui::vec2(10.0 * scale, 5.0 * scale);
    style.spacing.window_margin = egui::Margin::same((12.0 * scale).round() as i8);
    style.spacing.interact_size = egui::vec2(40.0 * scale, 24.0 * scale);
    style.spacing.combo_height = 240.0 * scale;

    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new((11.0 * scale).round(), FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new((14.0 * scale).round(), FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new((14.0 * scale).round(), FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new((18.0 * scale).round(), FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new((13.0 * scale).round(), FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_style(style);
}

/// 為 `egui::Context` 掛載系統正體中文、注音符號、康熙部首與符號字型（Microsoft JhengHei / Segoe UI Symbol）
#[cfg(feature = "ui")]
pub fn configure_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let candidate_cjk_paths = [
        "C:\\Windows\\Fonts\\msjh.ttc",
        "C:\\Windows\\Fonts\\msjh.ttf",
        "C:\\Windows\\Fonts\\mingliu.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];

    for path in candidate_cjk_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "atg_cjk".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.insert(0, "atg_cjk".to_owned());
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.push("atg_cjk".to_owned());
            }
            break;
        }
    }

    let candidate_sym_paths = [
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
    ];
    for (idx, path) in candidate_sym_paths.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let key = format!("atg_sym_{idx}");
            fonts.font_data.insert(
                key.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                vec.push(key.clone());
            }
            if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                vec.push(key);
            }
        }
    }

    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_hex_and_colorref() {
        let c = ColorRgb::from_hex("#181A1F").unwrap();
        assert_eq!(c, ColorRgb::new(0x18, 0x1A, 0x1F));
        assert_eq!(c.to_hex(), "#181A1F");
        // Win32 COLORREF 為 0x00BBGGRR
        assert_eq!(c.to_win32_colorref(), 0x001F1A18);
    }

    #[test]
    fn test_hsl_roundtrip_and_adjust() {
        let base = ColorRgb::from_hex("#3B82F6").unwrap();
        let lighter = base.adjust_hsl(0.12, 0.0);
        let darker = base.adjust_hsl(-0.12, 0.0);
        assert!(lighter.r >= base.r || lighter.g >= base.g || lighter.b >= base.b);
        assert!(darker.r <= base.r || darker.g <= base.g || darker.b <= base.b);
    }

    #[test]
    fn test_semantic_palettes_and_custom_accent() {
        let dark = SemanticPalette::studio_dark();
        assert!(dark.is_dark);
        assert_eq!(dark.bg_main.to_hex(), "#181A1F");

        let light = SemanticPalette::rice_paper_light();
        assert!(!light.is_dark);
        assert_eq!(light.bg_main.to_hex(), "#F6F5F0");

        let custom = dark.with_accent(ColorRgb::new(0x00, 0x78, 0xD4));
        assert_ne!(custom.accent, dark.accent);
    }

    #[test]
    fn test_theme_preference_parse() {
        assert_eq!(ThemePreference::from_code("dark"), ThemePreference::Dark);
        assert_eq!(ThemePreference::from_code("light"), ThemePreference::Light);
        assert_eq!(
            ThemePreference::from_code("system"),
            ThemePreference::System
        );
    }
}
