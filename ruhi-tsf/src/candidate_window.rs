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
const WM_SYNC_TRAY_MODE: u32 = WM_USER + 102;
const TRAY_ICON_ID: u32 = 1001;
const TIMER_TRAY_SYNC: usize = 2001;
const TIMER_HIDE_TOAST: usize = 2002;

static LAST_TRAY_STATE: Mutex<(String, bool)> = Mutex::new((String::new(), false));
static CURRENT_TRAY_HICON: Mutex<isize> = Mutex::new(0);

pub fn current_effective_ui_lang() -> &'static str {
    let cfg_lang = get_candidate_state()
        .read()
        .map(|s| s.ui_lang.clone())
        .unwrap_or_else(|_| "auto".to_string());
    crate::service::resolve_effective_ui_lang(&cfg_lang)
}

pub fn get_mode_badge_and_name_i18n(kb_mode: &str, is_caps: bool, ui_lang: &str) -> (&'static str, &'static str) {
    match ui_lang {
        "en-US" => {
            if is_caps {
                return ("EU", "Pan-European / Roman Mode");
            }
            match kb_mode.to_lowercase().as_str() {
                "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => ("EU", "Pan-European / Roman Mode"),
                "hsu" => ("HSU", "Hsu Bopomofo"),
                "japanese" | "jp" | "romaji" | "kana" => ("JA", "Japanese Romaji"),
                "korean" | "ko" | "hangul" | "romaja" => ("KO", "Korean Romaja"),
                "standard" | "daikien" | "dachen" | "bopomofo" => ("BPM", "Standard Bopomofo"),
                "cangjie" => ("CJ", "Cangjie"),
                "array30" | "ar30" => ("AR", "Array 30"),
                "et26" | "eten26" => ("ET", "Eten 26 Bopomofo"),
                "et41" => ("41", "Eten 41 Bopomofo"),
                "pinyin" => ("PY", "Hanyu Pinyin"),
                _ => ("ZH", "Chinese Input"),
            }
        }
        "ja-JP" => {
            if is_caps {
                return ("欧", "汎欧州／ローマ字モード");
            }
            match kb_mode.to_lowercase().as_str() {
                "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => ("欧", "汎欧州／ローマ字モード"),
                "hsu" => ("許", "許氏双拼注音"),
                "japanese" | "jp" | "romaji" | "kana" => ("日", "日本語ローマ字"),
                "korean" | "ko" | "hangul" | "romaja" => ("韓", "韓国語ローマ字"),
                "standard" | "daikien" | "dachen" | "bopomofo" => ("注", "標準注音"),
                "cangjie" => ("倉", "倉頡入力"),
                "array30" | "ar30" => ("行", "行列 30"),
                "et26" | "eten26" => ("倚", "倚天 26 鍵"),
                "et41" => ("41", "倚天 41 鍵"),
                "pinyin" => ("拼", "ピンイン"),
                _ => ("中", "中国語入力"),
            }
        }
        "ko-KR" => {
            if is_caps {
                return ("EU", "범유럽 / 로마자 모드");
            }
            match kb_mode.to_lowercase().as_str() {
                "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => ("EU", "범유럽 / 로마자 모드"),
                "hsu" => ("許", "허씨 쌍핀 주음"),
                "japanese" | "jp" | "romaji" | "kana" => ("日", "일본어 로마자"),
                "korean" | "ko" | "hangul" | "romaja" => ("한", "한국어 로마자"),
                "standard" | "daikien" | "dachen" | "bopomofo" => ("注", "표준 주음"),
                "cangjie" => ("倉", "창힐 입력기"),
                "array30" | "ar30" => ("行", "행렬 30"),
                "et26" | "eten26" => ("倚", "이천 26키"),
                "et41" => ("41", "이천 41키"),
                "pinyin" => ("拼", "한어병음"),
                _ => ("中", "중국어 입력"),
            }
        }
        _ => {
            if is_caps {
                return ("歐", "泛歐／羅馬字模式");
            }
            match kb_mode.to_lowercase().as_str() {
                "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => ("歐", "泛歐／羅馬字模式"),
                "hsu" => ("許", "許氏雙拼注音"),
                "japanese" | "jp" | "romaji" | "kana" => ("日", "日文羅馬字"),
                "korean" | "ko" | "hangul" | "romaja" => ("韓", "韓文羅馬拼音"),
                "standard" | "daikien" | "dachen" | "bopomofo" => ("注", "標準大千注音"),
                "cangjie" => ("倉", "倉頡輸入法"),
                "array30" | "ar30" => ("行", "行列 30 輸入法"),
                "et26" | "eten26" => ("倚", "倚天 26 鍵注音"),
                "et41" => ("41", "倚天 41 鍵注音"),
                "pinyin" => ("拼", "漢語拼音"),
                _ => ("中", "中文輸入"),
            }
        }
    }
}

pub fn get_mode_menu_items_i18n(ui_lang: &str) -> &'static [(u32, &'static str, &'static str)] {
    match ui_lang {
        "en-US" => &[
            (2001, "hsu", "Hsu Bopomofo (HSU)\tCtrl/Shift + CapsLock"),
            (2005, "japanese", "Japanese Romaji (JA)\tCtrl/Shift + CapsLock"),
            (2009, "korean", "Korean Romaja (KO)\tCtrl/Shift + CapsLock"),
            (2004, "cangjie", "Cangjie (CJ)\tCtrl/Shift + CapsLock"),
            (2007, "array30", "Array 30 (AR)\tCtrl/Shift + CapsLock"),
            (2002, "standard", "Standard Bopomofo (BPM)\tCtrl/Shift + CapsLock"),
            (2003, "et26", "Eten 26 Bopomofo (ET)\tCtrl/Shift + CapsLock"),
        ],
        "ja-JP" => &[
            (2001, "hsu", "許氏双拼注音 (許)\tCtrl/Shift + CapsLock"),
            (2005, "japanese", "日本語ローマ字入力 (日)\tCtrl/Shift + CapsLock"),
            (2009, "korean", "韓国語ローマ字入力 (韓)\tCtrl/Shift + CapsLock"),
            (2004, "cangjie", "倉頡入力 (倉)\tCtrl/Shift + CapsLock"),
            (2007, "array30", "行列 30 入力 (行)\tCtrl/Shift + CapsLock"),
            (2002, "standard", "標準大千注音 (注)\tCtrl/Shift + CapsLock"),
            (2003, "et26", "倚天 26 鍵注音 (倚)\tCtrl/Shift + CapsLock"),
        ],
        "ko-KR" => &[
            (2001, "hsu", "허씨 쌍핀 주음 (許)\tCtrl/Shift + CapsLock"),
            (2005, "japanese", "일본어 로마자 입력 (日)\tCtrl/Shift + CapsLock"),
            (2009, "korean", "한국어 로마자 입력 (한)\tCtrl/Shift + CapsLock"),
            (2004, "cangjie", "창힐 입력기 (倉)\tCtrl/Shift + CapsLock"),
            (2007, "array30", "행렬 30 입력기 (行)\tCtrl/Shift + CapsLock"),
            (2002, "standard", "표준 주음 (注)\tCtrl/Shift + CapsLock"),
            (2003, "et26", "이천 26키 주음 (倚)\tCtrl/Shift + CapsLock"),
        ],
        _ => &[
            (2001, "hsu", "許氏雙拼注音 (許)\tCtrl/Shift + CapsLock"),
            (2005, "japanese", "日文羅馬字輸入 (日)\tCtrl/Shift + CapsLock"),
            (2009, "korean", "韓文羅馬拼音輸入 (韓)\tCtrl/Shift + CapsLock"),
            (2004, "cangjie", "倉頡輸入法 (倉)\tCtrl/Shift + CapsLock"),
            (2007, "array30", "行列 30 輸入法 (行)\tCtrl/Shift + CapsLock"),
            (2002, "standard", "標準大千注音 (注)\tCtrl/Shift + CapsLock"),
            (2003, "et26", "倚天 26 鍵注音 (倚)\tCtrl/Shift + CapsLock"),
        ],
    }
}

pub fn get_mode_badge_and_name(kb_mode: &str, is_caps: bool) -> (&'static str, &'static str) {
    get_mode_badge_and_name_i18n(kb_mode, is_caps, current_effective_ui_lang())
}

#[allow(dead_code)]
fn _legacy_get_mode_badge_and_name(kb_mode: &str, is_caps: bool) -> (&'static str, &'static str) {
    if is_caps {
        return ("歐", "泛歐／羅馬字模式");
    }
    match kb_mode.to_lowercase().as_str() {
        "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => ("歐", "泛歐／羅馬字模式"),
        "hsu" => ("許", "許氏雙拼注音"),
        "japanese" | "jp" | "romaji" | "kana" => ("日", "日文羅馬字"),
        "korean" | "ko" | "hangul" | "romaja" => ("韓", "韓文羅馬拼音"),
        "standard" | "daikien" | "dachen" | "bopomofo" => ("注", "標準大千注音"),
        "cangjie" => ("倉", "倉頡輸入法"),
        "array30" | "ar30" => ("行", "行列 30 輸入法"),
        "et26" | "eten26" => ("倚", "倚天 26 鍵注音"),
        "et41" => ("41", "倚天 41 鍵注音"),
        "pinyin" => ("拼", "漢語拼音"),
        _ => ("中", "中文輸入"),
    }
}

/// 動態繪製 32x32 高解析度輸入狀態圖示（如「許」、「日」、「歐」、「行」、「倉」），供右下角工作列與系統匣即時顯示
pub unsafe fn create_mode_hicon(badge: &str, is_caps: bool) -> HICON {
    let size: i32 = 32;
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let _ = ReleaseDC(None, screen_dc);
    if mem_dc.0.is_null() {
        return LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // Top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;

    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib_res = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0);
    let dib_bmp = match dib_res {
        Ok(b) if !b.0.is_null() && !bits_ptr.is_null() => b,
        _ => {
            let _ = DeleteDC(mem_dc);
            return LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        }
    };

    let old_bmp = SelectObject(mem_dc, dib_bmp);

    // 依輸入模式選擇鮮明易辨識的底色（COLORREF 格式為 0x00BBGGRR）
    let bg_color = if is_caps || badge == "歐" || badge == "英" {
        COLORREF(0x00EB6325) // 皇家藍 #2563EB（泛歐／羅馬字模式）
    } else {
        match badge {
            "日" => COLORREF(0x005E3FDB), // 櫻緋紅 #DB3F5E（日文模式，一眼區分）
            "行" | "倉" => COLORREF(0x0088940D), // 青松綠 #0D9488（字形模式）
            _ => COLORREF(0x00699605),    // 翠玉綠 #059669（許氏／注音模式）
        }
    };

    let full_rect = RECT { left: 0, top: 0, right: size, bottom: size };
    let bg_brush = CreateSolidBrush(bg_color);
    FillRect(mem_dc, &full_rect, bg_brush);
    let _ = DeleteObject(bg_brush);

    SetBkMode(mem_dc, TRANSPARENT);
    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));

    let font_name: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();
    let font_h = if badge.chars().count() > 1 { -18 } else { -24 };
    let font = CreateFontW(
        font_h, 0, 0, 0,
        FW_BOLD.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        PCWSTR(font_name.as_ptr()),
    );
    let old_font = SelectObject(mem_dc, font);

    let mut text_wide: Vec<u16> = badge.encode_utf16().chain(Some(0)).collect();
    let mut text_rect = RECT { left: 0, top: -1, right: size, bottom: size };
    DrawTextW(mem_dc, &mut text_wide, &mut text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

    SelectObject(mem_dc, old_font);
    let _ = DeleteObject(font);
    let _ = GdiFlush();

    // 修正 32-bit BGRA Alpha 通道並裁切圓角 (r = 5px)
    let pixels = std::slice::from_raw_parts_mut(bits_ptr as *mut u8, (size * size * 4) as usize);
    let r: i32 = 5;
    for y in 0..size {
        for x in 0..size {
            let dx = if x < r { r - x } else if x >= size - r { x - (size - r - 1) } else { 0 };
            let dy = if y < r { r - y } else if y >= size - r { y - (size - r - 1) } else { 0 };
            let idx = ((y * size + x) * 4) as usize;
            if dx * dx + dy * dy > r * r + 1 {
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
                pixels[idx + 3] = 0;
            } else {
                pixels[idx + 3] = 0xFF;
            }
        }
    }

    SelectObject(mem_dc, old_bmp);
    let _ = DeleteDC(mem_dc);

    let mask_bmp = CreateBitmap(size, size, 1, 1, None);
    let icon_info = ICONINFO {
        fIcon: BOOL::from(true),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bmp,
        hbmColor: dib_bmp,
    };
    let hicon = CreateIconIndirect(&icon_info).unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());
    let _ = DeleteObject(dib_bmp);
    if !mask_bmp.0.is_null() {
        let _ = DeleteObject(mask_bmp);
    }
    hicon
}

