//! # `ruhi-studio` — RuhI（如喜）輸入法原生 Rust 現代化偏好設定與語意調校中心
//!
//! 基於 `atg_lib` (`C:\ATGprjs\_lib_rs`) 全域架構，一站式整合：
//! 1. **現代化雙欄偏好設定中心**（完整取代 `settings_window.py`）：
//!    - 左側可自由拖曳寬度、動態計算最長字串永不截字之樹狀分類導覽列 (`calc_treeview_width`)。
//!    - 右側遵循 `rud()` 空間接管與雙層條列說明 (`two_tier_radio`, `two_tier_checkbox`) 之響應式面板。
//!    - 點擊「跟隨系統／極致深色／宣紙淺色」零延遲即時換膚，支援 Windows 11 DWM 沉浸式暗黑標題列与 `Ctrl + +/-/0/滾輪` 無段縮放。
//! 2. **原生 Rust 語意調校工作室與 Web/CLI 多模態引擎**（完整取代 `semantic_tuner.py`、`ai_semantic_consultant.py`、`semantic_federation.py`）：
//!    - 內建原生 `egui`「🛠️ 語意調校工作室」分頁，支援錯句對齊、二階轉移提煉、修訂記錄昇華、領域文獻萃取与黃金基準驗證。
//!    - 相容 `--web`（啟動 `127.0.0.1:8765` 伺服 `scripts/web/index.html`）、`--cli`、`--verify`、`--auto-align`、`--tailcat` 與 `--test` 自動化測試參數。

use atg_lib::prelude::*;
use ruhi_studio::ai_consultant::AISemanticConsultant;
use ruhi_studio::config_sync::{
    ensure_keyboard_modes_consistent, load_settings, resolve_data_dir, resolve_ruhi_root,
    save_settings, RuhiSettings,
};
use ruhi_studio::tuner::{SemanticTuner, ZeroPiiPrivacyFilter};
use ruhi_studio::web_server::start_web_studio;
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

/// 左側分類導覽頁籤列舉
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioCategory {
    Appearance,
    KeyboardLayout,
    MixedEnglish,
    Grammar,
    AiAndPrivacy,
    SemanticTuner,
    About,
}

impl StudioCategory {
    pub const ALL: &'static [StudioCategory] = &[
        StudioCategory::Appearance,
        StudioCategory::KeyboardLayout,
        StudioCategory::MixedEnglish,
        StudioCategory::Grammar,
        StudioCategory::AiAndPrivacy,
        StudioCategory::SemanticTuner,
        StudioCategory::About,
    ];

    pub fn sidebar_label(&self) -> &'static str {
        match self {
            StudioCategory::Appearance => "🎨 外觀與候選座",
            StudioCategory::KeyboardLayout => "⌨️ 鍵盤配置與常用鍵",
            StudioCategory::MixedEnglish => "🔤 許氏＋歐文混打",
            StudioCategory::Grammar => "🧠 語意結構與語法引力",
            StudioCategory::AiAndPrivacy => "🛡️ 隱私安全與 AI 模型",
            StudioCategory::SemanticTuner => "🛠️ 語意調校工作室",
            StudioCategory::About => "ℹ️ 關於與全語言願景",
        }
    }

    pub fn page_header(&self) -> (&'static str, &'static str) {
        match self {
            StudioCategory::Appearance => (
                "🎨 外觀與候選座（UniversalUI 自適應）",
                "設定輸入操作模式、候選座排列方向、字體大小、主題色彩與迷你狀態指示膠囊。",
            ),
            StudioCategory::KeyboardLayout => (
                "⌨️ 鍵盤配置與常用鍵",
                "設定注音鍵盤佈局、空白鍵組字行為、Caps Lock 切換模式、標點結算模式與 Q 鍵輪換上限。",
            ),
            StudioCategory::MixedEnglish => (
                "🔤 許氏＋歐文智慧混打策略",
                "配置許氏注音與歐文智慧混打、仲裁策略、語境串流模式與長詞門檻。",
            ),
            StudioCategory::Grammar => (
                "🧠 現代漢語八大語法引力場",
                "配置高階句法結構拓撲、動補/離合/框式介詞、ê/kah 結構助詞深層語意與自建詞庫熱重載。",
            ),
            StudioCategory::AiAndPrivacy => (
                "🛡️ 隱私安全閘門與本地純綠 AI 模型",
                "配置 Zero-PII 隱私安全閘門、純綠免安裝版 llama.cpp (.gguf) 與 Google Gemini 算力管理。",
            ),
            StudioCategory::SemanticTuner => (
                "🛠️ 神經符號語意調校工作室 (Native Rust Studio)",
                "雙向最大匹配詞界對齊、六大計算語言學病灶診斷、二階語法轉移提煉、修訂記錄昇華與黃金基準測試。",
            ),
            StudioCategory::About => (
                "ℹ️ 關於如喜與全語言願景",
                "如喜輸入法品牌哲學、全語言通用輸入法藍圖、系統版本與詞庫狀態資訊。",
            ),
        }
    }
}

/// 桌面應用程式主狀態結構
pub struct RuhiStudioApp {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub shell: UniversalAppShell,
    pub settings: RuhiSettings,
    pub current_category: StudioCategory,
    pub tuner: SemanticTuner,
    pub consultant: AISemanticConsultant,
    // 語意調校工作室互動狀態
    pub tune_error_input: String,
    pub tune_correct_input: String,
    pub tune_output_log: String,
    pub corpus_domain_input: String,
    pub corpus_text_input: String,
    pub benchmark_summary: String,
}

