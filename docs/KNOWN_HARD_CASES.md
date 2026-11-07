# 如喜（RuhI）已知困難選字案例帳本 (`KNOWN_HARD_CASES.md`)

本文件依 [`docs/RUHI_WIP_TRIAGE_AND_QUALITY_GATE_HANDOVER.md`](./RUHI_WIP_TRIAGE_AND_QUALITY_GATE_HANDOVER.md) 第 4.3 節規範建立，記錄於 `wip/2026-10-31-unreviewed` 分流過程中，因對應之規則單位未通過開發集品質閘門（`ruhi_eval --gate`）而標記為 `#[ignore = "known hard case: ..."]` 之測試句子、輸入按鍵、預期輸出、目前實際輸出、嘗試過之規則做法與品質閘門量化數據。

---

## 1. 品質閘門攔截之規則單位與量化數據摘要

於 `triage/2026-10-31` 分支針對 `eval_reports/dev_gate_baseline.json`（基準 commit `dd8ea03`，`dev` CER `6.52%`、`2,924 / 44,855` 字元錯誤、整句完全正確率 `55.44%`、`2,018 / 3,640` 句）逐單位評測時，以下三個規則單位因造成開發集整體退步而依規範捨棄：

| 捨棄單位 | 檔案與嘗試做法 | 開發集 CER | 相對基準變化 | 修好句數 (`fixed`) | 弄壞句數 (`broken`) | 閘門攔截原因 |
|---|---|---:|---:|---:|---:|---|
| **Unit A-3b** | `ruhi_lexicon/data/transitions_user.tsv`（新增 277 條人工二元轉移，含部分 `\|score\|` 達 `48.0 ~ 55.0` 之高分轉移） | `6.54%` (`2,932 / 44,855`) | `+0.0178%` (`+8` 字錯) | `16` | `18` | `弄壞的句數 (18) 多於修好的句數 (16)` |
| **Unit A-3c** | `ruhi_lexicon/data/patterns_user.jsonl`（新增 116 條 Layer 0 硬性槽位覆寫規則，大量分數達 `55.0 ~ 68.0`） | `6.84%` (`3,069 / 44,855`) | `+0.3233%` (`+145` 字錯) | `31` | `76` | `整體 CER 退步 +0.3233%`、`civic_public +0.4934%`、`literary_narrative +0.4424%`、`tech_doc +0.5890%`、`弄壞的句數 (76) 多於修好的句數 (31)` |
| **Unit A-5a** | `ruhi-core/src/router/constructions.rs`（改動構式七「得 vs 的」、構式十一「在 vs 再」及構式十九 `C04`「地 vs 的」之廣泛前文條件） | `6.59%` (`2,956 / 44,855`) | `+0.0713%` (`+32` 字錯) | `44` | `79` | `整體 CER 退步 +0.0713%`、`civic_public +0.3553% (> +0.20%)`、`弄壞的句數 (79) 多於修好的句數 (44)` |

> **備註**：同批次中通過品質閘門並保留之單位（`Unit A-2`、`Unit A-3a`、`Unit A-4`、`Unit A-5b`、`Unit A-6`）已將開發集 CER 由 `6.52%`（`2,924` 字錯）降至 **`6.37%`（`2,857` 字錯，`-0.1494%`、減少 `67` 個字元錯誤，整句正確率升至 `56.18%`，`fixed = 28, broken = 1`）**，且原測試中之 34 組通過案例已獨立收錄於常駐測試 `test_kept_triage_units_a3_a4_a5b_a6_261031`。

---

## 2. 逐項困難案例明細

### 2.1 `test_user_three_pairs_plus_cheng_de_261031`（`ruhi-core/src/engine.rs`）

本測試共 11 組輸入，其中 6 組「任一輸入模式按右鍵」已通過並移入 `test_kept_triage_units_a3_a4_a5b_a6_261031`；以下 5 組因依賴已捨棄之 `Unit A-3c`（`patterns_user.jsonl`）、`Unit A-3b`（`transitions_user.tsv`）與 `Unit A-5a`（構式七改動）而列為困難案例：

