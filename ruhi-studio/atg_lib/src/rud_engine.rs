//! # `rud_engine` — `rud()` 空間幾何與自適應排版核心引擎
//!
//! 實作 ATGprjs 全域 `rud()` (Responsive UI Design) 空間幾何第一原理：
//! 1. **文字不可侵犯原則 (`cjk_smart_wrap`)**：英文單詞、數字與快捷鍵視為不可分割原子，並嚴格遵守漢字避頭尾法則（Kinsoku Shori）。
//! 2. **短標籤雙端齊行 (`justify_short_label`)**：2～4 字元漢字短標籤自動均分間距，達成首尾雙端對齊。
//! 3. **表格二階段需求驅動分配 (`calculate_table_rwd_widths`)**：支援 `Fixed`、`Uniform` 與 `Flex` 三態欄位，消除僵化百分比造成的截字與空白浪費。
//! 4. **垂直三欄收斂與雙主區對等 (`calculate_vertical_rwd_panes`)**：底部日誌區優先收斂至實際需求高度，將剩餘空間 50:50 對等釋放予主工作區。
//! 5. **樹狀導覽動態寬度精算 (`calc_treeview_width`)**：依最長項目文字、階層縮排與呼吸留白精算側欄最小寬度，杜絕 `...` 截斷。
//! 6. **原子群組流式折行 (`rud_flowing_bar` / `rud_atomic_group`)** 與 **雙層選項排版 (`two_tier_option`)**。

use serde::{Deserialize, Serialize};

/// 避頭點字元集合（禁止出現在新行行首）
const KINSOKU_NO_START: &[char] = &[
    '，', '。', '、', '；', '：', '！', '？', '）', '》', '」', '』', '】', '〕', '〉', '”', '’',
    ',', '.', ';', ':', '!', '?', ')', ']', '}', '%',
];

/// 避尾點字元集合（禁止單獨留在行尾）
const KINSOKU_NO_END: &[char] = &[
    '（', '《', '「', '『', '【', '〔', '〈', '“', '‘', '(', '[', '{',
];

/// 判斷字元是否為全形或 CJK 寬字元（視覺寬度計為 2 個半形單位）
pub fn is_wide_cjk_char(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x1100..=0x115F
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
            | 0x20000..=0x2FA1F
    )
}

/// 計算字串之視覺半形字元總寬度（CJK 全形字元 = 2，ASCII 半形字元 = 1）
pub fn visual_char_length(text: &str) -> usize {
    text.chars()
        .map(|c| if is_wide_cjk_char(c) { 2 } else { 1 })
        .sum()
}

/// 計算字串中純 CJK 漢字/全形字元數量（忽略尾端冒號與空白）
pub fn count_cjk_chars(text: &str) -> usize {
    let trimmed = text.trim().trim_end_matches([':', '：']);
    trimmed.chars().filter(|&c| is_wide_cjk_char(c)).count()
}

/// 判斷是否為適合首尾雙端對齊之短漢字標籤（2～4 個全形字元且無空白句）
pub fn is_short_cjk_label(text: &str) -> bool {
    let clean = text.trim().trim_end_matches([':', '：']);
    if clean.is_empty() || clean.contains(' ') {
        return false;
    }
    let chars: Vec<char> = clean.chars().collect();
    (2..=4).contains(&chars.len()) && chars.iter().all(|&c| is_wide_cjk_char(c))
}

