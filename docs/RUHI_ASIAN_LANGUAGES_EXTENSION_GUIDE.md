# 如喜（RuhI）多語系亞洲文字輸入架構與擴充開發指南

本文件闡述 **如喜（RuhI）通用智慧型輸入法** 的跨語系多腳本架構設計（Multi-Script Input Architecture），涵蓋：
1. **自訂義多軸向輸入模式切換迴圈（Customizable Mode-Switching Loop）**
2. **內建輕量級日文羅馬字／假名／漢字轉換引擎（`JapaneseEngine`）**
3. **零詞庫代數合成類與多層次亞洲文字擴充開發指引（韓文、越南文、泰文、印地天城文、阿拉伯文）**

---

## 一、二維正交切換與自訂模式迴圈架構

### 1. 正交雙軸快捷鍵設計
如喜將「亞歐文字直通切換」與「亞洲輸入法內部切換」解耦為兩個正交維度：

| 切換維度 | 觸發方式 | 行為說明 |
| :--- | :--- | :--- |
| **維度 A：亞 ⇄ 歐雙語直通** | 單按 `Caps Lock` | 在「當前亞洲輸入模式（如 `許` 或 `日`）」與「英數／歐文直通模式（`英`）」之間一鍵切換。 |
| **維度 B：亞洲輸入模式循環** | `Ctrl + Caps Lock` 或 `Shift + Caps Lock` 或滑鼠點擊輸入座狀態標籤 | 僅在 **`enabled_keyboard_modes`（使用者自選啟用清單）** 內循環切換。 |

### 2. 自選啟用清單（`enabled_keyboard_modes`）
於 `config.json` 與 `settings_window.py` 中，透過 `enabled_keyboard_modes` 陣列控制參與循環的亞洲模式：
```json
{
    "keyboard_mode": "hsu",
    "enabled_keyboard_modes": [
        "hsu",
        "japanese"
    ]
}
```
- 當使用者僅勾選 `["hsu", "japanese"]` 時，其他注音模式（標準大千 `standard`、倚天 26 鍵 `et26`）與倉頡（`cangjie`）會**徹底排除於快捷鍵迴圈與狀態標籤選單之外**。
- 按下 `Ctrl + Caps Lock` 或 `Shift + Caps Lock` 時，即成為純粹的 **「許氏雙拼（許）⇄ 日文羅馬字（日）」雙態切換器**。

---

## 二、內建日文智慧型輸入引擎（`JapaneseEngine`）：外國學習者最佳夥伴與新世代直覺體驗

