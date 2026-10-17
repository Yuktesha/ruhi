# 如喜（RuhI）語意調校與多語系消歧工作成果打包交接清單
**來源對話 ID**：`741e6836-1bef-4d3a-a70f-218b9d857259`  
**目標打包對話 ID**：`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`  
**更新時間**：2026-10-17

本文件彙整 `741e6836-1bef-4d3a-a70f-218b9d857259` 完成之所有 Rust 核心語意消歧模組、Python 語意調校工作室升級、詞庫與轉移表數據、以及多語系擴充規範文件，供 `8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708` 執行完整建置與安裝包封裝。

---

## 一、核心功能成果總覽（Key Deliverables）

### 1. Rust 核心引擎（`ruhi-core`）
1. **Layer 1 數詞與曆法日期／貨幣／度量衡／時間單位通用語法規則**：
   - 檔案：[`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs)
   - 實作 `WordPattern::Numeric` 與 `WordPattern::UnitClassifier`，當前驅詞為阿拉伯數字（如 `11`、`2026`、`100`）、中文數詞（`一～十、百、千、萬、兩、幾`）或複合數詞（`二十五`）時，對後方曆法日期（`年、月、日、號、週、星期`）、貨幣（`元、塊、萬、億、台幣、美金`）、度量衡（`坪、公尺、公分、公斤、公升、度、瓦`）與時間單位（`秒、分、分鐘、小時、鐘頭、天、歲`）統一提供 `+38.0` 構式引力。
2. **字形輸入法（倉頡／速成／大易／行列）重碼情境消歧**：
   - 檔案：[`ruhi-core/src/cangjie/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/cangjie/mod.rs)、[`ruhi-core/src/cangjie/table.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/cangjie/table.rs)
   - 單一候選字維持 100% 確定性直出；多候選重碼字（如倉頡 `wg` 理/裡、`mmu` 頑/元、`gm` 瓶/坪、`it` 渡/度、`to` 越/月）與速成模式接入 `TransitionEngine` 與詞庫雙字詞凝聚力動態排序。
3. **歐文多語系（英／法／西／德）語法同音詞與無重音快速鍵入消歧**：
   - 檔案：[`ruhi-core/src/western/scoring.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/western/scoring.rs)、[`ruhi-core/src/western/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)
   - 實作 `WesternConfidenceScorer::rank_contextual_confusables` 與 `WesternEngine::lookup_contextual_confusables`，支援英語同音詞（`their/there/they're`, `your/you're`, `its/it's`, `than/then`, `affect/effect`, `to/too`）與法語／西語重音變音（`a/à`, `ou/où`, `sur/sûr`, `tu/tú`, `el/él`, `mas/más`）之上下文語法消歧。

---

### 2. 如喜語意調校工作室與 AI 語意顧問（`scripts/`）
- 檔案：
  - [`scripts/semantic_tuner.py`](file:///C:/ATGprjs/ruhi/scripts/semantic_tuner.py)
  - [`scripts/ai_semantic_consultant.py`](file:///C:/ATGprjs/ruhi/scripts/ai_semantic_consultant.py)
- 升級重點：
  1. **中英混打與數詞邊界隔離**：嚴格阻斷跨越阿拉伯數字與 ASCII 詞元抓取遠端漢字（徹底根除「待10月15日」生出 `待 ➔ 月`、`月 ➔ 日`，或「將aup檔」生出 `將 ➔ 檔` 之跨界偽規則）。
  2. **十大高階漢語與跨語系構式合成（`synthesize_hierarchical_constructions_for_diff`）**：
     - 數詞與度量衡／曆法日期／貨幣／時間單位構式（`[<NUM>] ➔ [<UNIT>]`）
     - 跨語言中英混打邊界構式（`[<ASCII_ID>] ➔ [檔/格式/介面/系統…]`）
     - 歐文多語系語法同音與重音消歧構式
     - 動補可能式否定構式（`[動作動詞] ➔ [不] ➔ [{贏, 過, 動, 到, 起, 了}]`，如「搶不贏」）
     - 正反並列選擇構式（`[V] ➔ [或] ➔ [不] ➔ [V]`，如「包含或不包含」）
     - 動詞持續貌助詞構式（`[動作動詞] ➔ [著 (ㄓㄜ˙/ㄓㄜˊ)]`，如「供奉著」）
  3. **交錯劫掠與封閉類字元防誤判**：防止「轉依個 ➔ 轉一個」（佛學罕見詞「轉依」跨界劫掠）或「搶不嬴 ➔ 搶不贏」被誤判為未登錄名詞（OOV）；補齊「在地、當地、時間軸、倒推、天龍人、純綠版」等台灣實務詞彙識別。
  4. **批次 AI 診斷防張冠李戴與防佔位符照抄**：強制將 LLM 回傳對齊至原始句對（Ground-Truth Locking），小型本地模型照抄 Schema 佔位符時自動回退至確定性符號診斷。

---

### 3. 詞庫與語意轉移資料檔（`ruhi_lexicon/data/`）
打包時請確保以下資料檔完整納入安裝包之 `data/` 目錄：
- [`ruhi_lexicon/data/transitions_user.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv)：已清洗所有跨數字偽規則（如 `待 ➔ 月`、`月 ➔ 日`），並寫入經黃金測試集驗證之正向二階轉移規則（含 `語意分析261015.txt` 與 `語意分析261016.txt` 提煉成果）。
- [`ruhi_lexicon/data/lexicon_domain.tsv`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv)：已收錄姓名拆分詞條（如「若涵」`ㄖㄨㄛˋ ㄏㄢˊ`、「庭佑」、「可恩」、「羅恩」）與領域複合詞。
- [`ruhi_lexicon/data/patterns_user.jsonl`](file:///C:/ATGprjs/ruhi/ruhi_lexicon/data/patterns_user.jsonl)：因式分解槽位構式規則庫。

---

### 4. 架構白皮書與多語系擴充指引文件（`docs/`）
- [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)：《如喜漢語計算語言學架構與 AI 語意調校白皮書》（含第五章跨輸入法字形重碼與多語系消歧規範）。
- [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md)：《如喜多語言與跨輸入法擴充架構指引（繁體中文版）》——確立「手動切換主權、漢字系統 `Tab` 鍵確定性選字、歐文系統空白鍵自然分詞且 100% 放行 `Tab` 鍵」之設計鐵律與五步擴充 SOP。
- [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md)：《RuhI Multi-Language & Input Method Expansion Guide (English Edition)》——供國際開發者參閱之英文版擴充規範。

---

## 二、打包前驗證狀態（Verification Status）

| 測試套件 | 執行指令 | 驗證結果 |
| :--- | :--- | :--- |
| **Rust 語意轉移與數詞單位構式測試** | `cargo test transitions` (`ruhi-core`) | ✅ 100% 通過 |
| **Rust 倉頡／速成重碼情境消歧測試** | `cargo test cangjie` (`ruhi-core`) | ✅ 8 項全數通過 |
| **Rust 歐文多語系同音與重音消歧測試** | `cargo test western` (`ruhi-core`) | ✅ 10 項全數通過 |
| **Python 語意調校工作室與 AI 顧問測試** | `python -m unittest discover -s tests -p "test_semantic_tuner.py" -v` | ✅ 18 項全數通過 |

---

## 三、給 `8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708` 的打包檢查清單（Packaging Checklist）

1. **編譯 Rust 核心與 TSF DLL**：
   - `cargo build --release`（編譯 `ruhi-core` 與 `ruhi-tsf`）。
2. **封裝核心資料檔（`ruhi_lexicon/data/*` ➔ 安裝目錄 `data/`）**：
   - `lexicon_core.tsv`、`lexicon_moe.tsv`、`lexicon_mcbopomofo.tsv`、`lexicon_domain.tsv`
   - `transitions_user.tsv`、`patterns_user.jsonl`
   - `symbols.tsv`、`cangjie5.tsv`（若有外掛字形表）
3. **封裝管理工具與語意調校工作室（`scripts/`）**：
   - `scripts/semantic_tuner.py`、`scripts/ai_semantic_consultant.py`、`scripts/settings_window.py`
   - 根目錄與 `scripts/` 下之 UTF-8 BOM `.ps1` 啟動器（`語意調校工作室.ps1`、`一鍵安裝.ps1`、`安裝本地AI純綠推論引擎.ps1` 等）。
4. **封裝技術文件（`docs/`）**：
   - `RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`
   - `RUHI_MULTILINGUAL_EXPANSION_GUIDE.md`
   - `RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md`
   - `RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md`
   - `TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC.md`
