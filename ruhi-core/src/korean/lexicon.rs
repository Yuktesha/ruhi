/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/korean/lexicon.rs - 韓語標準羅馬拼音（RR）常用詞表與學習者骨架索引

use crate::korean::jamo::to_romaja;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct KoreanEntry {
    pub surface: String, // 諺文詞（只能含完整諺文音節）
    pub rr: String,      // TSV 提供的官方 RR 拼寫
    pub weight: u32,     // 1000 或 1100
}

#[derive(Debug, Clone, Default)]
pub struct KoreanLexicon {
    pub entries: Vec<KoreanEntry>,
    /// 精確索引：鍵 1（rr）與鍵 2（to_romaja(surface)），皆已移除 '-' 並轉小寫
    pub key_index: HashMap<String, Vec<usize>>,
    /// 學習者骨架索引：鍵 3
    pub skeleton_index: HashMap<String, Vec<usize>>,
    /// 精確索引中最長的鍵長（位元組），供 DP 限制詞邊搜尋範圍
    pub max_key_len: usize,
}

/// 內建精簡常用詞表（約 55 條，為 data/korean.tsv 之嚴格子集）
pub const BUILTIN_LEXICON: &[(&str, &str, u32)] = &[
    ("안녕하세요", "annyeonghaseyo", 1100),
    ("안녕히가세요", "annyeonghigaseyo", 1100),
    ("안녕히계세요", "annyeonghigyeseyo", 1100),
    ("감사합니다", "gamsahamnida", 1100),
    ("고맙습니다", "gomapseumnida", 1100),
    ("고마워요", "gomawoyo", 1100),
    ("죄송합니다", "joesonghamnida", 1100),
    ("미안합니다", "mianhamnida", 1100),
    ("미안해요", "mianhaeyo", 1100),
    ("실례합니다", "sillyehamnida", 1100),
    ("반갑습니다", "bangapseumnida", 1100),
    ("환영합니다", "hwanyeonghamnida", 1100),
    ("사랑해요", "saranghaeyo", 1100),
    ("사랑합니다", "saranghamnida", 1100),
    ("괜찮아요", "gwaenchanayo", 1100),
    ("괜찮습니다", "gwaenchanseumnida", 1100),
    ("잠시만요", "jamsimanyo", 1100),
    ("도와주세요", "dowajuseyo", 1100),
    ("살려주세요", "sallyeojuseyo", 1100),
    ("한글", "hangeul", 1000),
    ("한국", "hanguk", 1000),
    ("한국어", "hangugeo", 1000),
    ("대한민국", "daehanminguk", 1000),
    ("서울", "seoul", 1000),
    ("부산", "busan", 1000),
    ("인천", "incheon", 1000),
    ("제주도", "jejudo", 1000),
    ("명동", "myeongdong", 1000),
    ("홍대", "hongdae", 1000),
    ("강남", "gangnam", 1000),
    ("공항", "gonghang", 1000),
    ("지하철", "jihacheol", 1000),
    ("기차역", "gichayeok", 1000),
    ("버스", "beoseu", 1000),
    ("택시", "taeksi", 1000),
    ("호텔", "hotel", 1000),
    ("화장실", "hwajangsil", 1000),
    ("병원", "byeongwon", 1000),
    ("약국", "yakguk", 1000),
    ("경찰서", "gyeongchalseo", 1000),
    ("식당", "sikdang", 1000),
    ("주세요", "juseyo", 1000),
    ("맛있어요", "masisseoyo", 1000),
    ("얼마예요", "eolmayeyo", 1000),
    ("김치", "gimchi", 1000),
    ("비빔밥", "bibimbap", 1000),
    ("불고기", "bulgogi", 1000),
    ("떡볶이", "tteokbokki", 1000),
    ("삼겹살", "samgyeopsal", 1000),
    ("김밥", "gimbap", 1000),
    ("라면", "ramyeon", 1000),
    ("치킨", "chikin", 1000),
    ("커피", "keopi", 1000),
    ("아메리카노", "amerikano", 1000),
    ("하나", "hana", 1000),
    ("가을", "gaeul", 1000),
    ("겨울", "gyeoul", 1000),
    ("여름", "yeoreum", 1000),
    ("학교", "hakgyo", 1000),
    ("친구", "chingu", 1000),
];

impl KoreanLexicon {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin() -> Self {
        let mut lex = Self::new();
        for &(surface, rr, weight) in BUILTIN_LEXICON {
            lex.add_entry(surface, rr, weight);
        }
        lex
    }

