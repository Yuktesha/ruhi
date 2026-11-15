/// RuhI (如喜) Input Method Framework - Rust Core
/// src/templates/expander.rs - TSV 文字範本與巨集展開器

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[cfg(target_os = "windows")]
#[repr(C)]
struct SystemTime {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(lp_system_time: *mut SystemTime);
}

/// 取得本機目前系統時間元組：(year, month, day, hour, minute, second, weekday_sun0)
pub fn current_local_datetime() -> (i32, u32, u32, u32, u32, u32, u32) {
    #[cfg(target_os = "windows")]
    unsafe {
        let mut st = std::mem::zeroed::<SystemTime>();
        GetLocalTime(&mut st);
        (
            st.w_year as i32,
            st.w_month as u32,
            st.w_day as u32,
            st.w_hour as u32,
            st.w_minute as u32,
            st.w_second as u32,
            st.w_day_of_week as u32,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 跨平台後備：基於 SystemTime::now() 與 Howard Hinnant 演算法（預設 UTC+8）
        let now = std::time::SystemTime::now();
        let dur = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let secs = dur.as_secs() as i64 + 8 * 3600;
        let days = secs.div_euclid(86400);
        let rem_secs = secs.rem_euclid(86400);
        let hour = (rem_secs / 3600) as u32;
        let minute = ((rem_secs % 3600) / 60) as u32;
        let second = (rem_secs % 60) as u32;
        let weekday_sun0 = ((days + 4).rem_euclid(7)) as u32;

        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y } as i32;
        (year, m, d, hour, minute, second, weekday_sun0)
    }
}

/// 展開文字範本字串中之動態日期時間與剪貼簿巨集
pub fn expand_template_str(raw: &str, clipboard: &str) -> (String, i32) {
    let (year, month, day, hour, minute, second, weekday_sun0) = current_local_datetime();
    let roc = year - 1911;
    let yy = (year.rem_euclid(100)) as u32;

    let mut text = raw.to_string();

    text = text.replace("%CLIPBOARD%", clipboard);
    text = text.replace("\\n", "\n");
    text = text.replace("\\t", "\t");
    text = text.replace("{Tab}", "\t");
    text = text.replace("{Enter}", "\n");
    text = text.replace("{Shift+Enter}", "\n");
    text = text.replace("{Ctrl+Enter}", "\n");

    // 日期／時間巨集展開（長標籤優先比對，防止子字串截斷）
    text = text.replace("%YYYY%", &format!("{:04}", year));
    text = text.replace("%YY%", &format!("{:02}", yy));
    text = text.replace("%ROC%", &format!("{}", roc));
    text = text.replace("%YYY%", &format!("{}", roc));

    text = text.replace("%MM%", &format!("{:02}", month));
    text = text.replace("%M%", &format!("{}", month));

    const WEEKDAYS_EN_FULL: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const WEEKDAYS_EN_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let wk_full = WEEKDAYS_EN_FULL.get(weekday_sun0 as usize).copied().unwrap_or("Sunday");
    let wk_short = WEEKDAYS_EN_SHORT.get(weekday_sun0 as usize).copied().unwrap_or("Sun");
    text = text.replace("%DDDD%", wk_full);
    text = text.replace("%DDD%", wk_short);

    text = text.replace("%DD%", &format!("{:02}", day));
    text = text.replace("%D%", &format!("{}", day));

    text = text.replace("%HH%", &format!("{:02}", hour));
    text = text.replace("%NN%", &format!("{:02}", minute));
    text = text.replace("%MI%", &format!("{:02}", minute));
    text = text.replace("%SS%", &format!("{:02}", second));

    const WEEKDAYS: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];
    let wk = WEEKDAYS.get(weekday_sun0 as usize).copied().unwrap_or("日");
    text = text.replace("%WEEKDAY%", wk);
    text = text.replace("%WEEK%", wk);
    text = text.replace("%W%", wk);

    let mut cursor_pos = -1;
    if let Some(pos) = text.find("%CURSOR%") {
        cursor_pos = pos as i32;
        text = text.replace("%CURSOR%", "");
    }

    (text, cursor_pos)
}

