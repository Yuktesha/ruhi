// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 A Solid GUI Studio (龜鋼A工作室) / RuhI Contributors
// Licensed under the Apache License, Version 2.0 (see LICENSE in repository root).
//
// ruhi-core/src/longkan/table.rs — 如喜・龍龕形音輸入法 (Longkan Morpho-Phonetic 4-Code IME) 碼表與反查索引
//
// 架構設計要點：
// 1. 延遲載入 (Lazy Loading)：`LongkanTable::new()` 建立時不主動載入 32 MB 碼表，僅在首次實際查詢或切換至龍龕時按需載入。
// 2. 唯讀記憶體映射 (`RLKB0001` `longkan.bin`)：透過作業系統原生唯讀檔案映射 (`CreateFileMappingW` / `MapViewOfFile`)
//    讓多個 TSF 宿主程序共享同一份作業系統頁快取 (Shared Page Cache)，私有工作集記憶體低於 1 MB。
// 3. 冷熱資料分離：`breakdown` 拆字說明與反查明細置於二進位檔案尾部冷區 (`cold_pool`)，日常打字完全不觸碰該頁，僅於反查教練按需讀取。
// 4. 固定順序二分搜尋 (`lower_bound_key` / `partition_point`)：以排序鍵陣列取代雜湊表全表掃描，同權重候選以「編碼長度 → 文字字典序」固定次排序。

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub const KIND_NAMES: &[&str] = &[
    "rad_phon",
    "rad_char",
    "rad2_phon1_tone",
    "pict_3",
    "pict_4",
    "mult_c",
    "visual_4",
    "visual_3",
    "visual_2",
    "stroke_comp",
    "latin_comp",
    "hsu_tone4",
    "hsu_full_comp",
    "pinyin_full_comp",
    "multi_comp",
    "curated",
    "builtin",
    "phrase2",
    "phrase3",
    "phrase4",
    "phrase5",
    "phrase6",
    "phrase_multi",
    "other",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongkanEntry {
    pub code: String,
    pub text: String,
    pub weight: u32,
    pub kind: String,
    pub breakdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongkanReverseEntry {
    pub primary_code: String,
    pub primary_score: u32,
    pub alt_codes: Vec<String>,
    pub alt_breakdowns: Vec<(String, String)>,
    pub breakdown: String,
}

/// 作業系統原生唯讀記憶體映射緩衝區（Windows `CreateFileMappingW` / `MapViewOfFile`；非 Windows 採用唯讀緩衝區）
pub struct ReadOnlyMmap {
    ptr: *const u8,
    len: usize,
    #[cfg(target_os = "windows")]
    mapping_handle: isize,
    #[cfg(not(target_os = "windows"))]
    _owned_fallback: Vec<u8>,
}

unsafe impl Send for ReadOnlyMmap {}
unsafe impl Sync for ReadOnlyMmap {}

#[cfg(target_os = "windows")]
mod win_mmap {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    const PAGE_READONLY: u32 = 0x02;
    const FILE_MAP_READ: u32 = 0x0004;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(
            hFile: isize,
            lpFileMappingAttributes: *const core::ffi::c_void,
            flProtect: u32,
            dwMaximumSizeHigh: u32,
            dwMaximumSizeLow: u32,
            lpName: *const u16,
        ) -> isize;
        fn MapViewOfFile(
            hFileMappingObject: isize,
            dwDesiredAccess: u32,
            dwFileOffsetHigh: u32,
            dwFileOffsetLow: u32,
            dwNumberOfBytesToMap: usize,
        ) -> *const u8;
        fn UnmapViewOfFile(lpBaseAddress: *const u8) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }

    pub fn map_file(file: &File, len: usize) -> io::Result<(*const u8, isize)> {
        if len == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "empty file"));
        }
        unsafe {
            let h_file = file.as_raw_handle() as isize;
            let h_map = CreateFileMappingW(
                h_file,
                core::ptr::null(),
                PAGE_READONLY,
                0,
                0,
                core::ptr::null(),
            );
            if h_map == 0 {
                return Err(io::Error::last_os_error());
            }
            let ptr = MapViewOfFile(h_map, FILE_MAP_READ, 0, 0, len);
            if ptr.is_null() {
                let err = io::Error::last_os_error();
                CloseHandle(h_map);
                return Err(err);
            }
            Ok((ptr, h_map))
        }
    }

    pub fn unmap(ptr: *const u8, h_map: isize) {
        unsafe {
            if !ptr.is_null() {
                UnmapViewOfFile(ptr);
            }
            if h_map != 0 {
                CloseHandle(h_map);
            }
        }
    }
}

impl ReadOnlyMmap {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        #[cfg(target_os = "windows")]
        {
            let (ptr, mapping_handle) = win_mmap::map_file(&file, len)?;
            Ok(Self {
                ptr,
                len,
                mapping_handle,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            drop(file);
            let bytes = std::fs::read(path)?;
            let ptr = bytes.as_ptr();
            let len = bytes.len();
            Ok(Self {
                ptr,
                len,
                _owned_fallback: bytes,
            })
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for ReadOnlyMmap {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        {
            win_mmap::unmap(self.ptr, self.mapping_handle);
        }
    }
}

/// `RLKB0001` 唯讀記憶體映射碼表檢視器
pub struct LongkanMmap {
    raw: ReadOnlyMmap,
    pub num_keys: u32,
    pub num_entries: u32,
    pub num_reverse_chars: u32,
    pub source_fingerprint: u64,
    keys_offset: usize,
    entries_offset: usize,
    reverse_offset: usize,
    hot_strings_offset: usize,
    hot_strings_len: usize,
    cold_pool_offset: usize,
    cold_pool_len: usize,
}

impl std::fmt::Debug for LongkanMmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LongkanMmap")
            .field("num_keys", &self.num_keys)
            .field("num_entries", &self.num_entries)
            .field("num_reverse_chars", &self.num_reverse_chars)
            .field("source_fingerprint", &format_args!("{:#018x}", self.source_fingerprint))
            .finish()
    }
}

#[inline]
fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

#[inline]
fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

#[inline]
fn read_u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// 計算忽略 `\r` 換行差異之確定性 64-bit 來源 TSV 指紋：`((norm_len as u64) << 32) | (crc32_ieee as u64)`
pub fn compute_tsv_fingerprint_from_slice(raw: &[u8]) -> u64 {
    let mut crc: u32 = 0xFFFF_FFFF;
    let mut norm_len: u32 = 0;
    for &b in raw {
        if b == b'\r' {
            continue;
        }
        norm_len = norm_len.wrapping_add(1);
        let idx = ((crc ^ (b as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    let final_crc = !crc;
    ((norm_len as u64) << 32) | (final_crc as u64)
}

/// 計算指定 `longkan.tsv` 檔案之 64-bit 來源指紋
pub fn compute_tsv_fingerprint(tsv_path: &Path) -> io::Result<u64> {
    let bytes = std::fs::read(tsv_path)?;
    Ok(compute_tsv_fingerprint_from_slice(&bytes))
}

impl LongkanMmap {
    pub fn open(path: &Path) -> io::Result<Self> {
        let raw = ReadOnlyMmap::open(path)?;
        let buf = raw.as_slice();
        if buf.len() < 64 || &buf[0..8] != b"RLKB0001" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid RLKB0001 header",
            ));
        }
        let num_keys = read_u32_le(buf, 8);
        let num_entries = read_u32_le(buf, 12);
        let num_reverse_chars = read_u32_le(buf, 16);
        let keys_offset = read_u32_le(buf, 20) as usize;
        let entries_offset = read_u32_le(buf, 24) as usize;
        let reverse_offset = read_u32_le(buf, 28) as usize;
        let hot_strings_offset = read_u32_le(buf, 32) as usize;
        let hot_strings_len = read_u32_le(buf, 36) as usize;
        let cold_pool_offset = read_u32_le(buf, 40) as usize;
        let cold_pool_len = read_u32_le(buf, 44) as usize;
        let source_fingerprint = read_u64_le(buf, 48);

        if keys_offset + (num_keys as usize) * 16 > buf.len()
            || entries_offset + (num_entries as usize) * 16 > buf.len()
            || reverse_offset + (num_reverse_chars as usize) * 20 > buf.len()
            || hot_strings_offset + hot_strings_len > buf.len()
            || cold_pool_offset + cold_pool_len > buf.len()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "RLKB0001 section out of bounds",
            ));
        }

