# 如喜（RuhI）四層宣告式語言擴充套件規範與目錄說明

本目錄為如喜（RuhI）多語種與鍵盤配置擴充套件之標準掛載根目錄。依據 [EXTENSION_ARCHITECTURE_SPEC.md](../../../docs/EXTENSION_ARCHITECTURE_SPEC.md) 與 `ruhi-core` 擴充套件載入器（[`extensions.rs`](../../../ruhi-core/src/extensions.rs)）之實作規格，每組語言或鍵盤擴充套件均為獨立資料夾，透過 `manifest.toml` 與四層宣告式資料檔達成核心引擎與語言資料的完整解耦。

---

## 一、標準目錄結構（Four-Layer Extension Bundle Layout）

每個擴充套件建議以 `<lang_tag>-<layout_id>` 命名子目錄，置於 `extensions/` 目錄下：

```text
extensions/
├── README.md
└── <lang_tag>-<layout_id>/
    ├── manifest.toml        # 套件中繼資料、BCP-47 語系碼、FSM 原型與各層資產路徑
    ├── keymap.toml          # 第一層：按鍵至音素／字根轉導規則（Keystroke Transducer）
    ├── lexicon.tsv          # 第二層：正規詞庫表（詞條 \t 詞頻 \t 對數正規化權重）
    ├── morphology.toml      # 第二層：屈折變化、變音同位字與縮寫展開規則
    ├── phonetics.toml       # 第三層：宣告式音韻近似與外語學習者易混音素距離規格
    └── patterns.jsonl       # 第四層：雙向語境構式與高信心自動勘誤白名單
```

### 各層職責摘要

1. **第一層：按鍵至語音／字根轉換層（`keymap.toml`）**
   - 透過 `manifest.toml` 中的 `archetype` 指定有限狀態機（FSM）原型：
     - `syllabic_combinator`（音節拼合型）：適用於注音、漢語拼音、韓語諺文（Jamo）與日語假名轉換，定義聲母、韻母、聲調轉移與合法音節集合。
     - `code_table_shape`（字根碼表型）：適用於倉頡、行列 30、四角號碼、五筆等形碼，定義按鍵序列對映並支援組字中段 `Tab` 前綴補全。
     - `tokenizing_lexeme`（詞元分詞型）：適用於歐美、拉丁、西里爾與東南亞拼音文字，保留 `camelCase`、`snake_case` 與網址路徑等識別字直通能力。
2. **第二層：詞彙與屈折形態層（`lexicon.tsv`、`morphology.toml`）**
   - `lexicon.tsv` 嚴格採用三欄 Tab 分隔格式：`詞條 \t 詞頻 \t 對數正規化權重 (0.0 ~ 100.0)`。
   - 複合詞須透過詞幹與詞綴規則進行因式分解，禁止為修正單一句型偏差而手動調整無上下文基礎單字權重。
3. **第三層：表驅動音韻近似層（`phonetics.toml`）**
   - 宣告圖位至音位群組轉換規則（`G2pClusterRule`）、字首靜音字頭（`silent_h_prefixes`）與跨音位替換容錯代價（`PhoneticConfusionPair`），提供零編譯之 `O(1)` 子音骨架倒排檢索與前綴近似補全。
4. **第四層：語境句法與互動契約層（`patterns.jsonl`）**
   - 定義具明確正交句法槽位邊界之雙向語境加減分規則與高信心自動勘誤白名單，單條評分幅度須維持在 `|Δ| <= 35.0` 上限內。

---

## 二、`manifest.toml` 欄位與安全性規範

### 欄位對照表

