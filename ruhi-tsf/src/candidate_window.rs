use std::sync::{OnceLock, RwLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::panic::catch_unwind;
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::System::Threading::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use crate::logger;
use crate::service::RuhiConfig;

const WM_TRAYICON: u32 = WM_USER + 101;
const TRAY_ICON_ID: u32 = 1001;

#[derive(Clone, Debug)]
pub struct CandidateState {
    pub candidates: Vec<String>,
    pub selected_index: usize,
    pub page_offset: usize,
    pub page_size: usize,
    pub quick_key_mode: String,
    pub is_dark_theme: bool,
    pub dpi_scale: f32,

    // 如喜輸入座擴充 (RuhI Input Desk)
    pub input_display_mode: String, // "inline_underline", "floating_follow", "floating_fixed"
    pub is_candidates_only: bool,   // true = 微軟新注音純候選字浮動卡片, false = 如喜三層輸入座
    pub orientation: String,        // "horizontal" 或 "vertical"
    pub composition_text: String,
    pub cursor_pos: usize,
    pub pending_syllable: String,
    pub is_cand_selecting: bool,
    pub target_char_idx: usize,
    pub target_range: (usize, usize),
    pub selection_range: (usize, usize),
    pub is_caps_on: bool,
    pub font_size: f32,
    pub fixed_x: i32,
    pub fixed_y: i32,
    pub keyboard_mode: String,
    pub enabled_keyboard_modes: Vec<String>,
    pub gear_rect: RECT,
    pub mode_rect: RECT,
    pub max_composition_chars: usize,
}

static IS_DRAGGING: AtomicBool = AtomicBool::new(false);
static DRAG_START_CURSOR: Mutex<(i32, i32)> = Mutex::new((0, 0));
static DRAG_START_WINDOW: Mutex<(i32, i32)> = Mutex::new((0, 0));

static CANDIDATE_STATE: OnceLock<RwLock<CandidateState>> = OnceLock::new();


fn get_candidate_state() -> &'static RwLock<CandidateState> {
    CANDIDATE_STATE.get_or_init(|| {
        RwLock::new(CandidateState {
            candidates: Vec::new(),
            selected_index: 0,
            page_offset: 0,
            page_size: 9,
            quick_key_mode: "asdf".to_string(),
            is_dark_theme: CandidateWindow::detect_dark_theme(),
            dpi_scale: 1.0,
            input_display_mode: "inline_underline".to_string(),
            is_candidates_only: false,
            orientation: "horizontal".to_string(),
            composition_text: String::new(),
            cursor_pos: 0,
            pending_syllable: String::new(),
            is_cand_selecting: false,
            target_char_idx: 0,
            target_range: (0, 0),
            selection_range: (0, 0),
            is_caps_on: false,
            font_size: 32.0,
            fixed_x: 350,
            fixed_y: 550,
            keyboard_mode: "hsu".to_string(),
            enabled_keyboard_modes: vec!["hsu".to_string(), "japanese".to_string()],
            gear_rect: RECT::default(),
            mode_rect: RECT::default(),
            max_composition_chars: 30,
        })
    })
}

pub struct CandidateWindow {
    pub hwnd: HWND,
    tray_mutex: Option<HANDLE>,
    is_tray_registered: bool,
}

impl Drop for CandidateWindow {
    fn drop(&mut self) {
        unsafe {
            if self.is_tray_registered {
                let mut nid = NOTIFYICONDATAW::default();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = self.hwnd;
                nid.uID = TRAY_ICON_ID;
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            if !self.hwnd.0.is_null() && IsWindow(self.hwnd).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
            if let Some(h) = self.tray_mutex {
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
        }
    }
}

impl CandidateWindow {
    pub fn new() -> Result<Self> {
        unsafe {
            let is_dark_theme = Self::detect_dark_theme();
            let instance = GetModuleHandleW(None)?;
            let class_name = w!("RuhICandidateWindow");

            let mut wc = WNDCLASSW::default();
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.lpfnWndProc = Some(wnd_proc);
            wc.hInstance = instance.into();
            wc.hCursor = LoadCursorW(None, IDC_ARROW)?;
            wc.hbrBackground = HBRUSH(std::ptr::null_mut());
            wc.lpszClassName = class_name;
            let _ = RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class_name,
                w!("RuhICandidateWindow"),
                WS_POPUP,
                0, 0, 100, 100,
                None,
                None,
                instance,
                None,
            )?;

            let dpi = GetDpiForWindow(hwnd);
            let dpi_scale = if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 };

            if let Ok(mut state) = get_candidate_state().write() {
                state.is_dark_theme = is_dark_theme;
                state.dpi_scale = dpi_scale;
            }

            let (tray_mutex, is_tray_registered) = Self::try_register_tray_icon(hwnd);

            logger::log(&format!("CandidateWindow/InputDesk created: hwnd={:?}, dpi={}, tray={}", hwnd, dpi_scale, is_tray_registered));
            Ok(Self { hwnd, tray_mutex, is_tray_registered })
        }
    }

    unsafe fn try_register_tray_icon(hwnd: HWND) -> (Option<HANDLE>, bool) {
        let mutex_res = CreateMutexW(None, true, windows_core::w!("Local\\RuhiSystemTrayMutex"));
        let mutex = match mutex_res {
            Ok(h) => h,
            Err(_) => return (None, false),
        };

        if GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            return (Some(mutex), false);
        }

        let paths = crate::service::RuhiPaths::resolve();
        let ico_path = paths.sandbox_dir.parent().map(|p| p.join("ruhi.ico")).unwrap_or_else(|| paths.sandbox_dir.join("ruhi.ico"));
        let ico_wide: Vec<u16> = ico_path.to_string_lossy().encode_utf16().chain(Some(0)).collect();

        let hicon = LoadImageW(
            None,
            windows_core::PCWSTR(ico_wide.as_ptr()),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_LOADFROMFILE,
        ).map(|h| HICON(h.0)).unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = hicon;

        let tip = "RuhI 如喜輸入法 (點擊開啟設定)";
        let tip_wide: Vec<u16> = tip.encode_utf16().collect();
        for (i, &c) in tip_wide.iter().enumerate().take(nid.szTip.len() - 1) {
            nid.szTip[i] = c;
        }

