//! # `table_engine` — `rud()` 自適應資料表與漢字多模態排序/批次行內編輯引擎
//!
//! 對應 Python 版 `_lib/universal_ui.py` 之 Treeview 進階表格系統，提供：
//! 1. **`rud()` 二階段需求驅動欄寬分配**：自動量測表頭與前 N 筆資料，為 `Fixed`、`Uniform`、`Flex` 欄位精算最適像素寬度。
//! 2. **多模態智慧排序**：自動識別純數值、時間碼 (`HH:MM:SS.ms`) 與漢字（支援教育部注音、康熙 214 部首、總筆畫、漢語拼音、台語羅馬字，以及智慧略過開頭括號/曲目編號）。
//! 3. **表頭右鍵快顯功能表**：支援動態隱藏/顯示欄位、切換漢字排序檢索法與一鍵重算最適欄寬。
//! 4. **雙擊 / `F2` 行內編輯與 `Alt+Click` 批次流水號填充**：支援 `[##1]` ➔ `[01], [02]...`、`(#005)` ➔ `(005), (006)...` 等智慧序號展開。
//! 5. **奇偶列斑馬紋 (`row_alt`) 與列上下搬移**。

use std::collections::BTreeSet;

use crate::cjk_sort_engine::{CjkSortEngine, CjkSortMode};
use crate::rud_engine::{
    calculate_table_rwd_widths, visual_char_length, RwdColMode, RwdColumnSpec,
};
#[cfg(feature = "ui")]
use crate::theme_engine::SemanticPalette;

/// 表格欄位定義
#[derive(Debug, Clone)]
pub struct TableColumnDef {
    pub id: String,
    pub title: String,
    pub mode: RwdColMode,
    pub min_width: f32,
    pub max_width: Option<f32>,
    pub visible: bool,
    pub editable: bool,
}

impl TableColumnDef {
    pub fn fixed(id: impl Into<String>, title: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            mode: RwdColMode::Fixed,
            min_width: width,
            max_width: Some(width),
            visible: true,
            editable: false,
        }
    }

    pub fn uniform(
        id: impl Into<String>,
        title: impl Into<String>,
        min_width: f32,
        max_width: f32,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            mode: RwdColMode::Uniform,
            min_width,
            max_width: Some(max_width),
            visible: true,
            editable: true,
        }
    }

    pub fn flex(id: impl Into<String>, title: impl Into<String>, min_width: f32) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            mode: RwdColMode::Flex,
            min_width,
            max_width: None,
            visible: true,
            editable: true,
        }
    }

    pub fn with_editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }
}

/// 單列資料結構
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub id: u64,
    pub cells: Vec<String>,
}

impl TableRow {
    pub fn new(id: u64, cells: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            id,
            cells: cells.into_iter().map(Into::into).collect(),
        }
    }
}

/// 行內編輯活動狀態
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ActiveInlineEdit {
    row_idx: usize,
    col_idx: usize,
    buffer: String,
    batch_rows: Vec<usize>,
    just_opened: bool,
}

/// 批次流水號語法解析結果（如 `[##1]`、`(#005)`、`#1`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequencePattern {
    pub prefix: String,
    pub bracket_open: String,
    pub digits_width: usize,
    pub start_num: usize,
    pub bracket_close: String,
    pub suffix: String,
}

/// 解析字串中是否包含 `[#1]`、`[##01]`、`(#1)` 或 `#01` 流水號樣板，並展開為指定序號 (`offset = 0, 1, 2...`)。
pub fn expand_batch_sequence_pattern(template: &str, offset: usize) -> String {
    // 尋找第一個 `#` 區段
    let bytes = template.as_bytes();
    let mut hash_start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' {
            hash_start = Some(i);
            break;
        }
    }
    let Some(h_start) = hash_start else {
        return template.to_string();
    };

    let mut h_end = h_start;
    while h_end < bytes.len() && bytes[h_end] == b'#' {
        h_end += 1;
    }
    let hash_count = h_end - h_start;

    let mut d_end = h_end;
    while d_end < bytes.len() && bytes[d_end].is_ascii_digit() {
        d_end += 1;
    }

    if d_end == h_end {
        // 僅有 `#` 而無起始數字，預設從 1 開始
        let num = 1 + offset;
        let formatted = format!("{num:0width$}", width = hash_count);
        return format!(
            "{}{}{}",
            &template[..h_start],
            formatted,
            &template[d_end..]
        );
    }

    let digit_str = &template[h_end..d_end];
    let start_num: usize = digit_str.parse().unwrap_or(1);
    let digit_len = digit_str.len();
    // `[##1]` 總位數 = hash_count (2) 或 digit_len 的較大者（若 hash_count >= 2 則補零至 hash_count 位）
    let pad_width = if hash_count >= 2 {
        hash_count
    } else {
        digit_len.max(1)
    };
    let num = start_num + offset;
    let formatted = format!("{num:0width$}", width = pad_width);
    format!(
        "{}{}{}",
        &template[..h_start],
        formatted,
        &template[d_end..]
    )
}

