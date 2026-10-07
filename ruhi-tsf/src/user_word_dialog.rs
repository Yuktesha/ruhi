/// RuhI (如喜) Windows 11 TSF - 使用者自建詞與厭惡抑制詞對話框與持久化模組
/// ruhi-tsf/src/user_word_dialog.rs

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use windows_core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Controls::*;
use crate::service::RuhiPaths;
use crate::logger;

/// 顯示自建詞確認或抑制確認對話框
/// 回傳 (是否確認, 是否勾選不再詢問)
pub fn show_confirm_dialog(
    title: &str,
    instruction: &str,
    content: &str,
    verification_text: &str,
) -> (bool, bool) {
    let title_h = HSTRING::from(title);
    let instr_h = HSTRING::from(instruction);
    let content_h = HSTRING::from(content);
    let verif_h = HSTRING::from(verification_text);

    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        pszWindowTitle: PCWSTR(title_h.as_ptr()),
        pszMainInstruction: PCWSTR(instr_h.as_ptr()),
        pszContent: PCWSTR(content_h.as_ptr()),
        pszVerificationText: PCWSTR(verif_h.as_ptr()),
        dwCommonButtons: TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
        ..Default::default()
    };

    let mut button_id = 0;
    let mut radio_id = 0;
    let mut verif_checked = BOOL::from(false);

    let hr = unsafe {
        TaskDialogIndirect(
            &config,
            Some(&mut button_id),
            Some(&mut radio_id),
            Some(&mut verif_checked),
        )
    };

    if hr.is_ok() {
        let confirmed = button_id == IDYES.0;
        let do_not_ask = verif_checked.as_bool();
        return (confirmed, do_not_ask);
    }

    // 備援：若宿主環境未載入 ComCtl32 v6，退回標準 MessageBoxW
    let msg = format!("{}\n\n{}", instruction, content);
    let msg_h = HSTRING::from(msg);
    let res = unsafe {
        MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(msg_h.as_ptr()),
            PCWSTR(title_h.as_ptr()),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST,
        )
    };

    (res == IDYES, false)
}

/// 計算安全中立先驗權重（落實《嚴禁單字權重打地鼠守則》）
pub fn calculate_safe_weight(word: &str) -> u32 {
    match word.chars().count() {
        1 => 600,
        2 => 750,
        3 => 820,
        _ => 880,
    }
}

/// 載入遮蔽詞清單 (deleted_dict.tsv)
pub fn load_deleted_words(paths: &RuhiPaths) -> HashSet<String> {
    let mut set = HashSet::new();
    let file_path = if paths.deleted_dict.exists() {
        paths.deleted_dict.clone()
    } else {
        paths.sandbox_dir.join("deleted_dict.tsv")
    };

    if file_path.exists() {
        if let Ok(file) = File::open(&file_path) {
            let reader = BufReader::new(file);
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split('\t').collect();
                    if let Some(w) = parts.first() {
                        if !w.is_empty() {
                            set.insert(w.to_string());
                        }
                    }
                }
            }
        }
    }
    set
}