| # | 許氏注音按鍵序列 (`keys`) | 預期輸出 (`expected`) | 目前實際輸出 (`actual`) | 競爭歧義點 | 嘗試過之做法與捨棄原因 |
|---|---|---|---|---|---|
| 1 | `xhfbyfcx rxjfyfbemjaxl meldlgs` | `我把輸入法變聰明了` | `我把書入髮辮聰明了` | `輸入法 + 變` vs. `書 + 入 + 髮辮`（跨詞界雙字詞「髮辮」先驗權重高於「法 + 變」） | `wip/2026-10-31-unreviewed` 於 `patterns_user.jsonl` 加入高分硬性模板並於 `lexicon_domain.tsv` 調權；惟 `patterns_user.jsonl`（Unit A-3c）在 `dev` 造成 `+145` 字錯（`broken = 76 > fixed = 31`），違反品質閘門。 |
| 2 | `bewfemfe ggscx rxjfyfdsvedmewjfk cjgafnefkmj` | `表演一個輸入法的奇妙方式給你看` | `表演一個輸入法的奇妙方是給你看` | `奇妙 + 方式` vs. `奇妙 + 方 + 是`（單字「是」與後接「給」之轉移競爭） | `wip/2026-10-31-unreviewed` 於 `patterns_user.jsonl`（Unit A-3c）及 `transitions_user.tsv`（Unit A-3b）加入形容詞後接「方式」強轉移，兩單位皆未通過 `dev` 品質閘門。 |
| 3 | `bewfemfejggjcx rxjfyfdsvedmewjfk cjgafnefkmj` | `表演一個輸入法的奇妙方式給你看` | `表演一個輸入法的奇妙方是給你看` | 同上（四聲 `jggj`「一個」變體） | 同上（Unit A-3b / Unit A-3c 未通過品質閘門）。 |
| 4 | `velfbk xhfjemfvydty xmdvlddgddxajbxjdxaj` | `請幫我檢查他完成得對不對` | `請幫我檢查塌完承德對不對` | `他 + 完成 + 得（ㄉㄜˊ dgd） + 對不對` vs. `塌 + 完 + 承德 + 對不對` | 二聲 `dgd`（`ㄉㄜˊ`）與前字 `vld`（`ㄔㄥˊ`）構成專名「承德」，導致前字「他 + 完成」被拆解為「塌 + 完」；`wip` 透過 `patterns_user.jsonl`（Unit A-3c）與構式七（Unit A-5a）壓制，惟兩者於 `dev` 均造成整體退步。 |
| 5 | `velfbk xhfjemfvydty xmdvlddsdxajbxjdxaj` | `請幫我檢查他完成得對不對` | `請幫我檢查他完成的對不對` | `完成 + 得（ㄉㄜ˙ ds） + 對不對` vs. `完成 + 的 + 對不對` | 輕聲 `ds`（`ㄉㄜ˙`）於動詞「完成」後接正反疑問補語「對不對」時，需區分補語標記「得」與定語／名物化助詞「的」；`Unit A-5a` 放寬構式七「得」觸發條件後，於 `civic_public` 造成 `+0.3553%` 退步（`broken = 79 > fixed = 44`）。 |

---

### 2.2 `test_no_archaic_de_and_final_boss_de_zai_disambiguation_261031`（`ruhi-core/src/engine.rs`）

本測試分為兩部分：
1. **第一部分（罕用古異體字 `嶳 / 㡳 / 埅 / 埊` 攔截）**：原測試預期該 4 字以權重 `5` 保留於候選清單最末段；依交接文件第 4.3 節規範，已改用 `rejected_readings.tsv` 帳本攔截罕用讀音（不直接改動詞庫權重），故候選清單不再出現該 4 個古異體字（已於常駐測試 `test_kept_triage_units_a3_a4_a5b_a6_261031` 第一節驗證通過）。
2. **第二部分（22 組對照句）**：其中 13 組已通過並收錄於 `test_kept_triage_units_a3_a4_a5b_a6_261031`；以下 9 組因依賴已捨棄之 `Unit A-5a`（構式七／十一／十九改動）與 `Unit A-3b` / `Unit A-3c` 而列為困難案例：