/// 嘗試將時間碼 (`HH:MM:SS.ms` 或 `MM:SS.ms`) 轉為秒數以供精確數值排序
fn parse_timecode_seconds(text: &str) -> Option<f64> {
    let s = text.trim();
    if !s.contains(':') {
        return None;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut total = 0.0_f64;
    for part in parts {
        let v: f64 = part.parse().ok()?;
        total = total * 60.0 + v;
    }
    Some(total)
}

/// 嘗試將純數字或帶百分比/單位的數值轉為 `f64`
fn parse_numeric_value(text: &str) -> Option<f64> {
    let s = text
        .trim()
        .trim_end_matches(['%', 's', 'x', 'X'])
        .replace(',', "");
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok()
}

/// `UniversalTable` — 整合 `rud()` 欄寬精算、漢字多模態排序與行內批次編輯之自適應表格引擎
pub struct UniversalTable {
    pub id: String,
    pub columns: Vec<TableColumnDef>,
    pub rows: Vec<TableRow>,
    pub selected_rows: BTreeSet<usize>,
    pub sort_col: Option<usize>,
    pub sort_ascending: bool,
    pub cjk_sort_mode: CjkSortMode,
    pub clean_title_prefix: bool,
    cjk_engine: CjkSortEngine,
    active_edit: Option<ActiveInlineEdit>,
}

impl UniversalTable {
    pub fn new(id: impl Into<String>, columns: Vec<TableColumnDef>) -> Self {
        Self {
            id: id.into(),
            columns,
            rows: Vec::new(),
            selected_rows: BTreeSet::new(),
            sort_col: None,
            sort_ascending: true,
            cjk_sort_mode: CjkSortMode::Zhuyin,
            clean_title_prefix: true,
            cjk_engine: CjkSortEngine::new(),
            active_edit: None,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<TableRow>) {
        self.rows = rows;
        self.selected_rows.clear();
        self.active_edit = None;
    }

    /// 根據指定欄位索引執行智慧排序（自動判斷時間碼、純數值或 CJK 漢字檢索法）
    pub fn sort_by_column(&mut self, col_idx: usize, ascending: bool) {
        if col_idx >= self.columns.len() {
            return;
        }
        self.sort_col = Some(col_idx);
        self.sort_ascending = ascending;

        let mode = self.cjk_sort_mode;
        let clean_prefix = self.clean_title_prefix;
        let engine = &self.cjk_engine;

        self.rows.sort_by(|a, b| {
            let va = a.cells.get(col_idx).map(String::as_str).unwrap_or("");
            let vb = b.cells.get(col_idx).map(String::as_str).unwrap_or("");

            let ord = if let (Some(ta), Some(tb)) =
                (parse_timecode_seconds(va), parse_timecode_seconds(vb))
            {
                ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
            } else if let (Some(na), Some(nb)) = (parse_numeric_value(va), parse_numeric_value(vb))
            {
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                let ka = engine.sort_key(va, mode, clean_prefix);
                let kb = engine.sort_key(vb, mode, clean_prefix);
                ka.cmp(&kb)
            };

            if ascending {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    /// 計算目前所有可見欄位之 `rud()` 二階段需求驅動像素寬度
    pub fn compute_visible_column_widths(
        &self,
        available_width: f32,
        half_char_px: f32,
    ) -> Vec<(usize, f32)> {
        let char_px = half_char_px.max(6.0);
        let cell_pad = 22.0;

        let mut visible_indices = Vec::new();
        let mut specs = Vec::new();

        for (col_idx, col) in self.columns.iter().enumerate() {
            if !col.visible {
                continue;
            }
            visible_indices.push(col_idx);

            let header_px = (visual_char_length(&col.title) as f32) * char_px + cell_pad + 16.0;
            let sample_max_px = self
                .rows
                .iter()
                .take(80)
                .filter_map(|r| r.cells.get(col_idx))
                .map(|cell| (visual_char_length(cell) as f32) * char_px + cell_pad)
                .fold(header_px, f32::max);

            specs.push(RwdColumnSpec {
                id: col.id.clone(),
                header_title: col.title.clone(),
                mode: col.mode,
                min_width: col.min_width,
                ideal_width: sample_max_px.max(col.min_width),
                max_width: col.max_width,
            });
        }

        let widths = calculate_table_rwd_widths(&specs, available_width, char_px);
        visible_indices.into_iter().zip(widths).collect()
    }

    /// 對指定列集合與欄位套用批次行內編輯（支援 `[##1]` 流水號自動展開）
    pub fn apply_batch_inline_edit(
        &mut self,
        target_rows: &[usize],
        col_idx: usize,
        template: &str,
    ) {
        for (offset, &row_idx) in target_rows.iter().enumerate() {
            if let Some(row) = self.rows.get_mut(row_idx) {
                if let Some(cell) = row.cells.get_mut(col_idx) {
                    *cell = expand_batch_sequence_pattern(template, offset);
                }
            }
        }
    }

    /// 將選取列向上移動一列
    pub fn move_selected_up(&mut self) -> bool {
        if self.selected_rows.is_empty() || self.selected_rows.contains(&0) {
            return false;
        }
        let indices: Vec<usize> = self.selected_rows.iter().copied().collect();
        let mut new_sel = BTreeSet::new();
        for idx in indices {
            self.rows.swap(idx - 1, idx);
            new_sel.insert(idx - 1);
        }
        self.selected_rows = new_sel;
        true
    }

    /// 將選取列向下移動一列
    pub fn move_selected_down(&mut self) -> bool {
        if self.selected_rows.is_empty() || self.rows.is_empty() {
            return false;
        }
        let last = self.rows.len() - 1;
        if self.selected_rows.contains(&last) {
            return false;
        }
        let indices: Vec<usize> = self.selected_rows.iter().rev().copied().collect();
        let mut new_sel = BTreeSet::new();
        for idx in indices {
            self.rows.swap(idx, idx + 1);
            new_sel.insert(idx + 1);
        }
        self.selected_rows = new_sel;
        true
    }

    /// 在 `egui::Ui` 中繪製具備 `rud()` 自適應欄寬、表頭右鍵選單、多模態排序與行內編輯之表格
    #[cfg(feature = "ui")]
    pub fn render(&mut self, ui: &mut egui::Ui, palette: &SemanticPalette, ui_scale: f32) {
        use egui_extras::{Column, TableBuilder};

        let scale = ui_scale.clamp(0.75, 2.50);
        let half_char_px = 7.2 * scale;
        let avail_w = (ui.available_width() - 8.0).max(240.0);
        let visible_cols = self.compute_visible_column_widths(avail_w, half_char_px);
        if visible_cols.is_empty() {
            return;
        }

        // F2 快捷鍵啟動目前選取列的第一個可編輯欄
        let f2_pressed = ui.input(|i| i.key_pressed(egui::Key::F2));
        if f2_pressed && self.active_edit.is_none() {
            if let Some(&first_row) = self.selected_rows.iter().next() {
                if let Some(&(first_editable_col, _)) = visible_cols
                    .iter()
                    .find(|&&(c_idx, _)| self.columns[c_idx].editable)
                {
                    let init_val = self.rows[first_row]
                        .cells
                        .get(first_editable_col)
                        .cloned()
                        .unwrap_or_default();
                    let batch: Vec<usize> = self.selected_rows.iter().copied().collect();
                    self.active_edit = Some(ActiveInlineEdit {
                        row_idx: first_row,
                        col_idx: first_editable_col,
                        buffer: init_val,
                        batch_rows: if batch.is_empty() {
                            vec![first_row]
                        } else {
                            batch
                        },
                        just_opened: true,
                    });
                }
            }
        }

        let header_height = 28.0 * scale;
        let row_height = 26.0 * scale;

        let mut pending_sort: Option<(usize, bool)> = None;
        let mut commit_edit = false;
        let mut cancel_edit = false;

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .min_scrolled_height(140.0);

        for &(_col_idx, width) in &visible_cols {
            builder = builder.column(
                Column::initial(width)
                    .at_least(36.0 * scale)
                    .clip(true),
            );
        }

        let visible_count = self.columns.iter().filter(|c| c.visible).count();

        builder
            .header(header_height, |mut header| {
                for &(col_idx, _w) in &visible_cols {
                    header.col(|ui| {
                        let col_title = self.columns[col_idx].title.clone();
                        let arrow = if self.sort_col == Some(col_idx) {
                            if self.sort_ascending {
                                " ▲"
                            } else {
                                " ▼"
                            }
                        } else {
                            ""
                        };
                        let label_text = format!("{col_title}{arrow}");
                        let resp = ui.add(
                            egui::Button::new(
                                egui::RichText::new(label_text)
                                    .strong()
                                    .color(palette.fg_bright.to_egui()),
                            )
                            .frame(false),
                        );

                        if resp.clicked() {
                            let next_asc = if self.sort_col == Some(col_idx) {
                                !self.sort_ascending
                            } else {
                                true
                            };
                            pending_sort = Some((col_idx, next_asc));
                        }

                        // 表頭右鍵選單：切換欄位顯示與漢字排序法
                        resp.context_menu(|ui| {
                            ui.label(egui::RichText::new("漢字排序檢索模式").strong());
                            for mode in [
                                CjkSortMode::Zhuyin,
                                CjkSortMode::Radical,
                                CjkSortMode::Stroke,
                                CjkSortMode::Pinyin,
                                CjkSortMode::Taigi,
                            ] {
                                if ui
                                    .radio_value(&mut self.cjk_sort_mode, mode, mode.display_name_tw())
                                    .changed()
                                {
                                    if let Some(sc) = self.sort_col {
                                        pending_sort = Some((sc, self.sort_ascending));
                                    }
                                    ui.close_menu();
                                }
                            }
                            if ui
                                .checkbox(
                                    &mut self.clean_title_prefix,
                                    "智慧略過開頭括號與編號",
                                )
                                .changed()
                            {
                                if let Some(sc) = self.sort_col {
                                    pending_sort = Some((sc, self.sort_ascending));
                                }
                            }
                            ui.separator();
                            ui.label(egui::RichText::new("顯示 / 隱藏欄位").strong());
                            for c in &mut self.columns {
                                let can_toggle = !c.visible || visible_count > 1;
                                ui.add_enabled_ui(can_toggle, |ui| {
                                    ui.checkbox(&mut c.visible, &c.title);
                                });
                            }
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, self.rows.len(), |mut row| {
                    let row_idx = row.index();
                    let is_selected = self.selected_rows.contains(&row_idx);
                    row.set_selected(is_selected);

                    for &(col_idx, _w) in &visible_cols {
                        row.col(|ui| {
                            let is_editing_this = self
                                .active_edit
                                .as_ref()
                                .is_some_and(|e| e.row_idx == row_idx && e.col_idx == col_idx);

                            if is_editing_this {
                                if let Some(edit) = self.active_edit.as_mut() {
                                    let te = egui::TextEdit::singleline(&mut edit.buffer)
                                        .desired_width(ui.available_width());
                                    let resp = ui.add(te);
                                    if edit.just_opened {
                                        resp.request_focus();
                                        edit.just_opened = false;
                                    }
                                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        commit_edit = true;
                                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        cancel_edit = true;
                                    } else if resp.lost_focus() {
                                        commit_edit = true;
                                    }
                                }
                            } else {
                                let cell_text = self
                                    .rows
                                    .get(row_idx)
                                    .and_then(|r| r.cells.get(col_idx))
                                    .map(String::as_str)
                                    .unwrap_or("");

                                let text_color = if is_selected {
                                    palette.fg_bright.to_egui()
                                } else {
                                    palette.fg_main.to_egui()
                                };

                                let resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(cell_text).color(text_color),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );

                                if resp.clicked() {
                                    let modifiers = ui.input(|i| i.modifiers);
                                    if modifiers.alt && self.columns[col_idx].editable {
                                        // Alt + Click：將當前點擊列加入選取，並立即啟動多列批次流水號編輯
                                        self.selected_rows.insert(row_idx);
                                        let batch: Vec<usize> =
                                            self.selected_rows.iter().copied().collect();
                                        self.active_edit = Some(ActiveInlineEdit {
                                            row_idx,
                                            col_idx,
                                            buffer: cell_text.to_string(),
                                            batch_rows: batch,
                                            just_opened: true,
                                        });
                                    } else if modifiers.ctrl || modifiers.command {
                                        if self.selected_rows.contains(&row_idx) {
                                            self.selected_rows.remove(&row_idx);
                                        } else {
                                            self.selected_rows.insert(row_idx);
                                        }
                                    } else {
                                        self.selected_rows.clear();
                                        self.selected_rows.insert(row_idx);
                                    }
                                }

                                if resp.double_clicked() && self.columns[col_idx].editable {
                                    if !self.selected_rows.contains(&row_idx) {
                                        self.selected_rows.clear();
                                        self.selected_rows.insert(row_idx);
                                    }
                                    let batch: Vec<usize> =
                                        self.selected_rows.iter().copied().collect();
                                    self.active_edit = Some(ActiveInlineEdit {
                                        row_idx,
                                        col_idx,
                                        buffer: cell_text.to_string(),
                                        batch_rows: batch,
                                        just_opened: true,
                                    });
                                }
                            }
                        });
                    }
                });
            });

        if commit_edit {
            if let Some(edit) = self.active_edit.take() {
                self.apply_batch_inline_edit(&edit.batch_rows, edit.col_idx, &edit.buffer);
            }
        } else if cancel_edit {
            self.active_edit = None;
        }

        if let Some((col_idx, asc)) = pending_sort {
            self.sort_by_column(col_idx, asc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_batch_sequence_pattern() {
        assert_eq!(expand_batch_sequence_pattern("Track [##1]", 0), "Track [01]");
        assert_eq!(expand_batch_sequence_pattern("Track [##1]", 9), "Track [10]");
        assert_eq!(
            expand_batch_sequence_pattern("Chapter (#005)", 2),
            "Chapter (007)"
        );
        assert_eq!(expand_batch_sequence_pattern("固定文字", 3), "固定文字");
    }

    #[test]
    fn test_table_multimode_sorting_and_move() {
        let cols = vec![
            TableColumnDef::fixed("idx", "#", 40.0),
            TableColumnDef::flex("title", "標題", 120.0),
            TableColumnDef::uniform("dur", "時長", 70.0, 100.0),
        ];
        let mut table = UniversalTable::new("demo_table", cols);
        table.set_rows(vec![
            TableRow::new(1, ["1", "【02】媽媽的歌", "03:45"]),
            TableRow::new(2, ["2", "【01】爸爸的信", "02:10"]),
            TableRow::new(3, ["3", "【03】朋友", "12:00"]),
        ]);

        // 依第 1 欄（標題）執行台灣教育部注音排序（且略過開頭【02】編號）：
        // 爸爸 (ㄅ) -> 朋友 (ㄆ) -> 媽媽 (ㄇ)
        table.sort_by_column(1, true);
        assert_eq!(table.rows[0].cells[1], "【01】爸爸的信");
        assert_eq!(table.rows[1].cells[1], "【03】朋友");
        assert_eq!(table.rows[2].cells[1], "【02】媽媽的歌");

        // 依第 2 欄（時長）執行時間碼排序
        table.sort_by_column(2, true);
        assert_eq!(table.rows[0].cells[2], "02:10");
        assert_eq!(table.rows[1].cells[2], "03:45");
        assert_eq!(table.rows[2].cells[2], "12:00");

        // 測試批次流水號填充
        table.apply_batch_inline_edit(&[0, 1, 2], 1, "[##1] 精選曲目");
        assert_eq!(table.rows[0].cells[1], "[01] 精選曲目");
        assert_eq!(table.rows[1].cells[1], "[02] 精選曲目");
        assert_eq!(table.rows[2].cells[1], "[03] 精選曲目");
    }
}