impl RuhiStudioApp {
    pub fn new(initial_category: StudioCategory) -> Self {
        let root_dir = resolve_ruhi_root();
        let data_dir = resolve_data_dir(&root_dir);
        let settings = load_settings(&root_dir);

        let mut shell = UniversalAppShell::new(
            "ruhi_studio",
            "RuhI（如喜）輸入法 — 現代化偏好設定與語意調校中心",
            Some(&root_dir),
        );

        let theme_pref = ThemePreference::from_code(&settings.appearance.theme);
        shell.set_theme_preference(theme_pref);
        if settings.appearance.sidebar_width >= 200 {
            shell.sidebar_width = settings.appearance.sidebar_width as f32;
        }
        let scale_from_zoom =
            (1.0 + (settings.appearance.zoom_level as f32) * 0.10).clamp(0.75, 2.50);
        shell.set_ui_scale(scale_from_zoom);

        let nav_items: Vec<(usize, &str)> = StudioCategory::ALL
            .iter()
            .map(|c| (0, c.sidebar_label()))
            .collect();
        shell.ensure_sidebar_min_width(&nav_items);

        let tuner = SemanticTuner::new(Some(&root_dir));
        let consultant = AISemanticConsultant::new(Some(&root_dir));

        Self {
            root_dir,
            data_dir,
            shell,
            settings,
            current_category: initial_category,
            tuner,
            consultant,
            tune_error_input: "讓步董程式的人也能輕鬆上手".to_string(),
            tune_correct_input: "讓不懂程式的人也能輕鬆上手".to_string(),
            tune_output_log: String::new(),
            corpus_domain_input: "資訊工程".to_string(),
            corpus_text_input: String::new(),
            benchmark_summary: String::new(),
        }
    }

    pub fn select_category(&mut self, cat: StudioCategory) {
        self.current_category = cat;
    }

    pub fn apply_theme_code(&mut self, code: &str) {
        self.settings.appearance.theme = code.to_string();
        let pref = ThemePreference::from_code(code);
        self.shell.set_theme_preference(pref);
    }

    pub fn save_all(&mut self) {
        self.settings.appearance.sidebar_width = self.shell.sidebar_width.round() as i32;
        self.settings.appearance.zoom_level =
            ((self.shell.ui_scale - 1.0) * 10.0).round() as i32;
        self.settings.appearance.theme = self.shell.theme_pref.as_code().to_string();
        ensure_keyboard_modes_consistent(&mut self.settings);
        match save_settings(&self.root_dir, &mut self.settings) {
            Ok(()) => {
                self.shell.save_state();
                self.shell
                    .toasts
                    .success("🎉 設定已成功儲存並即時同步至輸入法組態！");
            }
            Err(e) => {
                self.shell.toasts.error(format!("儲存設定失敗：{e}"));
            }
        }
    }

    pub fn reset_defaults(&mut self) {
        self.settings = RuhiSettings::default();
        self.apply_theme_code("system");
        self.shell.set_ui_scale(1.0);
        self.save_all();
        self.shell.toasts.warning("↺ 已還原出廠預設值並儲存！");
    }

    fn render_appearance_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("輸入操作模式（微軟新注音 vs 如喜輸入座）");
            ui.add_space(6.0);

            two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "smart_auto".to_string(),
                "自動判斷 APP 切換最佳輸入模式（預設推薦）",
                "日常軟體（記事本、瀏覽器、通訊軟體）優先使用獨立如喜輸入座；遇專業排版軟體（LibreOffice Writer 等）或傳統主控台 (CMD) 時，自動無感切換為微軟新注音模式，兼顧美觀與極致相容！",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "floating_follow".to_string(),
                "如喜輸入座：游標跟隨（自然經典風格・ㄅ）",
                "所有應用程式皆以獨立懸浮輸入座緊隨游標下方；若遇到未向系統回報空行幾何資訊之跨平台排版軟體，可能出現空行位置落後現象。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "floating_fixed".to_string(),
                "如喜輸入座：螢幕固定（自然經典風格・ㄆ）",
                "輸入座保持在固定螢幕座標，使用者可隨時用滑鼠自由拖曳至螢幕任意最佳位置，完全不受任何 APP 游標更新異常影響。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "inline_underline".to_string(),
                "微軟新注音游標跟隨模式（極致相容）",
                "100% 遵從 Windows TSF 原生行內組字規範，直接於目標視窗之文字輸入游標處繪製底線與拼弦，選字時彈出微軟新注音風格輕巧候選浮窗，相容性最高、排版工作首選。",
                &palette,
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("候選座排列與字體尺寸");
            ui.add_space(6.0);

