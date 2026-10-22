//! # `app_shell` — `UniversalAppShell` 全方位桌面應用程式主外殼
//!
//! 對應 Python 版 `_lib/universal_ui.py` 之 `UniversalUI` 基底架構，一站式整合：
//! - **組態持久化 (`AppConfig`)**：自動讀寫 `_cfg/<app>_cfg.json`、記憶視窗與分割軸幾何位置、支援多組態設定檔 (`Profiles`)。
//! - **試算表多語系引擎 (`I18nEngine`)**：內建 `zh_TW` / `en_US` / `zh_CN` / `ja_JP` 即時零重啟切換。
//! - **深度系統布景主題與 DWM 沉浸式標題列 (`SemanticPalette` + `SystemThemeWatcher`)**：
//!   支援「跟隨系統」、「專業深色 (`StudioDark`)」、「溫潤宣紙淺色 (`RicePaperLight`)」零延遲熱切換，並同步更新 Win10/11 標題列色彩。
//! - **全域無段 UI 縮放 (`0.75x` ～ `2.50x`)**：支援 `Ctrl + 滾輪`、`Ctrl + +/-/0`、`Ctrl + T` 主題切換、`Ctrl + ,` 偏好設定中心。
//! - **內建偏好設定中心 (`PreferencesDialog`)**、**浮動通知 (`ToastManager`)**、**語意訊息框 (`UniversalMessageBox`)**、**電源倒數 (`ScreenOffCountdownDialog`)**、**自適應常用項 (`PresetEngine`)** 與 **純綠版 `llama.cpp` 推論引擎 (`RuhAIEngine`)**。

use std::path::Path;

use crate::ai_engine::{AIStatus, RuhAIEngine};
use crate::config_store::AppConfig;
use crate::dialog_engine::{ScreenOffCountdownDialog, ToastManager, UniversalMessageBox};
use crate::i18n::I18nEngine;
use crate::preset_engine::PresetEngine;
use crate::rud_engine::calc_treeview_width;
use crate::theme_engine::{SemanticPalette, SystemThemeWatcher, ThemePreference};

#[cfg(feature = "ui")]
use crate::dialog_engine::rud_button_bar;
#[cfg(feature = "ui")]
use crate::rud_engine::{justify_short_label, two_tier_radio};
#[cfg(feature = "ui")]
use crate::theme_engine::{
    apply_palette_to_egui, apply_win32_dwm_titlebar_for_current_process, configure_cjk_fonts,
};

/// `UniversalAppShell` — ATGprjs Rust 桌面應用程式通用核心狀態外殼
#[allow(dead_code)]
pub struct UniversalAppShell {
    pub app_id: String,
    pub app_title: String,
    pub config: AppConfig,
    pub i18n: I18nEngine,
    pub theme_pref: ThemePreference,
    pub use_system_accent: bool,
    pub palette: SemanticPalette,
    pub ui_scale: f32,
    pub sidebar_width: f32,
    pub toasts: ToastManager,
    pub msgbox: UniversalMessageBox,
    pub power_countdown: ScreenOffCountdownDialog,
    pub presets: PresetEngine,
    pub ai: RuhAIEngine,
    pub show_preferences_dialog: bool,
    new_profile_name: String,
    selected_profile: String,
    cached_ai_status: Option<AIStatus>,
    theme_watcher: SystemThemeWatcher,
    fonts_initialized: bool,
    style_dirty: bool,
    dwm_sync_frames: u8,
}

