//! # `dialog_engine` — 沉浸式對話框、非阻塞浮動通知 (`ToastManager`) 與電源倒數計時器
//!
//! 對應 Python 版 `_lib/universal_ui.py` 之對話框子系統，提供：
//! 1. **非阻塞式浮動通知 (`ToastManager`)**：右下角自動消退通知（`Info` / `Success` / `Warning` / `Error`）。
//! 2. **語意調色盤訊息框 (`UniversalMessageBox`)**：取代刺眼白底原生 `MessageBox`，支援 `Enter`/`Esc`/`Y`/`N` 鍵盤導覽與母視窗置中。
//! 3. **電源關閉倒數對話框 (`ScreenOffCountdownDialog`)**：執行關閉螢幕或睡眠前彈出 30 秒倒數緩衝，提供「延長 5 分鐘」、「立即關閉螢幕 (`Enter`)」與「取消 (`Esc`)」。
//! 4. **標準對話框底欄 (`rud_button_bar`)**：左群組（次要/重設）與右群組（確定/取消）自動對齊且不擠壓。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[cfg(feature = "ui")]
use crate::i18n::I18nEngine;
#[cfg(feature = "ui")]
use crate::power_engine;
#[cfg(feature = "ui")]
use crate::rud_engine::cjk_smart_wrap;
use crate::theme_engine::{ColorRgb, SemanticPalette};

/// 浮動通知等級
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Success => "✔",
            Self::Warning => "⚠",
            Self::Error => "✖",
        }
    }

    pub fn border_color(self, palette: &SemanticPalette) -> ColorRgb {
        match self {
            Self::Info => palette.accent,
            Self::Success => palette.success,
            Self::Warning => palette.warning,
            Self::Error => palette.error,
        }
    }
}

/// 單筆浮動通知項目
#[derive(Debug, Clone)]
pub struct ToastItem {
    pub id: u64,
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
    pub duration: Duration,
}

impl ToastItem {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    pub fn remaining_ratio(&self) -> f32 {
        let elapsed = self.created_at.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32().max(0.1);
        (1.0 - elapsed / total).clamp(0.0, 1.0)
    }
}

/// 非阻塞式右下角浮動通知管理器 (`ToastManager`)
#[derive(Debug, Default)]
pub struct ToastManager {
    queue: VecDeque<ToastItem>,
    next_id: u64,
}

impl ToastManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 推入一筆浮動通知（預設持續 2800 毫秒）
    pub fn show(&mut self, message: impl Into<String>, level: ToastLevel, duration_ms: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.queue.push_back(ToastItem {
            id,
            message: message.into(),
            level,
            created_at: Instant::now(),
            duration: Duration::from_millis(duration_ms.max(500)),
        });
        while self.queue.len() > 5 {
            self.queue.pop_front();
        }
        id
    }

    pub fn info(&mut self, message: impl Into<String>) -> u64 {
        self.show(message, ToastLevel::Info, 2800)
    }

    pub fn success(&mut self, message: impl Into<String>) -> u64 {
        self.show(message, ToastLevel::Success, 2800)
    }

    pub fn warning(&mut self, message: impl Into<String>) -> u64 {
        self.show(message, ToastLevel::Warning, 3500)
    }

    pub fn error(&mut self, message: impl Into<String>) -> u64 {
        self.show(message, ToastLevel::Error, 4500)
    }

    /// 清除已逾時的通知，並回傳目前活動通知清單
    pub fn active_toasts(&mut self) -> &VecDeque<ToastItem> {
        self.queue.retain(|item| !item.is_expired());
        &self.queue
    }

    /// 在 `egui::Context` 右下角繪製浮動通知堆疊
    #[cfg(feature = "ui")]
    pub fn render(&mut self, ctx: &egui::Context, palette: &SemanticPalette) {
        self.queue.retain(|item| !item.is_expired());
        if self.queue.is_empty() {
            return;
        }

        // 持續要求重繪以更新倒數進度與自動消退
        ctx.request_repaint_after(Duration::from_millis(50));

        let screen_rect = ctx.screen_rect();
        let mut y_offset = screen_rect.max.y - 20.0;
        let mut dismiss_id: Option<u64> = None;

        for item in self.queue.iter().rev() {
            let border_c = item.level.border_color(palette).to_egui();
            let area_id = egui::Id::new(("atg_toast", item.id));
            let toast_width = 340.0_f32.min(screen_rect.width() - 32.0).max(200.0);
            let pos = egui::pos2(screen_rect.max.x - toast_width - 20.0, y_offset - 56.0);

            let resp = egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    egui::Frame::window(ui.style())
                        .fill(palette.bg_elevated.to_egui())
                        .stroke(egui::Stroke::new(1.5_f32, border_c))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_min_width(toast_width - 28.0);
                            ui.set_max_width(toast_width - 28.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(item.level.icon())
                                        .strong()
                                        .size(16.0)
                                        .color(border_c),
                                );
                                ui.add_space(6.0);
                                let wrapped =
                                    cjk_smart_wrap(&item.message, toast_width - 68.0, 7.5);
                                ui.label(
                                    egui::RichText::new(wrapped).color(palette.fg_bright.to_egui()),
                                );
                            });
                        })
                        .response
                });

            if resp.inner.clicked() {
                dismiss_id = Some(item.id);
            }
            y_offset -= resp.response.rect.height() + 10.0;
        }

        if let Some(id) = dismiss_id {
            self.queue.retain(|t| t.id != id);
        }
    }
}

