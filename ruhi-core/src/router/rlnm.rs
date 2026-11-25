// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/rlnm.rs — 純 Rust RLNM0001 唯讀記憶體映射推論引擎與前綴樹共用 KV-Cache
//
// 依據 docs/RUHI_V1_NEURAL_RERANKER_SPEC.md 實作：
// - 零外部推論框架依賴，透過 ReadOnlyMmap 直接零拷貝映射 INT8 量化權重與字表；
// - x86_64 執行期偵測 AVX2 + FMA 以 4 路向量點積加速；
// - 前綴樹（Trie）共用 KV-Cache + 葉節點免算 lm_head，將 Top-16 候選重排計算量大幅壓縮。

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::mmap_io::{read_f32_le, read_u32_le, read_u64_le, slice_checked, ReadOnlyMmap};

pub const RLNM_MAGIC: &[u8; 8] = b"RLNM0001";
pub const RLNM_VERSION: u32 = 1;
pub const RLNM_HEADER_BYTES: usize = 128;

pub const PAD_ID: u32 = 0;
pub const BOS_ID: u32 = 1;
pub const EOS_ID: u32 = 2;
pub const UNK_ID: u32 = 3;

#[inline]
fn align8(n: usize) -> usize {
    (n + 7) & !7
}

#[inline]
fn read_f64_le(buf: &[u8], off: usize) -> Option<f64> {
    let s = slice_checked(buf, off, 8)?;
    Some(f64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[inline]
fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

#[derive(Debug, Clone, Copy)]
pub struct RlnmConfig {
    pub flags: u32,
    pub vocab_size: usize,
    pub d_model: usize,
    pub n_layers: usize,
    pub n_heads: usize,
    pub head_dim: usize,
    pub d_ff: usize,
    pub max_seq_len: usize,
    pub rope_theta: f64,
    pub corpus_fingerprint: u64,
    pub rms_eps: f32,
}

#[derive(Debug, Clone, Copy)]
struct QuantMatrixSlice {
    off_q: usize,
    off_s: usize,
    rows: usize,
    cols: usize,
}

#[derive(Debug, Clone, Copy)]
struct LayerOffsets {
    off_attn_norm: usize,
    off_ffn_norm: usize,
    wq: QuantMatrixSlice,
    wk: QuantMatrixSlice,
    wv: QuantMatrixSlice,
    wo: QuantMatrixSlice,
    w_gate: QuantMatrixSlice,
    w_up: QuantMatrixSlice,
    w_down: QuantMatrixSlice,
}

pub struct RlnmModel {
    mmap: ReadOnlyMmap,
    pub cfg: RlnmConfig,
    pub vocab: Vec<String>,
    char_to_id: HashMap<char, u32>,
    tok_emb: QuantMatrixSlice,
    off_norm_final: usize,
    layers: Vec<LayerOffsets>,
    rope_cos: Vec<f32>,
    rope_sin: Vec<f32>,
    has_avx2_fma: bool,
    has_avx512f: bool,
}

impl std::fmt::Debug for RlnmModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RlnmModel")
            .field("cfg", &self.cfg)
            .field("mapped_bytes", &self.mmap.len())
            .field("has_avx2_fma", &self.has_avx2_fma)
            .finish()
    }
}

impl RlnmModel {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mmap = ReadOnlyMmap::open(path)?;
        let buf = mmap.as_slice();
        if buf.len() < RLNM_HEADER_BYTES {
            return Err(invalid_data("RLNM0001 header truncated"));
        }
        if &buf[0..8] != RLNM_MAGIC {
            return Err(invalid_data("invalid RLNM0001 magic"));
        }
        let version = read_u32_le(buf, 8).ok_or_else(|| invalid_data("header truncated"))?;
        if version != RLNM_VERSION {
            return Err(invalid_data("unsupported RLNM version"));
        }
        let flags = read_u32_le(buf, 12).ok_or_else(|| invalid_data("header truncated"))?;
        let vocab_size = read_u32_le(buf, 16).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let d_model = read_u32_le(buf, 20).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let n_layers = read_u32_le(buf, 24).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let n_heads = read_u32_le(buf, 28).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let d_ff = read_u32_le(buf, 32).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let max_seq_len = read_u32_le(buf, 36).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let rope_theta = read_f64_le(buf, 40).ok_or_else(|| invalid_data("header truncated"))?;
        let corpus_fingerprint = read_u64_le(buf, 48).ok_or_else(|| invalid_data("header truncated"))?;
        let rms_eps = read_f32_le(buf, 56).ok_or_else(|| invalid_data("header truncated"))?;

        if vocab_size < 4 || d_model == 0 || n_layers == 0 || n_heads == 0 || d_ff == 0 || max_seq_len == 0 {
            return Err(invalid_data("invalid RLNM0001 model dimensions"));
        }
        if d_model % n_heads != 0 {
            return Err(invalid_data("d_model must be divisible by n_heads"));
        }
        let head_dim = d_model / n_heads;
        if head_dim % 2 != 0 {
            return Err(invalid_data("head_dim must be even for RoPE"));
        }

        let off_vocab_meta = read_u64_le(buf, 64).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let off_vocab_strings = read_u64_le(buf, 72).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let off_tok_emb_q = read_u64_le(buf, 80).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let off_tok_emb_s = read_u64_le(buf, 88).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let off_norm_final = read_u64_le(buf, 96).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let off_layers = read_u64_le(buf, 104).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let total_bytes = read_u64_le(buf, 112).ok_or_else(|| invalid_data("dir truncated"))? as usize;
        let payload_hash = read_u64_le(buf, 120).ok_or_else(|| invalid_data("dir truncated"))?;

        if total_bytes != buf.len() {
            return Err(invalid_data("RLNM0001 file length mismatch"));
        }
        for off in [
            off_vocab_meta,
            off_vocab_strings,
            off_tok_emb_q,
            off_tok_emb_s,
            off_norm_final,
            off_layers,
        ] {
            if off % 8 != 0 || off < RLNM_HEADER_BYTES || off >= total_bytes {
                return Err(invalid_data("RLNM0001 invalid or unaligned section offset"));
            }
        }
        if fnv1a_64(&buf[RLNM_HEADER_BYTES..]) != payload_hash {
            return Err(invalid_data("RLNM0001 payload FNV-1a checksum mismatch"));
        }

        // 解析詞表
        if slice_checked(buf, off_vocab_meta, vocab_size * 8).is_none() {
            return Err(invalid_data("RLNM0001 vocab meta out of bounds"));
        }
        let mut vocab = Vec::with_capacity(vocab_size);
        let mut char_to_id = HashMap::with_capacity(vocab_size);
        for idx in 0..vocab_size {
            let s_off = read_u32_le(buf, off_vocab_meta + idx * 8).unwrap() as usize;
            let s_len = read_u32_le(buf, off_vocab_meta + idx * 8 + 4).unwrap() as usize;
            let raw_tok = slice_checked(buf, off_vocab_strings + s_off, s_len)
                .ok_or_else(|| invalid_data("RLNM0001 vocab string out of bounds"))?;
            let tok = std::str::from_utf8(raw_tok)
                .map_err(|_| invalid_data("RLNM0001 invalid UTF-8 vocab token"))?
                .to_string();
            if idx >= 4 {
                let mut cs = tok.chars();
                if let (Some(ch), None) = (cs.next(), cs.next()) {
                    char_to_id.insert(ch, idx as u32);
                }
            }
            vocab.push(tok);
        }

        // 校驗 tok_emb 與 norm_final
        let tok_emb = QuantMatrixSlice {
            off_q: off_tok_emb_q,
            off_s: off_tok_emb_s,
            rows: vocab_size,
            cols: d_model,
        };
        if slice_checked(buf, tok_emb.off_q, vocab_size * d_model).is_none()
            || slice_checked(buf, tok_emb.off_s, vocab_size * 4).is_none()
            || slice_checked(buf, off_norm_final, d_model * 4).is_none()
        {
            return Err(invalid_data("RLNM0001 tok_emb/norm_final out of bounds"));
        }

        // 解析逐層權重偏移
        let mut layers = Vec::with_capacity(n_layers);
        let mut cursor = off_layers;

        let take_vec_f32 = |cur: &mut usize, len: usize| -> io::Result<usize> {
            let bytes = len.checked_mul(4).ok_or_else(|| invalid_data("overflow"))?;
            slice_checked(buf, *cur, bytes).ok_or_else(|| invalid_data("RLNM0001 vec out of bounds"))?;
            let start = *cur;
            *cur += align8(bytes);
            Ok(start)
        };
        let take_quant_mat = |cur: &mut usize, rows: usize, cols: usize| -> io::Result<QuantMatrixSlice> {
            let q_bytes = rows.checked_mul(cols).ok_or_else(|| invalid_data("overflow"))?;
            slice_checked(buf, *cur, q_bytes).ok_or_else(|| invalid_data("RLNM0001 mat q out of bounds"))?;
            let off_q = *cur;
            *cur += align8(q_bytes);
            let off_s = take_vec_f32(cur, rows)?;
            Ok(QuantMatrixSlice { off_q, off_s, rows, cols })
        };

        for _ in 0..n_layers {
            let off_attn_norm = take_vec_f32(&mut cursor, d_model)?;
            let off_ffn_norm = take_vec_f32(&mut cursor, d_model)?;
            let wq = take_quant_mat(&mut cursor, d_model, d_model)?;
            let wk = take_quant_mat(&mut cursor, d_model, d_model)?;
            let wv = take_quant_mat(&mut cursor, d_model, d_model)?;
            let wo = take_quant_mat(&mut cursor, d_model, d_model)?;
            let w_gate = take_quant_mat(&mut cursor, d_ff, d_model)?;
            let w_up = take_quant_mat(&mut cursor, d_ff, d_model)?;
            let w_down = take_quant_mat(&mut cursor, d_model, d_ff)?;
            layers.push(LayerOffsets {
                off_attn_norm,
                off_ffn_norm,
                wq,
                wk,
                wv,
                wo,
                w_gate,
                w_up,
                w_down,
            });
        }
        if cursor != total_bytes {
            return Err(invalid_data("RLNM0001 layer section length mismatch"));
        }

        // 預計算 RoPE cos/sin 表 [max_seq_len, head_dim / 2]
        let half = head_dim / 2;
        let mut inv_freq = Vec::with_capacity(half);
        for k in 0..half {
            let exp = (2.0 * k as f64) / (head_dim as f64);
            inv_freq.push(1.0 / rope_theta.powf(exp));
        }
        let mut rope_cos = Vec::with_capacity(max_seq_len * half);
        let mut rope_sin = Vec::with_capacity(max_seq_len * half);
        for pos in 0..max_seq_len {
            let p = pos as f64;
            for &inv in &inv_freq {
                let angle = p * inv;
                rope_cos.push(angle.cos() as f32);
                rope_sin.push(angle.sin() as f32);
            }
        }

        #[cfg(target_arch = "x86_64")]
        let (has_avx2_fma, has_avx512f) = (
            std::arch::is_x86_feature_detected!("avx2")
                && std::arch::is_x86_feature_detected!("fma"),
            std::arch::is_x86_feature_detected!("avx512f"),
        );
        #[cfg(not(target_arch = "x86_64"))]
        let (has_avx2_fma, has_avx512f) = (false, false);