| # | 許氏注音按鍵序列 (`keys`) | 預期輸出 (`expected`) | 目前實際輸出 (`actual`) | 競爭歧義點 | 嘗試過之做法與捨棄原因 |
|---|---|---|---|---|---|
| 1 | `ty xmdvlddgddxajbxjdxaj` | `他完成得對不對` | `塌完承德對不對` | `完成 + 得(ㄉㄜˊ)` vs. `完 + 承德` | 見 2.1 第 4 項（依賴 Unit A-3c / Unit A-5a）。 |
| 2 | `ty xmdvlddsdxajbxjdxaj` | `他完成得對不對` | `他完成的對不對` | `完成 + 得(ㄉㄜ˙) + 對不對` vs. `完成 + 的` | 見 2.1 第 5 項（依賴 Unit A-5a 構式七改動）。 |
| 3 | `xmdvlddshnfxmdmaf` | `完成得很完美` | `完成的很完美` | `完成 + 得 + 很完美` vs. `完成 + 的 + 很完美` | `Unit A-5a` 擴充構式七雙音節動詞「完成」後接程度補語「得」之權重，但在開發集誤傷大量 `[雙音節動詞 + 的 + 程度修飾名詞短語]` 定語關係子句（`broken = 79`）。 |
| 4 | `gk vxflefdsz lewjdkfmjlef` | `剛處理的資料檔案裡` | `剛處理得資料檔案裡` | `剛處理 + 的 + 資料` vs. `剛處理 + 得` | 「處理」本身兼具動補觸發傾向，需精確依賴右側名詞中心語統計轉移而非硬編碼詞表；`Unit A-5a` 改動未通過閘門。 |
| 5 | `mkdmxjdsjey vumd` | `盲目地加權` | `盲目的加權` | `盲目 + 地 + 加權` vs. `盲目 + 的 + 加權` | 雙音節狀態形容詞修飾雙音節動作動詞（可能名物化）時，「地／的」在現代書面語高度重疊；`Unit A-5a` 修改構式十九 `C04` 後在 `civic_public` 造成大量定語「的」誤轉為「地」。 |
| 6 | `nijcen dsjeefcj` | `耐心地解釋` | `耐心的解式` | `耐心 + 地 + 解釋` vs. `耐心 + 的 + 解 + 式` | 依賴 `Unit A-3b`（`transitions_user.tsv`）與 `Unit A-5a`（構式十九 `C04`），兩者均未通過 `dev` 品質閘門。 |
| 7 | `zfcejdsjemfvyd` | `仔細地檢查` | `仔細的檢查` | `仔細 + 地 + 檢查` vs. `仔細 + 的 + 檢查` | 與第 5 項相同，雙音節動詞「檢查」兼具名物化用法（如「仔細的檢查」本身亦為合法名詞短語），強制轉為「地」會在開發集造成大量誤傷。 |
| 8 | `velfzijceyjly cumfdm jxl cumfzgd` | `請在下拉選單中選擇` | `請再下拉選單中選擇` | `請 + 在 + 下拉選單 + 中` vs. `請 + 再` | 祈使動詞「請」後接「在／再」時，預設搭配傾向「請再」，需跨越 3 個節點識別右界方位詞「中」；`Unit A-5a` 擴充構式十一跨距視窗後於開發集造成回退。 |
| 9 | `velfcem xmdvldagjcjzijckjcemj` | `請先完成測試再上線` | `請先完成測試在上限` | `先...再上線` vs. `在上限` | 「上線」與「上限」同音（`ㄕㄤˋ ㄒㄧㄢˋ`），且「在上限」在詞庫搭配中與前文產生跨界競爭；依賴 `Unit A-3b` / `Unit A-3c` 與 `Unit A-5a`。 |

---

### 2.3 `test_chinese_construction_grammar_completion_261031`（`ruhi-core/src/engine.rs`）

本測試共 21 組語句，其中 **15 組已通過**（透過保留之 `Unit A-5b` 尾部構式二十三～二十七）並收錄於常駐測試 `test_kept_triage_units_a3_a4_a5b_a6_261031`；以下 6 組因依賴已捨棄之 `Unit A-3b`、`Unit A-3c` 或 `Unit A-5a` 而列為困難案例：

