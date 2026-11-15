# 如喜（RuhI）開源多語種與鍵盤模式宣告式擴充架構規範

> **設計宗旨**：隨現有十大鍵盤模式（大千標準注音、許氏注音、倚天 26 鍵、漢語拼音、倉頡、行列 30、四角號碼、日語羅馬字、韓語字母、泛歐語軌道）底層引擎架構收斂穩定，如喜朝向**「不更動核心程式碼、純以宣告式資料包開源擴充全球語種與鍵盤配置」**之插件化工程目標演進，讓各語種開發人員均能依循一致規範製作相容語言包，並使一般使用者僅透過匯入設定檔與詞庫表即可擴增新語言與新鍵盤支援。

---

## 一、核心設計原則（Core Architectural Principles）

1. **核心執行引擎與語言資料解耦（Engine–Data Decoupling）**：
   - 搜尋演算法（Viterbi 動態規劃、Damerau-Levenshtein 編輯距離、音韻倒排檢索）保留於核心原生程式庫（`ruhi-core`）；
   - 按鍵對映、音節拼寫合法性、詞彙權重、構式搭配與跨語音韻近音規則則抽離為宣告式資料表（TSV／TOML／JSON），以執行期零編譯方式熱載入。
2. **外國人友善之通用音韻近似哲學（L2-Friendly Phonological Approximation）**：
   - 不假設使用者完全熟記目標外語的歷史正字法（如英語 `ph`/`kn`/`ps`/`-tion`、法語 `-eau`/`-coup`），而是預設使用者僅依聽覺印象輸入近似拼音，由**表驅動圖位至音位規格（`PhoneticProfile`）**即時完成近似補全、勘誤與語境選詞。
3. **跨全模式統一之三鍵互動契約（Unified Three-Key Interaction Contract）**：
   - 任何現有與未來擴充模式，於 Windows TSF 輸入座架構中皆嚴格遵守一致的操作行為：
     - **`Tab`**：組字階段之智慧聯想、前綴／音韻近似補全、自訂詞庫／範本展開；
     - **`↓`**：同音選字、泛歐變音字母／外來語重音變體轉換；
     - **`Space`**：確認上詞與非破壞性精準自動勘誤（支援單鍵 `Backspace` 還原並釘選原詞）。
4. **COM ABI 與熱重載記憶體邊界防護**：
   - 所有跨模式共享或可選用之外掛索引（如會話記憶與 `WesternPhoneticIndex`）一律掛載於核心狀態指標之外（透過 `Arc` 指標識別之唯讀靜態側表），嚴格維持 `RuhiTextService`（`27 * size_of::<usize>()`）與 `WesternEngine` 既有二進位佈局，保障執行期熱切換穩定性。