        Ok(Self {
            raw,
            num_keys,
            num_entries,
            num_reverse_chars,
            source_fingerprint,
            keys_offset,
            entries_offset,
            reverse_offset,
            hot_strings_offset,
            hot_strings_len,
            cold_pool_offset,
            cold_pool_len,
        })
    }

    /// 開啟 `RLKB0001` 映射檔；若提供 `tsv_path` 且檔案存在，則強制比對標頭內記錄之來源 `.tsv` 指紋，防止載入過期 `.bin`
    pub fn open_verified(bin_path: &Path, tsv_path: Option<&Path>) -> io::Result<Self> {
        let mmap = Self::open(bin_path)?;
        if let Some(tp) = tsv_path {
            if tp.is_file() {
                let expected_fp = compute_tsv_fingerprint(tp)?;
                if mmap.source_fingerprint == 0 || mmap.source_fingerprint != expected_fp {
                    eprintln!(
                        "[RuhI Longkan WARN] 偵測到過期或不一致的二進位索引檔 {} (bin_fp={:#018x}, tsv_fp={:#018x}, tsv={})；拒絕使用過期 .bin。",
                        bin_path.display(),
                        mmap.source_fingerprint,
                        expected_fp,
                        tp.display()
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "stale longkan.bin fingerprint mismatch with longkan.tsv",
                    ));
                }
            }
        }
        Ok(mmap)
    }

    #[inline]
    fn hot_str(&self, off: u32, len: u16) -> &str {
        let buf = self.raw.as_slice();
        let start = self.hot_strings_offset + (off as usize);
        let end = start + (len as usize);
        if end <= self.hot_strings_offset + self.hot_strings_len {
            std::str::from_utf8(&buf[start..end]).unwrap_or("")
        } else {
            ""
        }
    }

    #[inline]
    fn key_record(&self, idx: usize) -> (&[u8], u16, u32, u32) {
        let buf = self.raw.as_slice();
        let base = self.keys_offset + idx * 16;
        let code_off = read_u32_le(buf, base) as usize;
        let code_len = read_u16_le(buf, base + 4) as usize;
        let flags = read_u16_le(buf, base + 6);
        let first_entry_idx = read_u32_le(buf, base + 8);
        let entry_count = read_u32_le(buf, base + 12);
        let s_start = self.hot_strings_offset + code_off;
        let s_end = (s_start + code_len).min(self.hot_strings_offset + self.hot_strings_len);
        (&buf[s_start..s_end], flags, first_entry_idx, entry_count)
    }

    #[inline]
    fn entry_record(&self, idx: u32, code_str: &str) -> LongkanEntry {
        let buf = self.raw.as_slice();
        let base = self.entries_offset + (idx as usize) * 16;
        let text_off = read_u32_le(buf, base);
        let text_len = read_u16_le(buf, base + 4);
        let kind_id = buf[base + 7] as usize;
        let weight = read_u32_le(buf, base + 8);
        let text = self.hot_str(text_off, text_len).to_string();
        let kind = KIND_NAMES
            .get(kind_id)
            .copied()
            .unwrap_or("other")
            .to_string();
        LongkanEntry {
            code: code_str.to_string(),
            text,
            weight,
            kind,
            // 依交接規範 1.2.3：日常查詢不載入拆字說明，避免觸碰冷區分頁與堆積配置
            breakdown: String::new(),
        }
    }

    #[inline]
    fn entry_meta(&self, idx: u32) -> (u8, u32, &str) {
        let buf = self.raw.as_slice();
        let base = self.entries_offset + (idx as usize) * 16;
        let text_off = read_u32_le(buf, base);
        let text_len = read_u16_le(buf, base + 4);
        let char_count = buf[base + 6];
        let weight = read_u32_le(buf, base + 8);
        let text = self.hot_str(text_off, text_len);
        (char_count, weight, text)
    }

    #[inline]
    fn entry_char_count(&self, idx: u32) -> u8 {
        let buf = self.raw.as_slice();
        let base = self.entries_offset + (idx as usize) * 16;
        buf[base + 6]
    }

    /// 二分搜尋找出第一個 `key >= target` 之索引 (`partition_point`)
    #[inline]
    pub fn lower_bound_key(&self, target: &[u8]) -> usize {
        let mut left = 0usize;
        let mut right = self.num_keys as usize;
        while left < right {
            let mid = left + (right - left) / 2;
            let (k_bytes, _, _, _) = self.key_record(mid);
            if k_bytes < target {
                left = mid + 1;
            } else {
                right = mid;
            }
        }
        left
    }

    pub fn reverse_lookup_char(&self, ch: char) -> Option<LongkanReverseEntry> {
        let target_cp = ch as u32;
        let buf = self.raw.as_slice();
        let mut left = 0usize;
        let mut right = self.num_reverse_chars as usize;
        while left < right {
            let mid = left + (right - left) / 2;
            let base = self.reverse_offset + mid * 20;
            let cp = read_u32_le(buf, base);
            if cp < target_cp {
                left = mid + 1;
            } else if cp > target_cp {
                right = mid;
            } else {
                let p_off = read_u32_le(buf, base + 4);
                let p_len = read_u16_le(buf, base + 8);
                let primary_code = self.hot_str(p_off, p_len).to_string();
                let cold_detail_off = read_u32_le(buf, base + 16) as usize;
                let c_base = self.cold_pool_offset + cold_detail_off;
                if c_base + 8 > self.cold_pool_offset + self.cold_pool_len {
                    return None;
                }
                let primary_score = read_u32_le(buf, c_base);
                let p_bd_len = read_u16_le(buf, c_base + 4) as usize;
                let alt_count = read_u16_le(buf, c_base + 6) as usize;
                let mut cur = c_base + 8;
                let breakdown = std::str::from_utf8(&buf[cur..cur + p_bd_len])
                    .unwrap_or("")
                    .to_string();
                cur += p_bd_len;
                let mut alt_codes = Vec::with_capacity(alt_count);
                let mut alt_breakdowns = Vec::with_capacity(alt_count);
                for _ in 0..alt_count {
                    if cur + 4 > self.cold_pool_offset + self.cold_pool_len {
                        break;
                    }
                    let ac_len = read_u16_le(buf, cur) as usize;
                    let abd_len = read_u16_le(buf, cur + 2) as usize;
                    cur += 4;
                    if cur + ac_len + abd_len > self.cold_pool_offset + self.cold_pool_len {
                        break;
                    }
                    let ac = std::str::from_utf8(&buf[cur..cur + ac_len])
                        .unwrap_or("")
                        .to_string();
                    cur += ac_len;
                    let abd = std::str::from_utf8(&buf[cur..cur + abd_len])
                        .unwrap_or("")
                        .to_string();
                    cur += abd_len;
                    alt_codes.push(ac.clone());
                    alt_breakdowns.push((ac, abd));
                }
                return Some(LongkanReverseEntry {
                    primary_code,
                    primary_score,
                    alt_codes,
                    alt_breakdowns,
                    breakdown,
                });
            }
        }
        None
    }

    /// 不觸碰冷區 `cold_pool`、直接以位元遮罩判斷單字是否有以指定首字母 `h` 開頭之龍龕碼
    #[inline]
    pub fn char_has_head_letter(&self, ch: char, h: char) -> bool {
        let h_lower = h.to_ascii_lowercase();
        if !h_lower.is_ascii_lowercase() {
            return false;
        }
        let bit = 1u32 << ((h_lower as u8) - b'a');
        let target_cp = ch as u32;
        let buf = self.raw.as_slice();
        let mut left = 0usize;
        let mut right = self.num_reverse_chars as usize;
        while left < right {
            let mid = left + (right - left) / 2;
            let base = self.reverse_offset + mid * 20;
            let cp = read_u32_le(buf, base);
            if cp < target_cp {
                left = mid + 1;
            } else if cp > target_cp {
                right = mid;
            } else {
                let mask = read_u32_le(buf, base + 12);
                return (mask & bit) != 0;
            }
        }
        false
    }
}

