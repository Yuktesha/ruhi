//! RuhI (如喜) Windows 11 TSF 原生深淺色自適應「關於如喜」對話框模組
//!
//! - 依系統深淺色佈景主題及 `config.json` 設定自動切換深色（`#1E1E22`）／淺色（`#F7F5F0`）調色盤與 Windows 11 原生 DWM 深色標題列。
//! - 內嵌零外部相依之 RLE Alpha 圖資（`ruhi_icon_128x128.rle1` 與 `studio_logo_640x100.rle3`），依深淺色動態合成「A Solid GUI Studio」雙主題品牌標誌（深靛藍 `#2F318B`／柔和冰白 `#E8EEF8` ＋ `#00A2E9`／`#E40082`）。

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE,
    TRACKMOUSEEVENT, VK_ESCAPE, VK_RETURN, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{s, w, PCWSTR};

use crate::candidate_window::{current_effective_ui_lang, read_candidate_state, CandidateWindow};
use crate::service::{load_ruhi_config, RuhiPaths};

const STUDIO_LOGO_RLE3: &[u8] = include_bytes!("../../assets/studio_logo_640x100.rle3");
const RUHI_ICON_RLE1: &[u8] = include_bytes!("../../assets/ruhi_icon_128x128.rle1");

const STUDIO_LOGO_ASPECT: f32 = 6.4;
const STUDIO_INK_LIGHT: [u8; 3] = [0x2F, 0x31, 0x8B];
const STUDIO_INK_DARK: [u8; 3] = [0xE8, 0xEE, 0xF8];
const STUDIO_CYAN: [u8; 3] = [0x00, 0xA2, 0xE9];
const STUDIO_MAGENTA: [u8; 3] = [0xE4, 0x00, 0x82];
const RUHI_ORANGE: [u8; 3] = [0xEA, 0x5B, 0x24];

const BASE_CLIENT_W: f32 = 460.0;
const BASE_CLIENT_H: f32 = 308.0;
const DWMWA_USE_IMMERSIVE_DARK_MODE_19: u32 = 19;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

static ABOUT_OPEN_HWND: AtomicIsize = AtomicIsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AboutCopy {
    pub window_title: String,
    pub heading: String,
    pub subtitle: String,
    pub version_line: String,
    pub motto_title: String,
    pub motto_body: String,
    pub studio_label: String,
    pub footer_left: String,
    pub ok_label: String,
}

#[derive(Clone, Copy)]
struct ThemePalette {
    bg_main: COLORREF,
    bg_footer: COLORREF,
    divider: COLORREF,
    ink_primary: COLORREF,
    ink_secondary: COLORREF,
    ink_muted: COLORREF,
    accent: COLORREF,
    btn_bg: COLORREF,
    btn_bg_hover: COLORREF,
    btn_bg_pressed: COLORREF,
    btn_border: COLORREF,
    btn_text: COLORREF,
}

impl ThemePalette {
    fn for_theme(is_dark: bool) -> Self {
        if is_dark {
            Self {
                bg_main: COLORREF(0x00221E1E),       // #1E1E22
                bg_footer: COLORREF(0x001E1A1A),     // #1A1A1E
                divider: COLORREF(0x00423A3A),       // #3A3A42
                ink_primary: COLORREF(0x00ECE8E8),   // #E8E8EC
                ink_secondary: COLORREF(0x00B8B0B0), // #B0B0B8
                ink_muted: COLORREF(0x008A8080),     // #80808A
                accent: COLORREF(0x0099D334),        // #34D399
                btn_bg: COLORREF(0x00699605),        // #059669
                btn_bg_hover: COLORREF(0x0081B910),  // #10B981
                btn_bg_pressed: COLORREF(0x00577804),// #047857
                btn_border: COLORREF(0x0081B910),    // #10B981
                btn_text: COLORREF(0x00FFFFFF),      // #FFFFFF
            }
        } else {
            Self {
                bg_main: COLORREF(0x00F0F5F7),       // #F7F5F0
                bg_footer: COLORREF(0x00E5EDF0),     // #F0EDE5
                divider: COLORREF(0x00CDD7DD),       // #DDD7CD
                ink_primary: COLORREF(0x00211D1C),   // #1C1D21
                ink_secondary: COLORREF(0x00574D4A), // #4A4D57
                ink_muted: COLORREF(0x00786E6A),     // #6A6E78
                accent: COLORREF(0x00699605),        // #059669
                btn_bg: COLORREF(0x00699605),        // #059669
                btn_bg_hover: COLORREF(0x00577804),  // #047857
                btn_bg_pressed: COLORREF(0x00465F06),// #065F46
                btn_border: COLORREF(0x00577804),    // #047857
                btn_text: COLORREF(0x00FFFFFF),      // #FFFFFF
            }
        }
    }
}

struct AboutDialogState {
    is_dark: bool,
    dpi_scale: f32,
    copy: AboutCopy,
    ok_rect: RECT,
    ok_hovered: bool,
    ok_pressed: bool,
    tracking_mouse: bool,
    closed: bool,
    ruhi_icon_premul: Option<(u32, u32, Vec<u8>)>,
    studio_logo_light_premul: Option<(u32, u32, Vec<u8>)>,
    studio_logo_dark_premul: Option<(u32, u32, Vec<u8>)>,
}