        Ok(Self {
            mmap,
            cfg: RlnmConfig {
                flags,
                vocab_size,
                d_model,
                n_layers,
                n_heads,
                head_dim,
                d_ff,
                max_seq_len,
                rope_theta,
                corpus_fingerprint,
                rms_eps,
            },
            vocab,
            char_to_id,
            tok_emb,
            off_norm_final,
            layers,
            rope_cos,
            rope_sin,
            has_avx2_fma,
            has_avx512f,
        })
    }

    #[inline]
    pub fn token_id_for_char(&self, ch: char) -> u32 {
        self.char_to_id.get(&ch).copied().unwrap_or(UNK_ID)
    }

    pub fn encode_text(&self, text: &str, out: &mut Vec<u32>) {
        out.clear();
        for ch in text.chars() {
            out.push(self.token_id_for_char(ch));
        }
    }

    #[inline]
    fn f32_slice(&self, off: usize, len: usize) -> &[f32] {
        let buf = self.mmap.as_slice();
        // SAFETY: 所有偏移已於 open() 驗證 8-byte 對齊與位元組邊界，且 x86_64/Little-Endian 下 f32 對齊為 4 bytes。
        unsafe { std::slice::from_raw_parts(buf.as_ptr().add(off) as *const f32, len) }
    }

    #[inline]
    fn i8_slice(&self, off: usize, len: usize) -> &[i8] {
        let buf = self.mmap.as_slice();
        unsafe { std::slice::from_raw_parts(buf.as_ptr().add(off) as *const i8, len) }
    }

    #[inline]
    fn mat_row_i8(&self, mat: QuantMatrixSlice, row: usize) -> (&[i8], f32) {
        let q = self.i8_slice(mat.off_q + row * mat.cols, mat.cols);
        let s = self.f32_slice(mat.off_s, mat.rows)[row];
        (q, s)
    }

    #[inline]
    fn load_token_embedding(&self, token_id: u32, dst: &mut [f32]) {
        let row = (token_id as usize).min(self.cfg.vocab_size - 1);
        let (q, s) = self.mat_row_i8(self.tok_emb, row);
        for (d, &qv) in dst.iter_mut().zip(q.iter()) {
            *d = (qv as f32) * s;
        }
    }

    #[inline]
    fn rmsnorm(&self, x: &[f32], gamma_off: usize, out: &mut [f32]) {
        let d = self.cfg.d_model;
        let gamma = self.f32_slice(gamma_off, d);
        let mut ss = 0.0f32;
        for &v in x {
            ss += v * v;
        }
        let inv_rms = 1.0f32 / (ss / (d as f32) + self.cfg.rms_eps).sqrt();
        for i in 0..d {
            out[i] = x[i] * inv_rms * gamma[i];
        }
    }

    #[inline]
    fn apply_rope(&self, vec: &mut [f32], pos: usize) {
        let half = self.cfg.head_dim / 2;
        let cos = &self.rope_cos[pos * half..(pos + 1) * half];
        let sin = &self.rope_sin[pos * half..(pos + 1) * half];
        for h in 0..self.cfg.n_heads {
            let base = h * self.cfg.head_dim;
            for k in 0..half {
                let i0 = base + 2 * k;
                let i1 = i0 + 1;
                let ze = vec[i0];
                let zo = vec[i1];
                let c = cos[k];
                let s = sin[k];
                vec[i0] = ze * c - zo * s;
                vec[i1] = ze * s + zo * c;
            }
        }
    }

    #[inline]
    fn dot_i8_f32(&self, q: &[i8], x: &[f32]) -> f32 {
        #[cfg(target_arch = "x86_64")]
        if self.has_avx2_fma {
            // SAFETY: 已於執行期驗證支援 AVX2 與 FMA 指令集，且函式內部完整處理任何尾數長度。
            return unsafe { dot_i8_f32_avx2_fma(q, x) };
        }
        dot_i8_f32_scalar(q, x)
    }

    fn matvec_range(&self, mat: QuantMatrixSlice, x: &[f32], row_start: usize, out_slice: &mut [f32]) {
        let cols = mat.cols;
        let q_all = self.i8_slice(mat.off_q, mat.rows * cols);
        let scales = self.f32_slice(mat.off_s, mat.rows);
        for (idx, dst) in out_slice.iter_mut().enumerate() {
            let r = row_start + idx;
            let q_row = &q_all[r * cols..(r + 1) * cols];
            *dst = self.dot_i8_f32(q_row, x) * scales[r];
        }
    }

    fn matvec(&self, mat: QuantMatrixSlice, x: &[f32], out: &mut [f32], pool: Option<&RlnmThreadPool>) {
        debug_assert_eq!(out.len(), mat.rows);
        if let Some(p) = pool {
            if p.num_workers() > 1 && mat.rows >= 512 {
                p.parallel_matvec(self, mat, x, out);
                return;
            }
        }
        self.matvec_range(mat, x, 0, out);
    }

    /// 針對單一位置 `pos` 推進 KV-Cache 並更新隱向量；僅在 `need_logits == true` 時計算最終 RMSNorm 與 `lm_head` 對數機率。
    pub fn step_token(
        &self,
        cache: &mut RlnmKvCache,
        pos: usize,
        token_id: u32,
        need_logits: bool,
        pool: Option<&RlnmThreadPool>,
    ) {
        let d = self.cfg.d_model;
        let f = self.cfg.d_ff;
        let n_heads = self.cfg.n_heads;
        let head_dim = self.cfg.head_dim;
        let attn_scale = 1.0f32 / (head_dim as f32).sqrt();

        // 避免長期借用 self 中的切片與 cache.ws 衝突，使用區域切片重組工作空間
        self.load_token_embedding(token_id, &mut cache.ws.x);

        let kv_stride = self.cfg.max_seq_len * d;
        for (li, layer) in self.layers.iter().enumerate() {
            // 1. Attention Pre-Norm + QKV
            self.rmsnorm(&cache.ws.x, layer.off_attn_norm, &mut cache.ws.h);
            if let Some(p) = pool {
                if p.num_workers() > 1 {
                    p.parallel_qkv(
                        self,
                        layer.wq,
                        layer.wk,
                        layer.wv,
                        &cache.ws.h,
                        &mut cache.ws.q,
                        &mut cache.ws.k,
                        &mut cache.ws.v,
                    );
                } else {
                    self.matvec_range(layer.wq, &cache.ws.h, 0, &mut cache.ws.q);
                    self.matvec_range(layer.wk, &cache.ws.h, 0, &mut cache.ws.k);
                    self.matvec_range(layer.wv, &cache.ws.h, 0, &mut cache.ws.v);
                }
            } else {
                self.matvec_range(layer.wq, &cache.ws.h, 0, &mut cache.ws.q);
                self.matvec_range(layer.wk, &cache.ws.h, 0, &mut cache.ws.k);
                self.matvec_range(layer.wv, &cache.ws.h, 0, &mut cache.ws.v);
            }

            self.apply_rope(&mut cache.ws.q, pos);
            self.apply_rope(&mut cache.ws.k, pos);

            // 寫入第 li 層位置 pos 之 KV-Cache
            let k_base = li * kv_stride;
            let k_pos_off = k_base + pos * d;
            cache.k_cache[k_pos_off..k_pos_off + d].copy_from_slice(&cache.ws.k);
            cache.v_cache[k_pos_off..k_pos_off + d].copy_from_slice(&cache.ws.v);

            // 2. Causal Multi-Head Attention（0..=pos）
            let seq_len = pos + 1;
            let k_layer = &cache.k_cache[k_base..k_base + seq_len * d];
            let v_layer = &cache.v_cache[k_base..k_base + seq_len * d];
            let scores = &mut cache.ws.attn_scores[..seq_len];

            for hd in 0..n_heads {
                let h_off = hd * head_dim;
                let q_head = &cache.ws.q[h_off..h_off + head_dim];
                let mut max_sc = f32::NEG_INFINITY;
                for t in 0..seq_len {
                    let k_head = &k_layer[t * d + h_off..t * d + h_off + head_dim];
                    let mut dot = 0.0f32;
                    for j in 0..head_dim {
                        dot += q_head[j] * k_head[j];
                    }
                    let sc = dot * attn_scale;
                    scores[t] = sc;
                    if sc > max_sc {
                        max_sc = sc;
                    }
                }
                let mut sum_exp = 0.0f32;
                for sc in scores.iter_mut() {
                    let e = (*sc - max_sc).exp();
                    *sc = e;
                    sum_exp += e;
                }
                let inv_sum = 1.0f32 / sum_exp;
                let out_head = &mut cache.ws.attn_out[h_off..h_off + head_dim];
                out_head.fill(0.0);
                for t in 0..seq_len {
                    let w = scores[t] * inv_sum;
                    let v_head = &v_layer[t * d + h_off..t * d + h_off + head_dim];
                    for j in 0..head_dim {
                        out_head[j] += w * v_head[j];
                    }
                }
            }

            // Output Projection + Residual
            self.matvec(layer.wo, &cache.ws.attn_out, &mut cache.ws.h, pool);
            for i in 0..d {
                cache.ws.x[i] += cache.ws.h[i];
            }

            // 3. FFN Pre-Norm + SwiGLU
            self.rmsnorm(&cache.ws.x, layer.off_ffn_norm, &mut cache.ws.h);
            if let Some(p) = pool {
                if p.num_workers() > 1 {
                    p.parallel_gate_up(
                        self,
                        layer.w_gate,
                        layer.w_up,
                        &cache.ws.h,
                        &mut cache.ws.gate,
                        &mut cache.ws.up,
                    );
                } else {
                    self.matvec_range(layer.w_gate, &cache.ws.h, 0, &mut cache.ws.gate);
                    self.matvec_range(layer.w_up, &cache.ws.h, 0, &mut cache.ws.up);
                }
            } else {
                self.matvec_range(layer.w_gate, &cache.ws.h, 0, &mut cache.ws.gate);
                self.matvec_range(layer.w_up, &cache.ws.h, 0, &mut cache.ws.up);
            }

            for i in 0..f {
                let g = cache.ws.gate[i];
                let sig = 1.0f32 / (1.0f32 + (-g.clamp(-60.0, 60.0)).exp());
                cache.ws.gate[i] = (g * sig) * cache.ws.up[i];
            }
            self.matvec(layer.w_down, &cache.ws.gate, &mut cache.ws.h, pool);
            for i in 0..d {
                cache.ws.x[i] += cache.ws.h[i];
            }
        }

        if need_logits {
            self.rmsnorm(&cache.ws.x, self.off_norm_final, &mut cache.ws.h);
            self.matvec(self.tok_emb, &cache.ws.h, &mut cache.ws.logits, pool);
            // 轉為 log_softmax 供直接查表 O(1) 讀取各子字元對數機率
            let mut max_v = f32::NEG_INFINITY;
            for &v in &cache.ws.logits {
                if v > max_v {
                    max_v = v;
                }
            }
            let mut sum_exp = 0.0f64;
            for &v in &cache.ws.logits {
                sum_exp += ((v - max_v) as f64).exp();
            }
            let lse = (max_v as f64) + sum_exp.ln();
            for v in &mut cache.ws.logits {
                *v = ((*v as f64) - lse) as f32;
            }
        }
    }

    /// 參考用單句逐字計分（`log P(cand | <bos> + prefix)`）
    pub fn score_candidate(&self, prefix: &str, cand: &str, pool: Option<&RlnmThreadPool>) -> f32 {
        let mut cache = self.alloc_kv_cache();
        let mut pref_ids = Vec::new();
        let mut cand_ids = Vec::new();
        self.encode_text(prefix, &mut pref_ids);
        self.encode_text(cand, &mut cand_ids);
        if cand_ids.is_empty() {
            return 0.0;
        }
        let max_len = self.cfg.max_seq_len;
        let budget = (max_len.saturating_sub(1 + cand_ids.len())).max(1);
        if pref_ids.len() > budget {
            let start = pref_ids.len() - budget;
            pref_ids.drain(0..start);
        }
        let mut seq = Vec::with_capacity(1 + pref_ids.len() + cand_ids.len());
        seq.push(BOS_ID);
        seq.extend_from_slice(&pref_ids);
        let cand_start = seq.len();
        seq.extend_from_slice(&cand_ids);
        if seq.len() > max_len {
            let keep = max_len - 1;
            let tail = cand_ids[cand_ids.len() - keep..].to_vec();
            seq.clear();
            seq.push(BOS_ID);
            seq.extend_from_slice(&tail);
        }

        let eff_cand_start = if seq.len() > cand_ids.len() {
            cand_start
        } else {
            1
        };
        let mut total_lp = 0.0f32;
        for pos in 0..(seq.len() - 1) {
            let need_logits = (pos + 1) >= eff_cand_start;
            self.step_token(&mut cache, pos, seq[pos], need_logits, pool);
            if need_logits {
                let target = seq[pos + 1] as usize;
                total_lp += cache.ws.logits[target];
            }
        }
        total_lp
    }

    #[inline]
    fn dot_f32(&self, a: &[f32], b: &[f32]) -> f32 {
        #[cfg(target_arch = "x86_64")]
        if self.has_avx2_fma {
            return unsafe { dot_f32_avx2_fma(a, b) };
        }
        dot_f32_scalar(a, b)
    }

    #[inline]
    fn axpy_f32(&self, alpha: f32, x: &[f32], y: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        if self.has_avx2_fma {
            unsafe { axpy_f32_avx2_fma(alpha, x, y) };
            return;
        }
        for (yv, &xv) in y.iter_mut().zip(x.iter()) {
            *yv += alpha * xv;
        }
    }

    #[inline]
    fn dequant_i8_row(&self, q_row: &[i8], scale: f32, dst: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        if self.has_avx2_fma {
            unsafe { dequant_i8_row_avx2(q_row, scale, dst) };
            return;
        }
        for (d, &qv) in dst.iter_mut().zip(q_row.iter()) {
            *d = (qv as f32) * scale;
        }
    }

    /// 對 `[batch, cols]` 輸入執行解量化一次、重用於多 token 磚塊的 GEMM，寫入 `y_batch[b * out_stride + r]`
    fn gemm_range(
        &self,
        mat: QuantMatrixSlice,
        x_batch: &[f32],
        batch: usize,
        row_start: usize,
        row_end: usize,
        y_batch: &mut [f32],
        out_stride: usize,
    ) {
        if batch == 0 || row_end <= row_start {
            return;
        }
        let cols = mat.cols;
        let q_all = self.i8_slice(mat.off_q, mat.rows * cols);
        let scales = self.f32_slice(mat.off_s, mat.rows);

        const STACK_CAP: usize = 4 * 2048;
        let mut w_stack = [0.0f32; STACK_CAP];
        let mut w_heap: Vec<f32> = Vec::new();
        let need_len = 4 * cols;
        let w_buf: &mut [f32] = if need_len <= STACK_CAP {
            &mut w_stack[..need_len]
        } else {
            w_heap.resize(need_len, 0.0);
            &mut w_heap
        };

        let mut r = row_start;
        while r + 4 <= row_end {
            for ri in 0..4 {
                let row = r + ri;
                let q_row = &q_all[row * cols..(row + 1) * cols];
                self.dequant_i8_row(q_row, scales[row], &mut w_buf[ri * cols..(ri + 1) * cols]);
            }
            let (w0, rest) = w_buf.split_at(cols);
            let (w1, rest) = rest.split_at(cols);
            let (w2, w3_full) = rest.split_at(cols);
            let w3 = &w3_full[..cols];

            let mut b = 0usize;
            while b + 4 <= batch {
                let x0 = &x_batch[b * cols..(b + 1) * cols];
                let x1 = &x_batch[(b + 1) * cols..(b + 2) * cols];
                let x2 = &x_batch[(b + 2) * cols..(b + 3) * cols];
                let x3 = &x_batch[(b + 3) * cols..(b + 4) * cols];

                let mut out_tile = [0.0f32; 16];
                #[cfg(target_arch = "x86_64")]
                {
                    if self.has_avx512f {
                        unsafe {
                            gemm_4x4_f32_avx512f(w0, w1, w2, w3, x0, x1, x2, x3, &mut out_tile);
                        }
                    } else if self.has_avx2_fma {
                        unsafe {
                            gemm_2x4_f32_avx2_fma(w0, w1, x0, x1, x2, x3, &mut out_tile[0..8]);
                            gemm_2x4_f32_avx2_fma(w2, w3, x0, x1, x2, x3, &mut out_tile[8..16]);
                        }
                    } else {
                        gemm_4x4_f32_scalar(w0, w1, w2, w3, x0, x1, x2, x3, &mut out_tile);
                    }
                }
                #[cfg(not(target_arch = "x86_64"))]
                gemm_4x4_f32_scalar(w0, w1, w2, w3, x0, x1, x2, x3, &mut out_tile);

                for ri in 0..4 {
                    for bi in 0..4 {
                        y_batch[(b + bi) * out_stride + (r + ri)] = out_tile[ri * 4 + bi];
                    }
                }
                b += 4;
            }
            while b < batch {
                let x0 = &x_batch[b * cols..(b + 1) * cols];
                let mut col4 = [0.0f32; 4];
                #[cfg(target_arch = "x86_64")]
                {
                    if self.has_avx512f {
                        unsafe { gemm_4x1_f32_avx512f(w0, w1, w2, w3, x0, &mut col4) };
                    } else if self.has_avx2_fma {
                        unsafe { gemm_4x1_f32_avx2_fma(w0, w1, w2, w3, x0, &mut col4) };
                    } else {
                        col4[0] = dot_f32_scalar(w0, x0);
                        col4[1] = dot_f32_scalar(w1, x0);
                        col4[2] = dot_f32_scalar(w2, x0);
                        col4[3] = dot_f32_scalar(w3, x0);
                    }
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
                    col4[0] = dot_f32_scalar(w0, x0);
                    col4[1] = dot_f32_scalar(w1, x0);
                    col4[2] = dot_f32_scalar(w2, x0);
                    col4[3] = dot_f32_scalar(w3, x0);
                }
                for ri in 0..4 {
                    y_batch[b * out_stride + (r + ri)] = col4[ri];
                }
                b += 1;
            }
            r += 4;
        }
        while r < row_end {
            let q_row = &q_all[r * cols..(r + 1) * cols];
            let w0 = &mut w_buf[..cols];
            self.dequant_i8_row(q_row, scales[r], w0);
            for b in 0..batch {
                let x0 = &x_batch[b * cols..(b + 1) * cols];
                y_batch[b * out_stride + r] = self.dot_f32(w0, x0);
            }
            r += 1;
        }
    }

    fn gemm_all(
        &self,
        mat: QuantMatrixSlice,
        x_batch: &[f32],
        batch: usize,
        y_batch: &mut [f32],
        out_stride: usize,
        pool: Option<&RlnmThreadPool>,
    ) {
        if let Some(p) = pool {
            if p.num_workers() > 1 && mat.rows >= 256 {
                p.parallel_gemm(self, mat, x_batch, y_batch, batch, out_stride);
                return;
            }
        }
        self.gemm_range(mat, x_batch, batch, 0, mat.rows, y_batch, out_stride);
    }

    fn tree_attn_range(
        &self,
        q_all: &[f32],
        k_all: &[f32],
        v_all: &[f32],
        out_all: &mut [f32],
        anc_offsets: &[usize],
        anc_list: &[usize],
        u_start: usize,
        u_end: usize,
    ) {
        let d = self.cfg.d_model;
        let n_heads = self.cfg.n_heads;
        let head_dim = self.cfg.head_dim;
        let attn_scale = 1.0f32 / (head_dim as f32).sqrt();
        let mut stack_scores = [0.0f32; 256];
        let mut heap_scores: Vec<f32> = Vec::new();

        for u in u_start..u_end {
            let a0 = anc_offsets[u];
            let a1 = anc_offsets[u + 1];
            let seq_len = a1 - a0;
            let ancs = &anc_list[a0..a1];
            let scores: &mut [f32] = if seq_len <= 256 {
                &mut stack_scores[..seq_len]
            } else {
                heap_scores.resize(seq_len, 0.0);
                &mut heap_scores[..seq_len]
            };

            for hd in 0..n_heads {
                let h_off = hd * head_dim;
                let q_head = &q_all[u * d + h_off..u * d + h_off + head_dim];
                let mut max_sc = f32::NEG_INFINITY;
                for (t_idx, &anc_u) in ancs.iter().enumerate() {
                    let k_head = &k_all[anc_u * d + h_off..anc_u * d + h_off + head_dim];
                    let sc = self.dot_f32(q_head, k_head) * attn_scale;
                    scores[t_idx] = sc;
                    if sc > max_sc {
                        max_sc = sc;
                    }
                }
                let mut sum_exp = 0.0f32;
                for sc in scores.iter_mut() {
                    let e = (*sc - max_sc).exp();
                    *sc = e;
                    sum_exp += e;
                }
                let inv_sum = 1.0f32 / sum_exp;
                let out_head = &mut out_all[u * d + h_off..u * d + h_off + head_dim];
                out_head.fill(0.0);
                for (t_idx, &anc_u) in ancs.iter().enumerate() {
                    let w = scores[t_idx] * inv_sum;
                    let v_head = &v_all[anc_u * d + h_off..anc_u * d + h_off + head_dim];
                    self.axpy_f32(w, v_head, out_head);
                }
            }
        }
    }

    /// 以字元前綴樹（Trie）共用 KV-Cache 並以樹狀批次化 GEMM 精確平行計算 `log P(cand_i | <bos> + prefix)`
    pub fn score_candidates_trie(
        &self,
        prefix: &str,
        candidates: &[&str],
        cache: &mut RlnmKvCache,
        pool: Option<&RlnmThreadPool>,
    ) -> Vec<f32> {
        let n = candidates.len();
        if n == 0 {
            return Vec::new();
        }
        let mut pref_ids = Vec::new();
        self.encode_text(prefix, &mut pref_ids);

        let mut cand_seqs: Vec<Vec<u32>> = Vec::with_capacity(n);
        let mut max_cand_len = 0usize;
        for &c in candidates {
            let mut ids = Vec::new();
            self.encode_text(c, &mut ids);
            if ids.len() > max_cand_len {
                max_cand_len = ids.len();
            }
            cand_seqs.push(ids);
        }

        let max_len = self.cfg.max_seq_len;
        let budget = max_len.saturating_sub(1 + max_cand_len);
        if pref_ids.len() > budget {
            let start = pref_ids.len() - budget;
            pref_ids.drain(0..start);
        }

        // 若個別極長候選仍超過剩餘可用上下文，退回單句安全截斷計分
        if 1 + pref_ids.len() + max_cand_len > max_len {
            return candidates
                .iter()
                .map(|&c| self.score_candidate(prefix, c, pool))
                .collect();
        }

        // 1. 建構字元 Trie
        #[derive(Default)]
        struct TrieNode {
            children: Vec<(u32, usize)>,
            terminates: Vec<usize>,
        }
        let mut nodes = vec![TrieNode::default()];
        for (cand_idx, seq) in cand_seqs.iter().enumerate() {
            let mut cur = 0usize;
            for &tok in seq {
                if let Some(&(_, child_idx)) = nodes[cur].children.iter().find(|&&(t, _)| t == tok) {
                    cur = child_idx;
                } else {
                    let next_idx = nodes.len();
                    nodes.push(TrieNode::default());
                    nodes[cur].children.push((tok, next_idx));
                    cur = next_idx;
                }
            }
            nodes[cur].terminates.push(cand_idx);
        }

        let mut out_scores = vec![0.0f32; n];
        for &ci in &nodes[0].terminates {
            out_scores[ci] = 0.0;
        }
        if nodes[0].children.is_empty() {
            return out_scores;
        }

        // 2. 攤平前置語境與非葉節點（拓撲順序），建立唯一批次 `0..M` 與祖先索引表
        let p_len = pref_ids.len();
        let mut batch_toks: Vec<u32> = Vec::with_capacity(1 + p_len + nodes.len());
        let mut batch_parent: Vec<Option<usize>> = Vec::with_capacity(1 + p_len + nodes.len());
        let mut batch_pos: Vec<usize> = Vec::with_capacity(1 + p_len + nodes.len());
        let mut batch_trie: Vec<usize> = Vec::with_capacity(1 + p_len + nodes.len());

        batch_toks.push(BOS_ID);
        batch_parent.push(None);
        batch_pos.push(0);
        batch_trie.push(usize::MAX);
        for (j, &pid) in pref_ids.iter().enumerate() {
            batch_toks.push(pid);
            batch_parent.push(Some(j));
            batch_pos.push(j + 1);
            batch_trie.push(usize::MAX);
        }
        let start_logit = p_len;
        batch_trie[start_logit] = 0;

        let mut bfs_queue: std::collections::VecDeque<(usize, usize)> = std::collections::VecDeque::new();
        bfs_queue.push_back((start_logit, 0));
        while let Some((p_batch, t_idx)) = bfs_queue.pop_front() {
            let p_depth = batch_pos[p_batch];
            for &(ctok, c_trie) in &nodes[t_idx].children {
                if !nodes[c_trie].children.is_empty() {
                    let b_idx = batch_toks.len();
                    batch_toks.push(ctok);
                    batch_parent.push(Some(p_batch));
                    batch_pos.push(p_depth + 1);
                    batch_trie.push(c_trie);
                    bfs_queue.push_back((b_idx, c_trie));
                }
            }
        }

        let m_batch = batch_toks.len();
        let mut anc_offsets: Vec<usize> = Vec::with_capacity(m_batch + 1);
        let mut anc_list: Vec<usize> = Vec::with_capacity(m_batch * 12);
        anc_offsets.push(0);
        for u in 0..m_batch {
            let start = anc_list.len();
            if let Some(p) = batch_parent[u] {
                let p0 = anc_offsets[p];
                let p1 = anc_offsets[p + 1];
                anc_list.extend_from_within(p0..p1);
            }
            anc_list.push(u);
            debug_assert_eq!(anc_list.len() - start, batch_pos[u] + 1);
            anc_offsets.push(anc_list.len());
        }

        // 3. 擴充共用批次工作緩衝區並執行一次載入 Embedding + N 層樹狀批次前向
        let d = self.cfg.d_model;
        let f = self.cfg.d_ff;
        let vocab_size = self.cfg.vocab_size;
        let bws = &mut cache.bws;
        bws.ensure_capacity(m_batch, d, f, vocab_size);

        for (u, &tok) in batch_toks.iter().enumerate() {
            self.load_token_embedding(tok, &mut bws.x[u * d..(u + 1) * d]);
        }

        let use_pool = pool.filter(|p| p.num_workers() > 1);
        for layer in &self.layers {
            for u in 0..m_batch {
                self.rmsnorm(
                    &bws.x[u * d..(u + 1) * d],
                    layer.off_attn_norm,
                    &mut bws.h[u * d..(u + 1) * d],
                );
            }

            if let Some(p) = use_pool {
                p.parallel_gemm_qkv(
                    self,
                    layer.wq,
                    layer.wk,
                    layer.wv,
                    &bws.h[..m_batch * d],
                    &mut bws.q[..m_batch * d],
                    &mut bws.k[..m_batch * d],
                    &mut bws.v[..m_batch * d],
                    m_batch,
                );
            } else {
                self.gemm_range(layer.wq, &bws.h[..m_batch * d], m_batch, 0, d, &mut bws.q[..m_batch * d], d);
                self.gemm_range(layer.wk, &bws.h[..m_batch * d], m_batch, 0, d, &mut bws.k[..m_batch * d], d);
                self.gemm_range(layer.wv, &bws.h[..m_batch * d], m_batch, 0, d, &mut bws.v[..m_batch * d], d);
            }

            for (u, &pos) in batch_pos.iter().enumerate() {
                self.apply_rope(&mut bws.q[u * d..(u + 1) * d], pos);
                self.apply_rope(&mut bws.k[u * d..(u + 1) * d], pos);
            }

            if let Some(p) = use_pool {
                if m_batch >= 4 {
                    p.parallel_tree_attn(
                        self,
                        &bws.q[..m_batch * d],
                        &bws.k[..m_batch * d],
                        &bws.v[..m_batch * d],
                        &mut bws.attn_out[..m_batch * d],
                        &anc_offsets,
                        &anc_list,
                        m_batch,
                    );
                } else {
                    self.tree_attn_range(
                        &bws.q[..m_batch * d],
                        &bws.k[..m_batch * d],
                        &bws.v[..m_batch * d],
                        &mut bws.attn_out[..m_batch * d],
                        &anc_offsets,
                        &anc_list,
                        0,
                        m_batch,
                    );
                }
            } else {
                self.tree_attn_range(
                    &bws.q[..m_batch * d],
                    &bws.k[..m_batch * d],
                    &bws.v[..m_batch * d],
                    &mut bws.attn_out[..m_batch * d],
                    &anc_offsets,
                    &anc_list,
                    0,
                    m_batch,
                );
            }

            self.gemm_all(
                layer.wo,
                &bws.attn_out[..m_batch * d],
                m_batch,
                &mut bws.h[..m_batch * d],
                d,
                use_pool,
            );
            for i in 0..m_batch * d {
                bws.x[i] += bws.h[i];
            }

            for u in 0..m_batch {
                self.rmsnorm(
                    &bws.x[u * d..(u + 1) * d],
                    layer.off_ffn_norm,
                    &mut bws.h[u * d..(u + 1) * d],
                );
            }

            if let Some(p) = use_pool {
                p.parallel_gemm_gate_up(
                    self,
                    layer.w_gate,
                    layer.w_up,
                    &bws.h[..m_batch * d],
                    &mut bws.gate[..m_batch * f],
                    &mut bws.up[..m_batch * f],
                    m_batch,
                );
            } else {
                self.gemm_range(
                    layer.w_gate,
                    &bws.h[..m_batch * d],
                    m_batch,
                    0,
                    f,
                    &mut bws.gate[..m_batch * f],
                    f,
                );
                self.gemm_range(
                    layer.w_up,
                    &bws.h[..m_batch * d],
                    m_batch,
                    0,
                    f,
                    &mut bws.up[..m_batch * f],
                    f,
                );
            }

            for i in 0..m_batch * f {
                let g = bws.gate[i];
                let sig = 1.0f32 / (1.0f32 + (-g.clamp(-60.0, 60.0)).exp());
                bws.gate[i] = (g * sig) * bws.up[i];
            }

            self.gemm_all(
                layer.w_down,
                &bws.gate[..m_batch * f],
                m_batch,
                &mut bws.h[..m_batch * d],
                d,
                use_pool,
            );
            for i in 0..m_batch * d {
                bws.x[i] += bws.h[i];
            }
        }

        // 4. 分塊（CHUNK=64）執行最終 RMSNorm + `lm_head` 並沿 Trie 拓撲順序累積邊機率
        let n_logit = m_batch - start_logit;
        bws.ensure_logit_capacity(n_logit, d, vocab_size);
        for (idx, u) in (start_logit..m_batch).enumerate() {
            self.rmsnorm(
                &bws.x[u * d..(u + 1) * d],
                self.off_norm_final,
                &mut bws.h_logits[idx * d..(idx + 1) * d],
            );
        }

        let mut node_lp = vec![0.0f32; nodes.len()];
        const LOGIT_CHUNK: usize = 64;
        let mut c0 = 0usize;
        while c0 < n_logit {
            let c1 = (c0 + LOGIT_CHUNK).min(n_logit);
            let clen = c1 - c0;
            self.gemm_all(
                self.tok_emb,
                &bws.h_logits[c0 * d..c1 * d],
                clen,
                &mut bws.logits_chunk[..clen * vocab_size],
                vocab_size,
                use_pool,
            );
            for local_i in 0..clen {
                let u = start_logit + c0 + local_i;
                let t_idx = batch_trie[u];
                let logits_row =
                    &bws.logits_chunk[local_i * vocab_size..(local_i + 1) * vocab_size];
                let mut max_v = f32::NEG_INFINITY;
                for &v in logits_row {
                    if v > max_v {
                        max_v = v;
                    }
                }
                let mut sum_exp = 0.0f64;
                for &v in logits_row {
                    sum_exp += ((v - max_v) as f64).exp();
                }
                let lse = (max_v as f64) + sum_exp.ln();
                let base_lp = node_lp[t_idx];
                for &(ctok, c_trie) in &nodes[t_idx].children {
                    let edge_lp = ((logits_row[ctok as usize] as f64) - lse) as f32;
                    node_lp[c_trie] = base_lp + edge_lp;
                }
            }
            c0 = c1;
        }

        for (t_idx, nd) in nodes.iter().enumerate() {
            let lp = node_lp[t_idx];
            for &ci in &nd.terminates {
                out_scores[ci] = lp;
            }
        }
        out_scores
    }

    pub fn alloc_kv_cache(&self) -> RlnmKvCache {
        let kv_len = self.cfg.n_layers * self.cfg.max_seq_len * self.cfg.d_model;
        RlnmKvCache {
            k_cache: vec![0.0; kv_len],
            v_cache: vec![0.0; kv_len],
            ws: WorkSpace {
                x: vec![0.0; self.cfg.d_model],
                h: vec![0.0; self.cfg.d_model],
                q: vec![0.0; self.cfg.d_model],
                k: vec![0.0; self.cfg.d_model],
                v: vec![0.0; self.cfg.d_model],
                attn_out: vec![0.0; self.cfg.d_model],
                attn_scores: vec![0.0; self.cfg.max_seq_len],
                gate: vec![0.0; self.cfg.d_ff],
                up: vec![0.0; self.cfg.d_ff],
                logits: vec![0.0; self.cfg.vocab_size],
            },
            bws: BatchWorkSpace::default(),
        }
    }
}