impl UniversalAppShell {
    /// 建立並初始化 `UniversalAppShell`，自動載入 `_cfg/<app_id>_cfg.json` 與語系表。
    pub fn new(
        app_id: impl Into<String>,
        app_title: impl Into<String>,
        base_dir: Option<&Path>,
    ) -> Self {
        let id_str = app_id.into();
        let title_str = app_title.into();
        let sig = format!("{id_str}_v1");
        let config = match base_dir {
            Some(dir) => AppConfig::with_base_dir(&id_str, &sig, dir, None),
            None => AppConfig::new(&id_str, &sig, None),
        };

        let mut i18n = I18nEngine::new();
        let saved_lang = config.get_string("language", "");
        if saved_lang.is_empty() {
            let detected = i18n.detect_system_language();
            i18n.set_language(&detected);
        } else {
            i18n.set_language(&saved_lang);
        }

        let theme_code = config.get_string("theme", "system");
        let theme_pref = ThemePreference::from_code(&theme_code);
        let use_system_accent = config.get_bool("use_system_accent", true);
        let palette = SemanticPalette::resolve(theme_pref, use_system_accent);

        let ui_scale = (config.get_f64("ui_scale", 1.0) as f32).clamp(0.75, 2.50);
        let sidebar_width = (config.get_f64("sidebar_width", 220.0) as f32).clamp(150.0, 600.0);

        let mut presets = PresetEngine::new();
        presets.load_from_config(&config);

        Self {
            app_id: id_str,
            app_title: title_str,
            config,
            i18n,
            theme_pref,
            use_system_accent,
            palette,
            ui_scale,
            sidebar_width,
            toasts: ToastManager::new(),
            msgbox: UniversalMessageBox::new(),
            power_countdown: ScreenOffCountdownDialog::new(),
            presets,
            ai: RuhAIEngine::new(),
            show_preferences_dialog: false,
            new_profile_name: String::new(),
            selected_profile: String::new(),
            cached_ai_status: None,
            theme_watcher: SystemThemeWatcher::default(),
            fonts_initialized: false,
            style_dirty: true,
            dwm_sync_frames: 4,
        }
    }

    /// 翻譯查詢快捷方法 (`t`)
    pub fn t(&self, key: &str, default: &str) -> String {
        self.i18n.t(key, default).to_string()
    }