    /// 學習者骨架函式（依序執行）：
    /// 1. 轉小寫，移除所有 '-'
    /// 2. `ch` → `j`
    /// 3. 逐字元替換：`k`→`g`、`t`→`d`、`p`→`b`、`l`→`r`
    /// 4. 把連續重複的同一個子音收斂成一個（母音 a e i o u 不收斂）
    pub fn skeleton(romaja: &str) -> String {
        let step1: String = romaja
            .to_ascii_lowercase()
            .chars()
            .filter(|&c| c != '-')
            .collect();
        let step2 = step1.replace("ch", "j");
        let mut step4 = String::with_capacity(step2.len());
        let mut prev_char: Option<char> = None;

        for ch in step2.chars() {
            let mapped = match ch {
                'k' => 'g',
                't' => 'd',
                'p' => 'b',
                'l' => 'r',
                other => other,
            };
            let is_vowel = matches!(mapped, 'a' | 'e' | 'i' | 'o' | 'u');
            if !is_vowel && mapped.is_ascii_alphabetic() && prev_char == Some(mapped) {
                continue;
            }
            step4.push(mapped);
            prev_char = Some(mapped);
        }

        step4
    }

    fn insert_and_sort_index(
        entries: &[KoreanEntry],
        map: &mut HashMap<String, Vec<usize>>,
        key: String,
        idx: usize,
    ) {
        if key.is_empty() {
            return;
        }
        let list = map.entry(key).or_default();
        if !list.contains(&idx) {
            list.push(idx);
        }
        list.sort_by(|&a, &b| {
            entries[b]
                .weight
                .cmp(&entries[a].weight)
                .then_with(|| a.cmp(&b))
        });
    }

    pub fn add_entry(&mut self, surface: &str, rr: &str, weight: u32) {
        let surface_trimmed = surface.trim();
        let rr_trimmed = rr.trim();
        if surface_trimmed.is_empty() || rr_trimmed.is_empty() {
            return;
        }

        if let Some((existing_idx, existing)) = self
            .entries
            .iter_mut()
            .enumerate()
            .find(|(_, e)| e.surface == surface_trimmed && e.rr.eq_ignore_ascii_case(rr_trimmed))
        {
            if weight > existing.weight {
                existing.weight = weight;
                let k1 = existing.rr.to_ascii_lowercase().replace('-', "");
                let k2 = to_romaja(&existing.surface)
                    .to_ascii_lowercase()
                    .replace('-', "");
                let s1 = Self::skeleton(&existing.rr);
                let s2 = Self::skeleton(&k2);
                Self::insert_and_sort_index(&self.entries, &mut self.key_index, k1, existing_idx);
                Self::insert_and_sort_index(&self.entries, &mut self.key_index, k2, existing_idx);
                Self::insert_and_sort_index(
                    &self.entries,
                    &mut self.skeleton_index,
                    s1,
                    existing_idx,
                );
                Self::insert_and_sort_index(
                    &self.entries,
                    &mut self.skeleton_index,
                    s2,
                    existing_idx,
                );
            }
            return;
        }

        let idx = self.entries.len();
        self.entries.push(KoreanEntry {
            surface: surface_trimmed.to_string(),
            rr: rr_trimmed.to_string(),
            weight,
        });

        let k1 = rr_trimmed.to_ascii_lowercase().replace('-', "");
        let k2 = to_romaja(surface_trimmed)
            .to_ascii_lowercase()
            .replace('-', "");
        self.max_key_len = self.max_key_len.max(k1.len()).max(k2.len());

        Self::insert_and_sort_index(&self.entries, &mut self.key_index, k1.clone(), idx);
        if k2 != k1 {
            Self::insert_and_sort_index(&self.entries, &mut self.key_index, k2.clone(), idx);
        }

        let s1 = Self::skeleton(&k1);
        let s2 = Self::skeleton(&k2);
        Self::insert_and_sort_index(&self.entries, &mut self.skeleton_index, s1.clone(), idx);
        if s2 != s1 {
            Self::insert_and_sort_index(&self.entries, &mut self.skeleton_index, s2, idx);
        }
    }

    pub fn load_tsv<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<usize> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        for line_res in reader.lines() {
            let line = line_res?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 2 {
                let surface = parts[0].trim();
                let rr = parts[1].trim();
                let weight: u32 = parts
                    .get(2)
                    .and_then(|w| w.trim().parse().ok())
                    .unwrap_or(1000);
                self.add_entry(surface, rr, weight);
                count += 1;
            }
        }
        Ok(count)
    }

    /// 依鍵查詢，回傳的條目依「權重降序、加入順序升序」排序
    pub fn lookup_exact(&self, key: &str) -> Vec<&KoreanEntry> {
        let norm_key: String = key
            .to_ascii_lowercase()
            .chars()
            .filter(|&c| c != '-')
            .collect();
        if let Some(indices) = self.key_index.get(&norm_key) {
            indices.iter().map(|&idx| &self.entries[idx]).collect()
        } else {
            Vec::new()
        }
    }
}
