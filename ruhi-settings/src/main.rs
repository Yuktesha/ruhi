#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! # `ruhi-settings` — RuhI（如喜）輸入法原生 Rust 現代化偏好設定與語意調校中心
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
use ruhi::engine::{find_mode_descriptor, mode_picker_options, ALL_MODES};
use ruhi_settings::config_sync::{
    ensure_keyboard_modes_consistent, load_settings, mount_single_keyboard_mode_only,
    resolve_data_dir, resolve_effective_ui_lang, resolve_ruhi_root, save_settings,
    unmount_keyboard_mode, RuhiSettings,
};
use ruhi_settings::telemetry_sync::{TelemetrySyncClient, HeuristicPiiFilter};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

/// 左側分類導覽頁籤列舉（支援「🎨 外觀與候選座」與「⌨ 鍵盤配置與歐亞全譜系」雙樹狀母節點及獨立子分枝頁面）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioCategory {
    Appearance,
    AppearanceLang,
    AppearanceInputMode,
    AppearanceCandidateDesk,
    AppearanceThemeCapsule,
    KeyboardLayout,
    KeyboardCjkPhonetic,
    KeyboardCjkShape,
    KeyboardJkEurasian,
    KeyboardHotkeysPunct,
    MixedEnglish,
    Grammar,
    AiAndPrivacy,
    About,
}

impl StudioCategory {
    pub const ALL: &'static [StudioCategory] = &[
        StudioCategory::Appearance,
        StudioCategory::AppearanceLang,
        StudioCategory::AppearanceInputMode,
        StudioCategory::AppearanceCandidateDesk,
        StudioCategory::AppearanceThemeCapsule,
        StudioCategory::KeyboardLayout,
        StudioCategory::KeyboardCjkPhonetic,
        StudioCategory::KeyboardCjkShape,
        StudioCategory::KeyboardJkEurasian,
        StudioCategory::KeyboardHotkeysPunct,
        StudioCategory::MixedEnglish,
        StudioCategory::Grammar,
        StudioCategory::AiAndPrivacy,
        StudioCategory::About,
    ];

    pub const ROOT_CATEGORIES: &'static [StudioCategory] = &[
        StudioCategory::Appearance,
        StudioCategory::KeyboardLayout,
        StudioCategory::MixedEnglish,
        StudioCategory::Grammar,
        StudioCategory::AiAndPrivacy,
        StudioCategory::About,
    ];

    pub const APPEARANCE_BRANCHES: &'static [StudioCategory] = &[
        StudioCategory::AppearanceLang,
        StudioCategory::AppearanceInputMode,
        StudioCategory::AppearanceCandidateDesk,
        StudioCategory::AppearanceThemeCapsule,
    ];

    pub const KEYBOARD_BRANCHES: &'static [StudioCategory] = &[
        StudioCategory::KeyboardCjkPhonetic,
        StudioCategory::KeyboardCjkShape,
        StudioCategory::KeyboardJkEurasian,
        StudioCategory::KeyboardHotkeysPunct,
    ];

    pub fn is_appearance_group(&self) -> bool {
        matches!(
            self,
            StudioCategory::Appearance
                | StudioCategory::AppearanceLang
                | StudioCategory::AppearanceInputMode
                | StudioCategory::AppearanceCandidateDesk
                | StudioCategory::AppearanceThemeCapsule
        )
    }

    pub fn is_keyboard_group(&self) -> bool {
        matches!(
            self,
            StudioCategory::KeyboardLayout
                | StudioCategory::KeyboardCjkPhonetic
                | StudioCategory::KeyboardCjkShape
                | StudioCategory::KeyboardJkEurasian
                | StudioCategory::KeyboardHotkeysPunct
        )
    }

    pub fn tree_depth(&self) -> usize {
        match self {
            StudioCategory::AppearanceLang
            | StudioCategory::AppearanceInputMode
            | StudioCategory::AppearanceCandidateDesk
            | StudioCategory::AppearanceThemeCapsule
            | StudioCategory::KeyboardCjkPhonetic
            | StudioCategory::KeyboardCjkShape
            | StudioCategory::KeyboardJkEurasian
            | StudioCategory::KeyboardHotkeysPunct => 1,
            _ => 0,
        }
    }

    pub fn sidebar_label(&self) -> &'static str {
        self.sidebar_label_i18n("zh-TW")
    }

    pub fn sidebar_label_i18n(&self, ui_lang: &str) -> &'static str {
        match (ui_lang, self) {
            ("en-US", StudioCategory::Appearance) => "🎨 Appearance & Desk",
            ("en-US", StudioCategory::AppearanceLang) => "├─ 🌐 UI Language",
            ("en-US", StudioCategory::AppearanceInputMode) => "├─ 🖥 Input Display Mode",
            ("en-US", StudioCategory::AppearanceCandidateDesk) => "├─ 📐 Layout & Font Sizes",
            ("en-US", StudioCategory::AppearanceThemeCapsule) => "└─ 🎭 Theme & Status Capsule",
            ("en-US", StudioCategory::KeyboardLayout) => "⌨ Keyboard & Eurasian Tree",
            ("en-US", StudioCategory::KeyboardCjkPhonetic) => "├─ 🔊 C: Phonetic (Bopomofo)",
            ("en-US", StudioCategory::KeyboardCjkShape) => "├─ 🐉 C: Longkan, Shape & Num",
            ("en-US", StudioCategory::KeyboardJkEurasian) => "├─ 🌍 JK & Eurasian Tree",
            ("en-US", StudioCategory::KeyboardHotkeysPunct) => "└─ ⚡ Hotkeys & Punctuation",
            ("en-US", StudioCategory::MixedEnglish) => "🔤 Eurasian Mixed Input",
            ("en-US", StudioCategory::Grammar) => "🧠 Semantic & Grammar",
            ("en-US", StudioCategory::AiAndPrivacy) => "🛡 Privacy & Cloud Sync",
            ("en-US", StudioCategory::About) => "ℹ About & Universal Vision",
            ("ja-JP", StudioCategory::Appearance) => "🎨 外観と候補デスク",
            ("ja-JP", StudioCategory::AppearanceLang) => "├─ 🌐 表示言語",
            ("ja-JP", StudioCategory::AppearanceInputMode) => "├─ 🖥 入力表示モード",
            ("ja-JP", StudioCategory::AppearanceCandidateDesk) => "├─ 📐 候補配列とフォント",
            ("ja-JP", StudioCategory::AppearanceThemeCapsule) => "└─ 🎭 テーマと状態カプセル",
            ("ja-JP", StudioCategory::KeyboardLayout) => "⌨ キー配列とユーラシア系統樹",
            ("ja-JP", StudioCategory::KeyboardCjkPhonetic) => "├─ 🔊 漢語(C)：表音・注音系",
            ("ja-JP", StudioCategory::KeyboardCjkShape) => "├─ 🐉 漢語(C)：龍龕形音・字形コード",
            ("ja-JP", StudioCategory::KeyboardJkEurasian) => "├─ 🌍 日韓(JK)・欧亜全系統樹",
            ("ja-JP", StudioCategory::KeyboardHotkeysPunct) => "└─ ⚡ 切替キーと句読点設定",
            ("ja-JP", StudioCategory::MixedEnglish) => "🔤 欧亜スマート混打",
            ("ja-JP", StudioCategory::Grammar) => "🧠 意味構造と文法重力",
            ("ja-JP", StudioCategory::AiAndPrivacy) => "🛡 プライバシーとクラウド同期",
            ("ja-JP", StudioCategory::About) => "ℹ RuhI について",
            ("ko-KR", StudioCategory::Appearance) => "🎨 외관 및 후보창",
            ("ko-KR", StudioCategory::AppearanceLang) => "├─ 🌐 인터페이스 언어",
            ("ko-KR", StudioCategory::AppearanceInputMode) => "├─ 🖥 입력 표시 모드",
            ("ko-KR", StudioCategory::AppearanceCandidateDesk) => "├─ 📐 배열 및 글꼴 크기",
            ("ko-KR", StudioCategory::AppearanceThemeCapsule) => "└─ 🎭 테마 및 상태 캡슐",
            ("ko-KR", StudioCategory::KeyboardLayout) => "⌨ 키보드 배열 및 유라시아 트리",
            ("ko-KR", StudioCategory::KeyboardCjkPhonetic) => "├─ 🔊 한어(C): 표음/주음계",
            ("ko-KR", StudioCategory::KeyboardCjkShape) => "├─ 🐉 한어(C): 용감형음/자형코드",
            ("ko-KR", StudioCategory::KeyboardJkEurasian) => "├─ 🌍 일한(JK) 및 유라시아 트리",
            ("ko-KR", StudioCategory::KeyboardHotkeysPunct) => "└─ ⚡ 빠른 전환 및 문장부호",
            ("ko-KR", StudioCategory::MixedEnglish) => "🔤 유라시아 스마트 혼용",
            ("ko-KR", StudioCategory::Grammar) => "🧠 의미 구조 및 문법 중력",
            ("ko-KR", StudioCategory::AiAndPrivacy) => "🛡 개인정보 및 클라우드 동기화",
            ("ko-KR", StudioCategory::About) => "ℹ RuhI 정보 및 비전",
            _ => match self {
                StudioCategory::Appearance => "🎨 外觀與候選座",
                StudioCategory::AppearanceLang => "├─ 🌐 介面顯示語系",
                StudioCategory::AppearanceInputMode => "├─ 🖥 輸入操作模式",
                StudioCategory::AppearanceCandidateDesk => "├─ 📐 候選座排列與字體尺寸",
                StudioCategory::AppearanceThemeCapsule => "└─ 🎭 色彩主題與狀態指示",
                StudioCategory::KeyboardLayout => "⌨ 鍵盤配置與常用鍵",
                StudioCategory::KeyboardCjkPhonetic => "├─ 🔊 漢語(C)：音韻注音系",
                StudioCategory::KeyboardCjkShape => "├─ 🐉 漢語(C)：龍龕形音與字形碼",
                StudioCategory::KeyboardJkEurasian => "├─ 🌍 日韓(JK)與歐亞全譜樹",
                StudioCategory::KeyboardHotkeysPunct => "└─ ⚡ 快速切換與標點常用鍵",
                StudioCategory::MixedEnglish => "🔤 歐亞智慧混打",
                StudioCategory::Grammar => "🧠 語意結構與語法引力",
                StudioCategory::AiAndPrivacy => "🛡 隱私安全與雲端回饋",
                
                StudioCategory::About => "ℹ 關於與全語言願景",
            },
        }
    }

    pub fn branch_short_label_i18n(&self, ui_lang: &str) -> &'static str {
        match (ui_lang, self) {
            ("en-US", StudioCategory::AppearanceLang) => "🌐 UI Language",
            ("en-US", StudioCategory::AppearanceInputMode) => "🖥 Input Mode",
            ("en-US", StudioCategory::AppearanceCandidateDesk) => "📐 Layout & Fonts",
            ("en-US", StudioCategory::AppearanceThemeCapsule) => "🎭 Theme & Capsule",
            ("en-US", StudioCategory::KeyboardCjkPhonetic) => "🔊 C: Phonetic",
            ("en-US", StudioCategory::KeyboardCjkShape) => "🐉 C: Longkan & Shape",
            ("en-US", StudioCategory::KeyboardJkEurasian) => "🌍 JK & Eurasian Tree",
            ("en-US", StudioCategory::KeyboardHotkeysPunct) => "⚡ Hotkeys & Punct",
            ("ja-JP", StudioCategory::AppearanceLang) => "🌐 表示言語",
            ("ja-JP", StudioCategory::AppearanceInputMode) => "🖥 入力表示モード",
            ("ja-JP", StudioCategory::AppearanceCandidateDesk) => "📐 候補配列とフォント",
            ("ja-JP", StudioCategory::AppearanceThemeCapsule) => "🎭 テーマとカプセル",
            ("ja-JP", StudioCategory::KeyboardCjkPhonetic) => "🔊 漢語(C)：表音・注音",
            ("ja-JP", StudioCategory::KeyboardCjkShape) => "🐉 漢語(C)：龍龕・字形",
            ("ja-JP", StudioCategory::KeyboardJkEurasian) => "🌍 日韓(JK)・欧亜系統樹",
            ("ja-JP", StudioCategory::KeyboardHotkeysPunct) => "⚡ 切替キーと句読点",
            ("ko-KR", StudioCategory::AppearanceLang) => "🌐 인터페이스 언어",
            ("ko-KR", StudioCategory::AppearanceInputMode) => "🖥 입력 표시 모드",
            ("ko-KR", StudioCategory::AppearanceCandidateDesk) => "📐 배열 및 글꼴 크기",
            ("ko-KR", StudioCategory::AppearanceThemeCapsule) => "🎭 테마 및 상태 캡슐",
            ("ko-KR", StudioCategory::KeyboardCjkPhonetic) => "🔊 한어(C): 표음/주음",
            ("ko-KR", StudioCategory::KeyboardCjkShape) => "🐉 한어(C): 용감/자형",
            ("ko-KR", StudioCategory::KeyboardJkEurasian) => "🌍 일한(JK) 및 유라시아 트리",
            ("ko-KR", StudioCategory::KeyboardHotkeysPunct) => "⚡ 빠른 전환 및 문장부호",
            (_, StudioCategory::AppearanceLang) => "🌐 介面顯示語系",
            (_, StudioCategory::AppearanceInputMode) => "🖥 輸入操作模式",
            (_, StudioCategory::AppearanceCandidateDesk) => "📐 候選座排列與字體尺寸",
            (_, StudioCategory::AppearanceThemeCapsule) => "🎭 色彩主題與狀態指示",
            (_, StudioCategory::KeyboardCjkPhonetic) => "🔊 漢語(C)：音韻注音系",
            (_, StudioCategory::KeyboardCjkShape) => "🐉 漢語(C)：龍龕形音與字形碼",
            (_, StudioCategory::KeyboardJkEurasian) => "🌍 日韓(JK)與歐亞全譜樹",
            (_, StudioCategory::KeyboardHotkeysPunct) => "⚡ 快速切換與標點常用鍵",
            _ => self.sidebar_label_i18n(ui_lang),
        }
    }

    pub fn page_header_i18n(&self, ui_lang: &str) -> (&'static str, &'static str) {
        match (ui_lang, self) {
            ("en-US", StudioCategory::Appearance | StudioCategory::AppearanceLang) => (
                "🎨 Appearance & Desk ▸ 🌐 Interface Language",
                "Configure UI display language with automatic cross-locale profile adaptation.",
            ),
            ("en-US", StudioCategory::AppearanceInputMode) => (
                "🎨 Appearance & Desk ▸ 🖥 Input Display Mode",
                "Choose between Smart Auto switching, Floating RuhI Desk (Follow/Fixed), or Windows TSF Inline mode.",
            ),
            ("en-US", StudioCategory::AppearanceCandidateDesk) => (
                "🎨 Appearance & Desk ▸ 📐 Candidate Orientation & Font Sizes",
                "Configure candidate orientation and adjust desk font size, candidate font size, opacity, and max line length.",
            ),
            ("en-US", StudioCategory::AppearanceThemeCapsule) => (
                "🎨 Appearance & Desk ▸ 🎭 Color Theme & Status Indicator",
                "Configure system/dark/light color themes and post-commit mini status capsule behavior.",
            ),
            ("en-US", StudioCategory::KeyboardLayout | StudioCategory::KeyboardCjkPhonetic) => (
                "⌨ Keyboard & Eurasian Tree ▸ 🌏 CJK ▸ 🔊 C: Phonetic (Bopomofo)",
                "Configure Sinitic phonetic input methods (Hsu, Standard Daikien, ETen 26-Key), Q-key candidate cycle, and reserved phonetic slots.",
            ),
            ("en-US", StudioCategory::KeyboardCjkShape) => (
                "⌨ Keyboard & Eurasian Tree ▸ 🌏 CJK ▸ 🧩 C: Structural Shape & Numeric",
                "Configure Array 30, Cangjie, Four-Corner 4×5 Numpad symbol behavior, reverse-lookup coach, and reserved structural slots.",
            ),
            ("en-US", StudioCategory::KeyboardJkEurasian) => (
                "⌨ Keyboard & Eurasian Tree ▸ 🌍 JK (Japanese/Korean) & Full Eurasian Spectrum Tree",
                "Full Eurasian language family hierarchy with active Japanese/Korean/Latin nodes and reserved Indo-Aryan, SE Asian, Cyrillic, Greek & Semitic slots.",
            ),
            ("en-US", StudioCategory::KeyboardHotkeysPunct) => (
                "⌨ Keyboard & Eurasian Tree ▸ ⚡ Quick-Switch Cycle, Hotkeys & Punctuation",
                "Configure Caps Lock / Ctrl+CapsLock / Shift+CapsLock rotation cycle, Spacebar commit behavior, and Punctuation commit modes (A/B/C).",
            ),
            ("en-US", StudioCategory::MixedEnglish) => (
                "🔤 Eurasian Smart Mixed Input Strategy",
                "Configure orthogonal low-collision mixed input (Standard Daikien, Cangjie, Four-Corner, Array 30), contextual stream arbitration, and thresholds.",
            ),
            ("en-US", StudioCategory::Grammar) => (
                "🧠 Syntactic Topology & Grammar Gravity Field",
                "Configure syntactic constructions, bigram transitions, structural particles, and custom lexicon hot-reload.",
            ),
            ("en-US", StudioCategory::AiAndPrivacy) => (
                "🛡 Zero-PII Privacy Gateway & Cloud Feedback",
                "Configure heuristic PII filtering, opt-in anonymous sentence revision telemetry, and Tailcat lexicon updates.",
            ),

            ("en-US", StudioCategory::About) => (
                "ℹ About RuhI & Universal Multilingual Vision",
                "RuhI philosophy, cross-language universal input architecture, engine version, and lexicon status.",
            ),
            ("ja-JP", StudioCategory::Appearance | StudioCategory::AppearanceLang) => (
                "🎨 外観と候補デスク ▸ 🌐 表示言語（UI 言語設定）",
                "環境設定センターおよび入力デスクの表示言語を設定します（導入ロケール自動連動対応）。",
            ),
            ("ja-JP", StudioCategory::AppearanceInputMode) => (
                "🎨 外観と候補デスク ▸ 🖥 入力表示モード",
                "スマート自動切替、RuhI 独立入力デスク（カーソル追従／画面固定）、MS-IME インライン互換モードから選択します。",
            ),
            ("ja-JP", StudioCategory::AppearanceCandidateDesk) => (
                "🎨 外観と候補デスク ▸ 📐 候補配列とフォントサイズ",
                "候補ウィンドウの並び方向、入力デスク文字サイズ、候補文字サイズ、不透明度、1行最大文字数を整列テーブルで調整します。",
            ),
            ("ja-JP", StudioCategory::AppearanceThemeCapsule) => (
                "🎨 外観と候補デスク ▸ 🎭 カラーテーマと状態カプセル",
                "OS 連動／ダーク／ライトテーマの即時切替および確定後のミニ状態カプセル表示動作を設定します。",
            ),
            ("ja-JP", StudioCategory::KeyboardLayout | StudioCategory::KeyboardCjkPhonetic) => (
                "⌨ キー配列と系統樹 ▸ 🌏 CJK ▸ 🔊 漢語(C)：表音・注音系",
                "許氏双拼・標準大千・倚天26キー注音、Qキー候補ローテーション上限、および表音系予約スロットを設定します。",
            ),
            ("ja-JP", StudioCategory::KeyboardCjkShape) => (
                "⌨ キー配列と系統樹 ▸ 🌏 CJK ▸ 🧩 漢語(C)：字形・数値コード系",
                "行列30、倉頡、四角号碼（4×5テンキー記号動作）、逆引きコーチ提示、および字形系予約スロットを設定します。",
            ),
            ("ja-JP", StudioCategory::KeyboardJkEurasian) => (
                "⌨ キー配列と系統樹 ▸ 🌍 日韓(JK)とユーラシア全言語系統樹",
                "日本語ローマ字・韓国語ローマ字の直接接続と、インド・東南アジア・汎欧州・キリル・ギリシア等の予約系統樹。",
            ),
            ("ja-JP", StudioCategory::KeyboardHotkeysPunct) => (
                "⌨ キー配列と系統樹 ▸ ⚡ 高速切替サイクル・空白キー・句読点設定",
                "Caps Lock／Ctrl+CapsLock／Shift+CapsLock 切替サイクル、スペースキー動作、句読点確定モード（A/B/C）を設定します。",
            ),
            ("ja-JP", StudioCategory::MixedEnglish) => (
                "🔤 欧亜スマート混在入力戦略",
                "低重碼・直交配列モード（標準大千注音・倉頡・四角号碼・行列30）の欧亜混打と仲裁ポリシーを設定します。",
            ),
            ("ja-JP", StudioCategory::Grammar) => (
                "🧠 構文トポロジーと文法重力場",
                "高次構文構造、二階遷移ルール、構造助詞の意味解析、ユーザー辞書のホットリロードを設定します。",
            ),
            ("ja-JP", StudioCategory::AiAndPrivacy) => (
                "🛡 Zero-PII プライバシーゲートウェイとクラウド同期",
                "個人情報フィルタリング、匿名修正文ペアの送信周期、Tailcat 辞書更新を設定します。",
            ),

            ("ja-JP", StudioCategory::About) => (
                "ℹ RuhI について・多言語ユニバーサルビジョン",
                "RuhI の設計哲学、全言語ユニバーサル入力ブループリント、システムバージョンと辞書ステータス。",
            ),
            ("ko-KR", StudioCategory::Appearance | StudioCategory::AppearanceLang) => (
                "🎨 외관 및 후보창 ▸ 🌐 인터페이스 표시 언어",
                "환경설정 센터 및 입력창의 표시 언어를 설정합니다 (설치된 입력 로케일 자동 연동).",
            ),
            ("ko-KR", StudioCategory::AppearanceInputMode) => (
                "🎨 외관 및 후보창 ▸ 🖥 입력 표시 모드",
                "앱별 자동 전환, RuhI 독립 입력창(커서 추적/화면 고정), MS-IME 인라인 호환 모드 중 선택합니다.",
            ),
            ("ko-KR", StudioCategory::AppearanceCandidateDesk) => (
                "🎨 외관 및 후보창 ▸ 📐 후보창 배열 및 글꼴 크기",
                "후보창 배열 방향과 입력창 글꼴 크기, 후보 글꼴 크기, 불투명도, 한 줄 최대 글자 수를 테이블에서 조정합니다.",
            ),
            ("ko-KR", StudioCategory::AppearanceThemeCapsule) => (
                "🎨 외관 및 후보창 ▸ 🎭 색상 테마 및 상태 표시 캡슐",
                "Windows 시스템 연동 / 다크 / 라이트 테마 즉시 전환 및 입력 완료 후 미니 상태 캡슐 동작을 설정합니다.",
            ),
            ("ko-KR", StudioCategory::KeyboardLayout | StudioCategory::KeyboardCjkPhonetic) => (
                "⌨ 키보드 및 유라시아 트리 ▸ 🌏 CJK ▸ 🔊 한어(C): 표음/주음계",
                "허씨 쌍핀, 표준 대천, 의천 26키 주음, Q키 후보 순환 상한 및 표음계 예약 슬롯을 설정합니다.",
            ),
            ("ko-KR", StudioCategory::KeyboardCjkShape) => (
                "⌨ 키보드 및 유라시아 트리 ▸ 🌏 CJK ▸ 🧩 한어(C): 자형 및 숫자 코드",
                "행열 30, 창힐, 사각호마(4×5 숫자패드 기호 동작), 역검색 코치 및 자형계 예약 슬롯을 설정합니다.",
            ),
            ("ko-KR", StudioCategory::KeyboardJkEurasian) => (
                "⌨ 키보드 및 유라시아 트리 ▸ 🌍 일한(JK) 및 유라시아 전 언어 계통 트리",
                "일본어/한국어 로마자 입력 직접 연결 및 인도-아리안, 동남아시아, 범유럽, 키릴, 그리스어 등 유라시아 예약 트리.",
            ),
            ("ko-KR", StudioCategory::KeyboardHotkeysPunct) => (
                "⌨ 키보드 및 유라시아 트리 ▸ ⚡ 빠른 전환 순환, 스페이스바 및 문장부호",
                "Caps Lock / Ctrl+CapsLock / Shift+CapsLock 순환 목록, 스페이스바 동작 및 문장부호 확정 모드(A/B/C)를 설정합니다.",
            ),
            ("ko-KR", StudioCategory::MixedEnglish) => (
                "🔤 유라시아 스마트 혼용 입력 전략",
                "저중복·직교 배열 모드(표준 대천 주음, 창힐, 사각호마, 행열 30)의 스마트 혼용 입력 및 중재 정책을 설정합니다.",
            ),
            ("ko-KR", StudioCategory::Grammar) => (
                "🧠 구문 토폴로지 및 문법 중력장",
                "고차 구문 구조, 2차 전이 규칙, 구조 조사 의미 분석 및 사용자 사전 핫 리로드를 설정합니다.",
            ),
            ("ko-KR", StudioCategory::AiAndPrivacy) => (
                "🛡 Zero-PII 개인정보 보호 게이트웨이 및 클라우드 피드백",
                "개인정보 필터링, 익명 수정 문장 쌍 전송 주기 및 Tailcat 사전 업데이트를 설정합니다.",
            ),

            ("ko-KR", StudioCategory::About) => (
                "ℹ RuhI 정보 및 다국어 유니버설 비전",
                "RuhI 브랜드 철학, 전 언어 범용 입력기 청사진, 시스템 버전 및 사전 상태 정보.",
            ),
            _ => self.page_header(),
        }
    }

    pub fn page_header(&self) -> (&'static str, &'static str) {
        match self {
            StudioCategory::Appearance | StudioCategory::AppearanceLang => (
                "🎨 外觀與候選座 ▸ 🌐 介面顯示語系",
                "設定偏好設定中心與輸入座之顯示語系，支援隨 Windows 安裝語系自動無感切換。",
            ),
            StudioCategory::AppearanceInputMode => (
                "🎨 外觀與候選座 ▸ 🖥 輸入操作模式",
                "切換智慧自動判斷模式、如喜獨立懸浮輸入座（游標跟隨／螢幕固定）或微軟新注音行內相容模式。",
            ),
            StudioCategory::AppearanceCandidateDesk => (
                "🎨 外觀與候選座 ▸ 📐 候選座排列與字體尺寸",
                "設定候選窗橫向／直向排列方向，並透過對齊表格與自適應等寬滑桿調整字體大小、不透明度與單行最大字數。",
            ),
            StudioCategory::AppearanceThemeCapsule => (
                "🎨 外觀與候選座 ▸ 🎭 色彩主題與狀態指示",
                "即時切換跟隨系統／極致深色／宣紙淺色主題，並設定送出字串後的迷你狀態膠囊行為。",
            ),
            StudioCategory::KeyboardLayout | StudioCategory::KeyboardCjkPhonetic => (
                "⌨ 鍵盤配置與常用鍵 ▸ 🌏 東亞漢字文化圈 (CJK) ▸ 🔊 漢語(C)：音韻注音系",
                "管理漢語音韻拼讀輸入法（許氏雙拼、標準大千、倚天 26 鍵）、Q 鍵候選輪換滑桿與音韻分支預留槽位。",
            ),
            StudioCategory::KeyboardCjkShape => (
                "⌨ 鍵盤配置與常用鍵 ▸ 🌏 東亞漢字文化圈 (CJK) ▸ 🧩 漢語(C)：字形與數位檢字系",
                "管理漢字結構拆碼與數位檢字（行列 30、倉頡、智慧型四角號碼 4×5 小鍵盤）及反查教練設定。",
            ),
            StudioCategory::KeyboardJkEurasian => (
                "⌨ 鍵盤配置與常用鍵 ▸ 🌍 日韓(JK)與歐亞全譜系樹狀圖 (Eurasian Full-Spectrum Tree)",
                "涵蓋東亞 CJK（漢・日・韓）、古印度雅利安梵巴文字圈、大陸東南亞、泛歐印歐與閃含語系之巢狀分類樹，已實作模組直接掛載。",
            ),
            StudioCategory::KeyboardHotkeysPunct => (
                "⌨ 鍵盤配置與常用鍵 ▸ ⚡ 快速切換迴圈、常用鍵與標點結算",
                "設定 Caps Lock／Ctrl+CapsLock／Shift+CapsLock 循環切換清單、空白鍵組字行為與標點符號結算模式（A / B / C）。",
            ),
            StudioCategory::MixedEnglish => (
                "🔤 歐亞智慧混打策略",
                "針對鍵位空間正交、低重碼率之輸入模式（標準大千注音、倉頡、智慧型四角號碼、行列 30）提供免切換歐亞混打；高重碼 26 鍵模式則由 Caps Lock 確定性切換守護。",
            ),
            StudioCategory::Grammar => (
                "🧠 現代漢語八大語法引力場",
                "配置高階句法結構拓撲、動補/離合/框式介詞、ê/kah 結構助詞深層語意與自建詞庫熱重載。",
            ),
            StudioCategory::AiAndPrivacy => (
                "🛡 隱私過濾閘門與匿名改字雲端進化回饋",
                "配置啟發式個資過濾閘門與匿名改字雙句（錯誤句 vs. 修正句）雲端回傳週期。",
            ),
            StudioCategory::About => (
                "ℹ 關於如喜與全語言願景",
                "如喜輸入法品牌哲學、全語言通用輸入法藍圖、系統版本與詞庫狀態資訊。",
            ),
        }
    }
}

