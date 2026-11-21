// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/ngram_lm.rs — 台灣正體統計 N-gram 語言模型 (`RLNG0001` 唯讀記憶體映射引擎與編譯器)

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use crate::mmap_io::{read_f32_le, read_u16_le, read_u32_le, read_u64_le, ReadOnlyMmap};

/// 檔案魔數與格式常數
pub const RLNG_MAGIC: &[u8; 8] = b"RLNG0001";
pub const RLNG_VERSION: u32 = 1;
pub const RLNG_HEADER_LEN: usize = 144;
pub const BOS_WORD: &str = "<s>";
pub const BOS_CHAR: char = '\u{0002}';

const WORD_VOCAB_REC_BYTES: usize = 12;
const CHAR_VOCAB_REC_BYTES: usize = 8;
const CHAR_BI_REC_BYTES: usize = 6;
const CHAR_TRI_REC_BYTES: usize = 4;

static GLOBAL_NGRAM_LM: OnceLock<RwLock<Option<Arc<NgramLanguageModel>>>> = OnceLock::new();
static AUTO_PROBED: AtomicBool = AtomicBool::new(false);

fn global_slot() -> &'static RwLock<Option<Arc<NgramLanguageModel>>> {
    GLOBAL_NGRAM_LM.get_or_init(|| RwLock::new(None))
}

/// 編譯 `RLNG0001` 二進位檔之統計摘要
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NgramCompileStats {
    pub vocab_count: usize,
    pub char_vocab_count: usize,
    pub word_bi_count: usize,
    pub char_bi_count: usize,
    pub char_tri_count: usize,
    pub skipped_missing_vocab: usize,
    pub duplicate_entries_overwritten: usize,
    pub corpus_fingerprint: u64,
    pub bytes_written: usize,
}

/// `RLNG0001` 唯讀記憶體映射統計 N-gram 語言模型檢視器（零堆積熱路徑查詢）
pub struct NgramLanguageModel {
    raw: ReadOnlyMmap,
    pub vocab_count: usize,
    pub char_vocab_count: usize,
    pub word_bi_count: usize,
    pub char_bi_count: usize,
    pub char_tri_count: usize,
    pub q_min: f32,
    pub q_max: f32,
    pub unk_uni_q16: u16,
    pub unk_char_uni_q16: u16,
    pub corpus_fingerprint: u64,
    strings_blob_len: usize,
    off_strings_blob: usize,
    off_word_vocab: usize,
    off_wb_row_off: usize,
    off_wb_cols: usize,
    off_wb_logp: usize,
    off_char_vocab: usize,
    off_cb_row_off: usize,
    off_cb_data: usize,
    off_ct_row_off: usize,
    off_ct_data: usize,
}

impl std::fmt::Debug for NgramLanguageModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NgramLanguageModel")
            .field("vocab_count", &self.vocab_count)
            .field("char_vocab_count", &self.char_vocab_count)
            .field("word_bi_count", &self.word_bi_count)
            .field("char_bi_count", &self.char_bi_count)
            .field("char_tri_count", &self.char_tri_count)
            .field("q_min", &self.q_min)
            .field("q_max", &self.q_max)
            .field("corpus_fingerprint", &format_args!("{:#018x}", self.corpus_fingerprint))
            .field("mapped_bytes", &self.raw.len())
            .finish()
    }
}

#[inline]
pub fn quantize_logp(val: f64, q_min: f32, q_max: f32) -> u16 {
    let min_f = q_min as f64;
    let max_f = q_max as f64;
    if !val.is_finite() || max_f <= min_f {
        return 0;
    }
    let norm = ((val - min_f) / (max_f - min_f)).clamp(0.0, 1.0);
    (norm * 65535.0).round() as u16
}

#[inline]
fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn check_section_bounds(buf_len: usize, off: usize, count: usize, stride: usize, label: &'static str) -> io::Result<()> {
    if off < RLNG_HEADER_LEN || off % 8 != 0 {
        return Err(invalid_data(label));
    }
    let byte_len = count.checked_mul(stride).ok_or_else(|| invalid_data(label))?;
    let end = off.checked_add(byte_len).ok_or_else(|| invalid_data(label))?;
    if end > buf_len {
        return Err(invalid_data(label));
    }
    Ok(())
}

fn validate_row_offsets(buf: &[u8], off: usize, num_rows: usize, expected_total: usize, label: &'static str) -> io::Result<()> {
    let first = read_u32_le(buf, off).ok_or_else(|| invalid_data(label))? as usize;
    if first != 0 {
        return Err(invalid_data(label));
    }
    let mut prev = 0usize;
    for r in 1..=num_rows {
        let cur = read_u32_le(buf, off + r * 4).ok_or_else(|| invalid_data(label))? as usize;
        if cur < prev {
            return Err(invalid_data(label));
        }
        prev = cur;
    }
    if prev != expected_total {
        return Err(invalid_data(label));
    }
    Ok(())
}

