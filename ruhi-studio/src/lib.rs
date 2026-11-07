//! # `ruhi_studio` — 如喜輸入法 (RuhI) 原生 Rust 現代化偏好設定與語意調校工作室程式庫
//!
//! 整合 [`atg_lib`] (`C:\ATGprjs\_lib_rs`) 全域桌面架構，提供：
//! - [`config_sync`]：`settings_config.json` 與 `config.json` 雙軌設定載入、預設值校驗與跨目錄即時同步。
//! - [`tuner`]：雙向最大匹配分詞 (`BiMM`)、六大計算語言學病灶分類 (`Type I` ～ `Type VI`)、二階轉移合成、黃金基準測試與 Zero-PII 隱私安全閘門。
//! - [`ai_consultant`]：內建確定性計算語言學與多維構式語意分析引擎（並支援選配遠端雲端 API）。
//! - [`web_server`]：相容 `scripts/web/index.html` 之原生多執行緒 HTTP 語意調校伺服器 (`127.0.0.1:8765`)。

pub mod ai_consultant;
pub mod config_sync;
pub mod http_client;
pub mod tuner;
pub mod web_server;