/// 滑桿表格列定義（用於 `egui::Grid` 多欄對齊與統一自適應滑桿寬度）
pub struct SliderTableRowSpec<'a> {
    pub id_key: &'a str,
    pub label: &'a str,
    pub val: &'a mut i32,
    pub range: std::ops::RangeInclusive<i32>,
    pub unit: &'a str,
}

/// 桌面應用程式主狀態結構
pub struct RuhiStudioApp {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub shell: UniversalAppShell,
    pub settings: RuhiSettings,
    pub current_category: StudioCategory,
    pub appearance_tree_expanded: bool,
    pub keyboard_tree_expanded: bool,
    pub sync_client: TelemetrySyncClient,
    pub slider_text_buffers: std::collections::HashMap<String, String>,
    pub last_wakeup_check: std::time::Instant,
    pub last_wakeup_ts: u64,
    pub last_config_mtime: Option<std::time::SystemTime>,
    pub cli_lang_override: Option<String>,
    pub studio_logo_dark_tex: Option<egui::TextureHandle>,
    pub studio_logo_light_tex: Option<egui::TextureHandle>,
}

impl RuhiStudioApp {
    pub fn new(initial_category: StudioCategory) -> Self {
        Self::new_with_mode(initial_category, None, false)
    }

    pub fn to_shell_i18n_lang(ui_lang: &str) -> &'static str {
        let low = ui_lang.trim().to_ascii_lowercase();
        if low == "auto" {
            "auto"
        } else if low == "en-us" || low == "en_us" {
            "en_US"
        } else if low == "ja-jp" || low == "ja_jp" {
            "ja_JP"
        } else if low == "ko-kr" || low == "ko_kr" {
            "ko_KR"
        } else {
            "zh_TW"
        }
    }

    pub fn effective_ui_lang(&self) -> &'static str {
        let cfg_lang = self.settings.appearance.ui_lang.trim();
        if !cfg_lang.is_empty() && !cfg_lang.eq_ignore_ascii_case("auto") {
            return resolve_effective_ui_lang(cfg_lang);
        }
        if let Some(ref cli_lang) = self.cli_lang_override {
            if !cli_lang.trim().is_empty() {
                return resolve_effective_ui_lang(cli_lang);
            }
        }
        resolve_effective_ui_lang(cfg_lang)
    }

    pub fn new_with_lang(initial_category: StudioCategory, cli_lang_override: Option<String>) -> Self {
        Self::new_with_mode(initial_category, cli_lang_override, false)
    }

    pub fn new_with_mode(
        initial_category: StudioCategory,
        cli_lang_override: Option<String>,
        _unused: bool,
    ) -> Self {
        let root_dir = resolve_ruhi_root();
        let data_dir = resolve_data_dir(&root_dir);
        let settings = load_settings(&root_dir);

        let app_id = "ruhi_settings";
        let win_title = "RuhI（如喜）輸入法 — 現代化偏好設定中心";

        let mut shell = UniversalAppShell::new(app_id, win_title, Some(&root_dir));

        let init_lang = if !settings.appearance.ui_lang.trim().is_empty()
            && !settings.appearance.ui_lang.trim().eq_ignore_ascii_case("auto")
        {
            Self::to_shell_i18n_lang(&settings.appearance.ui_lang)
        } else if let Some(ref cli_lang) = cli_lang_override {
            if !cli_lang.trim().is_empty() {
                Self::to_shell_i18n_lang(cli_lang)
            } else {
                "auto"
            }
        } else {
            "auto"
        };
        shell.set_language(init_lang);

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
            .map(|c| (c.tree_depth(), c.sidebar_label()))
            .collect();
        shell.ensure_sidebar_min_width(&nav_items);

        let sync_client = TelemetrySyncClient::new(Some(&root_dir));

        let last_config_mtime = std::fs::metadata(root_dir.join("config.json"))
            .and_then(|m| m.modified())
            .ok();
        let last_wakeup_ts = [
            root_dir.join("settings_wakeup.json"),
            root_dir.join("studio_wakeup.json"),
            root_dir.join("dist").join("settings_wakeup.json"),
            root_dir.join("dist").join("studio_wakeup.json"),
            root_dir.join("scripts").join("settings_wakeup.json"),
            root_dir.join("scripts").join("studio_wakeup.json"),
        ]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|s| serde_json::from_str::<Value>(&s).ok())
        .filter_map(|v| {
            v.get("ts")
                .or_else(|| v.get("timestamp"))
                .and_then(Value::as_u64)
        })
        .max()
        .unwrap_or(0);

        Self {
            root_dir,
            data_dir,
            shell,
            settings,
            current_category: initial_category,
            appearance_tree_expanded: true,
            keyboard_tree_expanded: true,
            sync_client,
            slider_text_buffers: std::collections::HashMap::new(),
            last_wakeup_check: std::time::Instant::now(),
            last_wakeup_ts,
            last_config_mtime,
            cli_lang_override,
            studio_logo_dark_tex: None,
            studio_logo_light_tex: None,
        }
    }

    /// 以結構化對齊表格 (`egui::Grid`) 繪製一組或多組「設定項目 ｜ ： ｜ 自適應等寬滑桿 ｜ ➖ ｜ 數值輸入框 ｜ 單位 ｜ ➕」控制列。
    /// 嚴格遵循 `rud()` 兩階段空間配給定律與獨立冒號欄規範，確保所有滑桿統一寬度並自動填滿剩餘水平空間，且在高倍縮放（如 180%）下絕不超出視窗右緣。
    fn render_slider_table(
        ui: &mut egui::Ui,
        grid_id: &str,
        rows: &mut [SliderTableRowSpec<'_>],
        scale: f32,
        text_buffers: &mut std::collections::HashMap<String, String>,
        show_header: bool,
        palette: &SemanticPalette,
    ) -> bool {
        let clip_avail_w = (ui.clip_rect().max.x - ui.cursor().min.x - 14.0 * scale).max(180.0);
        let avail_w = ui.available_width().min(clip_avail_w);
        let font_px = (14.0 * scale).round();
        let font_id = egui::FontId::proportional(font_px);

        let measure_px = |txt: &str| -> f32 {
            ui.fonts(|f| {
                f.layout_no_wrap(txt.to_string(), font_id.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            })
        };

        let max_label_w = rows
            .iter()
            .map(|r| measure_px(r.label))
            .fold(measure_px("輸入座單行最大字數"), f32::max)
            .ceil()
            + 4.0 * scale;

        let colon_w = measure_px("：").ceil().max(10.0 * scale);
        let btn_w = (measure_px("➖") + 10.0 * scale).ceil().max(26.0 * scale);
        let num_box_w = (measure_px("100") + 16.0 * scale).ceil().max(44.0 * scale);
        let unit_w = rows
            .iter()
            .map(|r| measure_px(r.unit))
            .fold(measure_px("單位"), f32::max)
            .ceil()
            + 4.0 * scale;
        let col_gap = (6.0 * scale).round();
        let row_gap = (7.0 * scale).round();
        let total_gaps_w = col_gap * 6.0;

        let fixed_w =
            max_label_w + colon_w + btn_w + num_box_w + unit_w + btn_w + total_gaps_w;
        let col2_w = (avail_w - fixed_w - 16.0 * scale).max(48.0);
        let unified_slider_w = (col2_w - 4.0 * scale).max(44.0);
        let row_h = (25.0 * scale).round();
        let header_h = (20.0 * scale).round();

        let mut any_changed = false;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(col_gap, row_gap);
            ui.spacing_mut().interact_size = egui::vec2(20.0 * scale, (row_h - 2.0 * scale).max(18.0));
            ui.spacing_mut().button_padding = egui::vec2(3.0 * scale, 2.0 * scale);

            egui::Grid::new(grid_id)
                .num_columns(7)
                .spacing([col_gap, row_gap])
                .striped(true)
                .show(ui, |ui| {
                    if show_header {
                        ui.allocate_ui_with_layout(
                            egui::vec2(max_label_w, header_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("設定項目")
                                        .size(12.0 * scale)
                                        .strong()
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(colon_w, header_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label("");
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col2_w, header_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_max_width(col2_w);
                                ui.label(
                                    egui::RichText::new("滑桿調節")
                                        .size(12.0 * scale)
                                        .strong()
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(btn_w, header_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("減")
                                        .size(11.5 * scale)
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(num_box_w, header_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("數值")
                                        .size(12.0 * scale)
                                        .strong()
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(unit_w, header_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("單位")
                                        .size(12.0 * scale)
                                        .strong()
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(btn_w, header_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("增")
                                        .size(11.5 * scale)
                                        .color(palette.fg_muted.to_egui()),
                                );
                            },
                        );
                        ui.end_row();
                    }

                    for spec in rows.iter_mut() {
                        let min_v = *spec.range.start();
                        let max_v = *spec.range.end();
                        let id_key = spec.id_key;
                        let unit = spec.unit;

                        // Col 0: 項目名稱（固定最大標籤欄寬，左對齊）
                        ui.allocate_ui_with_layout(
                            egui::vec2(max_label_w, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(egui::RichText::new(spec.label).size(font_px));
                            },
                        );

                        // Col 1: 獨立冒號軸線
                        ui.allocate_ui_with_layout(
                            egui::vec2(colon_w, row_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(egui::RichText::new("：").size(font_px).strong());
                            },
                        );

                        // Col 2: 統一寬度並自適應剩餘空間之滑桿
                        ui.allocate_ui_with_layout(
                            egui::vec2(col2_w, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_max_width(col2_w);
                                ui.spacing_mut().slider_width = unified_slider_w;
                                if ui
                                    .add(
                                        egui::Slider::new(spec.val, min_v..=max_v)
                                            .show_value(false)
                                            .trailing_fill(true),
                                    )
                                    .on_hover_text(format!(
                                        "拖曳調整 {}（範圍 {min_v} ~ {max_v} {unit}）",
                                        spec.label
                                    ))
                                    .changed()
                                {
                                    any_changed = true;
                                    text_buffers.insert(id_key.to_string(), spec.val.to_string());
                                }
                            },
                        );

                        // Col 3: 減少按鈕 (➖)
                        if ui
                            .add_sized([btn_w, row_h - 2.0 * scale], egui::Button::new("➖"))
                            .on_hover_text(format!("減少 1 {unit} (範圍 {min_v}~{max_v})"))
                            .clicked()
                        {
                            let next_v = (*spec.val - 1).clamp(min_v, max_v);
                            if next_v != *spec.val {
                                *spec.val = next_v;
                                any_changed = true;
                            }
                            text_buffers.insert(id_key.to_string(), spec.val.to_string());
                        }

                        // Col 4: 數值直接輸入框 (TextEdit)
                        let buf = text_buffers
                            .entry(id_key.to_string())
                            .or_insert_with(|| spec.val.to_string());

                        let edit_resp = ui
                            .add_sized(
                                [num_box_w, row_h - 2.0 * scale],
                                egui::TextEdit::singleline(buf)
                                    .id_source(format!("ruhi_num_input_{id_key}"))
                                    .desired_width((num_box_w - 8.0 * scale).max(20.0))
                                    .horizontal_align(egui::Align::Center),
                            )
                            .on_hover_text(format!(
                                "可直接點擊輸入數值 ({min_v} ~ {max_v} {unit})，按 Enter 或點擊外部立即套用"
                            ));

                        if edit_resp.has_focus() {
                            let normalized: String = buf
                                .chars()
                                .filter_map(|c| {
                                    if ('０'..='９').contains(&c) {
                                        char::from_u32((c as u32) - ('０' as u32) + ('0' as u32))
                                    } else if c.is_ascii_digit() {
                                        Some(c)
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if *buf != normalized {
                                *buf = normalized;
                            }
                            if let Ok(parsed) = buf.parse::<i32>() {
                                if (min_v..=max_v).contains(&parsed) && parsed != *spec.val {
                                    *spec.val = parsed;
                                    any_changed = true;
                                }
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let committed =
                                    buf.parse::<i32>().unwrap_or(*spec.val).clamp(min_v, max_v);
                                if committed != *spec.val {
                                    *spec.val = committed;
                                    any_changed = true;
                                }
                                *buf = spec.val.to_string();
                                edit_resp.surrender_focus();
                            }
                        } else if edit_resp.lost_focus() {
                            let committed =
                                buf.parse::<i32>().unwrap_or(*spec.val).clamp(min_v, max_v);
                            if committed != *spec.val {
                                *spec.val = committed;
                                any_changed = true;
                            }
                            *buf = spec.val.to_string();
                        } else {
                            let expected = spec.val.to_string();
                            if *buf != expected {
                                *buf = expected;
                            }
                        }

                        // Col 5: 單位欄（固定欄寬置中，杜絕 px / % / 字 寬度差異造成 + 鍵錯位）
                        ui.allocate_ui_with_layout(
                            egui::vec2(unit_w, row_h),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(egui::RichText::new(unit).size(font_px).strong());
                            },
                        );

                        // Col 6: 增加按鈕 (➕)
                        if ui
                            .add_sized([btn_w, row_h - 2.0 * scale], egui::Button::new("➕"))
                            .on_hover_text(format!("增加 1 {unit} (範圍 {min_v}~{max_v})"))
                            .clicked()
                        {
                            let next_v = (*spec.val + 1).clamp(min_v, max_v);
                            if next_v != *spec.val {
                                *spec.val = next_v;
                                any_changed = true;
                            }
                            text_buffers.insert(id_key.to_string(), spec.val.to_string());
                        }

                        ui.end_row();
                    }
                });
        });

        any_changed
    }

    /// 繪製單列「滑桿 ＋ 可直接點擊鍵盤輸入之獨立數值框 (`TextEdit`) ＋ 微調按鈕 (`-` / `+`)」，
    /// 底層委託 `render_slider_table` 確保與多列對齊表格共享自適應剩餘空間配置。
    fn render_slider_with_numeric_input(
        ui: &mut egui::Ui,
        id_key: &str,
        label: &str,
        val: &mut i32,
        range: std::ops::RangeInclusive<i32>,
        unit: &str,
        scale: f32,
        text_buffers: &mut std::collections::HashMap<String, String>,
    ) -> bool {
        let clean_label = label.trim_end_matches('：').trim_end_matches(':');
        let dummy_palette = SemanticPalette::studio_dark();
        let mut rows = [SliderTableRowSpec {
            id_key,
            label: clean_label,
            val,
            range,
            unit,
        }];
        Self::render_slider_table(
            ui,
            &format!("ruhi_single_slider_grid_{id_key}"),
            &mut rows,
            scale,
            text_buffers,
            false,
            &dummy_palette,
        )
    }

    fn auto_save_settings_quiet(&mut self) {
        ensure_keyboard_modes_consistent(&mut self.settings);
        if save_settings(&self.root_dir, &mut self.settings).is_ok() {
            self.last_config_mtime = std::fs::metadata(self.root_dir.join("config.json"))
                .and_then(|m| m.modified())
                .ok();
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

    /// 繪製「🎨 外觀與候選座」四大樹狀子分枝之頂部快速切換導覽列
    fn render_appearance_branch_switcher(&mut self, ui: &mut egui::Ui) {
        let cur_lang = self.effective_ui_lang();
        let active_branch = match self.current_category {
            StudioCategory::Appearance => StudioCategory::AppearanceLang,
            other => other,
        };
        ui.horizontal_wrapped(|ui| {
            for &branch in StudioCategory::APPEARANCE_BRANCHES {
                let is_sel = active_branch == branch;
                let lbl = branch.branch_short_label_i18n(cur_lang);
                if ui
                    .selectable_label(
                        is_sel,
                        egui::RichText::new(lbl)
                            .size(13.0 * self.shell.ui_scale)
                            .strong(),
                    )
                    .clicked()
                {
                    self.appearance_tree_expanded = true;
                    self.current_category = branch;
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(8.0);
    }

    /// 樹狀分枝一：🌐 介面顯示語系
    fn render_appearance_lang_page(&mut self, ui: &mut egui::Ui) {
        self.render_appearance_branch_switcher(ui);
        let palette = self.shell.palette;

        ui.group(|ui| {
            let ui_lang = self.effective_ui_lang();
            let grp_title = match ui_lang {
                "en-US" => "🌐 Interface Language & Cross-Locale Profile Adaptation",
                "ja-JP" => "🌐 表示言語（UI 言語と導入ロケール連動）",
                "ko-KR" => "🌐 인터페이스 표시 언어 (설치 로케일 자동 연동)",
                _ => "🌐 介面顯示語系（穿透各安裝語系自動適配）",
            };
            ui.heading(grp_title);
            ui.add_space(6.0);
            let mut lang_changed = false;
            for (code, label) in [
                (
                    "auto",
                    "自動跟隨安裝語系／系統語系 (Auto: Follow Active Language Profile / OS)",
                ),
                ("zh-TW", "繁體中文（台灣 · Traditional Chinese）"),
                ("en-US", "English（英文／泛歐語系介面 · Pan-European UI）"),
                ("ja-JP", "日本語（日文介面 · Japanese UI）"),
                ("ko-KR", "한국어（韓文介面 · Korean UI）"),
            ] {
                if ui
                    .radio_value(
                        &mut self.settings.appearance.ui_lang,
                        code.to_string(),
                        label,
                    )
                    .changed()
                {
                    lang_changed = true;
                }
            }
            if lang_changed {
                self.cli_lang_override = None;
                let eff = self.effective_ui_lang();
                self.shell.set_language(Self::to_shell_i18n_lang(eff));
                self.auto_save_settings_quiet();
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "說明：選擇「自動跟隨」時，當您在 Windows 切換至繁體中文、日文、韓文或英文輸入法設定檔，介面與狀態膠囊將自動切換對應語系。",
                )
                .color(palette.fg_muted.to_egui()),
            );
        });
    }

    /// 樹狀分枝二：🖥️ 輸入操作模式（微軟新注音 vs 如喜輸入座）
    fn render_appearance_input_mode_page(&mut self, ui: &mut egui::Ui) {
        self.render_appearance_branch_switcher(ui);
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("🖥️ 輸入操作模式（微軟新注音 vs 如喜輸入座）");
            ui.add_space(6.0);

            let mut changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "smart_auto".to_string(),
                "自動判斷 APP 切換最佳輸入模式（預設推薦）",
                "日常軟體（記事本、瀏覽器、通訊軟體）優先使用獨立如喜輸入座；遇專業排版軟體（LibreOffice Writer 等）或傳統主控台 (CMD) 時，自動無感切換為微軟新注音模式，兼顧美觀與極致相容！",
                &palette,
            ) {
                changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "floating_follow".to_string(),
                "如喜輸入座：游標跟隨（自然經典風格・ㄅ）",
                "所有應用程式皆以獨立懸浮輸入座緊隨游標下方；若遇到未向系統回報空行幾何資訊之跨平台排版軟體，可能出現空行位置落後現象。",
                &palette,
            ) {
                changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "floating_fixed".to_string(),
                "如喜輸入座：螢幕固定（自然經典風格・ㄆ）",
                "輸入座保持在固定螢幕座標，使用者可隨時用滑鼠自由拖曳至螢幕任意最佳位置，完全不受任何 APP 游標更新異常影響。",
                &palette,
            ) {
                changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.appearance.input_display_mode,
                "inline_underline".to_string(),
                "微軟新注音游標跟隨模式（極致相容）",
                "100% 遵從 Windows TSF 原生行內組字規範，直接於目標視窗之文字輸入游標處繪製底線與拼弦，選字時彈出微軟新注音風格輕巧候選浮窗，相容性最高、排版工作首選。",
                &palette,
            ) {
                changed = true;
            }

            if changed {
                self.auto_save_settings_quiet();
            }
        });
    }

    /// 樹狀分枝三：📐 候選座排列與字體尺寸（採用 7 欄對齊 Table 與統一自適應寬度滑桿）
    fn render_appearance_candidate_desk_page(&mut self, ui: &mut egui::Ui) {
        self.render_appearance_branch_switcher(ui);
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.heading("📐 候選窗排列方向");
            ui.add_space(6.0);

            let mut orient_changed = false;
            if ui
                .radio_value(
                    &mut self.settings.appearance.orientation,
                    "horizontal".to_string(),
                    "橫向流式排列（預設推薦，視線左右平移最自然）",
                )
                .changed()
            {
                orient_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.appearance.orientation,
                    "vertical".to_string(),
                    "直向傳統排列（經典直書自上而下列出）",
                )
                .changed()
            {
                orient_changed = true;
            }
            if orient_changed {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.heading("📏 字體尺寸、不透明度與單行容量（對齊表格・自適應等寬滑桿）");
            ui.add_space(8.0);

            let scale = self.shell.ui_scale;
            let mut rows = [
                SliderTableRowSpec {
                    id_key: "font_size",
                    label: "輸入座字體大小",
                    val: &mut self.settings.appearance.font_size,
                    range: 16..=96,
                    unit: "px",
                },
                SliderTableRowSpec {
                    id_key: "candidate_font_size",
                    label: "候選字體大小",
                    val: &mut self.settings.appearance.candidate_font_size,
                    range: 14..=72,
                    unit: "px",
                },
                SliderTableRowSpec {
                    id_key: "desk_opacity",
                    label: "輸入座不透明度",
                    val: &mut self.settings.appearance.desk_opacity,
                    range: 20..=100,
                    unit: "%",
                },
                SliderTableRowSpec {
                    id_key: "max_composition_chars",
                    label: "輸入座單行最大字數",
                    val: &mut self.settings.appearance.max_composition_chars,
                    range: 8..=80,
                    unit: "字",
                },
            ];

            let slider_changed = Self::render_slider_table(
                ui,
                "ruhi_appearance_desk_sliders_table",
                &mut rows,
                scale,
                &mut self.slider_text_buffers,
                true,
                &palette,
            );

            if slider_changed {
                self.auto_save_settings_quiet();
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "註：右側數值框可直接點擊輸入數字（按 Enter 確認），亦可拖曳滑桿或直接於輸入座 UI 點擊「調校 ▾」即時同步調整。",
                )
                .color(palette.fg_muted.to_egui()),
            );
        });
    }

    /// 樹狀分枝四：🎭 色彩主題與狀態指示
    fn render_appearance_theme_capsule_page(&mut self, ui: &mut egui::Ui) {
        self.render_appearance_branch_switcher(ui);
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("🎭 色彩主題（即時零延遲換膚）");
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
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("💊 送出字串後狀態指示行為");
            ui.add_space(6.0);

            let mut cap_changed = false;
            if ui
                .radio_value(
                    &mut self.settings.appearance.capsule_mode,
                    "mini_capsule".to_string(),
                    "縮為常駐迷你狀態膠囊（隨時可點齒輪 ⚙ 開啟設定，狀態清晰）",
                )
                .changed()
            {
                cap_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.appearance.capsule_mode,
                    "auto_hide".to_string(),
                    "完全隱藏輸入座（純淨極簡模式，由工作列系統匣圖示指示狀態）",
                )
                .changed()
            {
                cap_changed = true;
            }
            if cap_changed {
                self.auto_save_settings_quiet();
            }
        });
    }

    /// 將輸入模式代碼轉為友善顯示名稱
    fn mode_code_display_name(code: &str) -> &'static str {
        match code {
            "user_abbrev" | "abbrev" | "user_dict" => "🔖 自訂詞庫（頭碼／自訂縮略碼）",
            "punct" | "symbol" | "tilde" | "fullwidth" | "none" => "〰 直出全形波浪號「～」",
            other => find_mode_descriptor(other)
                .map(|desc| desc.display_name)
                .unwrap_or("自訂模式"),
        }
    }

    /// 繪製「⌨ 鍵盤配置與常用鍵」四大樹狀子分枝之頂部快速切換導覽列與即時掛載狀態摘要
    fn render_keyboard_branch_switcher(&mut self, ui: &mut egui::Ui) {
        let cur_lang = self.effective_ui_lang();
        let scale = self.shell.ui_scale;
        let palette = self.shell.palette;
        let active_branch = match self.current_category {
            StudioCategory::KeyboardLayout => StudioCategory::KeyboardCjkPhonetic,
            other => other,
        };
        ui.horizontal_wrapped(|ui| {
            for &branch in StudioCategory::KEYBOARD_BRANCHES {
                let is_sel = active_branch == branch;
                let lbl = branch.branch_short_label_i18n(cur_lang);
                if ui
                    .selectable_label(
                        is_sel,
                        egui::RichText::new(lbl).size(13.0 * scale).strong(),
                    )
                    .clicked()
                {
                    self.keyboard_tree_expanded = true;
                    self.current_category = branch;
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(6.0);

        // 頂部即時狀態與一鍵切換控制條：支援直接下拉切換或單擊徽章切換當前預設輸入法（含龍龕形音 4 碼）
        egui::Frame::NONE
            .fill(palette.bg_surface.to_egui())
            .stroke(egui::Stroke::new(1.0_f32, palette.border.to_egui()))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(
                (10.0 * scale).round() as i8,
                (6.0 * scale).round() as i8,
            ))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let mut quick_mode_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("🎯 當前使用輸入法：")
                            .size(12.8 * scale)
                            .strong()
                            .color(palette.accent.to_egui()),
                    );
                    let cur_def_name = Self::mode_code_display_name(&self.settings.keyboard.layout);
                    egui::ComboBox::from_id_salt("ruhi_kb_branch_top_mode_combo")
                        .selected_text(
                            egui::RichText::new(cur_def_name)
                                .size(12.8 * scale)
                                .strong(),
                        )
                        .width(210.0 * scale)
                        .show_ui(ui, |ui| {
                            for desc in ALL_MODES {
                                if ui
                                    .selectable_value(
                                        &mut self.settings.keyboard.layout,
                                        desc.id.to_string(),
                                        desc.display_name,
                                    )
                                    .changed()
                                {
                                    quick_mode_changed = true;
                                }
                            }
                        });

                    ui.add_space(6.0 * scale);
                    let cur_mode_id = find_mode_descriptor(&self.settings.keyboard.layout)
                        .map(|d| d.id.to_string())
                        .unwrap_or_else(|| self.settings.keyboard.layout.clone());
                    for desc in ALL_MODES {
                        let is_cur = cur_mode_id == desc.id;
                        if ui
                            .selectable_label(
                                is_cur,
                                egui::RichText::new(desc.display_name)
                                    .size(11.8 * scale)
                                    .strong(),
                            )
                            .on_hover_text(format!(
                                "一鍵切換當前輸入法為：{}",
                                desc.display_name
                            ))
                            .clicked()
                            && !is_cur
                        {
                            self.settings.keyboard.layout = desc.id.to_string();
                            quick_mode_changed = true;
                        }
                    }
                });
                if quick_mode_changed {
                    self.auto_save_settings_quiet();
                }

                ui.add_space(3.0 * scale);
                ui.horizontal_wrapped(|ui| {
                    let cycle_names: Vec<&str> = self
                        .settings
                        .keyboard
                        .enabled_keyboard_modes
                        .iter()
                        .map(|m| Self::mode_code_display_name(m))
                        .collect();
                    let cycle_str = if cycle_names.is_empty() {
                        "（僅預設）".to_string()
                    } else {
                        cycle_names.join(" ➔ ")
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "🔄 快速切換迴圈 (Ctrl/Shift+CapsLock)：{cycle_str} ｜ 臨時單字/詞 (`)：{} ｜ 雙向切換 (~)：{}",
                            Self::mode_code_display_name(&self.settings.keyboard.backtick_inline_mode),
                            Self::mode_code_display_name(&self.settings.keyboard.tilde_toggle_mode),
                        ))
                        .size(11.8 * scale)
                        .color(palette.fg_muted.to_egui()),
                    );
                });
            });
        ui.add_space(8.0);
    }

    /// 繪製歐亞語系全譜樹中之單一輸入法節點（已實作節點直接接上「設為預設」與「納入快速切換迴圈」；未實作節點顯示預留槽位）
    fn render_spectrum_method_node(
        &mut self,
        ui: &mut egui::Ui,
        mode_code: Option<&str>,
        title: &str,
        family_badge: &str,
        desc: &str,
    ) {
        let scale = self.shell.ui_scale;
        let palette = self.shell.palette;
        let is_default = mode_code
            .map(|c| self.settings.keyboard.layout == c)
            .unwrap_or(false);

        let border_col = if is_default {
            palette.accent.to_egui()
        } else {
            palette.border.to_egui()
        };

        egui::Frame::NONE
            .fill(palette.bg_surface.to_egui())
            .stroke(egui::Stroke::new(
                if is_default { 1.5_f32 } else { 1.0_f32 },
                border_col,
            ))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(
                (10.0 * scale).round() as i8,
                (8.0 * scale).round() as i8,
            ))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                match mode_code {
                    Some(code) => {
                        let mut changed = false;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("✅ 已實作掛載")
                                    .size(11.5 * scale)
                                    .strong()
                                    .color(palette.accent.to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(format!("［{family_badge}］"))
                                    .size(11.5 * scale)
                                    .color(palette.fg_muted.to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(title)
                                    .size(14.0 * scale)
                                    .strong()
                                    .color(palette.fg_bright.to_egui()),
                            );
                        });
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(desc)
                                .size(12.0 * scale)
                                .color(palette.fg_muted.to_egui()),
                        );
                        ui.add_space(5.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .radio_value(
                                    &mut self.settings.keyboard.layout,
                                    code.to_string(),
                                    egui::RichText::new("🔘 設為當前預設輸入法")
                                        .size(12.5 * scale)
                                        .strong(),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.add_space(14.0 * scale);
                            let enabled_count = self.settings.keyboard.enabled_keyboard_modes.len();
                            let mut in_cycle = self
                                .settings
                                .keyboard
                                .enabled_keyboard_modes
                                .iter()
                                .any(|m| m == code);
                            let can_toggle = !in_cycle || enabled_count > 1;
                            if ui
                                .add_enabled(
                                    can_toggle,
                                    egui::Checkbox::new(
                                        &mut in_cycle,
                                        egui::RichText::new(
                                            "☑ 掛載並納入快速切換迴圈 (Ctrl/Shift+CapsLock)",
                                        )
                                        .size(12.5 * scale),
                                    ),
                                )
                                .on_disabled_hover_text("至少須保留 1 種已掛載之鍵盤模式；若欲改用其他模式，請先勾選或點擊「僅單獨掛載此模式」。")
                                .changed()
                            {
                                if in_cycle {
                                    if !self
                                        .settings
                                        .keyboard
                                        .enabled_keyboard_modes
                                        .iter()
                                        .any(|m| m == code)
                                    {
                                        self.settings
                                            .keyboard
                                            .enabled_keyboard_modes
                                            .push(code.to_string());
                                    }
                                    ensure_keyboard_modes_consistent(&mut self.settings);
                                } else {
                                    let _ = unmount_keyboard_mode(&mut self.settings, code);
                                }
                                changed = true;
                            }
                            ui.add_space(10.0 * scale);
                            let is_sole_mode = enabled_count == 1
                                && self
                                    .settings
                                    .keyboard
                                    .enabled_keyboard_modes
                                    .first()
                                    .map(|m| m == code)
                                    .unwrap_or(false);
                            if ui
                                .add_enabled(
                                    !is_sole_mode,
                                    egui::Button::new(if is_sole_mode {
                                        "🎯 目前為唯一單獨掛載模式"
                                    } else {
                                        "🎯 僅單獨掛載此模式"
                                    }),
                                )
                                .on_hover_text("卸載其他所有模式（含許氏注音），僅保留此單一鍵盤模式運作")
                                .clicked()
                            {
                                mount_single_keyboard_mode_only(&mut self.settings, code);
                                changed = true;
                            }
                        });
                        if changed {
                            self.auto_save_settings_quiet();
                        }
                    }
                    None => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("🔌 預留槽位・待掛載")
                                    .size(11.5 * scale)
                                    .color(palette.fg_muted.to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(format!("［{family_badge}］"))
                                    .size(11.5 * scale)
                                    .color(palette.fg_muted.to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(title)
                                    .size(13.5 * scale)
                                    .strong()
                                    .color(palette.fg_main.to_egui()),
                            );
                        });
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(desc)
                                .size(11.8 * scale)
                                .color(palette.fg_muted.to_egui()),
                        );
                    }
                }
            });
    }

    /// 鍵盤樹狀分枝一：🌏 CJK ▸ 🇹🇼 漢語(C)：音韻注音系
    fn render_keyboard_cjk_phonetic_page(&mut self, ui: &mut egui::Ui) {
        self.render_keyboard_branch_switcher(ui);
        let palette = self.shell.palette;
        let scale = self.shell.ui_scale;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("🌏 東亞漢字文化圈 (CJK) ▸ 🇹🇼 漢語支 (C) ▸ 🔊 音韻拼讀系 (Phonetic & Bopomofo)");
            ui.add_space(6.0);

            self.render_spectrum_method_node(
                ui,
                Some("hsu"),
                "許氏雙拼注音（Hsu Bopomofo · 許）",
                "CJK ▸ C ▸ 音韻雙拼",
                "依聲韻出現頻率與手指人體工學配置於 26 字母鍵，盲打節奏最流暢，支援 Caps Lock 單鍵英數對切與 Q 鍵候選快切。",
            );

            // 許氏注音專屬子設定（直接巢狀掛載於許氏節點下方）
            ui.horizontal(|ui| {
                ui.add_space(18.0 * scale);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new("↳ 許氏雙拼專屬進階調校（Q 鍵快速候選輪換）")
                                .size(13.0 * scale)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        if Self::render_slider_with_numeric_input(
                            ui,
                            "q_cycle_max",
                            "Q 鍵快速候選輪換次數上限",
                            &mut self.settings.keyboard.q_cycle_max,
                            1..=8,
                            "次",
                            scale,
                            &mut self.slider_text_buffers,
                        ) {
                            self.auto_save_settings_quiet();
                        }
                        ui.label(
                            egui::RichText::new(
                                "說明：在許氏鍵盤中，單擊 Q 鍵可在首選詞與次選詞間快速切換，連續按滿指定次數自動展開完整選字座。",
                            )
                            .size(11.8 * scale)
                            .color(palette.fg_muted.to_egui()),
                        );
                    });
                });
            });

            ui.add_space(6.0);
            self.render_universal_reverse_coach_panel(ui);

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("longkan"),
                "龍龕形音輸入法（Longkan Morpho-Phonetic 4-Code · 龍）",
                "CJK ▸ C ▸ 整合創新：形音雙拼＋象形＋部件×數字 4 碼",
                "龍龕以形聲結構（形符＋聲符讀音）為核心，相容注音雙拼與漢語拼音，免切換模式即可以部件讀音組字，並提供輸入座內的事後部件融合（技術公開文件：DOI 10.5281/zenodo.23751480；經 scripts/longkan_stats.py 實測：356,536 筆碼表、187,997 組編碼、13,000 單字、42,002 多字詞、單字熵 16.38 bits）：原生相容「部首／部件讀音雙拼（如鋪=金km+甫fu➔kmfu／許氏jnfx、龕=合he+龍lo➔helo）」、「英文字母視覺象形＋聲符＋聲調補碼（如中=oi、品=ooo、肴=乂x+有eo➔xeo/xeo2）」與「部件×數字（如區=cko3、品=ko3、森=mu3、協=tli3、蕊=wsm3）」。",
            );

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("standard"),
                "標準大千注音（Standard Daikien · 注）",
                "CJK ▸ C ▸ 傳統標準",
                "台灣通行最廣之 41 鍵標準大千注音排列，零學習門檻，支援 41 鍵音位正交之「歐亞智慧混打」並完整共享如喜八大語法引擎。",
            );

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("et26"),
                "倚天 26 鍵注音（ETen 26-Key · 倚）",
                "CJK ▸ C ▸ 26 鍵注音",
                "經典倚天 26 鍵英文字母鍵位映射，將聲母與韻母收斂於三排字母區，減輕小指與數字列負擔。",
            );

            ui.add_space(10.0);
            egui::CollapsingHeader::new(
                egui::RichText::new("🔌 漢語音韻分支預留槽位（歐亞全譜系樹・實作後直接掛載）")
                    .size(13.0 * scale)
                    .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "漢語拼音／通用拼音音韻轉寫 (Mandarin Pinyin / Tongyong Pinyin)",
                    "CJK ▸ C ▸ 羅馬拼音預留槽",
                    "預留拉丁字母音節解析器 (Syllable Transducer) 與雙拼方案介面，底層直接共用如喜漢語語法構式引擎。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "台灣閩南語台羅／白話字音韻輸入 (Taigi Tâi-lô / Pe̍h-ōe-jī)",
                    "CJK ▸ C ▸ 台語音韻預留槽",
                    "預留八聲調與鼻化元音拼讀介面，結合既有跨語言 ê/kah 結構助詞分野律輸出教育部推薦台灣閩南語漢字與羅馬字。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "台灣客語拼音／粵語粵拼輸入 (Hakka Pinyin / Cantonese Jyutping)",
                    "CJK ▸ C ▸ 客粵音韻預留槽",
                    "預留客語四縣／海陸腔拼音與香港語言學學會粵拼 (LSHK Jyutping) 音韻映射槽位。",
                );
            });
        });
    }

    /// 鍵盤樹狀分枝二：🌏 CJK ▸ 🇹🇼 漢語(C)：字形與數位碼
    fn render_keyboard_cjk_shape_page(&mut self, ui: &mut egui::Ui) {
        self.render_keyboard_branch_switcher(ui);
        let scale = self.shell.ui_scale;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("🌏 東亞漢字文化圈 (CJK) ▸ 🇹🇼 漢語支 (C) ▸ 🧩 字形拆碼與數位檢字系 (Structural, Morpho-Phonetic & Numeric)");
            ui.add_space(6.0);

            self.render_spectrum_method_node(
                ui,
                Some("longkan"),
                "龍龕形音輸入法（Longkan Morpho-Phonetic 4-Code · 龍）",
                "CJK ▸ C ▸ 整合創新：形音雙拼＋象形＋部件×數字 4 碼",
                "龍龕以形聲結構（形符＋聲符讀音）為核心，相容注音雙拼與漢語拼音，免切換模式即可以部件讀音組字，並提供輸入座內的事後部件融合（技術公開文件：DOI 10.5281/zenodo.23751480；經 scripts/longkan_stats.py 實測：356,536 筆碼表、187,997 組編碼、13,000 單字、42,002 多字詞、單字熵 16.38 bits）：原生相容「部首／部件讀音雙拼（如鋪=金km+甫fu➔kmfu／許氏jnfx、龕=合he+龍lo➔helo）」、「英文字母視覺象形＋聲符＋聲調補碼（如中=oi、品=ooo、肴=乂x+有eo➔xeo/xeo2）」與「部件×數字（如區=cko3、品=ko3、森=mu3、協=tli3、蕊=wsm3）」。",
            );

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("array30"),
                "行列 30 輸入法（Array 30 · 行）",
                "CJK ▸ C ▸ 筆形座標字根",
                "以 0~9 十種基本筆形搭配首尾座標定位於 30 鍵，內建一二級簡碼、?/* 萬用字元查詢與 W 鍵十類符號表。",
            );

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("cangjie"),
                "倉頡輸入法（Cangjie · 倉）",
                "CJK ▸ C ▸ 哲理字形拆碼",
                "經典哲理筆畫輔助字形拆碼，支援 Z 鍵萬用字元查詢，重碼率極低、適合高速古籍與專業文獻錄入。",
            );

            ui.add_space(8.0);
            self.render_spectrum_method_node(
                ui,
                Some("four_corner"),
                "智慧型四角號碼（Four-Corner 4×5 Numpad · 四）",
                "CJK ▸ C ▸ 4×5 數位小鍵盤",
                "王雲五四角號碼數位化演進版：專為右側 4×5 數字小鍵盤單手盲打設計，支援「-」詞彙前導（二字 4 碼 -1,2+1,2 ／ 三字以上 -1,3[1×n]2,4）、正異體字形共享與「*」萬用字元查詢。",
            );

            // 四角號碼專屬子設定（4×5 小鍵盤外圍符號鍵行為）
            ui.horizontal(|ui| {
                ui.add_space(18.0 * scale);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(
                                "↳ 智慧型四角號碼（4×5 數字小鍵盤）外圍符號鍵行為：",
                            )
                            .size(13.0 * scale)
                            .strong(),
                        );
                        ui.add_space(4.0);
                        let mut fc_changed = false;
                        if ui
                            .radio_value(
                                &mut self.settings.keyboard.four_corner_keymap,
                                "blind_closed_loop".to_string(),
                                "小鍵盤全盲打端到端流程（預設推薦：- 詞前導／上頁、+ 字詞分隔／下頁、/ 字詞選單、1~0 快選、. 30 標點選單、* 萬用字元、Enter 整句完稿）",
                            )
                            .changed()
                        {
                            fc_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut self.settings.keyboard.four_corner_keymap,
                                "direct_punct".to_string(),
                                "小鍵盤直出標點模式（+ 確認首選、* 直出「。」、/ 直出「，」、- 直出「？」、. 觸發選字／萬用字元）",
                            )
                            .changed()
                        {
                            fc_changed = true;
                        }
                        if fc_changed {
                            self.auto_save_settings_quiet();
                        }
                    });
                });
            });

            ui.add_space(8.0);
            self.render_universal_reverse_coach_panel(ui);

            ui.add_space(10.0);
            egui::CollapsingHeader::new(
                egui::RichText::new("🔌 漢語字形拆碼分支預留槽位（歐亞全譜系樹・實作後直接掛載）")
                    .size(13.0 * scale)
                    .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "大易輸入法 (Dayi 4-Code / 3-Code)",
                    "CJK ▸ C ▸ 字根拆碼預留槽",
                    "預留 40 鍵大易字根與三碼／四碼簡碼拆解表槽位。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "五筆畫／部件結構數位檢字 (5-Stroke & Component Lookup)",
                    "CJK ▸ C ▸ 筆順數位預留槽",
                    "預留「橫、豎、撇、點、折」五基本筆順與部件拆解查詢槽位。",
                );
            });
        });
    }

    /// 開啟「跨輸入法互動查碼與拆碼教學工作台」(`docs/ruhi_interactive_workbench.html`)
    fn open_interactive_workbench(&mut self) {
        let mut candidates = vec![
            self.root_dir.join("docs").join("ruhi_interactive_workbench.html"),
            self.root_dir.join("ruhi_interactive_workbench.html"),
            self.root_dir.join("docs").join("four_corner_tutorial_anim.html"),
            self.root_dir.join("four_corner_tutorial_anim.html"),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            let app_docs = PathBuf::from(appdata).join("Ruhi").join("docs");
            candidates.push(app_docs.join("ruhi_interactive_workbench.html"));
            candidates.push(app_docs.join("four_corner_tutorial_anim.html"));
        }
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            let path_str = found.to_string_lossy().to_string();
            let _ = ruhi_settings::http_client::open_url_or_path(&path_str);
            self.shell.toasts.success(format!(
                "已於瀏覽器開啟跨輸入法互動查碼與教學工作台：{}",
                path_str
            ));
        } else {
            self.shell
                .toasts
                .error("找不到 docs/ruhi_interactive_workbench.html 互動工作台檔案。");
        }
    }

    /// 通用可擴展跨輸入法反查教練與互動拆碼工作台面板
    fn render_universal_reverse_coach_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;
        let scale = self.shell.ui_scale;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(
                        "🎯 通用跨輸入法反查教練 ＆ 互動拆碼教學工作台 (Universal Cross-IME Reverse-Lookup)",
                    )
                    .size(13.5 * scale)
                    .strong()
                    .color(palette.accent.to_egui()),
                );
                if ui
                    .button(
                        egui::RichText::new("🎬 開啟跨輸入法互動查碼與拆碼教學工作台 (推薦主力工具)")
                            .size(12.5 * scale)
                            .strong(),
                    )
                    .clicked()
                {
                    self.open_interactive_workbench();
                }
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "主力推薦：點擊上方按鈕開啟全螢幕視覺化「跨輸入法互動查碼與拆碼教學工作台（docs/ruhi_interactive_workbench.html）」，貼上任何單字或整串句子，即可依所選輸入法純淨檢視或同屏對照 7 大輸入法（龍龕形音 4 碼、智慧四角 4×5、行列 30、倉頡、許氏雙拼、標準大千、倚天 26 鍵）之拆碼步驟、相容碼思考推論與互動動畫，完全不佔用日常打字時的懸浮輸入座空間！",
                )
                .size(11.8 * scale)
                .color(palette.fg_muted.to_egui()),
            );

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("↳ 懸浮輸入座底部即時反查提示目標（選用）：")
                        .size(12.5 * scale)
                        .strong(),
                );
                let mut cur_target = if !self.settings.keyboard.reverse_coach_target.is_empty()
                    && self.settings.keyboard.reverse_coach_target != "none"
                {
                    self.settings.keyboard.reverse_coach_target.clone()
                } else if self.settings.keyboard.enable_array30_coach {
                    "array30".to_string()
                } else {
                    "none".to_string()
                };

                let mut changed = false;
                if ui
                    .radio_value(
                        &mut cur_target,
                        "none".to_string(),
                        "關閉（推薦使用互動查碼工作台）",
                    )
                    .changed()
                {
                    changed = true;
                }
                for desc in ALL_MODES
                    .iter()
                    .filter(|d| !matches!(d.id, "japanese" | "korean" | "english"))
                {
                    if ui
                        .radio_value(&mut cur_target, desc.id.to_string(), desc.display_name)
                        .changed()
                    {
                        changed = true;
                    }
                }
                if changed {
                    self.settings.keyboard.reverse_coach_target = cur_target.clone();
                    self.settings.keyboard.enable_array30_coach = cur_target == "array30";
                    self.auto_save_settings_quiet();
                }
            });
        });
    }

    /// 鍵盤樹狀分枝三：🌍 日韓(JK)與歐亞全譜系樹狀圖 (Eurasian Full-Spectrum Tree)
    fn render_keyboard_jk_eurasian_page(&mut self, ui: &mut egui::Ui) {
        self.render_keyboard_branch_switcher(ui);
        let palette = self.shell.palette;
        let scale = self.shell.ui_scale;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("🌍 歐亞語系全譜巢狀樹狀圖 (Eurasian Full-Spectrum Linguistic Tree)");
            ui.label(
                egui::RichText::new(
                    "架構說明：如喜以統一之有限狀態音韻／字素轉譯器 (FST Transducer) 與 Viterbi 語意拓撲網格為基底，預留歐亞大陸各大語系槽位；已實作之輸入法模組皆直接掛載於對應語系節點上。",
                )
                .size(12.0 * scale)
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(8.0);

            // I. 東亞漢字文化圈 (CJK Sphere)
            egui::CollapsingHeader::new(
                egui::RichText::new(
                    "🌏 I. 東亞漢字文化圈 (CJK Sphere — Sinitic, Japonic & Koreanic) ［9 項已掛載］",
                )
                .size(14.0 * scale)
                .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                // C 分支摘要與快速跳轉
                egui::Frame::NONE
                    .fill(palette.bg_surface.to_egui())
                    .stroke(egui::Stroke::new(1.0_f32, palette.border.to_egui()))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(
                        (10.0 * scale).round() as i8,
                        (8.0 * scale).round() as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "🇹🇼 漢語支 (C — Sinitic)：已掛載 7 種輸入法（音韻注音系 3 種 ＋ 形音／字形數位系 4 種）",
                                )
                                .size(13.0 * scale)
                                .strong(),
                            );
                            if ui.button("🔊 前往漢語(C)：音韻注音系 ▸").clicked() {
                                self.current_category = StudioCategory::KeyboardCjkPhonetic;
                            }
                            if ui.button("🧩 前往漢語(C)：字形與數位碼 ▸").clicked() {
                                self.current_category = StudioCategory::KeyboardCjkShape;
                            }
                        });
                    });

                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    Some("japanese"),
                    "日文羅馬字輸入（Japanese Romaji · 日）",
                    "CJK ▸ J (Japonic) ▸ 羅馬字假名漢字轉換",
                    "支援標準 Hepburn / 訓令式羅馬字拼讀、促音 (っ)・撥音 (ん)・長音即時轉換，以及常用漢字與片假名詞彙選取。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    Some("korean"),
                    "韓文羅馬拼音輸入（Korean Revised Romanization · 韓）",
                    "CJK ▸ K (Koreanic) ▸ 羅馬字諺文音節合成",
                    "專為跨語種使用者設計之文化觀光部標準韓文羅馬拼音組字引擎，自動將聲母 (Choseong)、中聲 (Jungseong)、終聲 (Jongseong) 合成為韓文諺文音節。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "日文 JIS 假名直打 ／ 韓文兩벌式標準鍵盤 (JIS Kana / Dubeolsik 2-Set)",
                    "CJK ▸ JK ▸ 實體鍵位直打預留槽",
                    "預留日文 JIS 48 假名鍵位與韓國國家標準 KS X 5002 兩벌式 (Dubeolsik) 實體鍵位直打映射槽位。",
                );
            });

            ui.add_space(8.0);

            // II. 古印度雅利安與南亞婆羅米文字圈 (Indo-Aryan & Brahmic)
            egui::CollapsingHeader::new(
                egui::RichText::new(
                    "🕉 II. 古印度雅利安與南亞婆羅米文字圈 (Indo-Aryan, Sanskrit/Pali & Brahmic Sphere)",
                )
                .size(14.0 * scale)
                .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "梵文／巴利語國際轉寫羅馬化 (Sanskrit / Pali IAST & ISO-15919)",
                    "Indo-European ▸ Indo-Aryan ▸ IAST 學術轉寫預留槽",
                    "預留長短母音 (ā/ī/ū/ṛ/ṝ/ḷ)、捲舌音 (ṭ/ḍ/ṇ/ṣ)、顎音 (ś/ñ)、隨韻 (ṃ) 與止韻 (ḥ) 組合鍵映射，供佛學、哲學與印度學文獻精確輸入。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "天城體音韻直打與連字元組字 (Devanagari Phonetic & Conjuncts)",
                    "Brahmic ▸ Devanagari ▸ 天城體預留槽",
                    "預留梵語／印地語天城體元音附標 (Matra)、去元音符 (Virama / Halant) 與複合子音連字 (Ligatures) 自動組字槽位。",
                );
            });

            ui.add_space(8.0);

            // III. 大陸東南亞聲調與婆羅米系 (Mainland Southeast Asian Sphere)
            egui::CollapsingHeader::new(
                egui::RichText::new(
                    "🌴 III. 大陸東南亞聲調與婆羅米系 (Mainland Southeast Asian: Austroasiatic & Kra-Dai)",
                )
                .size(14.0 * scale)
                .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "越南語國語字雙拼與聲調輸入 (Vietnamese Quốc ngữ — Telex / VNI)",
                    "Austroasiatic ▸ Vietic ▸ 越南語預留槽",
                    "預留越南語字母變音冠標 (ă/â/đ/ê/ô/ơ/ư) 與六聲調尾碼 (s/f/r/x/j) 即時疊加合成槽位。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "泰語音韻羅馬拼讀與 Kedmanee 映射 (Thai Phonetic & Kedmanee)",
                    "Kra-Dai ▸ Tai ▸ 泰語預留槽",
                    "預留泰語無空格連續音節切分、前引元音順序自動重排與五聲調標記組字槽位。",
                );
            });

            ui.add_space(8.0);

            // IV. 泛歐印歐語系與閃含語系圈 (Pan-European, Cyrillic, Greek & Semitic)
            egui::CollapsingHeader::new(
                egui::RichText::new(
                    "🏛 IV. 泛歐印歐語系與閃含語系圈 (Pan-European, Cyrillic, Greek & Semitic) ［1 項已掛載］",
                )
                .size(14.0 * scale)
                .strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                self.render_spectrum_method_node(
                    ui,
                    Some("english"),
                    "英數／歐文直通與歐亞智慧混打串流 (Latin Direct & Eurasian Mixed Stream · 英)",
                    "Pan-European ▸ Latin ▸ 歐文直通與歐亞混打",
                    "支援 Tab 鍵智慧聯想、模糊／拼音近似自動勘誤與詞彙補完，可與其他亞洲模式對切，亦可「僅單獨掛載此模式」作為純歐文智慧輸入法使用。",
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("🔤 前往歐亞智慧混打與 Tab 聯想設定 ▸").clicked() {
                        self.current_category = StudioCategory::MixedEnglish;
                    }
                });

                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "泛歐多國重音與變音符號死鍵組字 (Pan-European Diacritics: De/Fr/Es/It/Nordic)",
                    "Indo-European ▸ Germanic / Romance ▸ 歐文變音預留槽",
                    "預留德語 Umlaut (ä/ö/ü/ß)、法西葡語重音符 (é/è/ê/ç/ñ) 與北歐字母 (å/æ/ø) 複合鍵映射槽位。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "斯拉夫西里爾音韻映射與希臘語學術輸入 (Cyrillic Phonetic & Greek Polytonic)",
                    "Indo-European ▸ Balto-Slavic / Hellenic ▸ 西里爾與希臘預留槽",
                    "預留烏克蘭語／俄語／保加利亞語西里爾字母音韻轉寫，以及古典希臘語多調系統 (Polytonic Greek) 槽位。",
                );
                ui.add_space(6.0);
                self.render_spectrum_method_node(
                    ui,
                    None,
                    "閃含語系雙向音韻轉寫 (Afro-Asiatic / Semitic: Arabic & Hebrew)",
                    "Afro-Asiatic ▸ Semitic ▸ 雙向書寫預留槽",
                    "預留阿拉伯語／希伯來語由右至左 (RTL) 輔音骨架與母音附標音韻映射槽位。",
                );
            });
        });
    }

    /// 鍵盤樹狀分枝四：⚡ 快速切換迴圈、常用鍵與標點結算
    fn render_keyboard_hotkeys_punct_page(&mut self, ui: &mut egui::Ui) {
        self.render_keyboard_branch_switcher(ui);
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("🔄 快速切換迴圈總表與手動排序（Caps Lock 對切 ／ Ctrl+CapsLock 往後 ／ Shift+CapsLock 往前）");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "• 單按 Caps Lock：在最近使用的「最後兩個輸入模式」間快速對切（如 許⇄英、許⇄日、行⇄英）。\n• Ctrl + Caps Lock：依下方「手動排序清單」順序「往後（下一個）」循環切換。\n• Shift + Caps Lock：依下方「手動排序清單」順序「往前（上一個）」循環切換。",
                )
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(6.0);

            let mut cycle_changed = false;

            // 使用者手動排序區塊（支援 ▲ 上移 / ▼ 下移 / ⏫ 移至第一順 / ✖ 移出迴圈）
            egui::Frame::NONE
                .fill(palette.bg_surface.to_egui())
                .stroke(egui::Stroke::new(1.0_f32, palette.border.to_egui()))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("📋 目前循環切換順序（當在三種以上輸入法間以 Shift+CapsLock ／ Ctrl+CapsLock 切換時，可手動調整順序）：")
                            .strong()
                            .color(palette.accent.to_egui()),
                    );
                    ui.add_space(4.0);
                    let len = self.settings.keyboard.enabled_keyboard_modes.len();
                    let mut swap_action: Option<(usize, usize)> = None;
                    let mut move_top_action: Option<usize> = None;
                    let mut remove_action: Option<usize> = None;

                    for idx in 0..len {
                        let m_code = self.settings.keyboard.enabled_keyboard_modes[idx].clone();
                        let is_cur_layout = self.settings.keyboard.layout == m_code;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("第 {} 順：", idx + 1))
                                    .strong()
                                    .color(palette.fg_muted.to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(Self::mode_code_display_name(&m_code))
                                    .strong()
                                    .color(if is_cur_layout {
                                        palette.accent.to_egui()
                                    } else {
                                        palette.fg_bright.to_egui()
                                    }),
                            );
                            if is_cur_layout {
                                ui.label(
                                    egui::RichText::new("［當前使用中］")
                                        .color(palette.accent.to_egui()),
                                );
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_enabled(idx > 0, egui::Button::new("▲ 上移"))
                                .on_hover_text("在 Shift/Ctrl + Caps Lock 循環順序中往前移一位")
                                .clicked()
                            {
                                swap_action = Some((idx, idx - 1));
                            }
                            if ui
                                .add_enabled(idx + 1 < len, egui::Button::new("▼ 下移"))
                                .on_hover_text("在 Shift/Ctrl + Caps Lock 循環順序中往後移一位")
                                .clicked()
                            {
                                swap_action = Some((idx, idx + 1));
                            }
                            if ui
                                .add_enabled(idx > 0, egui::Button::new("⏫ 移至第一順"))
                                .on_hover_text("直接移至循環清單第一位")
                                .clicked()
                            {
                                move_top_action = Some(idx);
                            }
                            if ui
                                .add_enabled(len > 1, egui::Button::new("🎯 僅留此模式"))
                                .on_hover_text("卸載其餘所有模式，僅單獨掛載此輸入法")
                                .clicked()
                            {
                                mount_single_keyboard_mode_only(&mut self.settings, &m_code);
                                cycle_changed = true;
                            }
                            if ui
                                .add_enabled(len > 1, egui::Button::new("✖ 移出"))
                                .on_hover_text("從已掛載清單中移出（若目前正使用此模式，將自動遞補下一個已掛載模式；至少須保留 1 種模式）")
                                .clicked()
                            {
                                remove_action = Some(idx);
                            }
                        });
                    }

                    if let Some((a, b)) = swap_action {
                        self.settings.keyboard.enabled_keyboard_modes.swap(a, b);
                        cycle_changed = true;
                    } else if let Some(idx) = move_top_action {
                        let item = self.settings.keyboard.enabled_keyboard_modes.remove(idx);
                        self.settings.keyboard.enabled_keyboard_modes.insert(0, item);
                        cycle_changed = true;
                    } else if let Some(idx) = remove_action {
                        if let Some(removed_code) = self
                            .settings
                            .keyboard
                            .enabled_keyboard_modes
                            .get(idx)
                            .cloned()
                        {
                            let _ = unmount_keyboard_mode(&mut self.settings, &removed_code);
                        }
                        cycle_changed = true;
                    }
                });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("勾選納入循環切換之輸入法（勾選後將加入上方排序清單，任意模式皆可自由卸載或單獨掛載）：").strong());
            ui.add_space(4.0);

            let enabled_len = self.settings.keyboard.enabled_keyboard_modes.len();
            for desc in ALL_MODES {
                let m_code = desc.id;
                let m_label = desc.display_name;
                let mut enabled = self
                    .settings
                    .keyboard
                    .enabled_keyboard_modes
                    .iter()
                    .any(|m| m == m_code);
                let is_sole_mode = enabled && enabled_len == 1;
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!enabled || enabled_len > 1, egui::Checkbox::new(&mut enabled, m_label))
                        .on_disabled_hover_text("至少須保留 1 種已掛載模式；若欲切換為其他模式，請先勾選目標模式或點擊右側「僅單獨掛載」。")
                        .changed()
                    {
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
                            ensure_keyboard_modes_consistent(&mut self.settings);
                        } else {
                            let _ = unmount_keyboard_mode(&mut self.settings, m_code);
                        }
                        cycle_changed = true;
                    }
                    ui.add_space(8.0);
                    if ui
                        .radio_value(
                            &mut self.settings.keyboard.layout,
                            m_code.to_string(),
                            "🔘 設為當前使用輸入法",
                        )
                        .changed()
                    {
                        ensure_keyboard_modes_consistent(&mut self.settings);
                        cycle_changed = true;
                    }
                    ui.add_space(6.0);
                    if ui
                        .add_enabled(
                            !is_sole_mode,
                            egui::Button::new(if is_sole_mode {
                                "🎯 僅此模式"
                            } else {
                                "🎯 僅單獨掛載"
                            }),
                        )
                        .clicked()
                    {
                        mount_single_keyboard_mode_only(&mut self.settings, m_code);
                        cycle_changed = true;
                    }
                });
            }
            if cycle_changed {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("⚡ 通用「`」與「~」雙輔助輸入法與快打組態（一主二輔黃金三角）");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "• 單按「`」（反引號）：依下方設定啟動「自訂詞庫」（注音頭碼／縮略碼，忘記縮略碼可按 Tab 展開清單）或指定的臨時單字輸入法，上屏後自動返回原輸入法。\n• 連按兩下「`」x2：顯示「📋自訂範本」，可直接輸入頭碼或範本縮略碼（忘記縮略碼可按 Tab 展開候選清單，支援 templates.tsv 動態日期與簽名）。\n• 單按「~」（Shift + ` 波浪號）：依下方設定直接輸出全形「～」，或切換至指定「持續切換輔助輸入法」，再次按下「~」即刻回到原輸入法。\n• 連按兩下「~」x2：立即開啟「🔢 數字快打模式」（輸入數字即時轉中文大小寫與財務大寫金額，支援 20261112 或 2026/11/12 即時轉日期格式）。\n• 反斜線轉義直出：隨時輸入「\\`」、「\\~」或「\\\\」即可強制直接輸出原符號（含反引號／波浪號／反斜線），不觸發輔助切換與範本。\n💡 配合 Caps Lock 原有對切功能，設定得當時可在「一種主要輸入法」與「另外兩種輔助輸入法」間瞬時切換！",
                )
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(8.0);

            let mut aux_options: Vec<(String, String)> = vec![(
                "user_abbrev".to_string(),
                "🔖 自訂詞庫：頭碼／自訂縮略碼輸入（預設推薦）".to_string(),
            )];
            aux_options.extend(mode_picker_options(Some("english")));
            let mut aux_changed = false;

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("單按「`」臨時單一字／詞輸入法：")
                        .strong()
                        .color(palette.accent.to_egui()),
                );
                egui::ComboBox::from_id_salt("ruhi_backtick_inline_mode_combo")
                    .selected_text(Self::mode_code_display_name(
                        &self.settings.keyboard.backtick_inline_mode,
                    ))
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for (code, label) in &aux_options {
                            if ui
                                .selectable_value(
                                    &mut self.settings.keyboard.backtick_inline_mode,
                                    code.clone(),
                                    label.as_str(),
                                )
                                .changed()
                            {
                                aux_changed = true;
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new("（預設單按 ` 進行自訂詞庫縮略與頭碼輸入，連按 `` 開啟文字範本；完成後即刻返回原輸入法）")
                        .color(palette.fg_muted.to_egui()),
                );
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("單按「~」（Shift + `）按鍵行為：")
                        .strong()
                        .color(palette.accent.to_egui()),
                );
                egui::ComboBox::from_id_salt("ruhi_tilde_toggle_mode_combo")
                    .selected_text(Self::mode_code_display_name(
                        &self.settings.keyboard.tilde_toggle_mode,
                    ))
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut self.settings.keyboard.tilde_toggle_mode,
                                "punct".to_string(),
                                "〰 直出全形波浪號「～」（預設推薦）",
                            )
                            .changed()
                        {
                            aux_changed = true;
                        }
                        for (code, label) in &aux_options {
                            if ui
                                .selectable_value(
                                    &mut self.settings.keyboard.tilde_toggle_mode,
                                    code.clone(),
                                    label.as_str(),
                                )
                                .changed()
                            {
                                aux_changed = true;
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new("（預設直接輸出中文全形「～」；若選輔助輸入法則按 `~` 雙向對切，且隨時可用反斜線轉義 `\\``、`\\~` 或 `\\\\` 直出原符號／全形「～」）")
                        .color(palette.fg_muted.to_egui()),
                );
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("按「Shift + ↓」跨輸入法組字／候選字詞查詢：")
                        .strong()
                        .color(palette.accent.to_egui()),
                );
                egui::ComboBox::from_id_salt("ruhi_shift_down_mode_combo")
                    .selected_text(Self::mode_code_display_name(
                        &self.settings.keyboard.shift_down_mode,
                    ))
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for (code, label) in mode_picker_options(Some("english")) {
                            if ui
                                .selectable_value(
                                    &mut self.settings.keyboard.shift_down_mode,
                                    code,
                                    label,
                                )
                                .changed()
                            {
                                aux_changed = true;
                            }
                        }
                    });
                ui.label(
                    egui::RichText::new("（預設查詢龍龕形音組字詞；若當前已在該模式，按 Shift+↓ 將智慧彈出模式詢問選單，且候選窗右側提供快速切換鈕）")
                        .color(palette.fg_muted.to_egui()),
                );
            });

            if aux_changed {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("⌨ 空白鍵、Tab／Shift+Tab 與 Caps Lock 按鍵行為");
            ui.add_space(6.0);

            let mut key_changed = false;
            ui.label(egui::RichText::new("空白鍵行為：").strong());
            if ui
                .radio_value(
                    &mut self.settings.keyboard.space_behavior,
                    "commit_first".to_string(),
                    "出第一候選字（如喜智慧預設，流暢選詞；日文模式下輸出半形空白以符合國際習慣）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.keyboard.space_behavior,
                    "space_only".to_string(),
                    "僅輸出半形空格",
                )
                .changed()
            {
                key_changed = true;
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Tab 與 Shift + Tab 按鍵行為：").strong());
            if ui
                .radio_value(
                    &mut self.settings.keyboard.tab_key_mode,
                    "associative".to_string(),
                    "現代智慧聯想模式（預設推薦：單按 Tab 啟動後半截字詞聯想選單，Shift + Tab 手動區隔詞界）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.keyboard.tab_key_mode,
                    "boundary_cut".to_string(),
                    "自然輸入法傳統相容模式（單按 Tab 手動區隔詞界，Shift + Tab 啟動後半截字詞聯想選單）",
                )
                .changed()
            {
                key_changed = true;
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Caps Lock 按鍵切換行為：").strong());
            if ui
                .radio_value(
                    &mut self.settings.keyboard.capslock_switch,
                    "english_direct".to_string(),
                    "英數/歐文直通模式(英)（單按 Caps Lock 固定在「當前亞洲輸入法 ⇄ 英數/歐文直通模式(英)」間對切）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.keyboard.capslock_switch,
                    "chinese_english".to_string(),
                    "最後兩種輸入模式快速切換（單按 Caps Lock 在最近使用的最後兩個模式如 許⇄日、許⇄英、日⇄英 間對切）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.keyboard.capslock_switch,
                    "capital_lock".to_string(),
                    "大寫鎖定相容模式（單按 Caps Lock 切換最後兩種輸入模式，英數模式下支援大寫鎖定）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .radio_value(
                    &mut self.settings.keyboard.capslock_switch,
                    "none".to_string(),
                    "不接管（還原系統原生大寫鎖定 Caps Lock／不切換輸入法，推薦單獨掛載一種鍵盤模式時使用）",
                )
                .changed()
            {
                key_changed = true;
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("半形 (半) ⇄ 全形 (全) 切換行為：").strong());
            if ui
                .checkbox(
                    &mut self.settings.keyboard.shift_space_toggle_fullwidth,
                    "啟用 Shift + Space 快速切換「半形 (半) ⇄ 全形 (全)」模式（切換時同步更新輸入座【 半 許 】／【 全 許 】指示器）",
                )
                .changed()
            {
                key_changed = true;
            }
            if ui
                .checkbox(
                    &mut self.settings.keyboard.fullwidth_mode,
                    "目前啟用「全形模式（全）」（英文字母、數字與空白一律以全形 ＳＩ、１２３ 輸出）",
                )
                .changed()
            {
                key_changed = true;
            }
            if key_changed {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("📝 標點符號結算行為與半形／全形標點寬度策略");
            ui.add_space(6.0);

            let mut punct_changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "A".to_string(),
                "經典即時送出（微軟新注音風格・預設推薦）",
                "鍵入任何標點（「，」、「。」、「、」等），整句連同標點立刻結算送出上屏。系統負擔最小、相容性最高、各類 APP 零排版延遲。",
                &palette,
            ) {
                punct_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "B".to_string(),
                "智慧句末送出（自然長句折衷風格）",
                "逗號「，」、頓號「、」保留在輸入座維持長句；打到句末標點（「。」、「？」、「！」等）自動結算送出。兼顧長句打字連續性與段落自然閉合。",
                &palette,
            ) {
                punct_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.punctuation_commit_mode,
                "C".to_string(),
                "全手動長句模式（純 Enter 送出）",
                "任何標點（包括句號）完全保留在輸入座內，絕不自動送出，一律由使用者確認無誤後自行按下 Enter 鍵統一結算。適合全文通盤檢視校對。",
                &palette,
            ) {
                punct_changed = true;
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("半形注音（【 半 許 】／【 半 注 】）下之標點符號寬度策略：").strong());
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.halfwidth_punct_mode,
                "fullwidth".to_string(),
                "半形注音預設輸出中文全形標點（「，」、「。」・傳統中文輸入法預設）",
                "處於「半形」注音模式時，英數維持半形（SI, 123），標點符號鍵預設輸出中文全形標點（「，」、「。」、「；」）。",
                &palette,
            ) {
                punct_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.halfwidth_punct_mode,
                "consistent".to_string(),
                "半形與全形指示器嚴格一致（「半 許」輸出半形 , . ；「全 許」輸出全形 ， 。）",
                "恢復【 半 許 】字面一致性：在半形模式下標點鍵輸出半形標點（, .）；需要全形標點時按 Shift + Space 切換至【 全 許 】即可輸出全形標點（， 。）。",
                &palette,
            ) {
                punct_changed = true;
            }

            if punct_changed {
                self.auto_save_settings_quiet();
            }
        });
    }

    fn render_mixed_english_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;
        let scale = self.shell.ui_scale;
        let mut changed = false;

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("歐亞智慧混打核心機制（Eurasian Smart Mixed Input）");
            ui.add_space(6.0);
            if two_tier_checkbox(
                ui,
                &mut self.settings.keyboard.enable_mixed_english,
                "啟用歐亞智慧混打總開關",
                "於下方勾選之「鍵位正交／低重碼率」輸入模式中，免切換輸入法即可自然混打歐文單字、拉丁學術轉寫與程式碼；按 Enter 鍵可直接以歐文上屏。",
                &palette,
            ) {
                changed = true;
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("適用之正交／低重碼率混打模式選擇");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "以下四種輸入法具備「物理鍵區分離」、「41 鍵聲調正交」或「五碼長碼界限」之低重碼特性，能精確區辨漢字編碼與歐文詞彙：",
                )
                .size(12.2 * scale)
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(6.0);

            let orthogonal_modes: [(&str, &str, &str); 4] = [
                (
                    "standard",
                    "標準大千注音（41 鍵音位與聲調正交 · 推薦）",
                    "大千注音之聲調鍵（空白鍵／6／3／4／7）獨立於字母區外；凡韻母或介音後未按聲調即連打字母（如 this、apple、hello、rust），或連續純聲母不構成漢語詞彙時，即 100% 確定性轉為歐文。",
                ),
                (
                    "cangjie",
                    "倉頡輸入法（五碼字根上限與大寫正交 · 推薦）",
                    "標準倉頡重碼率極低且最長為 5 碼；凡輸入大寫字母開頭（如 GitHub、Rust）、長度超過 5 碼之單字（如 international）、無對應漢字之字母組合，或組字中按 Enter 鍵，皆直接輸出歐文。",
                ),
                (
                    "four_corner",
                    "智慧型四角號碼（右小鍵盤 × 左主鍵盤物理正交 · 推薦）",
                    "右側 4×5 九宮格小鍵盤（0~9、.、+、-、*、/）專責四角號碼與標點檢索，左側主鍵盤（A~Z、橫排數字）100% 實體分離直出歐文與算式，達成零碰撞混打。",
                ),
                (
                    "array30",
                    "行列 30 輸入法（五碼上限與 Enter 歐文直通 · 推薦）",
                    "行列 30 字根重碼率低且最長為 5 碼；輸入大寫字母、超過 5 碼之歐文單字自動轉為歐文，1~5 碼組字期間按 Enter 鍵亦可直接輸出原始英文字串。",
                ),
            ];

            ui.add_enabled_ui(self.settings.keyboard.enable_mixed_english, |ui| {
                for (idx, (mode_id, title, desc)) in orthogonal_modes.iter().enumerate() {
                    let mut is_checked = self
                        .settings
                        .keyboard
                        .mixed_typing_modes
                        .iter()
                        .any(|m| m.eq_ignore_ascii_case(mode_id));
                    if two_tier_checkbox(ui, &mut is_checked, title, desc, &palette) {
                        if is_checked {
                            if !self
                                .settings
                                .keyboard
                                .mixed_typing_modes
                                .iter()
                                .any(|m| m.eq_ignore_ascii_case(mode_id))
                            {
                                self.settings
                                    .keyboard
                                    .mixed_typing_modes
                                    .push((*mode_id).to_string());
                            }
                        } else {
                            self.settings
                                .keyboard
                                .mixed_typing_modes
                                .retain(|m| !m.eq_ignore_ascii_case(mode_id));
                        }
                        changed = true;
                    }
                    if idx + 1 < orthogonal_modes.len() {
                        ui.add_space(4.0);
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "💡 高重碼率 26 鍵模式之確定性保護（不強行啟用自動混打猜測）：\n\
                     • 許氏雙拼注音 (`hsu`)、倚天 26 鍵 (`et26`)、龍龕形音 (`longkan`)、日韓羅馬字 (`japanese` / `korean`) 將全部音節與部件高度壓縮映射於 a~z 26 鍵，與歐文單字（如 is、as、to、do、he、me）高度重疊。\n\
                     • 為徹底杜絕整句打字時發生「非預期覆寫」，上述高重碼 26 鍵模式不強行介入自動混打猜測；請直接使用單鍵 `Caps Lock`（或 `` ` `` / `~` 鍵）進行零延遲之確定性英數切換。",
                )
                .size(11.8 * scale)
                .color(palette.fg_muted.to_egui()),
            );
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("混打仲裁策略與門檻（適用於大千注音等音韻混打）");
            ui.add_space(6.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "contextual_stream".to_string(),
                "語境串流模式（推薦）",
                "首詞確認為歐文時開啟串流，放行後續短詞；中文組字享有 100% 語境防護，杜絕非預期覆寫。",
                &palette,
            ) {
                changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "conservative".to_string(),
                "保守模式",
                "僅在大寫字母、駝峰命名 (CamelCase)、音位配列違規或特殊識別符時自動轉為歐文。",
                &palette,
            ) {
                changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.keyboard.mixed_english_mode,
                "min_length_only".to_string(),
                "長詞門檻模式",
                "連續未匹配字元長度達到門檻長度時，方轉為歐文串流。",
                &palette,
            ) {
                changed = true;
            }

            ui.add_space(8.0);
            if Self::render_slider_with_numeric_input(
                ui,
                "mixed_english_min_len",
                "長詞門檻字數（門檻模式下生效）：",
                &mut self.settings.keyboard.mixed_english_min_len,
                3..=6,
                "個字母",
                scale,
                &mut self.slider_text_buffers,
            ) {
                changed = true;
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("泛歐多國語文與學術羅馬拼音掛載（Western Languages & IAST）");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "依實際需要複選歐文詞庫與學術轉寫語種（設定即時寫入 western_languages；預設僅啟用 English (en)，梵語羅馬拼音 sa_am 預設關閉以免影響一般英數選詞）。",
                )
                .size(12.2 * scale)
                .color(palette.fg_muted.to_egui()),
            );
            ui.add_space(6.0);

            let western_options: [(&str, &str, &str); 5] = [
                (
                    "en",
                    "English (en)［預設啟用］",
                    "標準英文核心詞彙、專有名詞與軟體技術縮寫；作為歐文直通與混打之預設基底語系。",
                ),
                (
                    "fr",
                    "Français (fr) — 法語",
                    "啟用法語常用詞彙、重音與變音符號（é/è/ê/à/ç/œ）聯想補完與勘誤支援。",
                ),
                (
                    "de",
                    "Deutsch (de) — 德語",
                    "啟用德語詞彙與變音字母（ä/ö/ü/ß）聯想補完與複合詞識別支援。",
                ),
                (
                    "es",
                    "Español (es) — 西班牙語",
                    "啟用西班牙語詞彙與 ñ／重音母音（á/é/í/ó/ú/ü）輸入聯想支援。",
                ),
                (
                    "sa_am",
                    "Ánanda Márga Saṁskṛta 梵語羅馬拼音 (sa_am)［預設關閉］",
                    "提供 IAST 國際梵語轉寫標音詞庫（ā/ī/ū/ṛ/ṁ/ṅ/ñ/ṭ/ḍ/ṇ/ś/ṣ）與學術專詞支援。",
                ),
            ];

            for (idx, (lang_code, title, desc)) in western_options.iter().enumerate() {
                let mut is_checked = self
                    .settings
                    .keyboard
                    .western_languages
                    .iter()
                    .any(|w| w.eq_ignore_ascii_case(lang_code));
                if two_tier_checkbox(ui, &mut is_checked, title, desc, &palette) {
                    if is_checked {
                        if !self
                            .settings
                            .keyboard
                            .western_languages
                            .iter()
                            .any(|w| w.eq_ignore_ascii_case(lang_code))
                        {
                            self.settings
                                .keyboard
                                .western_languages
                                .push((*lang_code).to_string());
                        }
                    } else {
                        self.settings
                            .keyboard
                            .western_languages
                            .retain(|w| !w.eq_ignore_ascii_case(lang_code));
                        if self.settings.keyboard.western_languages.is_empty() {
                            self.settings.keyboard.western_languages.push("en".to_string());
                            self.shell
                                .toasts
                                .info("至少保留 English (en) 作為歐文基礎語系");
                        }
                    }
                    self.settings.keyboard.western_languages =
                        ruhi_settings::config_sync::normalize_western_languages(
                            &self.settings.keyboard.western_languages,
                        );
                    changed = true;
                }
                if idx + 1 < western_options.len() {
                    ui.add_space(4.0);
                }
            }
        });

        if changed {
            self.auto_save_settings_quiet();
        }
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
            ui.heading("文言單字語體引擎 (Classical Register Engine)");
            ui.add_space(6.0);

            let mut classical_changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.grammar.classical_mode,
                "auto".to_string(),
                "自動偵測古典文言語境（預設・現代白話零干擾）",
                "具備輕聲與現代指標詞嚴格否定閘門，僅於句中累積多重文言虛詞或對偶特徵時自動啟用古典單字語境。",
                &palette,
            ) {
                classical_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.grammar.classical_mode,
                "off".to_string(),
                "永遠停用（純現代白話解碼）",
                "關閉文言單字語境權重與對偶規則；若遇偶發古今同音誤觸，可隨時切換此選項恢復純白話計分。",
                &palette,
            ) {
                classical_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.grammar.classical_mode,
                "explicit".to_string(),
                "手動強制開啟文言模式（古籍／詩詞／散文長句適用）",
                "無視句末虛詞與句長門檻，全程以最高強度啟用文言虛詞句法槽位與古典單字先驗加權。",
                &palette,
            ) {
                classical_changed = true;
            }

            if classical_changed {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("輕量神經語言模型重排序 (RLNM0001 Sidecar・選配)");
            ui.add_space(6.0);

            let has_model = [
                self.root_dir.join("ruhi_lexicon").join("data").join("rlnm_zh_tw.bin"),
                self.root_dir.join("data").join("rlnm_zh_tw.bin"),
                self.root_dir.join("dist").join("data").join("rlnm_zh_tw.bin"),
            ]
            .into_iter()
            .find(|p| p.is_file());
            let has_exe = [
                self.root_dir.join("ruhi_reranker.exe"),
                self.root_dir.join("dist").join("ruhi_reranker.exe"),
                self.root_dir
                    .join("ruhi-core")
                    .join("target")
                    .join("release")
                    .join("ruhi_reranker.exe"),
            ]
            .into_iter()
            .any(|p| p.is_file());

            let status_text = match (has_model, has_exe) {
                (Some(mp), true) => {
                    let mb = std::fs::metadata(&mp)
                        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                        .unwrap_or(0.0);
                    format!("狀態：已偵測 RLNM0001 模型 ({:.2} MB) 與 ruhi_reranker.exe，開啟後自動無視窗常駐。", mb)
                }
                (Some(_), false) => "狀態：已偵測模型檔，尚未編譯或部署 ruhi_reranker.exe（開啟時會自動安全退回離線引擎）。".to_string(),
                (None, _) => "狀態：尚未安裝 rlnm_zh_tw.bin 模型檔（可先停用或待下載模型後啟用）。".to_string(),
            };
            ui.label(egui::RichText::new(status_text).color(palette.fg_muted.to_egui()));
            ui.add_space(4.0);

            if two_tier_checkbox(
                ui,
                &mut self.settings.grammar.enable_neural_reranker,
                "啟用神經語言模型重排序（Named Pipe 選配）",
                "針對 4 字以上多候選同音句子以 RLNM0001 進行重排，逾時立即無縫回退原離線解碼結果。",
                &palette,
            ) {
                self.auto_save_settings_quiet();
            }

            if self.settings.grammar.enable_neural_reranker {
                ui.add_space(6.0);
                let mut params_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label("逾時上限 (ms)：");
                    let mut tmo = self.settings.grammar.neural_reranker_timeout_ms as i32;
                    if ui.add(egui::Slider::new(&mut tmo, 10..=50)).changed() {
                        self.settings.grammar.neural_reranker_timeout_ms = tmo as u16;
                        params_changed = true;
                    }
                    ui.add_space(12.0);
                    ui.label("融合權重 λ：");
                    let mut lam = self.settings.grammar.neural_reranker_lambda as f32;
                    if ui
                        .add(egui::Slider::new(&mut lam, 0.10..=1.20).fixed_decimals(2))
                        .changed()
                    {
                        self.settings.grammar.neural_reranker_lambda = lam as f64;
                        params_changed = true;
                    }
                });
                if params_changed {
                    self.auto_save_settings_quiet();
                }
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("詞庫主權與熱重載");
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
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("🛠️ 語意調校工作室三層分級權限模式 (Three-Tier Studio Gating)");
            ui.add_space(6.0);

            let mut tier_changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.grammar.studio_access_tier,
                "pure".to_string(),
                "Lv.1 一般使用者純享模式（隱藏進階調校面板）",
                "側邊欄隱藏「語意調校工作室」，打字過程遭遇同音選字僅記錄匿名對照句回報，零設定負擔、絕不誤傷本機詞庫。",
                &palette,
            ) {
                tier_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.grammar.studio_access_tier,
                "expert_sandbox".to_string(),
                "Lv.2 進階專家沙盒模式（強制通過 Viterbi 三重零迴歸沙盒驗證 + 一鍵還原）",
                "開啟「🛠️ 語意調校工作室」，所有提煉規則均須先在記憶體內 Viterbi 沙盒通過「目標句修復 + 同音對抗反例 + 黃金基準零迴歸」三重驗證方可寫入個人層，並提供一鍵還原按鈕。",
                &palette,
            ) {
                tier_changed = true;
            }
            ui.add_space(4.0);

            if two_tier_radio(
                ui,
                &mut self.settings.grammar.studio_access_tier,
                "maintainer".to_string(),
                "Lv.3 核心總工程師模式（開發工作區預設・完整開放全域構式陣列與黃金基準入庫）",
                "完整開放全域 patterns_user.jsonl 多維槽位陣列編輯、黃金基準集入庫與 Tailcat 規則包發布。",
                &palette,
            ) {
                tier_changed = true;
            }

            if tier_changed {
                self.auto_save_settings_quiet();
            }
        });

    }

    /// 偏好設定中心內嵌之「獨立 AI 語意調校工作室」啟動面板（將複雜開發者工作室解耦為獨立桌面視窗與獨立 Web 介面）
    fn render_ai_privacy_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;

        ui.group(|ui| {
            ui.heading("啟發式個資過濾閘門 (Heuristic PII Filter)");
            ui.add_space(6.0);
            if two_tier_checkbox(
                ui,
                &mut self.settings.ai_and_privacy.zero_pii_gateway,
                "啟用啟發式個資過濾閘門",
                "自動攔截含電子郵件、網址、電話、連續數字、身分證字號與門牌地址之句子；在 Win32 與 TSF InputScope 密碼欄位中全面直通且絕不記錄任何修訂。",
                &palette,
            ) {
                self.auto_save_settings_quiet();
            }
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("☁️ 匿名改字紀錄與雲端語意進化回饋（預設關閉，隨時可更改）");
            ui.add_space(6.0);

            if two_tier_checkbox(
                ui,
                &mut self.settings.ai_and_privacy.telemetry_consent,
                "同意在手動改字時記錄匿名對照句，並定期回傳至官方雲端資料表供語意調校",
                "打字手動改字時僅將「原始錯誤句 vs. 手動修正後正確句」寫入本機紀錄檔（絕不在本地盲目修改權重），並經啟發式個資過濾後依下方設定週期回傳。",
                &palette,
            ) {
                self.auto_save_settings_quiet();
            }

            ui.add_space(6.0);
            ui.label(egui::RichText::new("回傳週期設定（隨時可切換）：").strong());
            let mut schedule_changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.upload_schedule,
                "weekly".to_string(),
                "每週自動回傳（預設推薦）",
                "每滿 7 天於開啟設定中心或執行背景同步時，自動將累積之匿名改字雙句回傳至雲端試算表並清空本機暫存。",
                &palette,
            ) {
                schedule_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.upload_schedule,
                "daily".to_string(),
                "每日自動回傳",
                "每滿 24 小時自動將累積之匿名改字雙句回傳至雲端試算表。",
                &palette,
            ) {
                schedule_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.upload_schedule,
                "manual".to_string(),
                "僅手動點擊回傳",
                "平時僅記錄於本機，唯有當您點擊下方「立即回傳至雲端資料表」按鈕時方才傳送。",
                &palette,
            ) {
                schedule_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.upload_schedule,
                "disabled".to_string(),
                "完全關閉回傳",
                "停止任何雲端回傳動作。",
                &palette,
            ) {
                schedule_changed = true;
            }
            if schedule_changed {
                self.auto_save_settings_quiet();
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("雲端試算表 Webhook 網址：");
                let field_w = (420.0 * self.shell.ui_scale)
                    .min((ui.available_width() - 24.0 * self.shell.ui_scale).max(160.0));
                if ui
                    .add(
                        egui::TextEdit::singleline(
                            &mut self.settings.ai_and_privacy.cloud_sheet_webhook_url,
                        )
                        .desired_width(field_w),
                    )
                    .changed()
                {
                    self.auto_save_settings_quiet();
                }
                if ui.button("↺ 還原官方網址").clicked() {
                    self.settings.ai_and_privacy.cloud_sheet_webhook_url =
                        ruhi_settings::config_sync::resolve_cloud_sheet_webhook_url("");
                    self.auto_save_settings_quiet();
                }
            });

            let pending_revs = self.sync_client.load_local_pending_revisions();
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!(
                    "📊 目前本機待回傳改字紀錄：{} 筆（儲存於 data/pending_revisions.json 與 user_revision_pairs.txt）",
                    pending_revs.len()
                ))
                .color(palette.fg_muted.to_egui()),
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("☁️ 立即回傳至雲端資料表").clicked() {
                    let res = self.sync_client.upload_revisions_to_cloud_sheet(
                        &self.settings.ai_and_privacy.cloud_sheet_webhook_url,
                        self.settings.ai_and_privacy.zero_pii_gateway,
                    );
                    let msg = res
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("已執行雲端回傳");
                    if res.get("status").and_then(Value::as_str) == Some("ok") {
                        if let Some(ts) = res.get("timestamp").and_then(Value::as_u64) {
                            self.settings.ai_and_privacy.last_upload_timestamp = ts;
                            self.auto_save_settings_quiet();
                        }
                        self.shell.toasts.success(msg.to_string());
                    } else {
                        self.shell.toasts.info(msg.to_string());
                    }
                }
                if ui.button("📄 檢視本機改字紀錄檔").clicked() {
                    let pairs_txt = self.data_dir.join("user_revision_pairs.txt");
                    let target = if pairs_txt.is_file() {
                        pairs_txt
                    } else {
                        self.data_dir.join("pending_revisions.json")
                    };
                    #[cfg(target_os = "windows")]
                    let _ = Command::new("notepad.exe").arg(&target).spawn();
                }
                if ui.button("🗑️ 清空本機待回傳紀錄").clicked() {
                    let cleared = self.sync_client.clear_local_pending_revisions();
                    self.shell
                        .toasts
                        .info(format!("已清空本機 {} 筆待回傳改字紀錄。", cleared));
                }
            });
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("📡 Tailcat 官方詞庫熱同步與版本更新（含念舊相容模式）");
            ui.add_space(6.0);

            if two_tier_checkbox(
                ui,
                &mut self.settings.ai_and_privacy.tailcat_auto_sync,
                "啟用 Tailcat 官方語意規則與詞庫背景自動熱同步 (GitHub CDN ETag)",
                "依設定週期透過 HTTP ETag / 304 Not Modified 零流量檢查官方最新詞庫、破音字黑名單與二階語意規則，通過本機三層沙盒守門員後免重裝即時熱載入。",
                &palette,
            ) {
                self.auto_save_settings_quiet();
            }

            ui.add_space(6.0);
            ui.label(egui::RichText::new("主程式版本升級與念舊相容策略：").strong());
            let mut mode_changed = false;
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.tailcat_update_mode,
                "notify_confirm".to_string(),
                "🔔 詞庫自動熱更新 ＋ 核心新版通知（經我點擊同意後自動更新）【預設推薦】",
                "平時自動熱更新詞庫與語意規則；當發現新版主程式時彈出通知，經您點擊同意後自動下載安裝包並完成無縫升級。",
                &palette,
            ) {
                mode_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.tailcat_update_mode,
                "auto_install".to_string(),
                "⚡ 全自動更新（預先授權同意：詞庫熱更新與新版主程式均自動下載安裝）",
                "發現新詞庫或新版主程式時均自動完成下載與升級，永遠維持最新語意核心。",
                &palette,
            ) {
                mode_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.tailcat_update_mode,
                "lexicon_only".to_string(),
                "🏛️ 念舊模式（僅更新相容詞庫與語意規則，永不升級主程式）",
                "專為習慣現有主程式版本的使用者設計：照常享受所有向下相容的新詞彙與基礎語意修正；若未來出現需高版本主程式支援的新語法格式，將自動優雅略過，絕不報錯或崩潰。",
                &palette,
            ) {
                mode_changed = true;
            }
            if two_tier_radio(
                ui,
                &mut self.settings.ai_and_privacy.tailcat_update_mode,
                "manual".to_string(),
                "🖐️ 僅手動檢查更新",
                "僅在您點擊下方「立即從官方 Tailcat 頻道同步」按鈕時連線檢查。",
                &palette,
            ) {
                mode_changed = true;
            }
            if mode_changed {
                self.auto_save_settings_quiet();
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("官方 Tailcat 清單網址：");
                let field_w = (420.0 * self.shell.ui_scale)
                    .min((ui.available_width() - 24.0 * self.shell.ui_scale).max(160.0));
                if ui
                    .add(
                        egui::TextEdit::singleline(
                            &mut self.settings.ai_and_privacy.tailcat_feed_url,
                        )
                        .desired_width(field_w),
                    )
                    .changed()
                {
                    self.auto_save_settings_quiet();
                }
                if ui.button("↺ 還原官方網址").clicked() {
                    self.settings.ai_and_privacy.tailcat_feed_url =
                        ruhi_settings::config_sync::DEFAULT_TAILCAT_FEED_URL.to_string();
                    self.auto_save_settings_quiet();
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button("🔄 立即從官方 Tailcat 頻道同步最新詞庫與檢查更新")
                    .clicked()
                {
                    let feed_url = self.settings.ai_and_privacy.tailcat_feed_url.clone();
                    let update_mode = self.settings.ai_and_privacy.tailcat_update_mode.clone();
                    let res = self.sync_client.pull_community_rules_with_etag(&feed_url, true);
                    let msg = res
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Tailcat 同步檢查完成");
                    if res.get("status").and_then(Value::as_str) == Some("ok")
                        || res.get("status").and_then(Value::as_str) == Some("not_modified")
                    {
                        let now_ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        self.settings.ai_and_privacy.last_tailcat_sync_timestamp = now_ts;
                        self.auto_save_settings_quiet();
                        self.shell.toasts.success(msg.to_string());
                        let _ = self.sync_client.handle_tailcat_version_upgrade(&res, &update_mode);
                    } else {
                        self.shell.toasts.info(msg.to_string());
                    }
                }
            });
        });
    }

    fn decode_studio_logo_premul_rgba(is_dark: bool) -> Option<(usize, usize, Vec<u8>)> {
        const RLE3: &[u8] = include_bytes!("../../assets/studio_logo_640x100.rle3");
        if RLE3.len() < 10 || &RLE3[0..4] != b"RLGO" {
            return None;
        }
        let w = u16::from_le_bytes([RLE3[4], RLE3[5]]) as usize;
        let h = u16::from_le_bytes([RLE3[6], RLE3[7]]) as usize;
        let layers_cnt = RLE3[8] as usize;
        if w == 0 || h == 0 || layers_cnt != 3 {
            return None;
        }
        let total = w.checked_mul(h)?;
        let mut cursor = 10usize;
        let mut layers = Vec::with_capacity(3);
        for _ in 0..3 {
            let mut alphas = Vec::with_capacity(total);
            while alphas.len() < total {
                if cursor + 1 >= RLE3.len() {
                    return None;
                }
                let run = RLE3[cursor] as usize;
                let a = RLE3[cursor + 1];
                cursor += 2;
                if run == 0 || alphas.len() + run > total {
                    return None;
                }
                alphas.resize(alphas.len() + run, a);
            }
            layers.push(alphas);
        }

        let ink = if is_dark {
            [0xE8u32, 0xEE, 0xF8]
        } else {
            [0x2Fu32, 0x31, 0x8B]
        };
        let palette = [[0x00u32, 0xA2, 0xE9], [0xE4, 0x00, 0x82], ink];
        let mut out = vec![0u8; total * 4];
        for (layer_idx, rgb) in palette.iter().enumerate() {
            let alphas = &layers[layer_idx];
            for i in 0..total {
                let sa = alphas[i] as u32;
                if sa == 0 {
                    continue;
                }
                let off = i * 4;
                let sr = (rgb[0] * sa + 127) / 255;
                let sg = (rgb[1] * sa + 127) / 255;
                let sb = (rgb[2] * sa + 127) / 255;
                let inv = 255 - sa;
                out[off] = (sr + (out[off] as u32 * inv + 127) / 255).min(255) as u8;
                out[off + 1] = (sg + (out[off + 1] as u32 * inv + 127) / 255).min(255) as u8;
                out[off + 2] = (sb + (out[off + 2] as u32 * inv + 127) / 255).min(255) as u8;
                out[off + 3] = (sa + (out[off + 3] as u32 * inv + 127) / 255).min(255) as u8;
            }
        }
        Some((w, h, out))
    }

    fn ensure_studio_logo_texture(
        &mut self,
        ctx: &egui::Context,
        is_dark: bool,
    ) -> Option<egui::TextureId> {
        let slot = if is_dark {
            &mut self.studio_logo_dark_tex
        } else {
            &mut self.studio_logo_light_tex
        };
        if let Some(ref tex) = slot {
            return Some(tex.id());
        }
        let (w, h, rgba) = Self::decode_studio_logo_premul_rgba(is_dark)?;
        let img = egui::ColorImage::from_rgba_premultiplied([w, h], &rgba);
        let tex_name = if is_dark {
            "ruhi_studio_logo_dark"
        } else {
            "ruhi_studio_logo_light"
        };
        let handle = ctx.load_texture(tex_name, img, egui::TextureOptions::LINEAR);
        let id = handle.id();
        *slot = Some(handle);
        Some(id)
    }

    fn render_about_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.shell.palette;
        let studio_tex_id = self.ensure_studio_logo_texture(ui.ctx(), palette.is_dark);

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
            ui.label("• 梵語語源：字根 Ruh / Rūhi (रूहि) 意為「萌芽、生長、向上攀升」，象徵詞庫隨日常書寫與語境持續精進成長。");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("• 出品團隊 (Studio)：");
                if let Some(tex_id) = studio_tex_id {
                    let h = 24.0 * self.shell.ui_scale;
                    let w = h * 6.4;
                    ui.add(egui::Image::from_texture((tex_id, egui::vec2(w, h))))
                        .on_hover_text("A Solid GUI Studio (Yuktesha Zer)");
                }
                ui.label(
                    egui::RichText::new("(Yuktesha Zer)")
                        .color(palette.fg_muted.to_egui()),
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("• 官方社群：「如喜通用智慧型輸入法」LINE 社群");
                if ui.button("💬 點擊開啟 LINE 社群邀請連結").clicked() {
                    let url = "https://line.me/ti/g2/iyLVoHye-Ho49K9SzQBaftTgFzej7stw9S1jSw?utm_source=invitation&utm_medium=link_copy&utm_campaign=default";
                    let _ = ruhi_settings::http_client::open_url_or_path(url);
                }
            });
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("通用智慧型輸入法全語言藍圖（Universal Smart IME Roadmap）");
            ui.add_space(6.0);
            ui.label("1. 繁體中文注音體系：許氏鍵盤（首發）、標準大千、倚天 26 鍵、漢語拼音");
            ui.label("2. 繁體中文形音、形碼與數位體系：龍龕形音（整合創新 4 碼／356,536 筆 Unihan 碼表）、智慧型四角號碼（4×5 小鍵盤）、行列 30、倉頡（三代／五代）、大易");
            ui.label("3. 亞洲多語系輸入：日語假名/漢字、韓語諺文、泰語");
            ui.label("4. 專業學術標音符號：梵文天城體羅馬化（IAST）、台羅/白話字拼音");
            ui.label("5. 歐文多語言擴展：德文、法文、西文等特殊重音符號智慧組字");
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.heading("系統狀態與全 Rust 原生架構資訊");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!(
                    "核心版本：RuhI v{} (100% Pure Rust — ruhi-core + ruhi-tsf + ruhi-settings + atg_lib)",
                    env!("CARGO_PKG_VERSION")
                ))
                .strong(),
            );
            ui.label("架構特點：解耦純淨偏好設定中心 (ruhi-settings)，啟動記憶體零開銷");
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
            });
        });
    }

    

    

}