static GLOBAL_DEFAULT_MMAP: OnceLock<Option<Arc<LongkanMmap>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct LongkanTable {
    /// 延遲載入之唯讀記憶體映射主碼表（跨實例共享 `Arc`）
    mmap_cell: OnceLock<Option<Arc<LongkanMmap>>>,
    /// 由 `RuhiPaths` 或 `new_with_data_dir` 傳入之資料目錄
    configured_data_dir: Option<PathBuf>,
    /// 僅存放使用者自訂詞或小型單元測試動態新增之覆寫條目（一般執行時為空）
    pub map: HashMap<String, Vec<LongkanEntry>>,
    /// 僅存放動態新增條目之排序索引 `(code, entries)`，供二分搜尋使用
    sorted_overlay: Vec<(String, Vec<LongkanEntry>)>,
    pub char_reverse: HashMap<char, LongkanReverseEntry>,
}

impl Default for LongkanTable {
    fn default() -> Self {
        Self {
            mmap_cell: OnceLock::new(),
            configured_data_dir: None,
            map: HashMap::new(),
            sorted_overlay: Vec::new(),
            char_reverse: HashMap::new(),
        }
    }
}

impl LongkanTable {
    /// 建立延遲載入之龍龕碼表（不於建構時讀取磁碟，首次查詢時才映射 `longkan.bin`）
    pub fn new() -> Self {
        Self::default()
    }

    /// 設定資料目錄（由 `RuhiEngine::new_with_data_dir` 或 `RuhiPaths` 傳入，不立即載入碼表）
    pub fn configure_data_dir(&mut self, dir: impl AsRef<Path>) {
        self.configured_data_dir = Some(dir.as_ref().to_path_buf());
    }

    /// 是否已完成主碼表映射或載入
    pub fn is_loaded(&self) -> bool {
        self.mmap_cell.get().is_some() || !self.sorted_overlay.is_empty()
    }

    /// 觸發延遲載入／唯讀記憶體映射，確保主碼表已就緒
    pub fn ensure_loaded(&self) {
        let _ = self.get_mmap();
    }

    /// 回傳唯讀記憶體映射區域之位元組大小（若尚未映射則先觸發映射）
    pub fn mmap_bytes(&self) -> usize {
        self.get_mmap().map(|m| m.raw.len).unwrap_or(0)
    }

    /// 正式版與開發環境之標準碼表路徑解析（遵循交接規範 4.1：正式版僅讀安裝目錄與 `%APPDATA%\Ruhi\data`）
    pub fn resolve_table_paths(configured_dir: Option<&Path>) -> (Option<PathBuf>, Option<PathBuf>) {
        let mut search_dirs: Vec<PathBuf> = Vec::new();
        if let Some(d) = configured_dir {
            search_dirs.push(d.to_path_buf());
        }
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
            search_dirs.push(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ruhi_lexicon/data"),
            );
        }