impl NgramLanguageModel {
    /// 開啟並嚴格校驗 `RLNG0001` 唯讀記憶體映射語言模型
    pub fn open(path: &Path) -> io::Result<Self> {
        let raw = ReadOnlyMmap::open(path)?;
        let buf = raw.as_slice();
        if buf.len() < RLNG_HEADER_LEN || &buf[0..8] != RLNG_MAGIC {
            return Err(invalid_data("invalid RLNG0001 header or magic"));
        }
        let version = read_u32_le(buf, 8).ok_or_else(|| invalid_data("header truncated"))?;
        if version != RLNG_VERSION {
            return Err(invalid_data("unsupported RLNG0001 version"));
        }
        let _flags = read_u32_le(buf, 12).ok_or_else(|| invalid_data("header truncated"))?;
        let vocab_count = read_u32_le(buf, 16).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let char_vocab_count = read_u32_le(buf, 20).ok_or_else(|| invalid_data("header truncated"))? as usize;
        if char_vocab_count > 65535 {
            return Err(invalid_data("char_vocab_count exceeds u16 index capacity"));
        }
        let word_bi_count = read_u32_le(buf, 24).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let char_bi_count = read_u32_le(buf, 28).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let char_tri_count = read_u32_le(buf, 32).ok_or_else(|| invalid_data("header truncated"))? as usize;
        let q_min = read_f32_le(buf, 36).ok_or_else(|| invalid_data("header truncated"))?;
        let q_max = read_f32_le(buf, 40).ok_or_else(|| invalid_data("header truncated"))?;
        if !q_min.is_finite() || !q_max.is_finite() || q_max <= q_min {
            return Err(invalid_data("invalid quantization range in RLNG0001"));
        }
        let unk_uni_q16 = read_u16_le(buf, 44).ok_or_else(|| invalid_data("header truncated"))?;
        let unk_char_uni_q16 = read_u16_le(buf, 46).ok_or_else(|| invalid_data("header truncated"))?;
        let corpus_fingerprint = read_u64_le(buf, 48).ok_or_else(|| invalid_data("header truncated"))?;
        let strings_blob_len_u64 = read_u64_le(buf, 56).ok_or_else(|| invalid_data("header truncated"))?;
        let strings_blob_len = usize::try_from(strings_blob_len_u64)
            .map_err(|_| invalid_data("strings_blob_len overflow"))?;

        let read_off = |pos: usize| -> io::Result<usize> {
            let v = read_u64_le(buf, pos).ok_or_else(|| invalid_data("directory truncated"))?;
            usize::try_from(v).map_err(|_| invalid_data("section offset overflow"))
        };
        let off_strings_blob = read_off(64)?;
        let off_word_vocab = read_off(72)?;
        let off_wb_row_off = read_off(80)?;
        let off_wb_cols = read_off(88)?;
        let off_wb_logp = read_off(96)?;
        let off_char_vocab = read_off(104)?;
        let off_cb_row_off = read_off(112)?;
        let off_cb_data = read_off(120)?;
        let off_ct_row_off = read_off(128)?;
        let off_ct_data = read_off(136)?;

        let blen = buf.len();
        check_section_bounds(blen, off_strings_blob, strings_blob_len, 1, "RLNG0001 strings_blob out of bounds")?;
        check_section_bounds(blen, off_word_vocab, vocab_count, WORD_VOCAB_REC_BYTES, "RLNG0001 word_vocab out of bounds")?;
        check_section_bounds(blen, off_wb_row_off, vocab_count + 1, 4, "RLNG0001 wb_row_off out of bounds")?;
        check_section_bounds(blen, off_wb_cols, word_bi_count, 4, "RLNG0001 wb_cols out of bounds")?;
        check_section_bounds(blen, off_wb_logp, word_bi_count, 2, "RLNG0001 wb_logp out of bounds")?;
        check_section_bounds(blen, off_char_vocab, char_vocab_count, CHAR_VOCAB_REC_BYTES, "RLNG0001 char_vocab out of bounds")?;
        check_section_bounds(blen, off_cb_row_off, char_vocab_count + 1, 4, "RLNG0001 cb_row_off out of bounds")?;
        check_section_bounds(blen, off_cb_data, char_bi_count, CHAR_BI_REC_BYTES, "RLNG0001 cb_data out of bounds")?;
        check_section_bounds(blen, off_ct_row_off, char_bi_count + 1, 4, "RLNG0001 ct_row_off out of bounds")?;
        check_section_bounds(blen, off_ct_data, char_tri_count, CHAR_TRI_REC_BYTES, "RLNG0001 ct_data out of bounds")?;

        validate_row_offsets(buf, off_wb_row_off, vocab_count, word_bi_count, "RLNG0001 invalid wb_row_off")?;
        validate_row_offsets(buf, off_cb_row_off, char_vocab_count, char_bi_count, "RLNG0001 invalid cb_row_off")?;
        validate_row_offsets(buf, off_ct_row_off, char_bi_count, char_tri_count, "RLNG0001 invalid ct_row_off")?;

        // 校驗所有 word_vocab 字串切片位於 strings_blob 範圍內，保證熱路徑查詢無界外風險
        for i in 0..vocab_count {
            let rec = off_word_vocab + i * WORD_VOCAB_REC_BYTES;
            let str_off = read_u32_le(buf, rec).ok_or_else(|| invalid_data("word_vocab truncated"))? as usize;
            let str_len = read_u16_le(buf, rec + 4).ok_or_else(|| invalid_data("word_vocab truncated"))? as usize;
            let end = str_off.checked_add(str_len).ok_or_else(|| invalid_data("word string overflow"))?;
            if end > strings_blob_len {
                return Err(invalid_data("RLNG0001 word string out of strings_blob bounds"));
            }
        }

        Ok(Self {
            raw,
            vocab_count,
            char_vocab_count,
            word_bi_count,
            char_bi_count,
            char_tri_count,
            q_min,
            q_max,
            unk_uni_q16,
            unk_char_uni_q16,
            corpus_fingerprint,
            strings_blob_len,
            off_strings_blob,
            off_word_vocab,
            off_wb_row_off,
            off_wb_cols,
            off_wb_logp,
            off_char_vocab,
            off_cb_row_off,
            off_cb_data,
            off_ct_row_off,
            off_ct_data,
        })
    }

    /// 明確指定資料目錄載入 `ngram_zh_tw.bin` 至進程單例
    pub fn init_from_data_dir(data_dir: &Path) -> bool {
        let bin_path = data_dir.join("ngram_zh_tw.bin");
        if !bin_path.is_file() {
            return false;
        }
        match Self::open(&bin_path) {
            Ok(lm) => {
                let mut guard = global_slot().write().unwrap_or_else(|e| e.into_inner());
                *guard = Some(Arc::new(lm));
                AUTO_PROBED.store(true, Ordering::Release);
                true
            }
            Err(_) => false,
        }
    }

    /// 取得進程級單例；首次呼叫若尚未顯式初始化則安全搜尋標準資料路徑
    pub fn global() -> Option<Arc<NgramLanguageModel>> {
        {
            let guard = global_slot().read().unwrap_or_else(|e| e.into_inner());
            if let Some(ref arc) = *guard {
                return Some(arc.clone());
            }
        }
        if !AUTO_PROBED.swap(true, Ordering::AcqRel) {
            if let Some(path) = Self::resolve_default_bin_path() {
                if let Ok(lm) = Self::open(&path) {
                    let arc = Arc::new(lm);
                    let mut guard = global_slot().write().unwrap_or_else(|e| e.into_inner());
                    if guard.is_none() {
                        *guard = Some(arc.clone());
                    }
                    return guard.clone();
                }
            }
        }
        let guard = global_slot().read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    fn resolve_default_bin_path() -> Option<PathBuf> {
        let mut search_dirs: Vec<PathBuf> = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            search_dirs.push(PathBuf::from(&appdata).join("RuhI").join("data"));
            search_dirs.push(PathBuf::from(&appdata).join("Ruhi").join("data"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                search_dirs.push(dir.join("data"));
                search_dirs.push(dir.join("../data"));
                #[cfg(any(debug_assertions, test))]
                {
                    search_dirs.push(dir.join("../ruhi_lexicon/data"));
                    search_dirs.push(dir.join("../../ruhi_lexicon/data"));
                    search_dirs.push(dir.join("../../../ruhi_lexicon/data"));
                }
            }
        }
        #[cfg(any(debug_assertions, test))]
        {
            search_dirs.push(PathBuf::from("data"));
            search_dirs.push(PathBuf::from("ruhi_lexicon/data"));
            search_dirs.push(PathBuf::from("../ruhi_lexicon/data"));
            search_dirs.push(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ruhi_lexicon/data"),
            );
        }
        for d in search_dirs {
            let cand = d.join("ngram_zh_tw.bin");
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    }

    #[inline]
    pub fn dequantize(&self, q: u16) -> f64 {
        let min_f = self.q_min as f64;
        let max_f = self.q_max as f64;
        min_f + (q as f64 / 65535.0) * (max_f - min_f)
    }

    #[inline]
    fn word_bytes_at<'a>(&self, buf: &'a [u8], idx: usize) -> &'a [u8] {
        let rec = self.off_word_vocab + idx * WORD_VOCAB_REC_BYTES;
        let str_off = u32::from_le_bytes([buf[rec], buf[rec + 1], buf[rec + 2], buf[rec + 3]]) as usize;
        let str_len = u16::from_le_bytes([buf[rec + 4], buf[rec + 5]]) as usize;
        let start = self.off_strings_blob + str_off;
        let end = (start + str_len).min(self.off_strings_blob + self.strings_blob_len);
        &buf[start..end]
    }