5. **單一鍵盤模式獨立掛載與反斜線字面轉義契約（Single-Mode Decoupling & Backslash Escape Contract）**：
   - **去單核化獨立掛載**：架構上絕不將任何單一輸入法（如許氏注音 `"hsu"`）寫死為不可卸載的核心依賴；使用者得僅單獨掛載任一鍵盤配置（例如倉頡、行列 30、日語羅馬字、韓語諺文、大千標準注音、倚天 26 鍵、四角號碼或純歐文），設定中心與工作列切換選單皆嚴格收斂至已掛載清單（僅於掛載兩種以上模式時自動納入 `english` 以利雙語輪替），並提供 `capslock_switch = "none"` 選項還原作業系統原生 Caps Lock 大寫鎖定行為。
   - **跨全 10 模式統一反斜線字面轉義**：為兼顧軟體工程與 Markdown 寫作需求而不須跳離如喜輸入法，全數 10 種模式一致支援 `\`` ➔ 半形 `` ` ``（不觸發 `🔖 自訂詞庫`）、`\\` ➔ 半形 `\`，以及 `\~` ➔ 半形 `~`（不觸發 `tilde_toggle_mode`）；一般反斜線序列如 `\path` 則完整保留原字面輸出。

---

## 二、四層宣告式語言擴充套件規格（Four-Layer Extension Bundle Layout）

未來單一獨立語言／鍵盤擴充套件（例如 `ruhi-lang-fr-azerty/` 或 `ruhi-lang-vi-telex/`）建議依下列標準目錄組織：

```text
extensions/<lang_tag>-<layout_id>/
├── manifest.toml        # 套件識別、BCP-47 語系碼、輸入架構類型與相容版本
├── keymap.toml          # 第一層：按鍵至音素／字根轉導規則（Keystroke Transducer）
├── lexicon.tsv          # 第二層：正規詞庫表（word \t freq \t weight）
├── morphology.toml      # 第二層：屈折變化、變音同位字與縮寫展開規則
├── phonetics.toml       # 第三層：宣告式音韻近似與 L2 易混音素距離規格
└── patterns.jsonl       # 第四層：雙向語境構式與高信心自動勘誤白名單
```

### 第一層：按鍵至語音／字根轉換層（Keystroke-to-Unit Transducer）
- **對應核心模組**：[`fsm/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/fsm/mod.rs)、[`cangjie/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/cangjie/mod.rs)、[`array30/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/array30/mod.rs)、[`four_corner/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/four_corner/mod.rs)、[`japanese/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/japanese/mod.rs)、[`korean/`](file:///c:/ATGprjs/ruhi/ruhi-core/src/korean/mod.rs)、[`western/fsm.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/fsm.rs)。
- **三種統一有限狀態機原型（FSM Archetypes）**：
  1. **音節拼合型（Syllabic Combinator）**：適用於注音、漢語拼音、諺文音節方塊（Jamo）與假名（Romaji ➔ Kana），定義「聲／韻／調」狀態轉移矩陣與合法音節集合。
  2. **字根碼表型（Code-Table Shape）**：適用於倉頡、行列 30、四角號碼與未來五筆／嘸蝦米等形碼，純以 `key_seq \t char \t weight` 對映並自動繼承「組字中段 `Tab` 前綴補全」機制。
  3. **詞元分詞型（Tokenizing Lexeme FSM）**：適用於全體歐美／拉丁／西里爾拼音文字，保留 CamelCase／snake_case／URL 等開發者符號直通能力。

### 第二層：詞彙與屈折形態層（Lexicon & Morphology Pack）
- **對應核心模組**：[`western/lexicon.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs)、[`western/morphology.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/morphology.rs)、[`lexicon/manager.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs)。
- **標準格式鐵律**：
  - `lexicon.tsv` 一律維持三欄格式：`詞條 \t 詞頻 \t 對數正規化權重 (0.0 ~ 100.0)`。
  - **嚴禁因單句偏差手動微調單字無上下文基礎權重**；複合詞應採詞幹因式分解，屈折變化交由形態學詞綴規則即時推導，並與基準詞幹維持一致之分數縮放比例。

### 第三層：表驅動音韻近似層（Declarative Phonological Approximation Profile）
- **對應核心模組**：[`western/phonetics.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/phonetics.rs)。
- 目前已於核心抽離為**純宣告式規格結構體**：
  - `PhoneticProfile`：定義語種音韻設定檔名稱與字首靜音字頭（`silent_h_prefixes`）；
  - `G2pClusterRule`（`G2pPosition::{Onset, WordFinal, Anywhere}`）：宣告多字母拼寫群組至粗粒度音素骨架之轉換及其次要歧義讀音（例如希臘詞根 `ch` 可映射 `X`（`/tʃ/`）或 `K`（`/k/`）；`sch` 可映射 `SK` 或 `X`）；
  - `PhoneticConfusionPair`：宣告非母語學習者（如華語／日語／韓語使用者學習歐語）之跨音位替換容錯代價（例如 `L↔R`、`V↔W`、`F↔V`、`P↔B`、`T↔D`、`K↔G`）。
- 未來其他語種（如西班牙語、德語、越南語或印度拉丁轉寫）開發者僅需填寫 `G2pClusterRule` 與 `PhoneticConfusionPair` 清單，即可直接獲得 `O(1)` 子音骨架倒排索引、`2..=6` 音素長度前綴補全（如 `filos ➔ philosophy`）與加權音韻距離排序，無須重寫任何倒排檢索程式碼。

### 第四層：語境句法與互動統一協定層（Contextual Scorer & Unified UX Contract）
- **對應核心模組**：[`western/collocations.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/collocations.rs)、[`router/constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs)。
- **句法構式介面原則**：
  - 所有語境加／減分規則均不得依賴局部字串硬編碼片段，一律透過正交句法槽位（例如不定詞 `InfinitivalTo`、進行式 `BeProgressive`、定義縮略語槽位 `AcronymContext`、全稱量化副詞主謂框架）或明確詞性邊界進行雙向保護；
  - 評分規則幅度遵守靜態檢查上限（`scripts/lint_scoring_rules.py`：`|\Delta| <= 35.0`），確保不同語言包與擴充規則並存時不產生局部權重壟斷。

---

## 三、外部貢獻與品質驗證閘門（Contributor Verification Checklist）

全球開發人員提交新語種擴充套件或音韻規則時，應一併隨附三類可重複驗證資料並通過如喜測試框架：
1. **靜態完整性檢查**：通過詞庫無重複衝突詞頻檢驗與 `lint_scoring_rules.py` 分數邊界規範。
2. **正反對比迴歸測試（Positive + Contrastive Pairs）**：每一條語境規則或音韻近似救援規則，必須同時具備「目標錯拼／近似補全命中」與「既有正字法／對比語境不受破壞」兩組斷言。
3. **非破壞性防護驗收**：驗證程式碼識別字（camelCase、snake_case）、網址路徑與全大寫縮寫不被空白鍵自動勘誤誤改，且不破壞既有鍵盤模式之吞吐延遲目標。
