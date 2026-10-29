// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/bin/ruhi_lkbin.rs — 龍龕形音碼表 TSV → RLKB0001 二進位記憶體映射檔獨立轉換工具
//
// 用法：
//   cargo run --release --bin ruhi_lkbin -- [ruhi_lexicon/data/longkan.tsv] [ruhi_lexicon/data/longkan.bin]
// 完全不依賴 Unihan.zip 或 Python，任何人複製儲存庫後皆可由正本 longkan.tsv 一鍵產出含指紋驗證之 longkan.bin。

use ruhi::longkan::table::{compile_tsv_to_bin, compute_tsv_fingerprint};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn resolve_default_tsv() -> PathBuf {
    let candidates = [
        PathBuf::from("ruhi_lexicon/data/longkan.tsv"),
        PathBuf::from("../ruhi_lexicon/data/longkan.tsv"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ruhi_lexicon/data/longkan.tsv"),
    ];
    for c in &candidates {
        if c.is_file() {
            return c.clone();
        }
    }
    candidates[0].clone()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tsv_path = if let Some(first) = args.first() {
        PathBuf::from(first)
    } else {
        resolve_default_tsv()
    };

    if !tsv_path.is_file() {
        eprintln!(
            "[錯誤] 找不到來源 TSV 碼表：{}",
            tsv_path.display()
        );
        std::process::exit(1);
    }

    let bin_path = if let Some(second) = args.get(1) {
        PathBuf::from(second)
    } else {
        tsv_path.with_extension("bin")
    };

    let t0 = Instant::now();
    let count = match compile_tsv_to_bin(Path::new(&tsv_path), Path::new(&bin_path)) {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "[錯誤] 轉換 {} -> {} 失敗：{}",
                tsv_path.display(),
                bin_path.display(),
                e
            );
            std::process::exit(1);
        }
    };
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let fp = compute_tsv_fingerprint(&tsv_path).unwrap_or(0);
    let size_mb = std::fs::metadata(&bin_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    println!(
        "[ruhi_lkbin] 轉換完成：{} 筆詞條 -> {} ({:.2} MB, tsv_fp={:#018x}, {:.1} ms)",
        count,
        bin_path.display(),
        size_mb,
        fp,
        elapsed_ms
    );
}