pub unsafe fn purge_legacy_overflow_tray_icons() {
    let class_name = w!("RuhICandidateWindow");
    let mut child = HWND::default();
    for _ in 0..64 {
        if let Ok(found) = FindWindowExW(None, child, class_name, None) {
            if found.0.is_null() {
                break;
            }
            for uid in [TRAY_ICON_ID, 1, 0] {
                let mut nid = NOTIFYICONDATAW::default();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = found;
                nid.uID = uid;
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            child = found;
        } else {
            break;
        }
    }
}

pub unsafe fn update_tray_icon_for_hwnd(hwnd: HWND, force: bool) {
    if !hwnd.0.is_null() && IsWindowVisible(hwnd).as_bool() {
        let fg_hwnd = GetForegroundWindow();
        if !fg_hwnd.0.is_null() {
            let mut fg_pid = 0u32;
            GetWindowThreadProcessId(fg_hwnd, Some(&mut fg_pid));
            let cur_pid = windows::Win32::System::Threading::GetCurrentProcessId();
            let is_cands_only = get_candidate_state().read().map(|s| s.is_candidates_only).unwrap_or(true);
            if fg_pid != 0 && fg_pid != cur_pid && !is_cands_only {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    let paths = crate::service::RuhiPaths::resolve();
    let cfg_mode = if paths.config.exists() {
        std::fs::read_to_string(&paths.config)
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| v.get("keyboard_mode").and_then(|m| m.as_str()).map(|s| s.to_string()))
    } else {
        None
    }.unwrap_or_else(|| {
        get_candidate_state().read().map(|s| s.keyboard_mode.clone()).unwrap_or_else(|_| "hsu".to_string())
    });

    if let Ok(mut st) = get_candidate_state().write() {
        st.keyboard_mode = cfg_mode.clone();
    }
    if !hwnd.0.is_null() && IsWindowVisible(hwnd).as_bool() {
        let _ = InvalidateRect(hwnd, None, true);
    }

    let is_caps = false;
    if !force {
        if let Ok(last) = LAST_TRAY_STATE.lock() {
            if last.0 == cfg_mode && last.1 == is_caps {
                return;
            }
        }
    }
    if let Ok(mut last) = LAST_TRAY_STATE.lock() {
        *last = (cfg_mode.clone(), is_caps);
    }

    drop(CURRENT_TRAY_HICON.lock());
    crate::service::notify_langbar_mode_update();
}

pub fn broadcast_tray_update() {
    crate::service::notify_langbar_mode_update();
    unsafe {
        let class_name = w!("RuhICandidateWindow");
        let mut child = HWND::default();
        for _ in 0..32 {
            if let Ok(found) = FindWindowExW(None, child, class_name, None) {
                if found.0.is_null() {
                    break;
                }
                let _ = PostMessageW(found, WM_SYNC_TRAY_MODE, WPARAM(0), LPARAM(0));
                child = found;
            } else {
                break;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTuneAction {
    UiScaleDown,
    UiScaleUp,
    OpacityDown,
    OpacityUp,
    DeskFontDown,
    DeskFontUp,
    CandFontDown,
    CandFontUp,
    MaxCharsDown,
    MaxCharsUp,
}

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
    pub candidate_font_size: f32,
    pub desk_opacity: u32,
    pub fixed_x: i32,
    pub fixed_y: i32,
    pub last_caret_x: i32,
    pub last_caret_y: i32,
    pub keyboard_mode: String,
    pub enabled_keyboard_modes: Vec<String>,
    pub gear_rect: RECT,
    pub mode_rect: RECT,
    pub ui_tune_toggle_rect: RECT,
    pub show_ui_tuner_bar: bool,
    pub ui_tune_buttons: Vec<(RECT, UiTuneAction)>,
    pub max_composition_chars: usize,
    pub enable_array30_coach: bool,
    pub array30_coach_hint: String,
    pub is_array30_live_preview: bool,
    pub ui_lang: String,
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
            candidate_font_size: 24.0,
            desk_opacity: 95,
            fixed_x: 350,
            fixed_y: 550,
            last_caret_x: 350,
            last_caret_y: 550,
            keyboard_mode: "hsu".to_string(),
            enabled_keyboard_modes: vec!["hsu".to_string(), "japanese".to_string()],
            gear_rect: RECT::default(),
            mode_rect: RECT::default(),
            ui_tune_toggle_rect: RECT::default(),
            show_ui_tuner_bar: false,
            ui_tune_buttons: Vec::new(),
            max_composition_chars: 30,
            enable_array30_coach: false,
            array30_coach_hint: String::new(),
            is_array30_live_preview: false,
            ui_lang: "auto".to_string(),
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
            let _ = KillTimer(self.hwnd, TIMER_TRAY_SYNC);
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
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
                class_name,
                w!("RuhICandidateWindow"),
                WS_POPUP,
                0, 0, 100, 100,
                None,
                None,
                instance,
                None,
            )?;

            let _ = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as usize as isize);

            let dpi = GetDpiForWindow(hwnd);
            let dpi_scale = if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 };

            let mut init_opacity = 95u32;
            if let Ok(mut state) = get_candidate_state().write() {
                state.is_dark_theme = is_dark_theme;
                state.dpi_scale = dpi_scale;
                init_opacity = state.desk_opacity;
            }
            apply_window_opacity(hwnd, init_opacity);

            // 徹底移除舊版隱藏於「^」溢位匣之 Shell_NotifyIconW 圖示，將功能 100% 整合至右下角工作列 TSF 圖示本身
            purge_legacy_overflow_tray_icons();
            let _ = SetTimer(hwnd, TIMER_TRAY_SYNC, 500, None);
            crate::service::notify_langbar_mode_update();

            logger::log(&format!("CandidateWindow/InputDesk created: hwnd={:?}, dpi={}", hwnd, dpi_scale));
            Ok(Self { hwnd, tray_mutex: None, is_tray_registered: false })
        }
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
        let mut opacity = 95u32;
        if let Ok(mut state) = get_candidate_state().write() {
            state.quick_key_mode = cfg.candidate_quick_key.clone();
            state.input_display_mode = cfg.input_display_mode.clone();
            state.keyboard_mode = cfg.keyboard_mode.clone();
            state.enabled_keyboard_modes = cfg.enabled_keyboard_modes.clone();
            state.font_size = (cfg.font_size as f32).clamp(16.0, 96.0);
            state.candidate_font_size = (cfg.candidate_font_size as f32).clamp(14.0, 72.0);
            state.desk_opacity = cfg.desk_opacity.clamp(20, 100);
            opacity = state.desk_opacity;
            state.orientation = cfg.orientation.clone();
            state.fixed_x = cfg.desk_x;
            state.fixed_y = cfg.desk_y;
            state.max_composition_chars = cfg.max_composition_chars.clamp(8, 80);
            state.enable_array30_coach = cfg.enable_array30_coach;
            state.ui_lang = cfg.ui_lang.clone();
            if cfg.theme == "dark" {
                state.is_dark_theme = true;
            } else if cfg.theme == "light" {
                state.is_dark_theme = false;
            } else {
                state.is_dark_theme = Self::detect_dark_theme();
            }
        }
        unsafe {
            apply_window_opacity(self.hwnd, opacity);
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

    pub fn set_coach_hint(&mut self, hint: &str, is_array30_live_preview: bool) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.array30_coach_hint = hint.to_string();
            state.is_array30_live_preview = is_array30_live_preview;
        }
    }

    /// 計算如喜輸入座 (RuhI Floating Input Desk) 視窗幾何位置與多行自適應尺寸
    /// 以 GDI 實際字型引擎測量單一字串之物理像素寬度
    pub fn measure_gdi_text_px(text: &str, font_px: f32) -> i32 {
        if text.is_empty() {
            return 0;
        }
        unsafe {
            let mem_dc = CreateCompatibleDC(HDC::default());
            if mem_dc.0.is_null() {
                return (text.chars().count() as f32 * font_px * 0.85).round() as i32;
            }
            let font_name: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();
            let font_h = -(font_px.round() as i32);
            let font = CreateFontW(
                font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_f = SelectObject(mem_dc, font);
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(mem_dc, &wide, &mut sz);
            SelectObject(mem_dc, old_f);
            let _ = DeleteObject(font);
            let _ = DeleteDC(mem_dc);
            sz.cx
        }
    }

    /// 以單一漢字（漢字寬度單位）為基準，將組字文字流拆分為字素叢集並計算各叢集之「所在行號、行首布林值、像素寬度」，
    /// 以及「總行數、單行最大實際像素寬度、單行漢字上限像素寬度」。
    /// 解決歐文半形字元在固定字元數換算下造成輸入座右側兩倍空白膨脹與提早折行之問題。
    pub fn layout_composition_clusters(comp_chars: &[char], font_px: f32, max_cjk_chars: usize) -> (Vec<(usize, usize, usize, bool, i32)>, usize, i32, i32) {
        let is_combining = |c: char| -> bool { ('\u{0300}'..='\u{036F}').contains(&c) };
        unsafe {
            let mem_dc = CreateCompatibleDC(HDC::default());
            let font_name: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();
            let font_h = -(font_px.round() as i32);
            let font = CreateFontW(
                font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_f = if !mem_dc.0.is_null() {
                SelectObject(mem_dc, font)
            } else {
                HGDIOBJ::default()
            };

            let cjk_px = if !mem_dc.0.is_null() {
                let sample: Vec<u16> = "漢".encode_utf16().collect();
                let mut sz = SIZE::default();
                let _ = GetTextExtentPoint32W(mem_dc, &sample, &mut sz);
                sz.cx.max(font_px.round() as i32)
            } else {
                (font_px * 1.05).round() as i32
            };
            let max_line_px = (max_cjk_chars.clamp(8, 80) as i32 * cjk_px).max(cjk_px * 6);

            let mut clusters = Vec::new();
            let mut cur_line = 0usize;
            let mut cur_line_px = 0i32;
            let mut max_used_px = 0i32;

            let mut i = 0usize;
            while i < comp_chars.len() {
                let mut cluster_end = i + 1;
                while cluster_end < comp_chars.len() && is_combining(comp_chars[cluster_end]) {
                    cluster_end += 1;
                }
                let cluster_str: String = comp_chars[i..cluster_end].iter().collect();
                let cx = if !mem_dc.0.is_null() {
                    let wide: Vec<u16> = cluster_str.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    let _ = GetTextExtentPoint32W(mem_dc, &wide, &mut sz);
                    sz.cx
                } else {
                    (font_px * 0.6).round() as i32
                };

                let is_line_start = if cur_line_px > 0 && cur_line_px + cx > max_line_px {
                    cur_line += 1;
                    cur_line_px = 0;
                    true
                } else {
                    cur_line_px == 0
                };
                cur_line_px += cx;
                if cur_line_px > max_used_px {
                    max_used_px = cur_line_px;
                }
                clusters.push((i, cluster_end, cur_line, is_line_start, cx));
                i = cluster_end;
            }

            if !mem_dc.0.is_null() {
                SelectObject(mem_dc, old_f);
                let _ = DeleteObject(font);
                let _ = DeleteDC(mem_dc);
            }

            let num_lines = if clusters.is_empty() { 1 } else { cur_line + 1 };
            let min_6_cjk_px = cjk_px * 6;
            let effective_used_px = if num_lines > 1 {
                max_line_px
            } else {
                max_used_px.max(min_6_cjk_px)
            };
            (clusters, num_lines, effective_used_px, max_line_px)
        }
    }

    /// 測量當前分頁所有候選項目（如 `"a.候選詞"`）之各自像素寬度與單行總需求寬度
    pub fn measure_page_candidates_px(state: &CandidateState, cand_font_px: f32, page_sz: usize) -> (Vec<(usize, String, i32)>, i32) {
        let use_ar30_num_keys = state.is_array30_live_preview && !state.is_cand_selecting;
        let asdf_keys = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
        let cur_page = state.selected_index / page_sz.max(1);
        let start_idx = cur_page * page_sz;
        let gap_px = (16.0 * state.dpi_scale).round() as i32;
        let mut items = Vec::new();
        let mut single_row_total_px = (28.0 * state.dpi_scale).round() as i32;

        unsafe {
            let mem_dc = CreateCompatibleDC(HDC::default());
            let font_name: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();
            let font_h = -(cand_font_px.round() as i32);
            let font = CreateFontW(
                font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_f = if !mem_dc.0.is_null() {
                SelectObject(mem_dc, font)
            } else {
                HGDIOBJ::default()
            };

            for (page_offset, cand) in state.candidates.iter().skip(start_idx).take(page_sz).enumerate() {
                let actual_idx = start_idx + page_offset;
                let key_label = if use_ar30_num_keys {
                    if page_offset == 9 { "0".to_string() } else { (page_offset + 1).to_string() }
                } else if state.quick_key_mode == "asdf" {
                    if page_offset < asdf_keys.len() { asdf_keys[page_offset].to_string() } else { " ".to_string() }
                } else {
                    (page_offset + 1).to_string()
                };
                let item_str = format!("{}.{}", key_label, cand);
                let cx = if !mem_dc.0.is_null() {
                    let wide: Vec<u16> = item_str.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    let _ = GetTextExtentPoint32W(mem_dc, &wide, &mut sz);
                    sz.cx
                } else {
                    (item_str.chars().count() as f32 * cand_font_px * 0.85).round() as i32
                };
                single_row_total_px += cx + gap_px;
                items.push((actual_idx, item_str, cx));
            }

            if !mem_dc.0.is_null() {
                SelectObject(mem_dc, old_f);
                let _ = DeleteObject(font);
                let _ = DeleteDC(mem_dc);
            }
        }
        (items, single_row_total_px)
    }

    /// 當輸入座寬度確定後，計算候選欄需要幾行才可 100% 完整顯示所有候選詞（不截斷、不溢出邊界）
    pub fn count_candidate_rows(items: &[(usize, String, i32)], inner_cand_width: i32, dpi_scale: f32) -> usize {
        if items.is_empty() {
            return 1;
        }
        let left_start = (12.0 * dpi_scale).round() as i32;
        let right_limit = (inner_cand_width - (8.0 * dpi_scale).round() as i32).max(left_start + 60);
        let gap_px = (16.0 * dpi_scale).round() as i32;
        let mut rows = 1usize;
        let mut cur_x = left_start;
        for &(_, _, cx) in items {
            if cur_x > left_start && cur_x + cx + 6 > right_limit {
                rows += 1;
                cur_x = left_start;
            }
            cur_x += cx + gap_px;
        }
        rows
    }

    pub fn compute_desk_geometry(state: &CandidateState, caret_x: i32, caret_y: i32) -> (i32, i32, i32, i32) {
        let dpi_scale = state.dpi_scale;
        let show_cand_bar = (state.is_cand_selecting || state.is_array30_live_preview) && !state.candidates.is_empty();
        let page_sz = if state.is_array30_live_preview && !state.is_cand_selecting { 10 } else { 9 };
        let font_h = (state.font_size * 0.9 * dpi_scale).clamp(14.0 * dpi_scale, 90.0 * dpi_scale);
        let cand_font_h = (state.candidate_font_size * 0.9 * dpi_scale).clamp(13.0 * dpi_scale, 68.0 * dpi_scale);

        let ref_x = if state.input_display_mode == "floating_fixed" { state.fixed_x } else { caret_x };
        let ref_y = if state.input_display_mode == "floating_fixed" { state.fixed_y } else { caret_y };
        let (screen_l, screen_t, screen_r, screen_b) = Self::get_monitor_work_area(ref_x, ref_y);
        let screen_avail_w = ((screen_r - screen_l) - 24).max(460) as f32;

        let max_line_chars = state.max_composition_chars.clamp(8, 80);
        let comp_chars: Vec<char> = state.composition_text.chars().collect();
        let (_clusters, num_lines, max_used_px, max_line_px) =
            Self::layout_composition_clusters(&comp_chars, font_h, max_line_chars);

        let comp_w = (44.0 * dpi_scale) + (max_used_px as f32);
        let (cand_items, cand_single_row_px) = if show_cand_bar {
            Self::measure_page_candidates_px(state, cand_font_h, page_sz)
        } else {
            (Vec::new(), 0)
        };
        let cand_w = if show_cand_bar {
            (cand_single_row_px as f32 + 24.0 * dpi_scale).min(screen_avail_w)
        } else {
            0.0
        };
        let coach_w = if !state.array30_coach_hint.is_empty() {
            (Self::measure_gdi_text_px(&state.array30_coach_hint, 14.5 * dpi_scale) as f32) + (40.0 * dpi_scale)
        } else {
            0.0
        };
        let tuner_w = if state.show_ui_tuner_bar {
            630.0 * dpi_scale
        } else {
            0.0
        };

        let max_w_limit = ((max_line_px as f32) + 64.0 * dpi_scale)
            .max(cand_w)
            .max(coach_w)
            .max(tuner_w)
            .max(460.0 * dpi_scale)
            .min(screen_avail_w);
        let width = comp_w
            .max(cand_w)
            .max(coach_w)
            .max(tuner_w)
            .clamp(440.0 * dpi_scale, max_w_limit) as i32;

        let pad_x = (8.0 * dpi_scale) as i32;
        let inner_cand_w = (width - pad_x * 2).max(200);
        let cand_rows = if show_cand_bar {
            Self::count_candidate_rows(&cand_items, inner_cand_w, dpi_scale)
        } else {
            1
        };

        let line_step_h = (font_h + 10.0 * dpi_scale).round() as i32;
        let row1_h = (num_lines as i32 * line_step_h) + (12.0 * dpi_scale) as i32; // 多行組字區高度
        let single_row1_h = line_step_h + (12.0 * dpi_scale) as i32;
        let cand_row_step_h = (cand_font_h + 10.0 * dpi_scale).round() as i32;
        let row2_h = if show_cand_bar {
            (cand_rows as i32 * cand_row_step_h) + (8.0 * dpi_scale) as i32 // 候選欄高度（支援多行折行自適應）
        } else {
            0
        };
        let row_coach_h = if !state.array30_coach_hint.is_empty() {
            (28.0 * dpi_scale) as i32
        } else {
            0
        };
        let row_tuner_h = if state.show_ui_tuner_bar {
            (30.0 * dpi_scale) as i32 // 輸入座內嵌即時調校列高度
        } else {
            0
        };
        let row3_h = (26.0 * dpi_scale) as i32; // 底部狀態列高度
        let height = row1_h + row2_h + row_coach_h + row_tuner_h + row3_h + (16.0 * dpi_scale) as i32;
        let single_line_height = single_row1_h + row2_h + row_coach_h + row3_h + (16.0 * dpi_scale) as i32;
        let extra_grow_h = (height - single_line_height).max(0);

        let min_x = screen_l + 8;
        let max_x = (screen_r - width - 8).max(min_x);
        let min_y = screen_t + 8;
        let max_y = (screen_b - height - 8).max(min_y);

        let (final_x, final_y) = if state.input_display_mode == "floating_fixed" {
            let fx = state.fixed_x.clamp(min_x, max_x);
            let screen_mid_y = (screen_t + screen_b) / 2;
            let raw_fy = if state.fixed_y >= screen_mid_y {
                state.fixed_y - extra_grow_h
            } else {
                state.fixed_y
            };
            let fy = raw_fy.clamp(min_y, max_y);
            (fx, fy)
        } else {
            let fx = caret_x.clamp(min_x, max_x);
            let fy = if caret_y + height > screen_b - 20 {
                (caret_y - height - 32).clamp(min_y, max_y)
            } else {
                caret_y.clamp(min_y, max_y)
            };
            (fx, fy)
        };

        (final_x, final_y, width, height)
    }

    /// 顯示如喜輸入座 (RuhI Floating Input Desk) — 支援多行自動換行、獨立字級、透明度與內嵌即時調校列
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
        let (final_x, final_y, width, height, opacity) = {
            if let Ok(mut state) = get_candidate_state().write() {
                state.composition_text = comp_text.to_string();
                state.cursor_pos = cursor_pos;
                state.pending_syllable = String::new();
                state.candidates = candidates.to_vec();
                state.selected_index = selected;
                state.is_cand_selecting = is_cand_sel;
                state.target_char_idx = target_char_idx;
                state.target_range = target_range;
                state.selection_range = selection_range;
                state.is_caps_on = is_caps_on;
                state.is_candidates_only = false;
                if caret_x > 0 && caret_y > 0 {
                    state.last_caret_x = caret_x;
                    state.last_caret_y = caret_y;
                }
                let (fx, fy, w, h) = Self::compute_desk_geometry(&state, caret_x, caret_y);
                (fx, fy, w, h, state.desk_opacity)
            } else {
                (350, 550, 460, 96, 95)
            }
        };

        unsafe {
            apply_window_opacity(self.hwnd, opacity);
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

    /// 微軟新注音游標跟隨模式：僅在選字或開啟行列教練提示時彈出輕巧候選浮窗
    pub fn show_candidates_only(&mut self, candidates: &[String], selected: usize, x: i32, y: i32) {
        let (dpi_scale, cand_font_size, orientation, coach_hint, is_ar30_preview, is_cand_sel, opacity) = if let Ok(s) = get_candidate_state().read() {
            (
                s.dpi_scale,
                s.candidate_font_size,
                s.orientation.clone(),
                s.array30_coach_hint.clone(),
                s.is_array30_live_preview,
                s.is_cand_selecting,
                s.desk_opacity,
            )
        } else {
            (1.0, 24.0, "horizontal".to_string(), String::new(), false, false, 95)
        };

        if candidates.is_empty() && coach_hint.is_empty() {
            self.hide();
            return;
        }

        let coach_w = if !coach_hint.is_empty() {
            (coach_hint.chars().count() as f32 * 13.5 * dpi_scale) + (36.0 * dpi_scale)
        } else {
            0.0
        };
        let coach_h = if !coach_hint.is_empty() {
            (28.0 * dpi_scale) as i32
        } else {
            0
        };

        let page_sz = if is_ar30_preview && !is_cand_sel { 10 } else { 9 };
        let cur_page = selected / page_sz;
        let page_start = cur_page * page_sz;
        let page_end = (page_start + page_sz).min(candidates.len());
        let page_candidates = if page_start < candidates.len() {
            &candidates[page_start..page_end]
        } else {
            &[]
        };
        let display_count = page_candidates.len();

        let item_font_h = (cand_font_size * 0.9 * dpi_scale).clamp(13.0 * dpi_scale, 68.0 * dpi_scale);

        let (width, height) = if display_count == 0 {
            // 純行列 30 即時反查教練浮動小卡（微軟新注音底線模式下打字時即時浮現）
            let w = (coach_w.max(260.0 * dpi_scale).min(1040.0 * dpi_scale)) as i32;
            let h = (32.0 * dpi_scale) as i32;
            (w, h)
        } else if orientation == "vertical" {
            // 直向微軟新注音候選窗（以 GDI 實測最寬候選字詞寬度）
            let max_cand_px = page_candidates
                .iter()
                .map(|c| Self::measure_gdi_text_px(c, item_font_h))
                .max()
                .unwrap_or(60);
            let num_col_w = 32.0 * dpi_scale;
            let cand_col_w = (max_cand_px as f32 + 16.0 * dpi_scale).max(60.0 * dpi_scale);
            let pad_x = 12.0 * dpi_scale;
            let w = ((pad_x * 2.0 + num_col_w + cand_col_w).max(coach_w).max(160.0 * dpi_scale).min(1280.0 * dpi_scale)) as i32;

            let item_h = (item_font_h + 12.0 * dpi_scale) as i32;
            let pad_y = (8.0 * dpi_scale) as i32;
            let has_more_pages = candidates.len() > page_sz;
            let footer_h = if has_more_pages { (24.0 * dpi_scale) as i32 } else { 0 };
            let h = pad_y * 2 + (display_count as i32 * item_h) + footer_h + coach_h;
            return self.finish_show_candidates_only(candidates, selected, page_start, w, h, x, y, dpi_scale, opacity);
        } else {
            // 橫向微軟新注音候選窗（以 GDI 實測所有候選詞寬度，與繪製時之 cur_x 步進 100% 同步）
            let mut total_w = 20.0 * dpi_scale;
            for c in page_candidates {
                let text_px = Self::measure_gdi_text_px(c, item_font_h) as f32;
                let item_w = (24.0 * dpi_scale) + text_px + (12.0 * dpi_scale);
                total_w += item_w + (6.0 * dpi_scale);
            }
            if candidates.len() > page_sz {
                total_w += 68.0 * dpi_scale; // 頁碼指示空間
            }
            let (scr_l, _, scr_r, _) = Self::get_monitor_work_area(x, y);
            let max_scr_w = ((scr_r - scr_l) - 24).max(400) as f32;
            let w = (total_w.max(coach_w).max(220.0 * dpi_scale).min(max_scr_w)) as i32;
            let h = (item_font_h + 20.0 * dpi_scale) as i32 + coach_h;
            (w, h)
        };
        self.finish_show_candidates_only(candidates, selected, page_start, width, height, x, y, dpi_scale, opacity);
    }

    pub fn hide(&mut self) {
        if let Ok(mut state) = get_candidate_state().write() {
            state.composition_text.clear();
            state.pending_syllable.clear();
            state.candidates.clear();
            state.selected_index = 0;
            state.page_offset = 0;
            state.is_cand_selecting = false;
            state.target_char_idx = 0;
            state.target_range = (0, 0);
            state.selection_range = (0, 0);
            state.array30_coach_hint.clear();
            state.is_array30_live_preview = false;
            state.is_candidates_only = true;
        }
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_HIDE_TOAST);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// 切換輸入模式時（無組字狀態下）短暫顯示 1.2 秒的狀態膠囊提示，讓使用者即時確認已切換至何種輸入狀態
    pub fn show_mode_toast(&mut self, mode_code: &str, is_caps: bool, x: i32, y: i32) {
        let (badge, mode_name) = get_mode_badge_and_name(mode_code, is_caps);
        let toast = format!("如喜輸入狀態  ➔  【 半  {} 】 {}", badge, mode_name);
        if let Ok(mut state) = get_candidate_state().write() {
            state.keyboard_mode = mode_code.to_string();
            state.is_caps_on = is_caps;
            state.array30_coach_hint = toast;
            state.is_array30_live_preview = false;
            state.is_cand_selecting = false;
        }
        self.show_candidates_only(&[], 0, x, y);
        unsafe {
            let _ = SetTimer(self.hwnd, TIMER_HIDE_TOAST, 1200, None);
        }
    }

    fn finish_show_candidates_only(
        &mut self,
        candidates: &[String],
        selected: usize,
        page_start: usize,
        width: i32,
        height: i32,
        x: i32,
        y: i32,
        dpi_scale: f32,
        opacity: u32,
    ) {
        let (screen_l, screen_t, screen_r, screen_b) = Self::get_monitor_work_area(x, y);
        let min_x = screen_l + 8;
        let max_x = (screen_r - width - 8).max(min_x);
        let min_y = screen_t + 8;
        let max_y = (screen_b - height - 8).max(min_y);

        let final_x = x.clamp(min_x, max_x);
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
            state.is_candidates_only = true;
            state.composition_text.clear();
            if x > 0 && y > 0 {
                state.last_caret_x = x;
                state.last_caret_y = y;
            }
        }

        unsafe {
            apply_window_opacity(self.hwnd, opacity);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                final_x, final_y, width, height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(self.hwnd, None, true);
        }
    }

    pub fn set_selected(&mut self, index: usize) {
        let is_cands_only = if let Ok(mut state) = get_candidate_state().write() {
            state.selected_index = index;
            state.is_candidates_only
        } else {
            true
        };
        unsafe {
            if !is_cands_only {
                relayout_and_repaint_window(self.hwnd);
            } else {
                let _ = InvalidateRect(self.hwnd, None, true);
            }
        }
    }

    pub fn set_selected_with_coach(&mut self, index: usize, coach_hint: &str) {
        let is_cands_only = if let Ok(mut state) = get_candidate_state().write() {
            state.selected_index = index;
            state.array30_coach_hint = coach_hint.to_string();
            state.is_candidates_only
        } else {
            true
        };
        unsafe {
            if !is_cands_only {
                relayout_and_repaint_window(self.hwnd);
            } else {
                let _ = InvalidateRect(self.hwnd, None, true);
            }
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

pub unsafe fn apply_window_opacity(hwnd: HWND, opacity_pct: u32) {
    if hwnd.0.is_null() {
        return;
    }
    let clamped = opacity_pct.clamp(20, 100);
    let alpha = ((clamped * 255) / 100).clamp(50, 255) as u8;
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    if (ex_style & (WS_EX_LAYERED.0 as i32)) == 0 {
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED.0 as i32));
    }
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
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

/// 將輸入座 UI 上直接調整之字體大小、候選字體大小、透明度與單行最大字數即時寫入 config.json 與 settings_config.json
pub fn save_ui_appearance_to_configs(
    font_size: u32,
    candidate_font_size: u32,
    desk_opacity: u32,
    max_composition_chars: usize,
) {
    let paths = crate::service::RuhiPaths::resolve();
    let mut flat_targets = vec![
        paths.config.clone(),
        paths.sandbox_dir.join("scripts").join("config.json"),
        paths.sandbox_dir.join("dist").join("config.json"),
    ];
    flat_targets.dedup();

    for cfg_path in &flat_targets {
        if cfg_path.exists() {
            if let Ok(content) = std::fs::read_to_string(cfg_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    val["font_size"] = serde_json::Value::from(font_size);
                    val["candidate_font_size"] = serde_json::Value::from(candidate_font_size);
                    val["desk_opacity"] = serde_json::Value::from(desk_opacity);
                    val["max_composition_chars"] = serde_json::Value::from(max_composition_chars);
                    if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                        let _ = std::fs::write(cfg_path, new_content);
                    }
                }
            }
        }
    }

    let mut studio_targets = vec![
        paths.sandbox_dir.join("settings_config.json"),
        paths.sandbox_dir.join("scripts").join("settings_config.json"),
        paths.sandbox_dir.join("dist").join("settings_config.json"),
    ];
    studio_targets.dedup();

    for st_path in &studio_targets {
        if st_path.exists() {
            if let Ok(content) = std::fs::read_to_string(st_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(app) = val.get_mut("appearance").and_then(|v| v.as_object_mut()) {
                        app.insert("font_size".to_string(), serde_json::Value::from(font_size));
                        app.insert("candidate_font_size".to_string(), serde_json::Value::from(candidate_font_size));
                        app.insert("desk_opacity".to_string(), serde_json::Value::from(desk_opacity));
                        app.insert("max_composition_chars".to_string(), serde_json::Value::from(max_composition_chars));
                        if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                            let _ = std::fs::write(st_path, new_content);
                        }
                    }
                }
            }
        }
    }
    crate::service::invalidate_config_cache();
}

pub unsafe fn relayout_and_repaint_window(hwnd: HWND) {
    if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
        return;
    }
    let (fx, fy, w, h, opacity, is_cands_only, is_empty_desk) = if let Ok(state) = get_candidate_state().read() {
        let (x, y, width, height) = CandidateWindow::compute_desk_geometry(&state, state.last_caret_x, state.last_caret_y);
        let empty = state.composition_text.is_empty() && state.candidates.is_empty() && state.array30_coach_hint.is_empty();
        (x, y, width, height, state.desk_opacity, state.is_candidates_only, empty)
    } else {
        return;
    };
    if is_empty_desk {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return;
    }
    apply_window_opacity(hwnd, opacity);
    if !is_cands_only {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            fx, fy, w, h,
            SWP_NOACTIVATE,
        );
    }
    let _ = InvalidateRect(hwnd, None, true);
}

