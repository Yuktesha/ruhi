// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/bin/ruhi_reranker.rs — 如喜 (RuhI) v1.0 輕量神經重排序單例本機 IPC 守護服務
//
// 支援模式：
// - 預設：以單例 Mutex 與 Named Pipe `\\.\pipe\RuhI_NeuralReranker_v1` 常駐提供前綴樹共用 KV-Cache 批次重排；
// - `--bench`：於本機執行 10 組實境 Top-16 候選前綴樹重排並輸出延遲統計（p50 / p95 / max ms）；
// - `--once`：僅服務 1 筆請求後安全退出（供自動化 Named Pipe 端到端測試驗證）。

use ruhi::router::neural_reranker::{
    decode_request, encode_response, resolve_sibling_r2l_model_path,
    score_candidates_bidirectional, select_candidates_with_trie_budget, NeuralRerankerConfig,
    DEFAULT_PIPE_NAME, MAX_IPC_FRAME_BYTES,
};
use ruhi::router::rlnm::{RlnmModel, RlnmThreadPool};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn resolve_default_model_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p1 = dir.join("data").join("rlnm_zh_tw.bin");
            if p1.is_file() {
                return Some(p1);
            }
            let p2 = dir.join("rlnm_zh_tw.bin");
            if p2.is_file() {
                return Some(p2);
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for cand in [
        cwd.join("ruhi_lexicon").join("data").join("rlnm_zh_tw.bin"),
        cwd.join("data").join("rlnm_zh_tw.bin"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ruhi_lexicon")
            .join("data")
            .join("rlnm_zh_tw.bin"),
    ] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn run_benchmark(
    model: &RlnmModel,
    r2l_model: Option<&RlnmModel>,
    pool: &RlnmThreadPool,
) {
    let mut cache = model.alloc_kv_cache();
    let mut r2l_cache = r2l_model.map(|m| m.alloc_kv_cache());
    let bench_cases: Vec<(&str, Vec<&str>)> = vec![
        (
            "",
            vec![
                "再補上三句", "在補上三句", "再捕上三句", "在捕上三句",
                "再卜上三句", "在卜上三句", "再補尚三句", "在補尚三句",
                "再補上三具", "在補上三具", "再補上三巨", "在補上三巨",
                "再補上三聚", "在補上三聚", "再補上散句", "在補上散句",
            ],
        ),
        (
            "今天",
            vec![
                "已取消南下旅程", "以取消南下旅程", "乙取消南下旅程", "矣取消南下旅程",
                "椅取消南下旅程", "蟻取消南下旅程", "已取消難下旅程", "以取消難下旅程",
                "已取銷南下旅程", "以取銷南下旅程", "已區曉南下旅程", "以區曉南下旅程",
                "已取消南下旅城", "以取消南下旅城", "已取消南夏旅程", "以取消南夏旅程",
            ],
        ),
        (
            "",
            vec![
                "滑鼠點一下變兩下該怎麼修復", "滑鼠點一下便兩下該怎麼修復",
                "滑鼠點一下遍兩下該怎麼修復", "滑鼠點一下辨兩下該怎麼修復",
                "滑鼠點一下辯兩下該怎麼修復", "滑鼠點一下弁兩下該怎麼修復",
                "滑鼠點一下變兩下賅怎麼修復", "滑鼠點一下便兩下賅怎麼修復",
                "滑鼠點一下變兩夏該怎麼修復", "滑鼠點一下便兩夏該怎麼修復",
                "滑鼠典一下變兩下該怎麼修復", "滑鼠典一下便兩下該怎麼修復",
                "畫書點一下變兩下該怎麼修復", "畫書點一下便兩下該怎麼修復",
                "滑鼠點一夏變兩下該怎麼修復", "滑鼠點一夏便兩下該怎麼修復",
            ],
        ),
        (
            "此刻視力模糊",
            vec![
                "難以辨識滑鼠底下", "難以便是滑鼠底下", "難以變是滑鼠底下", "難以辨是滑鼠底下",
                "南以便是滑鼠底下", "男以便是滑鼠底下", "難以遍是滑鼠底下", "難以辯是滑鼠底下",
                "楠以便是滑鼠底下", "難以變市滑鼠底下", "難以辨室滑鼠底下", "難以辨世滑鼠底下",
                "難以辨式滑鼠底下", "難以辨示滑鼠底下", "難以變試滑鼠底下", "難以編是滑鼠底下",
            ],
        ),
        (
            "目前",
            vec![
                "結果您這裡又更新模型", "結果您蔗裡又更新模型", "結果您浙裡又更新模型", "結果您鷓裡又更新模型",
                "結果您這裡又庚新模型", "結果您蔗裡又庚新模型", "結果您這裡又跟新模型", "結果您這理又更新模型",
                "結果您這裡佑更新模型", "結果您這里又更新模型", "結果您這裡又更新魔型", "結果您這禮又更新模型",
                "結果寧這裡又更新模型", "結果您這裏又更新模型", "結果您這裡又更新模行", "結果您這裡幼更新模型",
            ],
        ),
        (
            "",
            vec![
                "此刻視力模糊難以便是滑鼠底下大概只有",
                "此刻視力模糊難以辨識滑鼠底下大概只有",
                "此刻視力模糊難以變是滑鼠底下大概只有",
                "此刻視力模糊難以辨是滑鼠底下大概只有",
                "此刻視力模糊難以遍是滑鼠底下大概只有",
                "此刻視力模糊難以辯是滑鼠底下大概只有",
                "此刻視力模糊難以便市滑鼠底下大概只有",
                "此刻視力模糊難以辨室滑鼠底下大概只有",
                "此刻視力模糊難以辨世滑鼠底下大概只有",
                "此刻視力模糊難以辨式滑鼠底下大概只有",
                "此刻視力模糊難以辨示滑鼠底下大概只有",
                "此刻視力模糊難以便試滑鼠底下大概只有",
                "此刻視力模糊難以編是滑鼠底下大概只有",
                "此刻事例模糊難以便是滑鼠底下大概只有",
                "此刻事理模糊難以辨識滑鼠底下大概只有",
                "此刻視力魔胡難以辨識滑鼠底下大概只有",
                "此刻視力磨糊難以辨識滑鼠底下大概只有",
                "此刻視力模糊難以辨識化屬底下大概只有",
                "此刻視力模糊難以辨識畫書底下大概只有",
                "此刻視力模糊難以辨識滑鼠底夏大概只有",
                "此刻視力模糊難以辨識滑鼠低下大概只有",
                "此刻視力模糊難以辨識滑鼠底下大改只有",
                "此刻視力模糊難以辨識滑鼠底下大該只有",
                "此刻視力模糊難以辨識滑鼠底下大蓋只有",
                "此刻視力模糊難以辨識滑鼠底下大概止有",
                "此刻視力模糊難以辨識滑鼠底下大概只有",
                "此刻視力模糊難以便是化屬底下大概只有",
                "此刻視力模糊難以辨識滑鼠低夏大概只有",
                "此刻視力模糊難以辨識滑鼠地夏大概只有",
                "此課視力模糊難以辨識滑鼠底下大概只有",
                "刺客視力模糊難以辨識滑鼠底下大概只有",
                "此刻試力模糊難以辨識滑鼠底下大概只有",
            ],
        ),
        (
            "",
            vec![
                "今天下午會議結束之後我們一起去吃晚餐再回家吧",
                "今天下午會議結束之後我們一起去吃晚餐在回家吧",
                "今天下午會議結束之後我們一起取吃晚餐再回家吧",
                "今天下午會議結束之厚我們一起去吃晚餐再回家吧",
                "今天下午會議接束之後我們一起去吃晚餐再回家吧",
                "今天下午會義結束之後我們一起去吃晚餐再回家吧",
                "今天下霧會議結束之後我們一起去吃晚餐再回家吧",
                "今天下午會議結束之後我門一起去吃晚餐再回家吧",
                "今天下午會議結束之後我們一啟去吃晚餐再回家吧",
                "今天下午會議結束之後我們一起去吃碗餐再回家吧",
                "今天下午會議結束之後我們一起去吃晚參再回家吧",
                "今天下午會議結束之後我們一起去吃晚餐再回佳吧",
                "今天下午會議結束之後我們一起去吃晚餐再回加吧",
                "今天下午會議結束之後我們一起去吃晚餐再回嘉吧",
                "今天下午會議結束之後我們一起去吃晚餐在回佳吧",
                "今天下午會議結束之後我們一起去持晚餐再回家吧",
                "今天下午會議界束之後我們一起去吃晚餐再回家吧",
                "今天下午匯議結束之後我們一起去吃晚餐再回家吧",
                "今天下午繪議結束之後我們一起去吃晚餐再回家吧",
                "今天下午會議結束隻後我們一起去吃晚餐再回家吧",
                "今天下午會議結束知後我們一起去吃晚餐再回家吧",
                "今天下午會議結束枝後我們一起去吃晚餐再回家吧",
                "今天下午會議結束之後我們以起去吃晚餐再回家吧",
                "今天下午會議結束之後我們已起去吃晚餐再回家吧",
                "今天下午會議結束之後我們一起去吃晩餐再回家吧",
                "今天下午會議結束之後我們一起去吃晚餐再會家吧",
                "今天下午會議結束之後我們一起去吃晚餐再灰家吧",
                "今天下午會議結束之後我們一起去吃晚餐再回家巴",
                "今天下午會議結束之後我們一起去吃晚餐再回家八",
                "今天下午會議結束之後我們一起去吃晚餐再回家把",
                "今天下物會議結束之後我們一起去吃晚餐再回家吧",
                "今天夏午會議結束之後我們一起去吃晚餐再回家吧",
            ],
        ),
    ];

    // 暖機 1 次確保 mmap 與快取頁完成載入
    let _ = score_candidates_bidirectional(
        model,
        r2l_model,
        bench_cases[0].0,
        &bench_cases[0].1,
        &mut cache,
        r2l_cache.as_mut(),
        Some(pool),
    );
    let max_trie_nodes = NeuralRerankerConfig::default().max_trie_nodes;

    let mut latencies_ms = Vec::new();
    for round in 0..2 {
        for (idx, (pref, cands)) in bench_cases.iter().enumerate() {
            let (kept_indices, trie_nodes) =
                select_candidates_with_trie_budget(pref, cands, max_trie_nodes);
            let active_cands: Vec<&str> = kept_indices.iter().map(|&i| cands[i]).collect();
            let t0 = Instant::now();
            let scores = score_candidates_bidirectional(
                model,
                r2l_model,
                pref,
                &active_cands,
                &mut cache,
                r2l_cache.as_mut(),
                Some(pool),
            );
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            latencies_ms.push(ms);
            if round == 0 {
                println!(
                    "  [bench #{}] kept={}/{} trie_nodes={} | top2=({:.3}, {:.3}) | {:.2} ms",
                    idx + 1,
                    active_cands.len(),
                    cands.len(),
                    trie_nodes,
                    scores[0],
                    scores[1],
                    ms
                );
            }
        }
    }
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = latencies_ms.len();
    let p50 = latencies_ms[n / 2];
    let p95 = latencies_ms[((n as f64 * 0.95).ceil() as usize).saturating_sub(1).min(n - 1)];
    let max_ms = latencies_ms[n - 1];
    let total_workers = pool.num_workers();
    let mode_str = if r2l_model.is_some() {
        "bidirectional"
    } else {
        "l2r-only"
    };
    println!(
        "[ruhi_reranker --bench] mode={} | workers={} | n={} | p50={:.2} ms | p95={:.2} ms | max={:.2} ms",
        mode_str,
        total_workers,
        n,
        p50,
        p95,
        max_ms
    );

    // [bench #8 prefix-kv-cache] 量測 36 字跨句前綴首次失誤 (Cold Miss) vs. 命中快取 (Warm Hit)
    let cross_prefix = "這台新買的伺服器效能非常穩定而且散熱表現也很優秀，所以";
    let cross_cands: Vec<&str> = vec![
        "它每天都可以穩定運作",
        "他每天都可以穩定運作",
        "她每天都可以穩定運作",
        "牠每天都可以穩定運作",
        "它每天都可靠穩定運作",
        "他每天都可靠穩定運作",
        "她每天都可靠穩定運作",
        "它每天都可以文定運作",
        "他每天都可以文定運作",
        "它每天逗可以穩定運作",
        "他每天逗可以穩定運作",
        "她每天逗可以穩定運作",
        "牠每天逗可以穩定運作",
        "它沒天都可以穩定運作",
        "他沒天都可以穩定運作",
        "它每天都可以穩錠運作",
    ];
    let mut kv_test_cache = model.alloc_kv_cache();
    let mut r2l_test_cache = r2l_model.map(|m| m.alloc_kv_cache());
    let t_cold = Instant::now();
    let sc_cold = score_candidates_bidirectional(
        model,
        r2l_model,
        cross_prefix,
        &cross_cands,
        &mut kv_test_cache,
        r2l_test_cache.as_mut(),
        Some(pool),
    );
    let cold_ms = t_cold.elapsed().as_secs_f64() * 1000.0;

    let t_warm = Instant::now();
    let sc_warm = score_candidates_bidirectional(
        model,
        r2l_model,
        cross_prefix,
        &cross_cands,
        &mut kv_test_cache,
        r2l_test_cache.as_mut(),
        Some(pool),
    );
    let warm_ms = t_warm.elapsed().as_secs_f64() * 1000.0;

    let max_drift = sc_cold
        .iter()
        .zip(sc_warm.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_drift < 1e-4,
        "prefix KV cache parity drift exceeded tolerance: {}",
        max_drift
    );
    println!(
        "  [bench #8 prefix-kv-cache] prefix_chars={} | cold_miss={:.2} ms | warm_hit={:.2} ms | speedup={:.2}x | max_drift={:.2e} | 它({:.3}) vs 他({:.3})",
        cross_prefix.chars().count(),
        cold_ms,
        warm_ms,
        cold_ms / warm_ms.max(0.001),
        max_drift,
        sc_warm[0],
        sc_warm[1]
    );
}

#[cfg(target_os = "windows")]
mod win_server {
    use super::*;
    use std::ffi::c_void;

    const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
    const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
    const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
    const PIPE_WAIT: u32 = 0x0000_0000;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const INVALID_HANDLE_VALUE: isize = -1;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const SDDL_REVISION_1: u32 = 1;

    #[repr(C)]
    struct SecurityAttributes {
        n_length: u32,
        lp_security_descriptor: *mut c_void,
        b_inherit_handle: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lpMutexAttributes: *const c_void,
            bInitialOwner: i32,
            lpName: *const u16,
        ) -> isize;
        fn CreateNamedPipeW(
            lpName: *const u16,
            dwOpenMode: u32,
            dwPipeMode: u32,
            nMaxInstances: u32,
            nOutBufferSize: u32,
            nInBufferSize: u32,
            nDefaultTimeOut: u32,
            lpSecurityAttributes: *const SecurityAttributes,
        ) -> isize;
        fn ConnectNamedPipe(hNamedPipe: isize, lpOverlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(hNamedPipe: isize) -> i32;
        fn FlushFileBuffers(hFile: isize) -> i32;
        fn ReadFile(
            hFile: isize,
            lpBuffer: *mut u8,
            nNumberOfBytesToRead: u32,
            lpNumberOfBytesRead: *mut u32,
            lpOverlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            hFile: isize,
            lpBuffer: *const u8,
            nNumberOfBytesToWrite: u32,
            lpNumberOfBytesWritten: *mut u32,
            lpOverlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
        fn LocalFree(hMem: *mut c_void) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            StringSecurityDescriptor: *const u16,
            StringSDRevision: u32,
            SecurityDescriptor: *mut *mut c_void,
            SecurityDescriptorSize: *mut u32,
        ) -> i32;
    }

    struct SdGuard(*mut c_void);
    impl Drop for SdGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
    }

    struct HandleGuard(isize);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn read_exact_pipe(h: isize, dst: &mut [u8]) -> bool {
        let mut off = 0usize;
        while off < dst.len() {
            let mut n = 0u32;
            let ok = unsafe {
                ReadFile(
                    h,
                    dst[off..].as_mut_ptr(),
                    (dst.len() - off) as u32,
                    &mut n,
                    core::ptr::null_mut(),
                )
            };
            if ok == 0 || n == 0 {
                return false;
            }
            off += n as usize;
        }
        true
    }

    fn write_all_pipe(h: isize, src: &[u8]) -> bool {
        let mut off = 0usize;
        while off < src.len() {
            let mut n = 0u32;
            let ok = unsafe {
                WriteFile(
                    h,
                    src[off..].as_ptr(),
                    (src.len() - off) as u32,
                    &mut n,
                    core::ptr::null_mut(),
                )
            };
            if ok == 0 || n == 0 {
                return false;
            }
            off += n as usize;
        }
        unsafe {
            FlushFileBuffers(h);
        }
        true
    }

    pub fn serve_pipe(
        pipe_name: &str,
        model: &RlnmModel,
        r2l_model: Option<&RlnmModel>,
        pool: &RlnmThreadPool,
        once: bool,
    ) -> Result<(), String> {
        let _singleton_guard = if !once {
            let mut mname: Vec<u16> = "Local\\RuhI_NeuralReranker_Singleton_v1"
                .encode_utf16()
                .collect();
            mname.push(0);
            let h_mtx = unsafe { CreateMutexW(core::ptr::null(), 1, mname.as_ptr()) };
            if h_mtx == 0 {
                return Err("CreateMutexW failed".to_string());
            }
            let err = unsafe { GetLastError() };
            let g = HandleGuard(h_mtx);
            if err == ERROR_ALREADY_EXISTS {
                println!("[ruhi_reranker] 已有實例正在運行，安全結束。");
                return Ok(());
            }
            Some(g)
        } else {
            None
        };

        // 允許本機使用者與 AppContainer 低權限沙盒程序（Edge / UWP 等）連線存取
        let mut sd_ptr: *mut c_void = core::ptr::null_mut();
        for sddl in [
            "D:(A;;GRGW;;;AU)(A;;GRGW;;;AC)S:(ML;;NW;;;LW)",
            "D:(A;;GRGW;;;AU)",
        ] {
            let mut ws: Vec<u16> = sddl.encode_utf16().collect();
            ws.push(0);
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    ws.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd_ptr,
                    core::ptr::null_mut(),
                )
            } != 0
            {
                break;
            }
        }
        let _sd_guard = SdGuard(sd_ptr);
        let sa = SecurityAttributes {
            n_length: std::mem::size_of::<SecurityAttributes>() as u32,
            lp_security_descriptor: sd_ptr,
            b_inherit_handle: 0,
        };
        let sa_ptr = if sd_ptr.is_null() {
            core::ptr::null()
        } else {
            &sa as *const SecurityAttributes
        };

        let mut wide_pipe: Vec<u16> = pipe_name.encode_utf16().collect();
        wide_pipe.push(0);
        let mut cache = model.alloc_kv_cache();
        let mut r2l_cache = r2l_model.map(|m| m.alloc_kv_cache());
        // 預熱 BatchWorkSpace 與 mmap 權重分頁，避免首次按鍵推論觸發冷頁延遲
        let _ = score_candidates_bidirectional(
            model,
            r2l_model,
            "",
            &["已取消南下行程", "以取消南下行程"],
            &mut cache,
            r2l_cache.as_mut(),
            Some(pool),
        );

        let total_workers = pool.num_workers();
        let mode_str = if r2l_model.is_some() {
            "bidirectional"
        } else {
            "l2r-only"
        };
        println!(
            "[ruhi_reranker] 已就緒: pipe={}, mode={}, workers={}, vocab={}",
            pipe_name,
            mode_str,
            total_workers,
            model.cfg.vocab_size
        );

        let h_pipe = unsafe {
            CreateNamedPipeW(
                wide_pipe.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                sa_ptr,
            )
        };
        if h_pipe == INVALID_HANDLE_VALUE {
            return Err(format!("CreateNamedPipeW failed: err={}", unsafe { GetLastError() }));
        }
        let _pipe_guard = HandleGuard(h_pipe);

        loop {
            let connected = unsafe { ConnectNamedPipe(h_pipe, core::ptr::null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            if connected {
                let mut len_buf = [0u8; 4];
                if read_exact_pipe(h_pipe, &mut len_buf) {
                    let plen = u32::from_le_bytes(len_buf) as usize;
                    if plen > 0 && plen <= MAX_IPC_FRAME_BYTES {
                        let mut payload = vec![0u8; plen];
                        if read_exact_pipe(h_pipe, &mut payload) {
                            if let Some((req_id, _tmo, prefix, cands)) = decode_request(&payload) {
                                let cand_refs: Vec<&str> =
                                    cands.iter().map(|s| s.as_str()).collect();
                                let scores = score_candidates_bidirectional(
                                    model,
                                    r2l_model,
                                    &prefix,
                                    &cand_refs,
                                    &mut cache,
                                    r2l_cache.as_mut(),
                                    Some(pool),
                                );
                                let resp = encode_response(req_id, 0, &scores);
                                let _ = write_all_pipe(h_pipe, &resp);
                            }
                        }
                    }
                }
                unsafe {
                    DisconnectNamedPipe(h_pipe);
                }
                if once {
                    break;
                }
            }
        }
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut model_path: Option<PathBuf> = None;
    let mut model_r2l_path: Option<PathBuf> = None;
    let mut no_r2l = false;
    let mut pipe_name = DEFAULT_PIPE_NAME.to_string();
    let mut threads: Option<usize> = None;
    let mut do_bench = false;
    let mut do_once = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--model" if i + 1 < args.len() => {
                model_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--model-r2l" if i + 1 < args.len() => {
                model_r2l_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--no-r2l" => {
                no_r2l = true;
                i += 1;
            }
            "--pipe" if i + 1 < args.len() => {
                pipe_name = args[i + 1].clone();
                i += 2;
            }
            "--threads" if i + 1 < args.len() => {
                threads = args[i + 1].parse::<usize>().ok();
                i += 2;
            }
            "--bench" => {
                do_bench = true;
                i += 1;
            }
            "--once" => {
                do_once = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    let resolved_model = model_path.or_else(resolve_default_model_path).unwrap_or_else(|| {
        eprintln!("[錯誤] 找不到 rlnm_zh_tw.bin，請透過 --model <path> 指定模型路徑。");
        std::process::exit(1);
    });

    let t0 = Instant::now();
    let model = RlnmModel::open(Path::new(&resolved_model)).unwrap_or_else(|e| {
        eprintln!("[錯誤] 無法載入 RLNM0001 模型 {}: {}", resolved_model.display(), e);
        std::process::exit(1);
    });
    println!(
        "[ruhi_reranker] 載入模型 {} ({:.2} MB) 耗時 {:.2} ms",
        resolved_model.display(),
        std::fs::metadata(&resolved_model).map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0),
        t0.elapsed().as_secs_f64() * 1000.0
    );

    let resolved_r2l = if no_r2l {
        None
    } else {
        model_r2l_path.or_else(|| resolve_sibling_r2l_model_path(&resolved_model))
    };
    let r2l_model = resolved_r2l.as_ref().map(|r2l_p| {
        let tr = Instant::now();
        let m = RlnmModel::open(r2l_p).unwrap_or_else(|e| {
            eprintln!("[錯誤] 無法載入 R2L RLNM0001 模型 {}: {}", r2l_p.display(), e);
            std::process::exit(1);
        });
        println!(
            "[ruhi_reranker] 載入 R2L 伴隨模型 {} ({:.2} MB) 耗時 {:.2} ms",
            r2l_p.display(),
            std::fs::metadata(r2l_p).map(|md| md.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0),
            tr.elapsed().as_secs_f64() * 1000.0
        );
        m
    });

    let default_threads = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(2, 8))
        .unwrap_or(4);
    let total_workers = threads.unwrap_or(default_threads).max(1);
    let pool = RlnmThreadPool::new(total_workers);

    if do_bench {
        run_benchmark(&model, r2l_model.as_ref(), &pool);
        return;
    }

    #[cfg(target_os = "windows")]
    if let Err(e) = win_server::serve_pipe(
        &pipe_name,
        &model,
        r2l_model.as_ref(),
        &pool,
        do_once,
    ) {
        eprintln!("[錯誤] Named Pipe 服務失敗: {}", e);
        std::process::exit(1);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pipe_name, do_once);
        eprintln!("[提示] 目前 Named Pipe 守護服務僅支援 Windows；可使用 --bench 執行本機推論測試。");
    }
}