/// 檢查是否已在自建詞庫中
pub fn is_in_user_dict(paths: &RuhiPaths, word: &str) -> bool {
    let file_path = if paths.user_dict.exists() {
        paths.user_dict.clone()
    } else {
        paths.sandbox_dir.join("user_dict.tsv")
    };

    if !file_path.exists() {
        return false;
    }

    if let Ok(file) = File::open(&file_path) {
        let reader = BufReader::new(file);
        for line_res in reader.lines() {
            if let Ok(line) = line_res {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split('\t').collect();
                if let Some(w) = parts.first() {
                    if *w == word {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 執行加詞寫入 user_dict.tsv 與 transitions_user.tsv
pub fn add_user_word_entry(
    paths: &RuhiPaths,
    word: &str,
    bpmf: &str,
    weight: u32,
    prev_word: Option<&str>,
) -> bool {
    let user_dict_file = if paths.user_dict.exists() {
        paths.user_dict.clone()
    } else {
        paths.sandbox_dir.join("user_dict.tsv")
    };

    // 1. 若該詞曾被列入 deleted_dict.tsv，自遮蔽清單中解封移除
    let del_dict_file = if paths.deleted_dict.exists() {
        paths.deleted_dict.clone()
    } else {
        paths.sandbox_dir.join("deleted_dict.tsv")
    };
    if del_dict_file.exists() {
        if let Ok(file) = File::open(&del_dict_file) {
            let reader = BufReader::new(file);
            let mut kept = Vec::new();
            for l_res in reader.lines() {
                if let Ok(l) = l_res {
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.first().map(|w| *w == word).unwrap_or(false) {
                        continue;
                    }
                    kept.push(l);
                }
            }
            if let Ok(mut f) = OpenOptions::new().write(true).truncate(true).open(&del_dict_file) {
                for l in kept {
                    let _ = writeln!(f, "{}", l);
                }
            }
        }
    }

    // 2. 寫入 user_dict.tsv
    let entry_line = format!("{}\t{}\t{}\tuser\tTSF-Selection\n", word, bpmf, weight);
    let mut file = match OpenOptions::new().create(true).append(true).open(&user_dict_file) {
        Ok(f) => f,
        Err(e) => {
            logger::log(&format!("Failed to open user_dict for writing: {:?}", e));
            return false;
        }
    };

    if let Err(e) = file.write_all(entry_line.as_bytes()) {
        logger::log(&format!("Failed to write entry to user_dict: {:?}", e));
        return false;
    }

    // 3. 寫入二階語意引力場 transitions_user.tsv（並清除既有的負向抑制屏蔽）
    if paths.transitions.exists() {
        if let Ok(file) = File::open(&paths.transitions) {
            let reader = BufReader::new(file);
            let mut kept_trans = Vec::new();
            for l_res in reader.lines() {
                if let Ok(l) = l_res {
                    let parts: Vec<&str> = l.split('\t').collect();
                    if parts.len() >= 2 && parts[1] == word && parts[0] == "*" {
                        continue; // 移除全域負向屏蔽
                    }
                    kept_trans.push(l);
                }
            }
            if let Ok(mut f) = OpenOptions::new().write(true).truncate(true).open(&paths.transitions) {
                for l in kept_trans {
                    let _ = writeln!(f, "{}", l);
                }
            }
        }
    }

    if let Some(prev) = prev_word {
        if !prev.is_empty() {
            let today = chrono_like_today();
            let trans_line = format!("{}\t{}\t32.0\tuser_attractor\t{}\n", prev, word, today);
            if let Ok(mut t_file) = OpenOptions::new().create(true).append(true).open(&paths.transitions) {
                let _ = t_file.write_all(trans_line.as_bytes());
            }
        }
    }

    logger::log(&format!("Added user word: [{}] ({}) wt={} with attractor prev={:?}", word, bpmf, weight, prev_word));
    true
}

/// 記錄使用者自建詞事件 (供 AI 語意分析師進行非同步提煉與防劫掠審查)
pub fn log_user_word_event(
    paths: &RuhiPaths,
    word: &str,
    context: &str,
    bpmf: &str,
) {
    let event_file = paths.data_dir.join("user_word_events.jsonl");
    let now_str = chrono_like_now();
    let escaped_word = word.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_ctx = context.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_bpm = bpmf.replace('\\', "\\\\").replace('"', "\\\"");
    let line = format!(
        "{{\"word\":\"{}\",\"context\":\"{}\",\"bopomofo\":\"{}\",\"timestamp\":\"{}\"}}\n",
        escaped_word, escaped_ctx, escaped_bpm, now_str
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&event_file) {
        let _ = f.write_all(line.as_bytes());
        logger::log(&format!("Logged user word event for AI analysis: [{}] context='{}'", word, context));
    }
}

/// 執行刪詞或列入厭惡詞遮蔽清單
pub fn suppress_or_delete_entry(
    paths: &RuhiPaths,
    word: &str,
    bpmf: &str,
) -> (bool, &'static str) {
    let is_custom = is_in_user_dict(paths, word);

    if is_custom {
        // 自自建詞庫中移除
        let user_dict_file = if paths.user_dict.exists() {
            paths.user_dict.clone()
        } else {
            paths.sandbox_dir.join("user_dict.tsv")
        };

        if let Ok(file) = File::open(&user_dict_file) {
            let reader = BufReader::new(file);
            let mut kept_lines = Vec::new();
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.first().map(|w| *w == word).unwrap_or(false) {
                        continue;
                    }
                    kept_lines.push(line);
                }
            }
            if let Ok(mut f) = OpenOptions::new().write(true).truncate(true).open(&user_dict_file) {
                for l in kept_lines {
                    let _ = writeln!(f, "{}", l);
                }
            }
        }

        // 移除轉移引力
        if let Ok(file) = File::open(&paths.transitions) {
            let reader = BufReader::new(file);
            let mut kept_trans = Vec::new();
            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 && (parts[0] == word || parts[1] == word) {
                        continue;
                    }
                    kept_trans.push(line);
                }
            }
            if let Ok(mut f) = OpenOptions::new().write(true).truncate(true).open(&paths.transitions) {
                for l in kept_trans {
                    let _ = writeln!(f, "{}", l);
                }
            }
        }

        logger::log(&format!("Deleted custom word from user_dict: [{}]", word));
        (true, "deleted")
    } else {
        // 非自建詞 ➔ 列入 deleted_dict.tsv 遮蔽清單並施加負向抑制
        let del_dict_file = if paths.deleted_dict.exists() {
            paths.deleted_dict.clone()
        } else {
            paths.sandbox_dir.join("deleted_dict.tsv")
        };

        let now_str = chrono_like_now();
        let del_line = format!("{}\t{}\t{}\n", word, bpmf, now_str);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&del_dict_file) {
            let _ = file.write_all(del_line.as_bytes());
        }

        // 在 transitions_user.tsv 中施加負向抑制懲罰 (-50.0)
        let today = chrono_like_today();
        let penalty_line = format!("*\t{}\t-50.0\tanti_hijack_shield\t{}\n", word, today);
        if let Ok(mut t_file) = OpenOptions::new().create(true).append(true).open(&paths.transitions) {
            let _ = t_file.write_all(penalty_line.as_bytes());
        }

        logger::log(&format!("Suppressed annoying word to deleted_dict & penalty: [{}]", word));
        (true, "suppressed")
    }
}

fn chrono_like_today() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    // 粗略日期計算或使用固定格式 (相容 Windows 系統標準)
    let _days = secs / 86400;
    // 2026-10-06 基準計算
    format!("2026-10-06")
}

fn chrono_like_now() -> String {
    format!("2026-10-06 18:00:00")
}
