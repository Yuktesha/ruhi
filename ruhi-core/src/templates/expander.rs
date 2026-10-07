/// RuhI (如喜) Input Method Framework - Rust Core
/// src/templates/expander.rs - TSV 文字範本與巨集展開器

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

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
        let mut text = self.expansion.clone();

        text = text.replace("%CLIPBOARD%", clipboard);
        text = text.replace("\\n", "\n");
        text = text.replace("\\t", "\t");
        text = text.replace("{Tab}", "\t");
        text = text.replace("{Enter}", "\n");
        text = text.replace("{Shift+Enter}", "\n");
        text = text.replace("{Ctrl+Enter}", "\n");

        let mut cursor_pos = -1;
        if let Some(pos) = text.find("%CURSOR%") {
            cursor_pos = pos as i32;
            text = text.replace("%CURSOR%", "");
        }

        (text, cursor_pos)
    }
}

#[derive(Debug, Default)]
pub struct TemplatesManager {
    pub templates: HashMap<String, TemplateItem>,
}

impl TemplatesManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;

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