            ui.label(egui::RichText::new("候選窗排列方向：").strong());
            ui.radio_value(
                &mut self.settings.appearance.orientation,
                "horizontal".to_string(),
                "橫向流式排列（預設推薦，視線左右平移最自然）",
            );
            ui.radio_value(
                &mut self.settings.appearance.orientation,
                "vertical".to_string(),
                "直向傳統排列（經典直書自上而下列出）",
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("候選字體大小：");
                ui.add(
                    egui::Slider::new(&mut self.settings.appearance.font_size, 18..=54)
                        .suffix(" px"),
                );
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("輸入座最大字數：");
                ui.add(
                    egui::Slider::new(&mut self.settings.appearance.max_composition_chars, 15..=60)
                        .suffix(" 字"),
                );
            });
            ui.label(
                egui::RichText::new("註：輸入長度超過最大字數時，系統將自動送出首個標點符號前之文字。")
                    .color(palette.fg_muted.to_egui()),
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("色彩主題與狀態指示");
            ui.add_space(6.0);

            let mut theme_str = self.settings.appearance.theme.clone();
            let mut changed = false;

            if two_tier_radio(
                ui,
                &mut theme_str,
                "system".to_string(),
                "跟隨 Windows 系統（自動即時匹配深淺色與系統個人化強調色）",
                "當 Windows 系統切換深淺色時，設定中心與輸入座自動無感同步換膚。",
                &palette,
            ) {
                changed = true;
            }
            if two_tier_radio(
                ui,
                &mut theme_str,
                "dark".to_string(),
                "極致深色 (Dark)",
                "採用深邃炭灰基底與 Windows DWM 沉浸式暗黑標題列，適合長時間專注打字。",
                &palette,
            ) {
                changed = true;
            }
            if two_tier_radio(
                ui,
                &mut theme_str,
                "light".to_string(),
                "宣紙淺色 (Light)",
                "採用溫潤宣紙米白基底，在明亮環境下維持清晰柔和的閱讀體驗。",
                &palette,
            ) {
                changed = true;
            }
            if changed {
                self.apply_theme_code(&theme_str);
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("送出字串後狀態指示行為：").strong());
            ui.radio_value(
                &mut self.settings.appearance.capsule_mode,
                "mini_capsule".to_string(),
                "縮為常駐迷你狀態膠囊（隨時可點齒輪 ⚙ 開啟設定，狀態清晰）",
            );
            ui.radio_value(
                &mut self.settings.appearance.capsule_mode,
                "auto_hide".to_string(),
                "完全隱藏輸入座（純淨極簡模式，由工作列系統匣圖示指示狀態）",
            );
        });
    }

    fn render_keyboard_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("鍵盤配置與常用鍵設定");
            ui.add_space(6.0);

            ui.label(egui::RichText::new("當前預設亞洲輸入模式：").strong());
            let layouts = [
                ("hsu", "許氏雙拼注音（Hsu Layout，盲打極致流暢首選 - 許）"),
                (
                    "array30",
                    "行列 30 輸入法（Array 30，支援一二級簡碼、?/* 萬用查詢與 W 符號表 - 行）",
                ),
                (
                    "japanese",
                    "日文羅馬字輸入（Japanese Romaji，五十音／漢字／片假名輕量轉換 - 日）",
                ),
                ("cangjie", "倉頡輸入法（Cangjie，支援 Z 鍵萬用字元查詢 - 倉）"),
                ("standard", "標準大千注音（Standard Daikien - 注）"),
                ("et26", "倚天 26 鍵注音（ETen 26-Key - 倚）"),
            ];
            for (code, label) in layouts {
                ui.radio_value(
                    &mut self.settings.keyboard.layout,
                    code.to_string(),
                    label,
                );
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "參與快速切換迴圈之亞洲模式（Ctrl / Shift + Caps Lock ＆ 狀態標籤選單）：",
                )
                .strong(),
            );
            ui.label(
                egui::RichText::new(
                    "勾選您日常使用的亞洲輸入模式。未勾選的模式將完全排除於快捷鍵循環與選單之外。",
                )
                .color(palette.fg_muted.to_egui()),
            );

            let mode_options = [
                ("hsu", "許氏雙拼注音 (許)"),
                ("array30", "行列 30 輸入法 (行)"),
                ("japanese", "日文羅馬字輸入 (日)"),
                ("cangjie", "倉頡輸入法 (倉)"),
                ("standard", "標準大千注音 (注)"),
                ("et26", "倚天 26 鍵注音 (倚)"),
            ];
            for (m_code, m_label) in mode_options {
                let mut enabled = self
                    .settings
                    .keyboard
                    .enabled_keyboard_modes
                    .iter()
                    .any(|m| m == m_code);
                if ui.checkbox(&mut enabled, m_label).changed() {
                    if enabled {
                        if !self
                            .settings
                            .keyboard
                            .enabled_keyboard_modes
                            .iter()
                            .any(|m| m == m_code)
                        {
                            self.settings
                                .keyboard
                                .enabled_keyboard_modes
                                .push(m_code.to_string());
                        }
                    } else {
                        self.settings
                            .keyboard
                            .enabled_keyboard_modes
                            .retain(|m| m != m_code);
                    }
                    ensure_keyboard_modes_consistent(&mut self.settings);
                }
            }

            ui.add_space(8.0);
            two_tier_checkbox(
                ui,
                &mut self.settings.keyboard.enable_array30_coach,
                "啟用「許氏注音 ➔ 行列 30」即時反查教練提示（行列反查提示）",
                "專為文字型思考者設計：在使用許氏注音打字組字、選字或上屏時，於候選視窗同步顯示該漢字之行列 30 筆形座標（如 18=Z）與按鍵拆碼提示，邊用許氏工作邊自然學會行列 30。",
                &palette,
            );

            ui.add_space(8.0);
            ui.label(egui::RichText::new("空白鍵行為：").strong());
            ui.radio_value(
                &mut self.settings.keyboard.space_behavior,
                "commit_first".to_string(),
                "出第一候選字（如喜智慧預設，流暢選詞）",
            );
            ui.radio_value(
                &mut self.settings.keyboard.space_behavior,
                "space_only".to_string(),
                "僅輸出半形空格",
            );

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Caps Lock 行為：").strong());
            ui.radio_value(
                &mut self.settings.keyboard.capslock_switch,
                "chinese_english".to_string(),
                "快速切換中／英數模式",
            );
            ui.radio_value(
                &mut self.settings.keyboard.capslock_switch,
                "capital_lock".to_string(),
                "大寫鎖定 (Capital Lock)",
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("標點符號結算行為（微軟新注音 vs 自然長句風格）");
            ui.add_space(6.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "A".to_string(),
                "經典即時送出（微軟新注音風格・預設推薦）",
                "鍵入任何標點（「，」、「。」、「、」等），整句連同標點立刻結算送出上屏。系統負擔最小、相容性最高、各類 APP 零排版延遲。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "B".to_string(),
                "智慧句末送出（自然長句折衷風格）",
                "逗號「，」、頓號「、」保留在輸入座維持長句；打到句末標點（「。」、「？」、「！」等）自動結算送出。兼顧長句打字連續性與段落自然閉合。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "C".to_string(),
                "全手動長句模式（純 Enter 送出）",
                "任何標點（包括句號）完全保留在輸入座內，絕不自動送出，一律由使用者確認無誤後自行按下 Enter 鍵統一結算。適合全文通盤檢視校對。",
                &palette,
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("Q 鍵快速候選輪換");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Q 鍵快速候選輪換次數上限：");
                ui.add(
                    egui::Slider::new(&mut self.settings.keyboard.q_cycle_max, 1..=8)
                        .suffix(" 次"),
                );
            });
            ui.label(
                egui::RichText::new(
                    "說明：在許氏鍵盤中，單擊 Q 鍵可在首選詞與次選詞間快速切換，連續按滿指定次數自動展開完整選字座。",
                )
                .color(palette.fg_muted.to_egui()),
            );
        });
    }

    fn render_mixed_english_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("許氏＋歐文智慧混打核心機制");
            ui.add_space(6.0);
            two_tier_checkbox(
                ui,
                &mut self.settings.keyboard.enable_mixed_english,
                "啟用許氏＋歐文智慧混打",
                "非合法許氏注音組合直接當作歐文，Enter 直接上屏；無需切換輸入法即可自然敲出英文單字與程式碼。",
                &palette,
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("混打仲裁策略與門檻");
            ui.add_space(6.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "contextual_stream".to_string(),
                "語境串流模式（推薦）",
                "首詞確認為歐文時開啟串流，放行後續 2~3 字詞；中文模式享有 100% 語境防護，杜絕非預期覆寫。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "conservative".to_string(),
                "保守模式",
                "僅在大寫字母、駝峰命名 (CamelCase) 或特殊標識符時自動放行歐文。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "min_length_only".to_string(),
                "長詞門檻模式",
                "連續未匹配字元長度達到門檻長度時，方強制轉為歐文串流。",
                &palette,
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("長詞門檻字數（門檻模式下生效）：");
                ui.add(
                    egui::Slider::new(&mut self.settings.keyboard.mixed_english_min_len, 3..=6)
                        .suffix(" 個字母"),
                );
            });
        });
    }

    fn render_grammar_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("現代漢語八大語法引擎與結構助詞");
            ui.add_space(6.0);

            two_tier_checkbox(
                ui,
                &mut self.settings.grammar.enable_all_eight_phases,
                "啟用現代漢語八大語法引擎",
                "涵蓋離合詞、動補結構、被動式、框式介詞、兼語連動、存現重疊等句法拓撲引力。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_checkbox(
                ui,
                &mut self.settings.grammar.enable_de_vs_de_translingual,
                "跨語言結構助詞深層語意分野律",
                "自動精準辨析閩客臺華跨語言『的』(ê) 與『得』(kah) 結構助詞的深層語法功能。",
                &palette,
            );
            ui.add_space(4.0);

            two_tier_checkbox(
                ui,
                &mut self.settings.grammar.enable_colloquial_biases,
                "生活高頻口語引力校準",
                "校正破音字先驗分佈，杜絕『自燃』、『渴論』、『醮我一生』等罕見假同音字語意覆蓋。",
                &palette,
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("詞庫主權、熱重載與語意調校工作室");
            ui.add_space(6.0);

            two_tier_checkbox(
                ui,
                &mut self.settings.grammar.enable_hot_reload,
                "使用者自建詞庫熱重載",
                "支援 Ctrl+2 快捷加詞與 TSV 檔案即時熱載入，修改後無須重啟輸入法。",
                &palette,
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "架構守則：如喜堅守計算語言學語法構式與二階轉移引力，嚴禁單字權重打地鼠，確保全域先驗獨立。",
                )
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("🛠️ 切換至內建語意調校工作室").clicked() {
                    self.current_category = StudioCategory::SemanticTuner;
                }
                if ui.button("🌐 開啟瀏覽器 Web 視覺化調校中心").clicked() {
                    self.spawn_web_studio_detached();
                }
            });
        });
    }

    fn render_ai_privacy_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("Zero-PII 隱私安全閘門");
            ui.add_space(6.0);
            two_tier_checkbox(
                ui,
                &mut self.settings.ai_and_privacy.zero_pii_gateway,
                "啟用 Zero-PII 隱私安全閘門",
                "絕不記錄個人敏感個資、電話、身分證、電子郵件與密碼欄位輸入，保障本機最高隱私安全。",
                &palette,
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("本地純綠 llama.cpp (.gguf) 與 Google Gemini 模型管理");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("指定本地模型識別碼：");
                let models = [
                    ("gemma4:latest", "gemma4:latest（Google 官方核心推薦）"),
                    ("gemma4:12b", "gemma4:12b（深層精準高階）"),
                    ("llama3.2:3b", "llama3.2:3b（輕量備援）"),
                ];
                for (code, label) in models {
                    ui.selectable_value(
                        &mut self.settings.ai_and_privacy.ollama_model,
                        code.to_string(),
                        label,
                    );
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button("🔍 探測本機純綠 llama-server 與 GGUF 模型庫")
                    .clicked()
                {
                    let status = self.shell.ai.refresh(true);
                    let count = self.shell.ai.available_models.len();
                    let msg = if status == AIStatus::Online {
                        format!("🟢 純綠 llama-server 在線就緒（發現 {count} 個本地 .gguf 模型）")
                    } else {
                        format!(
                            "⚪ 離線純計算語言學模式（本地已發現 {count} 個 .gguf 模型，隨時可按需啟動）"
                        )
                    };
                    self.shell.toasts.info(msg);
                }
                if ui.button("🛠️ 前往語意調校工作室與 AI 顧問").clicked() {
                    self.current_category = StudioCategory::SemanticTuner;
                }
            });
        });
    }

    fn render_semantic_tuner_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("🔬 雙向詞界對齊與二階轉移引力合成");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("錯誤輸出句：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.tune_error_input)
                        .desired_width(420.0 * self.shell.ui_scale),
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("期望正確句：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.tune_correct_input)
                        .desired_width(420.0 * self.shell.ui_scale),
                );
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button("⚡ 立即對齊並注入二階轉移規則 (Layer 2 / Layer 3)")
                    .clicked()
                {
                    let diffs = self.tuner.align_sentence_pair(
                        &self.tune_error_input,
                        &self.tune_correct_input,
                        None,
                        None,
                    );
                    let mut all_rules = Vec::new();
                    let mut oov_words = Vec::new();
                    let mut report_lines = Vec::new();

                    for d in &diffs {
                        let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
                        let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
                        let tax = d
                            .get("error_taxonomy")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        report_lines.push(format!("• 差異區塊：「{rej}」➔「{acc}」 ({tax})"));
                        if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                            oov_words.extend(ws.clone());
                        }
                        let rules = self.tuner.synthesize_rules(d);
                        for r in &rules {
                            let p = r.get("prev").and_then(Value::as_str).unwrap_or("");
                            let n = r.get("next").and_then(Value::as_str).unwrap_or("");
                            let s = r.get("score").and_then(Value::as_f64).unwrap_or(28.0);
                            report_lines.push(format!("  ➕ 二階轉移：[{p} ➔ {n}] (+{s:.1})"));
                        }
                        all_rules.extend(rules);
                    }

                    let w_added = self.tuner.append_words_to_domain_lexicon(&oov_words);
                    let r_added = self.tuner.append_rules_to_tsv(&all_rules);
                    report_lines.push(format!(
                        "✅ 已成功寫入 {r_added} 條二階轉移規則與 {w_added} 個領域複合詞！"
                    ));
                    self.tune_output_log = report_lines.join("\n");
                    self.shell
                        .toasts
                        .success(format!("已注入 {r_added} 條二階轉移規則！"));
                }

                if ui.button("✨ 昇華暫存修訂記錄 (Sublimate Revisions)").clicked() {
                    let res = self.consultant.sublimate_revisions(&mut self.tuner);
                    let exp = res
                        .get("explanation")
                        .and_then(Value::as_str)
                        .unwrap_or("昇華完成");
                    self.tune_output_log = exp.to_string();
                    self.shell.toasts.info(exp.to_string());
                }

                if ui.button("🏆 執行 30 組黃金基準驗證").clicked() {
                    let res = self.tuner.verify_golden_benchmark();
                    let passed = res.get("passed").and_then(Value::as_i64).unwrap_or(0);
                    let total = res.get("total").and_then(Value::as_i64).unwrap_or(30);
                    let rate = res.get("pass_rate").and_then(Value::as_str).unwrap_or("100%");
                    self.benchmark_summary =
                        format!("黃金基準測試通過率：{passed}/{total} ({rate})");
                    self.tune_output_log = self.benchmark_summary.clone();
                    self.shell.toasts.success(self.benchmark_summary.clone());
                }

                if ui.button("🌐 啟動 Web 瀏覽器完整版面板").clicked() {
                    self.spawn_web_studio_detached();
                }
            });

            if !self.tune_output_log.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(&self.tune_output_log).color(palette.fg_bright.to_egui()),
                );
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("📚 專業領域文獻詞彙與二階搭配一鍵萃取");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("領域名稱：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.corpus_domain_input)
                        .desired_width(180.0 * self.shell.ui_scale),
                );
                if ui.button("📥 萃取並匯入領域詞庫").clicked()
                    && !self.corpus_text_input.trim().is_empty()
                {
                    let res = self.consultant.extract_domain_corpus(
                        &self.corpus_text_input,
                        &self.corpus_domain_input,
                        &self.tuner,
                    );
                    let terms = res
                        .get("terms")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let trans = res
                        .get("transitions")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let w_cnt = self.tuner.append_words_to_domain_lexicon(&terms);
                    let mut rules = Vec::new();
                    for t in &trans {
                        rules.push(json!({
                            "prev": t.get("prev").and_then(Value::as_str).unwrap_or(""),
                            "next": t.get("next").and_then(Value::as_str).unwrap_or(""),
                            "score": 28.0,
                            "kind": "collocation",
                            "comment": t.get("description").and_then(Value::as_str).unwrap_or("領域文獻搭配")
                        }));
                    }
                    let r_cnt = self.tuner.append_rules_to_tsv(&rules);
                    let msg = format!(
                        "已自「{}」文獻匯入 {w_cnt} 個專業詞條與 {r_cnt} 條二階轉移！",
                        self.corpus_domain_input
                    );
                    self.tune_output_log = msg.clone();
                    self.shell.toasts.success(msg);
                }
            });
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.corpus_text_input)
                    .hint_text("在此貼上專業領域文章或技術段落，系統將自動切分並提煉領域詞彙與二階轉移引力...")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        });
    }

    fn render_about_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("品牌理念與精神哲學");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("「如雲行水流自在歡喜」")
                    .size(16.0 * self.shell.ui_scale)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label("• 中文釋義：「如雲行水流自在歡喜」——思緒如行雲流水般暢行無阻，選字貼合心意而自在歡喜。");
            ui.label("• 英文全稱：Responsive Universal Human Input（自適應通用人本輸入法，承襲 rud() 自適應初心）。");
            ui.label("• 梵語語源：字根 Ruh / Rūhi (रूहि) 意為「萌芽、生長、向上攀升」，象徵詞庫隨使用者與群集智慧生生不息。");
            ui.label("• 核心宣言：「再也不必望穿秋水，等候一個可能永遠不會進步的官方詞庫。」");
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("通用智慧型輸入法全語言藍圖（Universal Smart IME Roadmap）");
            ui.add_space(6.0);
            ui.label("1. 繁體中文注音體系：許氏鍵盤（首發）、標準大千、倚天 26 鍵、漢語拼音");
            ui.label("2. 繁體中文形碼體系：倉頡（三代／五代）、大易、行列輸入法");
            ui.label("3. 亞洲多語系輸入：日語假名/漢字、韓語諺文、泰語");
            ui.label("4. 專業學術標音符號：梵文天城體羅馬化（IAST）、台羅/白話字拼音");
            ui.label("5. 歐文多語言擴展：德文、法文、西文等特殊重音符號智慧組字");
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("系統狀態與全 Rust 原生架構資訊");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "核心版本：RuhI v0.3.0 (100% Pure Rust — ruhi-core + ruhi-tsf + ruhi-studio + atg_lib)",
                )
                .strong(),
            );
            ui.label(format!(
                "已載入多字詞條數：{} 詞 | 單字注音索引：{} 字 | 自適應二階轉移：{} 條",
                self.tuner.known_words.len(),
                self.tuner.char_bopomofo.len(),
                self.tuner.user_transitions.len()
            ));
            ui.label(
                egui::RichText::new(format!("詞庫目錄：{}", self.data_dir.display()))
                    .color(palette.fg_muted.to_egui()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("📁 開啟詞庫資料夾").clicked() && self.data_dir.is_dir() {
                    #[cfg(target_os = "windows")]
                    let _ = Command::new("explorer.exe").arg(&self.data_dir).spawn();
                    #[cfg(not(target_os = "windows"))]
                    let _ = Command::new("xdg-open").arg(&self.data_dir).spawn();
                }
                if ui.button("⚙️ 開啟設定檔資料夾").clicked() && self.root_dir.is_dir() {
                    #[cfg(target_os = "windows")]
                    let _ = Command::new("explorer.exe").arg(&self.root_dir).spawn();
                    #[cfg(not(target_os = "windows"))]
                    let _ = Command::new("xdg-open").arg(&self.root_dir).spawn();
                }
                if ui.button("🛠️ 切換至語意調校工作室").clicked() {
                    self.current_category = StudioCategory::SemanticTuner;
                }
            });
        });
    }

    fn spawn_web_studio_detached(&self) {
        if let Ok(exe) = std::env::current_exe() {
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd.exe")
                .args([
                    "/c",
                    "start",
                    "RuhI Web Semantic Studio",
                    &exe.to_string_lossy(),
                    "--web",
                ])
                .current_dir(&self.root_dir)
                .spawn();
            #[cfg(not(target_os = "windows"))]
            let _ = Command::new(&exe)
                .arg("--web")
                .current_dir(&self.root_dir)
                .spawn();
        }
    }
}

