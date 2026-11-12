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
    pub templates: HashMap<String, TemplateItem>,
    pub ordered_triggers: Vec<String>,
}

impl TemplatesManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        self.templates.clear();
        self.ordered_triggers.clear();

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
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn match_trigger(&self, trigger: &str) -> Option<&TemplateItem> {
        self.templates.get(trigger)
    }
}