        let mut bin_found = None;
        let mut tsv_found = None;
        for d in &search_dirs {
            let b = d.join("longkan.bin");
            if bin_found.is_none() && b.is_file() {
                bin_found = Some(b);
            }
            let t = d.join("longkan.tsv");
            if tsv_found.is_none() && t.is_file() {
                tsv_found = Some(t);
            }
            if bin_found.is_some() && tsv_found.is_some() {
                break;
            }
        }
        (bin_found, tsv_found)
    }

    /// 取得使用者應用資料快取目錄下之 `longkan.bin` 路徑（`%APPDATA%\Ruhi\data\longkan.bin`），絕不寫入安裝目錄
    pub fn user_cache_bin_path(tsv_path: &Path) -> Option<PathBuf> {
        let base_dir = if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata).join("Ruhi").join("data")
        } else if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
            PathBuf::from(xdg).join("ruhi").join("data")
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".cache").join("ruhi").join("data")
        } else {
            std::env::temp_dir().join("Ruhi").join("data")
        };
        let is_standard_longkan = tsv_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("longkan.tsv"))
            .unwrap_or(false);
        if is_standard_longkan {
            Some(base_dir.join("longkan.bin"))
        } else {
            let fp = compute_tsv_fingerprint(tsv_path).unwrap_or(0);
            Some(base_dir.join(format!("longkan_{:016x}.bin", fp)))
        }
    }

    fn open_or_build_mmap(
        bin_opt: Option<&Path>,
        tsv_opt: Option<&Path>,
    ) -> Option<Arc<LongkanMmap>> {
        if let Some(bin_path) = bin_opt {
            if bin_path.is_file() {
                if let Ok(m) = LongkanMmap::open_verified(bin_path, tsv_opt) {
                    return Some(Arc::new(m));
                }
            }
        }
        if let Some(tsv_path) = tsv_opt {
            if tsv_path.is_file() {
                if let Some(cache_bin) = Self::user_cache_bin_path(tsv_path) {
                    if cache_bin.is_file() {
                        if let Ok(m) = LongkanMmap::open_verified(&cache_bin, Some(tsv_path)) {
                            return Some(Arc::new(m));
                        }
                    }
                    eprintln!(
                        "[RuhI Longkan WARN] 未找到有效的 longkan.bin，請執行轉換工具（`cargo run --release --bin ruhi_lkbin -- {}`）；正在自動將 TSV 轉換並快取至 {} ...",
                        tsv_path.display(),
                        cache_bin.display()
                    );
                    if compile_tsv_to_bin(tsv_path, &cache_bin).is_ok() {
                        if let Ok(m) = LongkanMmap::open_verified(&cache_bin, Some(tsv_path)) {
                            return Some(Arc::new(m));
                        }
                    }
                } else {
                    eprintln!(
                        "[RuhI Longkan WARN] 未找到 longkan.bin，請執行轉換工具：`cargo run --release --bin ruhi_lkbin -- {}`",
                        tsv_path.display()
                    );
                }
            }
        }
        None
    }

    #[inline]
    fn get_mmap(&self) -> Option<&Arc<LongkanMmap>> {
        self.mmap_cell
            .get_or_init(|| {
                let (bin_opt, tsv_opt) =
                    Self::resolve_table_paths(self.configured_data_dir.as_deref());
                if self.configured_data_dir.is_none() {
                    return GLOBAL_DEFAULT_MMAP
                        .get_or_init(|| {
                            Self::open_or_build_mmap(bin_opt.as_deref(), tsv_opt.as_deref())
                        })
                        .clone();
                }
                if let Some(m) = Self::open_or_build_mmap(bin_opt.as_deref(), tsv_opt.as_deref()) {
                    return Some(m);
                }
                GLOBAL_DEFAULT_MMAP
                    .get_or_init(|| {
                        let (b_def, t_def) = Self::resolve_table_paths(None);
                        Self::open_or_build_mmap(b_def.as_deref(), t_def.as_deref())
                    })
                    .clone()
            })
            .as_ref()
    }

    pub fn insert_entry(
        &mut self,
        code: &str,
        text: &str,
        weight: u32,
        kind: &str,
        breakdown: &str,
    ) {
        let c_lower = code.trim().to_ascii_lowercase();
        let t_clean = text.trim();
        if c_lower.is_empty() || t_clean.is_empty() {
            return;
        }
        let entry = LongkanEntry {
            code: c_lower.clone(),
            text: t_clean.to_string(),
            weight,
            kind: kind.trim().to_string(),
            breakdown: breakdown.trim().to_string(),
        };
        let list = self.map.entry(c_lower.clone()).or_default();
        if let Some(existing) = list.iter_mut().find(|e| e.text == t_clean) {
            if weight > existing.weight {
                existing.weight = weight;
                existing.kind = entry.kind.clone();
                if !entry.breakdown.is_empty() {
                    existing.breakdown = entry.breakdown.clone();
                }
            }
        } else {
            list.push(entry);
        }

        if t_clean.chars().count() == 1 {
            if let Some(ch) = t_clean.chars().next() {
                let b_clean = breakdown.trim().to_string();
                let is_pure_4alpha =
                    c_lower.len() == 4 && c_lower.chars().all(|c| c.is_ascii_alphabetic());
                let score = weight
                    + if kind.trim() == "curated" { 1000 } else { 0 }
                    + if is_pure_4alpha { 250 } else { 0 };
                let rev = self
                    .char_reverse
                    .entry(ch)
                    .or_insert_with(|| LongkanReverseEntry {
                        primary_code: c_lower.clone(),
                        primary_score: score,
                        alt_codes: Vec::new(),
                        alt_breakdowns: Vec::new(),
                        breakdown: b_clean.clone(),
                    });
                if rev.primary_code == c_lower {
                    if score > rev.primary_score {
                        rev.primary_score = score;
                    }
                    if rev.breakdown.is_empty() && !b_clean.is_empty() {
                        rev.breakdown = b_clean;
                    }
                } else if score > rev.primary_score {
                    let old_code = rev.primary_code.clone();
                    let old_bdesc = rev.breakdown.clone();
                    rev.alt_codes.retain(|x| x != &c_lower && x != &old_code);
                    rev.alt_breakdowns
                        .retain(|(x, _)| x != &c_lower && x != &old_code);
                    rev.alt_codes.insert(0, old_code.clone());
                    rev.alt_breakdowns.insert(0, (old_code, old_bdesc));
                    rev.primary_code = c_lower;
                    rev.primary_score = score;
                    rev.breakdown = b_clean;
                } else if !rev.alt_codes.contains(&c_lower) {
                    rev.alt_codes.push(c_lower.clone());
                    rev.alt_breakdowns.push((c_lower, b_clean));
                }
            }
        }
    }

    pub fn finalize_sorting(&mut self) {
        for list in self.map.values_mut() {
            list.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.text.cmp(&b.text)));
        }
        let mut vec: Vec<(String, Vec<LongkanEntry>)> = self
            .map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        vec.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        self.sorted_overlay = vec;
    }

    pub fn load_tsv(&mut self, path: &Path) -> io::Result<usize> {
        // 優先驗證同目錄之 .bin 是否與來源 .tsv 指紋一致；若缺少或過期則自動轉換至 %APPDATA%\Ruhi\data 並映射
        let bin_candidate = path.with_extension("bin");
        let bin_opt = if bin_candidate.is_file() {
            Some(bin_candidate.as_path())
        } else {
            None
        };
        if let Some(m) = Self::open_or_build_mmap(bin_opt, Some(path)) {
            let count = m.num_entries as usize;
            self.map.clear();
            self.sorted_overlay.clear();
            self.char_reverse.clear();
            self.mmap_cell = OnceLock::new();
            let _ = self.mmap_cell.set(Some(m));
            return Ok(count);
        }

        eprintln!(
            "[RuhI Longkan WARN] 未找到有效的 longkan.bin，請執行轉換工具（`cargo run --release --bin ruhi_lkbin -- {}`）；目前退回以明文 TSV 載入記憶體。",
            path.display()
        );
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0usize;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 3 {
                let code = parts[0].trim();
                let text = parts[1].trim();
                let weight = parts[2].trim().parse::<u32>().unwrap_or(500);
                let kind = if parts.len() >= 4 {
                    parts[3].trim()
                } else {
                    "rad_phon"
                };
                let bdesc = if parts.len() >= 5 {
                    parts[4].trim()
                } else {
                    ""
                };
                self.insert_entry(code, text, weight, kind, bdesc);
                count += 1;
            }
        }
        self.finalize_sorting();
        Ok(count)
    }

    pub fn load_builtin_defaults(&mut self) {
        if self.get_mmap().is_some() {
            return;
        }
        let builtins: &[(&str, &str, u32, &str, &str)] = &[
            ("lnln", "龍", 2680, "curated", "獨體疊音:龍(ln)+龍(ln)"),
            ("kmfu", "鋪", 2600, "curated", "金(km)+甫(fu)"),
            ("jnfx", "鋪", 2500, "curated", "金(jn)+甫(fx)"),
            ("helo", "龕", 2550, "curated", "合(he)+龍(lo)"),
            ("hgln", "龕", 2660, "curated", "許氏友善:合(hg)+龍(ln)"),
            ("oi", "中", 2600, "curated", "視覺拆字:口(o)+丨(i)"),
            ("ooo", "品", 2650, "curated", "視覺拆字:口(o)×3+␣"),
            ("to", "古", 2600, "curated", "十(t)+口(o)+␣"),
            ("bd", "明", 2620, "curated", "視覺簡碼:日(b)+月(d)"),
            ("cooo", "區", 2620, "curated", "視覺拆字:匚(c)+口×3(ooo)"),
            ("cko3", "區", 2600, "curated", "匚(c)+口(ko)×3"),
            ("ko3", "品", 2600, "curated", "口(ko)×3+␣"),
            ("mu3", "森", 2600, "curated", "木(mu)×3+␣"),
        ];
        for &(code, text, weight, kind, bdesc) in builtins {
            self.insert_entry(code, text, weight, kind, bdesc);
        }
        self.finalize_sorting();
    }

    /// 僅回傳與給定編碼精確匹配之條目（不含前綴填充；供事後部件融合 `Shift+← + Tab` 與精確驗證使用）
    pub fn lookup_exact(&self, code: &str, is_word_mode: bool) -> Vec<LongkanEntry> {
        let clean = code.trim().trim_start_matches('-').to_ascii_lowercase();
        if clean.is_empty() {
            return Vec::new();
        }
        if !self.sorted_overlay.is_empty() && self.map.is_empty() {
            // already sorted
        }
        let mut results = Vec::new();
        let mut seen = HashSet::new();

        // 1. Check overlay first
        if !self.sorted_overlay.is_empty() {
            let idx = self
                .sorted_overlay
                .partition_point(|(k, _)| k.as_bytes() < clean.as_bytes());
            if idx < self.sorted_overlay.len() && self.sorted_overlay[idx].0 == clean {
                for e in &self.sorted_overlay[idx].1 {
                    let char_len = e.text.chars().count();
                    if is_word_mode && char_len < 2 {
                        continue;
                    }
                    if !is_word_mode && char_len >= 2 {
                        continue;
                    }
                    if seen.insert(e.text.clone()) {
                        results.push(e.clone());
                    }
                }
            }
        } else if let Some(exact) = self.map.get(&clean) {
            for e in exact {
                let char_len = e.text.chars().count();
                if is_word_mode && char_len < 2 {
                    continue;
                }
                if !is_word_mode && char_len >= 2 {
                    continue;
                }
                if seen.insert(e.text.clone()) {
                    results.push(e.clone());
                }
            }
        }

        // 2. Check memory-mapped RLKB0001 table via binary search
        if let Some(mmap) = self.get_mmap() {
            let idx = mmap.lower_bound_key(clean.as_bytes());
            if idx < mmap.num_keys as usize {
                let (k_bytes, _, first_idx, count) = mmap.key_record(idx);
                if k_bytes == clean.as_bytes() {
                    for i in 0..count {
                        let e_idx = first_idx + i;
                        let ch_len = mmap.entry_char_count(e_idx) as usize;
                        if is_word_mode && ch_len < 2 {
                            continue;
                        }
                        if !is_word_mode && ch_len >= 2 {
                            continue;
                        }
                        let entry = mmap.entry_record(e_idx, &clean);
                        if seen.insert(entry.text.clone()) {
                            results.push(entry);
                        }
                    }
                }
            }
        }
        results
    }

    pub fn lookup(&self, code: &str) -> Vec<LongkanEntry> {
        let raw_clean = code.trim().to_ascii_lowercase();
        if raw_clean.is_empty() {
            return Vec::new();
        }
        if raw_clean == "-" {
            return ["台灣", "輸入", "龍龕", "形音", "如喜", "智慧", "國家", "電腦"]
                .iter()
                .enumerate()
                .map(|(i, &w)| LongkanEntry {
                    code: "-".to_string(),
                    text: w.to_string(),
                    weight: (2500 - i * 20) as u32,
                    kind: "phrase2".to_string(),
                    breakdown: String::new(),
                })
                .collect();
        }
        let is_word_mode = raw_clean.starts_with('-');
        let clean = raw_clean.trim_start_matches('-').to_string();
        if clean.is_empty() {
            return Vec::new();
        }
        if clean.contains('?') || clean.contains('*') {
            let mut w_res = self.lookup_wildcard(&clean);
            if is_word_mode {
                w_res.retain(|e| e.text.chars().count() >= 2);
            } else if w_res.iter().any(|e| e.text.chars().count() == 1) {
                w_res.retain(|e| e.text.chars().count() == 1);
            }
            return w_res;
        }

        let mut results = self.lookup_exact(&clean, is_word_mode);
        let mut seen: HashSet<String> = results.iter().map(|e| e.text.clone()).collect();

        // 前綴預覽候選：透過二分搜尋 (`lower_bound_key` / `partition_point`) 僅走訪連續前綴區間，
        // 並在 mmap 切片層完成 Top-35 輕量篩選後才建構 `LongkanEntry`，杜絕中途堆積配置延遲
        if clean.len() < 12 && results.len() < 15 {
            let mut prefix_hits: Vec<LongkanEntry> = Vec::new();
            let clean_bytes = clean.as_bytes();
            let req_flag: u16 = if is_word_mode { 0x0002 } else { 0x0001 };

            if !self.sorted_overlay.is_empty() {
                let start_idx = self
                    .sorted_overlay
                    .partition_point(|(k, _)| k.as_bytes() <= clean_bytes);
                for (k, list) in &self.sorted_overlay[start_idx..] {
                    if !k.as_bytes().starts_with(clean_bytes) {
                        break;
                    }
                    for e in list {
                        let char_len = e.text.chars().count();
                        if (is_word_mode && char_len >= 2) || (!is_word_mode && char_len == 1) {
                            prefix_hits.push(e.clone());
                            break;
                        }
                    }
                }
            }

            if let Some(mmap) = self.get_mmap() {
                let mut raw_hits: Vec<(bool, u32, usize, &str, u32, &str)> = Vec::new();
                let mut idx = mmap.lower_bound_key(clean_bytes);
                let total = mmap.num_keys as usize;
                while idx < total {
                    let (k_bytes, flags, first_idx, count) = mmap.key_record(idx);
                    if !k_bytes.starts_with(clean_bytes) {
                        break;
                    }
                    if k_bytes.len() > clean_bytes.len() && (flags & req_flag) != 0 {
                        let k_str = std::str::from_utf8(k_bytes).unwrap_or("");
                        for i in 0..count {
                            let e_idx = first_idx + i;
                            let (ch_len_u8, weight, text_str) = mmap.entry_meta(e_idx);
                            let ch_len = ch_len_u8 as usize;
                            if (is_word_mode && ch_len >= 2) || (!is_word_mode && ch_len == 1) {
                                let multi3 = is_word_mode && ch_len >= 3;
                                raw_hits.push((multi3, weight, k_bytes.len(), text_str, e_idx, k_str));
                                break;
                            }
                        }
                    }
                    idx += 1;
                }
                raw_hits.sort_unstable_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| b.1.cmp(&a.1))
                        .then_with(|| a.2.cmp(&b.2))
                        .then_with(|| a.3.cmp(b.3))
                });
                for (_, _, _, _, e_idx, k_str) in raw_hits.into_iter().take(40) {
                    prefix_hits.push(mmap.entry_record(e_idx, k_str));
                }
            }

            // 依交接規範 3.2.2：權重相同時，以「編碼長度 → 文字字典序」作為固定次排序鍵
            prefix_hits.sort_by(|a, b| {
                let a_multi3 = is_word_mode && a.text.chars().count() >= 3;
                let b_multi3 = is_word_mode && b.text.chars().count() >= 3;
                b_multi3
                    .cmp(&a_multi3)
                    .then_with(|| b.weight.cmp(&a.weight))
                    .then_with(|| a.code.len().cmp(&b.code.len()))
                    .then_with(|| a.text.cmp(&b.text))
            });
            for e in prefix_hits {
                if seen.insert(e.text.clone()) {
                    results.push(e);
                    if results.len() >= 30 {
                        break;
                    }
                }
            }
        }

        // 無 `-` 前導時：若該碼連前綴預覽都完全沒有任何單字，才保底回傳精確匹配之多字詞
        if !is_word_mode && results.is_empty() {
            for e in self.lookup_exact(&clean, true) {
                if seen.insert(e.text.clone()) {
                    results.push(e);
                }
            }
        }
        results
    }

    /// 依單字模式 (`is_word_mode == false`) 或多字詞模式 (`is_word_mode == true`) 二分搜尋檢查給定字串是否為合法完整編碼或有效前綴
    pub fn has_exact_or_prefix_for_mode(&self, probe: &str, is_word_mode: bool) -> bool {
        let clean = probe.trim().trim_start_matches('-').to_ascii_lowercase();
        if clean.is_empty() || clean.len() > 12 {
            return false;
        }
        let clean_bytes = clean.as_bytes();
        let req_flag: u16 = if is_word_mode { 0x0002 } else { 0x0001 };

        if !self.sorted_overlay.is_empty() {
            let start_idx = self
                .sorted_overlay
                .partition_point(|(k, _)| k.as_bytes() < clean_bytes);
            for (k, list) in &self.sorted_overlay[start_idx..] {
                if !k.as_bytes().starts_with(clean_bytes) {
                    break;
                }
                if list.iter().any(|e| {
                    let clen = e.text.chars().count();
                    if is_word_mode {
                        clen >= 2
                    } else {
                        clen == 1
                    }
                }) {
                    return true;
                }
            }
        }

        if let Some(mmap) = self.get_mmap() {
            let mut idx = mmap.lower_bound_key(clean_bytes);
            let total = mmap.num_keys as usize;
            while idx < total {
                let (k_bytes, flags, _, _) = mmap.key_record(idx);
                if !k_bytes.starts_with(clean_bytes) {
                    break;
                }
                if (flags & req_flag) != 0 {
                    return true;
                }
                idx += 1;
            }
        }
        false
    }

    /// 二分搜尋判斷給定字串是否為字典中已存在的完整編碼或有效前綴
    pub fn has_exact_or_prefix(&self, probe: &str) -> bool {
        let raw_clean = probe.trim().to_ascii_lowercase();
        let clean = raw_clean.trim_start_matches('-');
        if clean.is_empty() || clean.len() > 12 {
            return false;
        }
        let clean_bytes = clean.as_bytes();

        if !self.sorted_overlay.is_empty() {
            let idx = self
                .sorted_overlay
                .partition_point(|(k, _)| k.as_bytes() < clean_bytes);
            if idx < self.sorted_overlay.len()
                && self.sorted_overlay[idx].0.as_bytes().starts_with(clean_bytes)
            {
                return true;
            }
        }

        if let Some(mmap) = self.get_mmap() {
            let idx = mmap.lower_bound_key(clean_bytes);
            if idx < mmap.num_keys as usize {
                let (k_bytes, _, _, _) = mmap.key_record(idx);
                if k_bytes.starts_with(clean_bytes) {
                    return true;
                }
            }
        }
        false
    }

    /// 二分搜尋判斷給定字串後是否仍可接續英文字母構成更長的同模式部件編碼
    pub fn has_longer_alpha_prefix(&self, probe: &str) -> bool {
        let raw_clean = probe.trim().to_ascii_lowercase();
        let is_word = raw_clean.starts_with('-');
        let clean = raw_clean.trim_start_matches('-');
        if clean.is_empty() {
            return false;
        }
        let clean_bytes = clean.as_bytes();
        let req_flag: u16 = if is_word { 0x0002 } else { 0x0001 };

        if !self.sorted_overlay.is_empty() {
            let start_idx = self
                .sorted_overlay
                .partition_point(|(k, _)| k.as_bytes() <= clean_bytes);
            for (k, list) in &self.sorted_overlay[start_idx..] {
                let kb = k.as_bytes();
                if !kb.starts_with(clean_bytes) {
                    break;
                }
                if kb.len() > clean_bytes.len()
                    && kb[clean_bytes.len()].is_ascii_alphabetic()
                    && list.iter().any(|e| {
                        let clen = e.text.chars().count();
                        if is_word {
                            clen >= 2
                        } else {
                            clen == 1
                        }
                    })
                {
                    return true;
                }
            }
        }

        if let Some(mmap) = self.get_mmap() {
            let mut idx = mmap.lower_bound_key(clean_bytes);
            let total = mmap.num_keys as usize;
            while idx < total {
                let (k_bytes, flags, _, _) = mmap.key_record(idx);
                if !k_bytes.starts_with(clean_bytes) {
                    break;
                }
                if k_bytes.len() > clean_bytes.len()
                    && k_bytes[clean_bytes.len()].is_ascii_alphabetic()
                    && (flags & req_flag) != 0
                {
                    return true;
                }
                idx += 1;
            }
        }
        false
    }

    /// 萬用字元查詢（`?`、`*`）：依交接規範 3.2.3 限制在萬用字元之前的固定前綴區間內掃描，並固定次排序鍵
    pub fn lookup_wildcard(&self, pattern: &str) -> Vec<LongkanEntry> {
        let pat_lower = pattern.trim().to_ascii_lowercase();
        let pat_bytes = pat_lower.as_bytes();
        let prefix_len = pat_bytes
            .iter()
            .position(|&b| b == b'?' || b == b'*')
            .unwrap_or(pat_bytes.len());
        let prefix_bytes = &pat_bytes[..prefix_len];

        let mut hits = Vec::new();
        if !self.sorted_overlay.is_empty() {
            let start_idx = self
                .sorted_overlay
                .partition_point(|(k, _)| k.as_bytes() < prefix_bytes);
            for (code, entries) in &self.sorted_overlay[start_idx..] {
                if !code.as_bytes().starts_with(prefix_bytes) {
                    break;
                }
                if Self::wildcard_match_bytes(pat_bytes, code.as_bytes()) {
                    for e in entries {
                        hits.push(e.clone());
                    }
                }
            }
        }

        if let Some(mmap) = self.get_mmap() {
            let mut raw_hits: Vec<(u32, usize, &str, u32, &str)> = Vec::new();
            let mut idx = mmap.lower_bound_key(prefix_bytes);
            let total = mmap.num_keys as usize;
            while idx < total {
                let (k_bytes, _, first_idx, count) = mmap.key_record(idx);
                if !k_bytes.starts_with(prefix_bytes) {
                    break;
                }
                if Self::wildcard_match_bytes(pat_bytes, k_bytes) {
                    let code_str = std::str::from_utf8(k_bytes).unwrap_or("");
                    for i in 0..count {
                        let e_idx = first_idx + i;
                        let (_, weight, text_str) = mmap.entry_meta(e_idx);
                        raw_hits.push((weight, k_bytes.len(), text_str, e_idx, code_str));
                    }
                }
                idx += 1;
            }
            raw_hits.sort_unstable_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| a.2.cmp(b.2))
            });
            for (_, _, _, e_idx, code_str) in raw_hits.into_iter().take(60) {
                hits.push(mmap.entry_record(e_idx, code_str));
            }
        }

        hits.sort_by(|a, b| {
            b.weight
                .cmp(&a.weight)
                .then_with(|| a.code.len().cmp(&b.code.len()))
                .then_with(|| a.text.cmp(&b.text))
        });
        let mut dedup = Vec::new();
        let mut seen = HashSet::new();
        for e in hits {
            if seen.insert(e.text.clone()) {
                dedup.push(e);
                if dedup.len() >= 45 {
                    break;
                }
            }
        }
        dedup
    }

    #[inline]
    fn wildcard_match_bytes(pat: &[u8], text: &[u8]) -> bool {
        if pat.is_empty() {
            return text.is_empty();
        }
        if pat[0] == b'*' {
            return Self::wildcard_match_bytes(&pat[1..], text)
                || (!text.is_empty() && Self::wildcard_match_bytes(pat, &text[1..]));
        }
        if !text.is_empty() && (pat[0] == b'?' || pat[0] == text[0]) {
            return Self::wildcard_match_bytes(&pat[1..], &text[1..]);
        }
        false
    }

    pub fn reverse_lookup_char(&self, ch: char) -> Option<LongkanReverseEntry> {
        if let Some(rev) = self.char_reverse.get(&ch) {
            return Some(rev.clone());
        }
        if let Some(mmap) = self.get_mmap() {
            return mmap.reverse_lookup_char(ch);
        }
        None
    }

    pub fn reverse_lookup(&self, ch: char) -> Option<LongkanReverseEntry> {
        self.reverse_lookup_char(ch)
    }

    pub fn lookup_compose_candidates(&self, code: &str, limit: usize) -> Vec<String> {
        self.lookup(code)
            .into_iter()
            .take(limit)
            .map(|e| e.text)
            .collect()
    }

    pub fn lookup_phrase_headcode(
        &self,
        code: &str,
        lexicon: &crate::lexicon::LexiconManager,
    ) -> Vec<String> {
        let lower = code.trim().to_ascii_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        if chars.len() < 2 || chars.len() > 4 {
            return Vec::new();
        }
        if chars.iter().any(|&c| !c.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let mmap_opt = self.get_mmap();
        let mut matched: Vec<(String, u32)> = Vec::new();
        for entry in &lexicon.entries {
            let word_chars: Vec<char> = entry.word.chars().collect();
            if word_chars.len() != chars.len() {
                continue;
            }
            let mut ok = true;
            for (&wc, &h) in word_chars.iter().zip(chars.iter()) {
                let starts = if let Some(rev) = self.char_reverse.get(&wc) {
                    rev.primary_code.starts_with(h)
                        || rev.alt_codes.iter().any(|ac| ac.starts_with(h))
                } else if let Some(mmap) = mmap_opt {
                    mmap.char_has_head_letter(wc, h)
                } else {
                    false
                };
                if !starts {
                    ok = false;
                    break;
                }
            }
            if ok {
                matched.push((entry.word.clone(), entry.weight));
            }
        }
        matched.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        matched.dedup_by(|a, b| a.0 == b.0);
        matched.into_iter().take(5).map(|(w, _)| w).collect()
    }

    pub fn format_char_coach(&self, ch: char) -> Option<String> {
        let rev = self.reverse_lookup_char(ch)?;
        if rev.alt_breakdowns.is_empty() {
            Some(format!(
                "{} [{}]",
                rev.primary_code.to_ascii_uppercase(),
                rev.breakdown
            ))
        } else {
            let alt = rev
                .alt_breakdowns
                .iter()
                .take(2)
                .map(|(code, bdesc)| {
                    if bdesc.is_empty() {
                        code.to_ascii_uppercase()
                    } else {
                        format!("{}={}", code.to_ascii_uppercase(), bdesc)
                    }
                })
                .collect::<Vec<_>>()
                .join(" / ");
            Some(format!(
                "{} [{}] (相容推論:{})",
                rev.primary_code.to_ascii_uppercase(),
                rev.breakdown,
                alt
            ))
        }
    }

    pub fn format_text_coach(&self, text: &str) -> String {
        let mut parts = Vec::new();
        for ch in text.chars() {
            if let Some(info) = self.format_char_coach(ch) {
                parts.push(format!("{}: {}", ch, info));
            }
        }
        parts.join(" ｜ ")
    }
}