/// 解碼 `RLGO` 緊湊 RLE Alpha 二進位資源
pub(crate) fn decode_rlgo(data: &[u8]) -> Option<(u32, u32, Vec<Vec<u8>>)> {
    if data.len() < 10 || &data[0..4] != b"RLGO" {
        return None;
    }
    let width = u16::from_le_bytes([data[4], data[5]]) as u32;
    let height = u16::from_le_bytes([data[6], data[7]]) as u32;
    let layer_count = data[8] as usize;
    if width == 0 || height == 0 || layer_count == 0 {
        return None;
    }
    let expected = (width as usize).checked_mul(height as usize)?;
    let mut cursor = 10usize;
    let mut layers = Vec::with_capacity(layer_count);

    for _ in 0..layer_count {
        let mut alphas = Vec::with_capacity(expected);
        while alphas.len() < expected {
            if cursor + 1 >= data.len() {
                return None;
            }
            let run_len = data[cursor] as usize;
            let alpha = data[cursor + 1];
            cursor += 2;
            if run_len == 0 || alphas.len() + run_len > expected {
                return None;
            }
            alphas.resize(alphas.len() + run_len, alpha);
        }
        layers.push(alphas);
    }
    Some((width, height, layers))
}

/// 以 Porter-Duff `src-over` 合成預乘 RGBA8 影像（無灰邊暈瑕疵）
fn composite_premul_layers(width: u32, height: u32, layers: &[(&[u8], [u8; 3])]) -> Option<Vec<u8>> {
    let pixel_count = (width as usize).checked_mul(height as usize)?;
    let mut out = vec![0u8; pixel_count * 4];
    for &(alphas, rgb) in layers {
        if alphas.len() != pixel_count {
            return None;
        }
        let (r, g, b) = (rgb[0] as u32, rgb[1] as u32, rgb[2] as u32);
        for i in 0..pixel_count {
            let sa = alphas[i] as u32;
            if sa == 0 {
                continue;
            }
            let off = i * 4;
            let sr = (r * sa + 127) / 255;
            let sg = (g * sa + 127) / 255;
            let sb = (b * sa + 127) / 255;
            let inv = 255 - sa;
            out[off] = (sr + (out[off] as u32 * inv + 127) / 255).min(255) as u8;
            out[off + 1] = (sg + (out[off + 1] as u32 * inv + 127) / 255).min(255) as u8;
            out[off + 2] = (sb + (out[off + 2] as u32 * inv + 127) / 255).min(255) as u8;
            out[off + 3] = (sa + (out[off + 3] as u32 * inv + 127) / 255).min(255) as u8;
        }
    }
    Some(out)
}

/// 產生深／淺色「A Solid GUI Studio」雙主題預乘 RGBA8 紋理 (`640x100`)
pub(crate) fn render_studio_logo_premul_rgba(is_dark: bool) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, layers) = decode_rlgo(STUDIO_LOGO_RLE3)?;
    if layers.len() != 3 {
        return None;
    }
    let ink = if is_dark {
        STUDIO_INK_DARK
    } else {
        STUDIO_INK_LIGHT
    };
    let rgba = composite_premul_layers(
        w,
        h,
        &[
            (&layers[0], STUDIO_CYAN),
            (&layers[1], STUDIO_MAGENTA),
            (&layers[2], ink),
        ],
    )?;
    Some((w, h, rgba))
}

/// 產生如喜 `#EA5B24` 品牌圖示預乘 RGBA8 紋理 (`128x128`)
pub(crate) fn render_ruhi_icon_premul_rgba() -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, layers) = decode_rlgo(RUHI_ICON_RLE1)?;
    if layers.is_empty() {
        return None;
    }
    let rgba = composite_premul_layers(w, h, &[(&layers[0], RUHI_ORANGE)])?;
    Some((w, h, rgba))
}

/// 以雙線性取樣將預乘 RGBA8 影像 Alpha 混色繪製至 32-bit Top-Down BGRA DIB
pub(crate) fn blit_premul_rgba_over_bgra_dib(
    dib_bgra: &mut [u8],
    dib_w: i32,
    dib_h: i32,
    src_rgba: &[u8],
    src_w: u32,
    src_h: u32,
    dst_x: i32,
    dst_y: i32,
    dst_w: i32,
    dst_h: i32,
) {
    if dib_w <= 0 || dib_h <= 0 || src_w == 0 || src_h == 0 || dst_w <= 0 || dst_h <= 0 {
        return;
    }
    let expected_dib = (dib_w as usize) * (dib_h as usize) * 4;
    let expected_src = (src_w as usize) * (src_h as usize) * 4;
    if dib_bgra.len() < expected_dib || src_rgba.len() < expected_src {
        return;
    }

    let x_start = dst_x.max(0);
    let y_start = dst_y.max(0);
    let x_end = (dst_x + dst_w).min(dib_w);
    let y_end = (dst_y + dst_h).min(dib_h);
    let sw = src_w as i32;
    let sh = src_h as i32;

    for dy in y_start..y_end {
        let sy_f = ((dy - dst_y) as f32 + 0.5) * (src_h as f32) / (dst_h as f32) - 0.5;
        let sy_clamped = sy_f.clamp(0.0, (sh - 1) as f32);
        let y0 = sy_clamped.floor() as i32;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = sy_clamped - (y0 as f32);
        let wy0 = 1.0 - wy;

        for dx in x_start..x_end {
            let sx_f = ((dx - dst_x) as f32 + 0.5) * (src_w as f32) / (dst_w as f32) - 0.5;
            let sx_clamped = sx_f.clamp(0.0, (sw - 1) as f32);
            let x0 = sx_clamped.floor() as i32;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = sx_clamped - (x0 as f32);
            let wx0 = 1.0 - wx;

            let i00 = ((y0 * sw + x0) * 4) as usize;
            let i10 = ((y0 * sw + x1) * 4) as usize;
            let i01 = ((y1 * sw + x0) * 4) as usize;
            let i11 = ((y1 * sw + x1) * 4) as usize;

            let w00 = wx0 * wy0;
            let w10 = wx * wy0;
            let w01 = wx0 * wy;
            let w11 = wx * wy;

            let sa = src_rgba[i00 + 3] as f32 * w00
                + src_rgba[i10 + 3] as f32 * w10
                + src_rgba[i01 + 3] as f32 * w01
                + src_rgba[i11 + 3] as f32 * w11;
            if sa <= 0.5 {
                continue;
            }
            let sr = src_rgba[i00] as f32 * w00
                + src_rgba[i10] as f32 * w10
                + src_rgba[i01] as f32 * w01
                + src_rgba[i11] as f32 * w11;
            let sg = src_rgba[i00 + 1] as f32 * w00
                + src_rgba[i10 + 1] as f32 * w10
                + src_rgba[i01 + 1] as f32 * w01
                + src_rgba[i11 + 1] as f32 * w11;
            let sb = src_rgba[i00 + 2] as f32 * w00
                + src_rgba[i10 + 2] as f32 * w10
                + src_rgba[i01 + 2] as f32 * w01
                + src_rgba[i11 + 2] as f32 * w11;

            let inv = (255.0 - sa.min(255.0)) / 255.0;
            let doff = ((dy * dib_w + dx) * 4) as usize;
            let out_b = (sb + dib_bgra[doff] as f32 * inv).round().clamp(0.0, 255.0) as u8;
            let out_g = (sg + dib_bgra[doff + 1] as f32 * inv)
                .round()
                .clamp(0.0, 255.0) as u8;
            let out_r = (sr + dib_bgra[doff + 2] as f32 * inv)
                .round()
                .clamp(0.0, 255.0) as u8;

            dib_bgra[doff] = out_b;
            dib_bgra[doff + 1] = out_g;
            dib_bgra[doff + 2] = out_r;
            dib_bgra[doff + 3] = 255;
        }
    }
}

