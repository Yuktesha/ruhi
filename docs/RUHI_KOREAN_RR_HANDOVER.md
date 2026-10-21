# 如喜（RuhI）外國人友善韓文輸入法 — 開發交接與分階段實作指南

> **文件目的**：交由接手的 Agent 依序實作。每個階段都附有交付物、驗收指令與停止條件，請**按順序執行，不要跳階段**。
> **撰寫日期**：2026-10-21
> **前置設計稿**：本文件取代先前的對話內計畫草稿，所有設計決策以本文件為準。

> [!IMPORTANT]
> **2026-10-22 更新**：P0～P3 已完成並通過審查。**接手者請直接從文末「十三、P4 起續做指引」開始**，該節的規定優先於第七節（P4）與第八節（P5）的原文。

---

## 〇、產品目標與設計決策（已定案）

### 產品定位
讓**完全不懂韓文、也不會韓文鍵盤配置**的使用者，依照路牌、旅遊書、教科書上的**標準羅馬拼音（Revised Romanization of Korean，2000 年韓國文化觀光部公告，以下簡稱 RR）**鍵入拉丁字母，引擎即時組成正確的諺文。

| 使用者鍵入 | 預期輸出 | 說明 |
| :--- | :--- | :--- |
| `annyeonghaseyo` | `안녕하세요` | 純規則即可組出 |
| `hangeul` | `한글` | 純規則即可組出，`↓` 另列 `항을` |
| `kimchi` | `김치` | 舊式拼法，需要常用詞表 |
| `gamsahamnida` | `감사합니다` | 依發音拼寫（鼻音化），需要常用詞表 |
| `tteokbokki` | `떡볶이` | 依發音拼寫，需要常用詞表；純規則只會組出 `떡복기` |

### 已定案的決策
1. **輸入方式**：採用 RR 羅馬拼音。既有文件中規劃的「兩鍵式（Dubeolsik）」保留為未來選項，本次不實作，但代數合成層（`jamo.rs`）必須設計成兩者可以共用。
2. **範圍**：從核心模組一路做到輸入法前端（TSF）與設定中心，共分 7 個階段（見下表）。
3. **常用詞表**：要做，第一版收錄約 300～500 條，用途是排序，而不是限制輸入。不在詞表中的詞仍然可以靠規則組出。
4. **模式代碼**：`korean`，別名 `ko`、`hangul`、`romaja`，徽章 `韓`。

### 階段總覽

| 階段 | 內容 | 主要檔案 | 必須停下回報？ |
| :--- | :--- | :--- | :--- |
| P0 | 環境確認與基準測試 | — | 否 |
| P1 | 字母對照表與 Unicode 代數層 | `korean/jamo.rs` | 否 |
| P2 | 音節切分動態規劃（N-best） | `korean/romaja.rs` | 否 |
| P3 | `KoreanEngine` 狀態機 | `korean/mod.rs` | **是**（核心完成點） |
| P4 | 常用詞表與學習者容錯 | `korean/lexicon.rs`、`data/korean.tsv` | 否 |
| P5 | 接入 `RuhiEngine` | `engine.rs` | **是** |
| P6 | 接入 TSF 前端與設定中心 | `ruhi-tsf/src/*.rs`、設定 UI | **是**（需使用者手動驗證） |
| P7 | 文件與變更紀錄 | `docs/`、`CHANGELOG.md` | 否 |

---

## 一、接手 Agent 必讀守則（違反任何一條即視為交付失敗）