        let reg = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
        if reg {
            logger::log("System Tray Icon registered successfully.");
        }
        (Some(mutex), reg)
    }

    pub fn is_cursor_over(&self) -> bool {
        unsafe {
            if !IsWindowVisible(self.hwnd).as_bool() {
                return false;
            }
            let mut pt = POINT::default();
            let mut rect = RECT::default();
            if GetCursorPos(&mut pt).is_ok() && GetWindowRect(self.hwnd, &mut rect).is_ok() {
                pt.x >= rect.left && pt.x <= rect.right && pt.y >= rect.top && pt.y <= rect.bottom
            } else {
                false
            }
        }
    }

    pub fn apply_config(&mut self, cfg: &RuhiConfig) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.quick_key_mode = cfg.candidate_quick_key.clone();
            state.input_display_mode = cfg.input_display_mode.clone();
            state.keyboard_mode = cfg.keyboard_mode.clone();
            state.enabled_keyboard_modes = cfg.enabled_keyboard_modes.clone();
            state.font_size = (cfg.font_size as f32).max(18.0);
            state.orientation = cfg.orientation.clone();
            state.fixed_x = cfg.desk_x;
            state.fixed_y = cfg.desk_y;
            state.max_composition_chars = cfg.max_composition_chars;
            if cfg.theme == "dark" {
                state.is_dark_theme = true;
            } else if cfg.theme == "light" {
                state.is_dark_theme = false;
            } else {
                state.is_dark_theme = Self::detect_dark_theme();
            }
        }
    }

    pub fn set_keyboard_mode(&mut self, mode: &str) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.keyboard_mode = mode.to_string();
        }
    }

    pub fn set_quick_key_mode(&mut self, mode: &str) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.quick_key_mode = mode.to_string();
        }
    }

    /// 顯示如喜輸入座 (RuhI Floating Input Desk) — 完全復刻如喜沙盒三層拓撲
    pub fn show_desk(
        &mut self,
        comp_text: &str,
        cursor_pos: usize,
        _pending_syl: &str,
        candidates: &[String],
        selected: usize,
        is_cand_sel: bool,
        target_char_idx: usize,
        target_range: (usize, usize),
        selection_range: (usize, usize),
        is_caps_on: bool,
        caret_x: i32,
        caret_y: i32,
    ) {
        let (dpi_scale, mode, font_size, fixed_x, fixed_y, max_chars) = {
            if let Ok(mut state) = get_candidate_state().write() {
                state.composition_text = comp_text.to_string();
                state.cursor_pos = cursor_pos;
                state.pending_syllable = String::new(); // 注音已包含在 comp_text 中，絕不重複疊加
                state.candidates = candidates.to_vec();
                state.selected_index = selected;
                state.is_cand_selecting = is_cand_sel;
                state.target_char_idx = target_char_idx;
                state.target_range = target_range;
                state.selection_range = selection_range;
                state.is_caps_on = is_caps_on;
                state.is_candidates_only = false;
                (state.dpi_scale, state.input_display_mode.clone(), state.font_size, state.fixed_x, state.fixed_y, state.max_composition_chars)
            } else {
                (1.0, "floating_follow".to_string(), 32.0, 350, 550, 30)
            }
        };

        let font_h = font_size * 0.9 * dpi_scale;
        let char_count = comp_text.chars().count().max(6);
        let comp_w = (40.0 * dpi_scale) + (char_count as f32 * font_h * 1.05);
        let cur_page = selected / 9;
        let page_items_count = candidates.iter().skip(cur_page * 9).take(9).count();
        let cand_w = if is_cand_sel && !candidates.is_empty() {
            (page_items_count.max(5) as f32 * 82.0 * dpi_scale) + (60.0 * dpi_scale)
        } else {
            0.0
        };

        let max_w_limit = ((max_chars.max(15) as f32 * font_h * 1.1) + 120.0 * dpi_scale).max(960.0 * dpi_scale);
        let width = comp_w.max(cand_w).clamp(420.0 * dpi_scale, max_w_limit) as i32;
        let row1_h = (font_h + 20.0 * dpi_scale) as i32; // 組字行高度
        let row2_h = if is_cand_sel && !candidates.is_empty() {
            (38.0 * dpi_scale) as i32 // 候選欄高度
        } else {
            0
        };
        let row3_h = (26.0 * dpi_scale) as i32; // 底部狀態列高度
        let height = row1_h + row2_h + row3_h + (16.0 * dpi_scale) as i32;

        let (screen_l, screen_t, screen_r, screen_b) = Self::get_monitor_work_area(caret_x, caret_y);
        let min_x = screen_l + 8;
        let max_x = (screen_r - width - 8).max(min_x);
        let min_y = screen_t + 8;
        let max_y = (screen_b - height - 8).max(min_y);

        let (final_x, final_y) = if mode == "floating_fixed" {
            let fx = fixed_x.clamp(min_x, max_x);
            let fy = fixed_y.clamp(min_y, max_y);
            (fx, fy)
        } else {
            // floating_follow (游標跟隨)
            let fx = caret_x.clamp(min_x, max_x);
            let fy = if caret_y + height > screen_b - 20 {
                (caret_y - height - 32).clamp(min_y, max_y)
            } else {
                caret_y.clamp(min_y, max_y)
            };
            (fx, fy)
        };

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                final_x, final_y, width, height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    fn get_monitor_work_area(x: i32, y: i32) -> (i32, i32, i32, i32) {
        let pt = POINT { x, y };
        let hmon = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(hmon, &mut mi) }.as_bool() {
            (mi.rcWork.left, mi.rcWork.top, mi.rcWork.right, mi.rcWork.bottom)
        } else {
            (0, 0, unsafe { GetSystemMetrics(SM_CXSCREEN) }, unsafe { GetSystemMetrics(SM_CYSCREEN) })
        }
    }

    /// 微軟新注音游標跟隨模式：僅在選字時彈出微軟新注音輕巧候選浮窗
    pub fn show_candidates_only(&mut self, candidates: &[String], selected: usize, x: i32, y: i32) {
        if candidates.is_empty() {
            self.hide();
            return;
        }

        let (dpi_scale, font_size, orientation) = if let Ok(s) = get_candidate_state().read() {
            (s.dpi_scale, s.font_size, s.orientation.clone())
        } else {
            (1.0, 32.0, "horizontal".to_string())
        };

        let cur_page = selected / 9;
        let page_start = cur_page * 9;
        let page_end = (page_start + 9).min(candidates.len());
        let page_candidates = &candidates[page_start..page_end];
        let display_count = page_candidates.len();

        let item_font_h = (font_size * 0.7 * dpi_scale).clamp(16.0 * dpi_scale, 28.0 * dpi_scale);

        let (width, height) = if orientation == "vertical" {
            // 直向微軟新注音候選窗
            let max_char_count = page_candidates.iter().map(|c| c.chars().count()).max().unwrap_or(2);
            let num_col_w = 32.0 * dpi_scale;
            let cand_col_w = (max_char_count as f32 * item_font_h * 1.15).max(60.0 * dpi_scale);
            let pad_x = 12.0 * dpi_scale;
            let w = ((pad_x * 2.0 + num_col_w + cand_col_w).max(160.0 * dpi_scale).min(500.0 * dpi_scale)) as i32;

            let item_h = (item_font_h + 12.0 * dpi_scale) as i32;
            let pad_y = (8.0 * dpi_scale) as i32;
            let has_more_pages = candidates.len() > 9;
            let footer_h = if has_more_pages { (24.0 * dpi_scale) as i32 } else { 0 };
            let h = pad_y * 2 + (display_count as i32 * item_h) + footer_h;
            (w, h)
        } else {
            // 橫向微軟新注音候選窗（單行精巧流式卡片，無任何組字空行與狀態條）
            let mut total_w = 20.0 * dpi_scale;
            for c in page_candidates {
                let char_cnt = c.chars().count();
                let item_w = (24.0 * dpi_scale) + (char_cnt as f32 * item_font_h * 1.1) + (16.0 * dpi_scale);
                total_w += item_w;
            }
            if candidates.len() > 9 {
                total_w += 65.0 * dpi_scale; // 頁碼指示空間
            }
            let w = (total_w.max(220.0 * dpi_scale).min(1100.0 * dpi_scale)) as i32;
            let h = (item_font_h + 20.0 * dpi_scale) as i32;
            (w, h)
        };

        let (screen_l, screen_t, screen_r, screen_b) = Self::get_monitor_work_area(x, y);
        let min_x = screen_l + 8;
        let max_x = (screen_r - width - 8).max(min_x);
        let min_y = screen_t + 8;
        let max_y = (screen_b - height - 8).max(min_y);

        let final_x = x.clamp(min_x, max_x);
        // 微軟新注音選字窗在游標下方（約 +24px），若到底部則放上方
        let caret_down_y = y + (24.0 * dpi_scale) as i32;
        let final_y = if caret_down_y + height > screen_b - 12 {
            (y - height - 8).clamp(min_y, max_y)
        } else {
            caret_down_y.clamp(min_y, max_y)
        };

        if let Ok(mut state) = get_candidate_state().write() {
            state.candidates = candidates.to_vec();
            state.selected_index = selected;
            state.page_offset = page_start;
            state.is_cand_selecting = true;
            state.is_candidates_only = true;
            state.composition_text.clear();
        }

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                final_x, final_y, width, height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn set_selected(&mut self, index: usize) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.selected_index = index;
        }
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    pub fn detect_dark_theme() -> bool {
        use windows::Win32::System::Registry::*;
        let mut hkey = HKEY::default();
        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                0,
                KEY_READ,
                &mut hkey,
            ) == ERROR_SUCCESS
            {
                let mut data: u32 = 0;
                let mut data_size = std::mem::size_of::<u32>() as u32;
                if RegQueryValueExW(
                    hkey,
                    w!("AppsUseLightTheme"),
                    None,
                    None,
                    Some(&mut data as *mut _ as *mut u8),
                    Some(&mut data_size),
                ) == ERROR_SUCCESS
                {
                    let _ = RegCloseKey(hkey);
                    return data == 0;
                }
                let _ = RegCloseKey(hkey);
            }
        }
        false
    }
}