/// 依語系與輸入模式建置多語系「關於如喜」文案
pub(crate) fn build_about_copy(
    ui_lang: &str,
    kb_mode: &str,
    is_caps: bool,
    version: &str,
) -> AboutCopy {
    let kb = kb_mode.to_ascii_lowercase();
    if ui_lang == "en-US" || is_caps || matches!(kb.as_str(), "english" | "latin" | "western") {
        AboutCopy {
            window_title: "About RuhI".to_string(),
            heading: "RuhI Universal Smart IME".to_string(),
            subtitle: "Responsive Universal Human Input".to_string(),
            version_line: format!("Version: {version} (Release)"),
            motto_title: "Flowing like clouds and water, effortless and joyful.".to_string(),
            motto_body: "Sanskrit root Rūhi — to sprout, grow, and evolve alongside your daily writing."
                .to_string(),
            studio_label: "Studio".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "OK".to_string(),
        }
    } else if matches!(kb.as_str(), "japanese" | "kana" | "romaji") || ui_lang == "ja-JP" {
        AboutCopy {
            window_title: "RuhI について".to_string(),
            heading: "RuhI 如喜スマートIME".to_string(),
            subtitle: "Responsive Universal Human Input".to_string(),
            version_line: format!("バージョン: {version} (Release)"),
            motto_title: "雲の行くが如く水の流るるが如く、自在にして歓喜。".to_string(),
            motto_body: "思考の流れに沿って自然に言葉を紡ぎ、日々の入力とともに成長する日本語・多言語IME。"
                .to_string(),
            studio_label: "Studio".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "OK".to_string(),
        }
    } else if matches!(kb.as_str(), "korean" | "ko" | "hangul" | "romaja") || ui_lang == "ko-KR"
    {
        AboutCopy {
            window_title: "RuhI 정보".to_string(),
            heading: "RuhI 루히 유니버설 스마트 입력기".to_string(),
            subtitle: "Responsive Universal Human Input".to_string(),
            version_line: format!("버전: {version} (Release)"),
            motto_title: "구름 가듯 물 흐르듯 자유롭고 기쁘게.".to_string(),
            motto_body: "산스크리트 어근 Rūhi(싹트고 자람)의 뜻처럼 사용자의 문맥과 함께 성장합니다."
                .to_string(),
            studio_label: "Studio".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "확인".to_string(),
        }
    } else if matches!(kb.as_str(), "iast" | "sanskrit") {
        AboutCopy {
            window_title: "About RuhI (IAST)".to_string(),
            heading: "RuhI Universal Smart IME".to_string(),
            subtitle: "Sanskrit root: Rūhi — to sprout, to grow".to_string(),
            version_line: format!("Version: {version} (Release)"),
            motto_title: "Yathā meghāḥ calanti yathā vāri vahati, svacchandataḥ ānandaḥ."
                .to_string(),
            motto_body: "如雲行水流自在歡喜 · Flowing like clouds and water, effortless and joyful."
                .to_string(),
            studio_label: "Studio".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "OK".to_string(),
        }
    } else if kb == "thai" {
        AboutCopy {
            window_title: "เกี่ยวกับ RuhI".to_string(),
            heading: "RuhI Universal Smart IME".to_string(),
            subtitle: "Responsive Universal Human Input".to_string(),
            version_line: format!("เวอร์ชัน: {version} (Release)"),
            motto_title: "ดั่งเมฆเคลื่อน ดั่งสายน้ำไหล สบายใจและเปี่ยมสุข".to_string(),
            motto_body: "พิมพ์ลื่นไหลเป็นธรรมชาติและเติบโตตามภาษาการเขียนของคุณ".to_string(),
            studio_label: "Studio".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "ตกลง".to_string(),
        }
    } else {
        AboutCopy {
            window_title: "關於如喜".to_string(),
            heading: "RuhI 如喜通用智慧型輸入法".to_string(),
            subtitle: "Responsive Universal Human Input".to_string(),
            version_line: format!("版本：{version} (Release)"),
            motto_title: "如雲行水流自在歡喜".to_string(),
            motto_body: "思緒如行雲流水般暢行無阻，選字貼合心意而自在歡喜。".to_string(),
            studio_label: "出品團隊 (Studio)".to_string(),
            footer_left: "100% Pure Rust · Apache-2.0".to_string(),
            ok_label: "確定".to_string(),
        }
    }
}