struct WorkSpace {
    x: Vec<f32>,
    h: Vec<f32>,
    q: Vec<f32>,
    k: Vec<f32>,
    v: Vec<f32>,
    attn_out: Vec<f32>,
    attn_scores: Vec<f32>,
    gate: Vec<f32>,
    up: Vec<f32>,
    logits: Vec<f32>,
}

#[derive(Default)]
struct BatchWorkSpace {
    x: Vec<f32>,
    h: Vec<f32>,
    q: Vec<f32>,
    k: Vec<f32>,
    v: Vec<f32>,
    attn_out: Vec<f32>,
    gate: Vec<f32>,
    up: Vec<f32>,
    h_logits: Vec<f32>,
    logits_chunk: Vec<f32>,
}

impl BatchWorkSpace {
    fn ensure_capacity(&mut self, batch: usize, d: usize, f: usize, _vocab_size: usize) {
        let bd = batch * d;
        let bf = batch * f;
        if self.x.len() < bd {
            self.x.resize(bd, 0.0);
        }
        if self.h.len() < bd {
            self.h.resize(bd, 0.0);
        }
        if self.q.len() < bd {
            self.q.resize(bd, 0.0);
        }
        if self.k.len() < bd {
            self.k.resize(bd, 0.0);
        }
        if self.v.len() < bd {
            self.v.resize(bd, 0.0);
        }
        if self.attn_out.len() < bd {
            self.attn_out.resize(bd, 0.0);
        }
        if self.gate.len() < bf {
            self.gate.resize(bf, 0.0);
        }
        if self.up.len() < bf {
            self.up.resize(bf, 0.0);
        }
    }