pub fn save_desk_position(x: i32, y: i32) {
    let paths = crate::service::RuhiPaths::resolve();
    if paths.config.exists() {
        if let Ok(content) = std::fs::read_to_string(&paths.config) {
            if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                val["desk_x"] = serde_json::Value::from(x);
                val["desk_y"] = serde_json::Value::from(y);
                if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                    let _ = std::fs::write(&paths.config, new_content);
                }
            }
        }
    }
}

pub fn launch_settings() {
    let paths = crate::service::RuhiPaths::resolve();
    let candidates = [
        paths.sandbox_dir.join("settings_window.py"),
        paths.sandbox_dir.join("scripts").join("settings_window.py"),
        paths.sandbox_dir.parent().map(|p| p.join("dist").join("settings_window.py")).unwrap_or_default(),
        paths.sandbox_dir.parent().map(|p| p.join("scripts").join("settings_window.py")).unwrap_or_default(),
        std::path::PathBuf::from("C:\\ATGprjs\\ruhi\\dist\\settings_window.py"),
        std::path::PathBuf::from("C:\\ATGprjs\\ruhi\\scripts\\settings_window.py"),
        std::path::PathBuf::from("C:\\ATGprjs\\ruhi\\settings_window.py"),
    ];

    let py_script = candidates.iter().find(|p| p.exists() && p.is_file()).cloned();
    let script_path = match py_script {
        Some(p) => p,
        None => {
            logger::log("launch_settings: settings_window.py not found in any candidate path!");
            return;
        }
    };
    logger::log(&format!("launch_settings: launching {:?}", script_path));

    let work_dir = script_path.parent().unwrap_or(&paths.sandbox_dir);
    let dir_str: Vec<u16> = work_dir.to_string_lossy().encode_utf16().chain(Some(0)).collect();

    let exe_path = if std::path::Path::new("C:\\Python\\pythonw.exe").exists() {
        std::path::PathBuf::from("C:\\Python\\pythonw.exe")
    } else if std::path::Path::new("C:\\Python\\python.exe").exists() {
        std::path::PathBuf::from("C:\\Python\\python.exe")
    } else {
        std::path::PathBuf::from("pythonw.exe")
    };
    let args = format!("\"{}\"", script_path.to_string_lossy());

    let exe_wide: Vec<u16> = exe_path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    let args_wide: Vec<u16> = args.encode_utf16().chain(Some(0)).collect();

    unsafe {
        let _ = ShellExecuteW(
            None,
            windows_core::w!("open"),
            windows_core::PCWSTR(exe_wide.as_ptr()),
            windows_core::PCWSTR(args_wide.as_ptr()),
            windows_core::PCWSTR(dir_str.as_ptr()),
            SW_SHOW,
        );
    }
}