#[derive(Debug, Clone)]
pub struct TemplateItem {
    pub trigger: String,
    pub expansion: String,
    pub description: String,
}

impl TemplateItem {
    pub fn new(trigger: &str, expansion: &str, description: &str) -> Self {
        Self {
            trigger: trigger.to_string(),
            expansion: expansion.to_string(),
            description: description.to_string(),
        }
    }

    pub fn expand(&self, clipboard: &str) -> (String, i32) {
        expand_template_str(&self.expansion, clipboard)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TemplatesManager {
    pub items: Vec<TemplateItem>,
    pub templates: HashMap<String, TemplateItem>,
    pub ordered_triggers: Vec<String>,
    pub file_path: Option<std::path::PathBuf>,
    pub last_mtime: Option<std::time::SystemTime>,
}

pub const BUILTIN_TEMPLATES: [(&str, &str, &str); 8] = [
    ("date", "%YYYY%年%MM%月%DD%日（週%WEEK%）", "完整日期含星期"),
    ("time", "%HH%:%NN%:%SS%", "現在時間"),
    ("now", "%YYYY%-%MM%-%DD% %HH%:%NN%:%SS%", "完整日期時間"),
    ("roc", "中華民國%ROC%年%M%月%D%日", "民國日期"),
    ("ymd", "%YYYY%-%MM%-%DD%", "ISO 日期"),
    ("cdate", "%YYYY%年%MM%月%DD%日", "西元中文日期"),
    ("hi", "敬啟者：您好，", "信件問候"),
    ("bye", "順頌 時祺", "信件結尾"),
];

fn extract_text_bpm_headcode(
    text: &str,
    lexicon: Option<&crate::lexicon::LexiconManager>,
) -> String {
    let Some(lex) = lexicon else { return String::new() };
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
            let s = ch.to_string();
            if let Some(ids) = lex.word_index.get(&s) {
                if let Some(best) = ids
                    .iter()
                    .filter_map(|&id| lex.entries.get(id))
                    .max_by(|a, b| a.weight.cmp(&b.weight))
                {
                    let hc = crate::lexicon::LexiconEntry::extract_bpm_headcode(&best.bopomofo);
                    if !hc.is_empty() {
                        out.push_str(&hc);
                    }
                }
            }
            count += 1;
            if count >= 10 {
                break;
            }
        }
    }
    out
}