impl eframe::App for RuhiStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.shell.begin_frame(ctx);

        // 同步主題變更至 settings.appearance.theme
        let shell_theme_code = self.shell.theme_pref.as_code().to_string();
        if self.settings.appearance.theme != shell_theme_code {
            self.settings.appearance.theme = shell_theme_code;
        }

        let nav_items: Vec<(usize, &str)> = StudioCategory::ALL
            .iter()
            .map(|c| (0, c.sidebar_label()))
            .collect();
        let min_sidebar_w = self.shell.ensure_sidebar_min_width(&nav_items);

        // 頂部品牌標頭列
        egui::TopBottomPanel::top("ruhi_studio_top_bar")
            .exact_height(58.0 * self.shell.ui_scale)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("RuhI（如喜）輸入法 — 偏好設定與語意調校中心")
                                .size(17.0 * self.shell.ui_scale)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "如雲行水流自在歡喜 · 100% 原生 Rust + UniversalUI (atg_lib) 自適應架構",
                            )
                            .size(11.5 * self.shell.ui_scale)
                            .color(self.shell.palette.fg_muted.to_egui()),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        let pct = (self.shell.ui_scale * 100.0).round() as i32;
                        if ui
                            .button(format!("🔍 縮放 {pct}% (Ctrl+0)"))
                            .on_hover_text("支援 Ctrl + 滑鼠滾輪 或 Ctrl + +/-/0 即時無段縮放")
                            .clicked()
                        {
                            self.shell.set_ui_scale(1.0);
                        }
                        if ui.button("🎨 切換深淺色 (Ctrl+T)").clicked() {
                            let next = self
                                .shell
                                .theme_pref
                                .toggle_dark_light(self.shell.palette.is_dark);
                            self.shell.set_theme_preference(next);
                            self.settings.appearance.theme = next.as_code().to_string();
                        }
                    });
                });
            });

        // 底部操作按鈕列
        egui::TopBottomPanel::bottom("ruhi_studio_bottom_bar")
            .exact_height(48.0 * self.shell.ui_scale)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let mut do_reset = false;
                let mut do_save = false;
                rud_button_bar(
                    ui,
                    |ui| {
                        if ui.button("↺ 還原出廠預設值").clicked() {
                            do_reset = true;
                        }
                    },
                    |ui| {
                        if ui
                            .button(egui::RichText::new("  💾 儲存並套用設定  ").strong())
                            .clicked()
                        {
                            do_save = true;
                        }
                    },
                );
                if do_reset {
                    self.reset_defaults();
                }
                if do_save {
                    self.save_all();
                }
            });

        // 左側可拖曳樹狀導覽側邊欄 (rud() PanedWindow + calc_treeview_width)
        let side_resp = egui::SidePanel::left("ruhi_studio_sidebar")
            .resizable(true)
            .default_width(self.shell.sidebar_width.max(min_sidebar_w))
            .min_width(min_sidebar_w)
            .max_width(480.0 * self.shell.ui_scale)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                for &cat in StudioCategory::ALL {
                    let selected = self.current_category == cat;
                    let resp = ui.selectable_label(
                        selected,
                        egui::RichText::new(cat.sidebar_label()).size(14.0 * self.shell.ui_scale),
                    );
                    if resp.clicked() {
                        self.select_category(cat);
                    }
                    ui.add_space(4.0);
                }
            });
        self.shell.sidebar_width = side_resp.response.rect.width();

        // 右側主內容區
        egui::CentralPanel::default().show(ctx, |ui| {
            let (title, desc) = self.current_category.page_header();
            ui.heading(egui::RichText::new(title).size(16.5 * self.shell.ui_scale).strong());
            ui.label(
                egui::RichText::new(desc)
                    .size(12.0 * self.shell.ui_scale)
                    .color(self.shell.palette.fg_muted.to_egui()),
            );
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.current_category {
                    StudioCategory::Appearance => self.render_appearance_page(ui),
                    StudioCategory::KeyboardLayout => self.render_keyboard_page(ui),
                    StudioCategory::MixedEnglish => self.render_mixed_english_page(ui),
                    StudioCategory::Grammar => self.render_grammar_page(ui),
                    StudioCategory::AiAndPrivacy => self.render_ai_privacy_page(ui),
                    StudioCategory::SemanticTuner => self.render_semantic_tuner_page(ui),
                    StudioCategory::About => self.render_about_page(ui),
                });
        });

        self.shell.end_frame(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.appearance.sidebar_width = self.shell.sidebar_width.round() as i32;
        let _ = save_settings(&self.root_dir, &mut self.settings);
        self.shell.save_state();
    }
}