| # | 許氏注音按鍵序列 (`keys`) | 預期輸出 (`expected`) | 目前實際輸出 (`actual`) | 競爭歧義點 | 嘗試過之做法與捨棄原因 |
|---|---|---|---|---|---|
| 1 | `cm ckjjljpew jgsnxldxj` | `山上正飄著濃霧` | `山上政飄著濃霧` | `山上 + 正 + 飄著` vs. `山上 + 政` | 單字副詞「正（`ㄐㄌㄐ` `ㄓㄥˋ`）」在方位詞「山上」後與同音單字「政」競爭；原依賴 `Unit A-3b`（`transitions_user.tsv`）與 `Unit A-3c`（`patterns_user.jsonl`）。 |
| 2 | `tr ceefdsnyjpem xndjk fa vkdjel aif` | `他寫的那篇文章非常精彩` | `他寫的那片文章非常精彩` | `那篇 + 文章` vs. `那片 + 文章` | 「那片」於基礎詞庫之無上下文權重高於「那篇」；原依賴 `Unit A-3b`（`transitions_user.tsv`）與 `Unit A-5a`。 |
| 3 | `lajdgdvxmfbxjgxhjvejlid` | `累得喘不過氣來` | `累德喘不過氣來` | `累 + 得(ㄉㄜˊ dgd) + 喘不過氣來` vs. `累 + 德` | 二聲 `dgd`（`ㄉㄜˊ`）在單音節狀態詞「累」後需連接可能補語子句「喘不過氣來」；原依賴 `Unit A-5a` 構式七改動（該單位於 `dev` 造成 `broken = 79 > fixed = 44`）。 |
| 4 | `vumdcndgxmjjxjdsxmdvldjgjcekjrnjxj` | `全神貫注地完成這項任務` | `全神貫注的完成這項任務` | `全神貫注 + 地 + 完成` vs. `全神貫注 + 的 + 完成` | 成語狀語後接 `ㄉㄜ˙`（`ds`）時，「地 vs. 的」受構式十九 `C04` 控制；`Unit A-5a` 改動 `C04` 於開發集造成顯著副作用。 |
| 5 | `jgjggsxnjtedxhfmnse cjzijcj` | `這個問題我們一試再試` | `這個問題我們一事再事` | `一試再試` vs. `一事再事` | 單音節動詞「試（`ㄕˋ`）」與高先驗名詞「事（`ㄕˋ`）」在對稱框架 `[一 + X + 再 + X]` 中同音競爭，兩個「事」同樣滿足字形對稱條件 `X_1 == X_2`；原依賴 `Unit A-3c`（`patterns_user.jsonl`）硬編碼詞條。 |
| 6 | `velfzijcerjly cumfdm jxl cumfzgdcjhgddszjtefdyjcewf` | `請在下拉選單中選擇適合的字體大小` | `請再下拉選單中選擇適合的字體大小` | `請 + 在 + 下拉選單中` vs. `請 + 再` | 後段「選擇適合的字體大小」已由保留之 `Unit A-5b`（構式二十六）成功解出，惟句首「請在 vs. 請再」同 2.2 第 8 項，依賴已捨棄之 `Unit A-5a` 構式十一改動。 |

---

## 3. 2026-11-05 分流新增之困難案例（`wip/2026-11-05-unreviewed`）

依修訂版交接提示詞第三節，`wip/2026-11-05-unreviewed`（commit `c135e1c`）中 `constructions.rs` 新增之 4,464 條加減分有 94.8% 超過 `|score| <= 35` 上限，且多以三字以上句子片段作為語境條件，未移植。以下新增測試所需之選字規則均來自該批未移植規則，故標記為 `#[ignore = "known hard case: ..."]`：

| 測試 | 內容 | 目前實際輸出（節錄） | 未通過原因 |
|---|---|---|---|
| `test_lyric_narrative_and_comma_parallel_discourse_261101` | 歌詞句「再高的牆擋不住溫柔」、「流向亮起來的街頭」及燈具／火源衍生句 | `再高的強擋不住溫柔`、`流向量起來的街頭` | 依賴 Section 24～30 之高分構式（多數 \|score\| ≥ 60） |
| `test_nine_topological_structures_valence_relativization_quantifier_serial_261101` | 處置式、關係子句、全稱量化、連謂句（如「把燈點亮」） | `把登點亮` | 同上 |
| `test_diagonal_pun_candidates_and_gongshi_weihe_disambiguation_261101` | 「一試再試」首選與諧音候選、「公式／為何」消歧 | 首選為 `一事再事` | 首選依賴未移植規則；諧音候選展開功能（`lookup_diagonal_pun_constructions`）本身已移植 |
| `test_user_clauses_si_geo_postal_and_meizhou_261103` | 使用者實戰分句（「川普正名」、「剩下最後1%的每週額度」、門牌地址與 3+3 郵遞區號等） | `川普證明` | 依賴未移植規則；自 `test_shift_space_fullwidth_and_punct_consistency_and_si_sentence_261103` 拆出，半全形功能部分仍為常駐測試 |
| `test_slot_assimilation_into_existing_7d_constructions_261031`（`ruhi-studio/src/tuner.rs`） | 驗證新詞「太魯閣」、「拉拉山」、「語意構式」同化至 `meteorological_subject_yu_continuous_aspect_7d_array` 與 `attributive_wancheng_de_noun_4d_array` | `matched_existing_constructions` 未命中上述兩模板 | 該兩條模板屬於 2026-11-05 分流捨棄之 `Unit A2`（`patterns_user.jsonl` 91 條模板，造成 `dev` CER 由 `6.32%` 退步至 `6.45%`、`fixed=15, broken=33`），正式資料檔未收錄 |

另外，`test_kept_triage_units_a3_a4_a5b_a6_261031` 在保全分支中被追加兩句歌詞案例；該兩句已移除，恢復為 10/31 分流後之內容（既有案例與預期值未變）。