impl TemplatesManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let path_ref = path.as_ref();
        self.file_path = Some(path_ref.to_path_buf());
        self.last_mtime = std::fs::metadata(path_ref).and_then(|m| m.modified()).ok();

        let file = File::open(path_ref)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        self.templates.clear();
        self.ordered_triggers.clear();
        self.items.clear();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if idx == 0 && (trimmed.contains("TRIGGER") || trimmed.contains("觸發")) {
                continue;
            }

            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let trigger = parts[0].trim();
                let expansion = parts[1];
                let desc = parts.get(2).unwrap_or(&"").trim();

                if !self.templates.contains_key(trigger) {
                    self.ordered_triggers.push(trigger.to_string());
                }
                self.templates.insert(
                    trigger.to_string(),
                    TemplateItem::new(trigger, expansion, desc),
                );
                if !self.items.iter().any(|it| it.trigger == trigger && it.expansion == expansion) {
                    self.items.push(TemplateItem::new(trigger, expansion, desc));
                }
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn maybe_reload(&mut self) {
        let Some(ref path) = self.file_path.clone() else { return };
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                if self.last_mtime != Some(mtime) {
                    let _ = self.load_tsv(&path);
                }
            }
        }
    }

    pub fn match_trigger(&self, trigger: &str) -> Option<&TemplateItem> {
        if let Some(item) = self.templates.get(trigger) {
            return Some(item);
        }
        static BUILTIN_ITEMS: std::sync::OnceLock<Vec<TemplateItem>> = std::sync::OnceLock::new();
        let builtins = BUILTIN_ITEMS.get_or_init(|| {
            BUILTIN_TEMPLATES
                .iter()
                .map(|(trig, exp, desc)| TemplateItem::new(trig, exp, desc))
                .collect()
        });
        builtins.iter().find(|item| item.trigger == trigger)
    }

    pub fn lookup_candidates(
        &self,
        query: &str,
        keyboard_mode: &str,
        lexicon: Option<&crate::lexicon::LexiconManager>,
    ) -> Vec<String> {
        let mut raw_items: Vec<(&str, &str, &str)> = Vec::new();
        if !self.items.is_empty() {
            for item in &self.items {
                raw_items.push((item.trigger.as_str(), item.expansion.as_str(), item.description.as_str()));
            }
        } else {
            let triggers: Vec<String> = if !self.ordered_triggers.is_empty() {
                self.ordered_triggers.clone()
            } else {
                let mut keys: Vec<String> = self.templates.keys().cloned().collect();
                keys.sort();
                keys
            };
            for t in &triggers {
                if let Some(item) = self.templates.get(t) {
                    raw_items.push((item.trigger.as_str(), item.expansion.as_str(), item.description.as_str()));
                }
            }
        }
        for (trig, exp, desc) in &BUILTIN_TEMPLATES {
            if !self.templates.contains_key(*trig) {
                raw_items.push((*trig, *exp, *desc));
            }
        }

        let mut expanded_items: Vec<(&str, String, &str)> = Vec::new();
        for (trig, raw_exp, desc) in raw_items {
            let (exp, _) = expand_template_str(raw_exp, "");
            let exp_trimmed = exp.trim();
            if exp_trimmed.is_empty() {
                continue;
            }
            expanded_items.push((trig, exp, desc));
        }

        let q = query.trim();
        if q.is_empty() {
            let mut out = Vec::new();
            let mut seen_expanded = std::collections::HashSet::new();
            for (_, exp, _) in expanded_items {
                if seen_expanded.insert(exp.clone()) {
                    out.push(exp);
                }
            }
            return out;
        }

        let q_lower = q.to_ascii_lowercase();
        let q_bpm = crate::lexicon::LexiconEntry::raw_query_to_bpm_headcode(q, keyboard_mode);

        let mut scored: Vec<(u8, i32, usize, String)> = Vec::new();
        for (idx, (trig, exp, desc)) in expanded_items.into_iter().enumerate() {
            let exp_hc = extract_text_bpm_headcode(&exp, lexicon);
            let desc_hc = extract_text_bpm_headcode(desc, lexicon);

            let tier_score: Option<(u8, i32)> = if trig.eq_ignore_ascii_case(q) || trig == q {
                Some((1u8, 2000i32 - idx as i32))
            } else if (!exp_hc.is_empty() && (exp_hc == q || q_bpm.as_deref() == Some(exp_hc.as_str())))
                || (!desc_hc.is_empty() && (desc_hc == q || q_bpm.as_deref() == Some(desc_hc.as_str())))
            {
                Some((2u8, 1500i32 - idx as i32))
            } else if trig.to_ascii_lowercase().starts_with(&q_lower) {
                let diff = trig.chars().count().saturating_sub(q.chars().count()) as i32;
                Some((3u8, 1000i32 - diff * 20 - idx as i32))
            } else if (!exp_hc.is_empty()
                && (exp_hc.starts_with(q) || q_bpm.as_deref().map_or(false, |qb| exp_hc.starts_with(qb))))
                || (!desc_hc.is_empty()
                    && (desc_hc.starts_with(q) || q_bpm.as_deref().map_or(false, |qb| desc_hc.starts_with(qb))))
                || exp.to_lowercase().contains(&q_lower)
                || desc.to_lowercase().contains(&q_lower)
            {
                Some((4u8, 600i32 - idx as i32))
            } else {
                None
            };

            if let Some((tier, score)) = tier_score {
                scored.push((tier, score, idx, exp));
            }
        }

        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let mut out: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (_, _, _, exp) in scored {
            if seen.insert(exp.clone()) {
                out.push(exp);
            }
        }
        out
    }

    pub fn format_template_badge(&self, expanded_text: &str) -> Option<String> {
        if !self.items.is_empty() {
            for item in &self.items {
                let (exp, _) = expand_template_str(&item.expansion, "");
                if exp == expanded_text {
                    let desc = item.description.trim();
                    return if !desc.is_empty() {
                        Some(format!("縮略碼: [{}] ｜ {}", item.trigger, desc))
                    } else {
                        Some(format!("縮略碼: [{}]", item.trigger))
                    };
                }
            }
        } else {
            let triggers: Vec<String> = if !self.ordered_triggers.is_empty() {
                self.ordered_triggers.clone()
            } else {
                let mut keys: Vec<String> = self.templates.keys().cloned().collect();
                keys.sort();
                keys
            };
            for t in &triggers {
                if let Some(item) = self.templates.get(t) {
                    let (exp, _) = expand_template_str(&item.expansion, "");
                    if exp == expanded_text {
                        let desc = item.description.trim();
                        return if !desc.is_empty() {
                            Some(format!("縮略碼: [{}] ｜ {}", item.trigger, desc))
                        } else {
                            Some(format!("縮略碼: [{}]", item.trigger))
                        };
                    }
                }
            }
        }
        for (trig, raw_exp, desc) in &BUILTIN_TEMPLATES {
            let (exp, _) = expand_template_str(raw_exp, "");
            if exp == expanded_text {
                let d = desc.trim();
                return if !d.is_empty() {
                    Some(format!("縮略碼: [{}] ｜ {}", trig, d))
                } else {
                    Some(format!("縮略碼: [{}]", trig))
                };
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_candidates_date_time_and_shared_expansion_261114() {
        let mut tm = TemplatesManager::new();
        // 1. Built-in templates: lookup "date", "time", "now"
        let cands_date = tm.lookup_candidates("date", "standard", None);
        assert!(!cands_date.is_empty(), "date should have candidate");
        let cands_time = tm.lookup_candidates("time", "standard", None);
        assert!(!cands_time.is_empty(), "time should have candidate");
        let cands_now = tm.lookup_candidates("now", "standard", None);
        assert!(!cands_now.is_empty(), "now should have candidate");

        // 2. Test shared expansion: TSV with both "today" and "date" having identical expansion
        let tsv_content = "TRIGGER\tEXPANSION\tDESCRIPTION\ntoday\t%YYYY%年%MM%月%DD%日\t今日日期\ndate\t%YYYY%年%MM%月%DD%日\t今日日期\ndate\t%YYYY%-%MM%-%DD%\tISO 日期\n";
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_templates_261114.tsv");
        std::fs::write(&temp_file, tsv_content).unwrap();

        let load_res = tm.load_tsv(&temp_file);
        assert!(load_res.is_ok());

        let cands_date_loaded = tm.lookup_candidates("date", "standard", None);
        // date should return both format 1 and format 2!
        assert_eq!(cands_date_loaded.len(), 2, "date should return 2 candidates");

        let cands_today_loaded = tm.lookup_candidates("today", "standard", None);
        assert_eq!(cands_today_loaded.len(), 1, "today should return 1 candidate");
        assert_eq!(cands_date_loaded[0], cands_today_loaded[0], "first candidate should match shared expansion");

        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn test_match_trigger_builtin_date_fallback_261114() {
        let mut tm = TemplatesManager::new();
        // 1. 尚未載入 TSV 前，Builtin date/time/now 直接觸發碼即可命中
        let d_item = tm.match_trigger("date").expect("builtin date trigger should match");
        assert!(d_item.expansion.contains("%YYYY%"));
        let (d_exp, _) = d_item.expand("");
        assert!(d_exp.contains("年") && d_exp.contains("週"));

        // 2. 載入僅含 time、不含 date 的自訂 TSV 後，date 仍可 fallback 命中 builtin
        let tsv_content = "TRIGGER\tEXPANSION\tDESCRIPTION\ntime\tCUSTOM_TIME\t自訂時間\n";
        let temp_file = std::env::temp_dir().join("test_templates_fallback_261114.tsv");
        std::fs::write(&temp_file, tsv_content).unwrap();
        assert!(tm.load_tsv(&temp_file).is_ok());

        let t_item = tm.match_trigger("time").expect("custom time should override builtin");
        assert_eq!(t_item.expansion, "CUSTOM_TIME");
        let d_fallback = tm.match_trigger("date").expect("date should still fallback to builtin");
        assert!(d_fallback.expansion.contains("%WEEK%"));

        let _ = std::fs::remove_file(temp_file);
    }
}