/// 對話框圖示類型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}

impl MessageBoxIcon {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Warning => "⚠",
            Self::Error => "✖",
            Self::Question => "❓",
        }
    }

    pub fn color(self, palette: &SemanticPalette) -> ColorRgb {
        match self {
            Self::Info | Self::Question => palette.accent,
            Self::Warning => palette.warning,
            Self::Error => palette.error,
        }
    }
}

/// 對話框按鈕組合
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBoxButtons {
    Ok,
    OkCancel,
    YesNo,
}

/// 對話框使用者回應結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBoxResult {
    Ok,
    Cancel,
    Yes,
    No,
}

/// 單一待決之訊息對話框請求
#[derive(Debug, Clone)]
pub struct MessageBoxRequest {
    pub id: String,
    pub title: String,
    pub message: String,
    pub icon: MessageBoxIcon,
    pub buttons: MessageBoxButtons,
}

/// 已完成的對話框回應紀錄
#[derive(Debug, Clone)]
pub struct CompletedMessageBox {
    pub id: String,
    pub result: MessageBoxResult,
}

/// 語意調色盤全域訊息對話框佇列 (`UniversalMessageBox`)
#[derive(Debug, Default)]
pub struct UniversalMessageBox {
    pending: VecDeque<MessageBoxRequest>,
    completed: VecDeque<CompletedMessageBox>,
}