    /// 帶參數之翻譯查詢快捷方法 (`tf`)
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.i18n.tf(key, args)
    }

    /// 即時切換布景主題偏好（零重啟立即重繪，並同步更新 Win32 DWM 標題列）
    pub fn set_theme_preference(&mut self, pref: ThemePreference) {
        self.theme_pref = pref;
        self.palette = SemanticPalette::resolve(self.theme_pref, self.use_system_accent);
        self.config.set("theme", pref.as_code());
        let _ = self.config.save();
        self.style_dirty = true;
        self.dwm_sync_frames = 3;
    }

    /// 即時切換介面語系（零重啟）
    pub fn set_language(&mut self, lang_code: &str) {
        self.i18n.set_language(lang_code);
        self.config.set("language", self.i18n.get_language());
        let _ = self.config.save();
    }

    /// 即時設定全域 UI 縮放倍率 (`0.75` ～ `2.50`)
    pub fn set_ui_scale(&mut self, scale: f32) {
        let clamped = ((scale.clamp(0.75, 2.50) * 20.0).round()) / 20.0;
        if (clamped - self.ui_scale).abs() > 0.001 {
            self.ui_scale = clamped;
            self.config.set("ui_scale", f64::from(clamped));
            let _ = self.config.save();
            self.style_dirty = true;
        }
    }

    /// 根據側邊欄樹狀導覽項目清單，動態精算永不截字之最小側邊欄寬度 (`calc_treeview_width`)
    pub fn ensure_sidebar_min_width(&mut self, items: &[(usize, &str)]) -> f32 {
        let half_char_px = 7.2 * self.ui_scale;
        let min_w = calc_treeview_width(items, half_char_px, self.ui_scale);
        if self.sidebar_width < min_w {
            self.sidebar_width = min_w;
        }
        min_w
    }

    /// 儲存目前所有狀態（含分割軸寬度與自適應常用項）至設定檔
    pub fn save_state(&mut self) {
        self.config
            .set("sidebar_width", f64::from(self.sidebar_width));
        self.presets.save_to_config(&mut self.config);
        let _ = self.config.save();
    }

    /// 在每個 `eframe::App::update` 週期開頭呼叫：
    /// 1. 確保 CJK 字型與 `SemanticPalette` 已注入 `egui::Context`。
    /// 2. 處理 Windows DWM 沉浸式暗色標題列同步。
    /// 3. 監聽系統深淺色變更與全域快捷鍵（`Ctrl + +/-/0/滾輪`、`Ctrl + T`、`Ctrl + ,`）。
    #[cfg(feature = "ui")]
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        if !self.fonts_initialized {
            configure_cjk_fonts(ctx);
            self.fonts_initialized = true;
            self.style_dirty = true;
        }

        // 若處於「跟隨系統」模式，定期偵測作業系統主題或強調色是否變更
        if self.theme_pref == ThemePreference::System && self.theme_watcher.poll_changed() {
            self.palette = SemanticPalette::resolve(self.theme_pref, self.use_system_accent);
            self.style_dirty = true;
            self.dwm_sync_frames = 2;
        }

        // 處理全域快捷鍵：Ctrl + 滾輪 / +/-/0 縮放、Ctrl + T 切換主題、Ctrl + , 偏好設定
        let mut scale_delta: f32 = 0.0;
        let mut reset_scale = false;
        let mut toggle_theme = false;
        let mut open_prefs = false;

        ctx.input(|input| {
            let ctrl = input.modifiers.ctrl || input.modifiers.command;
            if ctrl {
                let zoom = input.zoom_delta();
                if (zoom - 1.0).abs() > 0.01 {
                    scale_delta = if zoom > 1.0 { 0.10 } else { -0.10 };
                } else if input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)
                {
                    scale_delta = 0.10;
                } else if input.key_pressed(egui::Key::Minus) {
                    scale_delta = -0.10;
                } else if input.key_pressed(egui::Key::Num0) {
                    reset_scale = true;
                } else if input.key_pressed(egui::Key::T) {
                    toggle_theme = true;
                } else if input.key_pressed(egui::Key::Comma) {
                    open_prefs = true;
                }
            }
        });

        if reset_scale {
            self.set_ui_scale(1.0);
            let msg = format!(
                "{}: 100%",
                self.t("pref_ui_scale", "介面字體與控制項縮放")
            );
            self.toasts.info(msg);
        } else if scale_delta.abs() > 0.001 {
            let next = (self.ui_scale + scale_delta).clamp(0.75, 2.50);
            self.set_ui_scale(next);
            let pct = (self.ui_scale * 100.0).round() as i32;
            let msg = format!(
                "{}: {}%",
                self.t("pref_ui_scale", "介面字體與控制項縮放"),
                pct
            );
            self.toasts.info(msg);
        }

        if toggle_theme {
            let next_pref = self.theme_pref.toggle_dark_light(self.palette.is_dark);
            self.set_theme_preference(next_pref);
        }

        if open_prefs {
            self.show_preferences_dialog = true;
        }

        if self.style_dirty {
            apply_palette_to_egui(ctx, &self.palette, self.ui_scale);
            self.style_dirty = false;
        }

        if self.dwm_sync_frames > 0 {
            apply_win32_dwm_titlebar_for_current_process(&self.palette);
            self.dwm_sync_frames -= 1;
        }
    }

    /// 在每個 `eframe::App::update` 週期結尾呼叫：
    /// 統一繪製內建偏好設定中心、電源關閉倒數對話框、語意訊息框與右下角浮動通知。
    #[cfg(feature = "ui")]
    pub fn end_frame(&mut self, ctx: &egui::Context) {
        if self.show_preferences_dialog {
            self.render_preferences_dialog(ctx);
        }
        self.power_countdown
            .render(ctx, &self.palette, &self.i18n);
        self.msgbox.render(ctx, &self.palette, &self.i18n);
        self.toasts.render(ctx, &self.palette);
    }

    /// 繪製內建之全域偏好設定中心 (`PreferencesDialog`)
    #[cfg(feature = "ui")]
    fn render_preferences_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_preferences_dialog;
        let dlg_title = self.t("pref_title", "⚙ 系統偏好與外觀設定中心");
        let palette = self.palette;

        let mut next_theme = self.theme_pref;
        let mut theme_changed = false;
        let mut next_accent = self.use_system_accent;
        let mut accent_changed = false;
        let mut next_lang = self.i18n.get_language().to_string();
        let mut lang_changed = false;
        let mut next_scale = self.ui_scale;
        let mut scale_changed = false;
        let mut reset_dialogs_clicked = false;
        let mut probe_ai_clicked = false;
        let mut close_clicked = false;

        egui::Window::new(dlg_title)
            .id(egui::Id::new("atg_preferences_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(egui::vec2(540.0 * self.ui_scale, 520.0 * self.ui_scale))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // 1. 外觀主題區 (採用 two_tier_radio 雙層排版，點擊立即換膚)
                    ui.heading(self.t("pref_section_theme", "🎨 外觀主題與沉浸式視覺"));
                    ui.add_space(4.0);

                    if two_tier_radio(
                        ui,
                        &mut next_theme,
                        ThemePreference::System,
                        &self.t("pref_theme_system", "跟隨作業系統 (Follow System)"),
                        &self.t(
                            "pref_theme_system_desc",
                            "自動偵測 Windows 深淺色設定與個人化強調色，背景變更時即時無感同步。",
                        ),
                        &palette,
                    ) {
                        theme_changed = true;
                    }
                    ui.add_space(4.0);

                    if two_tier_radio(
                        ui,
                        &mut next_theme,
                        ThemePreference::Dark,
                        &self.t("pref_theme_dark", "專業暗色 (Studio Dark)"),
                        &self.t(
                            "pref_theme_dark_desc",
                            "採用深邃炭灰基底與柔和對比文字，搭配 Windows DWM 沉浸式暗黑標題列，適合長時間專注作業。",
                        ),
                        &palette,
                    ) {
                        theme_changed = true;
                    }
                    ui.add_space(4.0);

                    if two_tier_radio(
                        ui,
                        &mut next_theme,
                        ThemePreference::Light,
                        &self.t("pref_theme_light", "溫潤宣紙淺色 (Rice-Paper Light)"),
                        &self.t(
                            "pref_theme_light_desc",
                            "採用暖調宣紙米白取代刺眼純白 (#F6F5F0)，在明亮環境下維持清晰舒適的閱讀體驗。",
                        ),
                        &palette,
                    ) {
                        theme_changed = true;
                    }
                    ui.add_space(6.0);

                    if ui
                        .checkbox(
                            &mut next_accent,
                            self.t("pref_use_sys_accent", "自動融入 Windows 系統個人化強調色"),
                        )
                        .changed()
                    {
                        accent_changed = true;
                    }

                    ui.add_space(10.0);
                    ui.separator();

                    // 2. 語言與 UI 縮放區 (短漢字標籤採用 justify_short_label 雙端齊行)
                    ui.heading(self.t("pref_section_lang_scale", "🗚 介面語系與全視窗等比縮放"));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(justify_short_label(
                            &self.t("pref_lang_label", "介面語系："),
                            4,
                        ));
                        let lang_options = [
                            ("zh_TW", "繁體中文 (台灣)"),
                            ("en_US", "English (US)"),
                            ("zh_CN", "简体中文"),
                            ("ja_JP", "日本語"),
                        ];
                        for (code, display) in lang_options {
                            if ui
                                .selectable_value(&mut next_lang, code.to_string(), display)
                                .changed()
                            {
                                lang_changed = true;
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(justify_short_label(
                            &self.t("pref_scale_label", "字體縮放："),
                            4,
                        ));
                        let slider = egui::Slider::new(&mut next_scale, 0.75..=2.50)
                            .step_by(0.05)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                        if ui.add(slider).changed() {
                            scale_changed = true;
                        }
                        if ui.button("100% (Ctrl+0)").clicked() {
                            next_scale = 1.0;
                            scale_changed = true;
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();

                    // 3. 純綠版 llama.cpp 離線 AI 狀態探測
                    ui.heading("🧠 純綠版 llama.cpp 本地 AI 推論狀態");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("🔍 重新探測本地 GGUF 與 llama-server").clicked() {
                            probe_ai_clicked = true;
                        }
                        if let Some(st) = self.cached_ai_status {
                            let badge = if st == AIStatus::Online {
                                format!(
                                    "🟢 在線就緒 | 文字模型: {} | 本地 GGUF 數: {}",
                                    self.ai.active_chat_model.as_deref().unwrap_or("自動遴選"),
                                    self.ai.available_models.len()
                                )
                            } else {
                                format!(
                                    "⚪ 離線純演算法模式 | 本地已發現 {} 個 .gguf 模型",
                                    self.ai.available_models.len()
                                )
                            };
                            ui.label(badge);
                        } else {
                            ui.label(
                                egui::RichText::new("尚未探測（點擊左側按鈕檢查免安裝綠版環境）")
                                    .color(palette.fg_muted.to_egui()),
                            );
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();

                    // 4. 視窗幾何重設與多組態設定檔 (Profiles)
                    ui.heading("🗂 視窗記憶重設與多組態管理 (Profiles)");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(self.t(
                                "pref_btn_reset_dialogs",
                                "🗔 重設所有子視窗位置與大小",
                            ))
                            .clicked()
                        {
                            reset_dialogs_clicked = true;
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(justify_short_label("組態名稱：", 4));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_profile_name)
                                .hint_text("輸入新組態名稱...")
                                .desired_width(150.0),
                        );
                        if ui.button("💾 儲存組態").clicked()
                            && !self.new_profile_name.trim().is_empty()
                        {
                            let name = self.new_profile_name.trim().to_string();
                            let _ = self.config.save_profile(&name);
                            self.selected_profile = name.clone();
                            self.new_profile_name.clear();
                            self.toasts.success(format!("已儲存設定組態：{name}"));
                        }
                    });

                    let profiles = self.config.list_profiles();
                    if !profiles.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(justify_short_label("快速套用：", 4));
                            for p in profiles {
                                if ui.button(format!("📂 {p}")).clicked()
                                    && self.config.load_profile(&p).unwrap_or(false)
                                {
                                    let t_code = self.config.get_string("theme", "system");
                                    next_theme = ThemePreference::from_code(&t_code);
                                    theme_changed = true;
                                    next_scale = (self.config.get_f64("ui_scale", 1.0) as f32)
                                        .clamp(0.75, 2.50);
                                    scale_changed = true;
                                    self.toasts.info(format!("已套用設定組態：{p}"));
                                }
                            }
                        });
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                let factory_label = self.t("pref_btn_factory_reset", "↺ 還原出廠預設值");
                let close_label = format!("  {}  ", self.t("btn_close", "關閉"));
                let mut factory_reset_clicked = false;
                rud_button_bar(
                    ui,
                    |ui| {
                        if ui.button(factory_label).clicked() {
                            factory_reset_clicked = true;
                        }
                    },
                    |ui| {
                        if ui
                            .button(egui::RichText::new(close_label).strong())
                            .clicked()
                        {
                            close_clicked = true;
                        }
                    },
                );
                if factory_reset_clicked {
                    let _ = self.config.restore_factory_defaults();
                    next_theme = ThemePreference::System;
                    theme_changed = true;
                    next_scale = 1.0;
                    scale_changed = true;
                    self.toasts.warning("已還原出廠預設值");
                }
            });

        if close_clicked {
            open = false;
        }
        self.show_preferences_dialog = open;

        if accent_changed {
            self.use_system_accent = next_accent;
            self.config.set("use_system_accent", next_accent);
            theme_changed = true;
        }
        if theme_changed {
            self.set_theme_preference(next_theme);
        }
        if lang_changed {
            self.set_language(&next_lang);
        }
        if scale_changed {
            self.set_ui_scale(next_scale);
        }
        if reset_dialogs_clicked {
            self.config.reset_all_dialog_geometries();
            self.toasts
                .success(self.t("msg_dialogs_reset", "已重設所有子視窗位置與大小記憶"));
        }
        if probe_ai_clicked {
            let st = self.ai.refresh(true);
            self.cached_ai_status = Some(st);
        }
    }
}
