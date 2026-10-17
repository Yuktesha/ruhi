# 如喜（RuhI）輸入法 — 多語系擴充、雙軌方向鍵智慧預測與自訂模式迴圈工作成果交接書

- **來源對話（開發工作階段）**：`c7786106-edf7-4da8-8639-57c0388f4d17`
- **目標對話（打包工作階段）**：`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`
- **建立時間**：`2026-10-17 13:03 (+08:00)`
- **編譯與測試狀態**：
  - `ruhi-core` Release 單元測試：**92 passed; 0 failed**（含歐文雙軌、亞洲語境救援、頭碼快打、日文羅馬字引擎與自訂模式迴圈測試）
  - `ruhi-core` Release 產物：`C:\ATGprjs\ruhi\ruhi-core\target\release\ruhi.dll`（已重新編譯最新二進位檔）
  - `ruhi-tsf` Release 產物：`C:\ATGprjs\ruhi\ruhi-tsf\target\release\ruhi_tsf.dll`（已重新編譯最新二進位檔）

---

## 一、核心研發成果總覽（Core Achievements）

本次工作階段完成了五大核心里程碑，將如喜（RuhI）從繁體中文輸入法升級為具備**亞歐雙軸正交切換、上下方向鍵雙軌智慧預測、內建日文羅馬字輸入**的通用多語系智慧型輸入法架構：

### 1. 歐文模式與中英混打之「上下方向鍵雙軌按需擴展架構」
- **核心設計原則**：**零日常打字干擾（Zero Automatic Popup Interference）**。英數／歐文直通模式（`Caps Lock` ON）與許氏中英混打（`MixedEnglishRouter`）日常打字時絕不主動彈窗搶空白鍵；僅在使用者主動按下方向鍵時喚起：
  - **`↓` 下方向鍵（變音符號還原與多語系外來語展開）**：
    - 自動將純 ASCII 拼寫展開為法／德／西／葡／北歐／越／土／波蘭等帶重音變音符號之精準正字（如 `naive` ➔ `naïve`、`resume` ➔ `résumé`、`strasse` ➔ `Straße`、`zurich` ➔ `Zürich`、`sao` ➔ `São`）。
  - **`↑` 上方向鍵（長單字前綴補全、拼寫模糊救援與上文語境推論）**：
    - **前綴快打補全**：輸入 3 碼以上前綴（如 `inter`、`accomm`）按 `↑`，列出高頻長單字（`international`、`accommodate`）。
    - **Levenshtein 編輯距離與鍵位鄰近救援**：拼錯時（如 `definately`、`enviroment`、`recieve`）按 `↑` 救回正確拼法（`definitely`、`environment`、`receive`）。
    - **上文語境推論（Contextual Collocation Rescue）**：根據前文詞彙（如 `artificial i`、`machine l`、`quantum c`、`renewable e`）與句法槽位推斷後續單字（`intelligence`、`learning`、`computing`、`energy`）。

### 2. 亞洲文字（許氏注音／標準注音／倉頡）之「`↑` 上方向鍵語境推論與頭碼快打」
- **注音前綴／聲母頭碼快打（Initial-Code / Prefix Prediction）**：
  - 經詞庫碰撞密度實證分析（46,985 條多字詞）：**二字詞以上**支援「完整首字音節 + 次字聲母」（如 `台灣`、或未按空白鍵連續輸入 `ㄊㄞㄨ` 按 `↑`）；**三字詞與四字成語以上**支援純聲母頭碼（如三碼 `ㄊㄨㄍ` ➔ `圖書館`，四碼 `ㄒㄆㄑㄏ` ➔ `心平氣和`、`ㄧㄕㄦㄋ` ➔ `一石二鳥`）。
- **上文搭配語境救援（Contextual Bigram / Idiom Rescue）**：
  - 當某個字一時拼不出來或忘記讀音時，根據前文已輸入之漢字（讀取 `transitions_user.tsv` 二階轉移矩陣與多字詞／成語庫）按 `↑` 直接推論後續字詞（例如上文為 `胸有` 按 `↑` ➔ `成竹`；上文為 `台灣` 按 `↑` ➔ `積體電路`、`大學`）。
- **倉頡 `Z` 鍵萬用字元查詢與 `↑` 字根前綴預測**：
  - 支援倉頡 `Z`（難／萬用碼 `*`）通配查詢（如首尾碼 `日 + Z + 大` 查詢「器」等中間字根遺忘之字），以及按 `↑` 展開字根前綴候選。