impl UniversalMessageBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_info(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.pending.push_back(MessageBoxRequest {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            icon: MessageBoxIcon::Info,
            buttons: MessageBoxButtons::Ok,
        });
    }

    pub fn show_warning(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.pending.push_back(MessageBoxRequest {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            icon: MessageBoxIcon::Warning,
            buttons: MessageBoxButtons::Ok,
        });
    }

    pub fn show_error(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.pending.push_back(MessageBoxRequest {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            icon: MessageBoxIcon::Error,
            buttons: MessageBoxButtons::Ok,
        });
    }

    pub fn ask_yes_no(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.pending.push_back(MessageBoxRequest {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            icon: MessageBoxIcon::Question,
            buttons: MessageBoxButtons::YesNo,
        });
    }

    pub fn ask_ok_cancel(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.pending.push_back(MessageBoxRequest {
            id: id.into(),
            title: title.into(),
            message: message.into(),
            icon: MessageBoxIcon::Question,
            buttons: MessageBoxButtons::OkCancel,
        });
    }

    /// 取出指定 `id` 之對話框結果（若使用者已點擊按鈕）
    pub fn take_result(&mut self, id: &str) -> Option<MessageBoxResult> {
        if let Some(pos) = self.completed.iter().position(|c| c.id == id) {
            self.completed.remove(pos).map(|c| c.result)
        } else {
            None
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 在 `egui::Context` 繪製頂層模態訊息框，支援 `Enter`、`Esc`、`Y`、`N` 鍵盤導覽。
    #[cfg(feature = "ui")]
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        palette: &SemanticPalette,
        i18n: &I18nEngine,
    ) {
        let Some(req) = self.pending.front().cloned() else {
            return;
        };

        let mut chosen: Option<MessageBoxResult> = None;

        // 鍵盤快捷鍵導覽
        ctx.input(|input| {
            if input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space) {
                chosen = Some(match req.buttons {
                    MessageBoxButtons::Ok | MessageBoxButtons::OkCancel => MessageBoxResult::Ok,
                    MessageBoxButtons::YesNo => MessageBoxResult::Yes,
                });
            } else if input.key_pressed(egui::Key::Escape) {
                chosen = Some(match req.buttons {
                    MessageBoxButtons::Ok => MessageBoxResult::Ok,
                    MessageBoxButtons::OkCancel => MessageBoxResult::Cancel,
                    MessageBoxButtons::YesNo => MessageBoxResult::No,
                });
            } else if req.buttons == MessageBoxButtons::YesNo {
                if input.key_pressed(egui::Key::Y) {
                    chosen = Some(MessageBoxResult::Yes);
                } else if input.key_pressed(egui::Key::N) {
                    chosen = Some(MessageBoxResult::No);
                }
            }
        });

        let screen = ctx.screen_rect();
        let modal_w = 420.0_f32.min(screen.width() - 40.0).max(280.0);

        egui::Window::new(&req.title)
            .id(egui::Id::new(("atg_msgbox", &req.id)))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(egui::vec2(modal_w, 160.0))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let icon_color = req.icon.color(palette).to_egui();
                    ui.label(
                        egui::RichText::new(req.icon.symbol())
                            .size(28.0)
                            .strong()
                            .color(icon_color),
                    );
                    ui.add_space(10.0);
                    let wrapped = cjk_smart_wrap(&req.message, modal_w - 72.0, 7.5);
                    ui.label(
                        egui::RichText::new(wrapped)
                            .size(14.5)
                            .color(palette.fg_main.to_egui()),
                    );
                });

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(6.0);

                rud_button_bar(
                    ui,
                    |_ui| {},
                    |ui| match req.buttons {
                        MessageBoxButtons::Ok => {
                            let ok_text = format!("  {} (Enter)  ", i18n.t("btn_ok", "確定"));
                            if ui.button(egui::RichText::new(ok_text).strong()).clicked() {
                                chosen = Some(MessageBoxResult::Ok);
                            }
                        }
                        MessageBoxButtons::OkCancel => {
                            let ok_text = format!("  {} (Enter)  ", i18n.t("btn_ok", "確定"));
                            let cancel_text =
                                format!("  {} (Esc)  ", i18n.t("btn_cancel", "取消"));
                            if ui.button(egui::RichText::new(ok_text).strong()).clicked() {
                                chosen = Some(MessageBoxResult::Ok);
                            }
                            if ui.button(cancel_text).clicked() {
                                chosen = Some(MessageBoxResult::Cancel);
                            }
                        }
                        MessageBoxButtons::YesNo => {
                            let yes_text = format!("  {} (Y)  ", i18n.t("btn_yes", "是"));
                            let no_text = format!("  {} (N)  ", i18n.t("btn_no", "否"));
                            if ui.button(egui::RichText::new(yes_text).strong()).clicked() {
                                chosen = Some(MessageBoxResult::Yes);
                            }
                            if ui.button(no_text).clicked() {
                                chosen = Some(MessageBoxResult::No);
                            }
                        }
                    },
                );
            });

        if let Some(res) = chosen {
            self.pending.pop_front();
            self.completed.push_back(CompletedMessageBox {
                id: req.id,
                result: res,
            });
        }
    }
}

/// 關閉螢幕前之緩衝倒數對話框 (`ScreenOffCountdownDialog`)
#[derive(Debug, Clone)]
pub struct ScreenOffCountdownDialog {
    pub active: bool,
    pub lock_workstation: bool,
    total_seconds: u32,
    deadline: Option<Instant>,
}

impl Default for ScreenOffCountdownDialog {
    fn default() -> Self {
        Self {
            active: false,
            lock_workstation: false,
            total_seconds: 30,
            deadline: None,
        }
    }
}

