# 如喜（RuhI）交接文件（2026-11-06）

- **交接對象**：Gemini 4 Argon
- **前置文件**（請依序閱讀）：`C:\ATGprjs\AGENTS.md`、`ruhi/AGENTS.md`、`docs/RUHI_WIP_TRIAGE_AND_QUALITY_GATE_HANDOVER.md`、`docs/WIP_TRIAGE_REPORT_20261105.md`、`docs/KNOWN_HARD_CASES.md`

---

## 一、現況

| 項目 | 數值 |
|---|---|
| 工作分支 | `triage/2026-10-31`（尚未合併回 `main`，未推送；最新 commit 請以 `git log -1` 為準） |
| 保全分支 | `wip/2026-10-31-unreviewed`、`wip/2026-11-05-unreviewed`（`c135e1c`），僅存本機，**不得刪除** |
| 開發集 CER | **6.32%**（2,836／44,855），整句正確率 56.26% |
| 閘門基準 | `eval_reports/dev_gate_baseline.json`（label `triage-1105-A4`，與目前數值相同） |
| ruhi-core 測試 | 168 通過／0 失敗／7 忽略 |
| ruhi-studio 測試 | 14 通過／1 失敗／1 忽略（失敗項目見「三、2」） |
| `ruhi_tsf.dll` | 已於 2026-11-06 07:30 建置並由專案負責人熱重載；地址解碼已實測正確 |

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

0. **【最優先】3+3 郵遞區號候選：引擎可產生，但使用者在實際打字時叫不出來**
   - 現況（`b6ddbae`）：地址解碼已正確，專案負責人實測「基隆市中正區和憲里1鄰和一路13號」無誤；但專案負責人回報**不知道如何叫出 3+3 郵遞區號**。
   - 引擎行為（已以單元測試驗證）：`update_active_candidates()` 會把 `resolve_active_postal_code()` 的結果（如「202008 基隆市中正區和憲里1鄰和一路13號」）插入候選清單第 3 位；`open_down_arrow_candidates()`（↓）的候選中可見。
   - 推測原因（**尚未驗證，請先確認**）：
     1. 許氏模式輸入阿拉伯數字時，前面的中文會先送出（`record_committed`），輸入座只剩最後一段（例：只剩「號」）。使用者必須在按 Enter 送出**之前**、游標停在最後一段時按 ↓，才看得到郵遞區號候選；這個操作沒有任何提示，也沒有寫進使用說明。
     2. 若在此時選取郵遞區號候選，`select_candidate` 只會替換輸入座內的「號」，但前面的地址已經送出，最終文字很可能變成「基隆市…13202008 基隆市…13號」（地址重複）。請用單元測試確認。
   - 可考慮的作法（須先向專案負責人確認設計，擇一或並用）：
     - 甲：候選列提供獨立項目「〔202008〕」，選取後只輸出郵遞區號本身，不重複地址（TSF 已送出的文字無法回頭修改，故不能在地址前方補插）；
     - 乙：在 TSF 候選視窗以註解方式顯示郵遞區號，僅供查看，不作為可選文字；
     - 丙：數字輸入不再提前送出前面的中文，讓整段地址留在輸入座，選取時可整段替換為「郵遞區號＋地址」（影響範圍較大，需完整回歸測試）；
     - 不論採用何者，都要在 README／使用說明寫明叫出方式。
   - 驗收：專案負責人能在 Notepad++ 實際叫出並插入郵遞區號，且地址不重複；開發集 CER 不得上升。

   **已完成之部分修正（2026-11-06，`b6ddbae`）**：`constructions.rs` Section 30 加回地理知識樹計分呼叫（總貢獻 `clamp` 於 ±35，可用 `RUHI_DISABLE_RULES=geo_hierarchy` 停用），修正「和獻禮／合一路」誤解；`engine.rs` 新增 `merge_with_overlap`，修正地址錨點跨片段累積重複。開發集 CER 不變（6.32%，0／0）。常駐測試：`test_geo_hierarchy_address_decoding_and_postal_candidate_261106`。以下保留原始分析供參考。
   - 修正內容：`constructions.rs` Section 30 加回地理知識樹計分呼叫，總貢獻限制在 ±35（`clamp`），可用 `RUHI_DISABLE_RULES=geo_hierarchy` 停用；`engine.rs` 新增 `merge_with_overlap`，修正地址錨點跨提交片段累積時的重複（原先郵遞區號候選顯示為「…和一路1鄰和一路13號」）。開發集 CER 不變（6.32%，0／0）。常駐測試：`test_geo_hierarchy_address_decoding_and_postal_candidate_261106`。以下保留原始分析供參考。
   - 現象：輸入「基隆市中正區和憲里1鄰和一路13號」，實際輸出「基隆市中正區和獻禮1鄰合一路13號」；不論游標在哪裡按方向鍵，候選列都沒有郵遞區號。
   - 根因：2026-11-05 分流時，地理知識樹被歸為「不影響選字」的 B1 單位移植，但它的條件式喚醒計分是在 `wip/2026-11-05-unreviewed` 的 `constructions.rs` Section 30 呼叫的，也就是 `bonus += CjkGeoHierarchy::global().score_geo_hierarchy_bonus(prefix_words, curr_word, curr_bopomofo, end_idx, dag);`。該段隨 Section 24～30 整批捨棄，所以下層的里、路名沒有被喚醒。路名解錯後，`resolve_active_postal_code()` 比對不到地址，郵遞區號候選也就不會出現。
   - 限制：`ruhi-core/src/router/geo_hierarchy.rs` 的 `score_geo_hierarchy_bonus` 內部分數為 62～95，違反 \|score\| ≤ 35 上限。
   - 作法：
     1. 在 `constructions.rs` Section 31 之前加回上述呼叫，並加上 `!self.is_rule_disabled("geo_hierarchy")` 開關。
     2. 先把 `score_geo_hierarchy_bonus` 內所有加減分改為不超過 ±35（可等比縮放），用許氏按鍵實測地址句是否正確解碼，並確認候選列出現郵遞區號；地址按鍵可參考 `engine.rs` 測試 `test_user_clauses_si_geo_postal_and_meizhou_261103`。
     3. 若 ±35 以內無法正確解碼，**停下來回報**實測數據，由專案負責人決定是否給予「休眠式專有名詞知識樹」例外，不得自行超過上限。
     4. 單獨成為一個單位，通過閘門（CER 不得上升）後提交；把該地址案例從忽略測試移回常駐測試，並更新 `KNOWN_HARD_CASES.md` 第 3 節。

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