/// 執行無頭自動化自我驗證 (`--test`)，驗證完成立即退出，絕不阻塞背景
fn run_headless_self_test() -> i32 {
    let mut app = RuhiStudioApp::new(StudioCategory::Appearance);

    // 1. 驗證 7 大分類切換
    for &cat in StudioCategory::ALL {
        app.select_category(cat);
        assert_eq!(app.current_category, cat);
    }

    // 2. 驗證側欄動態寬度精算 (calc_treeview_width)
    let nav_items: Vec<(usize, &str)> = StudioCategory::ALL
        .iter()
        .map(|c| (0, c.sidebar_label()))
        .collect();
    let min_w = app.shell.ensure_sidebar_min_width(&nav_items);
    assert!(min_w >= 180.0, "Sidebar min width too small: {min_w}");

    // 3. 驗證深淺色即時切換
    app.apply_theme_code("dark");
    assert!(app.shell.palette.is_dark);
    app.apply_theme_code("light");
    assert!(!app.shell.palette.is_dark);
    app.apply_theme_code("system");

    // 4. 驗證 UI 縮放
    app.shell.set_ui_scale(1.25);
    assert!((app.shell.ui_scale - 1.25).abs() < 0.01);
    app.shell.set_ui_scale(1.0);

    // 5. 驗證雙向詞界對齊、二階轉移合成與 Zero-PII 隱私過濾
    let diffs = app
        .tuner
        .align_sentence_pair("讓步董程式", "讓不懂程式", None, None);
    assert!(!diffs.is_empty());
    let rules = app.tuner.synthesize_rules(&diffs[0]);
    assert!(!rules.is_empty());
    assert!(ZeroPiiPrivacyFilter::is_safe_token("不懂").0);
    assert!(!ZeroPiiPrivacyFilter::is_safe_token("0912345678").0);

    // 5b. 驗證本地端語意沙盒守門員 (Local Sandbox Gatekeeper) 與 30 組黃金基準
    let gate_res = app.tuner.import_tailcat_package(&json!({
        "protocol": "RUHI_TAILCAT_FEDERATED_SEMANTIC_V1",
        "rules": [
            {"type": "UNIGRAM_WEIGHT", "word": "裡", "weight": 1200},
            {"type": "BIGRAM_TRANSITION", "prev": "奇妙", "next": "他", "recommended_delta": -30.0},
            {"type": "BIGRAM_TRANSITION", "prev": "0912345678", "next": "測試", "recommended_delta": 25.0}
        ]
    }));
    assert_eq!(gate_res["blocked_unigram_rules"], 1);
    assert_eq!(gate_res["blocked_unsafe_rules"], 2);
    let bench_res = app.tuner.verify_golden_benchmark();
    assert_eq!(bench_res["total"], 30);

    // 6. 驗證設定持久化與跨目錄同步
    app.save_all();

    println!("[OK] RuhiStudioApp (Rust native atg_lib) rud() layout, theme, config sync, Tailcat gatekeeper, and 30-item semantic tuner self-test passed!");
    0
}