實作位置：[`ruhi-core/src/japanese/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/japanese/mod.rs)

### 0. 產品定位與兩階段演進願景（Two-Stage Product Vision）
傳統日文輸入法（如 ATOK、MS-IME）習慣「先盲打整串平假名 ➔ 按空白鍵才轉漢字 ➔ 若分詞錯誤再用 `Shift + ←/→` 手動伸縮文節、或記憶 `F6`~`F10` 功能鍵」。這套源於早期硬體算力受限的操作設計，對日語學習者與跨語系使用者而言門檻較高。

如喜日文智慧型輸入法以**「先成為外國人學習日文輸入的最佳夥伴，並期望口碑自然延伸至日文母語使用者」**為演進方向：
1. **第一階段（Phase 1：全球日語學習者最佳夥伴 — Zero-Baggage Learner Partner）**：
   - **即時漢字預覽（Live Inline Composition）**：免除「整串平假名盲打」，邊輸入羅馬字即同步於輸入座合成漢字與助詞（如 `watashihanihongogasuki` 即時預覽 `私は日本語が好き`），讓學習者一眼確認字義。
   - **用言活用語幹與送り仮名自動機（Inflection & Okurigana Engine）**：內建一段動詞、五段動詞（含促音便、イ音便、撥音便）、サ變／カ變動詞與い形容詞之語幹與活用語尾組合規則。學習者無論輸入辭書形（`taberu` ➔ `食べる`）、禮貌形（`tabemasu` ➔ `食べます`）、過去形（`tabeta` ➔ `食べた`、`oishikatta` ➔ `美味しかった`）或否定形（`ikanai` ➔ `行かない`），皆可自動合成正確漢字與送り仮名。
   - **格助詞與受詞語境消歧（Case-Particle Contextual Disambiguation）**：於連文節合成時自動感知前驅受詞與格助詞（如 `映画を観る` vs `病気を診る` vs `見る`、`音楽を聴く` vs `聞く`、`写真を撮る`），並於同音異義詞（如 `科学` vs `化学`、`橋` vs `箸`）提供清楚候選並列。
   - **學習者音韻盲點 `↑` 模糊救援（Learner Phonetic Fuzzy Rescue）**：針對學習者常見的「長音漏打（`tokyo`/`tookyoo` ➔ `東京`、`gakko` ➔ `学校`、`sensee` ➔ `先生`、`arigato` ➔ `ありがとう`）」、「促音漏打（`gakou` ➔ `学校`、`choto` ➔ `ちょっと`）」與「近音誤植（`yourishi` ➔ `よろしくお願いします`、`nihogo` ➔ `日本語`）」，按一下 `↑` 即可容錯救回正確日文詞彙。
2. **第二階段（Phase 2：藉由學習者口碑自然推廣，期望吸引日文母語者嘗試新世代輸入體驗）**：
   - **現階段沒有打算沿用傳統繁瑣熱鍵，專注打磨直覺操作**：現階段沒有打算回頭採用傳統日文輸入法 `Shift + ←/→` 手動拉扯文節或記憶 `F6`~`F10` 的設計，而是透過游標 `←`/`→` 定位與 `↓` 多跨距候選展開（同時列出複合長詞、短詞、單漢字、平片假名），提供「邊打邊轉漢字 ＋ 上下鍵直覺雙軌」的流暢體驗。
   - **期望透過海外口碑自然延伸至母語使用者**：期望藉由外國學習者與跨語系使用者的良好使用體驗與口碑創造風潮，結合大規模日語詞庫（`ruhi_lexicon/data/lexicon_japanese.tsv`）與「純 Rust 低延遲 ＋ 中日英歐免切換混打」的便利性，自然吸引更多日文母語使用者認識並嘗試這套新世代的直覺輸入方式。

### 1. 狀態機與拼寫規則（Hepburn / 訓令式雙相容）
`JapaneseEngine` 維護兩個核心緩衝區：
- `romaji_buffer`：原始羅馬字按鍵序列（支援退格鍵逐字母回退並即時重算假名）。
- `hira_buffer`：即時合成之平假名序列（含尾部尚未完成音節之殘餘子音顯示）。

支援完整的日文音韻規則：
- **清音、濁音、半濁音與拗音**：完整支援 `kya/kyu/kyo`、`sha/shi/shu/sho`（相容 `sya/si`）、`cha/chi/chu/cho`（相容 `tya/ti`）、`tsu`（相容 `tu`）、`fu`（相容 `hu`）、以及外來語擴充音節（`fa/fi/fe/fo`、`va/vi/vu/ve/vo`、`ti/di/tu/du`、`she/je/che`）。
- **促音（っ / ッ）**：重複子音（如 `kk`, `ss`, `tt`, `pp`）或 `tch`（如 `matcha` ➔ `まっちゃ`）自動轉為促音 `っ`。
- **撥音（ん / ン）**：
  - 輸入 `nn`、`n'` 或 `xn` 強制確認為 `ん`（用於後接母音或 `y` 行時，如 `kinyoubi` 需打 `kinnyoubi` 或 `kin'youbi`）。
  - 單一 `n` 後接非母音、非 `y`、非 `n` 之子音時（如 `nihongo`、`watashi no`、`arigatou`），或在詞尾結算時（如 `sumimasen`），自動收斂為 `ん`。
- **長音符號（ー）**：鍵入連字號 `-` 直接輸出全形長音符號 `ー`（如 `ko-hi-` ➔ `コーヒー`）。

### 2. 與國際習慣及如喜一致的「上下方向鍵雙軌選字」操作體驗
日文模式完全沿用如喜的核心鍵盤人體工學與國際輸入習慣：

| 按鍵 | 日文模式行為 (`keyboard_mode = "japanese"`) | 範例 |
| :--- | :--- | :--- |
| **直接打字** | 即時顯示平假名預覽，並由連文節與用言活用引擎自動合成漢字、送り仮名與助詞整句 | 輸入 `watashihanihongowobenkyoushimasu` ➔ 即時合成 `私は日本語を勉強します` |
| **`↓` 下方向鍵** | **同音漢字／平假名／片假名／羅馬字轉換選單**（免背 `F6`~`F10`，一鍵切換） | 輸入 `nihongo` 按 `↓` ➔ `a. 日本語`、`s. にほんご`、`d. ニホンゴ` |
| **`↑` 上方向鍵** | **前綴補全、敬語展開與學習者音韻盲點模糊救援**（長音／促音／近音誤植自動校正） | 輸入 `ariga` 按 `↑` ➔ `ありがとうございます`；誤打 `yourishi` 按 `↑` ➔ 救援出 `よろしくお願いします` |
| **`Space`（空白鍵）** | 將當前日文轉換段固定於輸入座並插入空白（與國際打字習慣接軌） | 在長句或單詞間自然插入空白分隔 |
| **`Enter`（確認鍵）** | 將如喜輸入座中的完整文字確認送出至目標視窗 | 確認無誤後一次送出上屏 |

---

## 三、亞洲多語系文字擴充開發指引（Developer Blueprints）

如喜的核心引擎採用 `LanguageSubEngine` 模組化設計。未來若需擴充其他亞洲文字，可依循「**零詞庫代數／規則合成優先，外掛輕量詞包次之**」之工程準則，於 `ruhi-core/src/` 下新增對應子模組。

### 1. 韓文諺文（Korean Hangul — 2-Set Dubeolsik 代數合成機）
- **核心原理**：韓文無需內建龐大字表，Unicode 諺文音節區塊（`U+AC00`..`U+D7A3`，共 11,172 字）具備嚴格的線性代數結構：
  $$\text{SyllableCode} = \text{0xAC00} + (L \times 21 + V) \times 28 + T$$
  其中：
  - $L \in [0, 18]$ 為初聲（Choseong，19 個子音：`ㄱ ㄲ ㄴ ㄷ ㄸ ㄹ ㅁ ㅂ ㅃ ㅅ ㅆ ㅇ ㅈ ㅉ ㅊ ㅋ ㅌ ㅍ ㅎ`）
  - $V \in [0, 20]$ 為中聲（Jungseong，21 個母音：`ㅏ ㅐ ㅑ ㅒ ㅓ ㅔ ㅕ ㅖ ㅗ ㅘ ㅙ ㅚ ㅛ ㅜ ㅝ ㅞ ㅟ ㅠ ㅡ ㅢ ㅣ`）
  - $T \in [0, 27]$ 為終聲（Jongseong，0 代表無收尾音，1..27 代表單／雙收尾子音）
- **連音移轉狀態機（Liaison / Consonant Migration）**：
  - 當狀態處於 $L + V + T$（已有終聲 $T$），若下一鍵為**母音 $V_2$**，則原終聲 $T$ 必須自動拆解（若為複合終聲如 `ㄳ` 則拆為 `ㄱ` 留原字、`ㅅ` 移轉；若為單一終聲則整個移轉）成為下一個音節的初聲 $L_2$，合成 $(L+V) + (L_2+V_2)$。
- **估算體積**：純 Rust 狀態機約 **150 行程式碼、0 KB 外部詞庫**。

### 2. 越南文（Vietnamese Quốc ngữ — Telex / VNI 聲調轉導器）
- **核心原理**：以拉丁字母為基底，透過有限狀態轉導器（FST）攔截字尾修飾鍵，動態合成雙重附加符號（母音帽子 + 五聲調號）：
  - **字母修飾鍵（Telex）**：`aa` ➔ `â`、`aw` ➔ `ă`、`ee` ➔ `ê`、`oo` ➔ `ô`、`ow` ➔ `ơ`、`uw` ➔ `ư`、`dd` ➔ `đ`。
  - **聲調修飾鍵（Telex）**：`s`（銳聲 Sắc `´`）、`f`（玄聲 Huyền `` ` ``）、`r`（問聲 Hỏi `?`）、`x`（跌聲 Ngã `~`）、`j`（重聲 Nặng `.`）、`z`（移除聲調）。
  - **自由語序聲調歸位演算法**：使用者可在音節末尾才輸入聲調鍵（例如輸入 `vieets`），引擎自動解析音節核（Nucleus），將銳聲精準掛載至主母音 `ê` 上，產出 `viết`；若重複按修飾鍵（如 `vieetss`）則自動還原為英文原字 `viets`。
- **估算體積**：純 Rust 規則表約 **120 行程式碼、0 KB 外部詞庫**。

### 3. 泰文（Thai Kedmanee / Pattachote — WTT 2.0 堆疊檢核矩陣）
- **核心原理**：泰文無空白分詞，且每個字元格子由四個垂直層級（Level）組成：
  - **Base**：子音（`ก-ฮ`）與前後置母音（`เ แ โ ไ ใ ะ า`）
  - **Above 1 / Below**：上母音（`ั ิ ี ึ ื`）與下母音（`ุ ู`）
  - **Above 2（Top）**：聲調符號（`่ ้ ๊ ๋`）與消音符（`์`）
- **WTT 2.0 序列合法性矩陣**：透過 $17 \times 17$ 字元類別轉移表（Character Classification Matrix），自動攔截不合法的重複聲調或無基座懸空母音，並在輸入順序顛倒（先打聲調再打上母音）時自動重排正規化（Canonical Reordering）。
- **估算體積**：約 **100 行程式碼、0 KB 外部詞庫**。

### 4. 印地天城文與南亞文字（Indic Devanagari — Abugida 音節轉導器）
- **核心原理**：天城文（Devanagari）屬於附標文字（Abugida），每個子音字母（如 `क` `ka`）預設自帶短母音 `a`：
  - **雙子音叢集（Conjuncts）**：當兩個子音連續輸入（如 `k` + `r`），引擎自動在第一子音後插入去母音符號 **Virama / Halant（`्`，`U+094D`）**，合成 `क्र`（`kra`）。
  - **獨立母音 vs 附標母音（Matra）**：詞首輸入母音（如 `aa`）輸出獨立母音字母 `आ`；子音後輸入母音（如 `kaa`）則自動消除預設 `a` 並改掛母音附標 `ा`（輸出 `का`）。

### 5. 阿拉伯文（Arabic — 連寫字形與雙向游標 BiDi 規範）
- **核心原理**：
  - Unicode 標準儲存邏輯字碼（`U+0600`..`U+06FF`），由作業系統 HarfBuzz / DirectWrite 自動根據前後文渲染「獨立（Isolated）、詞首（Initial）、詞中（Medial）、詞尾（Final）」四種連寫字形。
  - 在如喜懸浮輸入座（`candidate_window.rs`）中，若偵測到 RTL（Right-to-Left）字元集，只需將 `DrawTextW` 旗標加上 `DT_RTLREADING`，即可原生支援由右向左的組字游標與雙向文字排版。

---

## 四、新語系模組接入步驟（Step-by-Step Integration Checklist）

若開發者欲在如喜新增一套亞洲語言引擎（以韓文 `korean` 為例）：
1. **建立子引擎**：於 `ruhi-core/src/korean/mod.rs` 實作 `KoreanEngine`，提供 `process_char`、`backspace`、`commit_current`、`lookup_down_candidates` 與 `lookup_up_predictions`。
2. **註冊模式代碼與狀態標籤**：
   - 於 [`RuhiEngine::get_keyboard_mode_badge`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 與 [`paint_candidate_window`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/candidate_window.rs) 加入 `"korean" => "韓"`。
3. **掛載雙軌方向鍵與快捷鍵迴圈**：
   - 於 [`RuhiEngine::process_key_str`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 將 `"korean"` 分派至 `self.korean`。
   - 於 [`settings_window.py`](file:///C:/ATGprjs/ruhi/settings_window.py) 的 `mode_options` 清單加入 `("korean", "韓文二ボル式輸入 (韓)")`，即可由使用者自由勾選納入 `Ctrl / Shift + Caps Lock` 快速切換迴圈。