pub unsafe fn apply_ui_tune_action(hwnd: HWND, action: UiTuneAction) {
    let mut new_vals = None;
    if let Ok(mut st) = get_candidate_state().write() {
        match action {
            UiTuneAction::UiScaleDown => {
                st.font_size = (st.font_size - 2.0).clamp(16.0, 96.0);
                st.candidate_font_size = (st.candidate_font_size - 2.0).clamp(14.0, 72.0);
            }
            UiTuneAction::UiScaleUp => {
                st.font_size = (st.font_size + 2.0).clamp(16.0, 96.0);
                st.candidate_font_size = (st.candidate_font_size + 2.0).clamp(14.0, 72.0);
            }
            UiTuneAction::OpacityDown => {
                st.desk_opacity = st.desk_opacity.saturating_sub(5).clamp(20, 100);
            }
            UiTuneAction::OpacityUp => {
                st.desk_opacity = (st.desk_opacity + 5).clamp(20, 100);
            }
            UiTuneAction::DeskFontDown => {
                st.font_size = (st.font_size - 2.0).clamp(16.0, 96.0);
            }
            UiTuneAction::DeskFontUp => {
                st.font_size = (st.font_size + 2.0).clamp(16.0, 96.0);
            }
            UiTuneAction::CandFontDown => {
                st.candidate_font_size = (st.candidate_font_size - 2.0).clamp(14.0, 72.0);
            }
            UiTuneAction::CandFontUp => {
                st.candidate_font_size = (st.candidate_font_size + 2.0).clamp(14.0, 72.0);
            }
            UiTuneAction::MaxCharsDown => {
                st.max_composition_chars = st.max_composition_chars.saturating_sub(2).clamp(8, 80);
            }
            UiTuneAction::MaxCharsUp => {
                st.max_composition_chars = (st.max_composition_chars + 2).clamp(8, 80);
            }
        }
        new_vals = Some((
            st.font_size.round() as u32,
            st.candidate_font_size.round() as u32,
            st.desk_opacity,
            st.max_composition_chars,
        ));
    }
    if let Some((fs, cfs, op, mc)) = new_vals {
        save_ui_appearance_to_configs(fs, cfs, op, mc);
        relayout_and_repaint_window(hwnd);
    }
}

pub unsafe fn find_existing_studio_hwnd() -> Option<HWND> {
    let exact_title = w!("RuhI（如喜）輸入法 — 現代化偏好設定中心");
    if let Ok(h) = FindWindowW(PCWSTR::null(), exact_title) {
        if !h.0.is_null() && IsWindow(h).as_bool() {
            return Some(h);
        }
    }

    unsafe extern "system" fn enum_studio_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title.starts_with("RuhI（如喜）輸入法 — 現代化偏好設定")
                || title.starts_with("RuhI Universal IME — Preferences")
                || title.starts_with("RuhI（如喜）入力システム — 環境設定")
                || title.starts_with("RuhI 범용 입력기 — 환경설정")
            {
                let out_ptr = lparam.0 as *mut HWND;
                if !out_ptr.is_null() {
                    *out_ptr = hwnd;
                }
                return BOOL::from(false);
            }
        }
        BOOL::from(true)
    }

    let mut found = HWND::default();
    let _ = EnumWindows(Some(enum_studio_proc), LPARAM(&mut found as *mut HWND as isize));
    if !found.0.is_null() && IsWindow(found).as_bool() {
        Some(found)
    } else {
        None
    }
}

