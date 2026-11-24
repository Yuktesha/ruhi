# 如喜（RuhI）v1.0 輕量神經重排序架構規格書（`RuhI-CharLM-v1` ＋ `RLNM0001` ＋ Local Sidecar）

- **版本**：v1.0 Stage C1 定稿（2026-11-24）
- **適用範圍**：[`scripts/train_neural_reranker.py`](file:///c:/ATGprjs/ruhi/scripts/train_neural_reranker.py)、[`ruhi-core/src/bin/ruhi_eval.rs`](file:///c:/ATGprjs/ruhi/ruhi-core/src/bin/ruhi_eval.rs) 及後續 `ruhi-reranker` 本機推理守護服務與 TSF 選配 IPC 用戶端。
- **核心原則**：
  1. 嚴格維持 `ruhi_tsf.dll` 純 Rust、`mmap` 低記憶體與 AppContainer 沙盒相容特性，**不將神經推論框架連結至 TSF DLL，亦不變動 `RuhiTextService` COM ABI**。
  2. 以 **Forward-Viterbi ＋ Backward-A\* N-Best（`K=16`／長句可擴充至 `K=32`）** 為第一階段確定性候選生成器（開發集 Oracle CER 上界 **`1.52%`**），由輕量神經模型專責長距語篇與深層語意重排，並嚴格執行 **`15 ms` 逾時自動回退**至 `v0.9.0` 統計／構式 Top-1 結果。

---

## 一、神經語言模型規格（`RuhI-CharLM-v1`）

### 1.1 架構選擇與理由
- **字元級自迴歸因果語言模型（Character-Level Decoder-Only Transformer）**：
  - 注音輸入法選字重排的核心挑戰在於跨詞界同音錯字（如「`在/再`」、「`以/已`」、「`變/便`」、「`辨識/便是`」）；字元級架構無須在多個候選之間重新對齊異構詞表切分，任意同音字序列皆可逐字算得精確對數似然 $\log P_{\theta}(c_1, \dots, c_T \mid \text{prefix})$。
  - 採用 **前綴樹共用 KV-Cache（Prefix-Shared Trie KV-Cache）**：同一組 `K=16` 候選往往共享相同前綴或前文，經前綴共享推論可將重複計算量降至約 `20%–35%`。

### 1.2 預設超參數與精確參數量（`~22.59M`）

| 超參數 | 預設值 | 說明 |
|---|---:|---|
| `vocab_size` | `7,200`（上限 `8,192`） | 保留 4 個特殊符號（`<pad>=0`, `<bos>=1`, `<eos>=2`, `<unk>=3`），其餘依台灣正體常用／次常用字表（CNS11643 / 教育部字表）及清洗語料高頻字元＋標點依序建構 |
| `d_model` | `512` | 隱藏維度 |
| `n_layers` | `6` | Transformer Decoder 層數 |
| `n_heads` | `8` | 多頭注意力頭數（`head_dim = 64`） |
| `d_ff` | `1,368` | SwiGLU 前饋網路中間維度（約 $\frac{8}{3} d_{\text{model}}$ 並對齊 8 的倍數） |
| `max_seq_len` | `256` | 最大上下文視窗長度（字元數） |
| 正規化與位置編碼 | RMSNorm ($\epsilon = 10^{-5}$) + RoPE ($\theta = 10000.0$) | Pre-Norm 結構（`attn_norm`、`ffn_norm` 與 `norm_final`），不使用 Bias |
| 權重共享 | Weight-Tied Embedding / LM Head | 詞嵌入矩陣 `tok_emb` 與輸出投影共用 |

- **總參數量計算**：
  - 詞嵌入：`7,200 × 512 = 3,686,400`
  - 每層注意力（`wq`, `wk`, `wv`, `wo`）：`4 × 512 × 512 = 1,048,576`
  - 每層 SwiGLU（`w_gate`, `w_up`, `w_down`）：`3 × 512 × 1,368 = 2,101,248`
  - 每層 RMSNorm：`2 × 512 = 1,024`；最終 `norm_final`：`512`
  - 總計：`3,686,400 + 6 × (1,048,576 + 2,101,248 + 1,024) + 512 = 22,592,000` 參數（約 **`22.59M`**）。
- **INT8 量化後檔案大小**：採列向對稱量化（Per-Row Symmetric INT8 ＋ 每列 1 組 FP32 scale），總計約 **`22.8 MB`**，單純記憶體映射即可秒級啟動。

---

## 二、`RLNM0001` 唯讀記憶體映射二進位格式規範

所有整數與浮點數一律採 **Little-Endian** 儲存；全體張量與索引區段起始位址皆採 **8-byte 對齊**（不足補 `0x00`），以利 Rust 直接透過 `mmap` 零拷貝讀取。

### 2.1 固定檔頭與區段目錄（共 `128` bytes）

1. **主檔頭（`0..64`，64 bytes）**：
   | 位移 (`bytes`) | 型別 | 欄位名稱 | 說明 |
   |---|---|---|---|
   | `0..8` | `[u8; 8]` | `magic` | 固定為 `b"RLNM0001"` |
   | `8..12` | `u32` | `version` | `1` |
   | `12..16` | `u32` | `flags` | Bit 0: `weight_tied (1)`；Bit 1: `rope (1)`；Bit 2: `swiglu (1)`；預設 `0x7` |
   | `16..20` | `u32` | `vocab_size` | 詞表總字數 $V$ |
   | `20..24` | `u32` | `d_model` | $D$ |
   | `24..28` | `u32` | `n_layers` | $L$ |
   | `28..32` | `u32` | `n_heads` | $H$ |
   | `32..36` | `u32` | `d_ff` | $F$ |
   | `36..40` | `u32` | `max_seq_len` | 最大序列長度 |
   | `40..48` | `f64` | `rope_theta` | 預設 `10000.0` |
   | `48..56` | `u64` | `corpus_fingerprint` | 訓練語料來源與清潔字數雜湊指紋 |
   | `56..60` | `f32` | `rms_eps` | 預設 `1e-5` |
   | `60..64` | `u32` | `reserved` | 固定 `0` |

2. **區段目錄（`64..128`，64 bytes）**：
   | 位移 (`bytes`) | 型別 | 欄位名稱 | 說明 |
   |---|---|---|---|
   | `64..72` | `u64` | `off_vocab_meta` | $V$ 筆 `(str_offset: u32, byte_len: u32)`（相對 `off_vocab_strings`） |
   | `72..80` | `u64` | `off_vocab_strings` | UTF-8 緊密詞表字串資料區（對齊 8 bytes） |
   | `80..88` | `u64` | `off_tok_emb_q` | `tok_emb` 量化矩陣 `[V, D]` `i8` |
   | `88..96` | `u64` | `off_tok_emb_s` | `tok_emb` 列縮放因子 `[V]` `f32` |
   | `96..104` | `u64` | `off_norm_final` | `norm_final` 權重 `[D]` `f32` |
   | `104..112` | `u64` | `off_layers` | 第 `0..L-1` 層連續儲存區起始位址 |
   | `112..120` | `u64` | `total_bytes` | 完整檔案位元組長度（`mmap` 邊界檢核用） |
   | `120..128` | `u64` | `payload_fnv64` | 自 `128..total_bytes` 全體權重與詞表資料區之 64-bit FNV-1a 校驗值 |

### 2.2 Per-Row 對稱 INT8 量化規則與每層排列

- 對任意形狀為 `[R, C]` 的權重矩陣 $W$，逐列計算 $s_r = \frac{\max(|W_{r,:}|)}{127.0}$（若全零則取 $1.0$），量化儲存為 $Q_{r,c} = \text{round}(\text{clip}(W_{r,c} / s_r, -127, 127)) \in [-127, 127]$（`i8`）及 $s_r$（`f32`）。推論時僅需進行 `i8 × f32` 點積或對輸入活化同步動態量化為 `i8` 進行整數累加後乘以縮放因子。
- 每層內部依固定順序儲存並於每段尾端補齊 8-byte 對齊：
  1. `attn_norm` (`[D] f32`)
  2. `ffn_norm` (`[D] f32`)
  3. `wq_q` (`[D, D] i8`) ➔ `wq_s` (`[D] f32`)
  4. `wk_q` (`[D, D] i8`) ➔ `wk_s` (`[D] f32`)
  5. `wv_q` (`[D, D] i8`) ➔ `wv_s` (`[D] f32`)
  6. `wo_q` (`[D, D] i8`) ➔ `wo_s` (`[D] f32`)
  7. `w_gate_q` (`[F, D] i8`) ➔ `w_gate_s` (`[F] f32`)
  8. `w_up_q` (`[F, D] i8`) ➔ `w_up_s` (`[F] f32`)
  9. `w_down_q` (`[D, F] i8`) ➔ `w_down_s` (`[D] f32`)

---

## 三、本機 IPC Sidecar 與容錯設計（`ruhi-reranker`）

1. **獨立單例執行檔**：`ruhi-reranker.exe` 於使用者開啟選配「深度神經重排序」時由工作列／設定中心或隨需喚起，常駐載入 `rlnm_zh_tw.bin`（`~23 MB`）。
2. **Named Pipe 二進位協定**：
   - 通道名稱：`\\.\pipe\RuhI_NeuralReranker_v1`（配置標準安全描述元，允許桌面一般進程與低權限 UWP / 瀏覽器 AppContainer 沙盒連接）。
   - 請求幀（小端長度前綴）：`magic("RNQ1") | req_id: u64 | timeout_ms: u16 | prefix_len: u16 | prefix_utf8 | k: u16 | [cand_len: u16, cand_utf8] * k`。
   - 回應幀：`magic("RNR1") | req_id: u64 | status: u8 (0=OK) | k: u16 | [neural_logp: f32] * k`。
3. **嚴格延遲邊界與無感降級**：
   - TSF 用戶端僅在「送出整句／詞網格達 3 音節以上且使用者停頓重排」或選字確定前發出具 **`15 ms` 硬逾時**之非阻塞連線／讀寫請求；若管道未啟動、忙線或超過 `15 ms`，立即無聲退回 `v0.9.0` 原生 Top-1 結果，確保鍵盤輸入絕對零卡頓。
4. **融合計分公式**：
   $$S_{\text{total}}(y) = S_{\text{viterbi+ngram}}(y) + \lambda_{\text{neural}} \cdot \log P_{\theta}(y \mid \text{prefix})$$
   其中 $\lambda_{\text{neural}}$ 於 `dev.tsv` Top-16 N-Best 上以網格搜尋校準鎖定。

---

## 四、零資料洩漏訓練流程與驗收案例

1. **商業授權安全語料與去重規範**：
   - 全面復用 [`scripts/train_ngram_lm.py`](file:///c:/ATGprjs/ruhi/scripts/train_ngram_lm.py) 內建之 12 組商業安全台灣繁體來源（FineWeb-2 `.tw`、Common Voice zh-TW、立法院逐字稿與質詢、tw-chinese-chat、台灣法律推理與對話集、`agentlans/traditional-chinese` 等），經 `TaiwanPurityFilter`（`twfilter-tables`）及 `--dev-hashes`（9,456 筆 `dev`/`test`/`typing_tw` 雜湊）排除任何重疊句段。
2. **Stage C1 實境四句 N-Best 覆蓋率基準診斷（2026-11-24 實測）**：

   | 實境語句 | 現行 Top-1 輸出 | 目標句於 Top-16 名次 | 分差（`Top1 - Gold`） | `v1.0` 處置策略 |
   |---|---|---|---:|---|
   | `再補上三句` | `在補上三句` | **第 4 名** | `19.591` | 納入 `v1.0` 重排驗收測試；評估 $\lambda_{\text{neural}}$ 結合對數似然差翻正能力 |
   | `已取消南下旅程` | `以取消南下旅程` | **第 2 名** | `0.082` | 極小分差，由 `RuhI-CharLM-v1` 直接穩定翻正 |
   | `滑鼠點一下變兩下該怎麼修復` | `滑鼠點一下便兩下該怎麼修復` | **第 2 名** | `0.350` | 極小分差，由 `RuhI-CharLM-v1` 直接穩定翻正 |
   | `此刻視力模糊難以辨識滑鼠底下大概只有` | `此刻視力模糊難以便是滑鼠底下大概只有` | 整句擠出 Top-16（短句 `難以辨識滑鼠底下` 位於 **第 11 名**，分差 `9.649`） | — | 長句受單字高頻詞組合乘積佔滿前段，納入 `v1.0` 長句 `K=32` 擴充寬度或束搜尋多樣性調整評估對象 |

3. **其他固定驗收基準**：
   - [`eval_reports/local/dev_nbest_k16.jsonl`](file:///c:/ATGprjs/ruhi/eval_reports/local/dev_nbest_k16.jsonl)（3,640 句開發集離線 N-Best 重排目標：自基準 CER `3.49%` 往 Oracle 上界 `1.52%` 推進）。
   - [`docs/KNOWN_HARD_CASES.md`](file:///c:/ATGprjs/ruhi/docs/KNOWN_HARD_CASES.md) 第四／五節薄邊際例句（「歷經多代」、「多代機型」、「雞嘴變鴨嘴」、「他不捨得花錢」、「但他會不會對系統造成負擔」等）。
