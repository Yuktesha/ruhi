# RuhI Input Method - Agent Documentation & Translation Guidelines (agent.md)

## 1. 核心定位與願景 (Core Vision)
本文件為 RuhI (如喜) 通用智慧型輸入法之專案管理規範。RuhI 的設計核心是為了「解決問題」，而非單純的形式追求。所有技術說明與文件生成，必須以「簡單、直觀、降低使用者與測試者認知負載」為最高原則。

## 2. 嚴格禁止事項：去「中二病」污染 (Prohibited Vocabulary)
底層 Agent 在進行程式碼修改、技術說明、對話回覆或錯誤日誌（Changelog）生成時，禁止使用網路仙俠小說、修真迷因或誇大之神怪修辭。

應立即替換下列特定詞彙：
* ❌ 禁止使用「奪舍」、「奪舍災難」、「奪舍死循環」
  👉 必須一律改用：【劫掠】、【語意劫掠】、【全域權重污染】或【系統死鎖】。
* ❌ 禁止使用「純陽語意防禦盾」或類似玄幻包裝
  👉 必須一律改用：【二階語意防禦機制】或【上下文語境防護】。
* ❌ 禁止使用「打地鼠古法」
  👉 必須一律改用：【單字權重手動調整】或【治標不治本的權重偏置】。

## 3. 核心技術名詞標準化定義表 (Standardized Technical Lexicon)
當 Agent 分析詞庫矩陣與演算法 Bug 時，必須精準對接自然語言處理（NLP）之標準科學術語：

| 原中二病文案詞彙 | PM 規範標準技術名稱 | 核心工程定義與行為準則 |
| :--- | :--- | :--- |
| **全域連鎖奪舍災難** | **全域連鎖劫掠災難**<br>(Global Unigram Hijacking) | 用戶為修復特定句子而強行調高單字基礎權重（Global Unigram Prior），導致該字在其他正常上下文中產生強行替代與污染的副作用。 |
| **負向抑制地雷** | **過度抑制邊界錯誤**<br>(Boundary Over-Suppression) | 採用粗暴、一刀切的負向字詞過濾規則，導致系統在處理合法的無詞界跨詞連寫（Scriptio Continua）長句時產生誤判或阻斷。 |
| **缺乏長詞凝聚力** | **複合長詞凝聚力缺失**<br>(Loss of Lexical Cohesion) | 三字以上之文化複合名詞未及時沉澱為正規專有詞庫，導致演算法在拆分解析多音字/破音字時失去錨點。 |

## 4. 寫作風格與文體規範 (Tone and Style Guide)
* **直接給予答案**：技術回覆、PRD 生成或錯誤提示，必須在第一段、第一句話直接點出核心工程問題，**粗體**標註關鍵參數與演算法實體。
* **短句與主動語態**：捨棄冗長且帶有 meta 評論的修飾詞，使用清晰、短小、主動語態的句子。
* **結構化呈現**：全面採用 Markdown 標題、 punchy（短小精悍）的項目符號，或對比清晰的表格。

## 5. Agent 本地模型優化任務指引 (Local AI Engine Guidance)
當使用本地純綠版 llama (`llama.cpp` / `llama-server`) 或 Gemma 4 模型進行詞庫矩陣分析時，Agent 應專注於：
1. 設計**動態語境快取機制**，平滑化單字先驗機率，防止全域語意劫掠。
2. 計算複合詞之內部凝聚力，自動將長詞沉澱至正規用戶詞庫。

## 6. 漢語計算語言學架構與 AI 語意調校強制參照規範 (Canonical Linguistic Spec)
任何遠端 AI（Google Gemini、OpenAI、Anthropic Claude）或本地 AI（Google Gemma、Llama.cpp、Ollama）在執行如喜語意調校工作室（`scripts/semantic_tuner.py` 與 `scripts/ai_semantic_consultant.py`）之對照句診斷、詞庫增補與轉移規則提煉時，**必須強制讀取並遵行專案白皮書**：
* 📄 **[`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)**：定義如喜四層語意解碼架構（Layer 0 因式分解槽位 `patterns_user.jsonl`、Layer 1 封閉類句法構式 `transitions.rs`、Layer 2 二階條件轉移 `transitions_user.tsv`、Layer 3 複合詞凝聚力與古音隔離）、六大選字病灶分類學（Type I ~ Type VI）與 AI 雙階段混合智慧管線 JSON Schema。