/// 依 `config.json` 與 Windows 系統個人化色彩決定是否採用深色主題
pub(crate) fn resolve_about_dark_theme() -> bool {
    let paths = RuhiPaths::resolve();
    let cfg = load_ruhi_config(&paths);
    CandidateWindow::resolve_dark_theme(&cfg.theme)
}

unsafe fn apply_dwm_window_theme(hwnd: HWND, is_dark: bool) {
    type PfnDwmSetWindowAttribute = unsafe extern "system" fn(
        hwnd: HWND,
        dw_attribute: u32,
        pv_attribute: *const c_void,
        cb_attribute: u32,
    ) -> windows::core::HRESULT;

    let mut dwmapi = GetModuleHandleW(w!("dwmapi.dll")).unwrap_or_default();
    if dwmapi.0.is_null() {
        dwmapi = LoadLibraryW(w!("dwmapi.dll")).unwrap_or_default();
    }
    if dwmapi.0.is_null() {
        return;
    }
    if let Some(raw_fn) = GetProcAddress(dwmapi, s!("DwmSetWindowAttribute")) {
        let dwm_set_attr: PfnDwmSetWindowAttribute = std::mem::transmute(raw_fn);
        let dark_val = BOOL::from(is_dark);
        let _ = dwm_set_attr(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_val as *const _ as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let _ = dwm_set_attr(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE_19,
            &dark_val as *const _ as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = dwm_set_attr(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

unsafe fn create_hicon_from_premul_rgba(width: u32, height: u32, src_rgba: &[u8]) -> Option<HICON> {
    if width == 0 || height == 0 {
        return None;
    }
    let dst_size = 32i32;
    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(screen_dc);
    let _ = ReleaseDC(None, screen_dc);
    if mem_dc.0.is_null() {
        return None;
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = dst_size;
    bmi.bmiHeader.biHeight = -dst_size;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;

    let mut bits_ptr: *mut c_void = std::ptr::null_mut();
    let color_bmp = match CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) {
        Ok(b) if !b.0.is_null() && !bits_ptr.is_null() => b,
        _ => {
            let _ = DeleteDC(mem_dc);
            return None;
        }
    };
    let dib_len = (dst_size as usize) * (dst_size as usize) * 4;
    let dib_slice = std::slice::from_raw_parts_mut(bits_ptr as *mut u8, dib_len);
    dib_slice.fill(0);

    // 直接將雙線性取樣後之 (pr, pg, pb, pa) 寫為 BGRA32 透明圖示
    let sw = width as i32;
    let sh = height as i32;
    for dy in 0..dst_size {
        let sy_f = ((dy as f32 + 0.5) * (height as f32) / (dst_size as f32) - 0.5)
            .clamp(0.0, (sh - 1) as f32);
        let y0 = sy_f.floor() as i32;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = sy_f - (y0 as f32);
        let wy0 = 1.0 - wy;
        for dx in 0..dst_size {
            let sx_f = ((dx as f32 + 0.5) * (width as f32) / (dst_size as f32) - 0.5)
                .clamp(0.0, (sw - 1) as f32);
            let x0 = sx_f.floor() as i32;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = sx_f - (x0 as f32);
            let wx0 = 1.0 - wx;
            let i00 = ((y0 * sw + x0) * 4) as usize;
            let i10 = ((y0 * sw + x1) * 4) as usize;
            let i01 = ((y1 * sw + x0) * 4) as usize;
            let i11 = ((y1 * sw + x1) * 4) as usize;
            let w00 = wx0 * wy0;
            let w10 = wx * wy0;
            let w01 = wx0 * wy;
            let w11 = wx * wy;
            let off = ((dy * dst_size + dx) * 4) as usize;
            dib_slice[off] = (src_rgba[i00 + 2] as f32 * w00
                + src_rgba[i10 + 2] as f32 * w10
                + src_rgba[i01 + 2] as f32 * w01
                + src_rgba[i11 + 2] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            dib_slice[off + 1] = (src_rgba[i00 + 1] as f32 * w00
                + src_rgba[i10 + 1] as f32 * w10
                + src_rgba[i01 + 1] as f32 * w01
                + src_rgba[i11 + 1] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            dib_slice[off + 2] = (src_rgba[i00] as f32 * w00
                + src_rgba[i10] as f32 * w10
                + src_rgba[i01] as f32 * w01
                + src_rgba[i11] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            dib_slice[off + 3] = (src_rgba[i00 + 3] as f32 * w00
                + src_rgba[i10 + 3] as f32 * w10
                + src_rgba[i01 + 3] as f32 * w01
                + src_rgba[i11 + 3] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }

    let mask_bmp = CreateBitmap(dst_size, dst_size, 1, 1, None);
    let _ = DeleteDC(mem_dc);
    if mask_bmp.0.is_null() {
        let _ = DeleteObject(color_bmp);
        return None;
    }

    let ii = ICONINFO {
        fIcon: TRUE,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bmp,
        hbmColor: color_bmp,
    };
    let hicon = CreateIconIndirect(&ii).ok();
    let _ = DeleteObject(color_bmp);
    let _ = DeleteObject(mask_bmp);
    hicon
}

unsafe fn create_ui_font(px_height: i32, weight: i32) -> HFONT {
    let face: Vec<u16> = "Microsoft JhengHei UI".encode_utf16().chain(Some(0)).collect();
    CreateFontW(
        -px_height.max(10),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        PCWSTR(face.as_ptr()),
    )
}

unsafe fn draw_wide_text(
    hdc: HDC,
    font: HFONT,
    color: COLORREF,
    rect: RECT,
    text: &str,
    flags: DRAW_TEXT_FORMAT,
) {
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, color);
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rc = rect;
    DrawTextW(hdc, &mut wide, &mut rc, flags);
    SelectObject(hdc, old_font);
}

unsafe fn paint_about_dialog(hwnd: HWND, hdc: HDC, state: &mut AboutDialogState) {
    let mut client_rect = RECT::default();
    if GetClientRect(hwnd, &mut client_rect).is_err() {
        return;
    }
    let width = (client_rect.right - client_rect.left).max(1);
    let height = (client_rect.bottom - client_rect.top).max(1);

    let mem_dc = CreateCompatibleDC(hdc);
    if mem_dc.0.is_null() {
        return;
    }
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0;

    let mut bits_ptr: *mut c_void = std::ptr::null_mut();
    let mem_bmp = match CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) {
        Ok(b) if !b.0.is_null() && !bits_ptr.is_null() => b,
        _ => {
            let _ = DeleteDC(mem_dc);
            return;
        }
    };
    let old_bmp = SelectObject(mem_dc, mem_bmp);
    let palette = ThemePalette::for_theme(state.is_dark);
    let s = state.dpi_scale;
    let sc = |v: f32| -> i32 { (v * s).round() as i32 };

    let footer_top = sc(248.0).clamp(0, height);
    let top_rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: footer_top,
    };
    let footer_rect = RECT {
        left: 0,
        top: footer_top,
        right: width,
        bottom: height,
    };

    let main_brush = CreateSolidBrush(palette.bg_main);
    let footer_brush = CreateSolidBrush(palette.bg_footer);
    let div_brush = CreateSolidBrush(palette.divider);
    FillRect(mem_dc, &top_rect, main_brush);
    FillRect(mem_dc, &footer_rect, footer_brush);
    FillRect(
        mem_dc,
        &RECT {
            left: sc(24.0),
            top: sc(104.0),
            right: width - sc(24.0),
            bottom: sc(104.0) + sc(1.0).max(1),
        },
        div_brush,
    );
    FillRect(
        mem_dc,
        &RECT {
            left: 0,
            top: footer_top,
            right: width,
            bottom: footer_top + sc(1.0).max(1),
        },
        div_brush,
    );
    let _ = DeleteObject(main_brush);
    let _ = DeleteObject(footer_brush);
    let _ = DeleteObject(div_brush);

    // 按鈕幾何與圓角填充
    let ok_rect = RECT {
        left: width - sc(124.0),
        top: footer_top + sc(13.0),
        right: width - sc(24.0),
        bottom: footer_top + sc(47.0),
    };
    state.ok_rect = ok_rect;
    let btn_color = if state.ok_pressed {
        palette.btn_bg_pressed
    } else if state.ok_hovered {
        palette.btn_bg_hover
    } else {
        palette.btn_bg
    };
    let btn_brush = CreateSolidBrush(btn_color);
    let btn_pen = CreatePen(PS_SOLID, 1, palette.btn_border);
    let old_brush = SelectObject(mem_dc, btn_brush);
    let old_pen = SelectObject(mem_dc, btn_pen);
    let radius = sc(10.0).max(6);
    let _ = RoundRect(
        mem_dc,
        ok_rect.left,
        ok_rect.top,
        ok_rect.right,
        ok_rect.bottom,
        radius,
        radius,
    );
    SelectObject(mem_dc, old_brush);
    SelectObject(mem_dc, old_pen);
    let _ = DeleteObject(btn_brush);
    let _ = DeleteObject(btn_pen);

    let _ = GdiFlush();

    // 直接對 DIB 進行抗鋸齒透明疊圖（如喜圖示 + A Solid GUI Studio 向量標誌）
    let dib_len = (width as usize) * (height as usize) * 4;
    let dib_slice = std::slice::from_raw_parts_mut(bits_ptr as *mut u8, dib_len);
    if let Some((iw, ih, ref rgba)) = state.ruhi_icon_premul {
        blit_premul_rgba_over_bgra_dib(
            dib_slice,
            width,
            height,
            rgba,
            iw,
            ih,
            sc(24.0),
            sc(22.0),
            sc(56.0),
            sc(56.0),
        );
    }
    let logo_h = sc(28.0).max(16);
    let logo_w = ((logo_h as f32) * STUDIO_LOGO_ASPECT).round() as i32;
    let studio_tex = if state.is_dark {
        state.studio_logo_dark_premul.as_ref()
    } else {
        state.studio_logo_light_premul.as_ref()
    };
    if let Some((lw, lh, ref rgba)) = studio_tex {
        blit_premul_rgba_over_bgra_dib(
            dib_slice,
            width,
            height,
            rgba,
            *lw,
            *lh,
            sc(24.0),
            sc(206.0),
            logo_w,
            logo_h,
        );
    }

    // 繪製清晰 ClearType 文字
    SetBkMode(mem_dc, TRANSPARENT);
    let font_heading = create_ui_font(sc(20.0), FW_BOLD.0 as i32);
    let font_title_sm = create_ui_font(sc(15.5), FW_SEMIBOLD.0 as i32);
    let font_body = create_ui_font(sc(14.0), FW_NORMAL.0 as i32);
    let font_ver = create_ui_font(sc(13.5), FW_SEMIBOLD.0 as i32);
    let font_muted = create_ui_font(sc(12.5), FW_NORMAL.0 as i32);

    draw_wide_text(
        mem_dc,
        font_heading,
        palette.ink_primary,
        RECT {
            left: sc(96.0),
            top: sc(18.0),
            right: width - sc(20.0),
            bottom: sc(46.0),
        },
        &state.copy.heading,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    draw_wide_text(
        mem_dc,
        font_body,
        palette.ink_secondary,
        RECT {
            left: sc(96.0),
            top: sc(46.0),
            right: width - sc(20.0),
            bottom: sc(66.0),
        },
        &state.copy.subtitle,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    draw_wide_text(
        mem_dc,
        font_ver,
        palette.accent,
        RECT {
            left: sc(96.0),
            top: sc(68.0),
            right: width - sc(20.0),
            bottom: sc(88.0),
        },
        &state.copy.version_line,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );

    draw_wide_text(
        mem_dc,
        font_title_sm,
        palette.ink_primary,
        RECT {
            left: sc(24.0),
            top: sc(116.0),
            right: width - sc(24.0),
            bottom: sc(140.0),
        },
        &state.copy.motto_title,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    draw_wide_text(
        mem_dc,
        font_body,
        palette.ink_secondary,
        RECT {
            left: sc(24.0),
            top: sc(142.0),
            right: width - sc(24.0),
            bottom: sc(178.0),
        },
        &state.copy.motto_body,
        DT_LEFT | DT_WORDBREAK,
    );

    draw_wide_text(
        mem_dc,
        font_muted,
        palette.ink_muted,
        RECT {
            left: sc(24.0),
            top: sc(184.0),
            right: width - sc(24.0),
            bottom: sc(202.0),
        },
        &state.copy.studio_label,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    draw_wide_text(
        mem_dc,
        font_body,
        palette.ink_muted,
        RECT {
            left: sc(24.0) + logo_w + sc(10.0),
            top: sc(208.0),
            right: width - sc(20.0),
            bottom: sc(206.0) + logo_h,
        },
        "(Yuktesha Zer)",
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );

    draw_wide_text(
        mem_dc,
        font_muted,
        palette.ink_muted,
        RECT {
            left: sc(24.0),
            top: footer_top + sc(16.0),
            right: ok_rect.left - sc(12.0),
            bottom: footer_top + sc(44.0),
        },
        &state.copy.footer_left,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    draw_wide_text(
        mem_dc,
        font_ver,
        palette.btn_text,
        ok_rect,
        &state.copy.ok_label,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER,
    );

    let _ = DeleteObject(font_heading);
    let _ = DeleteObject(font_title_sm);
    let _ = DeleteObject(font_body);
    let _ = DeleteObject(font_ver);
    let _ = DeleteObject(font_muted);

    let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(mem_bmp);
    let _ = DeleteDC(mem_dc);
}

#[inline]
fn pt_in_rect(rc: &RECT, x: i32, y: i32) -> bool {
    x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom
}

#[inline]
fn mouse_coords(lparam: LPARAM) -> (i32, i32) {
    let raw = lparam.0 as u32;
    ((raw & 0xFFFF) as i16 as i32, ((raw >> 16) & 0xFFFF) as i16 as i32)
}

unsafe extern "system" fn about_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let res = catch_unwind(AssertUnwindSafe(|| {
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutDialogState;
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if !hdc.0.is_null() {
                    if let Some(state) = state_ptr.as_mut() {
                        paint_about_dialog(hwnd, hdc, state);
                    }
                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(state) = state_ptr.as_mut() {
                    let (x, y) = mouse_coords(lparam);
                    let hovered = pt_in_rect(&state.ok_rect, x, y);
                    if hovered != state.ok_hovered {
                        state.ok_hovered = hovered;
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                    if !state.tracking_mouse {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        if TrackMouseEvent(&mut tme).is_ok() {
                            state.tracking_mouse = true;
                        }
                    }
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                if let Some(state) = state_ptr.as_mut() {
                    state.tracking_mouse = false;
                    if state.ok_hovered {
                        state.ok_hovered = false;
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(state) = state_ptr.as_mut() {
                    let (x, y) = mouse_coords(lparam);
                    if pt_in_rect(&state.ok_rect, x, y) {
                        state.ok_pressed = true;
                        let _ = SetCapture(hwnd);
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(state) = state_ptr.as_mut() {
                    if state.ok_pressed {
                        state.ok_pressed = false;
                        let _ = ReleaseCapture();
                        let (x, y) = mouse_coords(lparam);
                        let _ = InvalidateRect(hwnd, None, FALSE);
                        if pt_in_rect(&state.ok_rect, x, y) {
                            state.closed = true;
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_ESCAPE.0 || vk == VK_RETURN.0 || vk == VK_SPACE.0 {
                    if let Some(state) = state_ptr.as_mut() {
                        state.closed = true;
                    }
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SETTINGCHANGE => {
                if let Some(state) = state_ptr.as_mut() {
                    let new_dark = resolve_about_dark_theme();
                    if new_dark != state.is_dark {
                        state.is_dark = new_dark;
                        apply_dwm_window_theme(hwnd, new_dark);
                        let _ = InvalidateRect(hwnd, None, FALSE);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                if let Some(state) = state_ptr.as_mut() {
                    state.closed = true;
                }
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                if let Some(state) = state_ptr.as_mut() {
                    state.closed = true;
                }
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }));
    res.unwrap_or_else(|_| DefWindowProcW(hwnd, msg, wparam, lparam))
}

unsafe fn compute_centered_window_rect(
    owner_hwnd: HWND,
    client_w: i32,
    client_h: i32,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
) -> (i32, i32, i32, i32) {
    let mut adj = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    let _ = AdjustWindowRectEx(&mut adj, style, FALSE, ex_style);
    let win_w = adj.right - adj.left;
    let win_h = adj.bottom - adj.top;

    let mut work = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    let mut anchor_rect = RECT::default();
    let has_visible_owner = !owner_hwnd.0.is_null()
        && IsWindow(owner_hwnd).as_bool()
        && IsWindowVisible(owner_hwnd).as_bool()
        && GetWindowRect(owner_hwnd, &mut anchor_rect).is_ok();

    let hmon = if has_visible_owner {
        MonitorFromWindow(owner_hwnd, MONITOR_DEFAULTTONEAREST)
    } else {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
    };
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(hmon, &mut mi).as_bool() {
        work = mi.rcWork;
    }

    let (cx, cy) = if has_visible_owner {
        (
            (anchor_rect.left + anchor_rect.right) / 2,
            (anchor_rect.top + anchor_rect.bottom) / 2,
        )
    } else {
        ((work.left + work.right) / 2, (work.top + work.bottom) / 2)
    };
    let x = (cx - win_w / 2).clamp(work.left, (work.right - win_w).max(work.left));
    let y = (cy - win_h / 2).clamp(work.top, (work.bottom - win_h).max(work.top));
    (x, y, win_w, win_h)
}

/// 顯示跟隨系統／如喜深淺色主題且搭載 A Solid GUI Studio 向量標誌的「關於如喜」對話框
pub fn show_about_dialog(owner_hwnd: HWND) {
    let prev = ABOUT_OPEN_HWND.load(Ordering::SeqCst);
    if prev != 0 {
        let prev_hwnd = HWND(prev as *mut c_void);
        if unsafe { IsWindow(prev_hwnd).as_bool() } {
            unsafe {
                let _ = SetForegroundWindow(prev_hwnd);
            }
            return;
        }
        ABOUT_OPEN_HWND.store(0, Ordering::SeqCst);
    }
    if ABOUT_OPEN_HWND
        .compare_exchange(0, -1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    struct OpenHwndResetGuard;
    impl Drop for OpenHwndResetGuard {
        fn drop(&mut self) {
            ABOUT_OPEN_HWND.store(0, Ordering::SeqCst);
        }
    }
    let _open_guard = OpenHwndResetGuard;

    let (kb_mode, is_caps) = read_candidate_state(|s| (s.keyboard_mode.to_lowercase(), s.is_caps_on))
        .unwrap_or_else(|_| ("hsu".to_string(), false));
    let ui_lang = current_effective_ui_lang();
    let copy = build_about_copy(ui_lang, &kb_mode, is_caps, env!("CARGO_PKG_VERSION"));
    let is_dark = resolve_about_dark_theme();

    let ruhi_icon_premul = render_ruhi_icon_premul_rgba();
    let studio_logo_light_premul = render_studio_logo_premul_rgba(false);
    let studio_logo_dark_premul = render_studio_logo_premul_rgba(true);

    unsafe {
        let mut hinst = crate::dll_hinstance();
        if hinst.0.is_null() {
            hinst = GetModuleHandleW(None).unwrap_or_default().into();
        }
        let class_name_str = format!(
            "RuhiAboutDialog_{:x}",
            about_wnd_proc as *const () as usize
        );
        let class_name_wide: Vec<u16> = class_name_str.encode_utf16().chain(Some(0)).collect();
        let class_pcw = PCWSTR(class_name_wide.as_ptr());

        let hicon = ruhi_icon_premul
            .as_ref()
            .and_then(|(w, h, rgba)| create_hicon_from_premul_rgba(*w, *h, rgba));

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(about_wnd_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_pcw,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        struct ClassCleanup {
            hinst: HINSTANCE,
            class_wide: Vec<u16>,
            hicon: Option<HICON>,
            hwnd: HWND,
            disabled_owner: Option<HWND>,
        }
        impl Drop for ClassCleanup {
            fn drop(&mut self) {
                unsafe {
                    if let Some(own) = self.disabled_owner {
                        if IsWindow(own).as_bool() {
                            let _ = EnableWindow(own, true);
                        }
                    }
                    if !self.hwnd.0.is_null() && IsWindow(self.hwnd).as_bool() {
                        let _ = SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                        let _ = DestroyWindow(self.hwnd);
                    }
                    if let Some(ic) = self.hicon {
                        let _ = DestroyIcon(ic);
                    }
                    let _ = UnregisterClassW(PCWSTR(self.class_wide.as_ptr()), self.hinst);
                }
            }
        }

        let mut cleanup = ClassCleanup {
            hinst,
            class_wide: class_name_wide.clone(),
            hicon,
            hwnd: HWND::default(),
            disabled_owner: None,
        };

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
        let ex_style = WS_EX_DLGMODALFRAME | WS_EX_TOPMOST;
        let title_wide: Vec<u16> = copy.window_title.encode_utf16().chain(Some(0)).collect();

        let owner_for_create = if !owner_hwnd.0.is_null()
            && IsWindow(owner_hwnd).as_bool()
            && IsWindowVisible(owner_hwnd).as_bool()
        {
            Some(owner_hwnd)
        } else {
            None
        };

        let dlg_hwnd = match CreateWindowExW(
            ex_style,
            class_pcw,
            PCWSTR(title_wide.as_ptr()),
            style,
            0,
            0,
            460,
            320,
            owner_for_create.unwrap_or_default(),
            None,
            hinst,
            None,
        ) {
            Ok(h) if !h.0.is_null() => h,
            _ => return,
        };
        cleanup.hwnd = dlg_hwnd;
        ABOUT_OPEN_HWND.store(dlg_hwnd.0 as isize, Ordering::SeqCst);

        let dpi = GetDpiForWindow(dlg_hwnd);
        let dpi_scale = if dpi > 0 {
            (dpi as f32 / 96.0).clamp(0.75, 4.0)
        } else {
            1.0
        };
        let client_w = (BASE_CLIENT_W * dpi_scale).round() as i32;
        let client_h = (BASE_CLIENT_H * dpi_scale).round() as i32;
        let (x, y, win_w, win_h) = compute_centered_window_rect(
            owner_for_create.unwrap_or_default(),
            client_w,
            client_h,
            style,
            ex_style,
        );

        let mut state = AboutDialogState {
            is_dark,
            dpi_scale,
            copy,
            ok_rect: RECT::default(),
            ok_hovered: false,
            ok_pressed: false,
            tracking_mouse: false,
            closed: false,
            ruhi_icon_premul,
            studio_logo_light_premul,
            studio_logo_dark_premul,
        };

        let _ = SetWindowLongPtrW(
            dlg_hwnd,
            GWLP_USERDATA,
            &mut state as *mut AboutDialogState as isize,
        );
        if let Some(ic) = hicon {
            let _ = SendMessageW(dlg_hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(ic.0 as isize));
        }
        apply_dwm_window_theme(dlg_hwnd, is_dark);

        if let Some(own) = owner_for_create {
            if IsWindowEnabled(own).as_bool() {
                let _ = EnableWindow(own, false);
                cleanup.disabled_owner = Some(own);
            }
        }

        let _ = SetWindowPos(dlg_hwnd, HWND_TOPMOST, x, y, win_w, win_h, SWP_NOACTIVATE);
        let _ = ShowWindow(dlg_hwnd, SW_SHOW);
        let _ = UpdateWindow(dlg_hwnd);
        let _ = SetForegroundWindow(dlg_hwnd);

        let mut msg = MSG::default();
        while !state.closed && IsWindow(dlg_hwnd).as_bool() {
            let gm = GetMessageW(&mut msg, None, 0, 0);
            if gm.0 <= 0 {
                break;
            }
            if msg.message == WM_QUIT {
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if IsWindow(dlg_hwnd).as_bool() {
            let _ = SetWindowLongPtrW(dlg_hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(dlg_hwnd);
        }
        cleanup.hwnd = HWND::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_color_matches(rgba: &[u8], rgb: [u8; 3]) -> usize {
        rgba.chunks_exact(4)
            .filter(|px| {
                let a = px[3];
                if a < 200 {
                    return false;
                }
                let r = ((px[0] as u16 * 255) / (a as u16)) as i16;
                let g = ((px[1] as u16 * 255) / (a as u16)) as i16;
                let b = ((px[2] as u16 * 255) / (a as u16)) as i16;
                (r - rgb[0] as i16).abs() <= 4
                    && (g - rgb[1] as i16).abs() <= 4
                    && (b - rgb[2] as i16).abs() <= 4
            })
            .count()
    }

    #[test]
    fn test_studio_logo_rle3_dual_theme_contrast_and_bounds() {
        let (lw, lh, light_rgba) = render_studio_logo_premul_rgba(false).expect("light studio logo");
        let (dw, dh, dark_rgba) = render_studio_logo_premul_rgba(true).expect("dark studio logo");
        assert_eq!((lw, lh), (640, 100));
        assert_eq!((dw, dh), (640, 100));
        assert_eq!(light_rgba.len(), 640 * 100 * 4);

        assert!(count_color_matches(&light_rgba, STUDIO_CYAN) > 80);
        assert!(count_color_matches(&light_rgba, STUDIO_MAGENTA) > 30);
        assert!(count_color_matches(&light_rgba, STUDIO_INK_LIGHT) > 1500);
        assert_eq!(count_color_matches(&light_rgba, STUDIO_INK_DARK), 0);

        assert!(count_color_matches(&dark_rgba, STUDIO_CYAN) > 80);
        assert!(count_color_matches(&dark_rgba, STUDIO_MAGENTA) > 30);
        assert!(count_color_matches(&dark_rgba, STUDIO_INK_DARK) > 1500);
        assert_eq!(count_color_matches(&dark_rgba, STUDIO_INK_LIGHT), 0);

        let check_lr_bounds = |rgba: &[u8]| {
            let mut left_q = 0usize;
            let mut right_q = 0usize;
            for (idx, px) in rgba.chunks_exact(4).enumerate() {
                if px[3] >= 200 {
                    let x = (idx as u32) % 640;
                    if x < 160 {
                        left_q += 1;
                    } else if x >= 480 {
                        right_q += 1;
                    }
                }
            }
            assert!(left_q > 80 && right_q > 80, "logo clipped: l={left_q}, r={right_q}");
        };
        check_lr_bounds(&light_rgba);
        check_lr_bounds(&dark_rgba);
    }

    #[test]
    fn test_ruhi_icon_rle1_decodes_orange() {
        let (w, h, rgba) = render_ruhi_icon_premul_rgba().expect("ruhi icon");
        assert_eq!((w, h), (128, 128));
        assert!(count_color_matches(&rgba, RUHI_ORANGE) > 2000);
    }

    #[test]
    fn test_bilinear_blit_and_about_copy_no_obsolete_slogan() {
        let (lw, lh, light_rgba) = render_studio_logo_premul_rgba(false).unwrap();
        for dpi in [1.0f32, 1.5, 2.0] {
            let dib_w = (BASE_CLIENT_W * dpi).round() as i32;
            let dib_h = (BASE_CLIENT_H * dpi).round() as i32;
            let mut dib = vec![240u8; (dib_w as usize) * (dib_h as usize) * 4];
            let logo_h = (28.0 * dpi).round() as i32;
            let logo_w = ((logo_h as f32) * STUDIO_LOGO_ASPECT).round() as i32;
            blit_premul_rgba_over_bgra_dib(
                &mut dib,
                dib_w,
                dib_h,
                &light_rgba,
                lw,
                lh,
                (24.0 * dpi).round() as i32,
                (206.0 * dpi).round() as i32,
                logo_w,
                logo_h,
            );
            let tinted = dib
                .chunks_exact(4)
                .filter(|px| px[0] != 240 || px[1] != 240 || px[2] != 240)
                .count();
            assert!(tinted > 200, "dpi={dpi} should paint studio pixels cleanly");
        }

        let cases = [
            ("zh-TW", "hsu", false),
            ("en-US", "english", false),
            ("zh-TW", "hsu", true),
            ("ja-JP", "japanese", false),
            ("ko-KR", "korean", false),
            ("zh-TW", "iast", false),
            ("zh-TW", "thai", false),
        ];
        for (lang, kb, caps) in cases {
            let copy = build_about_copy(lang, kb, caps, "0.9.6");
            assert!(copy.version_line.contains("0.9.6"));
            let all = format!(
                "{}\n{}\n{}\n{}\n{}",
                copy.window_title, copy.heading, copy.subtitle, copy.motto_title, copy.motto_body
            );
            assert!(!all.contains("望穿秋水"), "obsolete slogan found in {lang}/{kb}");
        }
    }
}