    #[inline]
    fn word_uni_bow_q16(&self, buf: &[u8], idx: usize) -> (u16, u16) {
        let rec = self.off_word_vocab + idx * WORD_VOCAB_REC_BYTES;
        let uni_q = u16::from_le_bytes([buf[rec + 6], buf[rec + 7]]);
        let bow_q = u16::from_le_bytes([buf[rec + 8], buf[rec + 9]]);
        (uni_q, bow_q)
    }

    /// 查詢詞彙於詞表中的索引（二分搜尋 UTF-8 位元組，零配置）
    #[inline]
    pub fn lookup_word_id(&self, word: &str) -> Option<u32> {
        let target = word.as_bytes();
        let buf = self.raw.as_slice();
        let mut lo = 0usize;
        let mut hi = self.vocab_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let mid_bytes = self.word_bytes_at(buf, mid);
            match mid_bytes.cmp(target) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid as u32),
            }
        }
        None
    }

    #[inline]
    fn char_record_at(&self, buf: &[u8], idx: usize) -> (u32, u16, u16) {
        let rec = self.off_char_vocab + idx * CHAR_VOCAB_REC_BYTES;
        let cp = u32::from_le_bytes([buf[rec], buf[rec + 1], buf[rec + 2], buf[rec + 3]]);
        let uni_q = u16::from_le_bytes([buf[rec + 4], buf[rec + 5]]);
        let bow_q = u16::from_le_bytes([buf[rec + 6], buf[rec + 7]]);
        (cp, uni_q, bow_q)
    }

    /// 查詢 Unicode 字元於字元表中的索引（二分搜尋 codepoint，零配置）
    #[inline]
    pub fn lookup_char_id(&self, ch: char) -> Option<u16> {
        let target = ch as u32;
        let buf = self.raw.as_slice();
        let mut lo = 0usize;
        let mut hi = self.char_vocab_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let rec = self.off_char_vocab + mid * CHAR_VOCAB_REC_BYTES;
            let cp = u32::from_le_bytes([buf[rec], buf[rec + 1], buf[rec + 2], buf[rec + 3]]);
            match cp.cmp(&target) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid as u16),
            }
        }
        None
    }

    #[inline]
    fn wb_row_range(&self, buf: &[u8], prev_id: u32) -> (usize, usize) {
        let p = prev_id as usize;
        let o0 = self.off_wb_row_off + p * 4;
        let start = u32::from_le_bytes([buf[o0], buf[o0 + 1], buf[o0 + 2], buf[o0 + 3]]) as usize;
        let end = u32::from_le_bytes([buf[o0 + 4], buf[o0 + 5], buf[o0 + 6], buf[o0 + 7]]) as usize;
        (start, end)
    }

    #[inline]
    fn lookup_word_bigram_idx(&self, buf: &[u8], prev_id: u32, curr_id: u32) -> Option<usize> {
        let (mut lo, mut hi) = self.wb_row_range(buf, prev_id);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let c_off = self.off_wb_cols + mid * 4;
            let col = u32::from_le_bytes([buf[c_off], buf[c_off + 1], buf[c_off + 2], buf[c_off + 3]]);
            match col.cmp(&curr_id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    #[inline]
    fn cb_row_range(&self, buf: &[u8], c1_id: u16) -> (usize, usize) {
        let p = c1_id as usize;
        let o0 = self.off_cb_row_off + p * 4;
        let start = u32::from_le_bytes([buf[o0], buf[o0 + 1], buf[o0 + 2], buf[o0 + 3]]) as usize;
        let end = u32::from_le_bytes([buf[o0 + 4], buf[o0 + 5], buf[o0 + 6], buf[o0 + 7]]) as usize;
        (start, end)
    }

    #[inline]
    fn lookup_char_bigram_rec(&self, buf: &[u8], c1_id: u16, c2_id: u16) -> Option<(usize, u16, u16)> {
        let (mut lo, mut hi) = self.cb_row_range(buf, c1_id);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let rec = self.off_cb_data + mid * CHAR_BI_REC_BYTES;
            let col = u16::from_le_bytes([buf[rec], buf[rec + 1]]);
            match col.cmp(&c2_id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let logp_q = u16::from_le_bytes([buf[rec + 2], buf[rec + 3]]);
                    let bow_q = u16::from_le_bytes([buf[rec + 4], buf[rec + 5]]);
                    return Some((mid, logp_q, bow_q));
                }
            }
        }
        None
    }

    #[inline]
    fn ct_row_range(&self, buf: &[u8], cb_idx: usize) -> (usize, usize) {
        let o0 = self.off_ct_row_off + cb_idx * 4;
        let start = u32::from_le_bytes([buf[o0], buf[o0 + 1], buf[o0 + 2], buf[o0 + 3]]) as usize;
        let end = u32::from_le_bytes([buf[o0 + 4], buf[o0 + 5], buf[o0 + 6], buf[o0 + 7]]) as usize;
        (start, end)
    }

    #[inline]
    fn lookup_char_trigram_q16(&self, buf: &[u8], cb_idx: usize, c3_id: u16) -> Option<u16> {
        let (mut lo, mut hi) = self.ct_row_range(buf, cb_idx);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let rec = self.off_ct_data + mid * CHAR_TRI_REC_BYTES;
            let col = u16::from_le_bytes([buf[rec], buf[rec + 1]]);
            match col.cmp(&c3_id) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    return Some(u16::from_le_bytes([buf[rec + 2], buf[rec + 3]]));
                }
            }
        }
        None
    }

    /// 查詢詞彙一元對數機率 $\log_{10} P(w)$（未登錄詞回傳 `unk_uni`）
    #[inline]
    pub fn word_unigram_logp(&self, word: &str) -> f64 {
        let buf = self.raw.as_slice();
        if let Some(id) = self.lookup_word_id(word) {
            let (uni_q, _) = self.word_uni_bow_q16(buf, id as usize);
            self.dequantize(uni_q)
        } else {
            self.dequantize(self.unk_uni_q16)
        }
    }

    /// 計算詞級二元回退對數機率 $\log_{10} P_{\text{KN}}(\text{curr} \mid \text{prev})$
    #[inline]
    pub fn score_word_bigram(&self, prev: &str, curr: &str) -> f64 {
        let prev_eff = if prev.is_empty() { BOS_WORD } else { prev };
        let buf = self.raw.as_slice();
        let curr_id_opt = self.lookup_word_id(curr);
        let curr_uni = match curr_id_opt {
            Some(cid) => {
                let (uni_q, _) = self.word_uni_bow_q16(buf, cid as usize);
                self.dequantize(uni_q)
            }
            None => self.dequantize(self.unk_uni_q16),
        };

        if let Some(pid) = self.lookup_word_id(prev_eff) {
            if let Some(cid) = curr_id_opt {
                if let Some(wb_idx) = self.lookup_word_bigram_idx(buf, pid, cid) {
                    let p_off = self.off_wb_logp + wb_idx * 2;
                    let logp_q = u16::from_le_bytes([buf[p_off], buf[p_off + 1]]);
                    return self.dequantize(logp_q);
                }
            }
            let (_, bow_q) = self.word_uni_bow_q16(buf, pid as usize);
            self.dequantize(bow_q) + curr_uni
        } else {
            curr_uni
        }
    }

    #[inline]
    fn char_unigram_logp_by_id(&self, buf: &[u8], id_opt: Option<u16>) -> f64 {
        match id_opt {
            Some(id) => {
                let (_, uni_q, _) = self.char_record_at(buf, id as usize);
                self.dequantize(uni_q)
            }
            None => self.dequantize(self.unk_char_uni_q16),
        }
    }

    /// 計算字級二元回退對數機率 $\log_{10} P_{\text{KN}}(c_2 \mid c_1)$
    #[inline]
    pub fn char_bigram_logp(&self, c1: char, c2: char) -> f64 {
        let buf = self.raw.as_slice();
        let c1_id_opt = self.lookup_char_id(c1);
        let c2_id_opt = self.lookup_char_id(c2);
        let c2_uni = self.char_unigram_logp_by_id(buf, c2_id_opt);
        if let Some(c1_id) = c1_id_opt {
            if let Some(c2_id) = c2_id_opt {
                if let Some((_, logp_q, _)) = self.lookup_char_bigram_rec(buf, c1_id, c2_id) {
                    return self.dequantize(logp_q);
                }
            }
            let (_, _, bow_q) = self.char_record_at(buf, c1_id as usize);
            self.dequantize(bow_q) + c2_uni
        } else {
            c2_uni
        }
    }

    /// 計算字級三元回退對數機率 $\log_{10} P_{\text{KN}}(c_3 \mid c_1, c_2)$
    #[inline]
    pub fn char_trigram_logp(&self, c1: char, c2: char, c3: char) -> f64 {
        let buf = self.raw.as_slice();
        let c1_id_opt = self.lookup_char_id(c1);
        let c2_id_opt = self.lookup_char_id(c2);
        let c3_id_opt = self.lookup_char_id(c3);

        let bi_23 = || -> f64 {
            let c3_uni = self.char_unigram_logp_by_id(buf, c3_id_opt);
            if let Some(c2_id) = c2_id_opt {
                if let Some(c3_id) = c3_id_opt {
                    if let Some((_, logp_q, _)) = self.lookup_char_bigram_rec(buf, c2_id, c3_id) {
                        return self.dequantize(logp_q);
                    }
                }
                let (_, _, bow_q) = self.char_record_at(buf, c2_id as usize);
                self.dequantize(bow_q) + c3_uni
            } else {
                c3_uni
            }
        };

        if let (Some(c1_id), Some(c2_id)) = (c1_id_opt, c2_id_opt) {
            if let Some((cb_idx, _, cb_bow_q)) = self.lookup_char_bigram_rec(buf, c1_id, c2_id) {
                if let Some(c3_id) = c3_id_opt {
                    if let Some(logp_q) = self.lookup_char_trigram_q16(buf, cb_idx, c3_id) {
                        return self.dequantize(logp_q);
                    }
                }
                return self.dequantize(cb_bow_q) + bi_23();
            }
        }
        bi_23()
    }

    /// 計算跨越詞界之字級 Bigram 與 Trigram 對數機率總和 `(char_bi_sum, char_tri_sum)`。
    /// 對任何整句切分路徑加總具備嚴格切分長度不變性（Segmentation Length-Invariance）。
    #[inline]
    pub fn score_char_boundary(
        &self,
        prev2_word: Option<&str>,
        prev_word: &str,
        curr_word: &str,
    ) -> (f64, f64) {
        let (mut ctx_m2, mut ctx_m1) = prev_last_two_chars(prev2_word, prev_word);
        let mut bi_sum = 0.0f64;
        let mut tri_sum = 0.0f64;
        for ch in curr_word.chars() {
            bi_sum += self.char_bigram_logp(ctx_m1, ch);
            tri_sum += self.char_trigram_logp(ctx_m2, ctx_m1, ch);
            ctx_m2 = ctx_m1;
            ctx_m1 = ch;
        }
        (bi_sum, tri_sum)
    }
}

