# 如喜輸入法 Rust 核心引擎升級工程規格書 (`RUHI_CORE_UPGRADE_PLAN.md`)

> **文件狀態**：已全數實作完成並通過 87 項 Rust 核心測試與 85 項 Python 聯動測試驗證（Completed & Verified in `v0.2.0`）  
> **上游理論依據**：[`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)  
> **目標模組**：`C:\ATGprjs\ruhi\ruhi-core` 與 `C:\ATGprjs\ruhi\ruhi-tsf`（Rust 核心引擎與 TSF DLL 底層路由）  
> **驗證指令**：`cargo test --manifest-path C:\ATGprjs\ruhi\ruhi-core\Cargo.toml`

---

## 壹、 升級背景與核心目標

在完成「如喜語意調校工作室」之神經符號雙階段診斷與 [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md) 規範白皮書後，語意調校工作室已能自動產出並持久化：
- **Layer 0**：語意範疇槽位笛卡兒積規則（`patterns_user.jsonl`）
- **Layer 2**：二階條件轉移引力規則（`transitions_user.tsv`）
- **Layer 3**：領域複合長詞與注音讀音（`lexicon_domain.tsv`）

然而，對照 Rust 核心（`ruhi-core`）現有實作，目前存在 **四項核心架構斷層**，導致調校工作室的部分成果無法在輸入座執行期（Runtime）零重啟即時生效，或未能發揮完整的計算語言學威力。本規格書定義四項明確、可獨立測試之工程升級任務，供新對話直接依序實作並驗證。

---

## 貳、 四大工程升級任務明細

### 任務一：打通 Layer 0 —— 於 `TransitionEngine` 實作 `patterns_user.jsonl` 載入與熱重載

#### 1. 現況斷層分析
- **關聯檔案**：
  - [`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L85-L105)
  - [`ruhi-core/src/engine.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L150-L156)
- **問題根因**：
  - Python 端 [`tools/semantic_router.py`](file:///C:/ATGprjs/ruhi/tools/semantic_router.py) 已支援載入 `patterns_user.jsonl`（每行一筆 JSON：`{"prev_slot": [...], "next_slot": [...], "bonus": 28.0, "description": "..."}`）。
  - 但 Rust 端 [`TransitionEngine`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L3030-L3060) 僅實作了 `load_tsv("transitions_user.tsv")`，完全未讀取 `patterns_user.jsonl`。

#### 2. 具體實作規格
1. **擴充 `TransitionEngine` 結構體欄位**（[`transitions.rs` L85-L105](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L85-L105)）：
   - 新增 `builtin_grammar_rule_count: usize`：記錄 `init_builtin_knowledge()` 初始化完成時的內建 `grammar_rules.len()`，以便熱重載 `patterns_user.jsonl` 時僅截斷（`truncate`）外掛規則，保留內建語法規則。
   - 新增 `patterns_file_path: Option<PathBuf>` 與 `patterns_last_modified: Option<SystemTime>`。
2. **新增 `load_patterns_jsonl<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error>`**：
   - 使用 `serde_json` 解析每行 JSON 結構：
     ```rust
     #[derive(serde::Deserialize)]
     struct UserSlotPattern {
         prev_slot: Vec<String>,
         next_slot: Vec<String>,
         #[serde(default = "default_pattern_bonus")]
         bonus: f64,
         #[serde(default)]
         description: String,
     }
     ```
   - 嚴格執行安全過濾：僅保留 `bonus > 0.0`（限制於 `15.0..=35.0` 安全區間）且槽位詞元皆為非空漢字之項目。
   - 將每筆 `UserSlotPattern` 轉換為 `GrammarRule`：
     - `prev_pattern`: 若 `prev_slot.len() == 1` 則為 `WordPattern::Exact`，否則為 `WordPattern::OneOf(prev_slot)`。
     - `curr_pattern`: 若 `next_slot.len() == 1` 則為 `WordPattern::Exact`，否則為 `WordPattern::OneOf(next_slot)`。
   - 載入完畢後呼叫 `self.rebuild_rule_index()`，使所有 Layer 0 槽位規則自動納入 `rules_by_prev` 與 `rules_by_curr` $O(1)$ 雙向倒排索引。
3. **接入 `RuhiEngine::new_with_data_dir()` 與 `maybe_reload()`**：
   - 於 [`engine.rs` L150-L156](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L150-L156) 載入 `transitions_user.tsv` 後，同步檢查並載入 `p.join("patterns_user.jsonl")`。
   - 於 [`TransitionEngine::maybe_reload()`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L3063-L3086) 的 2000ms 節流檢查中，一併檢查 `patterns_file_path` 之 `mtime` 並自動熱重載。

---

### 任務二：打通 Layer 3 執行期熱重載 —— `LexiconManager` 自動增量感知 `lexicon_domain.tsv` 與 `user_dict.tsv`

#### 1. 現況斷層分析
- **關聯檔案**：
  - [`ruhi-core/src/lexicon/manager.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs#L74-L118)
  - [`ruhi-core/src/engine.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L136-L149)
- **問題根因**：
  - 當使用者在語意調校工作室修復 `Type I (OOV_COMPOUND_SHATTERED)`（未登錄複合長詞被切碎，如「打地鼠」或新興專有名詞）時，工作室會將新詞連同注音寫入 `lexicon_domain.tsv`。
  - 但 `LexiconManager` 僅在 `RuhiEngine::new_with_data_dir()` 啟動時載入一次詞庫，缺乏執行期熱重載（Hot-Reload），導致使用者必須重啟輸入法程序才能使新收錄的複合長詞生效。

#### 2. 具體實作規格
1. **於 `LexiconManager` 新增動態檔案追蹤狀態**（[`manager.rs` L74-L85](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs#L74-L85)）：
   - 定義追蹤結構：
     ```rust
     #[derive(Debug, Clone)]
     pub struct WatchedLexiconFile {
         pub path: std::path::PathBuf,
         pub category: String,
         pub last_modified: Option<std::time::SystemTime>,
         pub last_line_count: usize,
     }
     ```
   - 於 `LexiconManager` 加入 `watched_files: Vec<WatchedLexiconFile>` 與 `last_check_instant: Option<std::time::Instant>`（標註 `#[serde(skip)]` 以維持序列化相容）。
2. **實作增量熱重載方法 `maybe_reload_dynamic_lexicons(&mut self) -> usize`**：
   - 內建 2000ms 節流防護（與 `TransitionEngine::maybe_reload` 一致）。
   - 僅針對動態可變詞庫（`lexicon_domain.tsv` 與 `user_dict.tsv`）檢查 `metadata().modified()`。
   - 當發現檔案更新時，讀取檔案並對尚未存在於 `(word, bopomofo)` 索引的新增詞條呼叫 `self.add_entry(entry)`，若同一詞條權重更新則更新 `self.entries[idx].weight`，最後呼叫 `self.sort_indices()`。
3. **於 `RuhiEngine` 組字管線觸發**：
   - 在 `RuhiEngine` 每次進行 Viterbi 組字路由前（例如 `rebuild_lattice_and_solve` 或 `update_composition`），呼叫 `self.lexicon.maybe_reload_dynamic_lexicons()`，達成「工作室一鍵收錄複合長詞，輸入座零重啟立即成詞」。

---

### 任務三：接通 Layer 3 離合詞跨詞隙引力 —— 於 `ViterbiRouter` 啟用 `calculate_vo_lookback_bonus()`

#### 1. 現況斷層分析
- **關聯檔案**：
  - [`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L2932-L2977) 及 [`L3849-L3872`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L3849-L3872)
  - [`ruhi-core/src/router/viterbi.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs#L83-L105)
- **問題根因**：
  - `TransitionEngine` 已內建 35 組漢語動賓離合詞（`vo_pairs`，如 `洗...澡`、`吃...飯`、`睡...覺`、`理...髮`、`幫...忙`、`生...氣`）以及 `calculate_vo_lookback_bonus(prefix_words: &[&str], curr_word: &str) -> f64`。
  - 然而在 [`viterbi.rs` L83-L105](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs#L83-L105) 的 Trellis 狀態轉移迴圈中，僅呼叫了二元相鄰的 `calculate_transition_bonus()`，從未呼叫 `calculate_vo_lookback_bonus()`。

#### 2. 具體實作規格
1. **於 `viterbi.rs` 狀態回溯串接離合詞計分**：
   - 在 [`ViterbiRouter::solve_best_path_with_prefix()`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs#L83-L105) 評估 `prev_state -> node` 時，沿 `trellis` 的 `prev_boundary` 與 `prev_state_idx` 往前輕量回溯最多 4 個前驅詞（`max_distance = 4`），收集為栈上固定緩衝陣列（避免 Heap 動態配置開銷）：
     ```rust
     let mut lookback_buf: [&str; 4] = [""; 4];
     let mut lb_count = 0;
     // 回溯 prev_state 及更前面的 1~3 個節點
     ```
   - 將回溯詞鏈傳入 `engine.calculate_vo_lookback_bonus(&lookback_slice, &node.word)`，並將回傳的離合詞加分併入 `trans_bonus`。
2. **新增離合詞語法單元測試**：
   - 驗證「洗了一個澡」（`ㄒㄧˇ ㄌㄜ˙ ㄧˊ ㄍㄜ˙ ㄗㄠˇ`）、「幫了他一個忙」（`ㄅㄤ ㄌㄜ˙ ㄊㄚ ㄧˊ ㄍㄜ˙ ㄇㄤˊ`）、「睡個好覺」（`ㄕㄨㄟˋ ㄍㄜ˙ ㄏㄠˇ ㄐㄧㄠˋ`）在跨詞隙狀態下均能正確觸發動賓離合引力。

---

### 任務四：結構化收斂 `transitions.rs` 尾端 540 行線性分支至 $O(1)$ 雙向倒排索引

#### 1. 現況斷層分析
- **關聯檔案**：[`ruhi-core/src/router/transitions.rs` L3300-L3844](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L3300-L3844)
- **問題根因**：
  - `calculate_transition_bonus()` 後半段（第 3300 至 3844 行）累積了約 540 行手寫 `if prev_word == ... && curr_word == ...` 的線性判斷。
  - 每次 Viterbi 評估任何節點轉移時，即使與上述詞彙毫無關聯，仍必須逐一執行這數百條字串比較，繞過了 `TransitionEngine` 精心設計的 `rules_by_prev` / `rules_by_curr` 倒排索引與 `transitions` 雜湊表。

#### 2. 具體實作規格
1. **精確詞對移入內建二元轉移雜湊表或 `GrammarRule` 倒排索引**：
   - 凡屬固定雙詞搭配（`prev_word == "A" && curr_word == "B"`）之項目，直接於 `init_builtin_knowledge()` 註冊為 `GrammarRule`（或內建 `builtin_transitions: HashMap<(String, String), f64>`），透過 $O(1)$ 雜湊查找取代 $O(N)$ 線性分支。
   - 注意保留 `load_tsv()` 時對內建二元轉移的疊加或保留機制（避免 `transitions_user.tsv` 熱重載時清空內建規則）。
2. **檢視負向懲罰邊界安全性**：
   - 依據 [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md) 第一章鐵律二（嚴禁無條件過度抑制邊界錯誤），審查所有負向規則：僅保留針對「零複詞古音／冷僻假詞（如 `赭衣`、`劈刺`、`家祠`、`諱字`）」或「明確跨界搭橋假詞」之防護，絕不對現代高頻單字之間的合法跨詞邊界施加無條件負向懲罰。

### 任務五：實作階層式集合槽位構式引擎 —— 64-bit 語素位元遮罩歸約 `[( {A} → {B} )]` 與詞級三階夾擊轉移

#### 1. 架構目標與原理
- 為徹底根除「把 `圈選框`、`每點擊` 等 `[2+1]` 組合當三字死詞塞入詞庫」之組合爆炸，同時避免平面單字高階 $n$-gram 的狀態空間爆炸，於 `ruhi-core` 引入 **雙層階層式集合槽位構式引擎（Hierarchical Slot-Set Construction Engine）**：
  $$\text{[外層前驅槽 } W_{i-2}\text{]} \longrightarrow \text{[目標候選詞 } W_{i-1}\text{]} \longrightarrow \big[\big(\{\text{語素集 }\mathcal{A}\} \longrightarrow \{\text{語素集 }\mathcal{B}\}\big)\big]$$
- 典型範例：
  1. `[每] → [點擊] → [({一, 二, 三, 四, 五, 六, 七, 八, 九, 十, 兩, 幾…} → {次, 回, 下, 遍, 趟})]`（遍指動量三階夾擊，精準鎖定動詞「點擊」且不誤傷「每部典籍」）。
  2. `[({圈, 點, 勾, 框, 篩, 挑, 複, 全, 單, 候} → {選, 取})] → [{框, 欄, 區, 鈕, 鍵}]`（內層合成選取動詞節點，外層引導單字容器中心語「框」，零三字死詞入庫）。
  3. `[{在, 於, 從, 往}] → [框] → [({中, 內, 裡, 外, 上, 下} → {的, 面, 邊, ∅})]`（框式空間介詞三階夾擊）。
  4. `[{會, 將, 要, 能, 可, 先, 請}] → [再] → [動作謂詞]`（時態助動詞＋重複副詞＋動詞三階向右視野分流）。

#### 2. 具體實作規格
1. **定義 64-bit 語素類別位元遮罩（`MorphemeMask`）**：
   - 於 [`ruhi-core/src/router/transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs) 新增常數與 O(1) 字元遮罩查詢函式 `char_morpheme_mask(ch: char) -> u64`：
     ```rust
     pub const MASK_FREQ_DET: u64    = 1 << 0; // 每, 各, 凡, 逢
     pub const MASK_NUMERAL: u64     = 1 << 1; // 一..十, 百, 千, 萬, 兩, 幾, 多, 數, 半
     pub const MASK_VERBAL_CL: u64   = 1 << 2; // 次, 回, 下, 遍, 趟, 番, 場, 陣, 聲
     pub const MASK_BOOK_CL: u64     = 1 << 3; // 部, 本, 冊, 卷, 套, 篇, 章
     pub const MASK_SELECT_ROOT: u64 = 1 << 4; // 圈, 點, 勾, 框, 篩, 挑, 複, 全, 單, 候
     pub const MASK_SELECT_ACT: u64  = 1 << 5; // 選, 取, 列
     pub const MASK_UI_HEAD: u64     = 1 << 6; // 框, 欄, 區, 鈕, 鍵, 窗, 頁, 表, 列, 項
     pub const MASK_PREP_LOC: u64    = 1 << 7; // 在, 於, 從, 往, 向, 自
     pub const MASK_POST_LOC: u64    = 1 << 8; // 中, 內, 裡, 外, 上, 下, 前, 後, 旁, 間
     pub const MASK_MODAL_AUX: u64   = 1 << 9; // 會, 將, 要, 能, 可, 先, 請
     pub const MASK_DEM_DET: u64     = 1 << 10; // 此, 這, 該, 那, 外, 邊, 方
     ```
2. **內層二元語素槽位歸約函式 `classify_composite_slot(word: &str) -> u64`**：
   - 對長度為 2 之詞元（或相鄰雙字），以單條位元與指令判定其合成槽位標籤（耗時 $< 1\text{ ns}$）：
     - `(m0 & MASK_NUMERAL != 0) && (m1 & MASK_VERBAL_CL != 0)` $\implies$ `SLOT_TAG_VERBAL_QP`（動量短語 `[(數→動量)]`）
     - `(m0 & MASK_NUMERAL != 0) && (m1 & MASK_BOOK_CL != 0)` $\implies$ `SLOT_TAG_BOOK_QP`（書籍名量短語 `[(數→名量)]`）
     - `(m0 & MASK_SELECT_ROOT != 0) && (m1 & MASK_SELECT_ACT != 0)` $\implies$ `SLOT_TAG_SELECT_VERB`（選取動詞 `[(X→選)]`）
3. **外層詞級三階夾擊計分 `calculate_trigram_construction_bonus(prev2_word: &str, prev1_word: &str, curr_word: &str) -> f64`**：
   - 於 [`viterbi.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs) 評估 `prev_state -> node` 時，直接讀取 `prev_state` 的前驅節點 `prev2_word`（已在任務三回溯緩衝中取得，零額外記憶體配置），呼叫 `calculate_trigram_construction_bonus(prev2_word, &prev_state.word, &node.word)`：
     - 當 `prev2_word` 為 `MASK_FREQ_DET`（如「每」）、`prev1_word == "點擊"`、`curr_word` 為 `SLOT_TAG_VERBAL_QP`（如「一次、兩下、三回」）時，賦予 `+32.0` 三階夾擊加成！
     - 當 `prev2_word` 為 `MASK_PREP_LOC`（如「在」）、`prev1_word == "框"`、`curr_word` 首字為 `MASK_POST_LOC`（如「中、中の、內、裡」）時，賦予 `+32.0` 框式介詞夾擊加成！
     - 當 `prev1_word` 為 `SLOT_TAG_SELECT_VERB`（如「圈選、點選、勾選、框選」）、`curr_word` 為 `MASK_UI_HEAD`（如「框、欄、區」）時，賦予 `+30.0` 構式加成！

### 任務六：數詞與曆法／貨幣／度量衡構式及中文實體（姓名／公司全稱）智慧拆分

1. **數詞＋量詞／單位構式（`Numeric-Unit Classifier Construction`）**：
   - 於 [`transitions.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs) 擴充 `WordPattern::is_numeric_token`，支援阿拉伯數字、漢字數詞與複合數詞（排除獨立出現會干擾「參數」、「設計」之單字大寫金額字如 `參`、`拾`），自動引導後續曆法（`年/月/日/號`）、貨幣（`元/圓/塊/角`）、度量衡與時間單位。
2. **中文姓名與組織實體智慧拆分（`LexiconManager::decompose_entity_subentries`）**：
   - 於 [`lexicon/manager.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs) 與 [`user_word_dialog.rs`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/user_word_dialog.rs) 實作三字／四字複姓姓名（如「吳玟蓉」➔「玟蓉」、「歐陽娜娜」➔「歐陽」＋「娜娜」）與公司機構全稱（如「如喜科技股份有限公司」➔「如喜科技」＋「如喜」）之衍生詞自動拆解與轉移引力寫入。

---

### 任務七：歐漢雙軌同台組字、`↓`／`↑` 智慧查詞與救援、及亞洲輸入法雙向切換架構

1. **歐文軌智慧升級（`ruhi-core/src/western/`）**：
   - **`↓` 下箭頭（及 `Q` 鍵）同形異體與重音變體查詢**：支援外來語整詞重音變體（`cafe` ➔ `café`、`resume` ➔ `résumé`、`naive` ➔ `naïve`）與單字母變音符號展開（`a` ➔ `á, à, â, ä, ã, å, ā, æ` 等）。
   - **`↑` 上箭頭縮寫展開與拼寫救援**：支援高頻縮寫展開（`tks` ➔ `thanks, thank you`、`pls` ➔ `please`、`btw` ➔ `by the way`）、漏撇號還原（`dont` ➔ `don't`、`im` ➔ `I'm`）、子音骨架匹配（`cmptr` ➔ `computer`）、編輯距離校正與跨軌救援。
2. **亞洲軌 `↑` 上箭頭上下文模糊音推理救援（`LexiconManager::lookup_contextual_fuzzy_rescue`）**：
   - 支援跨聲調與台灣常見近音模糊音（`ㄓ↔ㄗ`、`ㄔ↔ㄘ`、`ㄕ↔ㄙ`、`ㄢ↔ㄤ`、`ㄣ↔ㄥ`、`ㄌ↔ㄋ/ㄖ`、`ㄈ↔ㄏ`、`ㄜ↔ㄦ`）雙字成詞救援（如「答按」➔「答案」）及前後文語法搭配推論（如「可能地答案」➔「可能的答案」）。
3. **亞洲輸入法雙向切換（滑鼠點擊狀態匣 ＆ `Ctrl/Shift + Caps Lock`）**：
   - 於 [`candidate_window.rs`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/candidate_window.rs) 實作模式指示區點擊彈出選單（切換許氏雙拼、標準大千、倚天 26 鍵、倉頡、英數模式）。
   - 於 [`service.rs`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) 攔截 `Ctrl + Caps Lock` / `Shift + Caps Lock` 循環切換亞洲輸入法，並透過 `SUPPRESS_SYNTHETIC_CAPS` 對稱合成鍵脈衝中和 Windows 底層 `Caps Lock` 燈號反轉。

---

## 參、 測試先行（TDD）與防回歸驗證規範

在新對話執行上述升級時，必須嚴格遵循以下驗證流程：

1. **升級前基準測試**：
   ```powershell
   cargo test --manifest-path C:\ATGprjs\ruhi\ruhi-core\Cargo.toml
   ```
   確認現有全數單元測試 100% 通過，記錄執行耗時。

2. **新增專屬單元測試（於 `ruhi-core/src/engine.rs`、`ruhi-core/src/western/mod.rs` 與 `ruhi-core/src/router/transitions.rs`）**：
   - `test_layer0_patterns_user_jsonl_loading_and_matching`：驗證 `patterns_user.jsonl` 載入後，槽位笛卡兒積在 Viterbi 解碼中即時生效。
   - `test_layer3_dynamic_lexicon_hot_reload`：驗證執行期寫入新複合長詞至暫存 `lexicon_domain.tsv` 後，不重啟 `RuhiEngine` 即可直接組出該複合長詞。
   - `test_layer3_vo_separable_verb_lookback_in_viterbi`：驗證跨詞隙動賓離合詞在 `ViterbiRouter` 中獲得正確回溯加分。
   - `test_indexed_transitions_zero_regression`：驗證 `transitions.rs` 尾端規則結構化索引後，所有歷史語料測試集 100% 零退化通過，且長句解碼延遲進一步降低。
   - `test_hierarchical_slot_set_and_trigram_constructions`：驗證 `每點擊一次 / 每點擊三下`、`圈選框 / 點選框 / 勾選框`（無三字死詞入庫）、`在框中的`、`每部典籍` 均 100% 正確組字且單句解碼耗時 $< 1\text{ ms}$。
   - `test_numeric_date_currency_measure_time_units` ＆ `test_entity_decomposition_and_incremental_hot_reload`：驗證數詞量詞構式與實體智慧拆分。
   - `test_western_down_arrow_diacritics_and_loanwords`、`test_western_up_arrow_expansion_and_spell_rescue` ＆ `test_dual_track_arrow_lookup_and_contextual_rescue`：驗證歐漢雙軌 `↓`／`↑` 智慧查詞、縮寫展開、上下文模糊推理與輸入法循環切換。

3. **Python 端語意調校工作室聯動測試**：
   ```powershell
   python -m unittest discover -s C:\ATGprjs\ruhi\tests -p "test_semantic_tuner.py" -v
   ```
   確保工作室產出的 `patterns_user.jsonl`、`transitions_user.tsv` 與 `lexicon_domain.tsv` 與升級後的 `ruhi-core` 100% 規格相容。