impl ScreenOffCountdownDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// 啟動關閉螢幕倒數（預設 30 秒）
    pub fn start(&mut self, seconds: u32, lock_workstation: bool) {
        let secs = seconds.max(5);
        self.total_seconds = secs;
        self.lock_workstation = lock_workstation;
        self.deadline = Some(Instant::now() + Duration::from_secs(u64::from(secs)));
        self.active = true;
    }

    /// 延長倒數時間（例如 +300 秒 = 5 分鐘）
    pub fn extend(&mut self, extra_seconds: u32) {
        if let Some(dl) = self.deadline {
            let new_dl = dl + Duration::from_secs(u64::from(extra_seconds));
            self.total_seconds += extra_seconds;
            self.deadline = Some(new_dl);
        }
    }

    /// 取消關閉螢幕倒數
    pub fn cancel(&mut self) {
        self.active = false;
        self.deadline = None;
    }

    /// 計算剩餘秒數
    pub fn remaining_seconds(&self) -> u32 {
        let Some(dl) = self.deadline else {
            return 0;
        };
        let now = Instant::now();
        if dl <= now {
            0
        } else {
            dl.duration_since(now).as_secs() as u32 + 1
        }
    }

    /// 繪製關閉螢幕倒數計時器對話框，若倒數歸零或使用者按「立即關閉」則呼叫 `power_engine::turn_off_screen`。
    #[cfg(feature = "ui")]
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        palette: &SemanticPalette,
        i18n: &I18nEngine,
    ) {
        if !self.active {
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(200));
        let rem = self.remaining_seconds();
        let mut trigger_now = rem == 0;
        let mut cancel_now = false;
        let mut extend_5m = false;

        ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                cancel_now = true;
            } else if input.key_pressed(egui::Key::Enter) {
                trigger_now = true;
            }
        });

        let title = i18n.t("power_countdown_title", "準備關閉顯示器");
        egui::Window::new(title)
            .id(egui::Id::new("atg_screen_off_countdown"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(egui::vec2(440.0, 210.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    let msg = i18n.tf(
                        "power_countdown_msg",
                        &[("sec", &rem.to_string())],
                    );
                    ui.label(
                        egui::RichText::new(msg)
                            .size(16.0)
                            .strong()
                            .color(palette.fg_bright.to_egui()),
                    );
                    ui.add_space(10.0);
                    let progress = (rem as f32 / (self.total_seconds.max(1) as f32)).clamp(0.0, 1.0);
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .text(format!("{rem} s"))
                            .animate(true),
                    );
                    ui.add_space(16.0);
                });

                ui.separator();
                ui.add_space(6.0);

                rud_button_bar(
                    ui,
                    |ui| {
                        let ext_label =
                            i18n.t("power_btn_extend_5m", "⏳ 延長 5 分鐘 (+5m)");
                        if ui.button(ext_label).clicked() {
                            extend_5m = true;
                        }
                    },
                    |ui| {
                        let now_label =
                            i18n.t("power_btn_off_now", "🌙 立即關閉螢幕 (Enter)");
                        let cancel_label =
                            format!("{} (Esc)", i18n.t("btn_cancel", "取消"));
                        if ui
                            .button(egui::RichText::new(now_label).strong())
                            .clicked()
                        {
                            trigger_now = true;
                        }
                        if ui.button(cancel_label).clicked() {
                            cancel_now = true;
                        }
                    },
                );
            });

        if extend_5m {
            self.extend(300);
        } else if cancel_now {
            self.cancel();
        } else if trigger_now {
            let lock = self.lock_workstation;
            self.cancel();
            let _ = power_engine::turn_off_screen(lock, 450);
        }
    }
}

/// 標準對話框底部按鈕列 (`rud_button_bar`)：
/// 左側放置次要/重設按鈕，中間自動彈性撐開，右側放置主要確認與取消按鈕。
#[cfg(feature = "ui")]
pub fn rud_button_bar(
    ui: &mut egui::Ui,
    left_buttons: impl FnOnce(&mut egui::Ui),
    right_buttons: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        left_buttons(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            right_buttons(ui);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_manager_lifecycle() {
        let mut tm = ToastManager::new();
        let id1 = tm.info("測試訊息 1");
        let id2 = tm.success("儲存成功");
        assert!(id2 > id1);
        assert_eq!(tm.active_toasts().len(), 2);
    }

    #[test]
    fn test_message_box_queue() {
        let mut mb = UniversalMessageBox::new();
        mb.ask_yes_no("confirm_del", "確認刪除", "是否確定刪除此筆設定？");
        assert!(mb.has_pending());
        assert!(mb.take_result("confirm_del").is_none());
    }

    #[test]
    fn test_screen_off_countdown_state() {
        let mut dlg = ScreenOffCountdownDialog::new();
        assert!(!dlg.active);
        dlg.start(30, false);
        assert!(dlg.active);
        assert!(dlg.remaining_seconds() >= 29);
        dlg.extend(300);
        assert!(dlg.remaining_seconds() >= 325);
        dlg.cancel();
        assert!(!dlg.active);
    }
}