    fn ensure_logit_capacity(&mut self, n_logit: usize, d: usize, vocab_size: usize) {
        let ld = n_logit * d;
        if self.h_logits.len() < ld {
            self.h_logits.resize(ld, 0.0);
        }
        let chunk_elems = n_logit.min(64) * vocab_size;
        if self.logits_chunk.len() < chunk_elems {
            self.logits_chunk.resize(chunk_elems, 0.0);
        }
    }
}

pub struct RlnmKvCache {
    k_cache: Vec<f32>,
    v_cache: Vec<f32>,
    ws: WorkSpace,
    bws: BatchWorkSpace,
}

#[derive(Clone, Copy)]
enum TaskKind {
    None,
    Matvec {
        mat: QuantMatrixSlice,
        x_ptr: *const f32,
        out_ptr: *mut f32,
    },
    Qkv {
        wq: QuantMatrixSlice,
        wk: QuantMatrixSlice,
        wv: QuantMatrixSlice,
        x_ptr: *const f32,
        q_ptr: *mut f32,
        k_ptr: *mut f32,
        v_ptr: *mut f32,
    },
    GateUp {
        wg: QuantMatrixSlice,
        wu: QuantMatrixSlice,
        x_ptr: *const f32,
        g_ptr: *mut f32,
        u_ptr: *mut f32,
    },
    Gemm {
        mat: QuantMatrixSlice,
        x_ptr: *const f32,
        y_ptr: *mut f32,
        batch: usize,
        out_stride: usize,
    },
    GemmQkv {
        wq: QuantMatrixSlice,
        wk: QuantMatrixSlice,
        wv: QuantMatrixSlice,
        x_ptr: *const f32,
        q_ptr: *mut f32,
        k_ptr: *mut f32,
        v_ptr: *mut f32,
        batch: usize,
    },
    GemmGateUp {
        wg: QuantMatrixSlice,
        wu: QuantMatrixSlice,
        x_ptr: *const f32,
        g_ptr: *mut f32,
        u_ptr: *mut f32,
        batch: usize,
    },
    TreeAttn {
        q_ptr: *const f32,
        k_ptr: *const f32,
        v_ptr: *const f32,
        out_ptr: *mut f32,
        anc_off_ptr: *const usize,
        anc_list_ptr: *const usize,
        batch: usize,
        anc_total: usize,
    },
}