impl eframe::App for RuhiStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.shell.begin_frame(ctx);

        // 定期檢查單一實體喚醒訊號 (settings_wakeup.json / studio_wakeup.json) 與外部組態變更 (config.json)
        // 視窗最小化或未聚焦時拉長輪詢間隔並降低重繪頻率，消除閒置 250ms 連續喚醒耗電。
        let (is_minimized, is_focused) = ctx.input(|i| {
            (i.viewport().minimized.unwrap_or(false), i.focused)
        });
        let poll_interval_ms = if is_minimized || !is_focused { 1500 } else { 750 };
        if self.last_wakeup_check.elapsed() >= std::time::Duration::from_millis(poll_interval_ms)
        {
            self.last_wakeup_check = std::time::Instant::now();
            let wakeup_candidates = [
                self.root_dir.join("settings_wakeup.json"),
                self.root_dir.join("studio_wakeup.json"),
                self.root_dir.join("dist").join("settings_wakeup.json"),
                self.root_dir.join("dist").join("studio_wakeup.json"),
                self.root_dir.join("scripts").join("settings_wakeup.json"),
                self.root_dir.join("scripts").join("studio_wakeup.json"),
            ];
            let mut newest_val: Option<(u64, Value)> = None;
            for wakeup_path in &wakeup_candidates {
                if let Ok(raw) = std::fs::read_to_string(wakeup_path) {
                    if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                        let ts = val
                            .get("ts")
                            .or_else(|| val.get("timestamp"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        if ts > self.last_wakeup_ts
                            && newest_val.as_ref().map(|(best, _)| ts > *best).unwrap_or(true)
                        {
                            newest_val = Some((ts, val));
                        }
                    }
                }
            }
            if let Some((ts, val)) = newest_val {
                self.last_wakeup_ts = ts;
                if let Some(tab) = val
                    .get("tab")
                    .or_else(|| val.get("category"))
                    .and_then(Value::as_str)
                {
                    match tab {
                        "SemanticTuner" => {
                            self.current_category = StudioCategory::Appearance
                        }
                        "Appearance" => {
                            self.appearance_tree_expanded = true;
                            self.current_category = StudioCategory::Appearance;
                        }
                        "AppearanceLang" => {
                            self.appearance_tree_expanded = true;
                            self.current_category = StudioCategory::AppearanceLang;
                        }
                        "AppearanceInputMode" => {
                            self.appearance_tree_expanded = true;
                            self.current_category = StudioCategory::AppearanceInputMode;
                        }
                        "AppearanceCandidateDesk" => {
                            self.appearance_tree_expanded = true;
                            self.current_category = StudioCategory::AppearanceCandidateDesk;
                        }
                        "AppearanceThemeCapsule" => {
                            self.appearance_tree_expanded = true;
                            self.current_category = StudioCategory::AppearanceThemeCapsule;
                        }
                        _ => {}
                    }
                }
                self.settings = load_settings(&self.root_dir);
                let eff = self.effective_ui_lang();
                self.shell.set_language(Self::to_shell_i18n_lang(eff));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                #[cfg(target_os = "windows")]
                unsafe {
                    if let Some(hwnd) = find_any_settings_window_win32() {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{
                            BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow,
                            SW_RESTORE, SW_SHOW,
                        };
                        if IsIconic(hwnd) != 0 {
                            ShowWindow(hwnd, SW_RESTORE);
                        } else {
                            ShowWindow(hwnd, SW_SHOW);
                        }
                        let _ = BringWindowToTop(hwnd);
                        let _ = SetForegroundWindow(hwnd);
                    }
                }
            }

            if let Ok(meta) = std::fs::metadata(self.root_dir.join("config.json")) {
                if let Ok(mtime) = meta.modified() {
                    if self.last_config_mtime != Some(mtime) {
                        self.last_config_mtime = Some(mtime);
                        self.settings = load_settings(&self.root_dir);
                        let eff = self.effective_ui_lang();
                        self.shell.set_language(Self::to_shell_i18n_lang(eff));
                    }
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(poll_interval_ms));

        // 同步主題變更至 settings.appearance.theme
        let shell_theme_code = self.shell.theme_pref.as_code().to_string();
        if self.settings.appearance.theme != shell_theme_code {
            self.settings.appearance.theme = shell_theme_code;
        }

        let cur_ui_lang = self.effective_ui_lang();

        let (app_title, app_sub, zoom_btn_fmt, theme_btn_lbl, reset_btn_lbl, save_btn_lbl) = match cur_ui_lang {
            "en-US" => (
                "RuhI Universal IME — Preferences",
                "Responsive Universal Human Input · 100% Native Rust + UniversalUI (atg_lib)",
                format!("🔍 Zoom {}% (Ctrl+0)", (self.shell.ui_scale * 100.0).round() as i32),
                "🎨 Theme (Ctrl+T)",
                "↺ Reset Defaults",
                "  💾 Save & Apply  ",
            ),
            "ja-JP" => (
                "RuhI（如喜）入力システム — 環境設定センター",
                "雲の行くが如く水の流るるが如く · 100% ネイティブ Rust + UniversalUI (atg_lib)",
                format!("🔍 ズーム {}% (Ctrl+0)", (self.shell.ui_scale * 100.0).round() as i32),
                "🎨 テーマ切替 (Ctrl+T)",
                "↺ 既定値に戻す",
                "  💾 保存して適用  ",
            ),
            "ko-KR" => (
                "RuhI 범용 입력기 — 환경설정 센터",
                "구름 가듯 물 흐르듯 · 100% 네이티브 Rust + UniversalUI (atg_lib)",
                format!("🔍 배율 {}% (Ctrl+0)", (self.shell.ui_scale * 100.0).round() as i32),
                "🎨 테마 전환 (Ctrl+T)",
                "↺ 기본값 복원",
                "  💾 저장 및 적용  ",
            ),
            _ => (
                "RuhI（如喜）輸入法 — 現代化偏好設定中心",
                "如雲行水流自在歡喜 · 100% 原生 Rust + UniversalUI (atg_lib) 自適應架構",
                format!("🔍 縮放 {}% (Ctrl+0)", (self.shell.ui_scale * 100.0).round() as i32),
                "🎨 切換深淺色 (Ctrl+T)",
                "↺ 還原出廠預設值",
                "  💾 儲存並套用設定  ",
            ),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(app_title.to_string()));
        let nav_items: Vec<(usize, &str)> = StudioCategory::ALL
            .iter()
            .map(|c| (c.tree_depth(), c.sidebar_label_i18n(cur_ui_lang)))
            .collect();
        let min_sidebar_w = self.shell.ensure_sidebar_min_width(&nav_items);

        // 頂部品牌標頭列
        egui::TopBottomPanel::top("ruhi_studio_top_bar")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0 * self.shell.ui_scale);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(6.0 * self.shell.ui_scale);
                    // 詞塊 1：標題與副標題（整體不拆散，視窗收窄時下方控制列依 rud() 自動換行）
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(app_title)
                                .size(17.0 * self.shell.ui_scale)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(app_sub)
                                .size(11.5 * self.shell.ui_scale)
                                .color(self.shell.palette.fg_muted.to_egui()),
                        );
                    });
                    ui.add_space(10.0 * self.shell.ui_scale);

                    // 句子：控制元件詞塊流（各 ComboBox 與按鈕單字不折行，不足寬時整顆換行）
                    ui.scope(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        let avail_w = ui.available_width();

                        // 頂部標頭列直接輸入法快速切換下拉選單
                        let mut top_mode_changed = false;
                        let cur_mode_btn_text = format!(
                            "🎯 {}",
                            Self::mode_code_display_name(&self.settings.keyboard.layout)
                        );
                        let mode_w = (196.0 * self.shell.ui_scale).min((avail_w - 16.0).max(120.0));
                        egui::ComboBox::from_id_salt("ruhi_top_bar_quick_mode_switcher")
                            .selected_text(
                                egui::RichText::new(cur_mode_btn_text)
                                    .size(13.0 * self.shell.ui_scale)
                                    .strong()
                                    .color(self.shell.palette.accent.to_egui()),
                            )
                            .width(mode_w)
                            .show_ui(ui, |ui| {
                                for desc in ALL_MODES {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.keyboard.layout,
                                            desc.id.to_string(),
                                            desc.display_name,
                                        )
                                        .changed()
                                    {
                                        top_mode_changed = true;
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "一鍵切換當前預設／使用中輸入法（支援龍龕形音 4 碼、許氏雙拼、四角 4×5、行列 30、倉頡、標準大千、日韓語等）",
                            );
                        if top_mode_changed {
                            self.auto_save_settings_quiet();
                        }

                        // 頂部標頭列直接語言切換下拉選單
                        let mut top_lang_changed = false;
                        let cur_lang_btn_text = match self.settings.appearance.ui_lang.as_str() {
                            "zh-TW" => "🌐 繁體中文",
                            "en-US" => "🌐 English",
                            "ja-JP" => "🌐 日本語",
                            "ko-KR" => "🌐 한국어",
                            _ => match cur_ui_lang {
                                "en-US" => "🌐 Auto (English)",
                                "ja-JP" => "🌐 Auto (日本語)",
                                "ko-KR" => "🌐 Auto (한국어)",
                                _ => "🌐 Auto (繁體中文)",
                            },
                        };
                        let lang_w = (142.0 * self.shell.ui_scale).min((avail_w - 16.0).max(110.0));
                        egui::ComboBox::from_id_salt("ruhi_top_bar_quick_lang_switcher")
                            .selected_text(
                                egui::RichText::new(cur_lang_btn_text)
                                    .size(13.0 * self.shell.ui_scale)
                                    .strong(),
                            )
                            .width(lang_w)
                            .show_ui(ui, |ui| {
                                for (code, autonym) in [
                                    ("auto", "🌐 Auto (自動跟隨 / OS)"),
                                    ("zh-TW", "🇹🇼 繁體中文 (台灣)"),
                                    ("en-US", "🇺🇸 English"),
                                    ("ja-JP", "🇯🇵 日本語"),
                                    ("ko-KR", "🇰🇷 한국어"),
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.appearance.ui_lang,
                                            code.to_string(),
                                            autonym,
                                        )
                                        .changed()
                                    {
                                        top_lang_changed = true;
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "一鍵切換顯示語言 / Switch UI Language / 表示言語切替 / 표시 언어 전환",
                            );
                        if top_lang_changed {
                            self.cli_lang_override = None;
                            let eff = self.effective_ui_lang();
                            self.shell.set_language(Self::to_shell_i18n_lang(eff));
                            self.auto_save_settings_quiet();
                        }

                        if ui.button(theme_btn_lbl).clicked() {
                            let next = self
                                .shell
                                .theme_pref
                                .toggle_dark_light(self.shell.palette.is_dark);
                            self.shell.set_theme_preference(next);
                            self.settings.appearance.theme = next.as_code().to_string();
                        }
                        if ui
                            .button(zoom_btn_fmt)
                            .on_hover_text("支援 Ctrl + 滑鼠滾輪 或 Ctrl + +/-/0 即時無段縮放")
                            .clicked()
                        {
                            self.shell.set_ui_scale(1.0);
                        }
                    });
                });
                ui.add_space(4.0 * self.shell.ui_scale);
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
                        if ui.button(reset_btn_lbl).clicked() {
                            do_reset = true;
                        }
                    },
                    |ui| {
                        if ui
                            .button(egui::RichText::new(save_btn_lbl).strong())
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
                let scale = self.shell.ui_scale;
                for &cat in StudioCategory::ROOT_CATEGORIES {

                    if cat == StudioCategory::Appearance {
                        let arrow = if self.appearance_tree_expanded {
                            "▾ "
                        } else {
                            "▸ "
                        };
                        let parent_selected = self.current_category.is_appearance_group();
                        let parent_text =
                            format!("{arrow}{}", cat.sidebar_label_i18n(cur_ui_lang));
                        let resp = ui.selectable_label(
                            parent_selected,
                            egui::RichText::new(parent_text).size(14.0 * scale).strong(),
                        );
                        if resp.clicked() {
                            if self.current_category.is_appearance_group() {
                                self.appearance_tree_expanded = !self.appearance_tree_expanded;
                            } else {
                                self.appearance_tree_expanded = true;
                                self.select_category(StudioCategory::AppearanceLang);
                            }
                        }
                        ui.add_space(3.0);

                        if self.appearance_tree_expanded {
                            let active_branch = match self.current_category {
                                StudioCategory::Appearance => StudioCategory::AppearanceLang,
                                other => other,
                            };
                            for &branch in StudioCategory::APPEARANCE_BRANCHES {
                                let branch_selected = active_branch == branch;
                                ui.horizontal(|ui| {
                                    ui.add_space(14.0 * scale);
                                    let b_resp = ui.selectable_label(
                                        branch_selected,
                                        egui::RichText::new(branch.sidebar_label_i18n(cur_ui_lang))
                                            .size(13.2 * scale),
                                    );
                                    if b_resp.clicked() {
                                        self.select_category(branch);
                                    }
                                });
                                ui.add_space(2.0);
                            }
                            ui.add_space(4.0);
                        }
                    } else if cat == StudioCategory::KeyboardLayout {
                        let arrow = if self.keyboard_tree_expanded {
                            "▾ "
                        } else {
                            "▸ "
                        };
                        let parent_selected = self.current_category.is_keyboard_group();
                        let parent_text =
                            format!("{arrow}{}", cat.sidebar_label_i18n(cur_ui_lang));
                        let resp = ui.selectable_label(
                            parent_selected,
                            egui::RichText::new(parent_text).size(14.0 * scale).strong(),
                        );
                        if resp.clicked() {
                            if self.current_category.is_keyboard_group() {
                                self.keyboard_tree_expanded = !self.keyboard_tree_expanded;
                            } else {
                                self.keyboard_tree_expanded = true;
                                self.select_category(StudioCategory::KeyboardCjkPhonetic);
                            }
                        }
                        ui.add_space(3.0);

                        if self.keyboard_tree_expanded {
                            let active_branch = match self.current_category {
                                StudioCategory::KeyboardLayout => {
                                    StudioCategory::KeyboardCjkPhonetic
                                }
                                other => other,
                            };
                            for &branch in StudioCategory::KEYBOARD_BRANCHES {
                                let branch_selected = active_branch == branch;
                                ui.horizontal(|ui| {
                                    ui.add_space(14.0 * scale);
                                    let b_resp = ui.selectable_label(
                                        branch_selected,
                                        egui::RichText::new(branch.sidebar_label_i18n(cur_ui_lang))
                                            .size(13.2 * scale),
                                    );
                                    if b_resp.clicked() {
                                        self.select_category(branch);
                                    }
                                });
                                ui.add_space(2.0);
                            }
                            ui.add_space(4.0);
                        }
                    } else {
                        let selected = self.current_category == cat;
                        let resp = ui.selectable_label(
                            selected,
                            egui::RichText::new(cat.sidebar_label_i18n(cur_ui_lang))
                                .size(14.0 * scale),
                        );
                        if resp.clicked() {
                            self.select_category(cat);
                        }
                        ui.add_space(4.0);
                    }
                }
            });
        self.shell.sidebar_width = side_resp.response.rect.width();

        // 右側主內容區
        egui::CentralPanel::default().show(ctx, |ui| {
            let panel_avail_w = ui.available_width();
            let (title, desc) = self.current_category.page_header_i18n(cur_ui_lang);
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
                .show(ui, |ui| {
                    let content_w = (panel_avail_w - 10.0 * self.shell.ui_scale).max(200.0);
                    ui.set_max_width(content_w);
                    ui.set_min_width(content_w);
                    match self.current_category {
                        StudioCategory::Appearance | StudioCategory::AppearanceLang => {
                            self.render_appearance_lang_page(ui)
                        }
                        StudioCategory::AppearanceInputMode => {
                            self.render_appearance_input_mode_page(ui)
                        }
                        StudioCategory::AppearanceCandidateDesk => {
                            self.render_appearance_candidate_desk_page(ui)
                        }
                        StudioCategory::AppearanceThemeCapsule => {
                            self.render_appearance_theme_capsule_page(ui)
                        }
                        StudioCategory::KeyboardLayout | StudioCategory::KeyboardCjkPhonetic => {
                            self.render_keyboard_cjk_phonetic_page(ui)
                        }
                        StudioCategory::KeyboardCjkShape => {
                            self.render_keyboard_cjk_shape_page(ui)
                        }
                        StudioCategory::KeyboardJkEurasian => {
                            self.render_keyboard_jk_eurasian_page(ui)
                        }
                        StudioCategory::KeyboardHotkeysPunct => {
                            self.render_keyboard_hotkeys_punct_page(ui)
                        }
                        StudioCategory::MixedEnglish => self.render_mixed_english_page(ui),
                        StudioCategory::Grammar => self.render_grammar_page(ui),
                        StudioCategory::AiAndPrivacy => self.render_ai_privacy_page(ui),
                        // 在偏好設定中心內，不再把龐大的語意調校工作室硬塞進狹窄分頁，改以清爽的「獨立應用程式與 Web 介面啟動中心」呈現
                        StudioCategory::About => self.render_about_page(ui),
                    }
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

    // 1. 驗證全分類與「外觀與候選座」及「鍵盤配置與歐亞全譜樹」樹狀子分枝切換
    assert_eq!(StudioCategory::APPEARANCE_BRANCHES.len(), 4);
    for &b in StudioCategory::APPEARANCE_BRANCHES {
        assert!(b.is_appearance_group());
        assert_eq!(b.tree_depth(), 1);
    }
    assert_eq!(StudioCategory::KEYBOARD_BRANCHES.len(), 4);
    for &b in StudioCategory::KEYBOARD_BRANCHES {
        assert!(b.is_keyboard_group());
        assert_eq!(b.tree_depth(), 1);
    }
    for &cat in StudioCategory::ALL {
        app.select_category(cat);
        assert_eq!(app.current_category, cat);
    }

    // 2. 驗證側欄動態寬度精算 (calc_treeview_width 含樹狀階層縮排)
    let nav_items: Vec<(usize, &str)> = StudioCategory::ALL
        .iter()
        .map(|c| (c.tree_depth(), c.sidebar_label()))
        .collect();
    let min_w = app.shell.ensure_sidebar_min_width(&nav_items);
    assert!(min_w >= 200.0, "Sidebar min width too small: {min_w}");

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

    // 5. 驗證 HeuristicPiiFilter 隱私過濾
    assert!(HeuristicPiiFilter::is_safe_token("不懂").0);
    assert!(!HeuristicPiiFilter::is_safe_token("0912345678").0);
    assert!(!HeuristicPiiFilter::is_safe_token("user@example.com").0);

    // 6. 在暫存目錄測試 TelemetrySyncClient::import_tailcat_package 守門員
    let sandbox_dir = std::env::temp_dir().join(format!("ruhi_test_settings_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&sandbox_dir);
    let mut test_client = TelemetrySyncClient::new(Some(&sandbox_dir));
    let gate_res = test_client.import_tailcat_package(&json!({
        "protocol": "RUHI_TAILCAT_FEDERATED_SEMANTIC_V1",
        "latest_version": "0.7.1",
        "rules": [
            {"type": "UNIGRAM_WEIGHT", "word": "裡", "weight": 1200},
            {"type": "BIGRAM_TRANSITION", "prev": "歡迎", "next": "新架構", "recommended_delta": 28.0},
            {"type": "BIGRAM_TRANSITION", "prev": "0912345678", "next": "測試", "recommended_delta": 25.0},
            {"type": "COMPOUND_WORD", "word": "新架構", "bopomofo": "ㄒㄧㄣ ㄐㄧㄚˋ ㄍㄡˋ", "weight": 750}
        ]
    }));
    assert_eq!(gate_res["status"], "ok");
    assert_eq!(gate_res["blocked_unigram_rules"], 1);
    assert_eq!(gate_res["imported_compounds"], 1);
    assert_eq!(gate_res["imported_bigrams"], 1);
    let _ = std::fs::remove_dir_all(&sandbox_dir);

    // 7. 驗證 app.save_all() 雙軌設定同步
    app.save_all();

    println!("All headless self-tests passed for ruhi-settings!");
    0
}

#[cfg(target_os = "windows")]
unsafe fn find_any_settings_window_win32() -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetWindowTextW, IsWindow,
    };

    let exact_title: Vec<u16> = "RuhI（如喜）輸入法 — 現代化偏好設定中心\0"
        .encode_utf16()
        .collect();
    let h = FindWindowW(std::ptr::null(), exact_title.as_ptr());
    if !h.is_null() && IsWindow(h) != 0 {
        return Some(h);
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            if title.starts_with("RuhI（如喜）輸入法 — 現代化偏好設定")
                || title.starts_with("RuhI Universal IME — Preferences")
                || title.starts_with("RuhI（如喜）入力システム — 環境設定")
                || title.starts_with("RuhI 범용 입력기 — 환경설정")
            {
                let out_ptr = lparam as *mut HWND;
                if !out_ptr.is_null() {
                    *out_ptr = hwnd;
                }
                return 0;
            }
        }
        1
    }

    let mut found: HWND = std::ptr::null_mut();
    let _ = EnumWindows(Some(enum_proc), &mut found as *mut HWND as LPARAM);
    if !found.is_null() && IsWindow(found) != 0 {
        Some(found)
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--test") {
        std::process::exit(run_headless_self_test());
    }

    let mut is_tailcat_sync = false;
    let mut is_upload_revisions = false;
    let mut pull_community_url = String::new();
    let mut lang_arg = String::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tailcat-sync" => is_tailcat_sync = true,
            "--upload-revisions" => is_upload_revisions = true,
            "--pull-community" => {
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    pull_community_url = args[i + 1].clone();
                    i += 1;
                } else {
                    pull_community_url = "default".to_string();
                }
            }
            "--lang" | "-l" if i + 1 < args.len() => {
                lang_arg = args[i + 1].clone();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let root = resolve_ruhi_root();
    let settings = load_settings(&root);
    let mut sync_client = TelemetrySyncClient::new(Some(&root));

    if is_tailcat_sync {
        let feed = if !pull_community_url.is_empty() && pull_community_url != "default" {
            pull_community_url.as_str()
        } else {
            settings.ai_and_privacy.tailcat_feed_url.as_str()
        };
        let res = sync_client.pull_community_rules_with_etag(feed, true);
        println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
        return;
    }

    if !pull_community_url.is_empty() {
        let feed = if pull_community_url == "default" {
            settings.ai_and_privacy.tailcat_feed_url.as_str()
        } else {
            pull_community_url.as_str()
        };
        let res = sync_client.pull_community_rules_with_etag(feed, true);
        println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
        return;
    }

    if is_upload_revisions {
        let webhook = settings.ai_and_privacy.cloud_sheet_webhook_url.as_str();
        let res = sync_client.upload_revisions_to_cloud_sheet(webhook, settings.ai_and_privacy.zero_pii_gateway);
        println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
        return;
    }

    // Windows 單一實體檢查
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;

        #[link(name = "kernel32")]
        extern "system" {
            fn CreateMutexW(
                lp_mutex_attributes: *const std::ffi::c_void,
                b_initial_owner: i32,
                lp_name: *const u16,
            ) -> *mut std::ffi::c_void;
        }

        let mutex_name: Vec<u16> = "Local\\RuhiSettingsSingleInstanceMutex\0"
            .encode_utf16()
            .collect();
        let _mutex = unsafe { CreateMutexW(std::ptr::null(), 1, mutex_name.as_ptr()) };
        let already_running = unsafe { windows_sys::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS };

        if already_running {
            let payload = json!({
                "ts": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                "category": "Appearance",
            }).to_string();
            for p in &[
                root.join("settings_wakeup.json"),
                root.join("studio_wakeup.json"),
                root.join("dist").join("settings_wakeup.json"),
                root.join("dist").join("studio_wakeup.json"),
                root.join("scripts").join("settings_wakeup.json"),
                root.join("scripts").join("studio_wakeup.json"),
            ] {
                if let Some(parent) = p.parent() {
                    if parent.is_dir() {
                        let _ = std::fs::write(p, &payload);
                    }
                }
            }
            unsafe {
                if let Some(hwnd) = find_any_settings_window_win32() {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                    };
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_SHOW);
                    }
                    let _ = BringWindowToTop(hwnd);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            return;
        }
    }

    let initial_cat = StudioCategory::Appearance;
    let lang_opt = if lang_arg.is_empty() { None } else { Some(lang_arg) };
    let app = RuhiStudioApp::new_with_lang(initial_cat, lang_opt);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RuhI（如喜）輸入法 — 現代化偏好設定中心")
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([720.0, 520.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RuhI（如喜）輸入法 — 現代化偏好設定中心",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    );
}

#[cfg(test)]
mod studio_logo_tests {
    use super::RuhiStudioApp;

    fn has_color(rgba: &[u8], rgb: [u8; 3]) -> usize {
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
    fn test_settings_studio_logo_rle3_decodes_dual_theme() {
        let (lw, lh, light) =
            RuhiStudioApp::decode_studio_logo_premul_rgba(false).expect("light studio logo");
        let (dw, dh, dark) =
            RuhiStudioApp::decode_studio_logo_premul_rgba(true).expect("dark studio logo");
        assert_eq!((lw, lh), (640, 100));
        assert_eq!((dw, dh), (640, 100));
        assert!(has_color(&light, [0x00, 0xA2, 0xE9]) > 80);
        assert!(has_color(&light, [0xE4, 0x00, 0x82]) > 30);
        assert!(has_color(&light, [0x2F, 0x31, 0x8B]) > 1500);
        assert_eq!(has_color(&light, [0xE8, 0xEE, 0xF8]), 0);
        assert!(has_color(&dark, [0xE8, 0xEE, 0xF8]) > 1500);
        assert_eq!(has_color(&dark, [0x2F, 0x31, 0x8B]), 0);
    }
}