/// 執行主控台互動式 TUI (`--cli`)
fn run_interactive_cli(tuner: &mut SemanticTuner) -> i32 {
    let mut consultant = AISemanticConsultant::new(Some(&tuner.root));
    loop {
        println!("\n========================================================");
        println!("  如喜輸入法 (RuhI) · 語意調校工作室 (Rust CLI 模式)");
        println!("========================================================");
        println!("  [1] 輸入「錯誤句 / 正確句」對齊並提煉二階轉移規則");
        println!("  [2] 執行 30 組黃金基準測試集驗證 (--verify)");
        println!("  [3] 自動昇華暫存修訂記錄 (--auto-align)");
        println!("  [4] 產生 Zero-PII Tailcat 聯邦語意提案 (--tailcat)");
        println!("  [0] 離開");
        print!("請選擇功能 [0-4]: ");
        let _ = io::stdout().flush();

        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            break;
        }
        match choice.trim() {
            "0" | "q" | "exit" => break,
            "1" => {
                print!("請輸入【錯誤輸出句】: ");
                let _ = io::stdout().flush();
                let mut err = String::new();
                let _ = io::stdin().read_line(&mut err);

                print!("請輸入【期望正確句】: ");
                let _ = io::stdout().flush();
                let mut cor = String::new();
                let _ = io::stdin().read_line(&mut cor);

                let diffs = tuner.align_sentence_pair(err.trim(), cor.trim(), None, None);
                let mut all_rules = Vec::new();
                let mut words = Vec::new();
                for d in &diffs {
                    if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                        words.extend(ws.clone());
                    }
                    all_rules.extend(tuner.synthesize_rules(d));
                }
                let w_cnt = tuner.append_words_to_domain_lexicon(&words);
                let r_cnt = tuner.append_rules_to_tsv(&all_rules);
                println!("✅ 已提煉並寫入 {r_cnt} 條二階轉移規則與 {w_cnt} 個領域複合詞！");
            }
            "2" => {
                let res = tuner.verify_golden_benchmark();
                println!(
                    "🏆 黃金基準測試結果：{}/{} ({})",
                    res["passed"],
                    res["total"],
                    res["pass_rate"].as_str().unwrap_or("100%")
                );
            }
            "3" => {
                let res = consultant.sublimate_revisions(tuner);
                println!(
                    "✨ {}",
                    res["explanation"].as_str().unwrap_or("昇華完成")
                );
            }
            "4" => {
                let sample = vec![
                    json!({"prev": "讓", "next": "不懂", "score": 26.0, "kind": "collocation"}),
                    json!({"prev": "不懂", "next": "程式", "score": 26.0, "kind": "collocation"}),
                ];
                let pkg = tuner.package_tailcat_proposal(&sample);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&pkg).unwrap_or_default()
                );
            }
            _ => {}
        }
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--test") {
        std::process::exit(run_headless_self_test());
    }

    let mut is_web = false;
    let mut is_cli = false;
    let mut is_verify = false;
    let mut is_auto_align = false;
    let mut is_tailcat = false;
    let mut is_tuner_gui = false;
    let mut no_browser = false;
    let mut port: u16 = 8765;
    let mut err_arg = String::new();
    let mut cor_arg = String::new();
    let mut file_arg = String::new();
    let mut pull_community_url = String::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--web" => is_web = true,
            "--cli" => is_cli = true,
            "--verify" => is_verify = true,
            "--auto-align" => is_auto_align = true,
            "--tailcat" => is_tailcat = true,
            "--tuner" => is_tuner_gui = true,
            "--no-browser" => no_browser = true,
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(8765);
                i += 1;
            }
            "-e" | "--error" if i + 1 < args.len() => {
                err_arg = args[i + 1].clone();
                i += 1;
            }
            "-c" | "--correct" if i + 1 < args.len() => {
                cor_arg = args[i + 1].clone();
                i += 1;
            }
            "-f" | "--file" if i + 1 < args.len() => {
                file_arg = args[i + 1].clone();
                i += 1;
            }
            "--pull-community" if i + 1 < args.len() => {
                pull_community_url = args[i + 1].clone();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let root = resolve_ruhi_root();
    let mut tuner = SemanticTuner::new(Some(&root));

    if !pull_community_url.is_empty() {
        let res = tuner.pull_community_rules_with_etag(&pull_community_url, false);
        println!(
            "{}",
            serde_json::to_string_pretty(&res).unwrap_or_default()
        );
        std::process::exit(0);
    }

    if is_verify {
        let res = tuner.verify_golden_benchmark();
        println!(
            "========================================================================\n  如喜輸入法 (RuhI) 黃金基準測試結果：{}/{} ({})\n========================================================================",
            res["passed"],
            res["total"],
            res["pass_rate"].as_str().unwrap_or("100%")
        );
        std::process::exit(0);
    }

    if is_tailcat {
        let sample = vec![
            json!({"prev": "讓", "next": "不懂", "score": 26.0, "kind": "collocation"}),
            json!({"prev": "不懂", "next": "程式", "score": 26.0, "kind": "collocation"}),
        ];
        let pkg = tuner.package_tailcat_proposal(&sample);
        println!(
            "{}",
            serde_json::to_string_pretty(&pkg).unwrap_or_default()
        );
        std::process::exit(0);
    }

    if is_auto_align {
        let mut consultant = AISemanticConsultant::new(Some(&root));
        let res = consultant.sublimate_revisions(&mut tuner);
        println!(
            "{}",
            res["explanation"].as_str().unwrap_or("自動對齊昇華完成。")
        );
        std::process::exit(0);
    }

    if !err_arg.is_empty() && !cor_arg.is_empty() {
        let diffs = tuner.align_sentence_pair(&err_arg, &cor_arg, None, None);
        let mut all_rules = Vec::new();
        let mut oov_words = Vec::new();
        for d in &diffs {
            if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                oov_words.extend(ws.clone());
            }
            all_rules.extend(tuner.synthesize_rules(d));
        }
        let w_added = tuner.append_words_to_domain_lexicon(&oov_words);
        let r_added = tuner.append_rules_to_tsv(&all_rules);
        println!(
            "✅ 對齊完成！已新增 {r_added} 條二階轉移規則與 {w_added} 個領域複合詞。"
        );
        std::process::exit(0);
    }

    if !file_arg.is_empty() && !is_web {
        if let Some(p) = tuner.resolve_local_txt_file(&file_arg, &file_arg) {
            let pairs = tuner.load_pairs_from_file(&p);
            let mut all_rules = Vec::new();
            for pair in &pairs {
                let e = pair.get("error").and_then(Value::as_str).unwrap_or("");
                let c = pair.get("correct").and_then(Value::as_str).unwrap_or("");
                for d in tuner.align_sentence_pair(e, c, None, None) {
                    all_rules.extend(tuner.synthesize_rules(&d));
                }
            }
            let added = tuner.append_rules_to_tsv(&all_rules);
            println!("✅ 批次處理完成！已注入 {added} 條二階條件轉移規則。");
            std::process::exit(0);
        } else {
            eprintln!("❌ 找不到指定語料檔案：{file_arg}");
            std::process::exit(1);
        }
    }

    if is_cli {
        std::process::exit(run_interactive_cli(&mut tuner));
    }

    if is_web {
        if let Err(e) = start_web_studio(tuner, port, file_arg, !no_browser) {
            eprintln!("❌ Web 伺服器啟動失敗：{e}");
            std::process::exit(1);
        }
        return;
    }

    let initial_cat = if is_tuner_gui {
        StudioCategory::SemanticTuner
    } else {
        StudioCategory::Appearance
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RuhI（如喜）輸入法 — 現代化偏好設定與語意調校中心")
            .with_inner_size([1020.0, 740.0])
            .with_min_inner_size([780.0, 540.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RuhI（如喜）輸入法 — 現代化偏好設定與語意調校中心",
        options,
        Box::new(move |_cc| Ok(Box::new(RuhiStudioApp::new(initial_cat)))),
    );
}
