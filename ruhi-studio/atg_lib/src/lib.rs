//! # `atg_lib` (`C:\ATGprjs\_lib_rs`) — ATGprjs / RuhOS 全域共用核心與桌面 UI 程式庫 (Rust 原生版)
//!
//! 本程式庫為 `C:\ATGprjs\_lib` (Python 版) 的完整 Rust 原生對等實作，將長期累積的桌面工程標準與語言學演算法封裝為高内聚、型別安全之模組體系：
//!
//! ## 模組總覽
//! - [`i18n`]：試算表 (TSV/CSV) 矩陣式多語系引擎，支援 `zh_TW`、`en_US`、`zh_CN`、`ja_JP` 零重啟即時切換與 `{placeholder}` 參數插值。
//! - [`config_store`]：`_cfg/<app>_cfg.json` 組態持久化、視窗/分割軸幾何記憶、跨目錄相容組態掃描與多組態設定檔 (`Profiles`)。
//! - [`preset_engine`]：使用頻率加權 + 時間半衰期衰減之自適應常用項目引擎 (`PresetEngine`)。
//! - [`undo_manager`]：泛型深拷貝快照隔離之多步復原/重做管理器 (`UndoManager<T>`)。
//! - [`cjk_sort_engine`]：五合一漢字多模態檢索排序引擎（台灣教育部注音、康熙 214 部首、總筆畫、漢語拼音、台語羅馬字）與曲目編號前綴智慧剝離。
//! - [`power_engine`]：Win32 顯示器電源控制 (`SC_MONITORPOWER`)、S3 睡眠與硬體喚醒計時器 (`HardwareWakeTimer`)。
//! - [`ai_engine`]：純綠版免安裝 `llama.cpp` (`llama-server` + 本地 `.gguf`) 熱插拔離線 AI 推論引擎（Google Gemma 絕對優先，徹底排除誘導訂閱之 Ollama）。
//! - [`theme_engine`]：「專業暗色 (`StudioDark`)」與「溫潤宣紙淺色 (`RicePaperLight`)」語意調色盤、Win32 DWM 沉浸式暗色標題列與系統外觀即時監聽。
//! - [`rud_engine`]：`rud()` 空間幾何第一原理（漢字避頭尾智慧折行、短標籤雙端齊行、表格二階段需求驅動欄寬分配、垂直三欄收斂與樹狀導覽寬度精算）。
//! - [`dialog_engine`]：右下角非阻塞浮動通知 (`ToastManager`)、語意調色盤訊息框 (`UniversalMessageBox`) 與關閉螢幕倒數計時器 (`ScreenOffCountdownDialog`)。
//! - [`table_engine`]：整合 `rud()` 自適應欄寬、多模態漢字排序、右鍵欄位選單與 `[##1]` 批次流水號行內編輯之表格引擎 (`UniversalTable`)。
//! - [`app_shell`]：一站式桌面應用程式外殼 (`UniversalAppShell`)，內建無段 UI 縮放 (`Ctrl + +/-/0/滾輪`)、即時換膚 (`Ctrl + T`) 與偏好設定中心 (`Ctrl + ,`)。

pub mod ai_engine;
pub mod app_shell;
pub mod cjk_sort_engine;
pub mod config_store;
pub mod dialog_engine;
pub mod i18n;
pub mod power_engine;
pub mod preset_engine;
pub mod rud_engine;
pub mod table_engine;
pub mod theme_engine;
pub mod undo_manager;

/// 常用型別與函式匯出 (`use atg_lib::prelude::*;`)
pub mod prelude {
    pub use crate::ai_engine::{
        AIQueryResult, AIStatus, GpuHardwareInfo, LocalModelInfo, ModelCapability, RuhAIEngine,
    };
    pub use crate::app_shell::UniversalAppShell;
    pub use crate::cjk_sort_engine::{CjkSortEngine, CjkSortMode};
    pub use crate::config_store::{AppConfig, WindowGeometry};
    pub use crate::dialog_engine::{
        MessageBoxButtons, MessageBoxIcon, MessageBoxResult, ScreenOffCountdownDialog, ToastLevel,
        ToastManager, UniversalMessageBox,
    };
    pub use crate::i18n::{init as i18n_init, tr, tr_args, I18nEngine};
    pub use crate::power_engine::{
        allow_screen_sleep, enter_system_s3_sleep, lock_workstation, prevent_screen_sleep,
        turn_off_screen, wake_screen, HardwareWakeTimer,
    };
    pub use crate::preset_engine::{AdaptivePresetDomain, AdaptivePresetItem, PresetEngine};
    pub use crate::rud_engine::{
        calc_treeview_width, calculate_table_rwd_widths, calculate_vertical_rwd_panes,
        cjk_smart_wrap, justify_short_label, visual_char_length, RwdColMode, RwdColumnSpec,
        VerticalPaneHeights,
    };
    pub use crate::table_engine::{
        expand_batch_sequence_pattern, TableColumnDef, TableRow, UniversalTable,
    };
    pub use crate::theme_engine::{
        apply_win32_dwm_titlebar, apply_win32_dwm_titlebar_for_current_process,
        get_system_accent_color, is_system_dark_mode, ColorRgb, SemanticPalette,
        SystemThemeWatcher, ThemePreference,
    };
    pub use crate::undo_manager::UndoManager;

    #[cfg(feature = "ui")]
    pub use crate::dialog_engine::rud_button_bar;
    #[cfg(feature = "ui")]
    pub use crate::rud_engine::{
        justified_cjk_label, rud_atomic_group, rud_flowing_bar, two_tier_checkbox, two_tier_radio,
    };
    #[cfg(feature = "ui")]
    pub use crate::theme_engine::{apply_palette_to_egui, configure_cjk_fonts};
}