/// 將 2～4 字元之短漢字標籤插入均分間距，使其視覺寬度對齊 `target_cjk_chars`（預設 4 字寬）。
///
/// - 若輸入超過 4 字或為純英文句子，則原樣回傳（維持自然靠左）。
/// - 若原字串帶有尾端冒號（`：` 或 `:`），會在對齊後保留冒號。
pub fn justify_short_label(text: &str, target_cjk_chars: usize) -> String {
    let raw = text.trim();
    let (clean, suffix) = if let Some(stripped) = raw.strip_suffix('：') {
        (stripped.trim(), "：")
    } else if let Some(stripped) = raw.strip_suffix(':') {
        (stripped.trim(), ":")
    } else {
        (raw, "")
    };

    if !is_short_cjk_label(clean) {
        return text.to_string();
    }

    let chars: Vec<char> = clean.chars().collect();
    let n = chars.len();
    let target = target_cjk_chars.max(2);
    if n >= target {
        return format!("{clean}{suffix}");
    }

    // 以全形空白 `\u{3000}` 與半形空白組合補齊 (target - n) 個全形字寬
    let missing_half_units = (target - n) * 2;
    let gaps = n - 1;
    let base_half_per_gap = missing_half_units / gaps;
    let rem_half = missing_half_units % gaps;

    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        out.push(*ch);
        if i < gaps {
            let mut units = base_half_per_gap + usize::from(i < rem_half);
            while units >= 2 {
                out.push('\u{3000}');
                units -= 2;
            }
            if units == 1 {
                out.push(' ');
            }
        }
    }
    out.push_str(suffix);
    out
}

/// 語意分詞權杖（供 `cjk_smart_wrap` 使用）
#[derive(Debug, Clone, PartialEq, Eq)]
enum WrapToken {
    /// 換行符號
    Newline,
    /// 不可拆分的拉丁單詞/數字/快捷鍵
    AtomicAscii(String),
    /// 單一全形字元或攜帶避頭尾附著標點的字元組
    CjkCluster(String),
}

fn tokenize_for_smart_wrap(text: &str) -> Vec<WrapToken> {
    let mut raw_tokens: Vec<WrapToken> = Vec::new();
    let mut ascii_buf = String::new();

    let flush_ascii = |buf: &mut String, out: &mut Vec<WrapToken>| {
        if !buf.is_empty() {
            out.push(WrapToken::AtomicAscii(std::mem::take(buf)));
        }
    };

    for ch in text.chars() {
        if ch == '\n' {
            flush_ascii(&mut ascii_buf, &mut raw_tokens);
            raw_tokens.push(WrapToken::Newline);
        } else if ch.is_ascii_alphanumeric()
            || matches!(ch, '_' | '-' | '+' | '.' | '/' | '@' | '#')
        {
            ascii_buf.push(ch);
        } else if ch == ' ' {
            flush_ascii(&mut ascii_buf, &mut raw_tokens);
            raw_tokens.push(WrapToken::AtomicAscii(" ".to_string()));
        } else {
            flush_ascii(&mut ascii_buf, &mut raw_tokens);
            raw_tokens.push(WrapToken::CjkCluster(ch.to_string()));
        }
    }
    flush_ascii(&mut ascii_buf, &mut raw_tokens);

    // 合併避頭點與避尾點：
    // 1. 若某權杖為避頭標點 (如 `，。）》`)，將其併入前一個有效權杖，防止掉到行首。
    // 2. 若前一個權杖為避尾標點 (如 `（《「`)，將當前權杖併入前一個權杖，防止避尾標點孤留行尾。
    let mut merged: Vec<WrapToken> = Vec::with_capacity(raw_tokens.len());
    for tok in raw_tokens {
        match tok {
            WrapToken::Newline => merged.push(WrapToken::Newline),
            WrapToken::AtomicAscii(ref s) | WrapToken::CjkCluster(ref s) => {
                let first_char = s.chars().next().unwrap_or(' ');
                let attach_to_prev = KINSOKU_NO_START.contains(&first_char)
                    || merged.last().is_some_and(|prev| match prev {
                        WrapToken::AtomicAscii(ps) | WrapToken::CjkCluster(ps) => ps
                            .chars()
                            .last()
                            .is_some_and(|last_c| KINSOKU_NO_END.contains(&last_c)),
                        WrapToken::Newline => false,
                    });

                if attach_to_prev {
                    if let Some(WrapToken::AtomicAscii(prev_s) | WrapToken::CjkCluster(prev_s)) =
                        merged.last_mut()
                    {
                        prev_s.push_str(s);
                        continue;
                    }
                }
                merged.push(tok);
            }
        }
    }

    merged
}

