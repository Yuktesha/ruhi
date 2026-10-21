# 如喜輸入法 (RuhI) 評估語料庫 (`ruhi_lexicon/eval/`)

本目錄用於客觀衡量盲打準確率（CER／整句命中率）、規則消融實驗（`--ablation`）與平均結構化感知器權重校準（`--calibrate`）。

## 1. 語料不公開

`dev.tsv` 與 `test.tsv` **不納入公開儲存庫**（已列入 `.gitignore`），公開儲存庫只保留：

- 評估工具 `ruhi-core/src/bin/ruhi_eval.rs`
- 不含句子原文的彙總報告 `eval_reports/*.json`

原因：語料大部分擷取自第三方出版之中文譯本（見第 2 節），其著作權屬原出版單位，本專案未取得再散布授權。

## 2. 實際來源（如實記載）

| 來源 | 說明 |
|---|---|
| `RuhOS/corpus/bilingual_parallel/baba_bilingual_parallel.tsv` | P.R. Sarkar（Ananda Marga）著作之中文譯本平行語料，**為主要來源** |
| `RuhOS/tools/tranStyler/profiles/default_dashi/corpus_triplets.tsv` | 工作區自有譯校語料 |
| 工作區 `*.md` 技術文件 | 自有文件，構成 `tech_doc` 類別 |

> 先前版本的說明誤記為「教育部《重編國語辭典修訂本》CC BY-ND 3.0 TW 與自有文件」，與實際不符，特此更正。教育部辭典僅用於辨識成語（`moe_idiom_context` 分類依據），並未擷取其例句。

## 3. 已知限制

- **領域偏斜**：`dev.tsv` 約 10.5% 的句子含宗教哲學詞彙（如「上主、至上、靈性、解脫」），不能代表一般使用者的日常輸入分布。
- **分類以關鍵字判定**：五大類別（`daily_dialogue`、`tech_doc`、`literary_narrative`、`civic_public`、`moe_idiom_context`）以關鍵字分流，標籤不精確。
- `tech_doc` 類別取自 Markdown，混有表格與標記殘片，CER 偏高的部分原因在語料本身。

## 4. 本機重建

在具備上述來源檔的工作區執行（固定種子 `20261021`，70/30 切分，共 5,200 句）：

```powershell
cd ruhi-core
cargo run --release --bin ruhi_eval -- --build-corpus
```

## 5. 盲測集隔離規則

- 規則開發、消融與校準**只能**使用 `dev.tsv`；任何人與 AI 代理不得讀取 `test.tsv` 的句子內容。
- `ruhi_eval --set test` 只輸出彙總指標，程式強制遮蔽句子文字；**每個發布版本只評估一次**。
- 報告預設不含句子原文；`dev` 集需要錯誤分析時可加 `--with-text`，輸出檔請放在未入庫的位置。
