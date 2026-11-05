# 如喜（RuhI）交接文件（2026-11-06）

- **交接對象**：Gemini 4 Argon
- **前置文件**（請依序閱讀）：`C:\ATGprjs\AGENTS.md`、`ruhi/AGENTS.md`、`docs/RUHI_WIP_TRIAGE_AND_QUALITY_GATE_HANDOVER.md`、`docs/WIP_TRIAGE_REPORT_20261105.md`、`docs/KNOWN_HARD_CASES.md`

---

## 一、現況

| 項目 | 數值 |
|---|---|
| 工作分支 | `triage/2026-10-31`，HEAD `36ccd77`（尚未合併回 `main`，未推送） |
| 保全分支 | `wip/2026-10-31-unreviewed`、`wip/2026-11-05-unreviewed`（`c135e1c`），僅存本機，**不得刪除** |
| 開發集 CER | **6.32%**（2,836／44,855），整句正確率 56.26% |
| 閘門基準 | `eval_reports/dev_gate_baseline.json`（label `triage-1105-A4`，與目前數值相同） |
| ruhi-core 測試 | 167 通過／0 失敗／7 忽略 |
| ruhi-studio 測試 | 14 通過／1 失敗／1 忽略（失敗項目見「三、2」） |
| `ruhi_tsf.dll` | 已於 2026-11-06 00:26 建置（含所有 ruhi-core 修改），尚待專案負責人執行 `hot_reload.ps1` |

2026-11-05 分流結果與逐單位數據，請見 `docs/WIP_TRIAGE_REPORT_20261105.md`。新增之語境分岔規則位於 `ruhi-core/src/router/constructions.rs` 的「Section 31」（`ctx31-1`～`ctx31-4`），語意類別位於 `ruhi_lexicon/data/morpheme_ontology.json`（bit 45～57）。

---

## 二、驗收標準與鐵則（沿用 2026-11-05 修訂版）

1. **唯一交付標準**：pre-commit hook（`scripts/quality_gate.ps1`）通過，也就是 ruhi-core 測試全過，且 `ruhi_eval --gate` 判定 PASS。另外，**CER 不得上升**（即使只多 1 個字錯，閘門判 PASS 也不保留）。
2. 新增規則必須通過 `python scripts/lint_scoring_rules.py`：
   - \|score\| ≤ 35；
   - 語境條件只能使用 `morpheme_ontology.json` 的語意類別，或 1～2 字功能詞，不得使用三字以上句子片段。
3. 只有在 CER **下降**時才更新基準，作法是先執行 `cargo run --release --bin ruhi_eval -- --init-gate-baseline --baseline ../eval_reports/dev_gate_baseline.json --commit <label>`，再與修改一起提交。
4. 往返句集（`ruhi-studio` HRZD）只用來找錯誤類型，**不要求零差分**，也不得為單一句子寫規則。
5. 不得讀取 `ruhi_lexicon/eval/test.tsv`；不得使用 `--no-verify`；不得修改單字無上下文權重；不得啟動 GUI；不得修改 `docs/RUHI_LONGKAN_TECHNICAL_DISCLOSURE.md`。
6. 每完成一個單位，就回報原始數據（CER、修好／弄壞句數），再進行下一個。

---

## 三、待辦事項（依優先順序）

1. **【需專案負責人同意後才能開始】typing.tw 盲測集**（修訂版第五節）
   - 擷取前先確認使用條款與 `robots.txt`。語料存於 `ruhi_lexicon/eval/typing_tw.tsv`，並加入 `.gitignore`，不得寫入任何入庫檔案。
   - 為 `ruhi_eval` 新增 `--set typing_tw`，只輸出彙總指標。
   - 交付物：句數、字數、CER、整句正確率，以及錯誤最多的前 20 組同音字。typing.tw 的 CER 只用來觀察，不設零誤差目標；新規則仍以開發集閘門為準。
2. **【待專案負責人決定】** `ruhi-studio` 測試 `test_slot_assimilation_into_existing_7d_constructions_261031` 在 HEAD 就已失敗，原因是它依賴 10/31 捨棄的模板 `meteorological_subject_yu_continuous_aspect_7d_array`。要改為 `#[ignore]` 或修改測試，須先徵詢。
3. **Studio 本機伺服器安全修正**（評鑑報告 P0，尚未獲指派，須先徵詢）：
   - `ruhi-studio/src/web_server.rs` 回應帶 `Access-Control-Allow-Origin: *`，也沒有檢查 `Origin`／`Host` 或 token；
   - `tuner.rs` 的 `resolve_local_txt_file` 接受任意路徑，且寫死了 `C:\Users\Yuktesha\Desktop`。
4. 合併 `triage/2026-10-31` 回 `main`，並推送（須專案負責人確認）。
5. 其他建議（未獲指派，勿自行展開）：
   - TSF `service.rs` 有 294 處 `lock().unwrap()`；
   - 主詞庫改用 mmap；
   - 引入詞級 n-gram 語言模型；
   - `constructions.rs` 改為資料驅動。

---

## 四、工作技巧與已知陷阱

- **單位評測流程**：套用修改 → `cargo test --release --lib`（於 `ruhi-core`）→ `cargo run --release --bin ruhi_eval -- --gate --baseline ../eval_reports/dev_gate_baseline.json` → 比對 `eval_reports/local/dev_gate_latest_sentences.json` 與 `dev_gate_baseline_sentences.json`，算出修好／弄壞句數。單次約 3 分鐘；提交時 hook 會再跑一次，約 4 分鐘。
- **規則消融不必重新編譯**：設定環境變數 `RUHI_DISABLE_RULES=ctx31_meizhou,ctx31_sheng,...` 後執行閘門即可。新規則請比照加上 `!self.is_rule_disabled("<名稱>")`。
- **許氏按鍵編碼**：每個音節依 `RuhiEngine::bpm_to_hsu_full` 的對照表轉換，聲調鍵為一聲＝空白、二聲＝`d`、三聲＝`f`、四聲＝`j`、輕聲＝`s`。例：「可以」＝`kgfef`、「感謝」＝`gmfceej`。
- **PowerShell 陷阱**：
  - `[IO.File]::ReadAllText` 等 .NET 方法不吃目前目錄，必須用絕對路徑；
  - 含中文的 `Select-String`、`git diff` 輸出常出現亂碼，請改用 Python，並設定 `$env:PYTHONIOENCODING='utf-8'`；
  - 檔案多為 CRLF，以字串取代時要注意換行符。
- **pre-commit hook 需要 Git 的 `sh.exe`**：若出現 `NtCreateDirectoryObject ... 0xC0000022`，表示執行環境無法啟動 sh。此時請回報，不要用 `--no-verify` 繞過。
- `.last_bg_telemetry_spawn` 是未追蹤的雜項檔，請勿提交。

---

## 五、專案負責人手動驗證清單

見 `docs/WIP_TRIAGE_REPORT_20261105.md` 第 4 節（8 項）。由專案負責人執行 `hot_reload.ps1` 後自行驗證，Agent 不得代為啟動 GUI。