/// 遵循漢字避頭尾法則（Kinsoku Shori）與英文單詞完整性之智慧斷行演算法。
///
/// - `max_width_px`：容器可用像素寬度。
/// - `half_char_px`：單一半形字元估算像素寬度（例如 14pt 字體約為 `7.5` px）。
pub fn cjk_smart_wrap(text: &str, max_width_px: f32, half_char_px: f32) -> String {
    let unit_px = half_char_px.max(4.0);
    let max_units = ((max_width_px / unit_px).floor() as usize).max(8);

    let tokens = tokenize_for_smart_wrap(text);
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let mut current_units: usize = 0;

    for tok in tokens {
        match tok {
            WrapToken::Newline => {
                lines.push(std::mem::take(&mut current_line));
                current_units = 0;
            }
            WrapToken::AtomicAscii(s) | WrapToken::CjkCluster(s) => {
                if s == " " && current_line.is_empty() {
                    continue;
                }
                let tok_units = visual_char_length(&s);
                if current_units > 0 && current_units + tok_units > max_units {
                    lines.push(current_line.trim_end().to_string());
                    current_line = if s == " " { String::new() } else { s };
                    current_units = visual_char_length(&current_line);
                } else {
                    current_line.push_str(&s);
                    current_units += tok_units;
                }
            }
        }
    }

    if !current_line.is_empty() || lines.is_empty() {
        lines.push(current_line);
    }

    lines.join("\n")
}

/// 表格欄位自適應分配模式 (`rud()` 二階段分配定律)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RwdColMode {
    /// 固定寬度欄（如勾選框、序號、圖示）
    Fixed,
    /// 等寬欄（如時間碼、日期、數值，享有均等配額但設有上限）
    Uniform,
    /// 彈性文字欄（如標題、路徑、說明，按實際未滿足需求比例優先分配剩餘空間）
    Flex,
}

/// 表格欄位空間規格描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RwdColumnSpec {
    pub id: String,
    pub header_title: String,
    pub mode: RwdColMode,
    /// 最小保底像素寬度
    pub min_width: f32,
    /// 完整呈現內容所需的理想像素寬度（由表頭與前 N 筆資料取最大值）
    pub ideal_width: f32,
    /// 最大像素寬度上限（`None` 表無上限）
    pub max_width: Option<f32>,
}

impl RwdColumnSpec {
    pub fn fixed(id: impl Into<String>, header_title: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            header_title: header_title.into(),
            mode: RwdColMode::Fixed,
            min_width: width,
            ideal_width: width,
            max_width: Some(width),
        }
    }

    pub fn uniform(
        id: impl Into<String>,
        header_title: impl Into<String>,
        min_width: f32,
        ideal_width: f32,
        max_width: f32,
    ) -> Self {
        Self {
            id: id.into(),
            header_title: header_title.into(),
            mode: RwdColMode::Uniform,
            min_width,
            ideal_width: ideal_width.max(min_width),
            max_width: Some(max_width.max(min_width)),
        }
    }

    pub fn flex(
        id: impl Into<String>,
        header_title: impl Into<String>,
        min_width: f32,
        ideal_width: f32,
    ) -> Self {
        Self {
            id: id.into(),
            header_title: header_title.into(),
            mode: RwdColMode::Flex,
            min_width,
            ideal_width: ideal_width.max(min_width),
            max_width: None,
        }
    }
}

