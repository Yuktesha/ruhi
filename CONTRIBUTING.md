# 🤝 如喜（RuhI）輸入法開源社群貢獻指南 (Contributing Guide)

感謝您對 **如喜（RuhI）通用智慧型輸入法** 的關注與支持！如喜致力於打造極致手感、毫秒級響應、跨語言、尊重隱私且具備計算語言學智慧的現代輸入底座。

* **💬 官方討論與使用者交流社群**：[**「如喜通用智慧型輸入法」LINE 社群**](https://line.me/ti/g2/iyLVoHye-Ho49K9SzQBaftTgFzej7stw9S1jSw?utm_source=invitation&utm_medium=link_copy&utm_campaign=default)（歡迎隨時加入回報實戰同音選字案例或討論架構）

為了維護專案架構的純淨度、演算法的優雅性與整體穩定度，所有參與貢獻的開發者均必須恪守以下準則。

---

## 🧠 最高架構鐵律：計算語言學思維，嚴禁「單字權重打地鼠」

在中文輸入法的演算法中，**「單字權重打地鼠（Unigram Weight Whack-a-Mole）」是摧毀系統平衡的最大元凶**。凡是提交 PR 涉及選字或字詞排序調整者，必須遵循以下禁忌與規範：

### 1. 嚴禁事項 (Strict Prohibitions)
* **嚴禁隨意調高/調低字典中的單字無上下文權重**：
  * 嚴禁為了讓特定句子中的某個字排在前面，就擅自將 `理`、`裡`、`視`、`是`、`月`、`越` 等單字調為 1050、1100、1200 等。
  * 單字權重是無上下文的全域先驗機率（Global Unigram Prior）。隨意改動單字權重必然引發連鎖副作用（調高「視」破壞「的是」、調高「理」破壞「硬碟裡」、調高「裡」破壞「理想」），導致系統性崩潰！
* **嚴禁暴力硬灌超高分特例與極端互斥懲罰**：
  * 嚴禁在詞庫中動輒塞入 2000~2400 分的超高權重單詞，或在轉移矩陣中硬塞 `±70` ~ `±95` 的極端負向懲罰。
  * 人工硬編碼會形成「語意黑洞」，扭曲整體的自然概率分佈，破壞 Viterbi 動態規劃的平滑度。

### 2. 智慧來自「結構與關係」，而非「孤立分數」
漢語是高度依賴**語法構式（Constructions）**與**二階轉移（Transitions）**的語言：
* **方位後置詞構式（Localizers）**：如「裡、外、內、中、上、下、前、後、旁、之中、裡面」。這是 `名詞 + 方位詞` 的語法結構。解決「硬碟裡」的唯一正確方法是建立 `[名詞/實詞] ➔ [方位詞]` 的轉移引力，而非把單字「裡」調大！
* **關聯副詞構式**：如 `越 A 越 B`。解決「越弄越大」的唯一正確方法是建立 `越 ➔ 實詞` 與 `實詞 ➔ 越` 的路徑轉移，而非去動「越」與「月」的先驗權重。
* **繫詞結構**：如 `的 + 是`（「奇妙的是」、「重要的是」）。解決此類問題應透過 `的 ➔ 是` 的強語法轉移，而非調動「是」或「視」的單字權重。
* **數量詞與時間量詞**：如 `[量詞] + 鐘頭/小時/天`。解決「一個鐘頭」應建立數量詞到時間量詞的引力，並保持長詞凝聚力，絕不可任由引擎拆碎成「中」+「投」。

### 3. 三大強制工程規範 (Mandatory Protocols)
1. **以 Layer 1 語法構式與 Layer 2 二元轉移為唯一正道**：
   * 凡遇選字排序不佳，請先分析其**語法成分（構式、詞性、前後文搭配）**。
   * 一律在 `ruhi-core/src/router/transitions.rs` 擴充通用語法規則（`GrammarRule`），或在轉移矩陣中加入合理自然分值（20.0 ~ 35.0）的二階搭配。
2. **長詞凝聚力與正統 Viterbi Trellis 保證**：
   * 必須確保 Viterbi 演算法為保留多路徑假設的正統 Trellis 狀態機（不可退化為一維貪婪塌陷）。
   * 嚴格守護字典既有合法詞的整體凝聚力，防止演算法將合法詞切碎為無意義單字碎片。
3. **測試先行（TDD）與盲測集封存驗證義務**：
   * 凡修復任何選詞/字詞優先序問題，**必須在 `ruhi-core/src/engine.rs` 補上對應的注音拼弦單元測試**。
   * **盲測集防污染鐵律**：**任何開發者與 AI 代理在撰寫或調整語意規則時，絕對禁止讀取 `ruhi_lexicon/eval/test.tsv` 的具體句子內容。** 日常規則開發、錯誤分析與參數校準僅允許針對 `ruhi_lexicon/eval/dev.tsv` 進行；`test.tsv` 僅限於透過 `cargo run --release --bin ruhi_eval -- --set test` 取得最終聚合指標（CER / SER / p-value），嚴禁針對測試集句子進行人工對答案修補。
   * 唯有在 `cargo test` 100% 通過、`ruhi_eval --set dev` 無淨退化，且完全沒有人工單字加權副作用時，方可合併！

---

## 🪟 Windows 與指令碼開發規範

1. **全面揚棄 `.bat` / `.cmd` 批次檔**：
   * 本專案嚴格禁止提交任何 `.bat` 或 `.cmd` 檔案。
   * 業務邏輯一律使用 Rust / Python CLI 實現，Shell 包裝一律使用 PowerShell。
2. **PowerShell 腳本強制 UTF-8 BOM 鐵律**：
   * 所有新增或修改之 `.ps1` 腳本，**必須強制儲存為「帶 BOM 的 UTF-8」（UTF-8 with BOM）**，以防 Windows 預設之 PowerShell 5.1 解析時退回 ANSI 導致字串中斷。
   * 提交前請執行原生語法檢查：
     ```powershell
     powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "& { Get-Command -Syntax <path>.ps1 }"
     ```
3. **嚴禁將 GUI 視窗程式以背景守護進程 (Background/Daemon) 啟動**：
   * 本機驗證與自動化測試一律使用 headless 單元測試（`cargo test`），嚴禁在非互動工作階段中啟動視窗軟體。

---

## 🇹🇼 繁體中文在地化守則 (Localization & Terminology)

* 本專案以**台灣正體中文**為主要語言標準。
* 請使用台灣在地技術用語（如：程式碼、專案、伺服器、記憶體、演算法、快取、預設、介面、列印、效能最佳化、資料庫等），嚴禁混入非本土之科技支語。

---

## 🚀 提交 Pull Request (PR) 流程

1. **Fork 本儲存庫** 並建立獨立 feature 分支（例如 `feature/add-locative-rule`）。
2. 在本地完成修改，並在 `ruhi-core/src/engine.rs` 加入完整測試案例。
3. 執行全量測試，確保全部通過：
   ```bash
   cd ruhi-core
   cargo test
   ```
4. 提交清晰具體的 Commit 訊息（建議遵循 Conventional Commits 格式，如 `feat(router): add locative construction rule for spatial nouns`）。
5. 發起 PR，並在 PR 說明中附上語法構式分析與測試通過截圖。