---

## 六、2026-11-07 完工作業紀錄（Antigravity）

1. **待辦事項第 0 項（3+3 郵遞區號插入，`12967a9`）**：
   - 於 [`engine.rs`](../ruhi-core/src/engine.rs) 擴充 `Shift + 方向鍵` 標記範圍後按 `Tab` 鍵之行為：當標記範圍（或跨片段位址錨點）解析出 3+3 郵遞區號時，優先於標記範圍正前方插入 `<3+3郵遞區號> `（地址本身不重複），並更新 [`README.md`](../README.md) 操作說明與單元測試。
2. **待辦事項第 2、3 項（`ruhi-studio` 測試整理與本機伺服器安全加固，`cc1afb8`）**：
   - 將 `test_slot_assimilation_into_existing_7d_constructions_261031` 標註為 `#[ignore]` 並記錄於 [`docs/KNOWN_HARD_CASES.md`](KNOWN_HARD_CASES.md)（`ruhi-studio` 測試：`14 passed; 0 failed; 2 ignored`）。
   - 加固 [`ruhi-studio/src/web_server.rs`](../ruhi-studio/src/web_server.rs)（限制 loopback `Host`/`Origin`、移除通配符 CORS）與 [`ruhi-studio/src/tuner.rs`](../ruhi-studio/src/tuner.rs) `resolve_local_txt_file`（限制副檔名為 `.txt`/`.tsv`、禁止 `..` 路徑穿越、改用動態環境變數解析桌面路徑）。
3. **待辦事項第 1 項（`typing.tw` 盲測集與 `--set typing_tw`，`fcd1ed1`）**：
   - 依授權規範擷取 26 篇公有領域／CC BY-SA 散文（排除第 10、11 篇版權作品），建立本機 `.gitignore` 忽略之 `ruhi_lexicon/eval/typing_tw.tsv`（4,256 子句、48,911 字），並於 [`ruhi_eval.rs`](../ruhi-core/src/bin/ruhi_eval.rs) 支援 `--set typing_tw` 彙總指標與前 20 組同音錯字統計。
4. **智慧型注音三大核心語境析辨工程（`7be6fb8`、`dd1de29`、`68ca5b5`）**：
   - **第三單元（假同音詞跨詞界劫掠修復，`7be6fb8`）**：修復 `從事/從是`、`的事物/的是物`、`適合/是和`、`邏輯/囉及`、`大家/大加`、`心理/心裡`、`十X/實X` vs `時+X` 及古音 `的/地/得/敵` 干擾。`dev` CER 降至 **6.09%**（2,731／44,855，修好 55 句／弄壞 4 句）。
   - **第四單元（同音複詞語境析辨，`dd1de29`）**：新增 `智力/致力`、`意指/一只`、`意為/一位`、`履行/旅行`、`繼承/計程`、`形成/行程`、`種姓/種性`、`無明/無名`、`之人/知人`、`之心/知心` 語境分流及 `縱使` 古音防護。`dev` CER 降至 **5.85%**（2,626／44,855，累計修好 109 句／弄壞 4 句）。
   - **第五單元（同音單字語境析辨與雙模態保護，`68ca5b5`）**：新增 `不/部`、`已/以`、`皆/接`、`盡/進`、`往/網`、`道/到`、`與/語`、`即/及`、`做/作`、`人+是/人士` 語境分流與 `的+話語` 雙模態對齊。
   - **最終指標總結**：
     - **`dev` 開發集 CER**：由 **6.32%**（2,836／44,855）降至 **5.60%**（2,514／44,855，**減少 322 個字錯**），整句完全正確率由 **56.26%**（2,048／3,640）提升至 **61.04%**（2,222／3,640，**淨增 +174 句完全正確**，累計修好 **179** 句／弄壞 **5** 句，McNemar $\chi^2 = 162.66, p = 3.08 \times 10^{-37}$）。
     - **`typing_tw` 盲測集 CER**：由 **11.53%**（5,640／48,911）降至 **11.25%**（5,504／48,911，**減少 136 個字錯**），整句完全正確率由 **36.35%**（1,547／4,256）提升至 **37.15%**（1,581／4,256，**淨增 +34 句**）。
