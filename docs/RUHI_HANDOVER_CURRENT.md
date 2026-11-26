# RuhI 現況交接書（`v0.9.6.11.26` 61 歲生日特別版，2026-11-26）

> 本文件為每次新對話啟動時應優先閱讀的單一權威實況摘要，取代過時之過渡期筆記。
> 更早期歷程仍留存於 [`RUHI_HANDOVER_LOG.md`](RUHI_HANDOVER_LOG.md) 與 [`RUHI_V1_BENCHMARK_REPORT.md`](RUHI_V1_BENCHMARK_REPORT.md)。

---

## 一、目前穩定基準點（Milestone Snapshot）

- **目前釋出版本**：**`v0.9.6.11.26`**（61 歲生日特別版，基於 `v0.9.6.5 / b01f726` 持續整合完成）
- **活躍雙向神經模型組合（定案保留 `C6`）**：
  - **正向 (L2R)**：[`rlnm_zh_tw.bin`](../ruhi_lexicon/data/rlnm_zh_tw.bin)（`C6-L2R-32k`，`22,847,208` 位元組、`payload_fnv64 = 0x6901f0ff1187b536`）
  - **逆向 (R2L)**：[`rlnm_zh_tw_r2l.bin`](../ruhi_lexicon/data/rlnm_zh_tw_r2l.bin)（`C6-R2L-32k`，`22,847,208` 位元組、`payload_fnv64 = 0xae811525a1c68037`）
  - **四處資料鏡像同步**：[`ruhi_lexicon/data/`](../ruhi_lexicon/data/)、[`data/`](../data/)、[`scripts/data/`](../scripts/data/) 與 [`dist/data/`](../dist/data/)（模型、詞庫與 `lint_scoring_rules.py --full` 零違規完全一致）。
- **核心詞庫規模**：[`lexicon_core.tsv`](../ruhi_lexicon/data/lexicon_core.tsv) 與 [`data/lexicon_core.tsv`](../data/lexicon_core.tsv) 雙鏡像位元組完全一致（SHA-256 `b9f29b11dff64864aeee0379bf8db3acdde895ca40a5c982c0b8db33f4d914bb`），含新補入詞條 `正寫\tㄓㄥˋ ㄒㄧㄝˇ\t310`。
- **預設線上參數**：
  - `enable_neural_reranker = true`、`neural_reranker_lambda = 4.5`、`neural_reranker_timeout_ms = 35`、`DEFAULT_MAX_TRIE_NODES = 200`
  - `variant_memory_enabled = true`、`variant_preset = "general"`（支援 `general`／`official_tw`／`custom`）

---

## 二、量化評測指標（`v0.9.6.11.26` 實測）

| 資料集與規模 | 模式 | CER | 總字錯數 | 整句完全命中率 | 關鍵說明 |
|---|---|---:|---:|---:|---|
| **`dev.tsv`**（3,640 句／44,855 字） | 離線閘門 | `3.43%` | `1,537` | `74.26%` (`2,703`) | `--gate PASS`（`fixed=11, broken=0`） |
| **`dev.tsv`**（3,640 句／44,855 字） | **雙向神經 (`λ=4.5`)** | **`2.12%`** | **`949`** | **`83.10%` (`3,025`)** | 維持 `C6` 定案基準（五大文體：`145 / 150 / 170 / 122 / 362`） |
| **`test.tsv`**（1,560 句／19,058 字） | 離線盲測 | `4.34%` | `827` | `68.27%` (`1,065`) | 凍結後單次執行 |
| **`test.tsv`**（1,560 句／19,058 字） | **雙向神經 (`λ=4.5`)** | **`2.60%`** | **`495`** | **`79.29%` (`1,237`)** | 相對離線 **`-40.15%`**（`-332` 字錯），較 `v0.9.6` 再減 `-90` 字錯 |
| **`typing_tw.tsv`**（4,256 句／48,911 字） | 離線盲測 | `9.01%` | `4,406` | `45.47%` (`1,935`) | 凍結後單次執行 |
| **`typing_tw.tsv`**（4,256 句／48,911 字） | **雙向神經 (`λ=4.5`)** | **`6.10%`** | **`2,983`** | **`58.04%` (`2,470`)** | 相對離線 **`-32.30%`**（`-1,423` 字錯） |

- **雙向側車延遲基準（`ruhi_reranker --bench --threads 8`）**：`mode = bidirectional`、`max_trie_nodes = 200`、`workers = 8`、`n = 14` 測得 **`p50 = 11.66 ms`｜`p95 = 32.27 ms`｜`max = 32.27 ms`**（`#8` 前文 KV 快取 `1.32x` 加速、`max_drift = 0.00e0`）。
- **自動化測試與閘門**：`ruhi-core --lib` **`308 passed; 0 failed; 9 ignored`**、`ruhi-tsf --lib` **`31 passed; 0 failed`**、`ruhi-settings` **`10+1 passed; 0 failed`**、`lint_scoring_rules.py --full` **`0` 違規**。