#[inline]
fn prev_last_two_chars(prev2_word: Option<&str>, prev_word: &str) -> (char, char) {
    if prev_word.is_empty() || prev_word == BOS_WORD {
        return (BOS_CHAR, BOS_CHAR);
    }
    let mut rev = prev_word.chars().rev();
    let c_m1 = rev.next().unwrap_or(BOS_CHAR);
    let c_m2 = match rev.next() {
        Some(ch) => ch,
        None => prev2_word
            .and_then(|p2| {
                if p2.is_empty() || p2 == BOS_WORD {
                    None
                } else {
                    p2.chars().next_back()
                }
            })
            .unwrap_or(BOS_CHAR),
    };
    (c_m2, c_m1)
}

#[inline]
fn parse_char_token(tok: &str) -> Option<char> {
    if tok == BOS_WORD {
        return Some(BOS_CHAR);
    }
    let mut it = tok.chars();
    let ch = it.next()?;
    if it.next().is_some() {
        return None;
    }
    Some(ch)
}

#[inline]
fn align8(buf: &mut Vec<u8>) {
    let rem = buf.len() % 8;
    if rem != 0 {
        buf.resize(buf.len() + (8 - rem), 0);
    }
}

fn fnv1a_file_fingerprint(path: &Path) -> io::Result<u64> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = io::Read::read(&mut reader, &mut chunk)?;
        if n == 0 {
            break;
        }
        for &b in &chunk[..n] {
            h ^= b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    }
    Ok(h)
}