/// 實作 `rud()` 表格欄位二階段需求驅動分配定律 (`calculate_table_rwd_widths`)。
///
/// 1. **第一階段（生存底線保底）**：每一欄至少獲得 `max(min_width, 表頭不截斷寬度)`。
/// 2. **第二階段（剩餘空間智慧分配）**：
///    - `Fixed` 欄維持固定寬度。
///    - `Uniform` 欄先分配至理想寬度（受 `max_width` 限制）。
///    - `Flex` 欄計算實際需求缺口 `need = max(0, ideal_width - base_width)`，按需求比例分配；
///      若全數 `Flex` 欄皆已 100% 完整顯示無截斷，最後多出的視窗空間再由所有 `Flex` 欄均分。
pub fn calculate_table_rwd_widths(
    specs: &[RwdColumnSpec],
    available_width: f32,
    half_char_px: f32,
) -> Vec<f32> {
    if specs.is_empty() {
        return Vec::new();
    }

    let char_px = half_char_px.max(6.0);
    let header_padding = 24.0;

    // 第一階段：計算各欄保底寬度
    let mut widths: Vec<f32> = specs
        .iter()
        .map(|s| {
            let header_min = (visual_char_length(&s.header_title) as f32) * char_px + header_padding;
            match s.mode {
                RwdColMode::Fixed => s.ideal_width.max(s.min_width),
                RwdColMode::Uniform | RwdColMode::Flex => s.min_width.max(header_min),
            }
        })
        .collect();

    let base_sum: f32 = widths.iter().sum();
    let mut surplus = available_width - base_sum;
    if surplus <= 0.5 {
        return widths;
    }

    // 第二階段 A：滿足 Uniform 欄的理想寬度
    let uniform_indices: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| (s.mode == RwdColMode::Uniform).then_some(i))
        .collect();

    if !uniform_indices.is_empty() && surplus > 0.0 {
        let uniform_needs: Vec<f32> = uniform_indices
            .iter()
            .map(|&i| {
                let cap = specs[i].max_width.unwrap_or(specs[i].ideal_width);
                (specs[i].ideal_width.min(cap) - widths[i]).max(0.0)
            })
            .collect();
        let total_uniform_need: f32 = uniform_needs.iter().sum();
        if total_uniform_need > 0.0 {
            let ratio = (surplus / total_uniform_need).min(1.0);
            for (&col_idx, &need) in uniform_indices.iter().zip(uniform_needs.iter()) {
                let add = need * ratio;
                widths[col_idx] += add;
                surplus -= add;
            }
        }
    }

    // 第二階段 B：按需求缺口比例分配給 Flex 欄
    let flex_indices: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| (s.mode == RwdColMode::Flex).then_some(i))
        .collect();

    if !flex_indices.is_empty() && surplus > 0.0 {
        let flex_needs: Vec<f32> = flex_indices
            .iter()
            .map(|&i| {
                let ideal = match specs[i].max_width {
                    Some(max_w) => specs[i].ideal_width.min(max_w),
                    None => specs[i].ideal_width,
                };
                (ideal - widths[i]).max(0.0)
            })
            .collect();
        let total_flex_need: f32 = flex_needs.iter().sum();

        if total_flex_need > 0.0 {
            let ratio = (surplus / total_flex_need).min(1.0);
            for (&col_idx, &need) in flex_indices.iter().zip(flex_needs.iter()) {
                let add = need * ratio;
                widths[col_idx] += add;
                surplus -= add;
            }
        }

        // 若所有 Flex 欄內容皆已 100% 滿足且視窗仍有餘裕，均分給所有無上限之 Flex 欄
        if surplus > 0.5 {
            let share = surplus / (flex_indices.len() as f32);
            for &col_idx in &flex_indices {
                widths[col_idx] += share;
            }
        }
    }

    widths
}

/// 垂直三欄式分割高度計算結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalPaneHeights {
    pub top_main_height: f32,
    pub middle_main_height: f32,
    pub bottom_log_height: f32,
}

/// 實作 `rud()` 垂直分割視窗之「次要區優先收斂、雙主區 50:50 對等分配」定律。
///
/// - `available_height`：視窗垂直總可用高度。
/// - `log_content_lines`：底部狀態/日誌區目前實際行數。
/// - `line_height_px`：單行像素高度。
/// - `log_max_ratio`：底部日誌區佔總高度之上限比例（預設 `0.22`，即 22%）。
pub fn calculate_vertical_rwd_panes(
    available_height: f32,
    log_content_lines: usize,
    line_height_px: f32,
    log_max_ratio: f32,
) -> VerticalPaneHeights {
    let total_h = available_height.max(180.0);
    let ratio_cap = log_max_ratio.clamp(0.10, 0.35);
    let min_log_h = (line_height_px * 2.0 + 12.0).max(44.0);
    let ideal_log_h = (log_content_lines.max(2) as f32) * line_height_px + 16.0;
    let max_log_h = (total_h * ratio_cap).max(min_log_h);

    let bottom_log_height = ideal_log_h.clamp(min_log_h, max_log_h);
    let remaining = (total_h - bottom_log_height).max(100.0);
    let half = remaining * 0.5;

    VerticalPaneHeights {
        top_main_height: half,
        middle_main_height: remaining - half,
        bottom_log_height,
    }
}