| 欄位 / 區段 | 必填 | 型別 | 說明 |
|:---|:---:|:---|:---|
| `id` | 是 | 字串 | 套件唯一識別碼，僅限 ASCII 字母、數字、`-`、`_`、`.`，不得含 `..` 或路徑分隔符號 |
| `name` | 是 | 字串 | 套件顯示名稱 |
| `version` | 是 | 字串 | 語意化版本號（例如 `"0.1.0"`） |
| `language_code`（或 `bcp47`） | 是 | 字串 | BCP-47 語系代碼（例如 `"fr-FR"`、`"vi-VN"`、`"zh-Hant"`） |
| `archetype` | 是 | 字串 | `syllabic_combinator`、`code_table_shape` 或 `tokenizing_lexeme` |
| `author` / `description` / `license` | 否 | 字串 | 作者、摘要說明與授權條款（如 `"Apache-2.0"`） |
| `min_core_version` | 否 | 字串 | 最低相容之 `ruhi-core` 版本 |
| `tags` | 否 | 字串陣列 | 分類標籤清單，例如 `["western", "azerty"]` |
| `[layers]` 區段（或根層級鍵） | 否 | 相對路徑字串 | `keymap`、`lexicon`、`morphology`、`phonetics`、`patterns` |
| `[ui]` 區段 | 否 | 混合鍵值 | 介面提示（如 `indicator`、`display_name`、`capslock_switch = "none"`） |
| `[fsm]` 區段 | 否 | 混合鍵值 | 狀態機提示（如 `max_buffer_len`、`tab_prefix_completion`、`passthrough_identifiers`） |

### 路徑安全與靜態檢核契約

- 所有宣告於 `manifest.toml` 之四層資產路徑，**僅允許使用相對於該套件目錄內部的相對路徑**（例如 `"keymap.toml"` 或 `"data/lexicon.tsv"`）。
- 載入器會主動攔截並拒絕任何包含 `..` 上層目錄跳脫、絕對路徑（`/`、`\` 或 Windows 磁碟機代號）及符號連結越界之設定。
- 若明確宣告了某層檔案路徑，該檔案必須實際存在於套件目錄中；未宣告 `keymap` 或四層資料檔完全缺席時，載入器會回報結構化診斷警告。

---

## 三、`manifest.toml` 範例

### 範例一：詞元分詞型（`tokenizing_lexeme`）— 法語 AZERTY

```toml
id = "fr-azerty"
name = "Français AZERTY"
version = "0.1.0"
language_code = "fr-FR"
archetype = "tokenizing_lexeme"
author = "RuhOS Community"
description = "法語 AZERTY 鍵盤配置與拉丁變音近似詞彙套件"
license = "Apache-2.0"
min_core_version = "0.9.6"
tags = ["latin", "azerty", "l2-phonetics"]

[layers]
keymap = "keymap.toml"
lexicon = "lexicon.tsv"
morphology = "morphology.toml"
phonetics = "phonetics.toml"
patterns = "patterns.jsonl"

[ui]
indicator = "FR"
display_name = "法語 AZERTY"
capslock_switch = "none"

[fsm]
max_buffer_len = 64
tab_prefix_completion = true
passthrough_identifiers = true
```

### 範例二：音節拼合型（`syllabic_combinator`）— 越南語 Telex

```toml
id = "vi-telex"
name = "Tiếng Việt Telex"
version = "0.1.0"
bcp47 = "vi-VN"
archetype = "syllabic_combinator"
author = "RuhOS Community"
license = "Apache-2.0"

[layers]
keymap = "keymap.toml"
lexicon = "lexicon.tsv"
phonetics = "phonetics.toml"

[ui]
indicator = "VI"
display_name = "越南語 Telex"

[fsm]
max_buffer_len = 32
tab_prefix_completion = true
```

### 範例三：字根碼表型（`code_table_shape`）— 通用形碼擴充

```toml
id = "wubi-98"
name = "五筆 98 字根碼表"
version = "0.1.0"
language_code = "zh-Hant"
archetype = "code_table_shape"
license = "Apache-2.0"

[layers]
keymap = "keymap.toml"
lexicon = "lexicon.tsv"

[ui]
indicator = "五"
display_name = "五筆 98"

[fsm]
tab_prefix_completion = true
```
