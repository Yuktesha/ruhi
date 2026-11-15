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

    // 動態載入 TaskDialogIndirect（防禦宿主環境如 conhost.exe、LibreOffice 等僅載入 ComCtl32 v5.82 導致進入點缺失！）
    type PfnTaskDialogIndirect = unsafe extern "system" fn(
        *const TASKDIALOGCONFIG,
        *mut i32,
        *mut i32,
        *mut BOOL,
    ) -> HRESULT;

    let pfn: Option<PfnTaskDialogIndirect> = unsafe {
        use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
        use windows::core::s;
        let h_mod = GetModuleHandleA(s!("comctl32.dll")).unwrap_or_default();
        if !h_mod.is_invalid() {
            GetProcAddress(h_mod, s!("TaskDialogIndirect")).map(|f| std::mem::transmute(f))
        } else {
            None
        }
    };

    if let Some(task_dialog_indirect) = pfn {
        let hr = unsafe {
            task_dialog_indirect(
                &config,
                &mut button_id,
                &mut radio_id,
                &mut verif_checked,
            )
        };
        if hr.is_ok() {
            let confirmed = button_id == IDYES.0;
            let do_not_ask = verif_checked.as_bool();
            return (confirmed, do_not_ask);
        }
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

    // 2. 寫入 user_dict.tsv（含人名「姓＋名」與公司全稱「主名稱＋附加名」智慧拆分衍生詞條）
    let (sub_entries, sub_transitions) =
        ruhi::lexicon::LexiconManager::decompose_entity_subentries(word, bpmf, weight);

    let mut existing_user_words = HashSet::new();
    if user_dict_file.exists() {
        if let Ok(f_read) = File::open(&user_dict_file) {
            for l_res in BufReader::new(f_read).lines() {
                if let Ok(l) = l_res {
                    if let Some(w) = l.split('\t').next() {
                        let wt = w.trim();
                        if !wt.is_empty() && !wt.starts_with('#') {
                            existing_user_words.insert(wt.to_string());
                        }
                    }
                }
            }
        }
    }

    let is_new = !user_dict_file.exists();
    let mut file = match OpenOptions::new().create(true).append(true).open(&user_dict_file) {
        Ok(f) => f,
        Err(e) => {
            logger::log(&format!("Failed to open user_dict for writing: {:?}", e));
            return false;
        }
    };
    if is_new {
        let header = "# RuhI 使用者自建詞庫 (User Custom Lexicon)\n# 格式規範: WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\tHEADCODE\tABBREVIATIONS\n# 說明: 第 6 欄 HEADCODE 自動補上注音頭碼；第 7 欄起可自由輸入多個自訂縮略語（支援以 Tab 或連續空格隔開）\n";
        let _ = file.write_all(header.as_bytes());
    }

    let norm_bpm = ruhi::lexicon::LexiconEntry::normalize_bpm(bpmf);
    let hc = ruhi::lexicon::LexiconEntry::extract_bpm_headcode(&norm_bpm);
    let entry_line = format!("{}\t{}\t{}\tuser\tTSF-Selection\t{}\n", word, norm_bpm, weight, hc);
    if let Err(e) = file.write_all(entry_line.as_bytes()) {
        logger::log(&format!("Failed to write entry to user_dict: {:?}", e));
        return false;
    }

    for sub in &sub_entries {
        if !existing_user_words.contains(&sub.word) && sub.word != word {
            let norm_sub_bpm = ruhi::lexicon::LexiconEntry::normalize_bpm(&sub.bopomofo);
            let sub_hc = ruhi::lexicon::LexiconEntry::extract_bpm_headcode(&norm_sub_bpm);
            let sub_line = format!(
                "{}\t{}\t{}\tuser\t{}\t{}\n",
                sub.word, norm_sub_bpm, sub.weight, sub.note, sub_hc
            );
            let _ = file.write_all(sub_line.as_bytes());
            existing_user_words.insert(sub.word.clone());
        }
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

    let today = chrono_like_today();
    if let Ok(mut t_file) = OpenOptions::new().create(true).append(true).open(&paths.transitions) {
        if let Some(prev) = prev_word {
            if !prev.is_empty() {
                let trans_line = format!("{}\t{}\t32.0\tuser_attractor\t{}\n", prev, word, today);
                let _ = t_file.write_all(trans_line.as_bytes());
            }
        }
        for (p, c, b) in &sub_transitions {
            let trans_line = format!("{}\t{}\t{:.1}\tentity_decomp\t{}\n", p, c, b, today);
            let _ = t_file.write_all(trans_line.as_bytes());
        }
    }

    logger::log(&format!(
        "Added user word: [{}] ({}) wt={} with attractor prev={:?}, sub_entries={}",
        word,
        bpmf,
        weight,
        prev_word,
        sub_entries.len()
    ));
    true
}

/// 記錄使用者自建詞事件 (供 AI 語意分析師進行非同步提煉與防劫掠審查)
pub fn log_user_word_event(
    paths: &RuhiPaths,
    word: &str,
    context: &str,
    bpmf: &str,
) {
    let _ = std::fs::create_dir_all(&paths.sandbox_dir);
    let event_file = paths.sandbox_dir.join("user_word_events.jsonl");
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
        let trans_target = if !paths.transitions.starts_with(&paths.sandbox_dir) {
            let _ = std::fs::create_dir_all(&paths.sandbox_dir);
            let user_trans = paths.sandbox_dir.join("transitions_user.tsv");
            if !user_trans.exists() && paths.transitions.exists() {
                let _ = std::fs::copy(&paths.transitions, &user_trans);
            }
            user_trans
        } else {
            paths.transitions.clone()
        };
        if let Ok(mut t_file) = OpenOptions::new().create(true).append(true).open(&trans_target) {
            let _ = t_file.write_all(penalty_line.as_bytes());
        }

        logger::log(&format!("Suppressed annoying word to deleted_dict & penalty: [{}]", word));
        (true, "suppressed")
    }
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn get_taipei_time_components() -> (i32, u32, u32, u32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let taipei_secs = epoch_secs + 8 * 3600;
    let days = taipei_secs.div_euclid(86400);
    let rem_secs = taipei_secs.rem_euclid(86400) as u32;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let secs = rem_secs % 60;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, hours, mins, secs)
}

fn chrono_like_today() -> String {
    let (y, m, d, _, _, _) = get_taipei_time_components();
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn chrono_like_now() -> String {
    let (y, m, d, hh, mm, ss) = get_taipei_time_components();
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hh, mm, ss)
}
