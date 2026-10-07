use ruhi::cangjie::CangjieEngine;
use std::io::{self, BufRead, Write};
use std::path::Path;

fn print_banner() {
    println!("============================================================");
    println!("  如喜（RuhI）倉頡＋英文零切換智慧混打 - 獨立測試器");
    println!("============================================================");
    println!("  [手感哲學]");
    println!("  - 空白鍵 (Space) : 提交第一候選漢字 (如 ab -> 明)");
    println!("  - Enter 鍵       : 零切換直通出原始英文 (如 ab -> ab, git -> git)");
    println!("  - 超過 5 碼      : 自動判定為英文模式 (如 commit, github)");
    println!("  - 大寫字母       : 自動直通英文 (如 Git, API)");
    println!("  - 數字 1~9       : 選取重碼候選字");
    println!("  - 輸入 :q 退出測試器");
    println!("============================================================");
}

fn main() {
    print_banner();

    // 尋找鄰近的 cangjie.tsv
    let mut engine = CangjieEngine::new();
    let candidate_paths = [
        "data/cangjie.tsv",
        "../data/cangjie.tsv",
        "../ruhi_lexicon/data/cangjie.tsv",
        "../../ruhi_lexicon/data/cangjie.tsv",
    ];

    let mut loaded = false;
    for p_str in &candidate_paths {
        let p = Path::new(p_str);
        if p.exists() {
            if let Ok(count) = engine.table.load_tsv(p) {
                println!("[系統] 已成功載入官方正體倉頡碼表: {} (共 {} 字條)", p_str, count);
                loaded = true;
                break;
            }
        }
    }

    if !loaded {
        println!("[系統] 使用內建核心倉頡字庫運行。");
    }

    println!("\n請輸入測試指令或拼弦：");
    println!("提示：您可以直接鍵入字母，或輸入特殊代碼如 [SPACE]、[ENTER] 模擬操作。\n");

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    loop {
        print!("ruhi-cangjie> ");
        let _ = io::stdout().flush();
        line.clear();
        if reader.read_line(&mut line).is_err() || line.trim() == ":q" {
            println!("再見！");
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        // 支援逐字元模擬與特殊鍵位語法
        // 例如 "ab [SPACE]" 或 "ab [ENTER]" 或 "github [SPACE]"
        let mut sim_engine = CangjieEngine::new();
        sim_engine.table.map = engine.table.map.clone();

        let mut output = String::new();
        let mut i = 0;
        let chars: Vec<char> = trimmed.chars().collect();

        while i < chars.len() {
            if chars[i] == '[' {
                // 嘗試解析特殊鍵位
                let rest: String = chars[i..].iter().collect();
                if rest.starts_with("[SPACE]") {
                    let r = sim_engine.process_key(' ');
                    if r.action == "commit" {
                        output.push_str(&r.committed_chunk);
                    }
                    i += 7;
                    continue;
                } else if rest.starts_with("[ENTER]") {
                    let r = sim_engine.process_key('\n');
                    if r.action == "commit" {
                        output.push_str(&r.committed_chunk);
                    }
                    i += 7;
                    continue;
                } else if rest.starts_with("[BACKSPACE]") {
                    let _ = sim_engine.process_key('\x08');
                    i += 11;
                    continue;
                }
            }

            let ch = chars[i];
            if ch == ' ' {
                // 若空格後面緊接著特殊鍵位 [ENTER] 或 [SPACE]，跳過此裝飾性空格
                let rest_str: String = chars[i + 1..].iter().collect();
                if rest_str.starts_with("[ENTER]") || rest_str.starts_with("[SPACE]") {
                    i += 1;
                    continue;
                }
                let r = sim_engine.process_key(' ');
                if r.action == "commit" {
                    output.push_str(&r.committed_chunk);
                }
            } else {
                let r = sim_engine.process_key(ch);
                if r.action == "commit" {
                    output.push_str(&r.committed_chunk);
                }
            }
            i += 1;
        }

        // 若最後仍有未提交的 buffer，印出目前狀態
        if sim_engine.has_buffer() {
            println!("  ➔ 編輯區預覽: {} (候選: {:?})", sim_engine.buffer, sim_engine.candidates);
        }
        if !output.is_empty() {
            println!("  ➔ 上屏結果: 「{}」", output);
        }
        println!();
    }
}
