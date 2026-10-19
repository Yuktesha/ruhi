# 如喜輸入法 (RuhI) 語意架構演進、詞庫統合與 Rust 原生全面同步成果報告

- **負責工作階段 (Source Conversation)**：`741e6836-1bef-4d3a-a70f-218b9d857259`
- **協同工作階段 (Rust Rewrite Peer)**：`abd8e446-d2c9-4c46-8f36-8cb6b6257121`
- **統整交付對象 (Release Consolidation Target)**：`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`
- **基準日期**：2026-10-19

---

## 一、執行摘要 (Executive Summary)

本工作階段（`741e6836-1bef-4d3a-a70f-218b9d857259`）已完成四大核心工程，並與 `abd8e446-d2c9-4c46-8f36-8cb6b6257121` 所建置之純 Rust 原生工作室套件 [`ruhi-studio`](file:///C:/ATGprjs/ruhi/ruhi-studio/Cargo.toml) 及核心引擎 [`ruhi-core`](file:///C:/ATGprjs/ruhi/ruhi-core/Cargo.toml) 完成 100% 雙向同步、編譯與全路徑部署驗證：

1. **小麥注音與教育部《重編國語辭典修訂本》（萌典）之詞庫統合消化（「致敬來源、消化為一」架構）**：
   - 徹底解除過往兩套來源權重尺度衝突造成的「雙重人格分裂」問題。
   - 於 [`lexicon_moe.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv) 中將 **60,805 筆**與 [`lexicon_mcbopomofo.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_mcbopomofo.tsv) 重疊之詞條權重與小麥基準完全對齊；**4,106 筆**純萌典擴充詞條實施單字/雙字/長詞分層權重上限管制；剔除「旦會、試藥（偽裝高分）」等罕見古漢語干擾項，並完整保留 **196 筆**如喜專屬校準之核心語法單字（`category == "core"`，如 `句 900`）。
2. **「與」之五維語法消歧與 8 組高頻同音字雙向對比句自然出現能力**：
   - 於 [`ruhi-core/src/router/constructions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 與 [`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs) 建立「與」的五大語法環境識別（NP 並列連詞、精確雙音節複合詞、書面介詞結構、引號元語言提及、句首誤觸抑制）。
   - 嚴格遵守「嚴禁單字權重打地鼠」鐵律，在完全不改動無上下文單字先驗權重（Unigram Prior）的前提下，達成 **8 組核心同音字／詞（是/視、在/再、的/得、與/語/雨/予、但/旦/淡、裡/理/禮、檔/黨/擋、常/長/嚐）於 28 句正反對比自然語境中 100% 雙向正確浮現**。
3. **Tailcat 方案 A（GitHub / Gist `ETag` 條件式同步 + 本地端三層語意沙盒守門員）Rust 原生實作**：
   - 將 Python 原型 [`tools/semantic_federation.py`](file:///C:/ATGprjs/ruhi/tools/semantic_federation.py) 完整移植並同步至 Rust 原生 [`ruhi-studio/src/tuner.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs)、[`ruhi-studio/src/web_server.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/web_server.rs) 與 [`ruhi-studio/src/main.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/main.rs)。
   - 支援 `If-None-Match` / `ETag` 與 `HTTP 304 Not Modified` CDN 快取零流量檢查，以及本地端三層沙盒守門員（攔截 `UNIGRAM_WEIGHT`、攔截對核心功能詞之負向懲罰、強制將二階轉移增量鉗制於 `[-35.0, +35.0]` 並執行 Zero-PII 隱私審查）。
4. **Rust 原生 `ruhi-studio` 黃金基準擴充（20 組 ➔ 30 組）與全發佈路徑二進位檔同步**：
   - [`GOLDEN_BENCHMARK`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L17-L198) 擴充至 **30 組**（涵蓋雙向同音對比句與「與」字語意規則分析句），執行 `ruhi-studio.exe --test` 與 `ruhi-studio.exe --verify` 均達成 **30/30 (100.0%)** 通過。

---

## 二、核心檔案變更與跨模組同步清單 (Modified & Synchronized Files)

### 1. Rust 核心引擎 (`ruhi-core` & `ruhi-tsf`)
| 檔案路徑 | 關鍵結構與函式 | 變更說明 |
| :--- | :--- | :--- |
| [`ruhi-core/src/router/constructions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) | [`evaluate_construction_bonus`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L228-L820) | 實作「與」之五維語法分析（並列連詞 `X + 與 + Y`、介詞 `對 + 與 + NP`、引號提及、句首抑制）及 8 組同音字雙向語法構式消歧規則。 |
| [`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs) | [`TransitionMatrix::new`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L96-L860) | 補齊二階搭配轉移（如 `是要 ➔ 請`、`對 ➔ 與`、`加以 ➔ 分析`、`照理 ➔ 說`、`但 ➔ 會不會`、`造成 ➔ 負擔` 等），移除任何破壞對比句平衡之極端偏置。 |
| [`ruhi-core/src/lexicon/manager.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs) | [`LexiconManager::get_entries_with_dynamic`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs#L406-L520) | 優化動態構詞與多音字詞條（如「會計 `ㄎㄨㄞˋ_ㄐㄧˋ`」、「銀行 `ㄧㄣˊ_ㄏㄤˊ`」）檢索保護。 |
| [`ruhi-core/src/engine.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs) | [`test_eight_homophone_pairs_in_contrasting_sentences_261019`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L3170-L3330)、[`test_contrastive_conjunction_yu_and_homophone_cases_261019_b`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L3332-L3430) | 新增 28 句雙向同音對比自然句單元測試與「與」字語意規則、方案 A 提問句（`但會不會對系統造成負擔`）回歸測試，全數通過。 |

### 2. Rust 原生偏好設定與語意調校工作室 (`ruhi-studio`)
| 檔案路徑 | 關鍵結構與函式 | 變更說明 |
| :--- | :--- | :--- |
| [`ruhi-studio/src/tuner.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs) | [`GOLDEN_BENCHMARK`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L17-L198)、[`SemanticTuner::import_tailcat_package`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L1209-L1308)、[`SemanticTuner::pull_community_rules_with_etag`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L1310-L1404) | 1. 黃金基準測試由 20 組擴充至 **30 組**。<br>2. 新增本地端三層語意沙盒守門員 [`import_tailcat_package`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L1209-L1308)。<br>3. 新增 GitHub/Gist 條件式 `ETag` / `HTTP 304` 低頻同步 [`pull_community_rules_with_etag`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L1310-L1404)。<br>4. 新增單元測試 [`test_tailcat_sandbox_gatekeeper`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/tuner.rs#L1565-L1580)。 |
| [`ruhi-studio/src/web_server.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/web_server.rs) | [`handle_api_post`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/web_server.rs#L672-L693) | 新增 `POST /api/tailcat/import` 與 `POST /api/tailcat/pull_github` 兩個 REST API 端點，供 Web 語意工作室直接呼叫。 |
| [`ruhi-studio/src/main.rs`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/main.rs) | [`run_headless_self_test`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/main.rs#L1153-L1168)、[`main`](file:///C:/ATGprjs/ruhi/ruhi-studio/src/main.rs#L1250-L1305) | 於 `--test` 自檢流程加入 Tailcat 沙盒守門員與 30 組黃金基準斷言；於 CLI 加入 `--pull-community <url>` 參數支援。 |

### 3. 統合詞庫與文件資產 (`ruhi_lexicon/data` & `docs`)
| 檔案路徑 | 同步副本路徑 | 變更說明 |
| :--- | :--- | :--- |
| [`ruhi_lexicon/data/lexicon_moe.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv) | `data/lexicon_moe.tsv`、`scripts/data/lexicon_moe.tsv`、`dist/data/lexicon_moe.tsv`、`dist/RuhI_v0.2.0_Windows_x64/data/lexicon_moe.tsv` | 完成小麥注音與萌典權重尺度統合（60,805 筆對齊、4,106 筆萌典擴充詞分層限幅、196 筆 `core` 核心詞保留、罕見古詞剔除）。 |
| [`ruhi_lexicon/data/lexicon_domain.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv) | 同上 4 處部署路徑 | 清理單方面壓制同音字之人工硬編碼詞條，補充自然雙字搭配（如 `試藥 1420`、`是要 1520`）。 |
| [`ruhi_lexicon/data/transitions_user.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv) | 同上 4 處部署路徑 | 清理過往測試殘留之無效轉移，同步最新二階搭配規則。 |
| [`docs/LEXICON_ARCHITECTURE_AND_SOURCES.md`](file:///C:/ATGprjs/ruhi/docs/LEXICON_ARCHITECTURE_AND_SOURCES.md) | — | 正式記載小麥注音（McBopomofo, MIT）與教育部《重編國語辭典修訂本》／萌典（Moedict, CC BY-ND 3.0 TW / CC0）之致謝聲明與「單一權重宇宙消化管線」設計規範。 |

---

## 三、已同步之執行檔與發佈路徑清單 (Synchronized Binaries for `8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`)

為方便 `8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708` 直接進行最終封裝（Inno Setup 安裝包與免安裝 Portable ZIP），下列所有二進位檔與資料檔均已同步為最新 Release 版本：

1. **TSF 核心輸入法引擎 (`ruhi_tsf.dll`)**：
   - [`C:\ATGprjs\ruhi\target\release\ruhi_tsf.dll`](file:///C:/ATGprjs/ruhi/target/release/ruhi_tsf.dll)
   - [`C:\ATGprjs\ruhi\dist\ruhi_tsf.dll`](file:///C:/ATGprjs/ruhi/dist/ruhi_tsf.dll)
   - [`C:\ATGprjs\ruhi\dist\RuhI_v0.2.0_Windows_x64\ruhi_tsf.dll`](file:///C:/ATGprjs/ruhi/dist/RuhI_v0.2.0_Windows_x64/ruhi_tsf.dll)
2. **Rust 原生偏好設定與語意調校工作室 (`ruhi-studio.exe`)**：
   - [`C:\ATGprjs\ruhi\ruhi-studio\target\release\ruhi-studio.exe`](file:///C:/ATGprjs/ruhi/ruhi-studio/target/release/ruhi-studio.exe)
   - [`C:\ATGprjs\ruhi\ruhi-studio.exe`](file:///C:/ATGprjs/ruhi/ruhi-studio.exe)
   - [`C:\ATGprjs\ruhi\scripts\ruhi-studio.exe`](file:///C:/ATGprjs/ruhi/scripts/ruhi-studio.exe)
   - [`C:\ATGprjs\ruhi\dist\ruhi-studio.exe`](file:///C:/ATGprjs/ruhi/dist/ruhi-studio.exe)
   - [`C:\ATGprjs\ruhi\dist\RuhI_v0.2.0_Windows_x64\ruhi-studio.exe`](file:///C:/ATGprjs/ruhi/dist/RuhI_v0.2.0_Windows_x64/ruhi-studio.exe)
3. **統合詞庫與二階轉移表 (`data/*.tsv`)**：
   - `C:\ATGprjs\ruhi\ruhi_lexicon\data\`
   - `C:\ATGprjs\ruhi\data\`
   - `C:\ATGprjs\ruhi\scripts\data\`
   - `C:\ATGprjs\ruhi\dist\data\`
   - `C:\ATGprjs\ruhi\dist\RuhI_v0.2.0_Windows_x64\data\`

---

## 四、自動化測試與驗證數據 (Verification Summary)

| 測試套件 / 驗證指令 | 測試範圍 | 通過率 / 結果 |
| :--- | :--- | :--- |
| `cargo test -p ruhi-core` | 核心注音解碼、Viterbi Trellis、五維「與」字語法、28 句雙向同音對比句、多音字校準 | **100% 通過**（0 failed） |
| `cargo test` (`ruhi-studio`) | 設定同步、Zero-PII 隱私過濾、雙向詞界對齊、Tailcat 本地端語意沙盒守門員 | **4 / 4 通過**（100%） |
| `ruhi-studio.exe --test` | `atg_lib` 原生 `rud()` 版面、深淺色主題、DPI 縮放、Tailcat 守門員、30 組黃金基準 | **`[OK]` 0 報錯通過** |
| `ruhi-studio.exe --verify` | 30 組黃金基準測試集（含同音字雙向對比與「與」字語法分析） | **30 / 30 (100.0%)** |