/// 將標準 `longkan.tsv` 明文碼表編譯為 `RLKB0001` 唯讀記憶體映射二進位檔 (`longkan.bin`)，
/// 並於 64-byte 標頭偏移 `48..56` 嵌入來源 TSV 之 64-bit 指紋（完全不依賴 `Unihan.zip` 或 Python）。
pub fn compile_tsv_to_bin(tsv_path: &Path, bin_path: &Path) -> io::Result<usize> {
    let raw_bytes = std::fs::read(tsv_path)?;
    let source_fingerprint = compute_tsv_fingerprint_from_slice(&raw_bytes);
    let content = std::str::from_utf8(&raw_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // 1. 解析 TSV 並依 (code, text) 去重保留最高權重，同時建構單字反查索引
    let mut dedup_map: HashMap<(String, String), (u32, String, String)> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split('\t');
        let Some(code_raw) = parts.next() else { continue };
        let Some(text_raw) = parts.next() else { continue };
        let Some(wt_raw) = parts.next() else { continue };
        let c_lower = code_raw.trim().to_ascii_lowercase();
        let t_clean = text_raw.trim().to_string();
        if c_lower.is_empty() || t_clean.is_empty() {
            continue;
        }
        let wt = wt_raw.trim().parse::<u32>().unwrap_or(500);
        let k_clean = parts.next().unwrap_or("rad_phon").trim().to_string();
        let b_clean = parts.next().unwrap_or("").trim().to_string();

        let key = (c_lower, t_clean);
        match dedup_map.get_mut(&key) {
            Some(existing) => {
                if wt > existing.0 {
                    existing.0 = wt;
                    existing.1 = k_clean;
                    if !b_clean.is_empty() {
                        existing.2 = b_clean;
                    }
                }
            }
            None => {
                dedup_map.insert(key, (wt, k_clean, b_clean));
            }
        }
    }

    let mut sorted_rows: Vec<((String, String), (u32, String, String))> =
        dedup_map.into_iter().collect();
    sorted_rows.sort_by(|a, b| {
        a.0 .0
            .as_bytes()
            .cmp(b.0 .0.as_bytes())
            .then_with(|| b.1 .0.cmp(&a.1 .0))
            .then_with(|| a.0 .1.as_bytes().cmp(b.0 .1.as_bytes()))
    });

    let mut key_groups: HashMap<String, Vec<(String, u32, String, String)>> = HashMap::new();
    let mut char_rev: HashMap<char, LongkanReverseEntry> = HashMap::new();

    for ((c_lower, t_clean), (wt, k_clean, b_clean)) in &sorted_rows {
        key_groups.entry(c_lower.clone()).or_default().push((
            t_clean.clone(),
            *wt,
            k_clean.clone(),
            b_clean.clone(),
        ));

        if t_clean.chars().count() == 1 {
            if let Some(ch) = t_clean.chars().next() {
                let is_pure_4alpha =
                    c_lower.len() == 4 && c_lower.chars().all(|c| c.is_ascii_alphabetic());
                let score = *wt
                    + if k_clean == "curated" { 1000 } else { 0 }
                    + if is_pure_4alpha { 250 } else { 0 };
                let rev = char_rev.entry(ch).or_insert_with(|| LongkanReverseEntry {
                    primary_code: c_lower.clone(),
                    primary_score: score,
                    alt_codes: Vec::new(),
                    alt_breakdowns: Vec::new(),
                    breakdown: b_clean.clone(),
                });
                if &rev.primary_code == c_lower {
                    if score > rev.primary_score {
                        rev.primary_score = score;
                    }
                    if rev.breakdown.is_empty() && !b_clean.is_empty() {
                        rev.breakdown = b_clean.clone();
                    }
                } else if score > rev.primary_score {
                    let old_code = rev.primary_code.clone();
                    let old_bdesc = rev.breakdown.clone();
                    rev.alt_codes.retain(|x| x != c_lower && x != &old_code);
                    rev.alt_breakdowns
                        .retain(|(x, _)| x != c_lower && x != &old_code);
                    rev.alt_codes.insert(0, old_code.clone());
                    rev.alt_breakdowns.insert(0, (old_code, old_bdesc));
                    rev.primary_code = c_lower.clone();
                    rev.primary_score = score;
                    rev.breakdown = b_clean.clone();
                } else if !rev.alt_codes.contains(c_lower) {
                    rev.alt_codes.push(c_lower.clone());
                    rev.alt_breakdowns.push((c_lower.clone(), b_clean.clone()));
                }
            }
        }
    }

    let mut hot_pool: Vec<u8> = Vec::new();
    let mut hot_offsets: HashMap<String, (u32, u16)> = HashMap::new();
    let mut intern_hot = |s: &str| -> (u32, u16) {
        if let Some(&pair) = hot_offsets.get(s) {
            return pair;
        }
        let b = s.as_bytes();
        let off = hot_pool.len() as u32;
        let len = b.len().min(u16::MAX as usize) as u16;
        hot_pool.extend_from_slice(&b[..len as usize]);
        hot_offsets.insert(s.to_string(), (off, len));
        (off, len)
    };

    let mut cold_pool: Vec<u8> = Vec::new();
    let mut cold_bd_offsets: HashMap<String, u32> = HashMap::new();
    let mut intern_cold_str = |s: &str| -> u32 {
        if s.is_empty() {
            return 0xFFFF_FFFF;
        }
        if let Some(&off) = cold_bd_offsets.get(s) {
            return off;
        }
        let b = s.as_bytes();
        let slice_len = b.len().min(65535);
        let off = cold_pool.len() as u32;
        cold_pool.extend_from_slice(&(slice_len as u16).to_le_bytes());
        cold_pool.extend_from_slice(&b[..slice_len]);
        cold_bd_offsets.insert(s.to_string(), off);
        off
    };

    let mut sorted_keys: Vec<String> = key_groups.keys().cloned().collect();
    sorted_keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

    let mut key_records_buf: Vec<u8> = Vec::with_capacity(sorted_keys.len() * 16);
    let mut entry_records_buf: Vec<u8> = Vec::with_capacity(sorted_rows.len() * 16);
    let mut total_entries: u32 = 0;

    for code in &sorted_keys {
        let items = key_groups.get_mut(code).unwrap();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_bytes().cmp(b.0.as_bytes())));
        let (c_off, c_len) = intern_hot(code);
        let mut flags: u16 = 0;
        let first_idx = total_entries;
        for (t_clean, wt, k_clean, b_clean) in items.iter() {
            let ch_len = t_clean.chars().count();
            if ch_len == 1 {
                flags |= 0x0001;
            } else if ch_len == 2 {
                flags |= 0x0002;
            } else if ch_len >= 3 {
                flags |= 0x0002 | 0x0004;
            }
            let (t_off, t_len) = intern_hot(t_clean);
            let kind_id = KIND_NAMES
                .iter()
                .position(|&k| k == k_clean.as_str())
                .unwrap_or(KIND_NAMES.len() - 1) as u8;
            let bd_off = intern_cold_str(b_clean);

            entry_records_buf.extend_from_slice(&t_off.to_le_bytes());
            entry_records_buf.extend_from_slice(&t_len.to_le_bytes());
            entry_records_buf.push(ch_len.min(255) as u8);
            entry_records_buf.push(kind_id);
            entry_records_buf.extend_from_slice(&wt.to_le_bytes());
            entry_records_buf.extend_from_slice(&bd_off.to_le_bytes());
            total_entries += 1;
        }
        key_records_buf.extend_from_slice(&c_off.to_le_bytes());
        key_records_buf.extend_from_slice(&c_len.to_le_bytes());
        key_records_buf.extend_from_slice(&flags.to_le_bytes());
        key_records_buf.extend_from_slice(&first_idx.to_le_bytes());
        key_records_buf.extend_from_slice(&(items.len() as u32).to_le_bytes());
    }

    let mut sorted_rev_chars: Vec<char> = char_rev.keys().copied().collect();
    sorted_rev_chars.sort_by_key(|&c| c as u32);
    let mut rev_records_buf: Vec<u8> = Vec::with_capacity(sorted_rev_chars.len() * 20);

    for ch in &sorted_rev_chars {
        let rev = &char_rev[ch];
        let (p_off, p_len) = intern_hot(&rev.primary_code);
        let mut head_mask: u32 = 0;
        for cand_c in std::iter::once(&rev.primary_code)
            .chain(rev.alt_breakdowns.iter().map(|(ac, _)| ac))
        {
            if let Some(b0) = cand_c.as_bytes().first() {
                if b0.is_ascii_lowercase() {
                    head_mask |= 1u32 << (b0 - b'a');
                }
            }
        }
        let cold_detail_off = cold_pool.len() as u32;
        let p_bd_bytes = rev.breakdown.as_bytes();
        let p_bd_len = p_bd_bytes.len().min(65535);
        let alt_slice = &rev.alt_breakdowns[..rev.alt_breakdowns.len().min(32)];
        cold_pool.extend_from_slice(&rev.primary_score.to_le_bytes());
        cold_pool.extend_from_slice(&(p_bd_len as u16).to_le_bytes());
        cold_pool.extend_from_slice(&(alt_slice.len() as u16).to_le_bytes());
        cold_pool.extend_from_slice(&p_bd_bytes[..p_bd_len]);
        for (ac, abd) in alt_slice {
            let ac_b = ac.as_bytes();
            let ac_len = ac_b.len().min(255);
            let abd_b = abd.as_bytes();
            let abd_len = abd_b.len().min(65535);
            cold_pool.extend_from_slice(&(ac_len as u16).to_le_bytes());
            cold_pool.extend_from_slice(&(abd_len as u16).to_le_bytes());
            cold_pool.extend_from_slice(&ac_b[..ac_len]);
            cold_pool.extend_from_slice(&abd_b[..abd_len]);
        }

        rev_records_buf.extend_from_slice(&(*ch as u32).to_le_bytes());
        rev_records_buf.extend_from_slice(&p_off.to_le_bytes());
        rev_records_buf.extend_from_slice(&p_len.to_le_bytes());
        rev_records_buf.extend_from_slice(&(alt_slice.len() as u16).to_le_bytes());
        rev_records_buf.extend_from_slice(&head_mask.to_le_bytes());
        rev_records_buf.extend_from_slice(&cold_detail_off.to_le_bytes());
    }

    fn align8(b: &mut Vec<u8>) {
        let rem = b.len() % 8;
        if rem != 0 {
            b.resize(b.len() + (8 - rem), 0);
        }
    }

    let mut out: Vec<u8> = vec![0u8; 64];
    let keys_offset = out.len() as u32;
    out.extend_from_slice(&key_records_buf);
    align8(&mut out);

    let entries_offset = out.len() as u32;
    out.extend_from_slice(&entry_records_buf);
    align8(&mut out);

    let reverse_offset = out.len() as u32;
    out.extend_from_slice(&rev_records_buf);
    align8(&mut out);

    let hot_strings_offset = out.len() as u32;
    let hot_strings_len = hot_pool.len() as u32;
    out.extend_from_slice(&hot_pool);
    align8(&mut out);

    let cold_pool_offset = out.len() as u32;
    let cold_pool_len = cold_pool.len() as u32;
    out.extend_from_slice(&cold_pool);

    out[0..8].copy_from_slice(b"RLKB0001");
    out[8..12].copy_from_slice(&(sorted_keys.len() as u32).to_le_bytes());
    out[12..16].copy_from_slice(&total_entries.to_le_bytes());
    out[16..20].copy_from_slice(&(sorted_rev_chars.len() as u32).to_le_bytes());
    out[20..24].copy_from_slice(&keys_offset.to_le_bytes());
    out[24..28].copy_from_slice(&entries_offset.to_le_bytes());
    out[28..32].copy_from_slice(&reverse_offset.to_le_bytes());
    out[32..36].copy_from_slice(&hot_strings_offset.to_le_bytes());
    out[36..40].copy_from_slice(&hot_strings_len.to_le_bytes());
    out[40..44].copy_from_slice(&cold_pool_offset.to_le_bytes());
    out[44..48].copy_from_slice(&cold_pool_len.to_le_bytes());
    out[48..56].copy_from_slice(&source_fingerprint.to_le_bytes());
    out[56..64].copy_from_slice(&0u64.to_le_bytes());

    if let Some(parent) = bin_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = bin_path.with_extension(format!("bin.tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, &out)?;
    if let Err(_) = std::fs::rename(&tmp_path, bin_path) {
        let _ = std::fs::copy(&tmp_path, bin_path);
        let _ = std::fs::remove_file(&tmp_path);
    }

    Ok(total_entries as usize)
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn test_longkan_stale_bin_detection_and_tsv_to_bin_roundtrip() {
        let tmp_dir = std::env::temp_dir().join(format!("ruhi_lkbin_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let tsv_path = tmp_dir.join("sample.tsv");
        let bin_path = tmp_dir.join("sample.bin");

        // 1. 建立初始 TSV 並以 compile_tsv_to_bin 轉為 .bin
        let tsv_v1 = "# comment\nkmfu\t鋪\t2600\tcurated\t金+甫\nhelo\t龕\t2550\tcurated\t合+龍\n";
        std::fs::write(&tsv_path, tsv_v1).unwrap();
        let count = compile_tsv_to_bin(&tsv_path, &bin_path).unwrap();
        assert_eq!(count, 2);

        // 2. 驗證指紋相符時 open_verified 成功
        let mmap_ok = LongkanMmap::open_verified(&bin_path, Some(&tsv_path))
            .expect("open_verified should succeed when TSV matches");
        assert_eq!(mmap_ok.num_entries, 2);
        drop(mmap_ok);

        // 3. 修改 TSV 但不更新同目錄 .bin -> open_verified 必須偵測到過期並拒絕使用
        let tsv_v2 = "# comment\nkmfu\t鋪\t2600\tcurated\t金+甫\nhelo\t龕\t2550\tcurated\t合+龍\nzznew\t新\t2700\tcurated\t新增測試\n";
        std::fs::write(&tsv_path, tsv_v2).unwrap();
        assert!(
            LongkanMmap::open_verified(&bin_path, Some(&tsv_path)).is_err(),
            "Stale .bin must be rejected when .tsv has been modified"
        );

        // 4. LongkanTable::load_tsv 遇過期同目錄 .bin 時，自動略過舊 .bin 並載入最新 TSV 內容
        let mut table = LongkanTable::new();
        let loaded = table.load_tsv(&tsv_path).unwrap();
        assert_eq!(loaded, 3);
        let hits = table.lookup_exact("zznew", false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "新");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