---

## 三、`v0.9.6.11.26` 新增核心能力

1. **異體雙寫自動記憶開關（[`variant_prefs.rs`](../ruhi-core/src/router/variant_prefs.rs)）**：
   - 以 `OnceLock<RwLock<HashMap<usize, VariantPrefsState>>>` 旁表索引 `cache_lm_key()`，**嚴格不變動任何 COM ABI 結構配置**。
   - 支援 `general`（一般自動學習）、`official_tw`（中華民國公文書正寫偏好：`臺北／臺灣／臺中／臺南／臺東／公佈／瞭解` 等 19 組無歧義雙寫對）與 `custom`，並具備 `surname_or_name_overrides`（姓氏與人名語境保護）、`Program Files` 只讀目錄自動轉存 `%APPDATA%\Ruhi\variant_prefs.json` 與神經重排後覆寫防護；同步整合至 [`ruhi-tsf/src/service.rs`](../ruhi-tsf/src/service.rs) 與 [`ruhi-settings`](../ruhi-settings/src/main.rs) `rud()` 介面。
2. **共用無分歧主幹 LM-Head 相對快速評分與 `D2-B` 基線修補**：
   - 保留 `score_candidates_trie` 與 `score_candidates_bidirectional` 精確介面不變，新增 `score_candidates_trie_fast_relative` 與 `score_candidates_bidirectional_fast`；修補 `D2-B` 多島重組於同一 2 候選呼叫比較 `scores[1]` 與 `scores[0]`。
3. **`KNOWN_HARD_CASES.md` 雙向神經盤點（第 6 節）**：
   - 新增 [`test_v0961126_known_hard_cases_bidirectional_audit`](../ruhi-core/src/engine.rs)：12 項薄邊際案例由離線 `1 / 12` 提升至雙向神經 **`7 / 12` 通過**（Stage B2 `3 / 3` 全過）。
4. **P2-4 四層宣告式擴充基礎載入器（[`extensions.rs`](../ruhi-core/src/extensions.rs)）**：
   - 提供零外部依賴 `manifest.toml` 解析、三類 FSM 原型（`SyllabicCombinator`、`CodeTableShape`、`TokenizingLexeme`）、路徑安全檢核與跨目錄探索，規格見 [`ruhi_lexicon/data/extensions/README.md`](../ruhi_lexicon/data/extensions/README.md)。
5. **`C7` 訓練實驗記錄（不晉升）**：
   - [`eval_reports/local/c7_work/`](../eval_reports/local/c7_work/) 以 `C6` 併入台灣維基百科／維基新聞擴增至 `209,944,176` 字元接續訓練 `12,000` 步；因 `dev.tsv` 精確句數略遜 `C6`（`3,017` vs. `3,025`），依閘門保留 `C6`。

---

## 四、架構不變量與作業紅線（新對話必檢核）

1. **COM ABI 不變量**：嚴格禁止隨意變更 `RuhiTextService` / `RuhiEngine` / `TransitionEngine` 結構體排版（`LAYER_B_ABI_EPOCH = 2`）；跨執行緒或旁路狀態應採用 `OnceLock` 旁表索引 `cache_lm_key()` 或 `&self as *const Self as usize`。
2. **詞庫雙鏡像與品質閘門**：[`ruhi_lexicon/data/lexicon_core.tsv`](../ruhi_lexicon/data/lexicon_core.tsv) 與 [`data/lexicon_core.tsv`](../data/lexicon_core.tsv) 必須維持位元組一致；修改構式／詞權後必須通過 `python scripts/lint_scoring_rules.py --full`（0 違規）與 `ruhi_eval.exe --set dev --gate`（`PASS`）。
3. **本機環境工具鏈與 GUI 禁用規則**：嚴禁對 GUI 程式使用 `IsDaemon: true`；所有 `.ps1` 須保持 UTF-8 BOM 並以 Windows PowerShell 5.1 驗證語法；雲端訓練僅限 Colab H100/A100 且須於 `finally` 執行 `colab stop`（確認 `0` 活躍工作階段）。
4. **盲測紀律**：禁止以 `test.tsv` 或 `typing_tw.tsv` 作為任何微調、權重調整或超參數挑選依據。

---

## 五、後續可規劃方向（Next Directions）

1. **P2-4 擴充 FSM 執行期掛載與社群外掛示範包**：基於 [`extensions.rs`](../ruhi-core/src/extensions.rs) 完成執行期多鍵盤／外掛熱切換掛載。
2. **科技文件 (`tech_doc`) 與句首冷啟動語境精進**：針對盲測與開發集中比例最高的跨詞指代（`他／它／她／牠`）與技術專有名詞誤用進行詞彙／語法規則補充。
3. **更大視窗或精煉版語料（`C8`）全程雙向訓練**：於更長步數搭配更高比例對話與技術文章比例重新驗證 `dev.tsv` 全句命中率是否嚴格突破 `3,025`。
