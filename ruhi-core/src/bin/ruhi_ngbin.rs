// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/bin/ruhi_ngbin.rs — 台灣正體統計 N-gram ARPA TSV → RLNG0001 唯讀記憶體映射檔編譯工具
//
// 用法：
//   cargo run --release --bin ruhi_ngbin -- <input.arpa.tsv> [output_ngram_zh_tw.bin]

use ruhi::router::ngram_lm::{compile_arpa_tsv_to_bin, NgramLanguageModel};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct SequenceScores {
    word_bi: f64,
    char_bi: f64,
    char_tri: f64,
}

impl SequenceScores {
    fn combined(&self) -> f64 {
        self.word_bi + self.char_bi + self.char_tri
    }
}

/// 計算切分詞序在詞彙二元與字元二／三元模型下之對數機率總和（base-10）。
fn score_word_sequence(lm: &NgramLanguageModel, words: &[&str]) -> SequenceScores {
    let mut word_bi = 0.0;
    let mut char_bi = 0.0;
    let mut char_tri = 0.0;
    let mut prev2: Option<&str> = None;
    let mut prev1 = "";
    for &w in words {
        word_bi += lm.score_word_bigram(prev1, w);
        let (cb, ct) = lm.score_char_boundary(prev2, prev1, w);
        char_bi += cb;
        char_tri += ct;
        prev2 = if prev1.is_empty() { None } else { Some(prev1) };
        prev1 = w;
    }
    SequenceScores {
        word_bi,
        char_bi,
        char_tri,
    }
}

/// 載入 RLNG0001 二進位模型並執行 10 組台灣正體對比詞組診斷探針。
fn run_probe(bin_path: &Path) -> ! {
    if !bin_path.is_file() {
        eprintln!("[錯誤] 找不到二進位 N-gram 模型檔：{}", bin_path.display());
        std::process::exit(1);
    }

    let t0 = Instant::now();
    let lm = match NgramLanguageModel::open(bin_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[錯誤] 開啟 {} 失敗：{}", bin_path.display(), e);
            std::process::exit(1);
        }
    };
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let size_bytes = std::fs::metadata(bin_path).map(|m| m.len()).unwrap_or(0);
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);

    println!(
        "[ruhi_ngbin --probe] 載入成功：{} ({:.2} MB, fp={:#018x}, open={:.2} ms)",
        bin_path.display(),
        size_mb,
        lm.corpus_fingerprint,
        open_ms
    );
    println!(
        "  詞彙={} | 字元={} | 詞二元={} | 字二元={} | 字三元={}",
        lm.vocab_count,
        lm.char_vocab_count,
        lm.word_bi_count,
        lm.char_bi_count,
        lm.char_tri_count
    );

    let pairs: &[(&[&str], &[&str])] = &[
        (&["美", "是", "主觀"], &["美式", "主觀"]),
        (&["全力", "建置"], &["權利", "建置"]),
        (&["形成", "群集"], &["行程", "群集"]),
        (&["已", "被", "扭曲"], &["椅背", "扭曲"]),
        (&["本地", "型"], &["本", "地形"]),
        (&["花", "半", "毛"], &["花瓣", "毛"]),
        (&["這", "不是", "我"], &["這部", "事", "我"]),
        (&["已", "到貨"], &["已到", "或"]),
        (&["檔案", "建立"], &["當", "按鍵", "力"]),
        (&["與", "意識"], &["語意", "是"]),
    ];

    let mut pass_combined = 0usize;
    let mut pass_char = 0usize;
    println!();
    for (idx, (good, bad)) in pairs.iter().enumerate() {
        let sg = score_word_sequence(&lm, good);
        let sb = score_word_sequence(&lm, bad);
        let delta_comb = sg.combined() - sb.combined();
        let delta_char = (sg.char_bi + sg.char_tri) - (sb.char_bi + sb.char_tri);
        let delta_wb = sg.word_bi - sb.word_bi;
        if delta_comb > 0.0 {
            pass_combined += 1;
        }
        if delta_char > 0.0 {
            pass_char += 1;
        }
        let tag = if delta_comb > 0.0 { "PASS" } else { "BACK" };
        println!(
            "  [{:2}/10] {:4} | {} vs {}",
            idx + 1,
            tag,
            good.join("+"),
            bad.join("+")
        );
        println!(
            "         good: wb={:7.3} cb={:7.3} ct={:7.3} sum={:8.3}",
            sg.word_bi,
            sg.char_bi,
            sg.char_tri,
            sg.combined()
        );
        println!(
            "         bad : wb={:7.3} cb={:7.3} ct={:7.3} sum={:8.3} | d_wb={:+.3} d_char={:+.3} d_sum={:+.3}",
            sb.word_bi,
            sb.char_bi,
            sb.char_tri,
            sb.combined(),
            delta_wb,
            delta_char,
            delta_comb
        );
    }
    println!(
        "\n[probe 摘要] combined_win={}/10 | char_only_win={}/10",
        pass_combined, pass_char
    );
    std::process::exit(0);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法：ruhi_ngbin <input.arpa.tsv> [output.bin]");
        eprintln!("      ruhi_ngbin --probe <ngram_zh_tw.bin>");
        std::process::exit(1);
    }

    if args[0] == "--probe" {
        let Some(bin_arg) = args.get(1) else {
            eprintln!("用法：ruhi_ngbin --probe <ngram_zh_tw.bin>");
            std::process::exit(1);
        };
        run_probe(Path::new(bin_arg));
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
