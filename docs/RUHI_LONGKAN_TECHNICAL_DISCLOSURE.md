# 如喜・龍龕形音輸入法技術公開文件

**Technical Disclosure: Post-hoc Component Fusion and Phono-Semantic Component Coding in the RuhI Longkan Input Method**

| 項目 | 內容 |
|---|---|
| 作者 | Yuktesha Zer 曹大十 |
| 文件版本 | 1.0 |
| 文件日期 | 2026-10-29 |
| 首次公開 | GitHub 儲存庫 https://github.com/Yuktesha/ruhi ，commit `142019e`（2026-10-29 00:18:18 +0800） |
| 對應軟體版本 | RuhI v0.7.0 |
| 永久識別碼 | DOI: [10.5281/zenodo.23751481](https://doi.org/10.5281/zenodo.23751481) |

---

## 公開聲明 (Statement of Purpose)

本文件之目的，在於將下列技術思想**公開為先前技術 (prior art)**，使任何人均可自由引用與檢索。作者不就本文件所述技術申請專利。

本文件僅公開技術思想，**不構成對如喜程式碼、碼表或其他著作之授權**；各該著作之授權條件，依儲存庫中之授權檔案為準。

> This document is published as a defensive disclosure to place the described techniques in the public domain of prior art. The author does not intend to seek patent protection for them. This document does not grant any license to the RuhI source code, code tables, or other copyrighted works.

---

## 英文摘要 (Abstract)

This disclosure describes techniques implemented in the Longkan mode of the RuhI Chinese input method:

1. **Post-hoc component fusion**: After a user has entered several syllables in a phonetic input buffer, the user selects a contiguous range of two to four syllables and presses a fusion key. The engine treats the readings of the selected syllables as the readings of components of a single compound character, generates multiple lookup codes from them (full readings, two-key abbreviated readings, tone-suffixed variants, component-multiplicity codes, first-and-last component codes), looks up a compound-character table, and replaces the selected range with the best matching single character while preserving user selections and manual word boundaries elsewhere in the buffer. Because the operation uses syllable readings rather than displayed characters, it works even when the displayed homophones are incorrect.
2. **Phono-semantic component coding**: For each character, a code is generated from the reading of its semantic radical and the reading of a representative member of its phonetic series. The representative is chosen from characters sharing the same phonetic group as the simplest, most frequent traditional character.
3. **Tolerant multi-track coding in a single table**: Multiple coding tracks (component readings in a Bopomofo-based double-pinyin layout, Hanyu Pinyin full readings, visual Latin-letter pictographs, component multiplicity "C×N") coexist in a single table without mode switching, with exact matches always ranked before prefix previews.
4. **Single-word borrowing of an auxiliary input method** inside an ongoing phonetic composition, with automatic return to the primary input method.

---

## 1. 技術領域

本文件涉及中文電腦輸入法，特別是：
- 以漢字部件讀音組成單字之輸入技術；
- 音碼輸入法之組字緩衝區（輸入座）編輯操作；
- 多種編碼方式並存之碼表設計與候選排序。

## 2. 背景與既有技術

| 既有技術 | 內容 | 與本文件之差異 |
|---|---|---|
| 搜狗輸入法 U 模式拆字 | 先按 `u` 進入拆字模式，依序輸入各部件之漢語拼音組字（如 `u mu mu mu` → 森），可混用筆畫碼 | 必須**在輸入當下**先進入特定模式；無法對已輸入之內容事後組字 |
| Rime 方案 `rime-radical-pinyin` | 以部件拼音組字，支援多種漢語拼音雙拼方案 | 同上；以漢語拼音為基底 |
| 自然碼、小鶴音形 | 雙拼音碼加上以部首讀音首字母表示之形碼 | 形碼用於輔助選字，非以形符與聲符讀音配對成碼 |
| 嘸蝦米 | 以英文字母之形、音、義對應字根 | 本文件之視覺字母軌與其概念相近，此部分不主張新穎 |

---

## 3. 技術內容一：輸入座內事後部件融合 (Post-hoc Component Fusion)

### 3.1 問題

使用者遇到不會讀的合體字（如「龕」）時，常會改打其部件的讀音（如「合」「龍」），結果在組字緩衝區中得到兩個字。既有技術要求使用者在輸入**之前**就決定進入拆字模式；一旦已用一般方式打出部件，就只能刪除重打。

### 3.2 實施方式

以下以注音音碼（許氏雙拼鍵盤）為例。輸入法維護一個組字緩衝區，內含：

- 音節序列 `syllables[0..n]`：每個音節記錄聲母、介音、韻母與聲調；
- 顯示字元序列 `sentence_chars[0..n]`：由動態規劃解碼器依音節序列選出之文字；
- 使用者釘選表 `user_pinned`：位置 → 使用者手動選定之文字；
- 手動斷詞點集合 `manual_cuts`；
- 游標位置與選取錨點。

**步驟：**

1. **輸入部件讀音**：使用者以一般音碼方式輸入各部件之讀音，例如「合」`ㄏㄜˊ`、「龍」`ㄌㄨㄥˊ`。緩衝區中出現兩個音節，顯示文字可能是任何同音字（例如「何隆」）。
2. **標記範圍**：使用者以 `Shift + ←` 或 `Shift + →` 標記連續 2～4 個音節。按下 `Shift` 加方向鍵時，若尚有未完成之音節，先以預設聲調完成該音節；選取錨點設為原游標位置。
3. **觸發融合**：使用者按下融合鍵（本實作為 `Tab`）。若選取範圍長度 ≥ 2，進入融合程序。
4. **產生查詢碼**：將選取範圍內各音節之讀音（**而非顯示文字**）轉為鍵位碼，產生下列查詢碼，依序查詢合體字碼表：
   - (a) 各部件全拼鍵位碼依序串接（如 `hg` + `lxl` → `hglxl`）；
   - (b) (a) 加上最後一個音節之聲調鍵（如 `d`、`f`、`j`、`s`）或聲調數字；
   - (c) 若所有部件讀音相同：部件碼加部件個數（「部件 × N」，如 `mx3` 代表三個「木」→ 森），以及其加聲調變體與兩碼縮寫加個數之變體；
   - (d) 若部件數 ≥ 3 且讀音不全相同：首部件與末部件之全拼串接及其加聲調變體；
   - (e) 兩碼縮寫：各部件讀音取「首鍵＋末鍵」（單鍵者重複一次）。部件數為 2 時串接兩者，部件數 ≥ 3 時取首末兩部件。
5. **收集候選**：依查詢碼順序收集碼表中**單字**結果並去除重複，形成候選清單。查詢時不要求目標字之聲調與部件聲調一致，因為使用者輸入的是部件讀音（例如三個四聲「木」組成一聲「森」）。
6. **替換**：若候選清單非空，取第一候選：
   - 以該字在詞庫中之讀音建立新音節，取代選取範圍內之多個音節；
   - 以該字取代選取範圍內之顯示字元；
   - 將該位置加入使用者釘選表，並把範圍之後的釘選位置與斷詞點位置，依縮減長度向前平移；範圍內之釘選與斷詞點捨棄；
   - 重新解碼整句，游標移至融合字之後，清除選取錨點；
   - 把完整候選清單掛在該位置，使用者可再以選字鍵切換其他候選（例如「回」與「呂」）。
7. 若候選清單為空，融合失敗，緩衝區維持不變。

### 3.3 技術效果

- **事後補救**：使用者可先用熟悉的音碼打出部件，再決定融合，無須預先切換模式。
- **不受顯示文字影響**：因為使用讀音而非顯示字，解碼器即使選錯同音字（「何隆」），仍能融合為正確的「龕」。
- **與整句解碼共存**：融合結果以釘選方式參與後續整句解碼，句中其他使用者選擇與斷詞點均保留。

### 3.4 替代實施方式

下列變化同屬本文件公開之範圍：

1. 融合鍵可為任何按鍵或組合鍵、滑鼠或觸控手勢、語音指令，或候選視窗中之按鈕。
2. 範圍選取可改為：以游標前 N 個音節為預設範圍、依字數而非音節數選取、以滑鼠或觸控拖曳選取。
3. 讀音系統可為注音符號、漢語拼音（全拼或任何雙拼方案）、通用拼音、粵語拼音、台灣台語羅馬字、日語音讀，或其他漢字讀音系統。
4. 融合對象可為已送出至應用程式之文字：透過文字服務之重新轉換 (reconversion) 介面取回文字與讀音後，執行相同融合程序再寫回。
5. 融合對象可為顯示字元而非讀音：以字元之部件身分直接查詢「部件序列 → 合體字」表（例如以表意文字描述序列 IDS 反查）。
6. 部件數上限可為任意值；查詢碼產生規則可增減。
7. 反向操作「事後拆分」：將單一字拆回其部件讀音或部件字元。
8. 可結合語言模型或上下文，對多個融合候選排序。

---

## 4. 技術內容二：形聲結構碼之自動產生 (Phono-Semantic Component Coding)

### 4.1 實施方式

對每個目標字，自動產生「形符讀音碼＋聲符讀音碼」：

1. **取得形符**：由 Unicode Unihan 資料庫之 `kRSUnicode` 欄位取得康熙部首編號，對應為部首字。
2. **取得聲符系列**：由 Unihan 之 `kPhonetic` 欄位取得該字所屬之聲符群組編號（可能有多個，取第一個）。
3. **選出聲符代表字**：對每個聲符群組，收集群組內之常用字並排除純簡化字，依下列規則選出代表字：
   - 統計群組成員之部首分布，取最常見部首為「主要部首」；
   - 優先選擇非主要部首、權重達門檻之常用繁體字；若主要部首字本身筆畫較少，則可選主要部首字；
   - 在候選中取筆畫最少者，筆畫相同時取使用頻率最高者。
4. **退回規則**：若目標字本身即為部首、部首外筆畫數為 0，或聲符代表字之筆畫數不少於目標字，則以目標字本身之讀音代替聲符讀音。若聲符代表字罕用（權重低於門檻），且其讀音與目標字讀音不同，則改以目標字本身之讀音作為後兩碼。
5. **組碼**：形符讀音取兩碼、聲符（或本字）讀音取兩碼，組成四碼。兩碼讀音之取法可有多種並存：
   - 許氏雙拼鍵位之「首鍵＋末鍵」；
   - 以 `n` 表鼻音韻尾之變體；
   - 通用拼音或漢語拼音之前兩字母；
   - 人工指定之慣用別名（例如部首之口語讀音）。
6. **例**：
   - 鋪：形符「金」+ 聲符「甫」→ `jnfx`（許氏）、`kmfu`、`jinfu`（漢語拼音全拼）。
   - 銀：形符「金」+ 本字讀音 → `jnin`。

### 4.2 技術效果

- 編碼直接對應形聲字之造字原理（六書之形聲），使用者可依「有邊讀邊」的直覺推得編碼。
- 聲符代表字的自動選取，可在聲符本身罕用時改用同系列中較常見、較易讀的字。

### 4.3 替代實施方式

1. 聲符群組可改用其他資料來源（例如《說文解字》之聲系、Casey 聲符表以外之聲符分類，或自建資料）。
2. 代表字選取準則可改為：教育部常用字表等級、筆順、字形相似度等。
3. 讀音碼長度、取碼方式與鍵盤配列可任意變化。
4. 可擴及非形聲字：會意字取兩個主要部件之讀音；多部件字取首末部件或前兩個可讀部件之讀音。

---

## 5. 技術內容三：多軌寬容碼並存於單一碼表

### 5.1 實施方式

1. 同一碼表中，每個字可有多組編碼，分屬不同「軌」，以種類欄位標記，例如：
   - 形符讀音＋聲符讀音（第 4 節）；
   - 部件讀音之許氏全拼或漢語拼音全拼（如 `hglxl`、`helong` → 龕）；
   - 視覺字母象形碼：以外形相似之拉丁字母代表部件（如 `o` 代表「口」、`i` 代表「丨」：`oi` → 中、`ooo` → 品）；
   - 部件 × N：部件碼後接數字表示重複次數（`mu3` → 森、`ko3` → 品、`cko3` → 區）；
   - 聲調補碼：碼尾可接 `1`～`5` 或許氏聲調鍵 `d`、`f`、`j`、`s`。
2. 使用者無須切換模式，任何一軌之編碼均可直接輸入。
3. **候選排序**：精確匹配之候選一律排在前綴預覽候選之前；同類候選依權重與上下文轉移分數排序。
4. **自動送出**：當緩衝區已達 4 碼（或已以聲調數字結尾），且新按下的字母無法延續任何較長編碼時，自動送出目前首選，並以新字母開始下一字。如此可同時支援定長四碼連打與較長之全拼部件碼。
5. **數字鍵雙重用途**：數字鍵若能延續有效編碼（聲調補碼或部件 × N），則視為編碼；否則視為選字鍵。
6. **多字詞簡碼**：以 `Tab` 或 `-` 開始詞彙模式，取首字前兩碼、中間各字第一碼、末字後兩碼（上限 6 碼）。忘記先按 `Tab` 時，可將游標移至編碼開頭再補按 `Tab`，轉為詞彙模式。

### 5.2 替代實施方式

軌的種類與數量、字母與部件之對應、數字與重複次數之對應、聲調補碼之按鍵，均可任意變化。

---

## 6. 技術內容四：組字中臨時借用輔助輸入法

### 6.1 實施方式

1. 使用者以主要輸入法（如許氏注音）組字時，按下借用鍵（本實作為 `` ` ``），下一個字或詞改以輔助輸入法（如龍龕）輸入。
2. 輔助輸入法之結果插入主要輸入法之組字緩衝區，**整句組字不中斷**。
3. 輸入一個字或詞後，自動返回主要輸入法。結束條件包括：空白鍵、聲調鍵（如 `4` 或許氏聲調鍵）。以聲調鍵結束時，該鍵同時作為輔助輸入之聲調補碼，下一鍵即回到主要輸入法繼續組字。
4. 另設切換鍵（本實作為 `~`），在主要與輔助輸入法間持續切換，再按一次返回；此切換不覆寫其他切換鍵（如 `Caps Lock`）所記憶之狀態。

### 6.2 例

- 許氏注音輸入「我們」後，按 `` ` `` 並輸入 `hglxl`，再接著以注音輸入「好」，得到「我們龕好」。
- 按 `` ` `` 後輸入 `jenfx4`，聲調鍵 `4` 同時結束借用，接著以注音輸入「子」，得到「鋪子」。

### 6.3 替代實施方式

借用鍵、結束條件、借用範圍（一字、一詞、至標點為止）以及主輔輸入法之組合，均可任意變化；輔助輸入法可為任何形碼、音碼或手寫輸入。

---

## 7. 公開技術特徵摘要

為便於檢索，本文件公開之技術特徵整理如下：

1. 在音碼輸入法之組字緩衝區中，標記已輸入之連續多個音節，觸發後以其讀音作為部件讀音，查詢並融合為單一合體字。
2. 第 1 點之融合依據音節讀音而非顯示文字，因此不受解碼器選字錯誤影響。
3. 第 1 點之融合產生多種查詢碼：全拼串接、加聲調、部件 × N、首末部件、兩碼縮寫。
4. 第 1 點之融合結果以釘選方式保留在整句解碼中，並平移其後之釘選與斷詞點。
5. 依 Unihan `kRSUnicode` 與 `kPhonetic` 自動產生「形符讀音＋聲符代表字讀音」編碼，並依筆畫與頻率自動選出聲符代表字。
6. 多種編碼軌並存於單一碼表、免切換模式，精確匹配優先於前綴預覽，並依碼長與編碼延續性自動送出。
7. 以「部件碼＋數字」表示部件重複次數之編碼。
8. 組字中以單一按鍵臨時借用輔助輸入法輸入一字或一詞，並自動返回；以聲調鍵兼作借用之結束符號。

---

## 8. 參考實作

- 事後部件融合：`ruhi-core/src/engine.rs` 之 `fuse_syllables_with_longkan`、`fuse_range_with_longkan`；單元測試 `test_longkan_v2_hsu_inline_tilde_tab_fusion_and_quicknum_261028`。
- 形聲結構碼產生：`scripts/build_longkan_tsv.py`（聲符代表字選取與四碼產生段落）。
- 多軌碼表與輸入處理：`ruhi-core/src/longkan/mod.rs`、`ruhi-core/src/longkan/table.rs`。

以上路徑以 commit `142019e` 為準。

---

## 9. 引用方式

Yuktesha Zer 曹大十（2026）。〈如喜・龍龕形音輸入法技術公開文件〉，版本 1.0。DOI：10.5281/zenodo.23751481。