pub unsafe fn activate_and_bring_to_front(hwnd: HWND) {
    if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
        return;
    }
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    } else {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    let fg_hwnd = GetForegroundWindow();
    let fg_tid = if !fg_hwnd.0.is_null() {
        GetWindowThreadProcessId(fg_hwnd, None)
    } else {
        0
    };
    let target_tid = GetWindowThreadProcessId(hwnd, None);
    let cur_tid = GetCurrentThreadId();

    let attached_fg = if fg_tid != 0 && fg_tid != cur_tid {
        AttachThreadInput(cur_tid, fg_tid, true).as_bool()
    } else {
        false
    };
    let attached_target = if target_tid != 0 && target_tid != cur_tid && target_tid != fg_tid {
        AttachThreadInput(cur_tid, target_tid, true).as_bool()
    } else {
        false
    };

    let _ = AllowSetForegroundWindow(ASFW_ANY);
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetActiveWindow(hwnd);
    let _ = SetFocus(hwnd);

    if attached_target {
        let _ = AttachThreadInput(cur_tid, target_tid, false);
    }
    if attached_fg {
        let _ = AttachThreadInput(cur_tid, fg_tid, false);
    }
}

fn write_studio_wakeup_signal(paths: &crate::service::RuhiPaths, category: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let payload = serde_json::json!({
        "category": category,
        "timestamp": ts,
    });
    if let Ok(json_str) = serde_json::to_string(&payload) {
        let sp = paths.sandbox_dir.join("studio_wakeup.json");
        let _ = std::fs::write(&sp, &json_str);
    }
}