pub fn show_about_dialog(hwnd: HWND) {
    let (kb_mode, is_caps) = if let Ok(s) = get_candidate_state().read() {
        (s.keyboard_mode.to_lowercase(), s.is_caps_on)
    } else {
        ("hsu".to_string(), false)
    };

    let (title, msg) = if is_caps || kb_mode == "english" || kb_mode == "latin" || kb_mode == "western" {
        (
            "About RuhI",
            "RuhI Universal Smart IME\nVersion: 1.0.0 (Release)\nFlowing like clouds and water, effortless and joyful."
        )
    } else {
        match kb_mode.as_str() {
            "japanese" | "kana" | "romaji" => (
                "RuhI について",
                "RuhI 如喜スマートIME\nバージョン: 1.0.0 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            "korean" | "hangul" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기\n버전: 1.0.0 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            "iast" | "sanskrit" => (
                "About RuhI (IAST)",
                "RuhI Universal Smart IME (IAST)\nVersion: 1.0.0 (Release)\nYathā meghāḥ calanti yathā vāri vahati, svacchandataḥ ānandaḥ.\n(如雲行水流自在歡喜)"
            ),
            "thai" => (
                "เกี่ยวกับ RuhI",
                "RuhI Universal Smart IME\nเวอร์ชัน: 1.0.0 (Release)\nดั่งเมฆเคลื่อน ดั่งสายน้ำไหล สบายใจและเปี่ยมสุข"
            ),
            // 中文模式（許氏、大千、倉頡、倚天、拼音、大易、行列等）：
            _ => (
                "關於如喜",
                "RuhI 如喜通用智慧型輸入法\n版本: 1.0.0 (Release)\n如雲行水流自在歡喜。"
            ),
        }
    };

    let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let msg_wide: Vec<u16> = msg.encode_utf16().chain(Some(0)).collect();

    unsafe {
        let _ = MessageBoxW(
            hwnd,
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn get_context_menu_labels() -> (&'static str, &'static str) {
    let (kb_mode, is_caps) = if let Ok(s) = get_candidate_state().read() {
        (s.keyboard_mode.to_lowercase(), s.is_caps_on)
    } else {
        ("hsu".to_string(), false)
    };

    if is_caps || kb_mode == "english" || kb_mode == "latin" || kb_mode == "western" {
        ("Preferences Center (&S)...", "About RuhI (&A)")
    } else {
        match kb_mode.as_str() {
            "japanese" | "kana" | "romaji" => ("設定センター (&S)...", "RuhI について (&A)"),
            "korean" | "hangul" => ("환경설정 (&S)...", "RuhI 정보 (&A)"),
            _ => ("偏好設定中心 (&S)...", "關於 RuhI 如喜 (&A)"),
        }
    }
}


unsafe fn paint_candidate_window(hwnd: HWND, hdc: HDC) {
    let state = if let Ok(s) = get_candidate_state().read() {
        s.clone()
    } else {
        return;
    };

    let mut rect = RECT::default();
    if GetClientRect(hwnd, &mut rect).is_err() {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return;
    }

    let mem_dc = CreateCompatibleDC(hdc);
    if mem_dc.0.is_null() {
        return;
    }
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    if mem_bmp.0.is_null() {
        let _ = DeleteDC(mem_dc);
        return;
    }
    let old_bmp = SelectObject(mem_dc, mem_bmp);

    // 【沙盒雙調色盤精確繼承 (100% Matching Ruhi Sandbox main_window.py)】
    let (bg_main, bg_comp, bg_cand, ink_primary, ink_secondary, ink_muted, accent_commit, accent_blue, accent_num, border_color) = if state.is_dark_theme {
        (
            COLORREF(0x00221E1E), // #1E1E22 主底色
            COLORREF(0x00302525), // #252530 組字區底色
            COLORREF(0x001E1A1A), // #1A1A1E 候選欄底色
            COLORREF(0x00ECE8E8), // #E8E8EC 主墨色 (候選字與白字)
            COLORREF(0x00B8B0B0), // #B0B0B8 副墨色 (半/許)
            COLORREF(0x00787070), // #707078 淡墨色 (狀態文字)
            COLORREF(0x0099D334), // #34D399 翠玉綠 (組字漢字最美麗的核心代表色！)
            COLORREF(0x00FAA560), // #60A5FA 選中藍
            COLORREF(0x00F88C81), // #818CF8 序號紫
            COLORREF(0x00423A3A), // #3A3A42 邊框
        )
    } else {
        (
            COLORREF(0x00F0F5F7), // #F7F5F0 溫潤宣紙底色
            COLORREF(0x00F5FDFF), // #FFFDF5 組字區底色
            COLORREF(0x00E5EDF0), // #F0EDE5 候選欄底色
            COLORREF(0x00211D1C), // #1C1D21 主墨色
            COLORREF(0x00574D4A), // #4A4D57 副墨色
            COLORREF(0x009D928E), // #8E929D 淡墨色
            COLORREF(0x00699605), // #059669 翠玉深綠
            COLORREF(0x00EB6325), // #2563EB 選中藍
            COLORREF(0x00F16663), // #6366F1 序號紫
            COLORREF(0x00CDD7DD), // #DDD7CD 邊框
        )
    };

    // 1. 填滿整體底色 (bg_main) 與外圍細緻邊框
    let main_brush = CreateSolidBrush(bg_main);
    FillRect(mem_dc, &rect, main_brush);
    let _ = DeleteObject(main_brush);

    let border_brush = CreateSolidBrush(border_color);
    FrameRect(mem_dc, &rect, border_brush);

    SetBkMode(mem_dc, TRANSPARENT);
    let font_name: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();

    // 判斷渲染模式：凡微軟新注音模式或 show_candidates_only，一律以極致微軟新注音輕巧候選卡片呈現
    if state.is_candidates_only || state.input_display_mode == "inline_underline" {
        let cur_page = state.selected_index / 9;
        let page_start = cur_page * 9;
        let page_end = (page_start + 9).min(state.candidates.len());
        let total_pages = (state.candidates.len() + 8) / 9;
        let asdf_keys = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

        let item_font_h = (state.font_size * 0.7 * state.dpi_scale).clamp(16.0 * state.dpi_scale, 28.0 * state.dpi_scale);
        let font_height = -(item_font_h).round() as i32;
        let font = CreateFontW(
            font_height, 0, 0, 0,
            FW_NORMAL.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(font_name.as_ptr()),
        );
        let old_font = SelectObject(mem_dc, font);

        if state.orientation == "vertical" {
            // ── 直向微軟新注音候選清單 ──
            let pad_x = (10.0 * state.dpi_scale) as i32;
            let pad_y = (8.0 * state.dpi_scale) as i32;
            let item_h = (item_font_h + 12.0 * state.dpi_scale) as i32;
            let num_col_w = (28.0 * state.dpi_scale) as i32;

            for i in page_start..page_end {
                let row = (i - page_start) as i32;
                let item_top = pad_y + row * item_h;
                let item_bottom = item_top + item_h;
                let is_selected = i == state.selected_index;

                if is_selected {
                    let sel_rect = RECT {
                        left: 4,
                        top: item_top + 1,
                        right: width - 4,
                        bottom: item_bottom - 1,
                    };
                    let sel_brush = CreateSolidBrush(accent_blue);
                    FillRect(mem_dc, &sel_rect, sel_brush);
                    let _ = DeleteObject(sel_brush);
                }

                let idx = i - page_start;
                let key_label = if state.quick_key_mode == "asdf" {
                    if idx < asdf_keys.len() { asdf_keys[idx].to_string() } else { " ".to_string() }
                } else {
                    (idx + 1).to_string()
                };

                // 序號
                SetTextColor(mem_dc, if is_selected { COLORREF(0x00FFFFFF) } else { accent_num });
                let num_str = format!("{}.", key_label);
                let mut num_wide: Vec<u16> = num_str.encode_utf16().chain(Some(0)).collect();
                let mut num_rect = RECT {
                    left: pad_x,
                    top: item_top,
                    right: pad_x + num_col_w,
                    bottom: item_bottom,
                };
                DrawTextW(mem_dc, &mut num_wide, &mut num_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                // 候選字詞
                SetTextColor(mem_dc, if is_selected { COLORREF(0x00FFFFFF) } else { ink_primary });
                let mut cand_wide: Vec<u16> = state.candidates[i].encode_utf16().chain(Some(0)).collect();
                let mut cand_rect = RECT {
                    left: pad_x + num_col_w + 4,
                    top: item_top,
                    right: width - pad_x,
                    bottom: item_bottom,
                };
                DrawTextW(mem_dc, &mut cand_wide, &mut cand_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            // 底部翻頁提示
            if total_pages > 1 {
                let footer_y = height - (20.0 * state.dpi_scale) as i32;
                let sep_rect = RECT { left: 8, top: footer_y - 2, right: width - 8, bottom: footer_y - 1 };
                let sep_brush = CreateSolidBrush(border_color);
                FillRect(mem_dc, &sep_rect, sep_brush);
                let _ = DeleteObject(sep_brush);

                SetTextColor(mem_dc, ink_muted);
                let page_str = format!("{}/{} ▾", cur_page + 1, total_pages);
                let mut page_wide: Vec<u16> = page_str.encode_utf16().chain(Some(0)).collect();
                let mut page_rect = RECT {
                    left: pad_x,
                    top: footer_y,
                    right: width - pad_x,
                    bottom: height - 2,
                };
                DrawTextW(mem_dc, &mut page_wide, &mut page_rect, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
            }
        } else {
            // ── 橫向微軟新注音候選卡片（單行精巧流式排列）──
            let pad_y = (6.0 * state.dpi_scale) as i32;
            let item_h = height - pad_y * 2;
            let mut cur_x = (10.0 * state.dpi_scale) as i32;

            for i in page_start..page_end {
                let idx = i - page_start;
                let cand_chars = state.candidates[i].chars().count();
                let item_w = (24.0 * state.dpi_scale) as i32 + (cand_chars as f32 * item_font_h * 1.1) as i32 + (16.0 * state.dpi_scale) as i32;
                let is_selected = i == state.selected_index;

                if is_selected {
                    let sel_rect = RECT {
                        left: cur_x,
                        top: pad_y,
                        right: cur_x + item_w,
                        bottom: pad_y + item_h,
                    };
                    let sel_brush = CreateSolidBrush(accent_blue);
                    FillRect(mem_dc, &sel_rect, sel_brush);
                    let _ = DeleteObject(sel_brush);
                }

                let key_label = if state.quick_key_mode == "asdf" {
                    if idx < asdf_keys.len() { asdf_keys[idx].to_string() } else { " ".to_string() }
                } else {
                    (idx + 1).to_string()
                };

                // 序號
                SetTextColor(mem_dc, if is_selected { COLORREF(0x00FFFFFF) } else { accent_num });
                let num_str = format!("{}.", key_label);
                let mut num_wide: Vec<u16> = num_str.encode_utf16().chain(Some(0)).collect();
                let mut num_rect = RECT {
                    left: cur_x + 4,
                    top: pad_y,
                    right: cur_x + (22.0 * state.dpi_scale) as i32,
                    bottom: pad_y + item_h,
                };
                DrawTextW(mem_dc, &mut num_wide, &mut num_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                // 候選字詞
                SetTextColor(mem_dc, if is_selected { COLORREF(0x00FFFFFF) } else { ink_primary });
                let mut cand_wide: Vec<u16> = state.candidates[i].encode_utf16().chain(Some(0)).collect();
                let mut cand_rect = RECT {
                    left: cur_x + (24.0 * state.dpi_scale) as i32,
                    top: pad_y,
                    right: cur_x + item_w - 4,
                    bottom: pad_y + item_h,
                };
                DrawTextW(mem_dc, &mut cand_wide, &mut cand_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                cur_x += item_w + (6.0 * state.dpi_scale) as i32;
            }

            // 右側頁碼提示
            if total_pages > 1 {
                SetTextColor(mem_dc, ink_muted);
                let page_str = format!("{}/{} ▾", cur_page + 1, total_pages);
                let mut page_wide: Vec<u16> = page_str.encode_utf16().chain(Some(0)).collect();
                let mut page_rect = RECT {
                    left: cur_x + 4,
                    top: pad_y,
                    right: width - 8,
                    bottom: pad_y + item_h,
                };
                DrawTextW(mem_dc, &mut page_wide, &mut page_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
        }

        SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font);
    } else {
        // 【如喜輸入座 (RuhI Input Desk) — 徹底還原如喜沙盒視覺美學】
        let pad_x = (8.0 * state.dpi_scale) as i32;
        let mut cur_y = (6.0 * state.dpi_scale) as i32;

        let font_h = state.font_size * 0.9 * state.dpi_scale;
        let comp_card_h = (font_h + 18.0 * state.dpi_scale) as i32;
        let comp_rect = RECT {
            left: pad_x,
            top: cur_y,
            right: width - pad_x,
            bottom: cur_y + comp_card_h,
        };

        // 1. 組字卡片 (Composition Zone) — 底色 bg_comp
        let comp_brush = CreateSolidBrush(bg_comp);
        FillRect(mem_dc, &comp_rect, comp_brush);
        let _ = DeleteObject(comp_brush);
        FrameRect(mem_dc, &comp_rect, border_brush);

        // 組字字體：字級宏大、典雅翠玉綠 (accent_commit #34D399)
        let comp_font_h = -(font_h).round() as i32;
        let comp_font = CreateFontW(
            comp_font_h, 0, 0, 0,
            FW_NORMAL.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(font_name.as_ptr()),
        );
        let old_f = SelectObject(mem_dc, comp_font);

        let mut pen_x = comp_rect.left + (12.0 * state.dpi_scale) as i32;
        let _comp_inner_h = comp_rect.bottom - comp_rect.top;
        let comp_chars: Vec<char> = state.composition_text.chars().collect();
        let cur = state.cursor_pos.min(comp_chars.len());

        let sel_bg_brush = CreateSolidBrush(accent_blue);
        let cursor_brush = CreateSolidBrush(accent_commit);

        // 逐字繪製：直接從左側書寫，無任何多餘藍色標籤
        for (i, &ch) in comp_chars.iter().enumerate() {
            // 繪製游標垂直線
            if i == cur {
                let caret_r = RECT {
                    left: pen_x,
                    top: comp_rect.top + (6.0 * state.dpi_scale) as i32,
                    right: pen_x + (2.0 * state.dpi_scale).max(2.0) as i32,
                    bottom: comp_rect.bottom - (6.0 * state.dpi_scale) as i32,
                };
                FillRect(mem_dc, &caret_r, cursor_brush);
                pen_x += (3.0 * state.dpi_scale) as i32;
            }

            let ch_str = ch.to_string();
            let ch_wide: Vec<u16> = ch_str.encode_utf16().collect();
            let mut sz_ch = SIZE::default();
            let _ = GetTextExtentPoint32W(mem_dc, &ch_wide, &mut sz_ch);

            // 若正處於候選字選取模式且此字為目標選字（支援單字與多字詞區間），或處於 Shift+方向鍵手動選取範圍：以 accent_blue 高亮底色包覆
            let is_in_target = if state.target_range.1 > state.target_range.0 {
                i >= state.target_range.0 && i < state.target_range.1
            } else {
                i == state.target_char_idx
            };

            let is_in_selection = if state.selection_range.1 > state.selection_range.0 {
                i >= state.selection_range.0 && i < state.selection_range.1
            } else {
                false
            };

            if (state.is_cand_selecting && is_in_target) || is_in_selection {
                let hl_r = RECT {
                    left: pen_x - 1,
                    top: comp_rect.top + (4.0 * state.dpi_scale) as i32,
                    right: pen_x + sz_ch.cx + 2,
                    bottom: comp_rect.bottom - (4.0 * state.dpi_scale) as i32,
                };
                FillRect(mem_dc, &hl_r, sel_bg_brush);
                SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
            } else {
                SetTextColor(mem_dc, accent_commit); // 碧玉翠綠色
            }

            let mut r_ch = RECT { left: pen_x, top: comp_rect.top, right: pen_x + sz_ch.cx + 2, bottom: comp_rect.bottom };
            let mut c_w = ch_wide;
            c_w.push(0);
            DrawTextW(mem_dc, &mut c_w, &mut r_ch, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            pen_x += sz_ch.cx;
        }

        // 若游標在句尾
        if cur == comp_chars.len() {
            let caret_r = RECT {
                left: pen_x,
                top: comp_rect.top + (6.0 * state.dpi_scale) as i32,
                right: pen_x + (2.0 * state.dpi_scale).max(2.0) as i32,
                bottom: comp_rect.bottom - (6.0 * state.dpi_scale) as i32,
            };
            FillRect(mem_dc, &caret_r, cursor_brush);
        }

        let _ = DeleteObject(sel_bg_brush);
        let _ = DeleteObject(cursor_brush);
        SelectObject(mem_dc, old_f);
        let _ = DeleteObject(comp_font);

        cur_y = comp_rect.bottom + (4.0 * state.dpi_scale) as i32;

        // 2. 候選欄 (Candidate Panel) — 底色 bg_cand，白字大字體 (ink_primary)，支援 PgUp/PgDn 多頁翻頁
        if state.is_cand_selecting && !state.candidates.is_empty() {
            let cand_card_h = (38.0 * state.dpi_scale) as i32;
            let cand_rect = RECT {
                left: pad_x,
                top: cur_y,
                right: width - pad_x,
                bottom: cur_y + cand_card_h,
            };

            let cand_brush = CreateSolidBrush(bg_cand);
            FillRect(mem_dc, &cand_rect, cand_brush);
            let _ = DeleteObject(cand_brush);
            FrameRect(mem_dc, &cand_rect, border_brush);

            let cand_font_h = -(18.0 * state.dpi_scale).round() as i32;
            let cand_font = CreateFontW(
                cand_font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_f2 = SelectObject(mem_dc, cand_font);

            let asdf_keys = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
            let mut cand_x = cand_rect.left + (12.0 * state.dpi_scale) as i32;

            let cur_page = state.selected_index / 9;
            let start_idx = cur_page * 9;
            let page_cands: Vec<(usize, &String)> = state.candidates.iter()
                .skip(start_idx)
                .take(9)
                .enumerate()
                .collect();

            for (page_offset, cand) in page_cands {
                let actual_idx = start_idx + page_offset;
                let key_label = if state.quick_key_mode == "asdf" {
                    if page_offset < asdf_keys.len() { asdf_keys[page_offset].to_string() } else { " ".to_string() }
                } else {
                    (page_offset + 1).to_string()
                };

                let item_str = format!("{}.{}", key_label, cand);
                let item_wide: Vec<u16> = item_str.encode_utf16().collect();
                let mut sz_item = SIZE::default();
                let _ = GetTextExtentPoint32W(mem_dc, &item_wide, &mut sz_item);

                let is_sel = actual_idx == state.selected_index;
                let item_bg = RECT {
                    left: cand_x - 4,
                    top: cand_rect.top + (4.0 * state.dpi_scale) as i32,
                    right: cand_x + sz_item.cx + 6,
                    bottom: cand_rect.bottom - (4.0 * state.dpi_scale) as i32,
                };

                if is_sel {
                    let sel_b = CreateSolidBrush(accent_blue);
                    FillRect(mem_dc, &item_bg, sel_b);
                    let _ = DeleteObject(sel_b);
                    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                } else {
                    SetTextColor(mem_dc, ink_primary); // 純粹高雅的白字
                }

                let mut w = item_wide;
                w.push(0);
                let mut r = item_bg;
                r.left += 4;
                DrawTextW(mem_dc, &mut w, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                cand_x += sz_item.cx + (18.0 * state.dpi_scale) as i32;
            }

            SelectObject(mem_dc, old_f2);
            let _ = DeleteObject(cand_font);

            cur_y = cand_rect.bottom + (4.0 * state.dpi_scale) as i32;
        }

        // 3. 狀態列 (Status Bar) — 底色 bg_main，純文字極簡排版 (左：狀態，右：半 許)
        let status_h = (24.0 * state.dpi_scale) as i32;
        let status_rect = RECT {
            left: pad_x + (4.0 * state.dpi_scale) as i32,
            top: cur_y + (2.0 * state.dpi_scale) as i32,
            right: width - pad_x - (4.0 * state.dpi_scale) as i32,
            bottom: cur_y + status_h,
        };

        let status_font_h = -(15.0 * state.dpi_scale).round() as i32;
        let status_font = CreateFontW(
            status_font_h, 0, 0, 0,
            FW_NORMAL.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(font_name.as_ptr()),
        );
        let old_f3 = SelectObject(mem_dc, status_font);

        // 左側：淡墨色狀態提示（支援多頁翻頁狀態提示）
        SetTextColor(mem_dc, ink_muted);
        let total_cands = state.candidates.len();
        let total_pages = if total_cands > 0 { (total_cands + 8) / 9 } else { 1 };
        let cur_page = (state.selected_index / 9) + 1;

        let st_label = if state.selection_range.1 > state.selection_range.0 {
            "如喜輸入座  [已標記範圍 · Enter 加詞 / Shift+Enter 刪詞]".to_string()
        } else if state.is_cand_selecting && total_pages > 1 {
            format!("如喜輸入座  [{} / {} 頁 · PgUp/PgDn 翻頁]", cur_page, total_pages)
        } else {
            "如喜輸入座".to_string()
        };
        let mut st_text: Vec<u16> = st_label.encode_utf16().chain(Some(0)).collect();
        let mut st_rect = status_rect;
        DrawTextW(mem_dc, &mut st_text, &mut st_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

        // 齒輪設定按鈕 (Gear Button: ⚙)
        let gear_size = (20.0 * state.dpi_scale) as i32;
        let gear_rect = RECT {
            left: status_rect.right - gear_size,
            top: status_rect.top,
            right: status_rect.right,
            bottom: status_rect.bottom,
        };

        SetTextColor(mem_dc, ink_muted);
        let mut gear_text: Vec<u16> = "⚙".encode_utf16().chain(Some(0)).collect();
        let mut gear_r = gear_rect;
        DrawTextW(mem_dc, &mut gear_text, &mut gear_r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        // 右側：純文字大號「半   許 / 日 / 倉 / 注 / 倚 / 拼」或「半   英」(副墨色，大氣乾淨，支援滑鼠點擊切換輸入法)
        SetTextColor(mem_dc, ink_secondary);
        let kb_indicator = match state.keyboard_mode.to_lowercase().as_str() {
            "hsu" => "許",
            "japanese" | "jp" | "romaji" | "kana" => "日",
            "standard" | "daikien" | "dachen" | "bopomofo" => "注",
            "cangjie" => "倉",
            "et26" | "eten26" => "倚",
            "et41" => "41",
            "pinyin" => "拼",
            _ => "中",
        };
        let mode_label = if state.is_caps_on { "半   英".to_string() } else { format!("半   {}", kb_indicator) };
        let mode_wide_no_null: Vec<u16> = mode_label.encode_utf16().collect();
        let mut sz_mode = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &mode_wide_no_null, &mut sz_mode);
        let mode_right = gear_rect.left - (6.0 * state.dpi_scale) as i32;
        let mode_hit_left = (mode_right - sz_mode.cx - (12.0 * state.dpi_scale) as i32).max(status_rect.left);
        let mode_hit_rect = RECT {
            left: mode_hit_left,
            top: status_rect.top,
            right: mode_right,
            bottom: status_rect.bottom,
        };
        if let Ok(mut st) = get_candidate_state().write() {
            st.gear_rect = gear_rect;
            st.mode_rect = mode_hit_rect;
        }

        let mut mode_text: Vec<u16> = mode_label.encode_utf16().chain(Some(0)).collect();
        let mut mode_rect = RECT {
            left: status_rect.left,
            top: status_rect.top,
            right: mode_right,
            bottom: status_rect.bottom,
        };
        DrawTextW(mem_dc, &mut mode_text, &mut mode_rect, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);

        SelectObject(mem_dc, old_f3);
        let _ = DeleteObject(status_font);
    }

    let _ = DeleteObject(border_brush);

    // 複製到視窗 DC
    let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

    // 清理資源
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(mem_bmp);
    let _ = DeleteDC(mem_dc);
}

pub fn save_keyboard_mode(mode: &str) {
    if let Ok(mut state) = get_candidate_state().write() {
        state.keyboard_mode = mode.to_string();
    }
    let paths = crate::service::RuhiPaths::resolve();
    if paths.config.exists() {
        if let Ok(content) = std::fs::read_to_string(&paths.config) {
            if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                val["keyboard_mode"] = serde_json::Value::from(mode);
                if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                    let _ = std::fs::write(&paths.config, new_content);
                }
            }
        }
    }
}

pub unsafe fn show_mode_switch_menu(hwnd: HWND) {
    let mut pt = POINT::default();
    if GetCursorPos(&mut pt).is_err() {
        return;
    }
    let (cur_kb, is_caps, enabled_modes) = if let Ok(s) = get_candidate_state().read() {
        (
            s.keyboard_mode.to_lowercase(),
            s.is_caps_on,
            s.enabled_keyboard_modes.clone(),
        )
    } else {
        (
            "hsu".to_string(),
            false,
            vec!["hsu".to_string(), "japanese".to_string()],
        )
    };

    if let Ok(hmenu) = CreatePopupMenu() {
        if !hmenu.0.is_null() {
            let items: &[(usize, &str, &str)] = &[
                (2001, "hsu", "許氏雙拼注音 (許)\tCtrl/Shift + CapsLock"),
                (2005, "japanese", "日文羅馬字輸入 (日)\tCtrl/Shift + CapsLock"),
                (2004, "cangjie", "倉頡輸入法 (倉)\tCtrl/Shift + CapsLock"),
                (2002, "standard", "標準大千注音 (注)\tCtrl/Shift + CapsLock"),
                (2003, "et26", "倚天 26 鍵注音 (倚)\tCtrl/Shift + CapsLock"),
            ];
            for &(id, mode_code, label) in items {
                let is_active = !is_caps && (
                    cur_kb == mode_code
                    || (mode_code == "standard" && cur_kb == "daikien")
                    || (mode_code == "et26" && cur_kb == "eten26")
                    || (mode_code == "japanese" && (cur_kb == "jp" || cur_kb == "romaji" || cur_kb == "kana"))
                );
                let is_enabled_in_loop = enabled_modes.is_empty()
                    || enabled_modes.iter().any(|m| {
                        let m_low = m.to_lowercase();
                        m_low == mode_code
                            || (mode_code == "standard" && m_low == "daikien")
                            || (mode_code == "et26" && m_low == "eten26")
                            || (mode_code == "japanese" && (m_low == "jp" || m_low == "romaji" || m_low == "kana"))
                    });
                if !is_enabled_in_loop && !is_active {
                    continue;
                }
                let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let flags = if is_active { MF_STRING | MF_CHECKED } else { MF_STRING };
                let _ = AppendMenuW(hmenu, flags, id, PCWSTR(wide.as_ptr()));
            }
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());

            let caps_label = "英數／歐文直通模式 (英)\tCapsLock";
            let caps_wide: Vec<u16> = caps_label.encode_utf16().chain(Some(0)).collect();
            let caps_flags = if is_caps { MF_STRING | MF_CHECKED } else { MF_STRING };
            let _ = AppendMenuW(hmenu, caps_flags, 2006, PCWSTR(caps_wide.as_ptr()));

            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let (lbl_settings, _) = get_context_menu_labels();
            let text_settings: Vec<u16> = lbl_settings.encode_utf16().chain(Some(0)).collect();
            let _ = AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(text_settings.as_ptr()));

            let cmd = TrackPopupMenu(
                hmenu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_LEFTBUTTON,
                pt.x,
                pt.y,
                0,
                hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);

            let chosen_mode = match cmd.0 {
                2001 => Some("hsu"),
                2002 => Some("standard"),
                2003 => Some("et26"),
                2004 => Some("cangjie"),
                2005 => Some("japanese"),
                _ => None,
            };
            if let Some(m) = chosen_mode {
                save_keyboard_mode(m);
                if is_caps {
                    keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYBD_EVENT_FLAGS(0), 0);
                    keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYEVENTF_KEYUP, 0);
                    if let Ok(mut s) = get_candidate_state().write() {
                        s.is_caps_on = false;
                    }
                }
                let _ = InvalidateRect(hwnd, None, true);
            } else if cmd.0 == 2006 {
                keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_CAPITAL.0 as u8, 0x3A, KEYEVENTF_KEYUP, 0);
                if let Ok(mut s) = get_candidate_state().write() {
                    s.is_caps_on = !s.is_caps_on;
                }
                let _ = InvalidateRect(hwnd, None, true);
            } else if cmd.0 == 1001 {
                launch_settings();
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = catch_unwind(|| {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if !hdc.0.is_null() {
                    paint_candidate_window(hwnd, hdc);
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => {
                LRESULT(1)
            }
            WM_MOUSEACTIVATE => {
                // 鐵律：防止點擊輸入座奪取編輯視窗焦點，避免 TSF OnSetFocus(None) 誤判視窗失焦而關閉輸入座！
                LRESULT(MA_NOACTIVATE as isize)
            }
            WM_SETCURSOR => {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_ok() && ScreenToClient(hwnd, &mut pt).as_bool() {
                    let is_clickable = if let Ok(s) = get_candidate_state().read() {
                        let on_gear = pt.x >= s.gear_rect.left && pt.x <= s.gear_rect.right &&
                                      pt.y >= s.gear_rect.top && pt.y <= s.gear_rect.bottom;
                        let on_mode = pt.x >= s.mode_rect.left && pt.x <= s.mode_rect.right &&
                                      pt.y >= s.mode_rect.top && pt.y <= s.mode_rect.bottom;
                        on_gear || on_mode
                    } else { false };
                    if is_clickable {
                        let _ = SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default());
                        return LRESULT(1);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONDOWN => {
                let mouse_x = (lparam.0 & 0xFFFF) as i16 as i32;
                let mouse_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let (is_gear, is_mode) = if let Ok(s) = get_candidate_state().read() {
                    let g = mouse_x >= s.gear_rect.left && mouse_x <= s.gear_rect.right &&
                            mouse_y >= s.gear_rect.top && mouse_y <= s.gear_rect.bottom;
                    let m = mouse_x >= s.mode_rect.left && mouse_x <= s.mode_rect.right &&
                            mouse_y >= s.mode_rect.top && mouse_y <= s.mode_rect.bottom;
                    (g, m)
                } else { (false, false) };
                if is_gear {
                    launch_settings();
                    return LRESULT(0);
                }
                if is_mode {
                    show_mode_switch_menu(hwnd);
                    return LRESULT(0);
                }

                let mode = get_candidate_state().read().map(|s| s.input_display_mode.clone()).unwrap_or_default();
                if mode == "floating_fixed" || mode == "floating_follow" {
                    // 鐵律：嚴禁調用 SendMessageW(WM_NCLBUTTONDOWN, HTCAPTION)！
                    // 原因：WM_NCLBUTTONDOWN 會強制奪取視窗焦點 (NCACTIVATE) 並進入 Win32 模態拖曳迴圈，
                    // 在輸入座有組字內容時，會導致宿主進程 (如 Chromium/Antigravity/Electron) 的 IME 狀態機陷入死鎖凍結！
                    // 正解：改用無焦點非阻塞式純手動座標追蹤 (SWP_NOACTIVATE)，保證宿主視窗永不失焦，零死鎖。
                    let mut cursor_pt = POINT::default();
                    let mut win_rect = RECT::default();
                    if GetCursorPos(&mut cursor_pt).is_ok() && GetWindowRect(hwnd, &mut win_rect).is_ok() {
                        if let Ok(mut start_cur) = DRAG_START_CURSOR.lock() {
                            *start_cur = (cursor_pt.x, cursor_pt.y);
                        }
                        if let Ok(mut start_win) = DRAG_START_WINDOW.lock() {
                            *start_win = (win_rect.left, win_rect.top);
                        }
                        IS_DRAGGING.store(true, Ordering::SeqCst);
                        let _ = SetCapture(hwnd);
                    }
                    return LRESULT(0);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if IS_DRAGGING.load(Ordering::SeqCst) {
                    let mut cursor_pt = POINT::default();
                    if GetCursorPos(&mut cursor_pt).is_ok() {
                        let (start_cx, start_cy) = DRAG_START_CURSOR.lock().map(|g| *g).unwrap_or((0, 0));
                        let (start_wx, start_wy) = DRAG_START_WINDOW.lock().map(|g| *g).unwrap_or((0, 0));
                        let dx = cursor_pt.x - start_cx;
                        let dy = cursor_pt.y - start_cy;
                        let new_x = start_wx + dx;
                        let new_y = start_wy + dy;

                        // 鐵律：使用 SWP_NOACTIVATE 保證絕不搶奪焦點，杜絕目標視窗與 TSF 凍結死鎖
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            new_x,
                            new_y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );

                        if let Ok(mut state) = get_candidate_state().write() {
                            state.fixed_x = new_x;
                            state.fixed_y = new_y;
                        }
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONUP => {
                if IS_DRAGGING.swap(false, Ordering::SeqCst) {
                    let _ = ReleaseCapture();
                    let (cur_x, cur_y) = get_candidate_state().read()
                        .map(|s| (s.fixed_x, s.fixed_y))
                        .unwrap_or((0, 0));
                    save_desk_position(cur_x, cur_y);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CAPTURECHANGED => {
                if IS_DRAGGING.swap(false, Ordering::SeqCst) {
                    let (cur_x, cur_y) = get_candidate_state().read()
                        .map(|s| (s.fixed_x, s.fixed_y))
                        .unwrap_or((0, 0));
                    save_desk_position(cur_x, cur_y);
                }
                LRESULT(0)
            }

            WM_USER..=0x7FFF => {
                if msg == WM_TRAYICON {
                    let event = (lparam.0 & 0xFFFF) as u32;
                    match event {
                        WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                            launch_settings();
                        }
                        WM_RBUTTONUP => {
                            let mut pt = POINT::default();
                            if GetCursorPos(&mut pt).is_ok() {
                                if let Ok(hmenu) = CreatePopupMenu() {
                                    if !hmenu.0.is_null() {
                                        let (lbl_settings, lbl_about) = get_context_menu_labels();
                                        let text_settings: Vec<u16> = lbl_settings.encode_utf16().chain(Some(0)).collect();
                                        let text_about: Vec<u16> = lbl_about.encode_utf16().chain(Some(0)).collect();
                                        let _ = AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(text_settings.as_ptr()));
                                        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                                        let _ = AppendMenuW(hmenu, MF_STRING, 1002, PCWSTR(text_about.as_ptr()));

                                        let _ = SetForegroundWindow(hwnd);
                                        let cmd = TrackPopupMenu(
                                            hmenu,
                                            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                                            pt.x,
                                            pt.y,
                                            0,
                                            hwnd,
                                            None,
                                        );
                                        let _ = DestroyMenu(hmenu);
                                        if cmd.0 == 1001 {
                                            launch_settings();
                                        } else if cmd.0 == 1002 {
                                            show_about_dialog(hwnd);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_RBUTTONUP => {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_ok() {
                    if let Ok(hmenu) = CreatePopupMenu() {
                        if !hmenu.0.is_null() {
                            let (lbl_settings, lbl_about) = get_context_menu_labels();
                            let text_settings: Vec<u16> = lbl_settings.encode_utf16().chain(Some(0)).collect();
                            let text_about: Vec<u16> = lbl_about.encode_utf16().chain(Some(0)).collect();
                            let _ = AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(text_settings.as_ptr()));
                            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                            let _ = AppendMenuW(hmenu, MF_STRING, 1002, PCWSTR(text_about.as_ptr()));

                            let _ = SetForegroundWindow(hwnd);
                            let cmd = TrackPopupMenu(
                                hmenu,
                                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                                pt.x,
                                pt.y,
                                0,
                                hwnd,
                                None,
                            );
                            let _ = DestroyMenu(hmenu);
                            if cmd.0 == 1001 {
                                launch_settings();
                            } else if cmd.0 == 1002 {
                                show_about_dialog(hwnd);
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                let mode = get_candidate_state().read().map(|s| s.input_display_mode.clone()).unwrap_or_default();
                if mode == "floating_fixed" {
                    let mut rect = RECT::default();
                    if GetWindowRect(hwnd, &mut rect).is_ok() {
                        if let Ok(mut state) = get_candidate_state().write() {
                            state.fixed_x = rect.left;
                            state.fixed_y = rect.top;
                        }
                        save_desk_position(rect.left, rect.top);
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DPICHANGED => {
                let dpi = (wparam.0 & 0xFFFF) as f32;
                if let Ok(mut state) = get_candidate_state().write() {
                    state.dpi_scale = dpi / 96.0;
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    });

    result.unwrap_or_else(|_| DefWindowProcW(hwnd, msg, wparam, lparam))
}