/// 將標籤化之 ARPA TSV 中介檔 (`WU`/`WB`/`CU`/`CB`/`CT`) 編譯為 `RLNG0001` 唯讀記憶體映射檔
pub fn compile_arpa_tsv_to_bin(tsv_path: &Path, bin_path: &Path) -> io::Result<NgramCompileStats> {
    let fingerprint = fnv1a_file_fingerprint(tsv_path)?;
    let file = File::open(tsv_path)?;
    let reader = BufReader::with_capacity(256 * 1024, file);

    let mut word_unis: HashMap<String, (f64, f64)> = HashMap::new();
    let mut word_bis: HashMap<(String, String), f64> = HashMap::new();
    let mut char_unis: HashMap<char, (f64, f64)> = HashMap::new();
    let mut char_bis: HashMap<(char, char), (f64, f64)> = HashMap::new();
    let mut char_tris: HashMap<(char, char, char), f64> = HashMap::new();

    let mut explicit_unk_word_uni: Option<f64> = None;
    let mut explicit_unk_char_uni: Option<f64> = None;
    let mut duplicate_entries_overwritten = 0usize;
    let mut min_val = -12.0f64;
    let mut max_val = 2.0f64;

    let mut track_range = |v: f64| {
        if v.is_finite() {
            if v < min_val {
                min_val = v;
            }
            if v > max_val {
                max_val = v;
            }
        }
    };

    for line_res in reader.lines() {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.is_empty() {
            continue;
        }
        match cols[0] {
            "WU" if cols.len() >= 4 => {
                let w = cols[1];
                if let (Ok(uni), Ok(bow)) = (cols[2].parse::<f64>(), cols[3].parse::<f64>()) {
                    track_range(uni);
                    track_range(bow);
                    if w == "<unk>" {
                        explicit_unk_word_uni = Some(uni);
                    }
                    if word_unis.insert(w.to_string(), (uni, bow)).is_some() {
                        duplicate_entries_overwritten += 1;
                    }
                }
            }
            "WB" if cols.len() >= 4 => {
                let p = cols[1];
                let c = cols[2];
                if let Ok(lp) = cols[3].parse::<f64>() {
                    track_range(lp);
                    if word_bis.insert((p.to_string(), c.to_string()), lp).is_some() {
                        duplicate_entries_overwritten += 1;
                    }
                }
            }
            "CU" if cols.len() >= 4 => {
                if let (Ok(uni), Ok(bow)) = (cols[2].parse::<f64>(), cols[3].parse::<f64>()) {
                    track_range(uni);
                    track_range(bow);
                    if cols[1] == "<unk>" {
                        explicit_unk_char_uni = Some(uni);
                    } else if let Some(ch) = parse_char_token(cols[1]) {
                        if char_unis.insert(ch, (uni, bow)).is_some() {
                            duplicate_entries_overwritten += 1;
                        }
                    }
                }
            }
            "CB" if cols.len() >= 5 => {
                if let (Some(c1), Some(c2), Ok(lp), Ok(bow)) = (
                    parse_char_token(cols[1]),
                    parse_char_token(cols[2]),
                    cols[3].parse::<f64>(),
                    cols[4].parse::<f64>(),
                ) {
                    track_range(lp);
                    track_range(bow);
                    if char_bis.insert((c1, c2), (lp, bow)).is_some() {
                        duplicate_entries_overwritten += 1;
                    }
                }
            }
            "CT" if cols.len() >= 5 => {
                if let (Some(c1), Some(c2), Some(c3), Ok(lp)) = (
                    parse_char_token(cols[1]),
                    parse_char_token(cols[2]),
                    parse_char_token(cols[3]),
                    cols[4].parse::<f64>(),
                ) {
                    track_range(lp);
                    if char_tris.insert((c1, c2, c3), lp).is_some() {
                        duplicate_entries_overwritten += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if char_unis.len() > 65535 {
        return Err(invalid_data("char_vocab exceeds 65535 limit"));
    }

    let q_min = min_val.max(-20.0) as f32;
    let q_max = max_val.min(6.0) as f32;

    // 1. 排序詞彙表 (按 UTF-8 位元組序)
    let mut sorted_words: Vec<(String, f64, f64)> = word_unis
        .into_iter()
        .map(|(w, (u, b))| (w, u, b))
        .collect();
    sorted_words.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut word_to_id: HashMap<String, u32> = HashMap::with_capacity(sorted_words.len());
    let mut strings_blob: Vec<u8> = Vec::new();
    let mut word_vocab_buf: Vec<u8> = Vec::with_capacity(sorted_words.len() * WORD_VOCAB_REC_BYTES);

    let mut min_word_uni = -6.0f64;
    for (idx, (w, uni, bow)) in sorted_words.iter().enumerate() {
        if *uni < min_word_uni {
            min_word_uni = *uni;
        }
        word_to_id.insert(w.clone(), idx as u32);
        let bytes = w.as_bytes();
        if bytes.len() > u16::MAX as usize {
            return Err(invalid_data("word length exceeds u16::MAX"));
        }
        let str_off = strings_blob.len() as u32;
        strings_blob.extend_from_slice(bytes);
        let uni_q = quantize_logp(*uni, q_min, q_max);
        let bow_q = quantize_logp(*bow, q_min, q_max);
        word_vocab_buf.extend_from_slice(&str_off.to_le_bytes());
        word_vocab_buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        word_vocab_buf.extend_from_slice(&uni_q.to_le_bytes());
        word_vocab_buf.extend_from_slice(&bow_q.to_le_bytes());
        word_vocab_buf.extend_from_slice(&0u16.to_le_bytes());
    }
    let unk_word_uni = explicit_unk_word_uni.unwrap_or_else(|| (min_word_uni - 1.0).max(q_min as f64));
    let unk_uni_q16 = quantize_logp(unk_word_uni, q_min, q_max);

    // 2. 建置詞級 Bigram CSR (依 (prev_id, curr_id) 嚴格排序)
    let vocab_count = sorted_words.len();
    let mut skipped_missing_vocab = 0usize;
    let mut wb_indexed: Vec<(u32, u32, u16)> = Vec::with_capacity(word_bis.len());
    for ((pw, cw), lp) in word_bis {
        if let (Some(&pid), Some(&cid)) = (word_to_id.get(&pw), word_to_id.get(&cw)) {
            wb_indexed.push((pid, cid, quantize_logp(lp, q_min, q_max)));
        } else {
            skipped_missing_vocab += 1;
        }
    }
    wb_indexed.sort_unstable_by_key(|&(p, c, _)| (p, c));

    let mut wb_row_off_buf: Vec<u8> = Vec::with_capacity((vocab_count + 1) * 4);
    let mut wb_cols_buf: Vec<u8> = Vec::with_capacity(wb_indexed.len() * 4);
    let mut wb_logp_buf: Vec<u8> = Vec::with_capacity(wb_indexed.len() * 2);
    let mut wb_cursor = 0usize;
    for r in 0..vocab_count {
        wb_row_off_buf.extend_from_slice(&(wb_cursor as u32).to_le_bytes());
        while wb_cursor < wb_indexed.len() && wb_indexed[wb_cursor].0 as usize == r {
            wb_cols_buf.extend_from_slice(&wb_indexed[wb_cursor].1.to_le_bytes());
            wb_logp_buf.extend_from_slice(&wb_indexed[wb_cursor].2.to_le_bytes());
            wb_cursor += 1;
        }
    }
    wb_row_off_buf.extend_from_slice(&(wb_cursor as u32).to_le_bytes());
    let word_bi_count = wb_indexed.len();

    // 3. 排序字元表 (按 Unicode scalar codepoint 遞增)
    let mut sorted_chars: Vec<(u32, f64, f64)> = char_unis
        .into_iter()
        .map(|(ch, (u, b))| (ch as u32, u, b))
        .collect();
    sorted_chars.sort_unstable_by_key(|&(cp, _, _)| cp);

    let char_vocab_count = sorted_chars.len();
    let mut char_to_id: HashMap<u32, u16> = HashMap::with_capacity(char_vocab_count);
    let mut char_vocab_buf: Vec<u8> = Vec::with_capacity(char_vocab_count * CHAR_VOCAB_REC_BYTES);
    let mut min_char_uni = -6.0f64;
    for (idx, &(cp, uni, bow)) in sorted_chars.iter().enumerate() {
        if uni < min_char_uni {
            min_char_uni = uni;
        }
        char_to_id.insert(cp, idx as u16);
        let uni_q = quantize_logp(uni, q_min, q_max);
        let bow_q = quantize_logp(bow, q_min, q_max);
        char_vocab_buf.extend_from_slice(&cp.to_le_bytes());
        char_vocab_buf.extend_from_slice(&uni_q.to_le_bytes());
        char_vocab_buf.extend_from_slice(&bow_q.to_le_bytes());
    }
    let unk_char_uni = explicit_unk_char_uni.unwrap_or_else(|| (min_char_uni - 1.0).max(q_min as f64));
    let unk_char_uni_q16 = quantize_logp(unk_char_uni, q_min, q_max);

    // 4. 建置字級 Bigram CSR 並記錄 (c1_id, c2_id) -> cb_idx 供 Trigram 索引
    let mut cb_indexed: Vec<(u16, u16, u16, u16)> = Vec::with_capacity(char_bis.len());
    for ((c1, c2), (lp, bow)) in char_bis {
        if let (Some(&id1), Some(&id2)) = (char_to_id.get(&(c1 as u32)), char_to_id.get(&(c2 as u32))) {
            cb_indexed.push((
                id1,
                id2,
                quantize_logp(lp, q_min, q_max),
                quantize_logp(bow, q_min, q_max),
            ));
        } else {
            skipped_missing_vocab += 1;
        }
    }
    cb_indexed.sort_unstable_by_key(|&(id1, id2, _, _)| (id1, id2));

    let char_bi_count = cb_indexed.len();
    let mut cb_pair_to_idx: HashMap<(u16, u16), u32> = HashMap::with_capacity(char_bi_count);
    let mut cb_row_off_buf: Vec<u8> = Vec::with_capacity((char_vocab_count + 1) * 4);
    let mut cb_data_buf: Vec<u8> = Vec::with_capacity(char_bi_count * CHAR_BI_REC_BYTES);
    let mut cb_cursor = 0usize;
    for r in 0..char_vocab_count {
        cb_row_off_buf.extend_from_slice(&(cb_cursor as u32).to_le_bytes());
        while cb_cursor < char_bi_count && cb_indexed[cb_cursor].0 as usize == r {
            let (id1, id2, lp_q, bow_q) = cb_indexed[cb_cursor];
            cb_pair_to_idx.insert((id1, id2), cb_cursor as u32);
            cb_data_buf.extend_from_slice(&id2.to_le_bytes());
            cb_data_buf.extend_from_slice(&lp_q.to_le_bytes());
            cb_data_buf.extend_from_slice(&bow_q.to_le_bytes());
            cb_cursor += 1;
        }
    }
    cb_row_off_buf.extend_from_slice(&(cb_cursor as u32).to_le_bytes());

    // 5. 建置字級 Trigram CSR (以 cb_idx 為行鍵，c3_id 排序)
    let mut ct_indexed: Vec<(u32, u16, u16)> = Vec::with_capacity(char_tris.len());
    for ((c1, c2, c3), lp) in char_tris {
        if let (Some(&id1), Some(&id2), Some(&id3)) = (
            char_to_id.get(&(c1 as u32)),
            char_to_id.get(&(c2 as u32)),
            char_to_id.get(&(c3 as u32)),
        ) {
            if let Some(&cb_idx) = cb_pair_to_idx.get(&(id1, id2)) {
                ct_indexed.push((cb_idx, id3, quantize_logp(lp, q_min, q_max)));
            } else {
                skipped_missing_vocab += 1;
            }
        } else {
            skipped_missing_vocab += 1;
        }
    }
    ct_indexed.sort_unstable_by_key(|&(cb_idx, id3, _)| (cb_idx, id3));

    let char_tri_count = ct_indexed.len();
    let mut ct_row_off_buf: Vec<u8> = Vec::with_capacity((char_bi_count + 1) * 4);
    let mut ct_data_buf: Vec<u8> = Vec::with_capacity(char_tri_count * CHAR_TRI_REC_BYTES);
    let mut ct_cursor = 0usize;
    for r in 0..char_bi_count {
        ct_row_off_buf.extend_from_slice(&(ct_cursor as u32).to_le_bytes());
        while ct_cursor < char_tri_count && ct_indexed[ct_cursor].0 as usize == r {
            ct_data_buf.extend_from_slice(&ct_indexed[ct_cursor].1.to_le_bytes());
            ct_data_buf.extend_from_slice(&ct_indexed[ct_cursor].2.to_le_bytes());
            ct_cursor += 1;
        }
    }
    ct_row_off_buf.extend_from_slice(&(ct_cursor as u32).to_le_bytes());

    // 6. 組裝 8-byte 對齊之二進位映像並原子寫入
    let strings_blob_len = strings_blob.len() as u64;
    let mut out: Vec<u8> = vec![0u8; RLNG_HEADER_LEN];

    let off_strings_blob = out.len() as u64;
    out.extend_from_slice(&strings_blob);
    align8(&mut out);

    let off_word_vocab = out.len() as u64;
    out.extend_from_slice(&word_vocab_buf);
    align8(&mut out);

    let off_wb_row_off = out.len() as u64;
    out.extend_from_slice(&wb_row_off_buf);
    align8(&mut out);

    let off_wb_cols = out.len() as u64;
    out.extend_from_slice(&wb_cols_buf);
    align8(&mut out);

    let off_wb_logp = out.len() as u64;
    out.extend_from_slice(&wb_logp_buf);
    align8(&mut out);

    let off_char_vocab = out.len() as u64;
    out.extend_from_slice(&char_vocab_buf);
    align8(&mut out);

    let off_cb_row_off = out.len() as u64;
    out.extend_from_slice(&cb_row_off_buf);
    align8(&mut out);

    let off_cb_data = out.len() as u64;
    out.extend_from_slice(&cb_data_buf);
    align8(&mut out);

    let off_ct_row_off = out.len() as u64;
    out.extend_from_slice(&ct_row_off_buf);
    align8(&mut out);

    let off_ct_data = out.len() as u64;
    out.extend_from_slice(&ct_data_buf);
    align8(&mut out);

    out[0..8].copy_from_slice(RLNG_MAGIC);
    out[8..12].copy_from_slice(&RLNG_VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&0u32.to_le_bytes());
    out[16..20].copy_from_slice(&(vocab_count as u32).to_le_bytes());
    out[20..24].copy_from_slice(&(char_vocab_count as u32).to_le_bytes());
    out[24..28].copy_from_slice(&(word_bi_count as u32).to_le_bytes());
    out[28..32].copy_from_slice(&(char_bi_count as u32).to_le_bytes());
    out[32..36].copy_from_slice(&(char_tri_count as u32).to_le_bytes());
    out[36..40].copy_from_slice(&q_min.to_le_bytes());
    out[40..44].copy_from_slice(&q_max.to_le_bytes());
    out[44..46].copy_from_slice(&unk_uni_q16.to_le_bytes());
    out[46..48].copy_from_slice(&unk_char_uni_q16.to_le_bytes());
    out[48..56].copy_from_slice(&fingerprint.to_le_bytes());
    out[56..64].copy_from_slice(&strings_blob_len.to_le_bytes());

    out[64..72].copy_from_slice(&off_strings_blob.to_le_bytes());
    out[72..80].copy_from_slice(&off_word_vocab.to_le_bytes());
    out[80..88].copy_from_slice(&off_wb_row_off.to_le_bytes());
    out[88..96].copy_from_slice(&off_wb_cols.to_le_bytes());
    out[96..104].copy_from_slice(&off_wb_logp.to_le_bytes());
    out[104..112].copy_from_slice(&off_char_vocab.to_le_bytes());
    out[112..120].copy_from_slice(&off_cb_row_off.to_le_bytes());
    out[120..128].copy_from_slice(&off_cb_data.to_le_bytes());
    out[128..136].copy_from_slice(&off_ct_row_off.to_le_bytes());
    out[136..144].copy_from_slice(&off_ct_data.to_le_bytes());

    if let Some(parent) = bin_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp_path = bin_path.with_extension(format!("bin.tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, &out)?;
    if std::fs::rename(&tmp_path, bin_path).is_err() {
        std::fs::copy(&tmp_path, bin_path)?;
        let _ = std::fs::remove_file(&tmp_path);
    }

    Ok(NgramCompileStats {
        vocab_count,
        char_vocab_count,
        word_bi_count,
        char_bi_count,
        char_tri_count,
        skipped_missing_vocab,
        duplicate_entries_overwritten,
        corpus_fingerprint: fingerprint,
        bytes_written: out.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_arpa_tsv() -> &'static str {
        "\
# Synthetic Taiwan Traditional Chinese N-gram fixture
WU\t<s>\t-1.0\t-0.3
WU\t<unk>\t-7.5\t0.0
WU\t美\t-3.2\t-0.4
WU\t是\t-2.1\t-0.2
WU\t美式\t-4.0\t-0.5
WU\t主觀\t-3.6\t-0.3
WU\t的\t-1.4\t-0.1
WB\t<s>\t美\t-2.8
WB\t美\t是\t-1.1
WB\t是\t主觀\t-1.5
WB\t主觀\t的\t-0.8
CU\t<s>\t-0.9\t-0.2
CU\t<unk>\t-6.8\t0.0
CU\t美\t-2.9\t-0.3
CU\t是\t-1.8\t-0.2
CU\t式\t-3.4\t-0.4
CU\t主\t-2.6\t-0.3
CU\t觀\t-3.0\t-0.2
CB\t<s>\t美\t-2.4\t-0.3
CB\t美\t是\t-1.2\t-0.25
CB\t是\t主\t-1.7\t-0.15
CB\t主\t觀\t-0.6\t-0.1
CB\t美\t式\t-2.1\t-0.4
CT\t<s>\t美\t是\t-1.05
CT\t美\t是\t主\t-1.35
CT\t是\t主\t觀\t-0.45
"
    }

    #[test]
    fn test_rlng0001_compile_open_lookup_and_backoff_roundtrip() {
        let tmp_dir = std::env::temp_dir().join(format!("ruhi_rlng_roundtrip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let tsv_path = tmp_dir.join("ngram.arpa.tsv");
        let bin_path = tmp_dir.join("ngram.bin");
        std::fs::write(&tsv_path, synthetic_arpa_tsv()).unwrap();

        let stats = compile_arpa_tsv_to_bin(&tsv_path, &bin_path).expect("compile should succeed");
        assert_eq!(stats.vocab_count, 7);
        assert_eq!(stats.char_vocab_count, 6);
        assert_eq!(stats.word_bi_count, 4);
        assert_eq!(stats.char_bi_count, 5);
        assert_eq!(stats.char_tri_count, 3);
        assert_eq!(stats.skipped_missing_vocab, 0);

        let lm = NgramLanguageModel::open(&bin_path).expect("open RLNG0001 should succeed");
        assert_eq!(lm.vocab_count, 7);
        assert_eq!(lm.char_vocab_count, 6);
        assert_eq!(lm.corpus_fingerprint, stats.corpus_fingerprint);

        // Word unigram & OOV fallback
        assert!((lm.word_unigram_logp("美") - (-3.2)).abs() < 1e-3);
        assert!((lm.word_unigram_logp("未登錄詞") - (-7.5)).abs() < 1e-3);

        // Sentence start BOS + exact word bigram
        assert!((lm.score_word_bigram("", "美") - (-2.8)).abs() < 1e-3);
        assert!((lm.score_word_bigram("美", "是") - (-1.1)).abs() < 1e-3);

        // Backoff: "美" -> "的" not in WB => bow("美") + uni("的") = -0.4 + (-1.4) = -1.8
        assert!((lm.score_word_bigram("美", "的") - (-1.8)).abs() < 1e-3);

        // Unseen prev word -> fallback to curr unigram
        assert!((lm.score_word_bigram("未知前詞", "主觀") - (-3.6)).abs() < 1e-3);

        // Char bigram hit & backoff
        assert!((lm.char_bigram_logp(BOS_CHAR, '美') - (-2.4)).abs() < 1e-3);
        assert!((lm.char_bigram_logp('美', '觀') - (-0.3 + -3.0)).abs() < 1e-3);

        // Char trigram hit & backoff
        assert!((lm.char_trigram_logp(BOS_CHAR, '美', '是') - (-1.05)).abs() < 1e-3);
        assert!((lm.char_trigram_logp('美', '是', '主') - (-1.35)).abs() < 1e-3);
        // Backoff: (美, 式, 主) not in CT, but (美, 式) in CB (bow=-0.4) + bi(式, 主)
        // where bi(式, 主) falls back to bow(式)=-0.4 + uni(主)=-2.6 = -3.0 => total -3.4
        assert!((lm.char_trigram_logp('美', '式', '主') - (-3.4)).abs() < 1e-3);

        drop(lm);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_rlng0001_char_boundary_segmentation_length_invariance() {
        let tmp_dir = std::env::temp_dir().join(format!("ruhi_rlng_invariance_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let tsv_path = tmp_dir.join("ngram.arpa.tsv");
        let bin_path = tmp_dir.join("ngram.bin");
        std::fs::write(&tsv_path, synthetic_arpa_tsv()).unwrap();
        compile_arpa_tsv_to_bin(&tsv_path, &bin_path).unwrap();
        let lm = NgramLanguageModel::open(&bin_path).unwrap();

        let sum_path = |segs: &[&str]| -> (f64, f64) {
            let mut bi = 0.0;
            let mut tri = 0.0;
            for idx in 0..segs.len() {
                let prev = if idx >= 1 { segs[idx - 1] } else { "" };
                let prev2 = if idx >= 2 { Some(segs[idx - 2]) } else { None };
                let (b, t) = lm.score_char_boundary(prev2, prev, segs[idx]);
                bi += b;
                tri += t;
            }
            (bi, tri)
        };

        // 相同字元序列「美是主觀」以不同切詞邊界 [1,1,2], [2,2], [1,1,1,1], [3,1] 進行驗證
        let (bi_112, tri_112) = sum_path(&["美", "是", "主觀"]);
        let (bi_22, tri_22) = sum_path(&["美是", "主觀"]);
        let (bi_1111, tri_1111) = sum_path(&["美", "是", "主", "觀"]);
        let (bi_31, tri_31) = sum_path(&["美是主", "觀"]);

        assert!((bi_112 - bi_22).abs() < 1e-9, "bi [1,1,2] vs [2,2]");
        assert!((tri_112 - tri_22).abs() < 1e-9, "tri [1,1,2] vs [2,2]");
        assert!((bi_112 - bi_1111).abs() < 1e-9, "bi [1,1,2] vs [1,1,1,1]");
        assert!((tri_112 - tri_1111).abs() < 1e-9, "tri [1,1,2] vs [1,1,1,1]");
        assert!((bi_112 - bi_31).abs() < 1e-9, "bi [1,1,2] vs [3,1]");
        assert!((tri_112 - tri_31).abs() < 1e-9, "tri [1,1,2] vs [3,1]");

        // 確認正確搭配「美是主觀」的字級與詞級對數機率高於競爭錯詞組合「美式主觀」
        let (bi_wrong, tri_wrong) = sum_path(&["美式", "主觀"]);
        assert!(tri_112 > tri_wrong, "expected 美是主觀 trigram > 美式主觀");
        let w_right = lm.score_word_bigram("", "美")
            + lm.score_word_bigram("美", "是")
            + lm.score_word_bigram("是", "主觀");
        let w_wrong = lm.score_word_bigram("", "美式") + lm.score_word_bigram("美式", "主觀");
        assert!(w_right > w_wrong, "expected 美+是+主觀 > 美式+主觀 ({w_right} vs {w_wrong})");
        let _ = bi_wrong;

        drop(lm);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_rlng0001_corrupted_and_truncated_safety() {
        let tmp_dir = std::env::temp_dir().join(format!("ruhi_rlng_corrupt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let tsv_path = tmp_dir.join("ngram.arpa.tsv");
        let bin_path = tmp_dir.join("valid.bin");
        std::fs::write(&tsv_path, synthetic_arpa_tsv()).unwrap();
        compile_arpa_tsv_to_bin(&tsv_path, &bin_path).unwrap();
        let valid_bytes = std::fs::read(&bin_path).unwrap();

        // 1. 錯誤魔數
        let mut bad_magic = valid_bytes.clone();
        bad_magic[0..8].copy_from_slice(b"BADMAGIC");
        let p1 = tmp_dir.join("bad_magic.bin");
        std::fs::write(&p1, &bad_magic).unwrap();
        assert!(NgramLanguageModel::open(&p1).is_err());

        // 2. 截斷檔案
        let p2 = tmp_dir.join("truncated.bin");
        std::fs::write(&p2, &valid_bytes[..100]).unwrap();
        assert!(NgramLanguageModel::open(&p2).is_err());

        let p3 = tmp_dir.join("truncated_section.bin");
        std::fs::write(&p3, &valid_bytes[..valid_bytes.len() - 8]).unwrap();
        assert!(NgramLanguageModel::open(&p3).is_err());

        // 3. 越界區段偏移
        let mut bad_off = valid_bytes.clone();
        let huge_off: u64 = (valid_bytes.len() as u64) + 64;
        bad_off[136..144].copy_from_slice(&huge_off.to_le_bytes());
        let p4 = tmp_dir.join("bad_off.bin");
        std::fs::write(&p4, &bad_off).unwrap();
        assert!(NgramLanguageModel::open(&p4).is_err());

        // 4. 竄改 row_off 單調性
        let off_wb_row = read_u64_le(&valid_bytes, 80).unwrap() as usize;
        let mut bad_row = valid_bytes.clone();
        bad_row[off_wb_row..off_wb_row + 4].copy_from_slice(&999u32.to_le_bytes());
        let p5 = tmp_dir.join("bad_row.bin");
        std::fs::write(&p5, &bad_row).unwrap();
        assert!(NgramLanguageModel::open(&p5).is_err());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