/// 檢查當前宿主程序是否以高權限 (Elevated Administrator) 執行。
/// 若宿主程序具備高權限，一律禁止由 TSF 輸入法背景自動衍生子程序，徹底杜絕本機權限提升 (LPE) 風險。
fn is_current_process_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if windows::Win32::System::Threading::OpenProcessToken(
            windows::Win32::System::Threading::GetCurrentProcess(),
            windows::Win32::Security::TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return false;
        }
        let mut elevation = windows::Win32::Security::TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = windows::Win32::Security::GetTokenInformation(
            token,
            windows::Win32::Security::TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<windows::Win32::Security::TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

fn resolve_trusted_studio_exe(paths: &crate::service::RuhiPaths) -> Option<std::path::PathBuf> {
    let mut candidates = vec![paths.sandbox_dir.join("ruhi-studio.exe")];
    if cfg!(debug_assertions) {
        candidates.push(paths.sandbox_dir.join("scripts").join("ruhi-studio.exe"));
        candidates.push(paths.sandbox_dir.join("dist").join("ruhi-studio.exe"));
        candidates.push(
            paths
                .sandbox_dir
                .join("ruhi-studio")
                .join("target")
                .join("release")
                .join("ruhi-studio.exe"),
        );
    } else {
        // 支援開發樹內 scripts/ruhi_tsf.dll 之上一層根目錄或 dist/ 子目錄（均限於 DLL 自身所屬樹內，絕不讀取外部硬編碼路徑）
        if paths.sandbox_dir.join("ruhi_lexicon").is_dir() {
            candidates.push(paths.sandbox_dir.join("dist").join("ruhi-studio.exe"));
            candidates.push(
                paths
                    .sandbox_dir
                    .join("ruhi-studio")
                    .join("target")
                    .join("release")
                    .join("ruhi-studio.exe"),
            );
        }
    }
    candidates.into_iter().find(|p| p.exists() && p.is_file())
}

fn launch_or_focus_studio(category: &str, pass_tuner_arg: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let paths = crate::service::RuhiPaths::resolve();
    write_studio_wakeup_signal(&paths, category);

    unsafe {
        if let Some(existing_hwnd) = find_existing_studio_hwnd() {
            logger::log(&format!(
                "launch_or_focus_studio: waking existing ruhi-studio hwnd={:?}, category={}",
                existing_hwnd, category
            ));
            activate_and_bring_to_front(existing_hwnd);
            return;
        }
    }

    if let Some(rust_exe) = resolve_trusted_studio_exe(&paths) {
        logger::log(&format!("launch_or_focus_studio: spawning native Rust {:?} (category={})", rust_exe, category));
        let work_dir = rust_exe.parent().unwrap_or(&paths.sandbox_dir);
        let mut cmd = std::process::Command::new(&rust_exe);
        if pass_tuner_arg {
            cmd.arg("--tuner");
        }
        cmd.arg("--lang").arg(current_effective_ui_lang());
        if false {
        }
        let _ = cmd
            .current_dir(work_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        return;
    }

    logger::log("launch_or_focus_studio: ruhi-studio.exe not found in trusted installation path!");
}

pub fn launch_settings() {
    launch_or_focus_studio("Appearance", false);
}

pub fn launch_semantic_tuner() {
    launch_or_focus_studio("SemanticTuner", true);
}

static LAST_TELEMETRY_SPAWN_CHECK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 背景靜默檢查是否已到達使用者設定之匿名改字雙句回傳週期或 Tailcat 自動同步週期 (`weekly` / `daily`)；
/// 若已到期且當前宿主程序非高權限 (Non-Elevated)，自動以無視窗模式 (`CREATE_NO_WINDOW`) 呼叫受信任安裝目錄下之 `ruhi-studio.exe`。
pub fn maybe_spawn_background_telemetry_upload(paths: &crate::service::RuhiPaths, cfg: &crate::service::RuhiConfig) {
    // 安全防禦：若當前宿主程序已提升權限 (Elevated Administrator)，一律禁止背景衍生子程序
    if is_current_process_elevated() {
        return;
    }

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // 同一程序內每小時至多檢查一次，零效能損耗
    let prev_check = LAST_TELEMETRY_SPAWN_CHECK.load(std::sync::atomic::Ordering::Relaxed);
    if now_ts.saturating_sub(prev_check) < 3600 {
        return;
    }
    LAST_TELEMETRY_SPAWN_CHECK.store(now_ts, std::sync::atomic::Ordering::Relaxed);

    let interval_secs = match cfg.upload_schedule.as_str() {
        "daily" => 86_400u64,
        "weekly" => 604_800u64,
        _ => 86_400u64,
    };

    let mut need_upload = false;
    if cfg.telemetry_consent && matches!(cfg.upload_schedule.as_str(), "daily" | "weekly") {
        if now_ts.saturating_sub(cfg.last_upload_timestamp) >= interval_secs {
            let rev_file = paths.data_dir.join("pending_revisions.json");
            if let Ok(meta) = std::fs::metadata(&rev_file) {
                if meta.len() > 5 {
                    need_upload = true;
                }
            }
        }
    }

    let need_tailcat = cfg.tailcat_auto_sync
        && cfg.tailcat_update_mode != "manual"
        && now_ts.saturating_sub(cfg.last_tailcat_sync_timestamp) >= interval_secs;

    if !need_upload && !need_tailcat {
        return;
    }

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    if let Some(rust_exe) = resolve_trusted_studio_exe(paths) {
        let work_dir = rust_exe.parent().unwrap_or(&paths.sandbox_dir);
        let cli_flag = if need_upload {
            "--upload-revisions"
        } else {
            "--tailcat-sync"
        };
        let _ = std::process::Command::new(&rust_exe)
            .arg(cli_flag)
            .current_dir(work_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

pub fn show_about_dialog(hwnd: HWND) {
    let (kb_mode, is_caps) = if let Ok(s) = get_candidate_state().read() {
        (s.keyboard_mode.to_lowercase(), s.is_caps_on)
    } else {
        ("hsu".to_string(), false)
    };

    let ui_lang = current_effective_ui_lang();
    let (title, msg) = if ui_lang == "en-US" || is_caps || kb_mode == "english" || kb_mode == "latin" || kb_mode == "western" {
        (
            "About RuhI",
            "RuhI — Responsive Universal Human Input\nVersion: 0.5.5 (Release)\nFlowing like clouds and water, effortless and joyful."
        )
    } else {
        match kb_mode.as_str() {
            "japanese" | "kana" | "romaji" => (
                "RuhI について",
                "RuhI 如喜スマートIME (Responsive Universal Human Input)\nバージョン: 0.5.5 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            "korean" | "ko" | "hangul" | "romaja" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기 (Responsive Universal Human Input)\n버전: 0.5.5 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            "iast" | "sanskrit" => (
                "About RuhI (IAST)",
                "RuhI — Responsive Universal Human Input (Sanskrit root: Rūhi — to sprout, to grow)\nVersion: 0.5.5 (Release)\nYathā meghāḥ calanti yathā vāri vahati, svacchandataḥ ānandaḥ.\n(如雲行水流自在歡喜)"
            ),
            "thai" => (
                "เกี่ยวกับ RuhI",
                "RuhI — Responsive Universal Human Input\nเวอร์ชัน: 0.5.5 (Release)\nดั่งเมฆเคลื่อน ดั่งสายน้ำไหล สบายใจและเปี่ยมสุข"
            ),
            _ if ui_lang == "ja-JP" => (
                "RuhI について",
                "RuhI 如喜スマートIME (Responsive Universal Human Input)\nバージョン: 0.5.5 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            _ if ui_lang == "ko-KR" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기 (Responsive Universal Human Input)\n버전: 0.5.5 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            // 中文模式（許氏、大千、倉頡、倚天、拼音、大易、行列等）：
            _ => (
                "關於如喜",
                "RuhI 如喜通用智慧型輸入法 (Responsive Universal Human Input)\n版本: 0.5.5 (Release)\n如雲行水流自在歡喜 —— 再也不必望穿秋水，等候一個可能永遠不會進步的官方詞庫。"
            ),
        }
    };
    /*
                "RuhI について",
                "RuhI 如喜スマートIME (Responsive Universal Human Input)\nバージョン: 0.5.0 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            "korean" | "ko" | "hangul" | "romaja" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기 (Responsive Universal Human Input)\n버전: 0.5.0 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            "iast" | "sanskrit" => (
                "About RuhI (IAST)",
                "RuhI — Responsive Universal Human Input (Sanskrit root: Rūhi — to sprout, to grow)\nVersion: 0.5.0 (Release)\nYathā meghāḥ calanti yathā vāri vahati, svacchandataḥ ānandaḥ.\n(如雲行水流自在歡喜)"
            ),
            "thai" => (
                "เกี่ยวกับ RuhI",
                "RuhI — Responsive Universal Human Input\nเวอร์ชัน: 0.5.0 (Release)\nดั่งเมฆเคลื่อน ดั่งสายน้ำไหล สบายใจและเปี่ยมสุข"
            ),
            // 中文模式（許氏、大千、倉頡、倚天、拼音、大易、行列等）：
            _ => (
                "關於如喜",
                "RuhI 如喜通用智慧型輸入法 (Responsive Universal Human Input)\n版本: 0.5.0 (Release)\n如雲行水流自在歡喜 —— 再也不必望穿秋水，等候一個可能永遠不會進步的官方詞庫。"
            ),
        }
    };

    */
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

fn get_context_menu_labels() -> (&'static str, &'static str, &'static str) {
    let (kb_mode, is_caps) = if let Ok(s) = get_candidate_state().read() {
        (s.keyboard_mode.to_lowercase(), s.is_caps_on)
    } else {
        ("hsu".to_string(), false)
    };

    let ui_lang = current_effective_ui_lang();
    if ui_lang == "en-US" || is_caps || kb_mode == "english" || kb_mode == "latin" || kb_mode == "western" {
        ("Preferences Center (&S)...", "Semantic Tuning Studio (&T)...", "About RuhI (&A)")
    } else if ui_lang == "ja-JP" || matches!(kb_mode.as_str(), "japanese" | "kana" | "romaji") {
        ("設定センター (&S)...", "意味チューニングスタジオ (&T)...", "RuhI について (&A)")
    } else if ui_lang == "ko-KR" || matches!(kb_mode.as_str(), "korean" | "ko" | "hangul" | "romaja") {
        ("환경설정 (&S)...", "의미 튜닝 스튜디오 (&T)...", "RuhI 정보 (&A)")
    } else {
        ("偏好設定中心 (&S)...", "語意調校工作室 (&T)...", "關於 RuhI 如喜 (&A)")
    }
}

#[allow(dead_code)]
fn _legacy_context_menu_labels(_kb_mode: String, _is_caps: bool) -> (&'static str, &'static str, &'static str) { ("", "", "") }
/*
    if is_caps || kb_mode == "english" || kb_mode == "latin" || kb_mode == "western" {
        ("Preferences Center (&S)...", "Semantic Tuning Studio (&T)...", "About RuhI (&A)")
    } else {
        match kb_mode.as_str() {
            "japanese" | "kana" | "romaji" => (
                "RuhI について",
                "RuhI 如喜スマートIME (Responsive Universal Human Input)\nバージョン: 0.5.5 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            "korean" | "ko" | "hangul" | "romaja" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기 (Responsive Universal Human Input)\n버전: 0.5.5 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            "iast" | "sanskrit" => (
                "About RuhI (IAST)",
                "RuhI — Responsive Universal Human Input (Sanskrit root: Rūhi — to sprout, to grow)\nVersion: 0.5.5 (Release)\nYathā meghāḥ calanti yathā vāri vahati, svacchandataḥ ānandaḥ.\n(如雲行水流自在歡喜)"
            ),
            "thai" => (
                "เกี่ยวกับ RuhI",
                "RuhI — Responsive Universal Human Input\nเวอร์ชัน: 0.5.5 (Release)\nดั่งเมฆเคลื่อน ดั่งสายน้ำไหล สบายใจและเปี่ยมสุข"
            ),
            _ if ui_lang == "ja-JP" => (
                "RuhI について",
                "RuhI 如喜スマートIME (Responsive Universal Human Input)\nバージョン: 0.5.5 (Release)\n雲の行くが如く水の流るるが如く、自在にして歓喜。"
            ),
            _ if ui_lang == "ko-KR" => (
                "RuhI 정보",
                "RuhI 루히 유니버설 스마트 입력기 (Responsive Universal Human Input)\n버전: 0.5.5 (Release)\n구름 가듯 물 흐르듯 자유롭고 기쁘게."
            ),
            // 中文模式（許氏、大千、倉頡、倚天、拼音、大易、行列等）：
            _ => (
                "關於如喜",
                "RuhI 如喜通用智慧型輸入法 (Responsive Universal Human Input)\n版本: 0.5.5 (Release)\n如雲行水流自在歡喜 —— 再也不必望穿秋水，等候一個可能永遠不會進步的官方詞庫。"
            ),
        }
    };
    // "設定センター (&S)...", "意味チューニングスタジオ (&T)...", "RuhI について (&A)"),
            "korean" | "ko" | "hangul" | "romaja" => ("환경설정 (&S)...", "의미 튜닝 스튜디오 (&T)...", "RuhI 정보 (&A)"),
            _ => ("偏好設定中心 (&S)...", "語意調校工作室 (&T)...", "關於 RuhI 如喜 (&A)"),
        }
    }
}


*/
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
        let has_coach = !state.array30_coach_hint.is_empty();
        let coach_h = if has_coach { (28.0 * state.dpi_scale) as i32 } else { 0 };
        let use_ar30_num_keys = state.is_array30_live_preview && !state.is_cand_selecting;
        let page_sz = if use_ar30_num_keys { 10 } else { 9 };
        let cur_page = state.selected_index / page_sz;
        let page_start = cur_page * page_sz;
        let page_end = (page_start + page_sz).min(state.candidates.len());
        let total_pages = (state.candidates.len() + page_sz - 1) / page_sz.max(1);
        let asdf_keys = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

        let item_font_h = (state.candidate_font_size * 0.9 * state.dpi_scale).clamp(13.0 * state.dpi_scale, 68.0 * state.dpi_scale);
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

        if state.candidates.is_empty() && has_coach {
            // 純行列 30 即時反查教練浮動小卡
            let coach_font_h = -(15.0 * state.dpi_scale).round() as i32;
            let coach_font = CreateFontW(
                coach_font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_cf = SelectObject(mem_dc, coach_font);
            SetTextColor(mem_dc, accent_commit);
            let mut coach_wide: Vec<u16> = state.array30_coach_hint.encode_utf16().chain(Some(0)).collect();
            let mut coach_rect = RECT {
                left: (10.0 * state.dpi_scale) as i32,
                top: 2,
                right: width - (10.0 * state.dpi_scale) as i32,
                bottom: height - 2,
            };
            DrawTextW(mem_dc, &mut coach_wide, &mut coach_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem_dc, old_cf);
            let _ = DeleteObject(coach_font);
        } else if state.orientation == "vertical" {
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
                let key_label = if use_ar30_num_keys {
                    if idx == 9 { "0".to_string() } else { (idx + 1).to_string() }
                } else if state.quick_key_mode == "asdf" {
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
                let footer_y = height - coach_h - (20.0 * state.dpi_scale) as i32;
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
                    bottom: height - coach_h - 2,
                };
                DrawTextW(mem_dc, &mut page_wide, &mut page_rect, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
            }
        } else {
            // ── 橫向微軟新注音候選卡片（單行精巧流式排列）──
            let pad_y = (6.0 * state.dpi_scale) as i32;
            let item_h = (height - coach_h) - pad_y * 2;
            let mut cur_x = (10.0 * state.dpi_scale) as i32;

            for i in page_start..page_end {
                let idx = i - page_start;
                let cand_wide_measure: Vec<u16> = state.candidates[i].encode_utf16().collect();
                let mut sz_cand = SIZE::default();
                let _ = GetTextExtentPoint32W(mem_dc, &cand_wide_measure, &mut sz_cand);
                let item_w = (24.0 * state.dpi_scale) as i32 + sz_cand.cx + (12.0 * state.dpi_scale) as i32;
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

                let key_label = if use_ar30_num_keys {
                    if idx == 9 { "0".to_string() } else { (idx + 1).to_string() }
                } else if state.quick_key_mode == "asdf" {
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

        if !state.candidates.is_empty() && has_coach {
            let coach_top = height - coach_h;
            let sep_rect = RECT { left: 8, top: coach_top, right: width - 8, bottom: coach_top + 1 };
            let sep_brush = CreateSolidBrush(border_color);
            FillRect(mem_dc, &sep_rect, sep_brush);
            let _ = DeleteObject(sep_brush);

            let coach_font_h = -(14.5 * state.dpi_scale).round() as i32;
            let coach_font = CreateFontW(
                coach_font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_cf = SelectObject(mem_dc, coach_font);
            SetTextColor(mem_dc, accent_commit);
            let mut coach_wide: Vec<u16> = state.array30_coach_hint.encode_utf16().chain(Some(0)).collect();
            let mut coach_rect = RECT {
                left: (10.0 * state.dpi_scale) as i32,
                top: coach_top + 2,
                right: width - (10.0 * state.dpi_scale) as i32,
                bottom: height - 2,
            };
            DrawTextW(mem_dc, &mut coach_wide, &mut coach_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem_dc, old_cf);
            let _ = DeleteObject(coach_font);
        }

        SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font);
    } else {
        // 【如喜輸入座 (RuhI Input Desk) — 支援多行自動換行、獨立候選字級與內嵌即時調校列】
        let pad_x = (8.0 * state.dpi_scale) as i32;
        let mut cur_y = (6.0 * state.dpi_scale) as i32;

        let font_h = (state.font_size * 0.9 * state.dpi_scale).clamp(14.0 * state.dpi_scale, 90.0 * state.dpi_scale);
        let max_line_chars = state.max_composition_chars.clamp(8, 80);
        let comp_chars: Vec<char> = state.composition_text.chars().collect();
        let (comp_clusters, num_lines, _max_used_px, _max_line_px) =
            CandidateWindow::layout_composition_clusters(&comp_chars, font_h, max_line_chars);
            let line_step_h = (font_h + 10.0 * state.dpi_scale).round() as i32;
        let comp_card_h = (num_lines as i32 * line_step_h) + (12.0 * state.dpi_scale) as i32;
        let comp_rect = RECT {
            left: pad_x,
            top: cur_y,
            right: width - pad_x,
            bottom: cur_y + comp_card_h,
        };

        // 1. 組字卡片 (Composition Zone) — 底色 bg_comp，超過單行最大字數時自動換行擴展
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

        let cur = state.cursor_pos.min(comp_chars.len());
        let sel_bg_brush = CreateSolidBrush(accent_blue);
        let cursor_brush = CreateSolidBrush(accent_commit);

        let base_left_x = comp_rect.left + (12.0 * state.dpi_scale) as i32;
        let base_top_y = comp_rect.top + (6.0 * state.dpi_scale) as i32;
        let mut pen_x = base_left_x;
        let mut last_line_top = base_top_y;
        let mut last_line_bottom = base_top_y + line_step_h;

        // 以完整字素叢集（基底字母 + 緊隨之組合變音符號 U+0300..=U+036F）為單位繪製，
        // 避免 GDI 因逐一 char 拆開而使組合尖音符/底抑揚符與基底歐文字母分離錯位！
        for &(i, cluster_end, line_idx, is_line_start, _cx_est) in &comp_clusters {
            let line_top = base_top_y + (line_idx as i32 * line_step_h);
            let line_bottom = line_top + line_step_h;
            if is_line_start {
                pen_x = base_left_x;
            }
            last_line_top = line_top;
            last_line_bottom = line_bottom;

            // 繪製游標垂直線
            if cur >= i && cur < cluster_end {
                let caret_r = RECT {
                    left: pen_x,
                    top: line_top + (2.0 * state.dpi_scale) as i32,
                    right: pen_x + (2.0 * state.dpi_scale).max(2.0) as i32,
                    bottom: line_bottom - (2.0 * state.dpi_scale) as i32,
                };
                FillRect(mem_dc, &caret_r, cursor_brush);
                pen_x += (3.0 * state.dpi_scale) as i32;
            }

            let cluster_str: String = comp_chars[i..cluster_end].iter().collect();
            let ch_wide: Vec<u16> = cluster_str.encode_utf16().collect();
            let mut sz_ch = SIZE::default();
            let _ = GetTextExtentPoint32W(mem_dc, &ch_wide, &mut sz_ch);

            // 若正處於候選字選取模式且此字為目標選字，或處於 Shift+方向鍵手動選取範圍：以 accent_blue 高亮底色包覆
            let is_in_target = if state.target_range.1 > state.target_range.0 {
                i >= state.target_range.0 && i < state.target_range.1
            } else {
                state.target_char_idx >= i && state.target_char_idx < cluster_end
            };

            let is_in_selection = if state.selection_range.1 > state.selection_range.0 {
                i >= state.selection_range.0 && i < state.selection_range.1
            } else {
                false
            };

            if (state.is_cand_selecting && is_in_target) || is_in_selection {
                let hl_r = RECT {
                    left: pen_x - 1,
                    top: line_top + (1.0 * state.dpi_scale) as i32,
                    right: pen_x + sz_ch.cx + 2,
                    bottom: line_bottom - (1.0 * state.dpi_scale) as i32,
                };
                FillRect(mem_dc, &hl_r, sel_bg_brush);
                SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
            } else {
                SetTextColor(mem_dc, accent_commit); // 碧玉翠綠色
            }

            let mut r_ch = RECT { left: pen_x, top: line_top, right: pen_x + sz_ch.cx + 4, bottom: line_bottom };
            let mut c_w = ch_wide;
            c_w.push(0);
            DrawTextW(mem_dc, &mut c_w, &mut r_ch, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            pen_x += sz_ch.cx;
        }
        // 若游標在句尾
        if cur == comp_chars.len() {
            let caret_r = RECT {
                left: pen_x,
                top: last_line_top + (2.0 * state.dpi_scale) as i32,
                right: pen_x + (2.0 * state.dpi_scale).max(2.0) as i32,
                bottom: last_line_bottom - (2.0 * state.dpi_scale) as i32,
            };
            FillRect(mem_dc, &caret_r, cursor_brush);
        }

        let _ = DeleteObject(sel_bg_brush);
        let _ = DeleteObject(cursor_brush);
        SelectObject(mem_dc, old_f);
        let _ = DeleteObject(comp_font);

        cur_y = comp_rect.bottom + (4.0 * state.dpi_scale) as i32;

        // 2. 候選欄 (Candidate Panel) — 底色 bg_cand，候選字體大小獨立受 candidate_font_size 控制
        let show_cand_panel = (state.is_cand_selecting || state.is_array30_live_preview) && !state.candidates.is_empty();
        let use_ar30_num_keys = state.is_array30_live_preview && !state.is_cand_selecting;
        let page_sz = if use_ar30_num_keys { 10 } else { 9 };

        if show_cand_panel {
            let cand_font_px = (state.candidate_font_size * 0.9 * state.dpi_scale).clamp(13.0 * state.dpi_scale, 68.0 * state.dpi_scale);
            let (cand_items, _single_row_px) = CandidateWindow::measure_page_candidates_px(&state, cand_font_px, page_sz);
            let inner_cand_w = (width - pad_x * 2).max(200);
            let cand_rows = CandidateWindow::count_candidate_rows(&cand_items, inner_cand_w, state.dpi_scale);
            let cand_row_step_h = (cand_font_px + 10.0 * state.dpi_scale).round() as i32;
            let cand_card_h = (cand_rows as i32 * cand_row_step_h) + (8.0 * state.dpi_scale) as i32;
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

            let cand_font_h = -(cand_font_px).round() as i32;
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

            let left_start_x = cand_rect.left + (12.0 * state.dpi_scale) as i32;
            let right_limit_x = (cand_rect.right - (8.0 * state.dpi_scale) as i32).max(left_start_x + 60);
            let mut cand_x = left_start_x;
            let mut cand_row_idx = 0i32;
            for (actual_idx, item_str, sz_cx) in cand_items {
                if cand_x > left_start_x && cand_x + sz_cx + 6 > right_limit_x {
                    cand_row_idx += 1;
                    cand_x = left_start_x;
                }
                let row_top = cand_rect.top + (4.0 * state.dpi_scale) as i32 + (cand_row_idx * cand_row_step_h);
                let row_bottom = row_top + cand_row_step_h;
                let item_wide: Vec<u16> = item_str.encode_utf16().collect();
                let is_sel = actual_idx == state.selected_index && state.is_cand_selecting;
                let item_bg = RECT {
                    left: cand_x - 4,
                    top: row_top,
                    right: cand_x + sz_cx + 6,
                    bottom: row_bottom,
                };
                if is_sel {
                    let sel_b = CreateSolidBrush(accent_blue);
                    FillRect(mem_dc, &item_bg, sel_b);
                    let _ = DeleteObject(sel_b);
                    SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                } else {
                    SetTextColor(mem_dc, ink_primary);
                }
                let mut w = item_wide;
                w.push(0);
                let mut r = item_bg;
                r.left += 4;
                DrawTextW(mem_dc, &mut w, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                cand_x += sz_cx + (16.0 * state.dpi_scale) as i32;
            }

            SelectObject(mem_dc, old_f2);
            let _ = DeleteObject(cand_font);

            cur_y = cand_rect.bottom + (4.0 * state.dpi_scale) as i32;
        }

        // 2.5 「許氏注音 ➔ 行列 30 即時反查教練列」或「行列拆碼座標列」
        if !state.array30_coach_hint.is_empty() {
            let coach_card_h = (24.0 * state.dpi_scale) as i32;
            let coach_rect = RECT {
                left: pad_x,
                top: cur_y,
                right: width - pad_x,
                bottom: cur_y + coach_card_h,
            };

            let coach_bg = CreateSolidBrush(bg_cand);
            FillRect(mem_dc, &coach_rect, coach_bg);
            let _ = DeleteObject(coach_bg);
            FrameRect(mem_dc, &coach_rect, border_brush);

            let coach_font_h = -(14.5 * state.dpi_scale).round() as i32;
            let coach_font = CreateFontW(
                coach_font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_fc = SelectObject(mem_dc, coach_font);
            SetTextColor(mem_dc, accent_commit);
            let mut coach_wide: Vec<u16> = state.array30_coach_hint.encode_utf16().chain(Some(0)).collect();
            let mut coach_text_rect = RECT {
                left: coach_rect.left + (10.0 * state.dpi_scale) as i32,
                top: coach_rect.top,
                right: coach_rect.right - (8.0 * state.dpi_scale) as i32,
                bottom: coach_rect.bottom,
            };
            DrawTextW(mem_dc, &mut coach_wide, &mut coach_text_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem_dc, old_fc);
            let _ = DeleteObject(coach_font);

            cur_y = coach_rect.bottom + (4.0 * state.dpi_scale) as i32;
        }

        // 2.8 輸入座內嵌即時調校列 (On-Desk Interactive UI Tuner Bar)
        let mut recorded_tune_buttons: Vec<(RECT, UiTuneAction)> = Vec::new();
        if state.show_ui_tuner_bar {
            let tuner_h = (26.0 * state.dpi_scale) as i32;
            let tuner_rect = RECT {
                left: pad_x,
                top: cur_y,
                right: width - pad_x,
                bottom: cur_y + tuner_h,
            };
            let tuner_bg = CreateSolidBrush(bg_cand);
            FillRect(mem_dc, &tuner_rect, tuner_bg);
            let _ = DeleteObject(tuner_bg);
            FrameRect(mem_dc, &tuner_rect, border_brush);

            let tune_font_h = -(13.0 * state.dpi_scale).round() as i32;
            let tune_font = CreateFontW(
                tune_font_h, 0, 0, 0,
                FW_NORMAL.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(font_name.as_ptr()),
            );
            let old_ft = SelectObject(mem_dc, tune_font);

            let groups: [(String, UiTuneAction, UiTuneAction); 5] = [
                (
                    match crate::service::resolve_effective_ui_lang(&state.ui_lang) {
                        "en-US" => "Scale".to_string(),
                        "ja-JP" => "UIサイズ".to_string(),
                        "ko-KR" => "UI크기".to_string(),
                        _ => "UI大小".to_string(),
                    },
                    UiTuneAction::UiScaleDown,
                    UiTuneAction::UiScaleUp,
                ),
                (
                    match crate::service::resolve_effective_ui_lang(&state.ui_lang) {
                        "en-US" => format!("Alpha {}%", state.desk_opacity),
                        "ja-JP" => format!("不透明{}%", state.desk_opacity),
                        "ko-KR" => format!("불투명{}%", state.desk_opacity),
                        _ => format!("不透明{}%", state.desk_opacity),
                    },
                    UiTuneAction::OpacityDown,
                    UiTuneAction::OpacityUp,
                ),
                (
                    match crate::service::resolve_effective_ui_lang(&state.ui_lang) {
                        "en-US" => format!("Text {}", state.font_size.round() as i32),
                        "ja-JP" => format!("組字{}", state.font_size.round() as i32),
                        "ko-KR" => format!("조합{}", state.font_size.round() as i32),
                        _ => format!("座字{}", state.font_size.round() as i32),
                    },
                    UiTuneAction::DeskFontDown,
                    UiTuneAction::DeskFontUp,
                ),
                (
                    match crate::service::resolve_effective_ui_lang(&state.ui_lang) {
                        "en-US" => format!("Cand {}", state.candidate_font_size.round() as i32),
                        "ja-JP" => format!("候補{}", state.candidate_font_size.round() as i32),
                        "ko-KR" => format!("후보{}", state.candidate_font_size.round() as i32),
                        _ => format!("選字{}", state.candidate_font_size.round() as i32),
                    },
                    UiTuneAction::CandFontDown,
                    UiTuneAction::CandFontUp,
                ),
                (
                    match crate::service::resolve_effective_ui_lang(&state.ui_lang) {
                        "en-US" => format!("Wrap {}", state.max_composition_chars),
                        "ja-JP" => format!("1行{}字", state.max_composition_chars),
                        "ko-KR" => format!("1줄{}자", state.max_composition_chars),
                        _ => format!("單行{}字", state.max_composition_chars),
                    },
                    UiTuneAction::MaxCharsDown,
                    UiTuneAction::MaxCharsUp,
                ),
            ];
            let _unused_old_groups: [(String, UiTuneAction, UiTuneAction); 4] = [
                (format!("不透明{}%", state.desk_opacity), UiTuneAction::OpacityDown, UiTuneAction::OpacityUp),
                (format!("座字{}", state.font_size.round() as i32), UiTuneAction::DeskFontDown, UiTuneAction::DeskFontUp),
                (format!("選字{}", state.candidate_font_size.round() as i32), UiTuneAction::CandFontDown, UiTuneAction::CandFontUp),
                (format!("單行{}字", state.max_composition_chars), UiTuneAction::MaxCharsDown, UiTuneAction::MaxCharsUp),
            ];

            let mut tx = tuner_rect.left + (8.0 * state.dpi_scale) as i32;
            let btn_w = (19.0 * state.dpi_scale) as i32;
            let btn_pad_y = (3.0 * state.dpi_scale) as i32;
            let btn_bg_brush = CreateSolidBrush(bg_comp);

            for (label, act_down, act_up) in groups.iter() {
                SetTextColor(mem_dc, ink_secondary);
                let lbl_wide: Vec<u16> = label.encode_utf16().collect();
                let mut sz_lbl = SIZE::default();
                let _ = GetTextExtentPoint32W(mem_dc, &lbl_wide, &mut sz_lbl);
                let mut lbl_r = RECT {
                    left: tx,
                    top: tuner_rect.top,
                    right: tx + sz_lbl.cx + (4.0 * state.dpi_scale) as i32,
                    bottom: tuner_rect.bottom,
                };
                let mut lbl_w = lbl_wide;
                lbl_w.push(0);
                DrawTextW(mem_dc, &mut lbl_w, &mut lbl_r, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                tx = lbl_r.right + (2.0 * state.dpi_scale) as i32;

                // [-] 按鈕
                let r_minus = RECT {
                    left: tx,
                    top: tuner_rect.top + btn_pad_y,
                    right: tx + btn_w,
                    bottom: tuner_rect.bottom - btn_pad_y,
                };
                FillRect(mem_dc, &r_minus, btn_bg_brush);
                FrameRect(mem_dc, &r_minus, border_brush);
                SetTextColor(mem_dc, accent_commit);
                let mut minus_w: Vec<u16> = "−".encode_utf16().chain(Some(0)).collect();
                let mut mr = r_minus;
                DrawTextW(mem_dc, &mut minus_w, &mut mr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                recorded_tune_buttons.push((r_minus, *act_down));
                tx = r_minus.right + (2.0 * state.dpi_scale) as i32;

                // [+] 按鈕
                let r_plus = RECT {
                    left: tx,
                    top: tuner_rect.top + btn_pad_y,
                    right: tx + btn_w,
                    bottom: tuner_rect.bottom - btn_pad_y,
                };
                FillRect(mem_dc, &r_plus, btn_bg_brush);
                FrameRect(mem_dc, &r_plus, border_brush);
                SetTextColor(mem_dc, accent_commit);
                let mut plus_w: Vec<u16> = "+".encode_utf16().chain(Some(0)).collect();
                let mut pr = r_plus;
                DrawTextW(mem_dc, &mut plus_w, &mut pr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                recorded_tune_buttons.push((r_plus, *act_up));
                tx = r_plus.right + (10.0 * state.dpi_scale) as i32;
            }

            let _ = DeleteObject(btn_bg_brush);
            SelectObject(mem_dc, old_ft);
            let _ = DeleteObject(tune_font);

            cur_y = tuner_rect.bottom + (4.0 * state.dpi_scale) as i32;
        }

        // 3. 狀態列 (Status Bar) — 底色 bg_main，純文字極簡排版 (左：狀態，右：半 許/行 · 調校 ▾ · ⚙)
        let status_h = (24.0 * state.dpi_scale) as i32;
        let status_rect = RECT {
            left: pad_x + (4.0 * state.dpi_scale) as i32,
            top: cur_y + (2.0 * state.dpi_scale) as i32,
            right: width - pad_x - (4.0 * state.dpi_scale) as i32,
            bottom: cur_y + status_h,
        };

        let status_font_h = -(14.5 * state.dpi_scale).round() as i32;
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

        // 左側：淡墨色狀態提示（支援多行與多頁翻頁狀態提示）
        SetTextColor(mem_dc, ink_muted);
        let total_cands = state.candidates.len();
        let total_pages = if total_cands > 0 { (total_cands + page_sz - 1) / page_sz } else { 1 };
        let cur_page = (state.selected_index / page_sz) + 1;

        let eff_ui_lang = crate::service::resolve_effective_ui_lang(&state.ui_lang);
        let st_label = match eff_ui_lang {
            "en-US" => {
                if state.selection_range.1 > state.selection_range.0 {
                    "RuhI Desk  [Selected · Enter: Add / Shift+Enter: Hide]".to_string()
                } else if state.is_cand_selecting && total_pages > 1 {
                    format!("RuhI Desk  [Page {} / {} · PgUp/PgDn]", cur_page, total_pages)
                } else if num_lines > 1 {
                    format!("RuhI Desk  [Line {} · Max {} chars]", num_lines, max_line_chars)
                } else {
                    "RuhI Desk".to_string()
                }
            }
            "ja-JP" => {
                if state.selection_range.1 > state.selection_range.0 {
                    "RuhI 入力座  [範囲選択中 · Enter 登録 / Shift+Enter 非表示]".to_string()
                } else if state.is_cand_selecting && total_pages > 1 {
                    format!("RuhI 入力座  [{} / {} 頁 · PgUp/PgDn]", cur_page, total_pages)
                } else if num_lines > 1 {
                    format!("RuhI 入力座  [{} 行目 · 1行 {} 字]", num_lines, max_line_chars)
                } else {
                    "RuhI 入力座".to_string()
                }
            }
            "ko-KR" => {
                if state.selection_range.1 > state.selection_range.0 {
                    "RuhI 입력석  [범위 선택됨 · Enter 추가 / Shift+Enter 숨김]".to_string()
                } else if state.is_cand_selecting && total_pages > 1 {
                    format!("RuhI 입력석  [{} / {} 쪽 · PgUp/PgDn]", cur_page, total_pages)
                } else if num_lines > 1 {
                    format!("RuhI 입력석  [{}째 줄 · 1줄 {}자]", num_lines, max_line_chars)
                } else {
                    "RuhI 입력석".to_string()
                }
            }
            _ => {
                if state.selection_range.1 > state.selection_range.0 {
                    "如喜輸入座  [已標記範圍 · Enter 加詞 / Shift+Enter 刪詞]".to_string()
                } else if state.is_cand_selecting && total_pages > 1 {
                    format!("如喜輸入座  [{} / {} 頁 · PgUp/PgDn 翻頁]", cur_page, total_pages)
                } else if num_lines > 1 {
                    format!("如喜輸入座  [第 {} 行 · 單行 {} 字]", num_lines, max_line_chars)
                } else {
                    "如喜輸入座".to_string()
                }
            }
        };
        let _unused_st_label = if state.selection_range.1 > state.selection_range.0 {
            "如喜輸入座  [已標記範圍 · Enter 加詞 / Shift+Enter 刪詞]".to_string()
        } else if state.is_cand_selecting && total_pages > 1 {
            format!("如喜輸入座  [{} / {} 頁 · PgUp/PgDn 翻頁]", cur_page, total_pages)
        } else if num_lines > 1 {
            format!("如喜輸入座  [第 {} 行 · 單行 {} 字]", num_lines, max_line_chars)
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

        // 「調校 ▾ / ▴」按鈕：直接於輸入座 UI 展開或收合五合一即時外觀調校列
        let tune_btn_label = match eff_ui_lang {
            "en-US" => if state.show_ui_tuner_bar { "Tune▴" } else { "Tune▾" },
            "ja-JP" => if state.show_ui_tuner_bar { "調整▴" } else { "調整▾" },
            "ko-KR" => if state.show_ui_tuner_bar { "조정▴" } else { "조정▾" },
            _ => if state.show_ui_tuner_bar { "調校▴" } else { "調校▾" },
        };
        let tune_wide_no_null: Vec<u16> = tune_btn_label.encode_utf16().collect();
        let mut sz_tune = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &tune_wide_no_null, &mut sz_tune);
        let tune_right = gear_rect.left - (4.0 * state.dpi_scale) as i32;
        let tune_left = (tune_right - sz_tune.cx - (10.0 * state.dpi_scale) as i32).max(status_rect.left);
        let tune_toggle_rect = RECT {
            left: tune_left,
            top: status_rect.top,
            right: tune_right,
            bottom: status_rect.bottom,
        };
        SetTextColor(mem_dc, if state.show_ui_tuner_bar { accent_commit } else { ink_muted });
        let mut tune_text: Vec<u16> = tune_btn_label.encode_utf16().chain(Some(0)).collect();
        let mut tune_r = tune_toggle_rect;
        DrawTextW(mem_dc, &mut tune_text, &mut tune_r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        // 右側：純文字大號「半   許 / 日 / 倉 / 行 / 注 / 倚 / 拼」或「半   歐」
        SetTextColor(mem_dc, ink_secondary);
        let kb_indicator = match state.keyboard_mode.to_lowercase().as_str() {
            "english" | "en" | "latin" | "western" | "euro" | "sanskrit" => "歐",
            "hsu" => "許",
            "japanese" | "jp" | "romaji" | "kana" => "日",
            "korean" | "ko" | "hangul" | "romaja" => "韓",
            "standard" | "daikien" | "dachen" | "bopomofo" => "注",
            "cangjie" => "倉",
            "array30" | "ar30" => "行",
            "et26" | "eten26" => "倚",
            "et41" => "41",
            "pinyin" => "拼",
            _ => "中",
        };
        let (i18n_badge, _) = get_mode_badge_and_name_i18n(&state.keyboard_mode, state.is_caps_on, eff_ui_lang);
        let hw_prefix = match eff_ui_lang {
            "en-US" => "HW",
            "ko-KR" => "반",
            _ => "半",
        };
        let mode_label = format!("{}   {}", hw_prefix, i18n_badge);
        let _unused_mode_label = if state.is_caps_on { "半   歐".to_string() } else { format!("半   {}", kb_indicator) };
        let mode_wide_no_null: Vec<u16> = mode_label.encode_utf16().collect();
        let mut sz_mode = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &mode_wide_no_null, &mut sz_mode);
        let mode_right = tune_toggle_rect.left - (6.0 * state.dpi_scale) as i32;
        let mode_hit_left = (mode_right - sz_mode.cx - (12.0 * state.dpi_scale) as i32).max(status_rect.left);
        let mode_hit_rect = RECT {
            left: mode_hit_left,
            top: status_rect.top,
            right: mode_right,
            bottom: status_rect.bottom,
        };
        if let Ok(mut st) = get_candidate_state().write() {
            st.gear_rect = gear_rect;
            st.ui_tune_toggle_rect = tune_toggle_rect;
            st.mode_rect = mode_hit_rect;
            st.ui_tune_buttons = recorded_tune_buttons;
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
    save_keyboard_mode_with_last(mode, None);
}

pub fn save_keyboard_mode_with_last(mode: &str, last_mode: Option<&str>) {
    if let Ok(mut state) = get_candidate_state().write() {
        state.keyboard_mode = mode.to_string();
    }
    let paths = crate::service::RuhiPaths::resolve();
    let mut flat_targets = vec![
        paths.config.clone(),
        paths.sandbox_dir.join("scripts").join("config.json"),
        paths.sandbox_dir.join("dist").join("config.json"),
    ];
    flat_targets.dedup();

    let mut resolved_last: Option<String> = last_mode.map(|s| s.to_string());
    for cfg_path in &flat_targets {
        if cfg_path.exists() {
            if let Ok(content) = std::fs::read_to_string(cfg_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(ref lm) = resolved_last {
                        val["last_keyboard_mode"] = serde_json::Value::from(lm.clone());
                    } else if let Some(prev) = val.get("keyboard_mode").and_then(|v| v.as_str()) {
                        if !prev.eq_ignore_ascii_case(mode) {
                            resolved_last = Some(prev.to_string());
                            val["last_keyboard_mode"] = serde_json::Value::from(prev.to_string());
                        }
                    }
                    val["keyboard_mode"] = serde_json::Value::from(mode);
                    if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                        let _ = std::fs::write(cfg_path, new_content);
                    }
                }
            }
        }
    }

    let mut studio_targets = vec![
        paths.sandbox_dir.join("settings_config.json"),
        paths.sandbox_dir.join("scripts").join("settings_config.json"),
        paths.sandbox_dir.join("dist").join("settings_config.json"),
    ];
    studio_targets.dedup();
    for st_path in &studio_targets {
        if st_path.exists() {
            if let Ok(content) = std::fs::read_to_string(st_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(kb) = val.get_mut("keyboard").and_then(|v| v.as_object_mut()) {
                        kb.insert("default_mode".to_string(), serde_json::Value::from(mode));
                        if let Some(ref lm) = resolved_last {
                            kb.insert("last_mode".to_string(), serde_json::Value::from(lm.clone()));
                        }
                        if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                            let _ = std::fs::write(st_path, new_content);
                        }
                    }
                }
            }
        }
    }

    crate::service::invalidate_config_cache();
    broadcast_tray_update();
}

pub fn save_capslock_switch_mode(capslock_switch: &str) {
    let paths = crate::service::RuhiPaths::resolve();
    let mut flat_targets = vec![
        paths.config.clone(),
        paths.sandbox_dir.join("scripts").join("config.json"),
        paths.sandbox_dir.join("dist").join("config.json"),
    ];
    flat_targets.dedup();
    for cfg_path in &flat_targets {
        if cfg_path.exists() {
            if let Ok(content) = std::fs::read_to_string(cfg_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    val["capslock_switch"] = serde_json::Value::from(capslock_switch);
                    if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                        let _ = std::fs::write(cfg_path, new_content);
                    }
                }
            }
        }
    }

    let mut studio_targets = vec![
        paths.sandbox_dir.join("settings_config.json"),
        paths.sandbox_dir.join("scripts").join("settings_config.json"),
        paths.sandbox_dir.join("dist").join("settings_config.json"),
    ];
    studio_targets.dedup();
    for st_path in &studio_targets {
        if st_path.exists() {
            if let Ok(content) = std::fs::read_to_string(st_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(kb) = val.get_mut("keyboard").and_then(|v| v.as_object_mut()) {
                        kb.insert("capslock_switch".to_string(), serde_json::Value::from(capslock_switch));
                        if let Ok(new_content) = serde_json::to_string_pretty(&val) {
                            let _ = std::fs::write(st_path, new_content);
                        }
                    }
                }
            }
        }
    }

    crate::service::invalidate_config_cache();
    broadcast_tray_update();
}

/// 滑鼠左鍵點擊右下角狀態圖示（許／英／日）或輸入座狀態徽章時，直接輪流切換至下一個輸入模式
pub fn cycle_next_keyboard_mode_global(pt_opt: Option<POINT>) {
    let paths = crate::service::RuhiPaths::resolve();
    let cfg = crate::service::load_ruhi_config(&paths);
    let mut cycle_list: Vec<String> = if cfg.enabled_keyboard_modes.is_empty() {
        vec!["hsu".to_string(), "japanese".to_string()]
    } else {
        cfg.enabled_keyboard_modes
            .iter()
            .map(|m| ruhi::engine::RuhiEngine::normalize_keyboard_mode(m))
            .filter(|m| !m.is_empty())
            .collect()
    };
    if !cycle_list.iter().any(|m| m == "english") {
        cycle_list.push("english".to_string());
    }
    let cur_norm = ruhi::engine::RuhiEngine::normalize_keyboard_mode(&cfg.keyboard_mode);
    let next_mode = if let Some(pos) = cycle_list.iter().position(|m| m == &cur_norm) {
        cycle_list[(pos + 1) % cycle_list.len()].clone()
    } else {
        cycle_list.first().cloned().unwrap_or_else(|| "hsu".to_string())
    };
    save_keyboard_mode_with_last(&next_mode, Some(&cur_norm));
    show_mode_toast_on_current_thread(&next_mode, pt_opt);
}

/// 快速在最近使用的最後兩種輸入模式間對切（或依設定直通英數）
pub fn toggle_last_two_keyboard_modes_global(pt_opt: Option<POINT>) {
    let paths = crate::service::RuhiPaths::resolve();
    let cfg = crate::service::load_ruhi_config(&paths);
    let cur_norm = ruhi::engine::RuhiEngine::normalize_keyboard_mode(&cfg.keyboard_mode);
    let last_norm = ruhi::engine::RuhiEngine::normalize_keyboard_mode(&cfg.last_keyboard_mode);
    let target = if cfg.capslock_switch == "english_direct" {
        if cur_norm == "english" {
            if !last_norm.is_empty() && last_norm != "english" {
                last_norm
            } else {
                cfg.enabled_keyboard_modes
                    .first()
                    .map(|m| ruhi::engine::RuhiEngine::normalize_keyboard_mode(m))
                    .unwrap_or_else(|| "hsu".to_string())
            }
        } else {
            "english".to_string()
        }
    } else if !last_norm.is_empty() && last_norm != cur_norm {
        last_norm
    } else if cur_norm == "english" {
        cfg.enabled_keyboard_modes
            .first()
            .map(|m| ruhi::engine::RuhiEngine::normalize_keyboard_mode(m))
            .unwrap_or_else(|| "hsu".to_string())
    } else {
        "english".to_string()
    };
    save_keyboard_mode_with_last(&target, Some(&cur_norm));
    show_mode_toast_on_current_thread(&target, pt_opt);
}

fn show_mode_toast_on_current_thread(mode_code: &str, pt_opt: Option<POINT>) {
    unsafe {
        let mut pt = pt_opt.unwrap_or_default();
        if pt.x == 0 && pt.y == 0 {
            let _ = GetCursorPos(&mut pt);
        }
        let class_name = w!("RuhICandidateWindow");
        let mut child = HWND::default();
        for _ in 0..16 {
            if let Ok(found) = FindWindowExW(None, child, class_name, None) {
                if found.0.is_null() {
                    break;
                }
                if GetWindowThreadProcessId(found, None) == GetCurrentThreadId() {
                    let is_idle = get_candidate_state()
                        .read()
                        .map(|s| s.composition_text.is_empty() && s.candidates.is_empty())
                        .unwrap_or(true);
                    if is_idle {
                        let (badge, mode_name) = get_mode_badge_and_name(mode_code, false);
                        let toast = match current_effective_ui_lang() {
                            "en-US" => format!("RuhI Mode  ➔  [ HW  {} ] {}", badge, mode_name),
                            "ja-JP" => format!("RuhI 入力状態  ➔  【 半  {} 】 {}", badge, mode_name),
                            "ko-KR" => format!("RuhI 입력 상태  ➔  【 반  {} 】 {}", badge, mode_name),
                            _ => format!("如喜輸入狀態  ➔  【 半  {} 】 {}", badge, mode_name),
                        };
                        let (dpi_scale, opacity) = if let Ok(mut state) = get_candidate_state().write() {
                            state.keyboard_mode = mode_code.to_string();
                            state.is_caps_on = false;
                            state.array30_coach_hint = toast.clone();
                            state.is_array30_live_preview = false;
                            state.is_cand_selecting = false;
                            state.is_candidates_only = true;
                            state.candidates.clear();
                            state.composition_text.clear();
                            (state.dpi_scale, state.desk_opacity)
                        } else {
                            (1.0, 95)
                        };
                        let w = (((toast.chars().count() as f32 * 13.5 * dpi_scale) + (36.0 * dpi_scale))
                            .clamp(260.0 * dpi_scale, 900.0 * dpi_scale)) as i32;
                        let h = (32.0 * dpi_scale) as i32;
                        let (sl, st_top, sr, sb) = CandidateWindow::get_monitor_work_area(pt.x, pt.y);
                        let fx = (pt.x - w / 2).clamp(sl + 8, (sr - w - 8).max(sl + 8));
                        let fy = (pt.y - h - 18).clamp(st_top + 8, (sb - h - 8).max(st_top + 8));
                        apply_window_opacity(found, opacity);
                        let _ = SetWindowPos(
                            found,
                            HWND_TOPMOST,
                            fx,
                            fy,
                            w,
                            h,
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        let _ = InvalidateRect(found, None, true);
                        let _ = SetTimer(found, TIMER_HIDE_TOAST, 1200, None);
                    } else {
                        relayout_and_repaint_window(found);
                    }
                    break;
                }
                child = found;
            } else {
                break;
            }
        }
    }
}

pub fn toggle_ui_tuner_bar_global() {
    if let Ok(mut st) = get_candidate_state().write() {
        st.show_ui_tuner_bar = !st.show_ui_tuner_bar;
    }
    unsafe {
        let class_name = w!("RuhICandidateWindow");
        let mut child = HWND::default();
        for _ in 0..16 {
            if let Ok(found) = FindWindowExW(None, child, class_name, None) {
                if found.0.is_null() {
                    break;
                }
                if GetWindowThreadProcessId(found, None) == GetCurrentThreadId() {
                    relayout_and_repaint_window(found);
                }
                child = found;
            } else {
                break;
            }
        }
    }
}

pub unsafe fn show_mode_switch_menu(hwnd: HWND) {
    show_taskbar_ri_menu(None);
    if !hwnd.0.is_null() && IsWindow(hwnd).as_bool() {
        relayout_and_repaint_window(hwnd);
    }
}

/// 統一由右下角工作列 TSF 圖示（RI / 許 / 英 / 日）或輸入座喚起完整功能選單。
/// 自動於當前執行緒建立輕量無 WS_EX_NOACTIVATE 之宿主視窗承載 TrackPopupMenu，
/// 徹底解決跨程序 HWND 導致選單不彈出或點擊外部無法收合之問題。
pub unsafe fn show_taskbar_ri_menu(pt_opt: Option<POINT>) {
    let mut pt = pt_opt.unwrap_or_default();
    if pt.x == 0 && pt.y == 0 {
        if GetCursorPos(&mut pt).is_err() {
            return;
        }
    }
    let hw_caps = (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0;
    let paths = crate::service::RuhiPaths::resolve();
    let latest_cfg = crate::service::load_ruhi_config(&paths);
    let (cur_kb, enabled_modes, cur_fs, cur_cfs, cur_op, cur_mc, cur_tuner_bar) = if let Ok(mut s) = get_candidate_state().write() {
        s.is_caps_on = false;
        s.keyboard_mode = latest_cfg.keyboard_mode.clone();
        s.enabled_keyboard_modes = latest_cfg.enabled_keyboard_modes.clone();
        s.font_size = (latest_cfg.font_size as f32).clamp(16.0, 96.0);
        s.candidate_font_size = (latest_cfg.candidate_font_size as f32).clamp(14.0, 72.0);
        s.desk_opacity = latest_cfg.desk_opacity.clamp(20, 100);
        s.max_composition_chars = latest_cfg.max_composition_chars.clamp(8, 80);
        (
            s.keyboard_mode.to_lowercase(),
            s.enabled_keyboard_modes.clone(),
            s.font_size.round() as u32,
            s.candidate_font_size.round() as u32,
            s.desk_opacity,
            s.max_composition_chars,
            s.show_ui_tuner_bar,
        )
    } else {
        (
            latest_cfg.keyboard_mode.to_lowercase(),
            latest_cfg.enabled_keyboard_modes.clone(),
            latest_cfg.font_size,
            latest_cfg.candidate_font_size,
            latest_cfg.desk_opacity,
            latest_cfg.max_composition_chars,
            false,
        )
    };

    let instance = GetModuleHandleW(None).unwrap_or_default();
    let host_hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        w!("STATIC"),
        w!("RuhIPopupMenuHost"),
        WS_POPUP,
        pt.x,
        pt.y,
        1,
        1,
        None,
        None,
        instance,
        None,
    ).unwrap_or_default();

    let mut cur_thread_cw = HWND::default();
    {
        let class_name = w!("RuhICandidateWindow");
        let mut child = HWND::default();
        for _ in 0..16 {
            if let Ok(found) = FindWindowExW(None, child, class_name, None) {
                if found.0.is_null() {
                    break;
                }
                if GetWindowThreadProcessId(found, None) == GetCurrentThreadId() {
                    cur_thread_cw = found;
                    break;
                }
                child = found;
            } else {
                break;
            }
        }
    }

    let menu_owner = if !host_hwnd.0.is_null() { host_hwnd } else { cur_thread_cw };
    if menu_owner.0.is_null() {
        return;
    }

    if let Ok(hmenu) = CreatePopupMenu() {
        if !hmenu.0.is_null() {
            let menu_ui_lang = crate::service::resolve_effective_ui_lang(&latest_cfg.ui_lang);
            let items = get_mode_menu_items_i18n(menu_ui_lang);
            let _unused_items: &[(usize, &str, &str)] = &[
                (2001, "hsu", "許氏雙拼注音 (許)\tCtrl/Shift + CapsLock"),
                (2005, "japanese", "日文羅馬字輸入 (日)\tCtrl/Shift + CapsLock"),
                (2009, "korean", "韓文羅馬拼音輸入 (韓)\tCtrl/Shift + CapsLock"),
                (2004, "cangjie", "倉頡輸入法 (倉)\tCtrl/Shift + CapsLock"),
                (2007, "array30", "行列 30 輸入法 (行)\tCtrl/Shift + CapsLock"),
                (2002, "standard", "標準大千注音 (注)\tCtrl/Shift + CapsLock"),
                (2003, "et26", "倚天 26 鍵注音 (倚)\tCtrl/Shift + CapsLock"),
            ];
            for &(id, mode_code, label) in items {
                let is_active = cur_kb == mode_code
                    || (mode_code == "standard" && cur_kb == "daikien")
                    || (mode_code == "et26" && cur_kb == "eten26")
                    || (mode_code == "array30" && cur_kb == "ar30")
                    || (mode_code == "japanese" && (cur_kb == "jp" || cur_kb == "romaji" || cur_kb == "kana"))
                    || (mode_code == "korean" && (cur_kb == "ko" || cur_kb == "hangul" || cur_kb == "romaja"));
                let is_enabled_in_loop = enabled_modes.is_empty()
                    || enabled_modes.iter().any(|m| {
                        let m_low = m.to_lowercase();
                        m_low == mode_code
                            || (mode_code == "standard" && m_low == "daikien")
                            || (mode_code == "et26" && m_low == "eten26")
                            || (mode_code == "array30" && m_low == "ar30")
                            || (mode_code == "japanese" && (m_low == "jp" || m_low == "romaji" || m_low == "kana"))
                            || (mode_code == "korean" && (m_low == "ko" || m_low == "hangul" || m_low == "romaja"))
                    });
                if !is_enabled_in_loop && !is_active {
                    continue;
                }
                let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let flags = if is_active { MF_STRING | MF_CHECKED } else { MF_STRING };
                let _ = AppendMenuW(hmenu, flags, id as usize, PCWSTR(wide.as_ptr()));
            }
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());

            let is_en_active = matches!(cur_kb.as_str(), "english" | "en" | "latin" | "western" | "euro" | "sanskrit");
            let is_direct_en_caps = latest_cfg.capslock_switch == "english_direct";
            let en_label = match menu_ui_lang {
                "en-US" => if is_direct_en_caps { "Pan-European / Roman (EU)\tCapsLock" } else { "Pan-European / Roman (EU)\tCtrl/Shift + CapsLock" },
                "ja-JP" => if is_direct_en_caps { "汎欧州／ローマ字モード (欧)\tCapsLock" } else { "汎欧州／ローマ字モード (欧)\tCtrl/Shift + CapsLock" },
                "ko-KR" => if is_direct_en_caps { "범유럽 / 로마자 모드 (EU)\tCapsLock" } else { "범유럽 / 로마자 모드 (EU)\tCtrl/Shift + CapsLock" },
                _ => if is_direct_en_caps { "泛歐／羅馬字模式 (歐)\tCapsLock" } else { "泛歐／羅馬字模式 (歐)\tCtrl/Shift + CapsLock" },
            };
            let _unused_en_label = if is_direct_en_caps {
                "泛歐／羅馬字模式 (歐)\tCapsLock"
            } else {
                "泛歐／羅馬字模式 (歐)\tCtrl/Shift + CapsLock"
            };
            let en_wide: Vec<u16> = en_label.encode_utf16().chain(Some(0)).collect();
            let en_flags = if is_en_active { MF_STRING | MF_CHECKED } else { MF_STRING };
            let _ = AppendMenuW(hmenu, en_flags, 2006, PCWSTR(en_wide.as_ptr()));

            let (cur_badge, _) = get_mode_badge_and_name(&latest_cfg.keyboard_mode, false);
            let last_code = if latest_cfg.last_keyboard_mode.eq_ignore_ascii_case(&latest_cfg.keyboard_mode) {
                if is_en_active {
                    enabled_modes.first().map(|s| s.as_str()).unwrap_or("hsu")
                } else {
                    "english"
                }
            } else {
                latest_cfg.last_keyboard_mode.as_str()
            };
            let (last_badge, _) = get_mode_badge_and_name(last_code, false);

            if !is_direct_en_caps {
                let mru_label = match menu_ui_lang {
                    "en-US" => format!("Quick Toggle Last Two Modes ({} ⇄ {})\tCapsLock", cur_badge, last_badge),
                    "ja-JP" => format!("直近 2 モードの高速切替 ({} ⇄ {})\tCapsLock", cur_badge, last_badge),
                    "ko-KR" => format!("최근 두 입력 모드 빠른 전환 ({} ⇄ {})\tCapsLock", cur_badge, last_badge),
                    _ => format!("最後兩種輸入模式快速切換 ({} ⇄ {})\tCapsLock", cur_badge, last_badge),
                };
                let mru_wide: Vec<u16> = mru_label.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_STRING, 2008, PCWSTR(mru_wide.as_ptr()));
            }

            // CapsLock 按鍵功能快速設定子選單 (2011 / 2012)
            if let Ok(caps_cfg_menu) = CreatePopupMenu() {
                let (opt_direct_str, opt_mru_str) = match menu_ui_lang {
                    "en-US" => (
                        "Pan-European / Roman (EU)\tCapsLock direct to EU mode".to_string(),
                        format!("Quick Toggle Last Two Modes ({} ⇄ {})\tCapsLock toggle", cur_badge, last_badge),
                    ),
                    "ja-JP" => (
                        "汎欧州／ローマ字モード (欧)\tCapsLock 単押しで直行".to_string(),
                        format!("直近 2 モードの高速切替 ({} ⇄ {})\tCapsLock 単押しで切替", cur_badge, last_badge),
                    ),
                    "ko-KR" => (
                        "범유럽 / 로마자 모드 (EU)\tCapsLock 단독으로 직행".to_string(),
                        format!("최근 두 입력 모드 빠른 전환 ({} ⇄ {})\tCapsLock 단독 전환", cur_badge, last_badge),
                    ),
                    _ => (
                        "泛歐／羅馬字模式 (歐)\t單按 CapsLock 直通泛歐模式".to_string(),
                        format!("最後兩種輸入模式快速切換 ({} ⇄ {})\t單按 CapsLock 對切", cur_badge, last_badge),
                    ),
                };
                let opt_direct: Vec<u16> = opt_direct_str.encode_utf16().chain(Some(0)).collect();
                let opt_mru: Vec<u16> = opt_mru_str.encode_utf16().chain(Some(0)).collect();
                let _unused_opt_direct: Vec<u16> = "泛歐／羅馬字模式 (歐)\t單按 CapsLock 直通泛歐模式".encode_utf16().chain(Some(0)).collect();
                let _unused_opt_mru: Vec<u16> = format!("最後兩種輸入模式快速切換 ({} ⇄ {})\t單按 CapsLock 對切", cur_badge, last_badge).encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(
                    caps_cfg_menu,
                    if is_direct_en_caps { MF_STRING | MF_CHECKED } else { MF_STRING },
                    2011,
                    PCWSTR(opt_direct.as_ptr()),
                );
                let _ = AppendMenuW(
                    caps_cfg_menu,
                    if !is_direct_en_caps { MF_STRING | MF_CHECKED } else { MF_STRING },
                    2012,
                    PCWSTR(opt_mru.as_ptr()),
                );
                let sub_title = match menu_ui_lang {
                    "en-US" => if is_direct_en_caps { "CapsLock Behavior: Pan-European (EU)..." } else { "CapsLock Behavior: Quick Toggle Last Two Modes..." },
                    "ja-JP" => if is_direct_en_caps { "CapsLock 設定：汎欧州／ローマ字モード (欧)..." } else { "CapsLock 設定：直近 2 モードの高速切替..." },
                    "ko-KR" => if is_direct_en_caps { "CapsLock 설정: 범유럽 / 로마자 모드 (EU)..." } else { "CapsLock 설정: 최근 두 입력 모드 빠른 전환..." },
                    _ => if is_direct_en_caps { "CapsLock 按鍵設定：泛歐／羅馬字模式 (歐)..." } else { "CapsLock 按鍵設定：最後兩種輸入模式快速切換..." },
                };
                let _unused_sub_title = if is_direct_en_caps {
                    "CapsLock 按鍵設定：泛歐／羅馬字模式 (歐)..."
                } else {
                    "CapsLock 按鍵設定：最後兩種輸入模式快速切換..."
                };
                let sub_wide: Vec<u16> = sub_title.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_POPUP, caps_cfg_menu.0 as usize, PCWSTR(sub_wide.as_ptr()));
            }

            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());

            // ── 輸入座 UI 快速外觀調校子選單 ──
            let toggle_bar_str = match menu_ui_lang {
                "en-US" => "Show Inline Tuner Bar (Tune ▾)\tClick [-]/[+] to adjust",
                "ja-JP" => "入力座内の調整バーを表示 (調整 ▾)\t[-]/[+] で即時調整",
                "ko-KR" => "입력석 내장 조정 바 표시 (조정 ▾)\t[-]/[+] 클릭으로 조정",
                _ => "顯示輸入座內嵌調校列 (調校 ▾)\t直接點擊 [-]/[+] 調整",
            };
            let toggle_bar_lbl: Vec<u16> = toggle_bar_str.encode_utf16().chain(Some(0)).collect();
            let _ = AppendMenuW(
                hmenu,
                if cur_tuner_bar { MF_STRING | MF_CHECKED } else { MF_STRING },
                3001,
                PCWSTR(toggle_bar_lbl.as_ptr()),
            );

            // 1. 不透明度子選單 (3100 + pct)
            if let Ok(op_menu) = CreatePopupMenu() {
                for &pct in &[100u32, 95, 90, 85, 80, 70, 60, 50, 40] {
                    let lbl: Vec<u16> = format!("{} %", pct).encode_utf16().chain(Some(0)).collect();
                    let fl = if cur_op == pct { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let _ = AppendMenuW(op_menu, fl, (3100 + pct) as usize, PCWSTR(lbl.as_ptr()));
                }
                let op_title_str = match menu_ui_lang {
                    "en-US" => format!("Desk Opacity (Current {}%)...", cur_op),
                    "ja-JP" => format!("入力座の不透明度 (現在 {}%)...", cur_op),
                    "ko-KR" => format!("입력석 불투명도 (현재 {}%)...", cur_op),
                    _ => format!("輸入座不透明度 (目前 {}%)...", cur_op),
                };
                let op_title: Vec<u16> = op_title_str.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_POPUP, op_menu.0 as usize, PCWSTR(op_title.as_ptr()));
            }

            // 2. 輸入座字體大小子選單 (3300 + px)
            if let Ok(fs_menu) = CreatePopupMenu() {
                for &px in &[20u32, 24, 28, 32, 36, 40, 48, 56, 64, 72, 84, 96] {
                    let lbl: Vec<u16> = format!("{} px", px).encode_utf16().chain(Some(0)).collect();
                    let fl = if cur_fs == px { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let _ = AppendMenuW(fs_menu, fl, (3300 + px) as usize, PCWSTR(lbl.as_ptr()));
                }
                let fs_title_str = match menu_ui_lang {
                    "en-US" => format!("Desk Font Size (Current {} px)...", cur_fs),
                    "ja-JP" => format!("入力座の文字サイズ (現在 {} px)...", cur_fs),
                    "ko-KR" => format!("입력석 글꼴 크기 (현재 {} px)...", cur_fs),
                    _ => format!("輸入座字體大小 (目前 {} px)...", cur_fs),
                };
                let fs_title: Vec<u16> = fs_title_str.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_POPUP, fs_menu.0 as usize, PCWSTR(fs_title.as_ptr()));
            }

            // 3. 候選字體大小子選單 (3500 + px)
            if let Ok(cfs_menu) = CreatePopupMenu() {
                for &px in &[16u32, 18, 20, 22, 24, 28, 32, 36, 42, 48, 60, 72] {
                    let lbl: Vec<u16> = format!("{} px", px).encode_utf16().chain(Some(0)).collect();
                    let fl = if cur_cfs == px { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let _ = AppendMenuW(cfs_menu, fl, (3500 + px) as usize, PCWSTR(lbl.as_ptr()));
                }
                let cfs_title_str = match menu_ui_lang {
                    "en-US" => format!("Candidate Font Size (Current {} px)...", cur_cfs),
                    "ja-JP" => format!("候補の文字サイズ (現在 {} px)...", cur_cfs),
                    "ko-KR" => format!("후보 글꼴 크기 (현재 {} px)...", cur_cfs),
                    _ => format!("候選字體大小 (目前 {} px)...", cur_cfs),
                };
                let cfs_title: Vec<u16> = cfs_title_str.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_POPUP, cfs_menu.0 as usize, PCWSTR(cfs_title.as_ptr()));
            }

            // 4. 輸入座單行最大字數子選單 (3700 + chars)
            if let Ok(mc_menu) = CreatePopupMenu() {
                for &ch_cnt in &[12usize, 16, 20, 25, 30, 35, 40, 50, 60, 80] {
                    let lbl_str = match menu_ui_lang {
                        "en-US" => format!("{} CJK units per line (auto-wrap)", ch_cnt),
                        "ja-JP" => format!("1行 {} 字 (自動折り返し)", ch_cnt),
                        "ko-KR" => format!("1줄 {}자 (자동 줄바꿈)", ch_cnt),
                        _ => format!("單行 {} 字 (超過自動換行)", ch_cnt),
                    };
                    let lbl: Vec<u16> = lbl_str.encode_utf16().chain(Some(0)).collect();
                    let fl = if cur_mc == ch_cnt { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let _ = AppendMenuW(mc_menu, fl, 3700 + ch_cnt, PCWSTR(lbl.as_ptr()));
                }
                let mc_title_str = match menu_ui_lang {
                    "en-US" => format!("Max Width per Line (Current {})...", cur_mc),
                    "ja-JP" => format!("1行の最大文字数 (現在 {} 字)...", cur_mc),
                    "ko-KR" => format!("1줄 최대 글자 수 (현재 {}자)...", cur_mc),
                    _ => format!("輸入座單行最大字數 (目前 {} 字)...", cur_mc),
                };
                let mc_title: Vec<u16> = mc_title_str.encode_utf16().chain(Some(0)).collect();
                let _ = AppendMenuW(hmenu, MF_POPUP, mc_menu.0 as usize, PCWSTR(mc_title.as_ptr()));
            }

            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let (lbl_settings, _lbl_tuner, lbl_about) = get_context_menu_labels();
            let text_settings: Vec<u16> = lbl_settings.encode_utf16().chain(Some(0)).collect();
            let text_about: Vec<u16> = lbl_about.encode_utf16().chain(Some(0)).collect();
            let _ = AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(text_settings.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(hmenu, MF_STRING, 1002, PCWSTR(text_about.as_ptr()));

            let _ = SetForegroundWindow(menu_owner);
            let cmd = TrackPopupMenu(
                hmenu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                menu_owner,
                None,
            );
            let _ = PostMessageW(menu_owner, WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);

            let cmd_id = cmd.0 as usize;
            if cmd_id == 2008 {
                toggle_last_two_keyboard_modes_global(Some(pt));
                if hw_caps {
                    crate::service::trigger_synthetic_caps_toggle();
                }
                if !cur_thread_cw.0.is_null() {
                    let _ = InvalidateRect(cur_thread_cw, None, true);
                }
            } else if cmd_id == 2011 {
                save_capslock_switch_mode("english_direct");
            } else if cmd_id == 2012 {
                save_capslock_switch_mode("chinese_english");
            }
            let chosen_mode = match cmd_id {
                2001 => Some("hsu"),
                2002 => Some("standard"),
                2003 => Some("et26"),
                2004 => Some("cangjie"),
                2005 => Some("japanese"),
                2006 => Some("english"),
                2007 => Some("array30"),
                2009 => Some("korean"),
                _ => None,
            };
            if let Some(m) = chosen_mode {
                save_keyboard_mode(m);
                if hw_caps {
                    crate::service::trigger_synthetic_caps_toggle();
                }
                broadcast_tray_update();
                if !cur_thread_cw.0.is_null() {
                    let _ = InvalidateRect(cur_thread_cw, None, true);
                }
            } else if cmd_id == 3001 {
                toggle_ui_tuner_bar_global();
            } else if (3120..=3200).contains(&cmd_id) {
                let pct = (cmd_id - 3100) as u32;
                let mut vals = None;
                if let Ok(mut st) = get_candidate_state().write() {
                    st.desk_opacity = pct.clamp(20, 100);
                    vals = Some((st.font_size.round() as u32, st.candidate_font_size.round() as u32, st.desk_opacity, st.max_composition_chars));
                }
                if let Some((fs, cfs, op, mc)) = vals {
                    save_ui_appearance_to_configs(fs, cfs, op, mc);
                    if !cur_thread_cw.0.is_null() {
                        relayout_and_repaint_window(cur_thread_cw);
                    }
                }
            } else if (3316..=3396).contains(&cmd_id) {
                let px = (cmd_id - 3300) as u32;
                let mut vals = None;
                if let Ok(mut st) = get_candidate_state().write() {
                    st.font_size = (px as f32).clamp(16.0, 96.0);
                    vals = Some((st.font_size.round() as u32, st.candidate_font_size.round() as u32, st.desk_opacity, st.max_composition_chars));
                }
                if let Some((fs, cfs, op, mc)) = vals {
                    save_ui_appearance_to_configs(fs, cfs, op, mc);
                    if !cur_thread_cw.0.is_null() {
                        relayout_and_repaint_window(cur_thread_cw);
                    }
                }
            } else if (3514..=3572).contains(&cmd_id) {
                let px = (cmd_id - 3500) as u32;
                let mut vals = None;
                if let Ok(mut st) = get_candidate_state().write() {
                    st.candidate_font_size = (px as f32).clamp(14.0, 72.0);
                    vals = Some((st.font_size.round() as u32, st.candidate_font_size.round() as u32, st.desk_opacity, st.max_composition_chars));
                }
                if let Some((fs, cfs, op, mc)) = vals {
                    save_ui_appearance_to_configs(fs, cfs, op, mc);
                    if !cur_thread_cw.0.is_null() {
                        relayout_and_repaint_window(cur_thread_cw);
                    }
                }
            } else if (3708..=3780).contains(&cmd_id) {
                let mc_new = cmd_id - 3700;
                let mut vals = None;
                if let Ok(mut st) = get_candidate_state().write() {
                    st.max_composition_chars = mc_new.clamp(8, 80);
                    vals = Some((st.font_size.round() as u32, st.candidate_font_size.round() as u32, st.desk_opacity, st.max_composition_chars));
                }
                if let Some((fs, cfs, op, mc)) = vals {
                    save_ui_appearance_to_configs(fs, cfs, op, mc);
                    if !cur_thread_cw.0.is_null() {
                        relayout_and_repaint_window(cur_thread_cw);
                    }
                }
            } else if cmd_id == 1001 {
                launch_settings();
            } else if cmd_id == 1002 {
                show_about_dialog(cur_thread_cw);
            }
        }
    }

    if !host_hwnd.0.is_null() {
        let _ = DestroyWindow(host_hwnd);
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
            WM_TIMER => {
                if wparam.0 == TIMER_TRAY_SYNC {
                    update_tray_icon_for_hwnd(hwnd, false);
                    return LRESULT(0);
                } else if wparam.0 == TIMER_HIDE_TOAST {
                    let _ = KillTimer(hwnd, TIMER_HIDE_TOAST);
                    let should_hide = if let Ok(mut st) = get_candidate_state().write() {
                        if st.composition_text.is_empty() && st.candidates.is_empty() {
                            st.array30_coach_hint.clear();
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    };
                    if should_hide {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
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
                        let on_tune_toggle = pt.x >= s.ui_tune_toggle_rect.left && pt.x <= s.ui_tune_toggle_rect.right &&
                                             pt.y >= s.ui_tune_toggle_rect.top && pt.y <= s.ui_tune_toggle_rect.bottom;
                        let on_mode = pt.x >= s.mode_rect.left && pt.x <= s.mode_rect.right &&
                                      pt.y >= s.mode_rect.top && pt.y <= s.mode_rect.bottom;
                        let on_tune_btn = s.show_ui_tuner_bar && s.ui_tune_buttons.iter().any(|(r, _)| {
                            pt.x >= r.left && pt.x <= r.right && pt.y >= r.top && pt.y <= r.bottom
                        });
                        on_gear || on_tune_toggle || on_mode || on_tune_btn
                    } else { false };
                    if is_clickable {
                        let _ = SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default());
                        return LRESULT(1);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
                if delta != 0 {
                    let is_ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
                    let is_alt = (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
                    if is_alt {
                        apply_ui_tune_action(
                            hwnd,
                            if delta > 0 { UiTuneAction::OpacityUp } else { UiTuneAction::OpacityDown },
                        );
                        return LRESULT(0);
                    } else if is_ctrl {
                        apply_ui_tune_action(
                            hwnd,
                            if delta > 0 { UiTuneAction::UiScaleUp } else { UiTuneAction::UiScaleDown },
                        );
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONDOWN => {
                let mouse_x = (lparam.0 & 0xFFFF) as i16 as i32;
                let mouse_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let (is_gear, is_tune_toggle, is_mode, hit_tune_act) = if let Ok(s) = get_candidate_state().read() {
                    let g = mouse_x >= s.gear_rect.left && mouse_x <= s.gear_rect.right &&
                            mouse_y >= s.gear_rect.top && mouse_y <= s.gear_rect.bottom;
                    let tt = mouse_x >= s.ui_tune_toggle_rect.left && mouse_x <= s.ui_tune_toggle_rect.right &&
                             mouse_y >= s.ui_tune_toggle_rect.top && mouse_y <= s.ui_tune_toggle_rect.bottom;
                    let m = mouse_x >= s.mode_rect.left && mouse_x <= s.mode_rect.right &&
                            mouse_y >= s.mode_rect.top && mouse_y <= s.mode_rect.bottom;
                    let act = if s.show_ui_tuner_bar {
                        s.ui_tune_buttons.iter().find_map(|(r, a)| {
                            if mouse_x >= r.left && mouse_x <= r.right && mouse_y >= r.top && mouse_y <= r.bottom {
                                Some(*a)
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    };
                    (g, tt, m, act)
                } else { (false, false, false, None) };

                if let Some(act) = hit_tune_act {
                    apply_ui_tune_action(hwnd, act);
                    return LRESULT(0);
                }
                if is_tune_toggle {
                    if let Ok(mut st) = get_candidate_state().write() {
                        st.show_ui_tuner_bar = !st.show_ui_tuner_bar;
                    }
                    relayout_and_repaint_window(hwnd);
                    return LRESULT(0);
                }
                if is_gear {
                    launch_settings();
                    return LRESULT(0);
                }
                if is_mode {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    cycle_next_keyboard_mode_global(Some(pt));
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
                if msg == WM_SYNC_TRAY_MODE {
                    update_tray_icon_for_hwnd(hwnd, true);
                    return LRESULT(0);
                }
                if msg == WM_TRAYICON {
                    let event = (lparam.0 & 0xFFFF) as u32;
                    match event {
                        WM_LBUTTONDBLCLK => {
                            launch_settings();
                        }
                        WM_LBUTTONUP | WM_RBUTTONUP => {
                            let _ = SetForegroundWindow(hwnd);
                            show_mode_switch_menu(hwnd);
                        }
                        _ => {}
                    }
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_RBUTTONUP => {
                show_mode_switch_menu(hwnd);
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