/// 計算樹狀導覽側邊欄 (`Treeview`) 之不截字最佳像素寬度 (`calc_treeview_width`)。
///
/// - `items`：所有導覽項目之 `(階層深度 0..N, 顯示文字)`。
/// - `half_char_px`：半形字元像素寬度（隨 `ui_scale` 動態放大）。
/// - `ui_scale`：目前 UI 縮放倍率。
pub fn calc_treeview_width(items: &[(usize, &str)], half_char_px: f32, ui_scale: f32) -> f32 {
    let scale = ui_scale.clamp(0.75, 2.50);
    let char_px = half_char_px.max(6.0);
    let indent_px = 20.0 * scale;
    let icon_and_arrow_px = 36.0 * scale;
    let breathing_padding_px = 28.0 * scale;

    let max_item_px = items
        .iter()
        .map(|(depth, label)| {
            let text_px = (visual_char_length(label) as f32) * char_px;
            (*depth as f32) * indent_px + icon_and_arrow_px + text_px + breathing_padding_px
        })
        .fold(160.0 * scale, f32::max);

    max_item_px.ceil()
}

// ============================================================================
// egui 視覺排版元件 (需啟用 `ui` feature)
// ============================================================================

#[cfg(feature = "ui")]
use crate::theme_engine::SemanticPalette;

/// 繪製自動首尾對齊之短漢字標籤（2～4 字自動均分間距，超過 4 字靠左）
#[cfg(feature = "ui")]
pub fn justified_cjk_label(
    ui: &mut egui::Ui,
    text: &str,
    target_cjk_chars: usize,
) -> egui::Response {
    let formatted = justify_short_label(text, target_cjk_chars);
    ui.label(formatted)
}

/// 建立流式自動換行工具列 (`RudFlowingFrame`)，搭配 [`rud_atomic_group`] 確保控制項成對換行。
#[cfg(feature = "ui")]
pub fn rud_flowing_bar<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
        add_contents(ui)
    })
}

/// 建立不可拆分的原子控制項群組（如 `[標籤 + 下拉選單]` 或 `[核取方塊 + 微調框]`），
/// 在流式工具列中維持單一水平整體。
#[cfg(feature = "ui")]
pub fn rud_atomic_group<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.scope(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            add_contents(ui)
        })
        .inner
    })
}

/// 實作 `rud()` 偏好設定與表單之「雙層分離排版 (`two_tier_option`)」：
/// - **第一層（主控制項列）**：高對比粗體顯示簡短標題，左附單選鈕或核取方塊。
/// - **第二層（輔助說明列）**：退縮對齊文字起點、採用次要柔和色 (`fg_muted`)，並隨容器寬度執行 `cjk_smart_wrap` 智慧折行。
#[cfg(feature = "ui")]
pub fn two_tier_radio<V: PartialEq + Clone>(
    ui: &mut egui::Ui,
    current_value: &mut V,
    target_value: V,
    title: &str,
    subtitle: &str,
    palette: &SemanticPalette,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let resp = ui.radio_value(
            current_value,
            target_value,
            egui::RichText::new(title)
                .strong()
                .color(palette.fg_bright.to_egui()),
        );
        if resp.changed() {
            changed = true;
        }

        if !subtitle.trim().is_empty() {
            let avail_w = (ui.available_width() - 28.0).max(120.0);
            let body_font_size = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Small)
                .map(|f| f.size)
                .unwrap_or(12.0);
            let wrapped = cjk_smart_wrap(subtitle, avail_w, body_font_size * 0.55);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(wrapped)
                        .small()
                        .color(palette.fg_muted.to_egui()),
                );
            });
        }
    });
    changed
}