### 3. 亞歐正交雙軸切換與「自訂亞洲模式快速切換迴圈（`enabled_keyboard_modes`）」
- **正交雙軸快捷鍵**：
  - **單按 `Caps Lock`**：在「當前亞洲輸入模式（如 `許` 或 `日`）」與「英數／歐文直通模式（`英`）」之間一鍵切換。
  - **`Ctrl + Caps Lock` / `Shift + Caps Lock` / 滑鼠左鍵點擊輸入座狀態標籤（`半   許`）**：在亞洲輸入模式之間循環切換。
- **使用者自選啟用清單（`enabled_keyboard_modes`）**：
  - 於 `config.json` 與 `settings_window.py` 新增 `enabled_keyboard_modes` 設定，預設值為 **`["hsu", "japanese"]`**（僅保留許氏雙拼與日文羅馬字，將標準大千、倚天 26 鍵與倉頡完全排除於快捷鍵迴圈之外）。
  - 使用者可於偏好設定中心「⌨️ 鍵盤配置與常用鍵」頁籤自由勾選參與循環之亞洲模式。

### 4. 內建輕量級日文羅馬字輸入引擎（`JapaneseEngine`，狀態標籤：`半   日`）
- 新增模組 [`ruhi-core/src/japanese/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/japanese/mod.rs)，零外部龐大依賴，直接內建於 `ruhi-core` 與 `ruhi-tsf`：
  - **Hepburn / 訓令式雙相容五十音合成**：完整支援清音、濁音、半濁音、拗音、外來語音節、促音（雙子音與 `tch` ➔ `っ`）、撥音（`nn`/`n'` 與單一 `n` 自動收斂 `ん`）、長音（`-` ➔ `ー`）。
  - **最長前綴整句漢字自動合成**：輸入 `watashihanihongogasuki` 即時顯示並可直出 **`私は日本語が好き`**。
  - **完美沿用如喜 `↓` / `↑` 雙軌操作**：
    - 按 **`↓` 下方向鍵**：彈出同音漢字、平假名、片假名、羅馬字轉換選單（如 `nihongo` ➔ `1. 日本語`、`2. にほんご`、`3. ニホンゴ`）。
    - 按 **`↑` 上方向鍵**：彈出日文前綴補全與常用寒暄片語預測（如 `ariga` ➔ `1. ありがとうございます`、`2. ありがとう`）。

### 5. 多語系擴充開發指南與雙語技術藍圖文件
- 新增 [`docs/RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md)：詳述日文引擎架構，以及韓文（Hangul 2-Set Unicode 代數合成公式）、越南文（Telex/VNI 聲調轉導器）、泰文（WTT 2.0 四層堆疊檢核矩陣）、印地天城文（Abugida Virama 轉導器）與阿拉伯文（BiDi 連寫）之擴充規格。
- 新增 [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md) 與英文版 [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md)：供國際開源社群開發者直接串接歐洲與亞洲各國語言詞包。

---

## 二、打包檔案清單（Packaging File Manifest for `8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`）

請打包工作階段（`8f1c1e8a-cb2e-4e37-a40e-f2120dd3a708`）將以下新增與更新之核心檔案完整納入發佈包與安裝封裝中：

### 1. Rust 核心引擎與 TSF 服務原始碼與二進位產物
| 類別 | 檔案路徑 | 說明 |
| :--- | :--- | :--- |
| **最新編譯產物 (DLL)** | [`ruhi-tsf/target/release/ruhi_tsf.dll`](file:///C:/ATGprjs/ruhi/ruhi-tsf/target/release/ruhi_tsf.dll) | Windows TSF 輸入法核心動態連結庫（含雙軌方向鍵、自訂模式迴圈、日文模式與狀態標籤選單） |
| **最新編譯產物 (DLL)** | [`ruhi-core/target/release/ruhi.dll`](file:///C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll) | 如喜核心語言學與多語系引擎動態連結庫 |
| **新增核心模組** | [`ruhi-core/src/japanese/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/japanese/mod.rs) | 內建輕量級日文羅馬字／假名／漢字轉換與上下方向鍵預測引擎 (`JapaneseEngine`) |
| **修改核心模組** | [`ruhi-core/src/lib.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lib.rs) | 匯出 `japanese` 模組與 `JapaneseEngine` |
| **修改核心模組** | [`ruhi-core/src/engine.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/engine.rs) | 整合 `JapaneseEngine`、`enabled_keyboard_modes` 自訂循環、歐亞雙軌 `↓`/`↑` 方向鍵選字與單元測試 |
| **修改核心模組** | [`ruhi-core/src/lexicon/manager.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs) | 新增 `lookup_up_arrow_predictions`（注音頭碼／前綴快打與上文語境救援演算法） |
| **修改核心模組** | [`ruhi-core/src/cangjie/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/cangjie/mod.rs) & [`table.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/cangjie/table.rs) | 新增倉頡 `Z` 萬用碼查詢與 `↑` 前綴預測 |
| **修改核心模組** | [`ruhi-core/src/western/mod.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs), [`lexicon.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs), [`scoring.rs`](file:///C:/ATGprjs/ruhi/ruhi-core/src/western/scoring.rs) | 新增歐文 `↓` 變音符號／外來語展開與 `↑` 長字補全／拼寫救援／上文語境推論 |
| **修改 TSF 模組** | [`ruhi-tsf/src/service.rs`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) | 支援 `Ctrl/Shift + Caps Lock` 亞洲模式循環、`Caps Lock` 歐文模式雙軌方向鍵攔截、`JapaneseEngine` 組字與 `enabled_keyboard_modes` 熱重載 |
| **修改 TSF 模組** | [`ruhi-tsf/src/candidate_window.rs`](file:///C:/ATGprjs/ruhi/ruhi-tsf/src/candidate_window.rs) | 支援輸入座狀態標籤點擊彈出選單、`半   日` 標籤顯示，並依 `enabled_keyboard_modes` 過濾選單項目 |

### 2. 偏好設定中心 GUI 與組態檔（已同步至根目錄、`dist/` 與 `scripts/`）
| 類別 | 檔案路徑 | 說明 |
| :--- | :--- | :--- |
| **偏好設定中心 GUI** | [`settings_window.py`](file:///C:/ATGprjs/ruhi/settings_window.py)<br>[`dist/settings_window.py`](file:///C:/ATGprjs/ruhi/dist/settings_window.py)<br>[`scripts/settings_window.py`](file:///C:/ATGprjs/ruhi/scripts/settings_window.py) | 新增「日文羅馬字輸入 (日)」預設模式選項，以及「參與快速切換迴圈之亞洲模式 (`enabled_keyboard_modes`)」核取方塊清單 |
| **主設定檔** | [`config.json`](file:///C:/ATGprjs/ruhi/config.json)<br>[`dist/config.json`](file:///C:/ATGprjs/ruhi/dist/config.json)<br>[`scripts/config.json`](file:///C:/ATGprjs/ruhi/scripts/config.json) | 包含 `"enabled_keyboard_modes": ["hsu", "japanese"]` |
| **GUI 狀態設定檔** | [`settings_config.json`](file:///C:/ATGprjs/ruhi/settings_config.json)<br>[`dist/settings_config.json`](file:///C:/ATGprjs/ruhi/dist/settings_config.json)<br>[`scripts/settings_config.json`](file:///C:/ATGprjs/ruhi/scripts/settings_config.json) | 同步 GUI 預設勾選狀態 |

### 3. 技術規格與多語系開發指南文件
| 類別 | 檔案路徑 | 說明 |
| :--- | :--- | :--- |
| **亞洲文字擴充指南** | [`docs/RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_ASIAN_LANGUAGES_EXTENSION_GUIDE.md) | 自訂切換迴圈、內建日文引擎與韓／越／泰／印／阿文擴充開發規格書 |
| **多語系擴充指南 (中)** | [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE.md) | 歐亞雙軌方向鍵與多語系詞包擴充指南（繁體中文版） |
| **多語系擴充指南 (英)** | [`docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_MULTILINGUAL_EXPANSION_GUIDE_EN.md) | Multilingual & Cross-Script Expansion Guide (English Edition) |
| **語言學與 AI 調校規格** | [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///C:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md) | 語言學防呆護欄、頭碼分析與 AI 語意調校規格書 |

---

## 三、打包前注意事項（Notes for Packaging Session）

1. **複製最新 `ruhi_tsf.dll` 至 `dist/`**：
   - 若目前 Windows Explorer / 應用程式正載入舊版 `dist/ruhi_tsf.dll`，打包腳本在覆寫 `dist/ruhi_tsf.dll` 前，請先將鎖定中的舊檔重新命名為 `dist/ruhi_tsf.dll.old`（或呼叫 `hot_reload.ps1` 邏輯），再將 `ruhi-tsf/target/release/ruhi_tsf.dll` 與 `ruhi-core/target/release/ruhi.dll` 複製至 `dist/`。
2. **確認 `config.json` 之預設循環清單**：
   - 出廠預設 `"enabled_keyboard_modes": ["hsu", "japanese"]` 已寫入根目錄、`dist/` 與 `scripts/` 下之 `config.json`，打包時請保留此欄位。