unsafe impl Send for TaskKind {}
unsafe impl Sync for TaskKind {}

struct PoolShared {
    epoch: AtomicUsize,
    done_count: AtomicUsize,
    stop: AtomicBool,
    sleepers: AtomicUsize,
    model_ptr: AtomicUsize,
    task: std::cell::UnsafeCell<TaskKind>,
    sleep_mu: Mutex<()>,
    cond: Condvar,
}

unsafe impl Sync for PoolShared {}

/// 專為單筆即時重排設計之無鎖自旋屏障與持久執行緒池（零請求內互斥鎖爭奪）
pub struct RlnmThreadPool {
    n_threads: usize,
    shared: Arc<PoolShared>,
    handles: Vec<JoinHandle<()>>,
    exec_lock: Mutex<()>,
}

#[inline]
fn align16_partition(rows: usize, tid: usize, total_workers: usize) -> (usize, usize) {
    let blocks = (rows + 15) / 16;
    let r0 = ((blocks * tid) / total_workers) * 16;
    let r1 = (((blocks * (tid + 1)) / total_workers) * 16).min(rows);
    (r0.min(rows), r1)
}

impl RlnmThreadPool {
    pub fn new(n_threads: usize) -> Self {
        let n_threads = n_threads.clamp(1, 16);
        let shared = Arc::new(PoolShared {
            epoch: AtomicUsize::new(0),
            done_count: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            sleepers: AtomicUsize::new(0),
            model_ptr: AtomicUsize::new(0),
            task: std::cell::UnsafeCell::new(TaskKind::None),
            sleep_mu: Mutex::new(()),
            cond: Condvar::new(),
        });
        let mut handles = Vec::with_capacity(n_threads.saturating_sub(1));
        for tid in 1..n_threads {
            let sh = Arc::clone(&shared);
            handles.push(thread::spawn(move || {
                let mut seen_epoch = 0usize;
                loop {
                    let mut got = false;
                    for _ in 0..8192 {
                        if sh.stop.load(Ordering::Acquire) {
                            return;
                        }
                        let cur = sh.epoch.load(Ordering::Acquire);
                        if cur != seen_epoch {
                            seen_epoch = cur;
                            got = true;
                            break;
                        }
                        std::hint::spin_loop();
                    }
                    if !got {
                        let mut guard = sh.sleep_mu.lock().unwrap_or_else(|e| e.into_inner());
                        sh.sleepers.fetch_add(1, Ordering::SeqCst);
                        while sh.epoch.load(Ordering::SeqCst) == seen_epoch
                            && !sh.stop.load(Ordering::SeqCst)
                        {
                            guard = sh.cond.wait(guard).unwrap_or_else(|e| e.into_inner());
                        }
                        sh.sleepers.fetch_sub(1, Ordering::SeqCst);
                        if sh.stop.load(Ordering::Acquire) {
                            return;
                        }
                        seen_epoch = sh.epoch.load(Ordering::Acquire);
                    }
                    if sh.stop.load(Ordering::Acquire) {
                        return;
                    }
                    // SAFETY: 主執行緒於寫入 task 與 model_ptr 後才以 SeqCst 遞增 epoch，並等待全部 worker 遞增 done_count 才覆寫下一次任務。
                    let task = unsafe { *sh.task.get() };
                    let mptr = sh.model_ptr.load(Ordering::Acquire) as *const RlnmModel;
                    if !mptr.is_null() {
                        let model = unsafe { &*mptr };
                        Self::execute_slice(model, task, tid, n_threads);
                    }
                    sh.done_count.fetch_add(1, Ordering::Release);
                }
            }));
        }
        Self {
            n_threads,
            shared,
            handles,
            exec_lock: Mutex::new(()),
        }
    }

    #[inline]
    pub fn num_workers(&self) -> usize {
        self.n_threads
    }

