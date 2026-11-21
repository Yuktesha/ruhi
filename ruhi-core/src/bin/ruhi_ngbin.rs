// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/bin/ruhi_ngbin.rs — 台灣正體統計 N-gram ARPA TSV → RLNG0001 唯讀記憶體映射檔編譯工具
//
// 用法：
//   cargo run --release --bin ruhi_ngbin -- <input.arpa.tsv> [output_ngram_zh_tw.bin]

use ruhi::router::ngram_lm::compile_arpa_tsv_to_bin;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：ruhi_ngbin <input.arpa.tsv> [output.bin]");
        std::process::exit(1);
    }

    let tsv_path = PathBuf::from(&args[0]);
    if !tsv_path.is_file() {
        eprintln!("[錯誤] 找不到來源 N-gram TSV 檔案：{}", tsv_path.display());
        std::process::exit(1);
    }

    let bin_path = if let Some(second) = args.get(1) {
        PathBuf::from(second)
    } else {
        tsv_path.with_extension("bin")
    };

    let t0 = Instant::now();
    let stats = match compile_arpa_tsv_to_bin(Path::new(&tsv_path), Path::new(&bin_path)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[錯誤] 編譯 {} -> {} 失敗：{}",
                tsv_path.display(),
                bin_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let size_mb = stats.bytes_written as f64 / (1024.0 * 1024.0);
    println!(
        "[ruhi_ngbin] 編譯完成：{} ({:.2} MB, fp={:#018x}, {:.1} ms)",
        bin_path.display(),
        size_mb,
        stats.corpus_fingerprint,
        elapsed_ms
    );
    println!(
        "  詞彙={} | 字元={} | 詞二元={} | 字二元={} | 字三元={} | 略過缺詞={} | 重複覆寫={}",
        stats.vocab_count,
        stats.char_vocab_count,
        stats.word_bi_count,
        stats.char_bi_count,
        stats.char_tri_count,
        stats.skipped_missing_vocab,
        stats.duplicate_entries_overwritten
    );
}
