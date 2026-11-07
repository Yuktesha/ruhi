# 如喜（RuhI）專案專屬開發與計算語言學構式鐵律 (RuhI Repository Agent Rules)

本檔案為 `C:\ATGprjs\ruhi` 儲存庫之強制執行規範，所有進入本專案的 Agent 必須無條件恪守，徹底根絕「單純加詞入庫、偷懶調單字權重打地鼠」的退化行為。完整理論與架構細節請參閱 [`docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md`](file:///c:/ATGprjs/ruhi/docs/RUHI_LINGUISTIC_ARCHITECTURE_AND_AI_TUNING_SPEC.md)。

---

## 1. 四大核心禁忌（Strict Prohibitions）

1. **嚴禁「單字權重打地鼠（Unigram Weight Whack-a-Mole）」**：
   - 嚴禁為修復特定詞句中的某個選字，而手動調高或調低字典中的單字無上下文先驗權重（如隨意更動 `理/裡`、`視/是`、`月/越`、`久/九`、`牆/強`、`燈/登`、`亮/量`、`向/相/象` 等單字權重）。
2. **嚴禁將透明句法短語或可因式分解之複合詞預拼塞入詞庫／槽位（No Pre-Concatenated Syntactic Phrases or Unfactorized Compounds）**：
   - [`ruhi_lexicon/data/lexicon_domain.tsv`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv) 僅收納不可再由語法規則拆解之詞彙單元，**嚴禁**寫入含結構助詞「的／得／地」之偏正或述補短語（如 `翠綠的山林步道`、`古老的鐵道`）。
   - [`ruhi_lexicon/data/patterns_user.jsonl`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/patterns_user.jsonl) 與 [`ruhi-core/src/router/constructions.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/constructions.rs) 槽位內凡具共享詞根、否定前綴、體標記或補語後綴之組合（如 `圈選/點選/勾選`、`不選/不買`、`忘光/吃光`、`交到/送到`、`流向/奔向/望向`、`路燈/街燈/車燈/夜燈/檯燈`、`燭火/星火/燈火`、`擋不住/攔不住`），**嚴禁**預先拼接塞在同一槽位，**必須全面因式分解為正交語素槽位（Orthogonal Morpheme Slots）**。
3. **嚴禁暴力硬灌超高分特例與極端權重懲罰**：
   - 嚴禁在詞庫灌入 `2000+` 之極端單詞權重，或在轉移表塞入無上下文之全域負分。
4. **嚴禁引入半吊子本地小模型（Local LLM）干預核心語意路由**：
   - 語意消歧一律由 Rust 核心之計算語言學因式分解構式與確定性轉移張量執行（歷史實驗紀錄封存於 [`docs/LOCAL_LLM_SEMANTIC_ANALYST_FUTURE_ARCHIVE.md`](file:///c:/ATGprjs/ruhi/docs/LOCAL_LLM_SEMANTIC_ANALYST_FUTURE_ARCHIVE.md)）。

---

## 2. 九大高維語意析辨公式與構式拓撲結構（Nine High-Dimensional Topological Structures）

凡處理選字消歧與新增語意規則，必須先將語句映射至以下九大拓撲與對偶變換結構之一：

1. **拓撲結構 I — 雙模態正交語素纖維叢與子叢分解（$\mathcal{B}_{\text{fiber}} = (\bigcup_{r \in R} \mathcal{M}_r \times \mathcal{H}_r) \setminus E_{\text{excl}}$）**：
   - 按語意角色 $r \in R$ 分解為正交子叢之聯集，並扣除非法搭配奇異集 $E_{\text{excl}}$：
     - 發光體／火源構式：$(\{\text{路, 街, 車, 夜, 檯}\} \times \{\text{燈}\}) \cup (\{\text{燭, 星, 燈}\} \times \{\text{火}\})$
     - 空間位移／視線 × 趨向補語構式：$\{\text{流, 走, 奔, 飛, 駛, 望, 看, 朝, 邁, 通, 湧, 飄}\} \times \{\text{向}\}$
     - 氣象／降水／天象四子叢：降水 $M_{\text{precip}} \times \{\text{雨, 雪}\}$、氣流 $M_{\text{wind}} \times \{\text{風}\}$、凝結能見度 $M_{\text{vapor}} \times \{\text{霧, 霜, 露}\}$、劇烈天象 $M_{\text{severe}} \times \{\text{雷, 電, 雹, 太陽, 陽光}\}$
   - **雙模態同態定理**：$\Phi(L \to [m \cdot h] \to R) \simeq \Phi(L \to [m] \to [h] \to R)$。確保詞庫已收錄之複合詞節點 $[m \cdot h]$ 與未收錄之雙節點拆分路徑 $[m] \to [h]$ 在左右語境邊界享有嚴格同構之轉移位勢。
2. **拓撲結構 II — 對角線恆等流形（$\Delta$）與反對角線流形（$\Delta^\complement$）**：
   - **對角線流形 $\Delta = \{(x, y) \mid x = y\}$**：跨槽位變數綁定（如 $x + \text{好} + x + \text{滿}$、$\text{一} + x + \text{再} + x$、$x + \text{來} + x + \text{去}$、$x + \text{不} + x$、$\text{哪裡}\dots\text{就}\dots\text{哪裡}$）。
   - **反對角線流形 $\Delta^\complement = \{(x, y) \mid x \neq y\}$**：並列連詞構式（$x + \text{與/和/及/而非} + y$）於同音競爭時強制去退化（如「理想與現實」）。
3. **拓撲結構 III — 雙線性鞍點分岔張量（$\mathcal{J}_{\text{bifurcate}}(c_t \mid L_{t-1}, R_{t+1}) = \boldsymbol{\phi}_L(L_{t-1})^\top \mathbf{W}^{(c_t)} \boldsymbol{\phi}_R(R_{t+1})$）**：
   - 針對高熵同音樞紐（`的/得/地`、`在/再`、`是/試`、`對/隊`、`向/相/象`、`裡/理`、`久/九`、`牆/強`、`亮/量`），同時耦合左鄰與右鄰語法特徵向量進行雙向夾擊消歧。
4. **拓撲結構 IV — 非局域環綴與離合詞括號代數（$\mathcal{K}_{\text{bracket}}(\alpha \cdots X \cdots \beta)$）**：
   - 跨越中間任意名詞組或數量詞組 $X$ 建立非局域括號閉包（空間／時間介詞環綴 `[在/於/從/當 ... 中/裡/內/上/下/時/後]`、複合趨向補語分裂框 `[V+出/進/回/過/起 ... NP ... 來/去]`、離合動賓分裂框 `[睡/洗/散/幫/見 ... Quant ... 覺/澡/步/忙/面]`）。
5. **拓撲結構 V — 跨標點語篇平行同態映射（$\mathcal{T}_{\text{comma}} \oplus \mathcal{T}_{\text{parallel}}$）**：
   - 結合「主題—評述逗號穿透算子 $\mathcal{T}_{\text{comma}}$」（跨越逗號與語氣助詞檢查主題與謂語之選擇限制）與「跨句平行結構同態映射 $\mathcal{T}_{\text{parallel}}$」（當相鄰分句 $C_{k-1} \simeq C_k$ 具平行句法骨架時，沿句號映射主題與狀態謂語語意場，如「風，還在吹。燈，還亮著。」）。
6. **拓撲結構 VI — 論元換位與價態規範變換（$\mathcal{G}_{\text{valence}}^*$）**：
   - $\Phi_{\text{disposal}}(\text{把/將} \to N_{\text{pat}} \to V) \simeq \mathcal{G}_{\text{valence}}^*(\Phi_{\text{VO}}(V \to N_{\text{pat}}))$。將基礎動賓／主謂語意配對自動對偶投射至處置式（`把/將 + N_pat + V_res`，如「把路燈點亮」、「把燭火吹熄」）與被動式（`N_pat + 被/給 + V_res`，如「街頭被照亮了」）。
7. **拓撲結構 VII — 關係子句中心語拉回算子（$\mathcal{R}_{\text{rel}}^*$）**：
   - $\Phi_{\text{rel}}(V \xrightarrow{\text{的}} N) = \alpha_{\text{SV}} \Phi_{\text{SV}}(N \to V) + \alpha_{\text{VO}} \Phi_{\text{VO}}(V \to N)$。將順向主謂或動賓選擇限制跨越「的」拉回至中心語後置關係子句 `[V + 的 + N]`（如「亮著的路燈」、「點亮的燭火」、「吹過的風」）。
8. **拓撲結構 VIII — 全稱量化與極性焦點遠距協變叢（$\mathcal{Q}_{\text{concord}}$）**：
   - $\mathcal{Q}_{\text{concord}}(q_L, X, \text{op}_R) = \omega_{q_L, \text{op}_R} \cdot \mathbb{I}(\text{op}_R \in \{\text{都, 也, 皆, 全}\}) \cdot \exp(-\mu \|X\|_{\text{syll}})$。將左界全稱／讓步／焦點算子（`連/甚至/無論/不管/就算/誰/什麼`）跨越中間成分 $X$ 與右界協變算子（`再/都/也`）遠距鎖定（如「連再高的牆都擋不住」、「誰也擋不住溫柔」）。
9. **拓撲結構 IX — 連謂序列之時間—因果偏序錐（$\mathcal{C}_{\text{serial}}$）**：
   - $\mathcal{C}_{\text{serial}}(V_1, V_2) > 0 \iff \text{Phase}(V_1: \text{姿態/位移/取用}) \prec_{\text{causal}} \text{Phase}(V_2: \text{處置/感知/結果})$（如「走過去把燈點亮」）。

---

## 3. 漢語語素語意本體庫（Single-Source-of-Truth Morpheme Ontology）

- **資料層單一事實來源**：[`ruhi_lexicon/data/morpheme_ontology.json`](file:///c:/ATGprjs/ruhi/ruhi_lexicon/data/morpheme_ontology.json)（定義 44 大正交語素語意類別 `MorphemeClass` 與纖維叢配對排除規則 `fiber_bundles`）。
- **引擎層 $O(1)$ 位元遮罩索引**：[`ruhi-core/src/router/morpheme_ontology.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/router/morpheme_ontology.rs)（`MorphemeOntology::global()` 編譯為 64-bit `MorphemeSemanticMask: u64`）。
- **維護鐵律**：凡需擴充某語意類之同類單字語素（如燈具修飾 `LampMod`、火源修飾 `FireLightMod`、位移動詞詞根 `MotionGazeVerbStem`、氣象四子叢語素等），**一律集中於 `morpheme_ontology.json` 維護**，嚴禁在個別函式中散落重複硬編碼 `matches!` 清單。

---

## 4. 交付前強制驗證守門清單（Mandatory Verification Gate）

每次修改詞庫、轉移規則或構式陣列後，必須執行並通過以下測試方可交付：
1. **多維構式同音碰撞零容忍掃描**：
   - `cargo test --manifest-path c:\ATGprjs\ruhi\ruhi-studio\Cargo.toml test_array_collision_audit_and_auto_handover_delivery_261019`
   - 確保 `unsuppressed_active_collisions_count == 0`（所有跨槽位同音詞皆明確標註 `bifurcation_pairs` 或 `exclude_pairs`）。
2. **核心引擎構式與黃金測試集驗證**：
   - 在 [`ruhi-core/src/engine.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/engine.rs) 補齊包含衍生正交組合之單元測試，並確認 `cargo test` 100% 通過。