    fn execute_slice(model: &RlnmModel, task: TaskKind, tid: usize, total_workers: usize) {
        match task {
            TaskKind::None => {}
            TaskKind::Matvec { mat, x_ptr, out_ptr } => {
                let (r0, r1) = align16_partition(mat.rows, tid, total_workers);
                if r1 > r0 {
                    let x = unsafe { std::slice::from_raw_parts(x_ptr, mat.cols) };
                    let out = unsafe { std::slice::from_raw_parts_mut(out_ptr.add(r0), r1 - r0) };
                    model.matvec_range(mat, x, r0, out);
                }
            }
            TaskKind::Qkv {
                wq,
                wk,
                wv,
                x_ptr,
                q_ptr,
                k_ptr,
                v_ptr,
            } => {
                let d = wq.rows;
                let (g0, g1) = align16_partition(d * 3, tid, total_workers);
                let x = unsafe { std::slice::from_raw_parts(x_ptr, wq.cols) };
                for (mat, dst_ptr, base) in [(wq, q_ptr, 0usize), (wk, k_ptr, d), (wv, v_ptr, d * 2)] {
                    let s0 = g0.max(base);
                    let s1 = g1.min(base + d);
                    if s1 > s0 {
                        let r0 = s0 - base;
                        let r1 = s1 - base;
                        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr.add(r0), r1 - r0) };
                        model.matvec_range(mat, x, r0, out);
                    }
                }
            }
            TaskKind::GateUp {
                wg,
                wu,
                x_ptr,
                g_ptr,
                u_ptr,
            } => {
                let f = wg.rows;
                let (g0, g1) = align16_partition(f * 2, tid, total_workers);
                let x = unsafe { std::slice::from_raw_parts(x_ptr, wg.cols) };
                for (mat, dst_ptr, base) in [(wg, g_ptr, 0usize), (wu, u_ptr, f)] {
                    let s0 = g0.max(base);
                    let s1 = g1.min(base + f);
                    if s1 > s0 {
                        let r0 = s0 - base;
                        let r1 = s1 - base;
                        let out = unsafe { std::slice::from_raw_parts_mut(dst_ptr.add(r0), r1 - r0) };
                        model.matvec_range(mat, x, r0, out);
                    }
                }
            }
            TaskKind::Gemm {
                mat,
                x_ptr,
                y_ptr,
                batch,
                out_stride,
            } => {
                let (r0, r1) = align16_partition(mat.rows, tid, total_workers);
                if r1 > r0 {
                    let x = unsafe { std::slice::from_raw_parts(x_ptr, batch * mat.cols) };
                    let y = unsafe { std::slice::from_raw_parts_mut(y_ptr, batch * out_stride) };
                    model.gemm_range(mat, x, batch, r0, r1, y, out_stride);
                }
            }
            TaskKind::GemmQkv {
                wq,
                wk,
                wv,
                x_ptr,
                q_ptr,
                k_ptr,
                v_ptr,
                batch,
            } => {
                let d = wq.rows;
                let (g0, g1) = align16_partition(d * 3, tid, total_workers);
                let x = unsafe { std::slice::from_raw_parts(x_ptr, batch * wq.cols) };
                for (mat, dst_ptr, base) in [(wq, q_ptr, 0usize), (wk, k_ptr, d), (wv, v_ptr, d * 2)] {
                    let s0 = g0.max(base);
                    let s1 = g1.min(base + d);
                    if s1 > s0 {
                        let r0 = s0 - base;
                        let r1 = s1 - base;
                        let y = unsafe { std::slice::from_raw_parts_mut(dst_ptr, batch * d) };
                        model.gemm_range(mat, x, batch, r0, r1, y, d);
                    }
                }
            }
            TaskKind::GemmGateUp {
                wg,
                wu,
                x_ptr,
                g_ptr,
                u_ptr,
                batch,
            } => {
                let f = wg.rows;
                let (g0, g1) = align16_partition(f * 2, tid, total_workers);
                let x = unsafe { std::slice::from_raw_parts(x_ptr, batch * wg.cols) };
                for (mat, dst_ptr, base) in [(wg, g_ptr, 0usize), (wu, u_ptr, f)] {
                    let s0 = g0.max(base);
                    let s1 = g1.min(base + f);
                    if s1 > s0 {
                        let r0 = s0 - base;
                        let r1 = s1 - base;
                        let y = unsafe { std::slice::from_raw_parts_mut(dst_ptr, batch * f) };
                        model.gemm_range(mat, x, batch, r0, r1, y, f);
                    }
                }
            }
            TaskKind::TreeAttn {
                q_ptr,
                k_ptr,
                v_ptr,
                out_ptr,
                anc_off_ptr,
                anc_list_ptr,
                batch,
                anc_total,
            } => {
                let u0 = (batch * tid) / total_workers;
                let u1 = (batch * (tid + 1)) / total_workers;
                if u1 > u0 {
                    let d = model.cfg.d_model;
                    let q = unsafe { std::slice::from_raw_parts(q_ptr, batch * d) };
                    let k = unsafe { std::slice::from_raw_parts(k_ptr, batch * d) };
                    let v = unsafe { std::slice::from_raw_parts(v_ptr, batch * d) };
                    let out = unsafe { std::slice::from_raw_parts_mut(out_ptr, batch * d) };
                    let anc_off = unsafe { std::slice::from_raw_parts(anc_off_ptr, batch + 1) };
                    let anc_list = unsafe { std::slice::from_raw_parts(anc_list_ptr, anc_total) };
                    model.tree_attn_range(q, k, v, out, anc_off, anc_list, u0, u1);
                }
            }
        }
    }

    fn run_task(&self, model: &RlnmModel, task: TaskKind) {
        let _guard = self.exec_lock.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            *self.shared.task.get() = task;
        }
        self.shared
            .model_ptr
            .store(model as *const RlnmModel as usize, Ordering::Release);
        self.shared.done_count.store(0, Ordering::Relaxed);
        self.shared.epoch.fetch_add(1, Ordering::SeqCst);
        if self.shared.sleepers.load(Ordering::SeqCst) > 0 {
            let _g = self.shared.sleep_mu.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.cond.notify_all();
        }
        Self::execute_slice(model, task, 0, self.n_threads);
        let needed = self.n_threads - 1;
        while self.shared.done_count.load(Ordering::Acquire) < needed {
            std::hint::spin_loop();
        }
        self.shared.model_ptr.store(0, Ordering::Release);
    }

    fn parallel_matvec(&self, model: &RlnmModel, mat: QuantMatrixSlice, x: &[f32], out: &mut [f32]) {
        self.run_task(
            model,
            TaskKind::Matvec {
                mat,
                x_ptr: x.as_ptr(),
                out_ptr: out.as_mut_ptr(),
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_qkv(
        &self,
        model: &RlnmModel,
        wq: QuantMatrixSlice,
        wk: QuantMatrixSlice,
        wv: QuantMatrixSlice,
        x: &[f32],
        q: &mut [f32],
        k: &mut [f32],
        v: &mut [f32],
    ) {
        self.run_task(
            model,
            TaskKind::Qkv {
                wq,
                wk,
                wv,
                x_ptr: x.as_ptr(),
                q_ptr: q.as_mut_ptr(),
                k_ptr: k.as_mut_ptr(),
                v_ptr: v.as_mut_ptr(),
            },
        );
    }

    fn parallel_gate_up(
        &self,
        model: &RlnmModel,
        wg: QuantMatrixSlice,
        wu: QuantMatrixSlice,
        x: &[f32],
        g: &mut [f32],
        u: &mut [f32],
    ) {
        self.run_task(
            model,
            TaskKind::GateUp {
                wg,
                wu,
                x_ptr: x.as_ptr(),
                g_ptr: g.as_mut_ptr(),
                u_ptr: u.as_mut_ptr(),
            },
        );
    }

    fn parallel_gemm(
        &self,
        model: &RlnmModel,
        mat: QuantMatrixSlice,
        x: &[f32],
        y: &mut [f32],
        batch: usize,
        out_stride: usize,
    ) {
        self.run_task(
            model,
            TaskKind::Gemm {
                mat,
                x_ptr: x.as_ptr(),
                y_ptr: y.as_mut_ptr(),
                batch,
                out_stride,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_gemm_qkv(
        &self,
        model: &RlnmModel,
        wq: QuantMatrixSlice,
        wk: QuantMatrixSlice,
        wv: QuantMatrixSlice,
        x: &[f32],
        q: &mut [f32],
        k: &mut [f32],
        v: &mut [f32],
        batch: usize,
    ) {
        self.run_task(
            model,
            TaskKind::GemmQkv {
                wq,
                wk,
                wv,
                x_ptr: x.as_ptr(),
                q_ptr: q.as_mut_ptr(),
                k_ptr: k.as_mut_ptr(),
                v_ptr: v.as_mut_ptr(),
                batch,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_gemm_gate_up(
        &self,
        model: &RlnmModel,
        wg: QuantMatrixSlice,
        wu: QuantMatrixSlice,
        x: &[f32],
        g: &mut [f32],
        u: &mut [f32],
        batch: usize,
    ) {
        self.run_task(
            model,
            TaskKind::GemmGateUp {
                wg,
                wu,
                x_ptr: x.as_ptr(),
                g_ptr: g.as_mut_ptr(),
                u_ptr: u.as_mut_ptr(),
                batch,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_tree_attn(
        &self,
        model: &RlnmModel,
        q: &[f32],
        k: &[f32],
        v: &[f32],
        out: &mut [f32],
        anc_off: &[usize],
        anc_list: &[usize],
        batch: usize,
    ) {
        self.run_task(
            model,
            TaskKind::TreeAttn {
                q_ptr: q.as_ptr(),
                k_ptr: k.as_ptr(),
                v_ptr: v.as_ptr(),
                out_ptr: out.as_mut_ptr(),
                anc_off_ptr: anc_off.as_ptr(),
                anc_list_ptr: anc_list.as_ptr(),
                batch,
                anc_total: anc_list.len(),
            },
        );
    }
}

impl Drop for RlnmThreadPool {
    fn drop(&mut self) {
        {
            let _g = self.shared.sleep_mu.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.stop.store(true, Ordering::SeqCst);
            self.shared.cond.notify_all();
        }
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

#[inline]
fn dot_f32_scalar(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len();
    let mut s0 = 0.0f32;
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    let mut s3 = 0.0f32;
    let mut i = 0usize;
    while i + 4 <= n {
        s0 += a[i] * b[i];
        s1 += a[i + 1] * b[i + 1];
        s2 += a[i + 2] * b[i + 2];
        s3 += a[i + 3] * b[i + 3];
        i += 4;
    }
    let mut sum = (s0 + s1) + (s2 + s3);
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn gemm_4x4_f32_scalar(
    w0: &[f32],
    w1: &[f32],
    w2: &[f32],
    w3: &[f32],
    x0: &[f32],
    x1: &[f32],
    x2: &[f32],
    x3: &[f32],
    out_tile: &mut [f32; 16],
) {
    let ws = [w0, w1, w2, w3];
    let xs = [x0, x1, x2, x3];
    for ri in 0..4 {
        for bi in 0..4 {
            out_tile[ri * 4 + bi] = dot_f32_scalar(ws[ri], xs[bi]);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dequant_i8_row_avx2(q_row: &[i8], scale: f32, dst: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = q_row.len();
    let qp = q_row.as_ptr();
    let dp = dst.as_mut_ptr();
    let vs = _mm256_set1_ps(scale);
    let mut i = 0usize;
    while i + 16 <= n {
        let q0 = _mm_loadl_epi64(qp.add(i) as *const __m128i);
        let q1 = _mm_loadl_epi64(qp.add(i + 8) as *const __m128i);
        let f0 = _mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q0)), vs);
        let f1 = _mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q1)), vs);
        _mm256_storeu_ps(dp.add(i), f0);
        _mm256_storeu_ps(dp.add(i + 8), f1);
        i += 16;
    }
    while i + 8 <= n {
        let q0 = _mm_loadl_epi64(qp.add(i) as *const __m128i);
        let f0 = _mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q0)), vs);
        _mm256_storeu_ps(dp.add(i), f0);
        i += 8;
    }
    while i < n {
        *dp.add(i) = (*qp.add(i) as f32) * scale;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn hsum_ps256(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;
    let hi128 = _mm256_extractf128_ps(v, 1);
    let lo128 = _mm256_castps256_ps128(v);
    let s4 = _mm_add_ps(lo128, hi128);
    let s2 = _mm_add_ps(s4, _mm_movehl_ps(s4, s4));
    let s1 = _mm_add_ss(s2, _mm_shuffle_ps(s2, s2, 1));
    _mm_cvtss_f32(s1)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_f32_avx2_fma(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = a.len();
    let ap = a.as_ptr();
    let bp = b.as_ptr();
    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut i = 0usize;
    while i + 16 <= n {
        acc0 = _mm256_fmadd_ps(_mm256_loadu_ps(ap.add(i)), _mm256_loadu_ps(bp.add(i)), acc0);
        acc1 = _mm256_fmadd_ps(_mm256_loadu_ps(ap.add(i + 8)), _mm256_loadu_ps(bp.add(i + 8)), acc1);
        i += 16;
    }
    while i + 8 <= n {
        acc0 = _mm256_fmadd_ps(_mm256_loadu_ps(ap.add(i)), _mm256_loadu_ps(bp.add(i)), acc0);
        i += 8;
    }
    let mut sum = hsum_ps256(_mm256_add_ps(acc0, acc1));
    while i < n {
        sum += (*ap.add(i)) * (*bp.add(i));
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn axpy_f32_avx2_fma(alpha: f32, x: &[f32], y: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = x.len();
    let xp = x.as_ptr();
    let yp = y.as_mut_ptr();
    let va = _mm256_set1_ps(alpha);
    let mut i = 0usize;
    while i + 16 <= n {
        let y0 = _mm256_loadu_ps(yp.add(i));
        let y1 = _mm256_loadu_ps(yp.add(i + 8));
        let r0 = _mm256_fmadd_ps(va, _mm256_loadu_ps(xp.add(i)), y0);
        let r1 = _mm256_fmadd_ps(va, _mm256_loadu_ps(xp.add(i + 8)), y1);
        _mm256_storeu_ps(yp.add(i), r0);
        _mm256_storeu_ps(yp.add(i + 8), r1);
        i += 16;
    }
    while i + 8 <= n {
        let y0 = _mm256_loadu_ps(yp.add(i));
        let r0 = _mm256_fmadd_ps(va, _mm256_loadu_ps(xp.add(i)), y0);
        _mm256_storeu_ps(yp.add(i), r0);
        i += 8;
    }
    while i < n {
        *yp.add(i) += alpha * (*xp.add(i));
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn gemm_2x4_f32_avx2_fma(
    w0: &[f32],
    w1: &[f32],
    x0: &[f32],
    x1: &[f32],
    x2: &[f32],
    x3: &[f32],
    out8: &mut [f32],
) {
    use std::arch::x86_64::*;
    let n = w0.len();
    let (p0, p1) = (w0.as_ptr(), w1.as_ptr());
    let (q0, q1, q2, q3) = (x0.as_ptr(), x1.as_ptr(), x2.as_ptr(), x3.as_ptr());

    let mut c00 = _mm256_setzero_ps();
    let mut c01 = _mm256_setzero_ps();
    let mut c02 = _mm256_setzero_ps();
    let mut c03 = _mm256_setzero_ps();
    let mut c10 = _mm256_setzero_ps();
    let mut c11 = _mm256_setzero_ps();
    let mut c12 = _mm256_setzero_ps();
    let mut c13 = _mm256_setzero_ps();

    let mut k = 0usize;
    while k + 8 <= n {
        let vw0 = _mm256_loadu_ps(p0.add(k));
        let vw1 = _mm256_loadu_ps(p1.add(k));

        let vx0 = _mm256_loadu_ps(q0.add(k));
        c00 = _mm256_fmadd_ps(vw0, vx0, c00);
        c10 = _mm256_fmadd_ps(vw1, vx0, c10);

        let vx1 = _mm256_loadu_ps(q1.add(k));
        c01 = _mm256_fmadd_ps(vw0, vx1, c01);
        c11 = _mm256_fmadd_ps(vw1, vx1, c11);

        let vx2 = _mm256_loadu_ps(q2.add(k));
        c02 = _mm256_fmadd_ps(vw0, vx2, c02);
        c12 = _mm256_fmadd_ps(vw1, vx2, c12);

        let vx3 = _mm256_loadu_ps(q3.add(k));
        c03 = _mm256_fmadd_ps(vw0, vx3, c03);
        c13 = _mm256_fmadd_ps(vw1, vx3, c13);

        k += 8;
    }

    let mut s00 = hsum_ps256(c00);
    let mut s01 = hsum_ps256(c01);
    let mut s02 = hsum_ps256(c02);
    let mut s03 = hsum_ps256(c03);
    let mut s10 = hsum_ps256(c10);
    let mut s11 = hsum_ps256(c11);
    let mut s12 = hsum_ps256(c12);
    let mut s13 = hsum_ps256(c13);

    while k < n {
        let rw0 = *p0.add(k);
        let rw1 = *p1.add(k);
        let rx0 = *q0.add(k);
        let rx1 = *q1.add(k);
        let rx2 = *q2.add(k);
        let rx3 = *q3.add(k);
        s00 += rw0 * rx0;
        s01 += rw0 * rx1;
        s02 += rw0 * rx2;
        s03 += rw0 * rx3;
        s10 += rw1 * rx0;
        s11 += rw1 * rx1;
        s12 += rw1 * rx2;
        s13 += rw1 * rx3;
        k += 1;
    }

    out8[0] = s00;
    out8[1] = s01;
    out8[2] = s02;
    out8[3] = s03;
    out8[4] = s10;
    out8[5] = s11;
    out8[6] = s12;
    out8[7] = s13;
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn gemm_4x1_f32_avx2_fma(
    w0: &[f32],
    w1: &[f32],
    w2: &[f32],
    w3: &[f32],
    x0: &[f32],
    out4: &mut [f32; 4],
) {
    use std::arch::x86_64::*;
    let n = w0.len();
    let (p0, p1, p2, p3) = (w0.as_ptr(), w1.as_ptr(), w2.as_ptr(), w3.as_ptr());
    let xp = x0.as_ptr();
    let mut c0 = _mm256_setzero_ps();
    let mut c1 = _mm256_setzero_ps();
    let mut c2 = _mm256_setzero_ps();
    let mut c3 = _mm256_setzero_ps();
    let mut k = 0usize;
    while k + 8 <= n {
        let vx = _mm256_loadu_ps(xp.add(k));
        c0 = _mm256_fmadd_ps(_mm256_loadu_ps(p0.add(k)), vx, c0);
        c1 = _mm256_fmadd_ps(_mm256_loadu_ps(p1.add(k)), vx, c1);
        c2 = _mm256_fmadd_ps(_mm256_loadu_ps(p2.add(k)), vx, c2);
        c3 = _mm256_fmadd_ps(_mm256_loadu_ps(p3.add(k)), vx, c3);
        k += 8;
    }
    let mut s0 = hsum_ps256(c0);
    let mut s1 = hsum_ps256(c1);
    let mut s2 = hsum_ps256(c2);
    let mut s3 = hsum_ps256(c3);
    while k < n {
        let rx = *xp.add(k);
        s0 += (*p0.add(k)) * rx;
        s1 += (*p1.add(k)) * rx;
        s2 += (*p2.add(k)) * rx;
        s3 += (*p3.add(k)) * rx;
        k += 1;
    }
    out4[0] = s0;
    out4[1] = s1;
    out4[2] = s2;
    out4[3] = s3;
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx512f")]
unsafe fn hsum_ps512(v: std::arch::x86_64::__m512) -> f32 {
    use std::arch::x86_64::*;
    let v0 = _mm512_castps512_ps128(v);
    let v1 = _mm512_extractf32x4_ps::<1>(v);
    let v2 = _mm512_extractf32x4_ps::<2>(v);
    let v3 = _mm512_extractf32x4_ps::<3>(v);
    let s4 = _mm_add_ps(_mm_add_ps(v0, v1), _mm_add_ps(v2, v3));
    let s2 = _mm_add_ps(s4, _mm_movehl_ps(s4, s4));
    let s1 = _mm_add_ss(s2, _mm_shuffle_ps(s2, s2, 1));
    _mm_cvtss_f32(s1)
}

#[cfg(target_arch = "x86_64")]
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx512f")]
unsafe fn gemm_4x4_f32_avx512f(
    w0: &[f32],
    w1: &[f32],
    w2: &[f32],
    w3: &[f32],
    x0: &[f32],
    x1: &[f32],
    x2: &[f32],
    x3: &[f32],
    out16: &mut [f32; 16],
) {
    use std::arch::x86_64::*;
    let n = w0.len();
    let (p0, p1, p2, p3) = (w0.as_ptr(), w1.as_ptr(), w2.as_ptr(), w3.as_ptr());
    let (q0, q1, q2, q3) = (x0.as_ptr(), x1.as_ptr(), x2.as_ptr(), x3.as_ptr());

    let mut c00 = _mm512_setzero_ps();
    let mut c01 = _mm512_setzero_ps();
    let mut c02 = _mm512_setzero_ps();
    let mut c03 = _mm512_setzero_ps();
    let mut c10 = _mm512_setzero_ps();
    let mut c11 = _mm512_setzero_ps();
    let mut c12 = _mm512_setzero_ps();
    let mut c13 = _mm512_setzero_ps();
    let mut c20 = _mm512_setzero_ps();
    let mut c21 = _mm512_setzero_ps();
    let mut c22 = _mm512_setzero_ps();
    let mut c23 = _mm512_setzero_ps();
    let mut c30 = _mm512_setzero_ps();
    let mut c31 = _mm512_setzero_ps();
    let mut c32 = _mm512_setzero_ps();
    let mut c33 = _mm512_setzero_ps();

    let mut k = 0usize;
    while k + 16 <= n {
        let vw0 = _mm512_loadu_ps(p0.add(k));
        let vw1 = _mm512_loadu_ps(p1.add(k));
        let vw2 = _mm512_loadu_ps(p2.add(k));
        let vw3 = _mm512_loadu_ps(p3.add(k));

        let vx0 = _mm512_loadu_ps(q0.add(k));
        c00 = _mm512_fmadd_ps(vw0, vx0, c00);
        c10 = _mm512_fmadd_ps(vw1, vx0, c10);
        c20 = _mm512_fmadd_ps(vw2, vx0, c20);
        c30 = _mm512_fmadd_ps(vw3, vx0, c30);

        let vx1 = _mm512_loadu_ps(q1.add(k));
        c01 = _mm512_fmadd_ps(vw0, vx1, c01);
        c11 = _mm512_fmadd_ps(vw1, vx1, c11);
        c21 = _mm512_fmadd_ps(vw2, vx1, c21);
        c31 = _mm512_fmadd_ps(vw3, vx1, c31);

        let vx2 = _mm512_loadu_ps(q2.add(k));
        c02 = _mm512_fmadd_ps(vw0, vx2, c02);
        c12 = _mm512_fmadd_ps(vw1, vx2, c12);
        c22 = _mm512_fmadd_ps(vw2, vx2, c22);
        c32 = _mm512_fmadd_ps(vw3, vx2, c32);

        let vx3 = _mm512_loadu_ps(q3.add(k));
        c03 = _mm512_fmadd_ps(vw0, vx3, c03);
        c13 = _mm512_fmadd_ps(vw1, vx3, c13);
        c23 = _mm512_fmadd_ps(vw2, vx3, c23);
        c33 = _mm512_fmadd_ps(vw3, vx3, c33);

        k += 16;
    }

    let mut s = [
        hsum_ps512(c00), hsum_ps512(c01), hsum_ps512(c02), hsum_ps512(c03),
        hsum_ps512(c10), hsum_ps512(c11), hsum_ps512(c12), hsum_ps512(c13),
        hsum_ps512(c20), hsum_ps512(c21), hsum_ps512(c22), hsum_ps512(c23),
        hsum_ps512(c30), hsum_ps512(c31), hsum_ps512(c32), hsum_ps512(c33),
    ];

    while k < n {
        let rw = [*p0.add(k), *p1.add(k), *p2.add(k), *p3.add(k)];
        let rx = [*q0.add(k), *q1.add(k), *q2.add(k), *q3.add(k)];
        for ri in 0..4 {
            for bi in 0..4 {
                s[ri * 4 + bi] += rw[ri] * rx[bi];
            }
        }
        k += 1;
    }

    *out16 = s;
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn gemm_4x1_f32_avx512f(
    w0: &[f32],
    w1: &[f32],
    w2: &[f32],
    w3: &[f32],
    x0: &[f32],
    out4: &mut [f32; 4],
) {
    use std::arch::x86_64::*;
    let n = w0.len();
    let (p0, p1, p2, p3) = (w0.as_ptr(), w1.as_ptr(), w2.as_ptr(), w3.as_ptr());
    let xp = x0.as_ptr();
    let mut c0 = _mm512_setzero_ps();
    let mut c1 = _mm512_setzero_ps();
    let mut c2 = _mm512_setzero_ps();
    let mut c3 = _mm512_setzero_ps();
    let mut k = 0usize;
    while k + 16 <= n {
        let vx = _mm512_loadu_ps(xp.add(k));
        c0 = _mm512_fmadd_ps(_mm512_loadu_ps(p0.add(k)), vx, c0);
        c1 = _mm512_fmadd_ps(_mm512_loadu_ps(p1.add(k)), vx, c1);
        c2 = _mm512_fmadd_ps(_mm512_loadu_ps(p2.add(k)), vx, c2);
        c3 = _mm512_fmadd_ps(_mm512_loadu_ps(p3.add(k)), vx, c3);
        k += 16;
    }
    let mut s0 = hsum_ps512(c0);
    let mut s1 = hsum_ps512(c1);
    let mut s2 = hsum_ps512(c2);
    let mut s3 = hsum_ps512(c3);
    while k < n {
        let rx = *xp.add(k);
        s0 += (*p0.add(k)) * rx;
        s1 += (*p1.add(k)) * rx;
        s2 += (*p2.add(k)) * rx;
        s3 += (*p3.add(k)) * rx;
        k += 1;
    }
    out4[0] = s0;
    out4[1] = s1;
    out4[2] = s2;
    out4[3] = s3;
}

#[inline]
fn dot_i8_f32_scalar(q: &[i8], x: &[f32]) -> f32 {
    let mut s0 = 0.0f32;
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    let mut s3 = 0.0f32;
    let mut i = 0usize;
    let n = q.len();
    while i + 4 <= n {
        s0 += (q[i] as f32) * x[i];
        s1 += (q[i + 1] as f32) * x[i + 1];
        s2 += (q[i + 2] as f32) * x[i + 2];
        s3 += (q[i + 3] as f32) * x[i + 3];
        i += 4;
    }
    let mut sum = s0 + s1 + s2 + s3;
    while i < n {
        sum += (q[i] as f32) * x[i];
        i += 1;
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_i8_f32_avx2_fma(q: &[i8], x: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = q.len();
    let qp = q.as_ptr();
    let xp = x.as_ptr();
    let mut acc0 = _mm256_setzero_ps();
    let mut acc1 = _mm256_setzero_ps();
    let mut acc2 = _mm256_setzero_ps();
    let mut acc3 = _mm256_setzero_ps();
    let mut i = 0usize;

    while i + 32 <= n {
        let q0 = _mm_loadl_epi64(qp.add(i) as *const __m128i);
        let q1 = _mm_loadl_epi64(qp.add(i + 8) as *const __m128i);
        let q2 = _mm_loadl_epi64(qp.add(i + 16) as *const __m128i);
        let q3 = _mm_loadl_epi64(qp.add(i + 24) as *const __m128i);

        let f0 = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q0));
        let f1 = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q1));
        let f2 = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q2));
        let f3 = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q3));

        let x0 = _mm256_loadu_ps(xp.add(i));
        let x1 = _mm256_loadu_ps(xp.add(i + 8));
        let x2 = _mm256_loadu_ps(xp.add(i + 16));
        let x3 = _mm256_loadu_ps(xp.add(i + 24));

        acc0 = _mm256_fmadd_ps(f0, x0, acc0);
        acc1 = _mm256_fmadd_ps(f1, x1, acc1);
        acc2 = _mm256_fmadd_ps(f2, x2, acc2);
        acc3 = _mm256_fmadd_ps(f3, x3, acc3);
        i += 32;
    }

    while i + 8 <= n {
        let q0 = _mm_loadl_epi64(qp.add(i) as *const __m128i);
        let f0 = _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q0));
        let x0 = _mm256_loadu_ps(xp.add(i));
        acc0 = _mm256_fmadd_ps(f0, x0, acc0);
        i += 8;
    }

    let sum8 = _mm256_add_ps(_mm256_add_ps(acc0, acc1), _mm256_add_ps(acc2, acc3));
    let mut sum = hsum_ps256(sum8);

    while i < n {
        sum += (*qp.add(i) as f32) * *xp.add(i);
        i += 1;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_rlnm(path: &Path, vocab: &[&str], d_model: usize, n_layers: usize, n_heads: usize, d_ff: usize) {
        let vocab_size = vocab.len();
        let max_seq_len = 32usize;
        let mut payload = Vec::new();
        let append_aligned = |pay: &mut Vec<u8>, bytes: &[u8]| -> u64 {
            let off = (RLNM_HEADER_BYTES + pay.len()) as u64;
            pay.extend_from_slice(bytes);
            while pay.len() % 8 != 0 {
                pay.push(0);
            }
            off
        };

        let mut str_buf = Vec::new();
        let mut meta_buf = Vec::new();
        for &tok in vocab {
            let tb = tok.as_bytes();
            meta_buf.extend_from_slice(&(str_buf.len() as u32).to_le_bytes());
            meta_buf.extend_from_slice(&(tb.len() as u32).to_le_bytes());
            str_buf.extend_from_slice(tb);
        }
        let off_vocab_meta = append_aligned(&mut payload, &meta_buf);
        let off_vocab_strings = append_aligned(&mut payload, &str_buf);

        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next_i8 = || -> i8 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 56) as i8).clamp(-100, 100)
        };
        let write_mat = |pay: &mut Vec<u8>, rows: usize, cols: usize, ni8: &mut dyn FnMut() -> i8| {
            let q: Vec<u8> = (0..rows * cols).map(|_| ni8() as u8).collect();
            let s: Vec<u8> = (0..rows).flat_map(|_| 0.015f32.to_le_bytes()).collect();
            let oq = append_aligned(pay, &q);
            let os = append_aligned(pay, &s);
            (oq, os)
        };

        let (off_tok_emb_q, off_tok_emb_s) = write_mat(&mut payload, vocab_size, d_model, &mut next_i8);
        let ones: Vec<u8> = (0..d_model).flat_map(|_| 1.0f32.to_le_bytes()).collect();
        let off_norm_final = append_aligned(&mut payload, &ones);
        let off_layers = (RLNM_HEADER_BYTES + payload.len()) as u64;
        for _ in 0..n_layers {
            append_aligned(&mut payload, &ones);
            append_aligned(&mut payload, &ones);
            write_mat(&mut payload, d_model, d_model, &mut next_i8);
            write_mat(&mut payload, d_model, d_model, &mut next_i8);
            write_mat(&mut payload, d_model, d_model, &mut next_i8);
            write_mat(&mut payload, d_model, d_model, &mut next_i8);
            write_mat(&mut payload, d_ff, d_model, &mut next_i8);
            write_mat(&mut payload, d_ff, d_model, &mut next_i8);
            write_mat(&mut payload, d_model, d_ff, &mut next_i8);
        }

        let total_bytes = (RLNM_HEADER_BYTES + payload.len()) as u64;
        let payload_hash = fnv1a_64(&payload);

        let mut hdr = Vec::with_capacity(RLNM_HEADER_BYTES);
        hdr.extend_from_slice(RLNM_MAGIC);
        hdr.extend_from_slice(&RLNM_VERSION.to_le_bytes());
        hdr.extend_from_slice(&7u32.to_le_bytes());
        hdr.extend_from_slice(&(vocab_size as u32).to_le_bytes());
        hdr.extend_from_slice(&(d_model as u32).to_le_bytes());
        hdr.extend_from_slice(&(n_layers as u32).to_le_bytes());
        hdr.extend_from_slice(&(n_heads as u32).to_le_bytes());
        hdr.extend_from_slice(&(d_ff as u32).to_le_bytes());
        hdr.extend_from_slice(&(max_seq_len as u32).to_le_bytes());
        hdr.extend_from_slice(&10000.0f64.to_le_bytes());
        hdr.extend_from_slice(&0xCAFE_BABEu64.to_le_bytes());
        hdr.extend_from_slice(&1e-5f32.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes());
        for v in [
            off_vocab_meta,
            off_vocab_strings,
            off_tok_emb_q,
            off_tok_emb_s,
            off_norm_final,
            off_layers,
            total_bytes,
            payload_hash,
        ] {
            hdr.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(hdr.len(), RLNM_HEADER_BYTES);
        let mut full = hdr;
        full.extend_from_slice(&payload);
        std::fs::write(path, full).unwrap();
    }

    #[test]
    fn test_rlnm_synthetic_trie_parity_simd_and_checksum() {
        let dir = std::env::temp_dir().join(format!("ruhi_rlnm_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin_path = dir.join("syn.bin");

        let vocab = ["<pad>", "<bos>", "<eos>", "<unk>", "我", "們", "在", "再", "補", "上", "三", "句", "這", "裡"];
        build_synthetic_rlnm(&bin_path, &vocab, 40, 2, 4, 64);

        let mut model = RlnmModel::open(&bin_path).expect("synthetic RLNM should load");
        assert_eq!(model.cfg.vocab_size, vocab.len());
        assert_eq!(model.cfg.d_model, 40);

        // 驗證 AVX2/FMA 與 scalar 點積完全一致（含非 32 倍數長度 40）
        let q: Vec<i8> = (0..40).map(|i| ((i * 7 % 21) as i8) - 10).collect();
        let x: Vec<f32> = (0..40).map(|i| (i as f32) * 0.125 - 2.0).collect();
        let ref_dot = dot_i8_f32_scalar(&q, &x);
        let act_dot = model.dot_i8_f32(&q, &x);
        assert!((ref_dot - act_dot).abs() < 1e-4, "dot mismatch: {} vs {}", ref_dot, act_dot);

        // 驗證樹狀批次化 Trie 與單句參考計分嚴格等價（含偵測 SIMD、強制 AVX2+FMA、強制純純量三條路徑及有無執行緒池）
        let cands = ["再補上三句", "在補上三句", "再補上這裡", "我們在這裡"];
        let mut cache = model.alloc_kv_cache();
        let pool = RlnmThreadPool::new(4);
        let single_refs: Vec<f32> = cands
            .iter()
            .map(|&c| model.score_candidate("我們", c, None))
            .collect();

        for (label, use_512, use_avx2) in [
            ("native-simd", model.has_avx512f, model.has_avx2_fma),
            ("forced-avx2", false, model.has_avx2_fma),
            ("forced-scalar", false, false),
        ] {
            model.has_avx512f = use_512;
            model.has_avx2_fma = use_avx2;
            let trie_scores_st = model.score_candidates_trie("我們", &cands, &mut cache, None);
            let trie_scores_mt = model.score_candidates_trie("我們", &cands, &mut cache, Some(&pool));
            for (idx, &c) in cands.iter().enumerate() {
                assert!(
                    (trie_scores_st[idx] - single_refs[idx]).abs() < 1e-3,
                    "[{}] Trie ST vs single mismatch on `{}`: {} vs {}",
                    label,
                    c,
                    trie_scores_st[idx],
                    single_refs[idx]
                );
                assert!(
                    (trie_scores_mt[idx] - single_refs[idx]).abs() < 1e-3,
                    "[{}] Trie MT vs single mismatch on `{}`: {} vs {}",
                    label,
                    c,
                    trie_scores_mt[idx],
                    single_refs[idx]
                );
            }
        }

        // 竄改 1 位元組必須被 FNV-1a 攔截
        let mut bad = std::fs::read(&bin_path).unwrap();
        let last = bad.len() - 19;
        bad[last] ^= 0x5A;
        let bad_path = dir.join("bad.bin");
        std::fs::write(&bad_path, bad).unwrap();
        assert!(RlnmModel::open(&bad_path).is_err());

        drop(model);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rlnm_real_model_parity_if_present() {
        let model_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ruhi_lexicon")
            .join("data")
            .join("rlnm_zh_tw.bin");
        if !model_path.is_file() {
            return;
        }
        let model = RlnmModel::open(&model_path).expect("real rlnm_zh_tw.bin should load");
        let cands = [
            "此刻視力模糊難以辨識滑鼠底下大概只有",
            "此刻視力模糊難以便是滑鼠底下大概只有",
        ];
        let mut cache = model.alloc_kv_cache();
        let pool = RlnmThreadPool::new(4);
        let trie_sc = model.score_candidates_trie("", &cands, &mut cache, Some(&pool));
        let s0 = model.score_candidate("", cands[0], None);
        let s1 = model.score_candidate("", cands[1], None);
        assert!((trie_sc[0] - s0).abs() < 1e-3);
        assert!((trie_sc[1] - s1).abs() < 1e-3);
        let diff = trie_sc[0] - trie_sc[1];
        if model.cfg.corpus_fingerprint == 0x6f6ff534ce904c9b {
            // C5 模型 Python/NumPy 實測值 +11.172446（allow ±0.02 float accumulation tolerance）
            assert!(
                (diff - 11.172446).abs() < 0.02,
                "Rust vs NumPy parity drift: diff={}",
                diff
            );
        } else {
            assert!(
                diff.is_finite() && diff > 5.0,
                "Unexpected real model ranking margin: diff={}",
                diff
            );
        }
    }
}