1. **先讀 [`C:\ATGprjs\AGENTS.md`](file:///C:/ATGprjs/AGENTS.md)**，尤其是以下幾節：
   - **嚴禁以背景方式啟動 GUI**：不得用 `IsDaemon: true` 啟動任何 GUI 程式，也不得聲稱「已在背景啟動」。
   - **嚴禁單字權重打地鼠**：韓文詞表的排序加分必須是**統一公式**，不得為了讓某個測試通過而單獨調高某一個詞的權重。
   - **新建或修改的 `.ps1` 必須存成 UTF-8 with BOM**，交付前必須做 PowerShell 5.1 語法檢查。
   - **用語**：一律使用台灣正體中文資訊用語（程式碼、預設、字串、記憶體……），禁用浮誇用詞。
2. **不得修改與韓文無關的模組**，尤其是 `router/transitions.rs`、`router/constructions.rs`、`lexicon/manager.rs`、`japanese/`、`western/`。
3. **測試先行**：每個階段先寫測試再寫實作，`cargo test` 必須 100% 通過才能進入下一階段。**每個階段都要跑全套測試**，確認中文、日文、英文的既有測試沒有退化。
4. **行號只供參考**：`engine.rs` 有六千多行，本文件列出的行號可能已經偏移，請用 `Select-String` 或 codebase-memory-mcp 的 `search_graph` 重新定位。
5. **版本控制**：開工前執行 `git status` 確認工作區乾淨，並建立分支 `feature/korean-rr`。每個階段完成後提交一次，提交訊息格式為 `feat(korean): P<n> <摘要>`。**不要推送到遠端**，除非使用者要求。
6. **遇到本文件沒有涵蓋的設計歧義，請停下來詢問使用者**，不要自行擴大範圍。

---

## 二、背景知識：接手者需要理解的韓文結構（不懂韓文也能實作）

### 1. 諺文音節是代數組合
每個諺文音節 = **初聲 L**（子音，可以是無聲的 ㅇ）+ **中聲 V**（母音，必有）+ **終聲 T**（子音，可省略）。Unicode 依固定公式排列全部 11,172 個音節（19 × 21 × 28）：

$$\text{Syllable} = \text{0xAC00} + (L \times 21 + V) \times 28 + T$$

驗算：`한` 的 L=18（ㅎ）、V=0（ㅏ）、T=4（ㄴ），代入得 0xAC00 + (378 × 28) + 4 = **U+D55C** ✓。

### 2. RR 反向組字的四類歧義

| 類型 | 例子 | 本專案的處理方式 |
| :--- | :--- | :--- |
| ① 音節切分 | `hangeul` = `han-geul`（한글）或 `hang-eul`（항을） | 動態規劃取最低成本；`-` 可強制切分 |
| ② 母音連寫 | `ae` = ㅐ 或 ㅏ+ㅔ；`eo` = ㅓ 或 ㅔ+ㅗ | 音節數越少成本越低，自然偏好複合母音；`a-e` 可強制拆開 |
| ③ 終聲依發音拼寫 | RR 把 ㄷㅅㅆㅈㅊㅌㅎ 終聲都寫成 `t`，把 ㄱㄲㅋ 都寫成 `k` | 主要候選取最常見者，其餘放進 `↓`；也接受明確的轉寫字母（如終聲 `ss` = ㅆ） |
| ④ 音變 | 감사**합**니다 讀作 `gamsa`**`ham`**`nida` | 交由常用詞表（P4）處理 |

### 3. 韓文以空白分詞
與日文不同，韓文詞與詞之間用空白分隔。因此**按空白鍵就直接上屏當前的詞**，不需要跨詞做句子層級的 Viterbi 運算。這點對應 [RUHI_MULTILINGUAL_EXPANSION_GUIDE.md](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md) 中的 Track III（以空白分詞之拼音文字）：空白鍵直接上屏，`Tab` 一律放行不攔截。

---

## 三、P0：環境確認與基準測試

```powershell
cd C:\ATGprjs\ruhi
git status
git checkout -b feature/korean-rr
cd ruhi-core
cargo test 2>&1 | Select-Object -Last 30
```

- 記錄基準結果（通過數、失敗數）。**若原本就有失敗的測試，請把名稱列出來回報**，之後不得把這些失敗算到韓文模組頭上，也不得順手去修。
- 閱讀以下兩個參考實作，後續的韓文模組要比照它們的介面與行為慣例：
  - [`japanese/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/japanese/mod.rs)：重點是 `convert_romaji_stream`（每次按鍵都重新解析整串輸入）、`process_key_str`、`make_preedit`、`is_english_forced`、`load_builtin_lexicon`。
  - [`engine.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 中的 `EngineResponse` 結構，以及日文分支如何分派（搜尋 `self.japanese.`）。

---

## 四、P1：字母對照表與 Unicode 代數層

### 新增檔案
- `ruhi-core/src/korean/mod.rs`：本階段只放 `pub mod jamo;`
- `ruhi-core/src/korean/jamo.rs`
- 修改 `ruhi-core/src/lib.rs`：加入 `pub mod korean;`（排在 `japanese` 之後）

### `jamo.rs` 規格

```rust
/// 單一字母規格
pub struct JamoSpec {
    /// 相容字母（Hangul Compatibility Jamo, U+3131..U+318E），供未完成音節預覽
    pub compat: char,
    /// RR 標準拼寫（成本 0）
    pub rr: &'static str,
    /// 替代拼寫與附加成本
    pub alts: &'static [(&'static str, u8)],
}
```

**初聲表 `CHOSEONG: [JamoSpec; 19]`**（順序必須與 Unicode 一致）：

| L | 字母 | RR | 替代拼寫（成本） | 說明 |
|:-:|:-:|:-:|:--|:--|
| 0 | ㄱ | g | k(3) | 舊式拼法 `kimchi` |
| 1 | ㄲ | kk | gg(1) | |
| 2 | ㄴ | n | | |
| 3 | ㄷ | d | t(3) | |
| 4 | ㄸ | tt | dd(1) | |
| 5 | ㄹ | r | l(0) | `l` 不和其他字母衝突，視同標準拼寫 |
| 6 | ㅁ | m | | |
| 7 | ㅂ | b | p(3)、v(2) | 學習者常把 v 當成 b |
| 8 | ㅃ | pp | bb(1) | |
| 9 | ㅅ | s | sh(2) | `shi` → 시 |
| 10 | ㅆ | ss | | |
| 11 | ㅇ | （空字串） | | 無聲初聲 |
| 12 | ㅈ | j | z(2) | |
| 13 | ㅉ | jj | | |
| 14 | ㅊ | ch | | |
| 15 | ㅋ | k | | |
| 16 | ㅌ | t | | |
| 17 | ㅍ | p | f(2) | |
| 18 | ㅎ | h | | |

**中聲表 `JUNGSEONG: [JamoSpec; 21]`**：

| V | 字母 | RR | 替代 | V | 字母 | RR | 替代 |
|:-:|:-:|:-:|:--|:-:|:-:|:-:|:--|
| 0 | ㅏ | a | | 11 | ㅚ | oe | |
| 1 | ㅐ | ae | | 12 | ㅛ | yo | |
| 2 | ㅑ | ya | | 13 | ㅜ | u | oo(2) |
| 3 | ㅒ | yae | | 14 | ㅝ | wo | weo(1) |
| 4 | ㅓ | eo | | 15 | ㅞ | we | |
| 5 | ㅔ | e | | 16 | ㅟ | wi | |
| 6 | ㅕ | yeo | | 17 | ㅠ | yu | |
| 7 | ㅖ | ye | | 18 | ㅡ | eu | |
| 8 | ㅗ | o | | 19 | ㅢ | ui | eui(1) |
| 9 | ㅘ | wa | | 20 | ㅣ | i | |
| 10 | ㅙ | wae | | | | | |

**終聲表 `JONGSEONG: [Option<JamoSpec>; 28]`**（T=0 為 `None`，代表沒有終聲）：

| T | 字母 | RR | 替代（成本） | T | 字母 | RR | 替代（成本） |
|:-:|:-:|:-:|:--|:-:|:-:|:-:|:--|
| 1 | ㄱ | k | g(1) | 15 | ㅀ | lh | |
| 2 | ㄲ | kk | k(4) | 16 | ㅁ | m | |
| 3 | ㄳ | ks | gs(1) | 17 | ㅂ | p | b(1) |
| 4 | ㄴ | n | | 18 | ㅄ | ps | bs(1) |
| 5 | ㄵ | nj | | 19 | ㅅ | s | t(1) |
| 6 | ㄶ | nh | | 20 | ㅆ | ss | t(2) |
| 7 | ㄷ | d | t(3) | 21 | ㅇ | ng | |
| 8 | ㄹ | l | r(1) | 22 | ㅈ | j | t(3) |
| 9 | ㄺ | lk | lg(1) | 23 | ㅊ | ch | t(3) |
| 10 | ㄻ | lm | | 24 | ㅋ | kh | k(3) |
| 11 | ㄼ | lb | lp(3) | 25 | ㅌ | th | t(4) |
| 12 | ㄽ | ls | | 26 | ㅍ | ph | p(3) |
| 13 | ㄾ | lt | | 27 | ㅎ | h | t(4) |
| 14 | ㄿ | lp | | | | | |

> [!NOTE]
> 終聲 ㅋ／ㅌ／ㅍ 的 RR 標準拼寫原本是 `k`／`t`／`p`，會和 ㄱ／ㅅ／ㅂ 撞在一起。本專案把它們的**唯一拼寫改為 `kh`／`th`／`ph`**，原本的 `k`／`t`／`p` 則降為附加成本的替代拼寫。這樣每個字母都有一個不會與同位置其他字母重複的「唯一拼寫」，任何諺文字串都能被精確打出來。**複合終聲（T = 3、5、6、9～15、18）在動態規劃中另加 `COST_COMPOUND_CODA`**，見 P2。

### 必須提供的函式

```rust
pub const S_BASE: u32 = 0xAC00;
pub fn compose(l: usize, v: usize, t: usize) -> Option<char>;
pub fn decompose(c: char) -> Option<(usize, usize, usize)>;
pub fn is_hangul_syllable(c: char) -> bool;
/// 諺文 → 唯一拼寫（逐音節、逐字母使用 `rr` 欄位）。
/// 規則：凡「非詞首的零初聲（ㅇ）音節」之前一律插入 `-`
/// （例：항을 → `hang-eul`、서울 → `seo-ul`、가에 → `ga-e`），以保證往返還原無歧義
pub fn to_romaja(hangul: &str) -> String;
```

### P1 驗收測試（寫在 `jamo.rs` 的 `#[cfg(test)] mod tests`）
- `compose(18, 0, 4) == Some('한')`、`compose(0, 0, 0) == Some('가')`、`compose(18, 20, 27) == Some('힣')`
- 越界：`compose(19, 0, 0) == None`
- **全部 11,172 個音節做 `compose` ↔ `decompose` 往返檢查**
- **唯一性檢查**：在初聲表、中聲表、終聲表各自內部，`rr` 欄位不可重複（初聲 ㅇ 的空字串除外）
- 相容字母檢查：每一個 `compat` 都位於 U+3131..=U+318E

```powershell
cd C:\ATGprjs\ruhi\ruhi-core; cargo test korean::jamo; cargo test
```

---

## 五、P2：音節切分動態規劃（`romaja.rs`）

### 成本常數（集中放在檔案開頭，不得散落各處）

```rust
pub const COST_SYLLABLE: u32 = 10;          // 每個音節的基礎成本 → 偏好較少音節（最長匹配）
pub const COST_NULL_ONSET_AFTER_CODA: u32 = 8; // 前一音節有終聲、本音節零初聲 → 違反連音規律
pub const COST_COMPOUND_CODA: u32 = 5;      // 複合終聲較少見
pub const COST_PENDING_PREVIEW: u32 = 3;    // 預覽模式下，每個未完成字母的成本
pub const NBEST_K: usize = 8;
```

### 演算法
1. 輸入先轉成小寫，並以 `-` 切成數個片段，各片段分別求解後串接（`-` 不輸出任何字元）。
2. 對每個片段在位置 `0..=n` 上做動態規劃，每個位置保留成本最低的前 `NBEST_K` 條路徑。路徑狀態需要記錄「前一音節是否有終聲」。
3. 在位置 `i` 列舉所有 `初聲拼寫（含空字串）+ 中聲拼寫 + 終聲拼寫（含不接終聲）` 的組合。拼寫來源包含 `rr` 與 `alts`，各附帶對應成本。
4. **結尾殘留**有兩種模式：
   - **預覽模式**（`ParseMode::Preview`）：允許尾端留下無法成音節的子音，以相容字母顯示，每個字母加 `COST_PENDING_PREVIEW`。
   - **結算模式**（`ParseMode::Finalize`）：若存在不留殘餘的完整切分，必須選擇它。只有在完全無法完整切分時才保留殘留。
   - 無法辨識的字元（`x`、`q`、數字、標點）一律原樣保留為拉丁字元，不參與組字。
5. **成本相同時的決勝規則**：比較兩條路徑第一個不同的音節，取**較長者**（偏好左側貪婪）。這條規則必須是確定性的，並且寫成測試。

### 介面

```rust
pub enum ParseMode { Preview, Finalize }
pub struct Syllable { pub l: usize, pub v: usize, pub t: usize }
pub struct Parse { pub syllables: Vec<Syllable>, pub pending: String, pub cost: u32 }

pub fn parse_nbest(raw: &str, mode: ParseMode, k: usize) -> Vec<Parse>;
pub fn render(p: &Parse) -> String;              // 音節 + pending
pub fn romaja_to_hangul(raw: &str, mode: ParseMode) -> String;
```

### P2 驗收測試（以下期望值已依成本模型手算確認）

| 輸入 | 模式 | 期望 | 驗證重點 |
| :--- | :--- | :--- | :--- |
| `annyeonghaseyo` | Finalize | `안녕하세요` | 基本組字 |
| `hangeul` | Finalize | `한글` | 切分歧義；N-best 須包含 `항을` |
| `hang-eul` | Finalize | `항을` | `-` 強制切分 |
| `hana` | Finalize | `하나` | 不可為 `한아` |
| `saranghae` | Finalize | `사랑해` | |
| `seoul` | Finalize | `서울` | |
| `hakgyo` | Finalize | `학교` | 終聲 `k` 預設為 ㄱ |
| `kkachi` | Finalize | `까치` | 雙子音初聲 |
| `hoeui` | Finalize | `회의` | ㅚ、ㅢ |
| `a-e` | Finalize | `아에` | |
| `ae` | Finalize | `애` | |
| `bukhan` | Finalize | `북한` | 不可為 `붘안` |
| `dalk` | Finalize | `닭` | 複合終聲 |
| `dalk` | Preview | `달ㅋ` | 預覽時尚未確定 |
| `hanj` | Preview | `한ㅈ` | 不可為 `핝` |
| `g` | Preview | `ㄱ` | |
| `gaeul` | Finalize | `개울` | **成本相同，依左側貪婪決勝**；N-best 須包含 `가을`（由 P4 詞表修正排序） |
| `kimchi` | Finalize | `킴치` | 規則層結果；N-best 須包含 `김치` |
| `gamsahamnida` | Finalize | `감사함니다` | 規則層結果（P4 修正） |
| `xyz` | Preview | 不 panic，保留原字元 | 健全性 |
| 空字串 | 兩者 | `""` | 健全性 |

**往返檢查**：準備約 50 個諺文詞（可以用 P4 的詞表），驗證 `romaja_to_hangul(to_romaja(w), Finalize) == w` 全數成立。若有不成立的情況，修正的是 `to_romaja` 插入 `-` 的規則，**不是去改成本值**。

**效能**：輸入 40 個字母時，`parse_nbest` 需在 1 ms 內完成。可以用 `std::time::Instant` 寫一個寬鬆的測試，門檻設 50 ms，避免 CI 環境不穩定造成誤判。

---

## 六、P3：`KoreanEngine` 狀態機（`korean/mod.rs`）

### 結構

```rust
pub struct KoreanEngine {
    pub raw_romaja: String,
    pub candidates: Vec<String>,
    pub candidate_idx: usize,
    pub is_english_forced: bool, // 行為比照 JapaneseEngine
    pub lexicon: KoreanLexicon,  // P4 才填入，P3 先放空結構
}
```

### 按鍵行為

| 按鍵 | 行為 | `EngineResponse.action`（比照日文模組現有字串） |
| :--- | :--- | :--- |
| `a`–`z`、`-` | 加入 `raw_romaja`，以 Preview 模式重新組字 | 與日文相同 |
| 大寫字母開頭 | 比照 `JapaneseEngine` 的 `is_english_forced` 規則 | 與日文相同 |
| `Backspace` | 刪除**一個拉丁字母**；緩衝區已空時放行給系統 | 與日文相同 |
| `Space` | 以 Finalize 模式組字並上屏，附加一個半形空白 | commit |
| `Enter` | 以 Finalize 模式組字並上屏 | commit |
| `Esc` | 清空緩衝區 | 與日文相同 |
| `Down` | 開啟候選：N-best 結果去重，最多 9 筆 | 與日文相同 |
| `1`–`9` | 選擇候選並上屏 | 與日文相同 |
| `Tab` | **一律放行，不攔截** | pass_through |

> [!IMPORTANT]
> `action`、`mode` 等欄位的字串值**必須照抄 `JapaneseEngine` 實際使用的值**，否則 TSF 前端無法辨識。請先讀 `japanese/mod.rs` 的 `process_key_str`，把用到的值列成清單再開始實作。

### P3 驗收測試
- 逐鍵輸入 `h`,`a`,`n` 時，preedit 依序為 `ㅎ`、`하`、`한`；再輸入 `a` 後成為 `하나`（連音重新切分）
- `hangeul` 後按 `Backspace` 兩次得到 `hange` 的預覽（`한게`）
- `hangeul` + `Space` → `committed_chunk == "한글 "`，且緩衝區清空
- `dalk` + `Enter` → `committed_chunk == "닭"`
- `hangeul` + `Down` → 候選包含 `한글` 與 `항을`；接著按 `2` → 上屏 `항을`
- `Tab` 不改變緩衝區

### ⏸️ P3 停止點
完成後向使用者回報：測試結果摘要、`cargo test` 全套通過數，以及任何偏離本文件之處。**等使用者確認後再進入 P4。**

---

## 七、P4：常用詞表與學習者容錯

### 1. 資料格式 `ruhi/data/korean.tsv`
與 `japanese.tsv` 一致：`諺文詞<TAB>RR 拼寫<TAB>權重`，`#` 開頭為註解。

```tsv
# surface	romaja	weight
감사합니다	gamsahamnida	1000
김치	gimchi	1000
떡볶이	tteokbokki	1000
가을	gaeul	1000
```

另外在程式碼中內建一份精簡版（`load_builtin_lexicon`，做法比照日文模組），確保沒有 TSV 檔時也能運作。

### 2. 詞表內容要求
- **收錄範圍**：問候語、致謝與致歉、數字、交通（역、공항、지하철）、飲食、購物、緊急用語、首爾與釜山等主要地名、常見助詞組合（例如 `-은/는/이/가/을/를/에서`），共 300～500 條。
- **RR 拼寫必須遵守官方發音拼寫規則**，可以參考韓國國立國語院公布的羅馬拼音規則。**不得抄錄有著作權的商業辭典**，也**不得使用中國來源的資料集**（依 AGENTS.md）。
- **權重**：一律填 `1000`。只有極高頻的問候語可以用 `1100`，而且必須在檔案開頭註解中說明分級標準。**禁止為了通過特定測試而單獨調整某一個詞的權重**。

### 3. 每個詞自動產生三種索引鍵
1. `rr`：TSV 提供的官方發音拼寫，例如 `gamsahamnida`
2. `to_romaja(surface)`：逐字母的唯一拼寫，例如 `gamsahapnida`（終聲 ㅂ 的唯一拼寫為 `p`）。這樣依字母拼的人也能查到。

> 三種索引鍵在建立索引時一律**先移除 `-`**；動態規劃比對 `raw[i..j]` 時也同樣不含 `-`（詞邊不可跨越使用者輸入的 `-`）。
3. **學習者骨架鍵**：先移除 `-`，再做以下歸一：`k/g → g`、`t/d → d`、`p/b → b`、`l/r → r`、`ch/j → j`，並把雙子音收斂為單子音。例如 `kimchi`、`gimchi` 都會得到 `gimji`。

### 4. 詞表接入動態規劃的方式（統一公式，不做逐詞調整）
在 P2 的動態規劃中加入**詞邊**：若 `raw[i..j]` 正規化後等於某個詞的索引鍵 1 或 2，就加入一條產生該詞的邊，成本為

$$\text{cost} = \text{音節數} \times \text{COST\_SYLLABLE} - \text{COST\_LEXICON\_BONUS}$$

其中 `COST_LEXICON_BONUS = 6`，為所有詞共用的常數。這個設計讓「詞 + 規則組出的助詞」可以自然串接，例如 `gimchireul` 會組成 `김치` + `를`。

骨架鍵（索引鍵 3）**不進入動態規劃**，只用於 `Up` 鍵的模糊救援（見下一節）。原因是骨架鍵是有損的歸一，直接參與主要排序會造成誤判。

### 5. `Up` 鍵：學習者模糊救援（比照日文的 `lookup_up_arrow_predictions`）
- 以骨架鍵進行前綴比對與完全比對，列出詞表中的候選，例如 `kimchi` → `김치`、`pusan` → `부산`、`ann` → `안녕하세요`。
- 依「完全比對優先、權重降序、字數升序」排序，最多 9 筆。

### P4 驗收測試
- `gamsahamnida` → `감사합니다`；`tteokbokki` → `떡볶이`；`gaeul` → `가을`
- `gimchireul` → `김치를`
- `kimchi`：Finalize 結果仍然是 `킴치`（因為 `kimchi` 不是索引鍵 1 或 2），但 `Up` 候選第一筆為 `김치`
- **回歸檢查**：P2 中所有不受詞表影響的案例結果不變
- 詞表健全性：每一條的 surface 都只包含諺文音節，沒有重複的列，權重只有 `1000` 或 `1100`

---

## 八、P5：接入 `RuhiEngine`（`engine.rs`）

以下修改點都要比照日文分支的寫法：

| 修改點 | 搜尋關鍵字 | 內容 |
| :--- | :--- | :--- |
| 結構欄位 | `pub japanese: crate::japanese::JapaneseEngine` | 新增 `pub korean: crate::korean::KoreanEngine` |
| 建構子 | `japanese: crate::japanese::JapaneseEngine::new()` | 新增初始化 |
| 載入資料 | `japanese.tsv` | 若存在 `korean.tsv` 則載入 |
| 重置 | `self.japanese.reset()` | 新增 `self.korean.reset()` |
| 模式正規化 | `fn normalize_keyboard_mode` | `"ko" \| "hangul" \| "romaja" => "korean"` |
| 徽章 | `fn get_keyboard_mode_badge` | `"korean" => "韓"` |
| 行列教練 | `fn get_array30_coach_hint` | 在排除清單中加入 `"korean"` |
| 下箭頭 | `fn open_down_arrow_candidates` | 新增 korean 分支 |
| 上箭頭 | `fn open_up_arrow_candidates` | 新增 korean 分支 |
| 數字選字 | `self.japanese.candidates[digit_idx]` | 新增 korean 分支 |
| 按鍵分派 | `self.japanese.process_key_str(key_str)`、`self.japanese.process_key(key)` | 新增 korean 分支，並同步 `composed_sentence` 等欄位 |

**注意**：`enabled_keyboard_modes` 的預設值**不要**加入 `korean`，以免改變現有使用者的切換循環。由使用者在設定中心自行勾選。

### P5 驗收測試（寫在 `engine.rs` 既有的測試模組中）
- `set_keyboard_mode("ko")` 之後，`keyboard_mode == "korean"`，徽章為 `韓`
- 在 `korean` 模式下鍵入 `hangeul` 再按 `Space`，回應的 `committed_chunk == "한글 "`
- `Down`、`Up`、數字選字都能正確走到韓文分支
- 從 `korean` 切回 `hsu` 時緩衝區清空，`last_keyboard_mode == "korean"`
- 全套 `cargo test` 通過，`cargo build --release` 成功

### ⏸️ P5 停止點
向使用者回報後，等確認再進入 P6。

---

## 九、P6：接入 TSF 前端與設定中心

### 已知的修改點（2026-10-21 以 `Select-String '"japanese"'` 查得，行號可能已偏移）

| 檔案 | 約略行號 | 用途 |
| :--- | :--- | :--- |
| `ruhi-tsf/src/candidate_window.rs` | 32 | 模式徽章與名稱對照 |
| 同上 | 334、2287 | `enabled_keyboard_modes` 預設值（**不要加入 korean**） |
| 同上 | 1306、1354 | 右鍵選單文字 |
| 同上 | 2118 | 徽章字元 |
| 同上 | 2523、2534、2542、2702 | 模式選單項目 ID（日文使用 2005，**韓文請用下一個未被佔用的 ID**，先搜尋確認） |
| `ruhi-tsf/src/service.rs` | 241 | 預設啟用模式（**不要加入 korean**） |
| 同上 | 748、758、826 | 模式選單項目與 ID 對照 |
| 同上 | 1764、2282、3314、3679 | `is_jp` 一類的模式判斷。**請逐處閱讀**：若判斷的是「羅馬字組字類模式的共同行為」，就把 korean 一併納入；若是日文專屬行為，則不要加入 |
| 設定中心 | 待查 | `settings_window.py` 中沒有搜到 `japanese`，請到 `ruhi-studio/` 與 `settings_window.py` 搜尋 `japanese`、`日文`、`mode_options`，找到模式勾選清單後加入 `("korean", "韓文羅馬拼音輸入 (韓)")` |

### 建置與交付
- 依專案既有流程建置 DLL，可參考 `hot_reload.ps1` 與 `docs/TSF_WIN32_COMPATIBILITY_AND_INSTALLER_SPEC.md`。
- **嚴禁由 Agent 啟動 GUI 或以背景方式常駐**。建置完成後，請提供使用者以下手動驗證步驟：
  1. 在專案根目錄執行 `.\hot_reload.ps1`（或使用者慣用的重新載入方式）。
  2. 在設定中心勾選「韓文羅馬拼音輸入 (韓)」。
  3. 在記事本中用 `Ctrl + Caps Lock` 切換到 `韓`，依序測試 `annyeonghaseyo` + `Space`、`hangeul` + `Down` + `2`、`kimchi` + `Up`。
  4. 確認 `Tab` 不被攔截、徽章顯示為 `韓`、深色與淺色主題下候選視窗都正常顯示。

### ⏸️ P6 停止點
等使用者手動驗證回報後，再進入 P7。

---

## 十、P7：文件與變更紀錄
- 在 [RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md](file:///C:/ATGprjs/ruhi/docs/RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md) 的「韓文諺文」一節之前，新增「韓文 RR 羅馬拼音組字（已實作）」，內容包含對照表摘要、四類歧義的處理方式、成本常數表與按鍵行為表。原有的兩鍵式段落保留，並標註為「未來選項」。
- 在 `CHANGELOG.md` 新增一筆紀錄。
- 在本文件末尾的「實作紀錄」區填寫實際完成情況與偏離之處。

---

## 十一、常見陷阱（請在動手前先讀）

1. **不要用貪婪最長匹配取代動態規劃**。`hangeul` 若從左邊貪婪取 `hang`，就會得到錯誤的 `항을`。
2. **不要把終聲 `k`／`t`／`p` 當成唯一對應**。它們是有損的發音拼寫，只能作為附加成本的替代拼寫。
3. **`ng` 不是一定是終聲 ㅇ**。`hangeul` 中的 `ng` 應拆成終聲 ㄴ + 初聲 ㄱ，這個判斷由連音成本處理。
4. **退格刪的是拉丁字母，不是音節**。每次按鍵都從 `raw_romaja` 重新組字，不要另外維護一份可以被局部修改的諺文緩衝區。
5. **相容字母（U+3131 區段）只能用在未完成預覽**，結算後輸出的必須是完整音節（U+AC00 區段）。若真的無法成音節，保留拉丁字元，而不是輸出孤立的相容字母。
6. **不要修改 `enabled_keyboard_modes` 的預設值**，這會改變現有使用者的操作習慣。
7. **成本常數若需調整，必須同時跑完 P2 與 P4 的全部測試**，並在實作紀錄中說明理由。不得為了單一案例調整成本值。

---

## 十二、實作紀錄（由接手 Agent 填寫）

| 階段 | 完成日期 | 提交雜湊 | 測試結果 | 偏離本文件之處 |
| :--- | :--- | :--- | :--- | :--- |
| P0 | 2026-10-21 | `(branch: feature/korean-rr)` | 基準全套 108 項：107 通過、1 項既有失敗（`test_dual_track_arrow_lookup_and_contextual_rescue` 徽章 `"歐"` vs `"英"`，依規記錄且不更動） | 無 |
| P1 | 2026-10-21 | `2909131` | `korean::jamo` 5/5 通過（含 11,172 音節往返、RR 唯一性、相容字母區間驗證） | `to_romaja` 除「非詞首零初聲前加 `-`」外，於「無終聲後接緊音 `kk/pp/ss/jj`」與「複合終聲後」亦補上 `-` 以確保往返 100% 無歧義 |
| P2 | 2026-10-21 | `399f556` | `korean::romaja` 3/3 通過（含 20 個核心案例、75 詞往返檢查、40 字元 < 1ms 效能測試） | 無 |
| P3 | 2026-10-21 | `c878112` | `korean` 全模組 13/13 通過；日／歐／注／倉／行模組回歸全數通過 | 無 |
| P3 審查修正 | 2026-10-22 | `7393c1b` | `korean` 15/15 通過（新增 532 組終聲×初聲邊界窮舉往返、結算尾端保留拉丁字母） | ① `to_romaja` 的 `-` 規則改為通用判斷 `boundary_can_shift`，修正 각까、갓싸 往返失敗；② Finalize 模式不再輸出孤立相容字母（陷阱 5），如 `g` 結算為 `g`、`anny` 結算為 `안ny` |
| P4 | 2026-10-22 | `d88923d` | `korean` 全模組 17/17 通過（含指定案例、60 條內建詞表與 406 條 `data/korean.tsv` 全量 5 項窮舉檢查） | 全量檢查發現 4 條目需修正：`체크인` 補上音節連字號 `chekeu-in`；單音節複合終聲詞 `닭/삶/값` 因單音節不建 DP 詞邊而改收錄多音節詞 `닭갈비/달걀/가격` |
| P5 | 2026-10-22 | `fa47139` | 工作區全套 126/126 通過；`cargo build --release` 成功 | 該提交單獨檢出時 `test_dual_track_arrow_lookup_and_contextual_rescue` 失敗（見 P5 審查修正） |
| P5 審查修正 | 2026-10-22 | `622e681` | 乾淨工作目錄（僅含已提交內容）全套 **125/125** 通過（工作區多出之 1 項屬使用者未提交之梵文修改） | ① `fa47139` 只提交測試的 `"歐"` 斷言、未提交對應的徽章對照，已補提交該行；② P3 審查修正之雜湊原誤植為不存在的 `d0795ef`，已更正為 `7393c1b`；③ 內建詞表實為 60 條（原紀錄 55） |
| P6 | 2026-10-22 | `c8737e5` | 乾淨工作目錄（`_tmp_korean_verify`）全套 `ruhi-core` 126/126 通過（新增 `test_p6_finalized_commit_text`）；`ruhi-tsf` 與 `ruhi-studio` `cargo build --release` 均零錯誤完成 | 14.4(G) 方向鍵：因 `service.rs` 於方向鍵進入 `detect_*` 前會先呼叫 `consolidate_engine_to_pending()` 將 `raw_romaja` 轉入 `last_raw_ascii` 並重置緩衝區，故於 `KoreanEngine` 新增 `lookup_down_candidates_for_romaja` 與 `lookup_up_candidates_for_romaja` 供 `last_raw_ascii`／`pending_segments` 區段查詢；(G) 選做部分（對已併入暫存流無 `raw_romaja` 之純諺文表面反查）本輪未做 |
| P6 審查修正 | 2026-10-22 | 見 `fix(korean): P6 review` 提交 | `korean` 20/20 通過（新增 `test_p6_digit_after_consolidation_is_literal`） | 第 14.4(B) 節指引錯誤（非接手 Agent 之失誤）：數字鍵 `digit_consolidate` 未納入韓文，致 `hangeul2` 會上屏第 2 候選。已補上 `\|\| engine.korean.has_buffer()`，僅暫存該行 |
| P7 | | | | |


---

## 十三、P4 起續做指引（2026-10-22 審查後補充，優先於第七、八節原文）

### 13.1 目前狀態

| 項目 | 內容 |
| :--- | :--- |
| 分支 | `feature/korean-rr`（最新提交 `7393c1b`） |
| 已完成 | P1 `jamo.rs`、P2 `romaja.rs`、P3 `korean/mod.rs`；韓文測試 15/15 通過 |
| 工作區 | `main` 帶過來的未提交修改（`engine.rs`、`western/mod.rs`、TSF、設定檔等）屬於使用者其他工作，**提交時只能 `git add` 你修改的檔案，不得 `git add -A` 或 `git add .`** |

**可直接使用的現有介面**（不要重寫）：

```rust
// korean/jamo.rs
pub fn compose(l, v, t) -> Option<char>;
pub fn decompose(c) -> Option<(usize, usize, usize)>;
pub fn is_hangul_syllable(c) -> bool;
pub fn to_romaja(hangul: &str) -> String;   // 保證往返；含 '-'
// korean/romaja.rs
pub enum ParseMode { Preview, Finalize }
pub fn parse_nbest(raw, mode, k) -> Vec<Parse>;
pub fn render(p: &Parse) -> String;
pub fn romaja_to_hangul(raw, mode) -> String;
// korean/mod.rs
pub struct KoreanLexicon;   // 目前是空結構，P4 要替換成真正實作
pub struct KoreanEngine { raw_romaja, candidates, candidate_idx, is_english_forced, lexicon }
```

### 13.2 審查時發現的問題（請引以為戒）

上一輪審查修正了兩個錯誤，兩者的共同原因是**測試只抽樣、沒有窮舉**：
1. `to_romaja` 在 각까、갓싸 這類邊界無法往返。原本的 75 詞抽樣測試沒有涵蓋到，改成窮舉 532 組「終聲 × 初聲」後才發現。
2. 結算時輸出孤立相容字母（`g` 上屏成 `ㄱ`），違反第十一節陷阱 5。

**因此本階段起強制要求**：
- 凡是「對整份資料都應成立」的性質（例如詞表每一條都能往返），測試必須**跑過全部資料**，不得只挑幾個例子。
- 回報時列出的測試結果，必須是**實際執行的輸出**，不得預先寫好。

---

### 13.3 P4-0：更新一條過時的既有測試（使用者已授權）

使用者確認：原本的「英文」模式已升級為「泛歐」模式，徽章改為 `歐` 是**刻意的設計**。因此 P0 記錄的那一項既有失敗，是測試本身過時，不是程式錯誤。

- 檔案：`ruhi-core/src/engine.rs`，函式 `test_dual_track_arrow_lookup_and_contextual_rescue`（約第 5832 行起）
- 修改：約第 5934 行 `assert_eq!(engine.get_keyboard_mode_badge(), "英");` 改為 `"歐"`
- **只改這一個字串**，同一函式內其他斷言不動。若改完後該測試仍因其他斷言失敗，停下來回報，不要繼續修。
- 注意：`engine.rs` 已有使用者未提交的修改。這次提交時，請用 `git add -p ruhi-core/src/engine.rs` **只暫存這一行的變更**；若無法安全地只暫存這一行，就先不提交 `engine.rs`，並在回報中說明。

---

### 13.4 P4：常用詞表與學習者容錯（詳細規格）

#### (1) 新增 `ruhi-core/src/korean/lexicon.rs`，取代 `mod.rs` 中的空結構

```rust
#[derive(Debug, Clone)]
pub struct KoreanEntry {
    pub surface: String, // 諺文詞（只能含完整諺文音節）
    pub rr: String,      // TSV 提供的官方 RR 拼寫
    pub weight: u32,     // 1000 或 1100
}

#[derive(Debug, Clone, Default)]
pub struct KoreanLexicon {
    pub entries: Vec<KoreanEntry>,
    /// 精確索引：鍵 1（rr）與鍵 2（to_romaja(surface)），皆已移除 '-' 並轉小寫
    pub key_index: HashMap<String, Vec<usize>>,
    /// 學習者骨架索引：鍵 3
    pub skeleton_index: HashMap<String, Vec<usize>>,
    /// 精確索引中最長的鍵長（位元組），供 DP 限制詞邊搜尋範圍
    pub max_key_len: usize,
}

impl KoreanLexicon {
    pub fn new() -> Self;                 // 空詞表
    pub fn with_builtin() -> Self;        // 載入內建精簡詞表
    pub fn add_entry(&mut self, surface: &str, rr: &str, weight: u32);
    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<usize>;
    /// 依鍵查詢，回傳的條目依「權重降序、加入順序升序」排序
    pub fn lookup_exact(&self, key: &str) -> Vec<&KoreanEntry>;
    pub fn skeleton(romaja: &str) -> String;
}
```

- `add_entry` 遇到相同 `(surface, rr)` 時只保留較高權重，不重複加入。
- `load_tsv` 格式與 `japanese.tsv` 相同：`諺文詞<TAB>RR<TAB>權重`，`#` 開頭與空行略過，權重缺省為 1000。可參考 `JapaneseEngine::load_tsv` 的寫法。

**骨架函式 `skeleton` 的精確定義**（照順序執行，兩邊都用同一個函式）：
1. 轉小寫，移除所有 `-`
2. `ch` → `j`
3. 逐字元替換：`k`→`g`、`t`→`d`、`p`→`b`、`l`→`r`
4. 把連續重複的同一個子音收斂成一個（母音 `a e i o u` 不收斂）

驗算：`kimchi` → `kimji` → `gimji`；`gimchi` → `gimji` ✓；`pusan` → `busan` ✓。

#### (2) 詞邊接入動態規劃

**不要改動** `parse_nbest`、`romaja_to_hangul` 的簽名與行為，P2 測試必須原封不動地通過。另外新增：

```rust
pub fn parse_nbest_with_lexicon(raw: &str, mode: ParseMode, k: usize, lex: Option<&KoreanLexicon>) -> Vec<Parse>;
pub fn romaja_to_hangul_with_lexicon(raw: &str, mode: ParseMode, lex: Option<&KoreanLexicon>) -> String;
```

並讓原本的 `parse_nbest(raw, mode, k)` 改為呼叫 `parse_nbest_with_lexicon(raw, mode, k, None)`。

在 `parse_segment_nbest` 中，對每個位置 `i`、每個 `j ∈ (i, min(n, i + max_key_len)]`，若 `seg[i..j]` 在 `key_index` 中：

| 項目 | 規定 |
| :--- | :--- |
| 適用條目 | **只有 2 個音節以上的詞**才建立詞邊；單音節詞只用於 `Up` 查詢。原因：單音節鍵太短，容易在長字串中誤匹配 |
| 邊成本 | `音節數 × COST_SYLLABLE − COST_LEXICON_BONUS + 同鍵排名`，其中 `COST_LEXICON_BONUS = 6`，「同鍵排名」是該條目在 `lookup_exact(key)` 結果中的索引（0, 1, 2…） |
| 連音懲罰 | 若前一路徑有終聲、且詞的第一個音節為零初聲，照常加 `COST_NULL_ONSET_AFTER_CODA` |
| 終聲狀態 | 以詞最後一個音節是否有終聲，決定寫入 `dp[j][0]` 或 `dp[j][1]` |
| `span_lens` | 第一個音節記為 `j − i`，其餘音節記為 0 |

「同鍵排名」是統一公式，用來讓同一個拼寫對應多個詞時排序穩定，**不是**逐詞調整權重的手段。`COST_LEXICON_BONUS` 與其他成本常數放在一起。

#### (3) `KoreanEngine` 的修改
- `lexicon` 欄位改為 `lexicon::KoreanLexicon`，`new()` 使用 `KoreanLexicon::with_builtin()`。
- 新增 `pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<usize>`，轉呼叫詞表。
- `make_preedit` 與 `lookup_down_candidates` 改用 `*_with_lexicon(..., Some(&self.lexicon))`。
- 新增 `pub fn lookup_up_candidates(&self) -> Vec<String>`：
  1. 以 `skeleton(raw_romaja)` 在 `skeleton_index` 中找**完全相同**的條目
  2. 再找骨架**以輸入骨架為前綴**的條目（輸入骨架至少 2 個字元才做前綴比對）
  3. 各組內依「權重降序 → 音節數升序 → 加入順序」排序，去重，最多 9 筆
- `process_key_str` 新增 `"Up" | "up"`：有候選時回傳 `action: "show_candidate_popup"`、`mode: "candidates"`，並把候選寫入 `self.candidates`（行為比照 `JapaneseEngine` 的 `Up` 分支）；沒有候選或緩衝區為空時回傳 `action: "none"`、`mode: "korean"`。

#### (4) 詞表資料
- `ruhi/data/korean.tsv`：300～500 條，分類見第七節（2）。檔頭註解寫明：格式、權重分級標準（一般 1000、極高頻問候語 1100）、RR 依據為韓國國立國語院公布的〈국어의 로마자 표기법〉。
- 內建精簡版（`with_builtin`）：約 50 條最常用詞，**必須是 `korean.tsv` 的子集**，且至少包含：안녕하세요 `annyeonghaseyo`、감사합니다 `gamsahamnida`、김치 `gimchi`、떡볶이 `tteokbokki`、가을 `gaeul`、부산 `busan`、서울 `seoul`、한글 `hangeul`、하나 `hana`。
- **不要**收錄單獨的助詞（은、는、이、가、을、를……），它們由規則組字即可。
- 不得抄錄商業辭典，不得使用中國來源資料。

#### (5) P4 驗收測試（全部必須實際執行通過）

**A. 指定案例**（使用 `KoreanEngine` 或 `romaja_to_hangul_with_lexicon` 搭配內建詞表）

| 輸入 | 操作 | 期望 |
| :--- | :--- | :--- |
| `gamsahamnida` | Finalize | `감사합니다` |
| `tteokbokki` | Finalize | `떡볶이` |
| `gaeul` | Finalize | `가을` |
| `gimchireul` | Finalize | `김치를` |
| `kimchi` | Finalize | `킴치`（`kimchi` 不是精確鍵） |
| `kimchi` | `Up` 第一筆 | `김치` |
| `pusan` | `Up` 第一筆 | `부산` |

**B. 詞表全量檢查**（對 `korean.tsv` 的**每一條**執行；測試中以 `env!("CARGO_MANIFEST_DIR")` 組出 `../data/korean.tsv` 路徑讀檔）
1. `surface` 只含完整諺文音節；`weight` 只能是 1000 或 1100；沒有重複的 `(surface, rr)`。
2. **官方拼寫可還原**：`romaja_to_hangul_with_lexicon(rr, Finalize, Some(&lex)) == surface`。
3. **逐字母拼寫可還原**：`romaja_to_hangul_with_lexicon(&to_romaja(surface), Finalize, Some(&lex)) == surface`。
4. **拼寫合理性**：`rr` 與 `to_romaja(surface)`（去掉 `-`）的編輯距離 ≤ `max(2, rr.len() / 4)`。官方 RR 只會因音變產生少量差異，差距過大通常代表拼寫寫錯或對到別的詞。
5. 內建詞表的每一條都必須出現在 `korean.tsv` 中。

第 2、3 項若有失敗，**先檢查該條目的 RR 是否寫錯**。若 RR 正確但與其他條目衝突，在回報中列出衝突清單，不得為了通過測試而修改成本常數或單獨調整權重。

**C. 回歸**：P1～P3 的 15 項測試全部通過（P3 的期望值不應因詞表而改變；若有改變，停下來回報原因）。

---

### 13.5 P5：接入 `RuhiEngine`（補充說明）

第八節的修改點表格仍然有效，另補充：
- **方向鍵分派**：日文的 `Up`／`Down` 會走 `open_up_arrow_candidates`／`open_down_arrow_candidates`。韓文的 `Up`／`Down` 已由 `KoreanEngine::process_key_str` 自行處理，因此在 `RuhiEngine::process_key_str`（約第 955 行）的韓文分支中，**直接把所有按鍵交給 `self.korean.process_key_str`**，再比照日文分支同步 `composed_sentence`、`sentence_chars`、`current_candidates`、`candidate_cycle_idx`、`cursor_pos`。`open_up_arrow_candidates`／`open_down_arrow_candidates` 內也加上韓文分支，轉呼叫 `self.korean.process_key_str("Up"/"Down")`，以免從其他入口呼叫時落入中文邏輯。
- **字元分派**：`RuhiEngine::process_key`（約第 1022 行）也要加入韓文分支，轉呼叫 `self.korean.process_key(key)` 並同步上述欄位。
- **數字選字**：若 `RuhiEngine` 在進入子引擎前就攔截數字鍵（日文有 `self.japanese.candidates[digit_idx]` 的分支），韓文也要加上對應分支。
- **載入詞表**：在載入 `japanese.tsv` 的位置（約第 223 行）之後，若 `korean.tsv` 存在就呼叫 `engine.korean.load_tsv(...)`。
- **引擎層測試**：`RuhiEngine::new()` 會載入大型中文詞庫，每個測試約需 5 秒，屬正常現象。全套 `cargo test` 約需 5 分鐘，請以背景方式執行並等待結果，不要中途中斷。

### 13.6 停止點與回報格式

- **P4 完成後先提交，接著做 P5；P5 完成後停下來回報**（P4 本身不是停止點，但 P4 測試必須全數通過才能進入 P5）。
- 回報內容：
  1. 每個階段的提交雜湊
  2. 實際執行的測試輸出摘要（通過數 / 失敗數），全套 `cargo test` 必須跑完一次
  3. 詞表條目數，以及全量檢查中曾經失敗、後來修正的條目清單（說明是 RR 寫錯還是其他原因）
  4. 任何偏離本節規定之處
- 同步更新第十二節「實作紀錄」表格的 P4、P5 列。
- **P6（TSF 前端）不在本輪範圍**，P5 回報後等待使用者指示。


---

## 十四、P6 起續做指引（2026-10-22 P5 審查後補充，優先於第九節原文）

### 14.1 目前狀態

| 項目 | 內容 |
| :--- | :--- |
| 分支 | `feature/korean-rr`（最新提交見 `git log --oneline -1`，至少為 `622e681` 之後） |
| 已完成 | P1～P5；`RuhiEngine` 已可用 `set_keyboard_mode("korean")` 切換並分派全部按鍵 |
| 乾淨目錄全套測試 | 125/125 通過 |
| 本輪範圍 | **只做 P6**（TSF 前端 + 設定中心），完成後停下來回報，**不做 P7** |

### 14.2 P5 審查發現的問題（請引以為戒）

1. **提交的內容與測試的內容不一致**。`fa47139` 只提交了測試斷言 `"歐"`，卻把對應的徽章程式碼留在未提交區。回報的「126 項通過」是在工作區跑的，而提交本身單獨檢出時有 1 項失敗。
2. **回報中出現不存在的提交雜湊**（`d0795ef`）。
3. **數字與實際不符**（內建詞表回報 55 條，實際 60 條）。

**因此本階段起強制要求**：
- **提交後必須在「只含已提交內容」的乾淨目錄驗證**，做法見 14.6。工作區測試結果不能當作提交的測試結果。
- 回報中的每一個雜湊、每一個數字，都必須直接貼上產生它的指令輸出（例如 `git log --oneline -5`、`cargo test` 最後的 `test result:` 行），不得手寫或憑記憶填寫。

### 14.3 工作區注意事項（最重要）

`git status` 顯示以下檔案帶有**使用者其他工作的未提交修改**，本輪會動到其中兩個：

| 檔案 | 本輪是否修改 | 處理方式 |
| :--- | :--- | :--- |
| `ruhi-tsf/src/service.rs` | **是** | 只暫存韓文相關區塊 |
| `ruhi-tsf/src/candidate_window.rs` | **是** | 只暫存韓文相關區塊 |
| `ruhi-studio/src/main.rs` | 是 | 目前無未提交修改，可整檔暫存（暫存前再用 `git diff` 確認一次） |
| `ruhi-core/src/engine.rs`、`western/mod.rs`、設定檔、`user_dict.tsv`、`sanskrit_am.rs` | 否 | **不得暫存、不得還原、不得修改** |

- 一律以 `git add -p <檔案>` 逐區塊選擇；若某個區塊同時混有使用者的修改與你的修改而無法分開，**停下來回報**，不要猜。
- 嚴禁 `git add -A`、`git add .`、`git checkout -- <檔案>`、`git stash`。
- 下文的行號取自**含使用者未提交修改的工作區**（2026-10-22 查得），可能已偏移，請以搜尋關鍵字為準。

---

### 14.4 P6 修改點（已逐處閱讀分類，請照表施作）

#### (A) 核心修正：結算路徑必須使用 Finalize 結果（必做，否則違反陷阱 5）

**問題**：TSF 的所有結算路徑（Space、Enter、Tab、標點、`commit_current_composition`）最終都經過 `service.rs` 的 `fn consolidate_engine_to_pending`（約第 2795 行），它直接讀取 `engine.composed_sentence`。韓文模式下 `composed_sentence` 是 **Preview** 結果，可能含孤立相容字母（例如只打 `g` 時為 `ㄱ`）。若不處理，相容字母會被送進應用程式。

**做法**：
1. 在 `ruhi-core/src/korean/mod.rs`，把既有的私有函式 `fn finalized_commit_text(&self)`（約第 166 行）改為 `pub fn finalized_commit_text(&self) -> String`。**只改可見性，不改內容**。
2. 在 `consolidate_engine_to_pending` 開頭（取得 `engine` 鎖之後、讀取 `composed_sentence` 之前）加入韓文分支：若 `engine.korean.has_buffer()`，則以 `engine.korean.finalized_commit_text()` 作為本次要併入暫存流的文字，`raw_ascii` 使用 `engine.korean.raw_romaja`，並比照日文（`is_jp_chunk` 那段，約第 2819 行）在 `eng_segs` 為空時推入一個涵蓋整段的區段。
3. 此處需要可變借用或先複製資料再釋放鎖，請依該函式既有的鎖寫法處理，**不要在持有 `engine` 鎖時呼叫其他會再次上鎖的 `self.*` 方法**（會造成死鎖）。
4. 在 `ruhi-core` 新增單元測試：鍵入 `g` 後 `finalized_commit_text() == "g"`；鍵入 `hangeul` 後為 `한글`；鍵入 `hangeul` + `Down` + 選第 2 筆後為該候選。

#### (B) 「是否有緩衝區」判斷：四處都要加入韓文（必做）

以下四處的布林式都列舉了 `engine.japanese.has_buffer()`，請在其後加上 `|| engine.korean.has_buffer()`：

| 約略行號 | 所在位置 | 影響 |
| :--- | :--- | :--- |
| 1210 | 按鍵前置判斷 `let (has_buffer, is_english_mode)` | 漏掉會使韓文組字中的按鍵被當作無緩衝區而放行給應用程式 |
| 1389 | 同上（另一個入口） | 同上 |
| 2241 | Escape 處理 `has_engine_preedit` | 漏掉會使 Escape 無法取消韓文組字 |
| 4264 | `fn sync_input_display` | 漏掉會使韓文組字區不顯示、組字被提前結束 |

另外第 2305 行的 `!engine.japanese.has_buffer()` 屬於標點的「先結算注音」判斷，對韓文**不需要**修改。

> **【P6 審查更正】** 本節原文誤稱第 2311 行（數字鍵 `digit_consolidate`）對韓文也不需修改，此說法錯誤。`KoreanEngine` 每次按鍵都會更新候選，且緩衝區非空時 `1`～`9` 為選字鍵；TSF 一般組字時不顯示候選窗，若數字前不先結算，鍵入 `hangeul2` 會默默上屏第 2 候選而非 `한글2`。已於 P6 審查修正中補上 `|| engine.korean.has_buffer()`。

#### (C) Space（必做）

`service.rs` 約第 1759 行的 Space 區塊目前只有日文分支；韓文會落入 `commit_current_composition`，直接把整句送出，而不是「結算當前詞並插入空白」。

請在 `if is_jp_mode { ... }` 之後加入韓文分支，行為規格：
- 韓文有緩衝區：呼叫 `engine.process_key_str("Space")`，把回應的 `committed_chunk`（例如 `"한글 "`）以 `splice_pending_chunk` 併入暫存流（寫法比照第 2397～2400 行的 `action.starts_with("commit")` 分支），呼叫 `sync_input_display`，回傳 `true`。**不要**直接送出整句。
- 韓文無緩衝區但暫存流非空：比照日文 `insert_pending_char(' ', None)`。
- 整句仍由 Enter 送出，與日文一致。

#### (D) Backspace（必做）

約第 2124 行 `if engine.japanese.has_buffer() { let _ = engine.process_key('\x08'); true }`，請新增同樣寫法的 `else if engine.korean.has_buffer()` 分支。韓文退格刪的是拉丁字母（陷阱 4），引擎已正確處理，前端只需轉交。

#### (E) Enter（必做）

約第 2080 行的 Enter 區塊最終會走 `commit_current_composition` → `consolidate_engine_to_pending`。完成 (A) 之後，韓文會自動得到 Finalize 結果，**原則上不需另加分支**。請以 14.5 的手動驗證步驟確認 `g` + Enter 上屏為 `g`、不是 `ㄱ`。

#### (F) 字元分派與連字號（必做）

約第 2282 行：
```rust
let is_jp = matches!(engine.keyboard_mode.as_str(), "japanese" | "jp" | "romaji");
let is_jp_romaji_key = is_jp && matches!(ch, '-' | '\'');
```
韓文 RR 使用 `-` 標示音節邊界（例如 `chekeu-in`、`jung-ang`）。請新增：
```rust
let is_kr = matches!(engine.keyboard_mode.as_str(), "korean" | "ko" | "hangul" | "romaja");
let is_kr_romaja_key = is_kr && engine.korean.has_buffer() && ch == '-';
```
並在 `let punct = ...` 的條件中加入 `&& !is_kr_romaja_key`。只有在韓文**有緩衝區**時才把 `-` 當拼寫鍵；緩衝區為空時 `-` 仍照標點規則處理。

#### (G) 方向鍵（必做部分 + 選做部分）

- **必做**：韓文**有緩衝區**時，`Down`／`Up` 應使用韓文引擎的候選。TSF 在第 2038、2059 行分別呼叫 `detect_target_word_and_cands`（約第 3314 行起）與 `detect_up_arrow_target_and_cands`（約第 3679 行起），兩者開頭都有日文分支。請在兩個函式的日文分支之後各加一個韓文分支：若 `engine.korean.has_buffer()`，分別回傳 `engine.korean.lookup_down_candidates()` / `engine.korean.lookup_up_candidates()`，目標範圍為暫存流中韓文組字所在區段。範圍的計算方式請比照同函式內日文對 `raw_romaja` 區段的處理。
- **選做（本輪可不做，但須在回報中說明有沒有做）**：對**已併入暫存流**的韓文詞重新選字（日文用 `lookup_down_candidates_for_surface`）。韓文目前沒有對應函式；若要做，須先在 `ruhi-core` 新增函式並附測試，不可直接在 TSF 內拼湊邏輯。

#### (H) Tab

約第 1817 行 Tab 為手動斷詞。韓文引擎對 Tab 回傳 `pass_through`（不攔截）。請確認韓文模式下：有緩衝區時 Tab 不會造成資料遺失。最低要求是**不修改此區塊**，並在手動驗證中確認行為；若發現問題，回報而不是自行設計新行為。

#### (I) 選單與徽章

`candidate_window.rs`：

| 約略行號 | 內容 | 動作 |
| :--- | :--- | :--- |
| 32 | 模式徽章與名稱對照 | 新增 `"korean" \| "ko" \| "hangul" \| "romaja" => ("韓", "韓文羅馬拼音")` |
| 334、2287 | `enabled_keyboard_modes` 預設值 | **不要修改** |
| 1306、1354 | 依模式顯示的右鍵選單文字 | 閱讀後比照日文新增韓文分支；若該處文字是日文專屬功能（例如假名相關），韓文落入預設分支即可，不要硬套 |
| 2118 | 徽章字元 | 新增 `"korean" \| "ko" \| "hangul" \| "romaja" => "韓"` |
| 2522～2527 | 模式選單項目清單 | 在日文 `(2005, "japanese", ...)` 之後新增 `(2009, "korean", "韓文羅馬拼音輸入 (韓)\tCtrl/Shift + CapsLock")` |
| 2534、2542 | 目前模式與啟用模式的比對 | 比照日文加入 `mode_code == "korean" && (… == "ko" \|\| … == "hangul" \|\| … == "romaja")` |
| 2698～2704 | ID 對照 | 新增 `2009 => Some("korean")` |

`service.rs`：

| 約略行號 | 內容 | 動作 |
| :--- | :--- | :--- |
| 241 | `default_enabled_keyboard_modes` | **不要修改** |
| 747～752 | 模式選單項目清單 | 同上新增 `(2009, "korean", …)` |
| 758 | 目前模式比對 | 比照日文加入韓文 |
| 822～828 | ID 對照 | 新增 `2009 => Some("korean")` |

**選單 ID 使用 `2009`**：2026-10-22 已在 `ruhi-tsf`、`ruhi-studio`、`scripts` 中搜尋，`2009` 與 `2010` 皆未被使用（已使用者為 2001～2008、2011、2012）。施作前請用 `Select-String -Pattern '\b2009\b'` 再確認一次。兩個檔案**必須使用同一個 ID**。

#### (J) 設定中心（`ruhi-studio/src/main.rs`）

設定中心是 Rust（egui），**不是** Python。

| 約略行號 | 內容 | 動作 |
| :--- | :--- | :--- |
| 561～574 `let layouts = [...]` | 預設亞洲輸入模式單選 | 在日文之後新增 `("korean", "韓文羅馬拼音輸入（Korean RR，外國人友善之標準羅馬拼音組字 - 韓）")` |
| 597～604 `let mode_options = [...]` | 快速切換迴圈勾選 | 在日文之後新增 `("korean", "韓文羅馬拼音輸入 (韓)")` |
| `config_sync.rs` 第 86 行 | 預設啟用清單 | **不要修改** |

#### (K) 其他 `is_jp` 判斷（不要加入韓文）

以下為日文專屬行為，**不要**把韓文加進去：第 2292 行（日文全形標點）、第 2346～2382 行（`でしだが`→`でしたか` 等日文句尾修正）、`detect_*` 函式內的假名／平假名查詢。

---

### 14.5 建置與手動驗證

1. 建置：在 `ruhi-tsf` 與 `ruhi-studio` 各執行 `cargo build --release`，兩者都必須零錯誤。
2. **嚴禁由 Agent 啟動 GUI 或以背景方式常駐**（包含設定中心與 `hot_reload.ps1`）。建置完成後，請在回報中提供使用者以下手動驗證清單，由使用者執行：

| # | 操作 | 期望 |
| :--- | :--- | :--- |
| 1 | 執行 `.\hot_reload.ps1`，開啟設定中心勾選「韓文羅馬拼音輸入 (韓)」 | 勾選後儲存成功 |
| 2 | 記事本中按 `Ctrl + Caps Lock` 切到韓文 | 徽章顯示 `韓` |
| 3 | `annyeonghaseyo` + `Space` + `Enter` | 上屏 `안녕하세요 `（結尾含空白） |
| 4 | `hangeul` + `Down` + `2` + `Enter` | 上屏 `↓` 清單第 2 筆 |
| 5 | `kimchi` + `Up` + `1` + `Enter` | 上屏 `김치` |
| 6 | `g` + `Enter` | 上屏 `g`，**不是** `ㄱ` |
| 7 | `chekeu-in` + `Enter` | 上屏 `체크인` |
| 8 | `hangeul` + `Backspace` ×2 | 組字區顯示 `한그`（刪的是拉丁字母） |
| 9 | 韓文組字中按 `Escape` | 組字取消 |
| 10 | 切回許氏輸入中文 | 中文輸入正常，不受影響 |
| 11 | 深色與淺色主題各看一次候選視窗 | 韓文字顯示正常、無白底殘留 |

---

### 14.6 提交與乾淨目錄驗證（必做，順序不可顛倒）

1. 以 `git add -p` 只暫存韓文相關區塊後，先執行 `git diff --cached --stat` 與 `git diff --cached`，確認暫存內容**只有**本輪修改。
2. 提交。
3. 建立乾淨工作目錄並驗證：
   ```powershell
   cd C:\ATGprjs\ruhi
   git worktree add --detach C:\ATGprjs\_tmp_korean_verify HEAD
   $env:CARGO_TARGET_DIR = 'C:\ATGprjs\ruhi\target_verify'
   cd C:\ATGprjs\_tmp_korean_verify\ruhi-core;   cargo test --lib
   cd C:\ATGprjs\_tmp_korean_verify\ruhi-tsf;    cargo build --release
   cd C:\ATGprjs\_tmp_korean_verify\ruhi-studio; cargo build --release
   cd C:\ATGprjs\ruhi
   git worktree remove --force C:\ATGprjs\_tmp_korean_verify
   ```
   `ruhi-core` 全套約需 5 分鐘，請以背景方式執行並等待，不要中途中斷。
4. 若乾淨目錄的結果與工作區不同，**停下來回報差異原因**，不要以修改測試來掩蓋。

### 14.7 停止點與回報格式

- P6 完成後停下來回報，**不要進入 P7**。
- 回報內容（每一項都附上原始指令輸出）：
  1. `git log --oneline -5` 的輸出
  2. 乾淨目錄 `cargo test --lib` 的 `test result:` 行，以及 `ruhi-tsf`、`ruhi-studio` 建置的最後一行
  3. 14.4 各項（A～K）逐項說明：已完成／未做（附原因）
  4. (G) 選做部分有沒有做
  5. 任何因無法分離使用者修改而未提交的區塊
  6. 14.5 的手動驗證清單（原樣提供給使用者）
- 同步更新第十二節「實作紀錄」表格的 P6 列。

### 14.8 已知的輕微事項（本輪不處理，僅供知悉）

- `RuhiEngine` 在韓文模式下結算時不會更新 `last_committed_word`。TSF 的 `commit*` 分支（約第 2393 行）會另外呼叫 `record_committed`，因此在實際使用中影響有限。留待 P7 或之後評估。