/// 雙層核取方塊 (`two_tier_checkbox`)：第一層為粗體主選項，第二層為退縮自動折行之輔助說明。
#[cfg(feature = "ui")]
pub fn two_tier_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    title: &str,
    subtitle: &str,
    palette: &SemanticPalette,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let resp = ui.checkbox(
            checked,
            egui::RichText::new(title)
                .strong()
                .color(palette.fg_bright.to_egui()),
        );
        if resp.changed() {
            changed = true;
        }

        if !subtitle.trim().is_empty() {
            let avail_w = (ui.available_width() - 28.0).max(120.0);
            let body_font_size = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Small)
                .map(|f| f.size)
                .unwrap_or(12.0);
            let wrapped = cjk_smart_wrap(subtitle, avail_w, body_font_size * 0.55);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(wrapped)
                        .small()
                        .color(palette.fg_muted.to_egui()),
                );
            });
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_char_length_and_short_label() {
        assert_eq!(visual_char_length("ABC"), 3);
        assert_eq!(visual_char_length("主題"), 4);
        assert!(is_short_cjk_label("主題："));
        assert!(is_short_cjk_label("介面語言"));
        assert!(!is_short_cjk_label("自動跟隨作業系統深淺色"));

        let j2 = justify_short_label("主題：", 4);
        assert!(j2.starts_with('主'));
        assert!(j2.ends_with("題："));
        assert_eq!(visual_char_length(j2.trim_end_matches('：')), 8);
    }

    #[test]
    fn test_cjk_smart_wrap_kinsoku_and_atomic_ascii() {
        let text = "支援 Windows 11 DWM 沉浸式標題列，不會把單詞切斷。";
        let wrapped = cjk_smart_wrap(text, 120.0, 7.0);
        for line in wrapped.lines() {
            let first = line.chars().next().unwrap_or(' ');
            assert!(
                !KINSOKU_NO_START.contains(&first),
                "行首出現避頭標點: {line}"
            );
        }
        assert!(wrapped.contains("Windows"));
    }

    #[test]
    fn test_calculate_table_rwd_widths() {
        let specs = vec![
            RwdColumnSpec::fixed("idx", "#", 40.0),
            RwdColumnSpec::uniform("time", "時間碼", 80.0, 100.0, 120.0),
            RwdColumnSpec::flex("title", "曲目標題", 120.0, 320.0),
            RwdColumnSpec::flex("note", "備註", 80.0, 140.0),
        ];
        let widths = calculate_table_rwd_widths(&specs, 800.0, 7.0);
        assert_eq!(widths.len(), 4);
        assert!((widths[0] - 40.0).abs() < 0.1);
        // 曲目標題需求缺口較大，應獲得比備註欄更寬的空間
        assert!(widths[2] > widths[3]);
        let sum: f32 = widths.iter().sum();
        assert!((sum - 800.0).abs() < 1.0);
    }

    #[test]
    fn test_calculate_vertical_rwd_panes() {
        let panes = calculate_vertical_rwd_panes(900.0, 3, 18.0, 0.22);
        // 3 行日誌僅需約 70px，遠小於 900 * 0.22 = 198px
        assert!(panes.bottom_log_height < 100.0);
        assert!((panes.top_main_height - panes.middle_main_height).abs() < 0.1);
        assert!(
            (panes.top_main_height + panes.middle_main_height + panes.bottom_log_height - 900.0)
                .abs()
                < 0.5
        );
    }

    #[test]
    fn test_calc_treeview_width() {
        let items = vec![(0, "系統總覽"), (1, "語音轉寫與時間軸校對設定")];
        let w_1x = calc_treeview_width(&items, 7.0, 1.0);
        let w_15x = calc_treeview_width(&items, 10.5, 1.5);
        assert!(w_15x > w_1x);
    }
}
