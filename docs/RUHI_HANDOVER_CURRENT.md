# 如喜（RuhI）跨世代開發交接文件（常駐現況與待辦）

- **適用對象**：所有後續接手本專案的高階規劃主腦與高速執行代理（**不綁定特定模型版號**）。
- **前置閱讀順序**：[`C:\ATGprjs\AGENTS.md`](file:///c:/ATGprjs/AGENTS.md) ➔ [`AGENTS.md`](file:///c:/ATGprjs/ruhi/AGENTS.md) ➔ 本文件 ➔ [`docs/KNOWN_HARD_CASES.md`](file:///c:/ATGprjs/ruhi/docs/KNOWN_HARD_CASES.md)。
- **歷史參考**：前階段交接紀錄見 [`docs/RUHI_HANDOVER_20261106_ARGON.md`](file:///c:/ATGprjs/ruhi/docs/RUHI_HANDOVER_20261106_ARGON.md)（其中已標註 `2026-11-07` 完成之里程碑）；架構理論請參閱 [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///c:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)。

---

## 一、系統現況快照（最後更新：2026-11-21）

| 項目 | 目前狀態／實測數值 |
|---|---|
| Git 分支與基底 | `main`（`2c66361` Phase 2A 結構防拆碎與搭配防護 ＋ 本版 `v0.8` 局部上下文線上學習 `Cache LM`，前序 N-gram 基準為 `4f58d39`／`adf12f1`） |
| `dev` 開發集品質閘門（`ruhi_eval --set dev --gate`） | **`PASS`**（`gate_passed: true`）；CER **`3.88%`**（較 `4f58d39` 鎖定基準 `3.89%` 再降 `0.0067` 個百分點；相對於停用 `ngram_lm` 之 `4.43%` 基線，McNemar $\chi^2 = 56.6123, p = 5.35 \times 10^{-14}$）；總字元錯誤 **`1,742 / 44,855`**（較基準 `1,745` 淨減 **`-3`** 字錯）；整句完全命中 **`2,593 / 3,640`**（**`71.24%`**，較基準 `2,591` 淨增 **`+2`** 句）；五大文體穩定（`civic_public` `2.53%` / `daily_dialogue` `3.17%` / `literary_narrative` `4.04%` / `moe_idiom_context` `2.74%` / `tech_doc` `7.84%`）；基準線 [`eval_reports/dev_gate_baseline.json`](file:///c:/ATGprjs/ruhi/eval_reports/dev_gate_baseline.json) 鎖定於 `4f58d39` |
| 盲測驗證集（`test.tsv` / `typing_tw.tsv` 一次性 A/B 對照，`4f58d39`） | **`test.tsv`**（1,560 句／19,058 字）：CER **`4.68%`**（`891` 字錯、完全匹配 **`66.22%`** = 1,033 句、平均 `8.176 ms`／P50 `6.458 ms`），較停用 `ngram_lm` 基準（`5.45%`、`1,039` 字錯、`61.22%`）淨減 **`-148`** 字錯（相對降幅 **`14.2%`**）；**`typing_tw.tsv`**（4,256 句／48,911 字）：CER **`9.58%`**（`4,685` 字錯、完全匹配 **`43.49%`** = 1,851 句、平均 `9.537 ms`／P50 `6.502 ms`），較停用 `ngram_lm` 基準（`11.29%`、`5,524` 字錯、`37.03%`）淨減 **`-839`** 字錯（相對降幅 **`15.2%`**） |
| 統計與線上個人化語言模型 | 1. **靜態 N-gram（`ngram_zh_tw.bin`）**：商用安全授權 9 組台灣繁體語料（合計 `70,765,616` 字），`RLNG0001` 唯讀 `mmap` 格式 **`26.88 MB`**（詞表 `141,897`、`WB=1,808,458`、`CB=750,000`、`CT=1,600,000`）。<br>2. **線上學習快取（[`cache_lm.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/cache_lm.rs)）**：於不改動 COM ABI 下實作 `OnceLock` 旁表上下文快取與被拒路徑短期負向懲罰（`%APPDATA%\Ruhi\user_cache_lm.tsv` 原子持久化，正強化半衰期 30 天／負懲罰 7 天）。 |
| 雲端算力工具（Google Colab CLI） | **`colab` CLI 已就緒**（`C:\Users\Yuktesha\.local\bin\colab.exe`，預設 `--auth oauth2`），專供大規模口語語料清洗、`train_ngram_lm.py`（`v0.9` 詞三元 `RLNG0002`）及 `v1.0` 輕量神經重排模型訓練與量化；運行完畢須以 `colab stop` 回收並確認 `0` 閒置工作階段 |
| `ruhi-core` Release 單元測試 | `--lib`：**`257 passed; 0 failed; 8 ignored`**（含 `test_phase2a_live_typing_261121_seven_sentences` 與 `test_v08_contextual_cache_lm_online_learning_and_isolation`；`#[ignore]` 為 [`docs/KNOWN_HARD_CASES.md`](file:///c:/ATGprjs/ruhi/docs/KNOWN_HARD_CASES.md) 紀錄之困難案例）；整合測試 `--test person_names_and_y_cases`：**`4 passed; 0 failed; 1 ignored`** |
| `ruhi-tsf` 單元測試（`--lib`） | **`27 passed; 0 failed; 0 ignored`**（嚴格維持 Layer B `RuhiTextService` 既有 COM ABI 雜湊與 `LAYER_B_ABI_EPOCH = 2` 不變，支援免登出 `hot_reload.ps1`） |
| `ruhi-settings` 單元測試 | **`8 passed; 0 failed; 0 ignored`** |
| 規則靜態檢查（`scripts/lint_scoring_rules.py --full`） | **0 項違規**（全量掃描 **`69,115`** 行，覆蓋 `router/`、`western/` 與設定資料表） |
| 核心詞庫與轉移資料同步 | [`lexicon_core.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_core.tsv) **`226,932`** 筆詞條；[`morpheme_ontology.json`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/morpheme_ontology.json) **107 類／1,894 語素**；[`transitions_user.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv) 已清理 1,326 筆 `A100_Gemma2_27B` 退化／OOV 列（剩餘 **`10,965`** 筆主動規則，`23,523` 總行數，四份資料目錄 SHA-256 `f672ac60...` 完全一致）；[`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 嚴守凍結政策 |
| 正式版發布路線（`0.8 → 0.9 → 1.0`） | **`v0.8`**（局部上下文線上學習 ＋ Phase 2/3 打包）➔ **`v0.9`**（台灣口語語料擴充 ＋ `RLNG0002` 詞三元 ＋ N-Best A\* 純 Rust 語篇重排）➔ **`v1.0`**（Colab 訓練之 `15M–25M` INT8 選配 IPC 神經重排服務 ＋ 公開對照基準，不設期限壓力，期間歡迎彈性釋出 `0.8.x`～`0.9.x`） |

---

## 二、近期已完成之關鍵工程項目摘要

1. **自訂詞庫與自訂範本統一互動體驗**：
   - `user_dict.tsv` 全面支援自動補齊「注音頭碼」與「自訂縮略碼」（支援以 Tab 或連續空白分隔），並於 `templates.tsv` 擴充星期幾格式變數與 `date`／`time` 快速呼叫。
   - 統一單按 `` ` `` 與連按兩下 `` `` `` 之行為：引導進入「自訂詞庫」／「自訂範本」，待機等候使用者直接輸入頭碼或縮略碼；若遺忘縮略碼可按 `Tab` 鍵展開完整候選列，並可透過輸入座按鈕一鍵開啟設定／詞庫／範本檔編輯。
   - 候選座鍵盤行為一致化：許氏注音模式統一支援 `asdfghjkl` 選字、左右方向鍵與 `Tab` 鍵操作。
2. **TSF 輸入座長句穩定性與 48 音節無標點架構升級**：
   - 修正輸入座長句組字時預編輯注音碼消失及延遲升高問題，並修正 `Shift + Enter` 換行後尾端文字遺失問題。
   - [`ruhi-tsf/src/service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L5273-L5283)：`compute_joint_reeval_window` 聯合重估視窗擴展為 **`JOINT_REEVAL_MAX_LEFT = 28`**、`JOINT_REEVAL_MAX_RIGHT = 20`（合計 **48 音節**）。
   - [`ruhi-core/src/router/viterbi.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs#L324-L465)：動態規劃路徑回溯上限 `MAX_LOOKBACK` 由 `12` 提升至 **`24`**（約涵蓋 48 音節左鄰上下文）。
   - [`ruhi-tsf/src/candidate_window.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/candidate_window.rs#L4088)：輸入座單行最大字數選單新增 **`48`** 選項。
3. **實境同音詞與高維構式消歧（`constructions.rs` `(r)`–`(y7)` 與康熙筆畫姓名學）**：
   - 新增 [`onomastics.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/onomastics.rs) 康熙筆畫與姓氏析辨，以及 `(r)`–`(w)` 修復：`稍後／稍候`、`以下／已下`、`城市／程式`、`有趣／有去`＋`醫診所`、`編制／編製／編織` 等。
   - 補入輕聲詞條 `句子\tㄐㄩˋ ˙ㄗ\t478`（雙份詞庫同步），並於 [`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L8470-L8865) 實作 **`(y1)`–`(y7)`**：
     * `(y1)` `句子`（輕聲／三聲）vs. `鋸子` 與 `鋒利 vs. 風力`（保護 `拿一把鋒利的鋸子`）；
     * `(y2)` `按需指派`（保護 `案件／方案／需要`）；
     * `(y3)` `還滿常書寫` vs. `還滿長的`；
     * `(y4)` `舞弄` vs. 非地址 `五＋弄` 過擬合中和；
     * `(y5)` `長尾` vs. `常委`；
     * `(y6)` `滿樹`／`桐花` 植群纖維叢（保護 `同花順／同時／同意`）；
     * `(y7)` 跨 22 詞位非局域量詞—中心名詞閉包（穩定區辨 41／48 音節長句 `一隻穿著花襯衫搖頭晃腦舞弄長尾的小花貓漫不經心逍遙自在的走在滿樹盛開桐花的樹蔭底下曬著溫暖的陽光` 與對比句 `一支寫起來很順手的鋼筆`）。
4. **「以便新對話接續」（`心對話 ➔ 新對話`）消歧與詞網格跨度對比保護（2026-11-15 完成）**：
   - 擴充 [`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L8008-L8110) `(r)` 構式右側人機互動與對話抽象中心語（`"對話"`、`"視窗"`、`"分頁"`、`"話題"`、`"對話框"`、`"檔案"` 等），並實作詞網格跨度不變量防同音旁路（`shatters_heart_compound`、`has_lattice_comitative_yu`）與 `(r2)` `[彼此/互相] × [交] × [心]` 因式分解防護，守護 `內心對話`、`真心對話`、`彼此交心對話` 與 `靜下心來與心對話`。
   - 於 [`transitions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs#L2967-L2975) `copula_shi_indefinite_np` 補入複合繫詞 `"這是"`／`"那是"`／`"也是"`／`"還是"`／`"不是"`，防止 `"這是一場"` 被 `"試 -> 一場"` 碎裂為 `"這試一場"`。
   - 新增雙鍵盤模式單元測試 [`test_xin_duihua_disambiguation_and_contrast_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)（標準大千與許氏 8 組正反例共 16 次斷言全數通過）。
5. **多鍵盤模式輸入座 `Tab` 聯想擴充與歐文輸入法三大支柱升級（2026-11-15 完成）**：
   - `OnceLock<Arc<WesternLexicon>>` 程序級單例共享與寫入時複製（Copy-on-Write）個人詞彙插入（[`lexicon.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs)、[`mod.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)）。
   - **支柱一（`Tab` 前綴補完、變音符號救援與語境拼字勘誤）**：修正前綴命中優先於模糊拼字救援之排序，擴充英／法／德／西外來語變音與常見誤拼表並保留原輸入大小寫型態（`TokenCasing`），整合 `rank_contextual_confusables` 與搭配詞前綴加權（[`mod.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)、[`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs)）。
   - **支柱二（泛歐文空白後下詞聯想與行首縮排防護）**：新增零跨句界 1–3 詞搭配表 [`collocations.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/collocations.rs)（涵蓋英／法／德／西高頻搭配），支援詞尾空白後按 `Tab` 零長度 `(cur, cur)` 插入下詞候選，並保護行首純空白按 `Tab` 維持原生縮排。
   - **支柱三（雙層非破壞性空白鍵自動勘誤與單鍵還原）**：實作 `evaluate_space_autocorrect`（Tier 1a 無歧義誤拼與縮寫撇號補齊、Tier 1b 封閉雙詞／三詞語法與變音規則），對程式碼識別字、路徑、縮寫、全大寫、歧義實詞及德語 `"ich bin im"` 嚴格豁免；支援單次 `Backspace` 或 `Ctrl+Z` 原子化一鍵還原並釘選原詞防重複觸發，且以靜態雜湊表保存單步還原狀態以嚴格維持 `RuhiTextService` 既有 27 欄位 `#[repr(C)]` COM ABI 不變量。
   - **字形模式（倉頡、行列 30、四角號碼）組字碼中段 `Tab` 前綴補完**：新增 [`table.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/cangjie/table.rs) `lookup_with_prefix` 與 [`engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) `collect_shape_prefix_tab_candidates`，於 TSF 組字碼未送出階段按 `Tab` 即時展開前綴字根／數碼候選（同時保留已成字後按 `Tab` 觸發中文後繼詞聯想）。
   - 新增單元測試 [`test_multimode_tab_and_european_three_pillars_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)、`western::tests` 四項支柱測試與 [`test_tsf_com_abi_invariance_and_western_three_pillars_261115`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs)。
6. **語境感知 `Tab` 聯想與歐文正交句法補完升級（2026-11-15 完成）**：
   - **全詞典計分前綴檢索（解除靜態高權重遮蔽）**：[`lexicon.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs) 新增 `WesternLexicon::lookup_prefix_scored` 搭配自適應長度門檻，涵蓋 `data/lexicon_english.tsv` 全體 `37,615` 筆詞條，修正 `insert_word` 重複覆寫累計 `prefixes` 計數之缺陷，並擴充內建核心 `-ing`／分詞／日常詞彙，解決僅依靜態詞頻截取前 8 筆導致 `going`（`56.69`）、`glad`、`guys` 遭截斷的問題。
   - **正交句法構式投影與雙向右文前瞻**：[`collocations.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/collocations.rs) 實作 `ClauseFrame` 句法解析與 `score_candidate_in_clause_context`，支援 `Wh + BE + Subject` 進行式、不定詞與介系詞 `to` 區辨、`PerfectHave`、情態／`DO`-support、動詞名物化補語、副詞透明穿透、右鄰語境前瞻、同子句實詞提取與法／德／西語構式框架，使 `Where are you g` ➔ `going`、`Today, we are going to s` ➔ `see`、`Where have you g` ➔ `gone` 與 `their are` ➔ `there` 精準居於首位。
   - **會話詞彙／雙詞記憶與引擎／TSF 貫通**：[`mod.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)、[`engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 與 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) 新增上限 `96` 筆之 `recent_words`／`recent_bigrams`（採靜態旁表 `WesternSessionMemory` 保存，嚴格維持 `WesternEngine` 既有 3 欄位二進位佈局相容性以利執行期免登出熱切換；由 `record_committed` 寫入且不隨單次組字重置），整合 `lookup_contextual_up_arrow_candidates(prefix_before_word, token, next_context)`，並於 `VK_TAB`／`open_associative_candidates` 在歐文模式下略過中文聯想查詢；另已將兩份 `hot_reload.ps1` 之 `.rdata` 掃描上限由 `0x400000`（4 MB）調升至 `0x1000000`（16 MB）以容納成長後之 `4.79 MB` `.rdata` 區段。
   - **中文單字動詞、可離合動賓與同子句語意聯想**：[`manager.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs) `lookup_associative_continuations` 擴充單字動詞動賓搭配、最多 4 字透明體貌／數詞／量詞橋接與量詞語意親和（如 `洗個` ➔ `澡`、`吃了一頓` ➔ `飯`、`開了一場` ➔ `會`、`喝杯` ➔ `咖啡`）及同子句領域詞彙提升（`醫院...看` ➔ `醫生/醫師/診/病`、`電影院...看` ➔ `電影`）。
   - 新增驗證單元測試 [`test_western_contextual_syntactic_and_session_prediction_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)、[`test_chinese_and_european_contextual_tab_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 及 [`test_tsf_com_abi_invariance_and_western_three_pillars_261115`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) 擴充驗證。
7. **Windows 11 Notepad（`RichEditD2DPT` / `NotepadTextBox`）多行與 `Ctrl+Enter` 送出游標結尾定位修復（2026-11-15 完成）**：
   - 於 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L143-L240) 建立宿主換行協定三分類 `HostNewlineKind`（`Win32EditCrlf`、`RichEditCr`、`GenericLfChunked`）及優先透過 `ITfContext::GetActiveView() -> GetWnd()` 判讀之 `context_host_newline_kind`，修正 RichEdit 家族原先誤轉 `\r\n` 導致 ACP 長度與文件實際增長不符並觸發 `TS_E_INVALIDPOS` 而游標滯留段首之問題。
   - 於 [`edit_session.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/edit_session.rs#L58-L128) 實作共享輔助函式 `commit_into_range_and_collapse_to_end`，透過預置 `(TF_GRAVITY_BACKWARD, TF_GRAVITY_FORWARD)` 重力、`ITfRangeACP::GetExtent/SetExtent` 結尾位置校驗及 `ShiftEnd` 異常夾限重試，確保 `Shift+Enter` 多行送出與 `Ctrl+Enter` 換行送出後游標精確停駐於結尾；新增 [`test_notepad_richedit_multiline_and_ctrl_enter_acp_invariants_261115`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L8747) 驗證通過。
8. **無組字狀態 `Ctrl + V` 與全體軟體通用快捷鍵直通放行修復（2026-11-15 完成）**：
   - 於 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L2020-L2465) 重構 `OnTestKeyDown`、`evaluate_test_key_down_with_mods`（[`L4800`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L4800)）與 `OnKeyDown` 路由順序：將 `is_ctrl` 處理明確置於 `is_english_mode` 之前並補齊 `!is_ctrl && !is_alt && !is_win` 防護，解決部分宿主無視 `OnTestKeyDown` 回傳值逕呼 `OnKeyDown` 時，歐文模式誤攔截 `Ctrl + V` 等組合鍵插入字元之問題。
   - 新增 [`is_passthrough_non_typing_vk`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L1079-L1119)、`is_win_modifier_down` 與 `resolve_ctrl_symbol_combo`：無條件放行獨立修飾鍵、`Win+*`、`Alt+*`、`Ctrl+Alt+*`、`F1`–`F24`、媒體與瀏覽器功能鍵；於無組字緩衝區狀態下在全體 10 種輸入模式優先放行 `Ctrl+A..Z`／數字（含 `Ctrl+1`、`Ctrl+-`、`Ctrl+/`）／方向與翻頁鍵／`Insert`／`Delete`／`Shift+Insert`／`Shift+Delete`／`Ctrl+Insert` 與標準大千半形小鍵盤數字，並將密碼控制項檢查移至 `Shift+Space` 與 `Caps Lock` 之前。
   - 於組字緩衝區啟用期間補齊 `paste_text_into_desk`（[`L4741`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L4741)，`\r\n` 標準化與原子化選區貼上）、`normalize_clipboard_copy_text`（寫回 `CF_UNICODETEXT` 統一轉 `\r\n`）、`delete_forward_in_desk`（[`L4768`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L4768)，支援 `Ctrl+Delete` 刪詞）、`Shift+Insert`／`Ctrl+Insert`／`Shift+Delete`／`Ctrl+Home`／`Ctrl+End` 與選區刪除前 `push_undo()` 快照；新增 [`test_shortcut_passthrough_and_input_desk_combos_all_modes_261115`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L9075) 驗證通過。
9. **歐文縮略語與定義句型雙向對偶投影及開放詞彙形態對齊（2026-11-15 完成）**：
   - 於 [`collocations.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/collocations.rs#L1907-L2959)、[`mod.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs) 與 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L3416-L3448) 實作 `SentenceItem` 句子邊界與帶點縮寫（`A.I.`／`U.S.`）掃描、`CANONICAL_ACRONYMS` 因式分解標準縮略語表（內含 `require_upper` 小寫日常詞豁免保護如 `it`／`who`／`ram`／`rag`／`spa`）、`analyze_acronym_context`（以 `is_potential_acronym_token` 防止 `is` 等繫詞遮蔽主詞 `TSF`）、`lookup_acronym_candidates_with_context`（正向定義句型 `means`/`stands for`/`is short for`/`refers to`/`denotes`/`represents`/`(` 多槽連續展開、TitleCase／ALL CAPS 形態保持、以及括號雙向對偶與動態開放詞彙首字母合成 `Responsive User Interface (` ➔ `RUI`）、`score_candidate_with_acronym_frame`（未登錄縮略語非末槽形容詞修飾語優先、複數名詞降權），以及空白／`(` 後即時展開 `lookup_acronym_after_space`，同時嚴格維持 `WesternEngine` 既有 3 欄位二進位佈局與 `RuhiTextService` 27 欄位 COM ABI。
   - 新增驗證單元測試 [`test_western_acronym_definitional_and_parenthetical_projection_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs)（含完整 `37,615` 詞 `data/lexicon_english.tsv` 實測 `"Where are you going to eat your dinner? AI means art"` ➔ 第 1 候選 `artificial`、第 2 候選 `artificial intelligence`，以及未登錄縮略語 `AL means art` 形容詞槽位排序）。
10. **全 10 模式無組字按鍵行為與「`` ` ``」三模式「`~`」切換一致化修復（2026-11-15 完成）**：
   - 於 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) 統一全體 10 種鍵盤模式於無組字緩衝區（`!has_buffer`）狀態之按鍵路由：`OnKeyDown` 與 `evaluate_test_key_down_with_mods` 對非 `Ctrl+Caps Lock` 之 `Ctrl+*`／`Ctrl+Shift+*` 組合鍵一律直通放行；`resolve_ctrl_symbol_combo` 限定僅於 `has_buffer && !is_english_mode` 觸發；`OnKeyDown` 對 `Shift+0x6E` 與 `Shift+Numpad`（`0x60..=0x69`）補齊與 `evaluate_test_key_down_with_mods` 一致之直通保護；於 `english` 模式無組字時單按「`` ` ``」同樣進入 `🔖 自訂詞庫`，並於 `snapshot_key_routing_flags` 改以 `tilde_triggers_mode`（`!tilde_is_punct`）確保歐文模式下按「`~`」（`Shift`＋`` ` ``）可正確切換模式；同步擴充 [`test_shortcut_passthrough_and_input_desk_combos_all_modes_261115`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) 涵蓋全 10 模式與 `RuhiTextService` 27 欄位 COM ABI 驗證。
   - 於 [`engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 統一連按「`` ` ``」輪替切換之三種模式（`inline_longkan_active`、`EngineMode::TemplateSelect`、`EngineMode::LexiconSwitch`）於尚未輸入觸發或編碼字元前按「`~`」（`Shift`＋`` ` ``）之行為：一律重設計數器與狀態並呼叫 `open_quick_num_mode` 進入「🔢 數字快打模式」（`EngineMode::QuickNum`）；同步更新 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs) `compute_coach_hint_and_preview` 引導提示文字，並新增單元測試 [`test_unified_backtick_three_modes_shift_backtick_to_quicknum_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 通過驗證。
11. **外國人友善泛歐語音韻近似引擎、開源宣告式擴充規範與中文同音構式擴充（2026-11-15 完成）**：
   - **表驅動泛歐語音韻近似與前綴補全引擎**：新增 [`phonetics.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/phonetics.rs)（[`PhoneticProfile`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/phonetics.rs#L36)、`G2pClusterRule`、`PhoneticConfusionPair`、[`encode_phonetic_forms`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/phonetics.rs#L454)、`pronunciation_distance`、`WesternPhoneticIndex` 及 [`lookup_phonetic_candidates`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/phonetics.rs#L722)），涵蓋靜音輔音群、`ph`/`-tion`/`-ceipt`/`-eau`/`ai` 雙母音變體、`I`/`Y` 近音等效及 `L↔R`、`V↔W` 等 L1/L2 負遷移音素代價模型，並以 `Weak<WesternLexicon>` 靜態旁表快取嚴格維持既有二進位結構佈局。
   - **字典現代詞元與子句語法框架整合**：於 [`lexicon.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs#L256) 補入現代高頻詞幹並提供唯讀 [`iter_words`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/lexicon.rs#L256)；於 [`western/mod.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs#L1577-L1593) 協調 `COMMON_MISSPELLINGS`（含 `gramer -> grammar`）、長字子序列加權抑制（防 `resit -> repository` 遮蔽精確音韻匹配 `receipt`）與 `Please`/不定詞/`Be`/冠詞等語法框架，並通過雙字典模式測試 [`test_western_l2_phonetic_approximation_and_contextual_rescue_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/mod.rs#L2898)。
   - **開源四層宣告式擴充規範**：確立現行十大鍵盤架構收斂穩定之方向，撰寫 [`docs/EXTENSION_ARCHITECTURE_SPEC.md`](file:///c:/ATGprjs/ruhi/docs/EXTENSION_ARCHITECTURE_SPEC.md) 定義「按鍵轉導狀態機、詞彙形態、表驅動音韻近似與雙向語境構式」標準擴充介面。
   - **中文實境同音構式擴充**：於 [`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 擴充規則 [`3.`（`勘誤/刊物`）](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L3222-L3275) 與 [`3.`（`各/個 + 語種/民族/單位`）](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L8386-L8411)，新增 [`(y8)`（`助益/注意`）](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L8987-L9028) 與 [`(y9)`（`均/軍` 並修正 `shatters_prev` 首字邊界檢核以防 `"援軍"` 遮蔽 `"人員"`）](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs#L9030-L9101)，通過雙模式單元測試 [`test_errata_benefit_and_jun_homophone_disambiguation_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs#L17065)。
12. **單一鍵盤模式獨立掛載解耦、全 10 模式反斜線字面轉義與跨對話自動閃避防護（2026-11-15 完成）**：
    - **解除許氏注音核心強制綁定，支援任一單一鍵盤模式獨立掛載**：重構 [`engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)、[`config_sync.rs`](file:///c:/ATGprjs/ruhi/ruhi-settings/src/config_sync.rs)、[`main.rs`](file:///c:/ATGprjs/ruhi/ruhi-settings/src/main.rs)、[`candidate_window.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/candidate_window.rs) 與 [`service.rs`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs)。設定中心將歐文（`"english"`）納為第一級獨立可掛載模式並新增「`🎯 僅單獨掛載此模式`／`🎯 僅留此模式`」與「`✖ 移出`」（允許安全移除 `"hsu"` 並禁止刪除最後唯一保留模式）；掛載多模式時完整循環自動附帶 `"english"`，僅掛載單一模式時（無論倉頡、行列、日文、韓語諺文或歐文）嚴格維持單一模式而不強制退回許氏注音，並將工作列右鍵選單與 `Shift + ↓` 跨查選單嚴格過濾為已掛載模式；同步於 `capslock_switch` 新增 **`"none"`**（還原作業系統原生 Caps Lock 大寫鎖定）與系統匣選單項。
    - **軟體工程反斜線字面轉義序列（`\`` ➔ `` ` ``、`\\` ➔ `\`、`\~` ➔ `~`）跨全 10 模式**：不更動 `RuhiEngine` 欄位亦嚴格維持 `RuhiTextService` 27 欄位 COM ABI（以 `OnceLock<Mutex<HashSet<usize>>>` 旁表與 TSF `backslash_escape_pending` 協調），於全數 10 種鍵盤模式下輸入 `\`` 直接輸出半形 `` ` `` 而不觸發「`🔖 自訂詞庫`」、輸入 `\~` 直接輸出半形 `~` 而不觸發 `tilde_toggle_mode`、輸入 `\\` 直接輸出單一 `\`；一般 Windows 路徑如 `\path` 維持原樣不誤改；新增雙側單元測試 [`test_single_mode_mounting_and_decoupling_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 與 [`test_backslash_escape_sequences_all_10_modes_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)。
    - **六項日常打字同音消歧與正反對比防護**：雙份核心詞庫補入 `掛載／卸載`（保持位元組同步），於 [`lattice.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/lattice.rs) 與 [`manager.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs) 解除 `"諺文"` `moe_unattested2` 跨句剪枝屏蔽，並於 [`transitions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs) 與 [`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 實作 `(aa)`–`(ae)` 等構式：正確辨析「`把剛剛想到的寫進來` vs `開心地寫`」、「`選擇只掛載一種鍵盤模式` vs `主宰一切`」、「`韓語諺文` vs `檢驗文件`」、「`我這時候又想到一件事情` vs `向右轉`」、「`我啦` vs `瘌痢頭`」與「`再麻煩您了` vs `正在麻煩您`」，並通過 [`test_six_homophone_disambiguations_and_contrastives_261115`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)。
    - **原生 Antigravity 跨對話修改碰撞閃避機制**：於工作區根目錄配置 [`hooks.json`](file:///c:/ATGprjs/.agents/hooks.json)、[`workspace_collision_guard.py`](file:///c:/ATGprjs/.agents/hooks/workspace_collision_guard.py)（使用 Windows 原生 `msvcrt.locking` 鎖檔與獨立檔案鎖租約，按 `subproject_root` 隔離並對唯讀命令、Artifact 與父對話派工子代理豁免）及 4 項單元測試 [`test_workspace_collision_guard.py`](file:///c:/ATGprjs/.agents/hooks/test_workspace_collision_guard.py)。
13. **第二階段全系統健檢與 Wave 1–3b 架構進化（`e0362a8`–`0e48a67`，2026-11-16 完成）**：
    - **漢語同音消歧、詞網解鎖與自訂人名投射（Wave 1A `53bde24` ＋ Wave 2b `9e9733e`）**：
      * 修正 [`geo_hierarchy.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/geo_hierarchy.rs) 無行政／街道錨點即施予強分數之誤傷並將 `transitions_user.tsv` `718` 筆超限權重收斂至 `±35` 以內；清除 `862` 筆 `>= 4` 字詞庫短語與 `188` 筆負向遷移行。
      * 於 [`morpheme_ontology.json`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/morpheme_ontology.json) 啟用 `is_rare_impostor_fast` 阻斷低頻生僻字穿透 `ContentWord`、擴充 `MorphemeMask256` 位元 `75..=91` 收斂「在／再」、「是／事／適／試／視／式」、「為／位」與單句有生性、於 [`lattice.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/lattice.rs) 安全解鎖現代 `< 90` 雙字詞並建立 `OnceLock` 聯想前綴旁表（[`manager.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/lexicon/manager.rs)），還原 12 個多音字單字與 `一律`／`一部` 詞條，同步於 [`constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 拆解 `260` 槽位 `OnceLock<WordDispatchTable>` 與細粒度消融子開關，推動 `dev` CER 由 `5.22%` 降至 **`4.61%`**（`2,340 -> 2,068`）。
      * **使用者自訂人名分類與尊稱／親屬稱謂投射**：實作 `PersonNameForm`（單／雙姓 × 單／雙名）、`classify_user_person_name`、`format_person_note_tag`、`upsert_entry` 與 `UserOnomasticRegistry`，支援將「曹大十」等自訂人名自動標記並投影後續行文中的「曹先生／小姐／老師」、「大十」與「大十哥／兄／弟／姊／姐／妹」，並於 Wave 3b 補齊分號複合標記與單姓雙字高頻常用詞（`>= 300`）P2 攔截防護。
      * **實境八句新增消歧測試全數通過（標準大千與許氏雙模式）**：含「在風力強勁的海邊…鋸出一句…」、「按需指派所有的按鍵…方案獎勵」、「我個人還滿常書寫…滿長的句子」、「…五弄裡的巷弄…舞弄肢體」、「…常委養了一隻長尾貓」、「…滿樹桐花…一手同花順好牌…同時同意…」、「我用一支…鋼筆在紙上寫下…小花貓…」及「雖然其實是極小眾」（對比「市集」），詳見 [`person_names_and_y_cases.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/tests/person_names_and_y_cases.rs)。
    - **泛歐語多語種可選架構、`sa_am` 梵語獨立詞包與統一模式註冊表（Wave 1B `21e9754` ＋ Wave 2a `dc3dc2b`）**：
      * 將阿南達瑪迦羅馬拼音梵語詞條分離為獨立詞包 [`packs/pack_western_sanskrit.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/packs/pack_western_sanskrit.tsv)（`10,481` 筆），使核心 [`lexicon_english.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_english.tsv) 回歸清潔日常英語（`35,058` 筆），並建立可勾選之 `western_languages`（支援 `en`、`fr`、`de`、`es` 與 `sa_am`）。**出貨程式／範例組態預設維持 `["en"]`（`sa_am` 關閉），本機 6 份執行期組態檔維持 `["en", "sa_am"]` 開啟**。
      * 修復 [`sanskrit_am.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/western/sanskrit_am.rs#L4366) 變數遮蔽、`collocations.rs` `<= 35.0` 收斂與 Unicode 小寫、音素索引世代失效、`reset_buffer` I/O 節流與 CEFR 去偏斜／`48/64` 桶位保護；建立單一靜態 [`ModeRegistry`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs)（`ALL_MODES`）統一全體鍵盤模式顯示與挑選。
    - **TSF COM 安全邊界、多螢幕支援、`rud()` 介面與 Layer B 冷重啟收斂（Wave 1C `4fc297d` ＋ Wave 2a `dc3dc2b` ＋ Wave 3b `0e48a67`）**：
      * 修復負座標多螢幕哨兵 `CARET_UNPROBED = i32::MIN`、原地 Mutex 熱升級、`thread_local!` `DllForwardGuard`、`50 ms` 單鍵密碼欄消費權杖與延後按鍵日誌、Win32 `ToUnicodeEx`/`AltGr` 歐文鍵盤對映、前導 `/`/`.`/`_` 防護、`undo()` 區間釘選、`GDI` 執行緒快取與 `ruhi-settings` `rud()` 自適應排版。
      * Wave 3a（`ruhi_eval --set dev --calibrate`）於 Stage A/B 實測未能低於 `2,068` 錯字（`delta = 0`），依淨增益原則保留既有 `feature_weights.tsv` 並恢復全鏡像一致。
      * Wave 3b（`0e48a67` `[Cold-Restart Required]`）將 `BACKSLASH_ESCAPE_ENGINES` 靜態旁表內嵌為 `EscapePendingFlag(AtomicBool)`、補全 `impl Drop for WesternEngine` 與 `impl Drop for RuhiTextService`、升級 `undo_stack`/`redo_stack` 為 `VecDeque<EditorSnapshot>`、將 `CANDIDATE_STATE` 安全遷移為 `TLS_CANDIDATE_STATE`（同 STA 執行緒 `RefCell` 搭配 `CandidateWindow::drop` 重置），並於 [`compute_layer_a_abi_hash()`](file:///c:/ATGprjs/ruhi/ruhi-tsf/src/service.rs#L719) 納入 `LAYER_B_ABI_EPOCH = 2` 與 `needs_drop` 指紋，配合同步更新之 `hot_reload.ps1` 執行期守門。
14. **Phase 1（Phase 1A–1D）台灣正體商用安全統計 N-gram 底盤建置與維特比整合（`ea7a1d3`、`bb25909`、`4f58d39`，2026-11-21 完成）**：
    - **記憶體映射二進位引擎與授權合規語料訓練**：實作 [`mmap_io.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/mmap_io.rs)、[`ngram_lm.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/ngram_lm.rs)（`RLNG0001` 編譯與 16 位元量化讀取）、[`ruhi_ngbin.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/bin/ruhi_ngbin.rs)（與 `--probe` 十組對比診斷）及 [`train_ngram_lm.py`](file:///c:/ATGprjs/ruhi/scripts/train_ngram_lm.py)（移植 `production_s5` 斷詞並排除 `is_narrow_transparent_derived` 雙字代詞粘連問題）；於 Google Colab 完成 9 組商用安全台灣繁體語料（`70,765,616` 字）訓練並編譯為 `26.88 MB` `ngram_zh_tw.bin`（授權規範記於 [`THIRD_PARTY_LICENSES.md`](file:///c:/ATGprjs/ruhi/THIRD_PARTY_LICENSES.md)，同步於 [`package_release.py`](file:///c:/ATGprjs/ruhi/scripts/package_release.py) 與 `.gitignore`）。
    - **維特比三層邊界防護與均衡高精度校準（`v6`）**：於 [`transitions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/transitions.rs)、[`engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 與 [`viterbi.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs) 整合 `ngram_word_bi`／`ngram_char_bi`／`ngram_char_tri`，實作短詞與非中文前詞防護（Guard 1）、文言模式隔離（Guard 2）、人名邊界中性基線（Guard 3）及 `RUHI_DISABLE_RULES=ngram_lm` 優雅降級；經 [`ruhi_eval.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/bin/ruhi_eval.rs) `--calibrate --ngram-only` 鎖定既有 24 項規則權重並定案為 `(CT=0.2100, WB=0.3000, CB=0.1000)`，補齊 5 筆缺漏標準詞條並將 9 組薄邊際測試紀錄於 [`docs/KNOWN_HARD_CASES.md`](file:///c:/ATGprjs/ruhi/docs/KNOWN_HARD_CASES.md) 第四節；盲測 `test.tsv`（`5.45% -> 4.68%`）與外部 `typing_tw.tsv`（`11.29% -> 9.58%`）皆達成逾 14%–15% 相對 CER 降幅，確認無過擬合。
15. **Phase 2A 實測防拆碎修復與 `v0.8` 局部上下文線上學習（`2c66361` ＋本版 `Cache LM`，2026-11-21 完成）**：
    - **系統性防拆碎與構式邊界修復（`2c66361`）**：清理 [`transitions_user.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv) 1,326 筆 OOV／退化 `A100_Gemma2_27B` 列（同步至 4 份資料目錄），加入 `shatters_core_len2_at_start`、`shatters_strong_left_len2`（含 `!m.word.ends_with(ch)` 保護 `桌上`）與 `shatters_strong_v_le`（排除 `為了/除了/罷了` 等句尾誤觸發），擴充 `eval_functional_collocation_guards`（`func_copula_shijian_classifier` 與 `func_double_adv_jiu_zhi`）及 `slot_d287` 軟體前綴偵測，全部 7 句 10 處實測錯誤於 `test_phase2a_live_typing_261121_seven_sentences` 雙鍵盤模式全數通過，開發集 `dev.tsv` CER 再降至 **`3.88%`**（`1,742` 錯字，`-3` vs `4f58d39`）。
    - **`v0.8` 局部上下文線上學習（[`cache_lm.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/cache_lm.rs)）**：於不變動 `RuhiEngine`／`RuhiTextService` 二進位佈局下，透過 `OnceLock` 旁表掛載 `(left_ch, word, right_ch)` 三元與左右二元個人強化及被拒錯誤路徑短期負向懲罰（正向半衰期 30 天、負向半衰期 7 天、單表上限 `2,048` 筆），由 `record_revision` 與 `capture_sentence_revision_before_commit` 自動汲取（支援 1 字選字更正與 2–4 字跨界替換）並原子寫入 `user_cache_lm.tsv`，於 [`test_v08_contextual_cache_lm_online_learning_and_isolation`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 驗證「僅請著參 ➔ 謹請卓參」、「因為／他們原本舊址 ➔ 就只」且對 `緊急`、`看著`、`回到學校舊址` 零全域汙染。

---

## 三、後續優先待辦事項（`v0.8 → v0.9 → v1.0` 正式版路線圖）

> 里程碑以品質實證為準，不設固定日曆期限壓力；各階段之間視實測需求彈性釋出 `0.8.x`～`0.9.x` 中間版本。

### 1. **`v0.8` 階段（進行中）：個人化記憶與統計底盤封裝釋出**
- **已落地核心項目**：局部上下文線上學習 [`cache_lm.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/cache_lm.rs)（Contextual `Cache LM` ＋ 被拒路徑短期負向懲罰）已完成整合與單元／整合驗證，並嚴守零 COM ABI 異動（可透過 `hot_reload.ps1` 免登出更新）。
- **待辦清單**：
  - **Phase 2（詞庫／變調與維護清潔）**：檢視盲測統計彙總暴露之高頻通用誤差型態（如 `它／他`、`臺／台`）於非構式通用規則／詞庫層面之純化空間（嚴守 `constructions.rs` 凍結政策）。
  - **Phase 3（Windows 打包與一鍵安裝驗證）**：檢查並實測 [`scripts/package_release.py`](file:///c:/ATGprjs/ruhi/scripts/package_release.py)、[`scripts/hot_reload.ps1`](file:///c:/ATGprjs/ruhi/scripts/hot_reload.ps1) 與 [`一鍵安裝.ps1`](file:///c:/ATGprjs/ruhi/一鍵安裝.ps1) 納入 `ngram_zh_tw.bin` 後之完整打包、熱重載與實機輸入體驗，推進 `v0.8` 釋出。

### 2. **`v0.9` 階段（借重 Colab CLI）：純統計與搜尋引擎極限突破**
- **台灣本土口語與生活語料擴充**：於 [`train_ngram_lm.py`](file:///c:/ATGprjs/ruhi/scripts/train_ngram_lm.py) 納入手續乾淨（商用安全授權）之台灣對話／生活／論壇類型正體語料，並將書面語與口語語料分開加權，專攻日常長句準確率。
- **`RLNG0002` 詞三元（Word Trigram）升級**：利用 `colab` CLI 於雲端訓練並編譯高頻詞三元統計表至新格式 `RLNG0002`（預估唯讀 `mmap` 體積 `40–55 MB`）。
- **Forward-Viterbi + Backward-A\* N-Best 候選生成器**：基於 [`viterbi.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/viterbi.rs) 既有 `LatticeBackwardProfile.beta_score` 實作 Top-15～20 整句候選生成，先行搭配純 Rust 語篇重排序器（如 P2-2 句後代詞指代與同段主題一致性，改善 `它／他／她` 誤差），並消化 [`docs/KNOWN_HARD_CASES.md`](file:///c:/ATGprjs/ruhi/docs/KNOWN_HARD_CASES.md) 困難例句；可併同評估 **P2-4**（[`docs/EXTENSION_ARCHITECTURE_SPEC.md`](file:///c:/ATGprjs/ruhi/docs/EXTENSION_ARCHITECTURE_SPEC.md) `manifest.toml` 四層宣告式擴充）。

### 3. **`v1.0` 階段（跨世代正式版）：選配輕量神經重排序與客觀對照基準**
- **輕量神經語言模型（Colab CLI 訓練與量化）**：以高純度商用安全台灣正體語料於 Colab GPU 自訓 `15M–25M` 參數之 INT8 量化因果語言模型（約 `20–30 MB`）。
- **單例本機 IPC 背景推理架構（Local Sidecar）**：嚴格維持 `ruhi_tsf.dll` 純 Rust、`mmap` 低記憶體與瀏覽器 AppContainer 沙盒相容，不將神經推理框架直接連結進 DLL；改由獨立單例本機服務接收 `v0.9` 之 Top-N 候選並以「前綴樹批次計分 ＋ `15 ms` 嚴格逾時優雅回退」回傳重排分數（供硬體充裕使用者選配開啟）。
- **可重複公開對照基準**：建立與調參完全隔離之 200～300 句台灣日常長句同注音基準集，實證比對如喜 `v1.0` 與小麥／自然輸入法之字準率與整句命中率。

---

## 四、跨世代模型分派速查

- **單一獨立子代理（`invoke_subagent`）**：使用抽象能力層級參數（高速施工層 `Model: "flash"`、極輕量 `"flash_lite"`、深度推理 `"pro"` 或 `"inherit"`）。
- **多代理協作板（`swarm`）**：**先呼叫 `swarm(action: "models", all: true)` 動態查詢當下支援模型**，再按需求選派當代高速系列（如 `gemini-4-hydrogen-low`／`medium`／`high`／`xhigh` 或後續世代對應型號）。
- **知識圖譜**：新對話啟動時優先執行 `codebase-memory-mcp` 之 `list_projects`／`index_status` 確認 `C-ATGprjs-ruhi` 圖譜狀態（目前最新索引：**`4,495`** 節點、**`25,036`** 邊）。

---

## 五、新對話接續啟動提示詞（可直接複製貼上）

```text
請依序閱讀以下檔案以接手如喜（RuhI）輸入法專案之開發工作：
1. C:\ATGprjs\AGENTS.md
2. C:\ATGprjs\ruhi\AGENTS.md
3. C:\ATGprjs\ruhi\docs\RUHI_HANDOVER_CURRENT.md
4. C:\ATGprjs\ruhi\docs\KNOWN_HARD_CASES.md

先確認 codebase-memory-mcp 專案圖譜（C-ATGprjs-ruhi）與 git 工作樹現況，並遵守「高階規劃主腦診斷把關 ➔ 高速執行代理批次施工 ➔ 主腦最終複驗」規範（多代理協作板請先以 swarm models all=true 動態查詢當代高速模型系列）。

確認就緒後，請依序檢視 docs/RUHI_HANDOVER_CURRENT.md 第三節待辦事項（階段性 Git 盤點／基準線更新與實境同音詞持續優化），待完成任何修改後須執行 `lint_scoring_rules.py`、單元測試、`ruhi_eval --set dev --gate` 與熱重載部署，並簡要向我回報結果與基準數據。
```
