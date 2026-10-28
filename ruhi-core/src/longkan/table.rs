// ruhi-core/src/longkan/table.rs — 如喜・龍龕形音輸入法 (Longkan Morpho-Phonetic 4-Code IME) 碼表與反查索引
//
// 源自西元 997 年遼代高僧行均《龍龕手鑑》「打破說文死板部首、依視覺字形歸部並結合四聲檢字」之精神：
// - 4 碼定長正規化：
//   1. 有聲部首(2碼) + 聲符/部件音(2碼)（羅馬/台羅古音與許氏雙拼雙基底並容，如 鋪=kmfu/jnfx、龕=helo/holg/hgll）
//   2. 視覺英文字母象形(1碼) + 聲符音(2碼) + Space/聲調補碼(12345/dfjs)（如 肴=xeo/xeo2/xeo3/xeof、凶=xu/xu1）
//   3. 部件 × 數字 (C × N)（如 區=cko3/cpn/cpn3/cpnf、品=ko3/ko33/ko3f、森=mu3、協=tli3、蕊=wsm3）

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongkanEntry {
    pub code: String,
    pub text: String,
    pub weight: u32,
    pub kind: String,
    pub breakdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongkanReverseEntry {
    pub primary_code: String,
    pub primary_score: u32,
    pub alt_codes: Vec<String>,
    pub alt_breakdowns: Vec<(String, String)>,
    pub breakdown: String,
}

#[derive(Debug, Clone)]
pub struct LongkanTable {
    pub map: HashMap<String, Vec<LongkanEntry>>,
    pub char_reverse: HashMap<char, LongkanReverseEntry>,
}

impl Default for LongkanTable {
    fn default() -> Self {
        let mut table = Self {
            map: HashMap::new(),
            char_reverse: HashMap::new(),
        };
        table.load_builtin_defaults();
        if let Some(p) = Self::find_default_tsv_path() {
            let _ = table.load_tsv(&p);
        }
        if let Some(up) = Self::find_user_tsv_path() {
            let _ = table.load_tsv(&up);
        }
        table
    }
}

impl LongkanTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_default_tsv_path() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ruhi_lexicon/data/longkan.tsv"),
            PathBuf::from("ruhi_lexicon/data/longkan.tsv"),
            PathBuf::from("data/longkan.tsv"),
        ];
        for p in candidates {
            if p.is_file() {
                return Some(p);
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for rel in [
                    "data/longkan.tsv",
                    "../data/longkan.tsv",
                    "../ruhi_lexicon/data/longkan.tsv",
                ] {
                    let p = dir.join(rel);
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn find_user_tsv_path() -> Option<PathBuf> {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ruhi_lexicon/data/longkan_user.tsv"),
            PathBuf::from("ruhi_lexicon/data/longkan_user.tsv"),
            PathBuf::from("data/longkan_user.tsv"),
        ];
        for p in candidates {
            if p.is_file() {
                return Some(p);
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for rel in [
                    "data/longkan_user.tsv",
                    "../data/longkan_user.tsv",
                ] {
                    let p = dir.join(rel);
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn insert_entry(
        &mut self,
        code: &str,
        text: &str,
        weight: u32,
        kind: &str,
        breakdown: &str,
    ) {
        let c_lower = code.trim().to_ascii_lowercase();
        let t_clean = text.trim();
        if c_lower.is_empty() || t_clean.is_empty() {
            return;
        }
        let entry = LongkanEntry {
            code: c_lower.clone(),
            text: t_clean.to_string(),
            weight,
            kind: kind.trim().to_string(),
            breakdown: breakdown.trim().to_string(),
        };
        let list = self.map.entry(c_lower.clone()).or_default();
        if let Some(existing) = list.iter_mut().find(|e| e.text == t_clean) {
            if weight > existing.weight {
                existing.weight = weight;
                existing.kind = entry.kind.clone();
                if !entry.breakdown.is_empty() {
                    existing.breakdown = entry.breakdown.clone();
                }
            }
        } else {
            list.push(entry);
        }

        if t_clean.chars().count() == 1 {
            if let Some(ch) = t_clean.chars().next() {
                let b_clean = breakdown.trim().to_string();
                let is_pure_4alpha =
                    c_lower.len() == 4 && c_lower.chars().all(|c| c.is_ascii_alphabetic());
                let score = weight
                    + if kind.trim() == "curated" { 1000 } else { 0 }
                    + if is_pure_4alpha { 250 } else { 0 };
                let rev = self
                    .char_reverse
                    .entry(ch)
                    .or_insert_with(|| LongkanReverseEntry {
                        primary_code: c_lower.clone(),
                        primary_score: score,
                        alt_codes: Vec::new(),
                        alt_breakdowns: Vec::new(),
                        breakdown: b_clean.clone(),
                    });
                if rev.primary_code == c_lower {
                    if score > rev.primary_score {
                        rev.primary_score = score;
                    }
                    if rev.breakdown.is_empty() && !b_clean.is_empty() {
                        rev.breakdown = b_clean;
                    }
                } else if score > rev.primary_score {
                    let old_code = rev.primary_code.clone();
                    let old_bdesc = rev.breakdown.clone();
                    rev.alt_codes.retain(|x| x != &c_lower && x != &old_code);
                    rev.alt_breakdowns
                        .retain(|(x, _)| x != &c_lower && x != &old_code);
                    rev.alt_codes.insert(0, old_code.clone());
                    rev.alt_breakdowns.insert(0, (old_code, old_bdesc));
                    rev.primary_code = c_lower;
                    rev.primary_score = score;
                    rev.breakdown = b_clean;
                } else if !rev.alt_codes.contains(&c_lower) {
                    rev.alt_codes.push(c_lower.clone());
                    rev.alt_breakdowns.push((c_lower, b_clean));
                }
            }
        }
    }

    pub fn finalize_sorting(&mut self) {
        for list in self.map.values_mut() {
            list.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.text.cmp(&b.text)));
        }
    }

    pub fn load_tsv(&mut self, path: &Path) -> io::Result<usize> {
        if !path.is_file() {
            return Ok(self.map.len());
        }
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0usize;
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("CODE\t") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\t').collect();
            if parts.len() >= 3 {
                let code = parts[0];
                let text = parts[1];
                let weight: u32 = parts[2].parse().unwrap_or(500);
                let kind = parts.get(3).copied().unwrap_or("rad_phon");
                let breakdown = parts.get(4).copied().unwrap_or("");
                self.insert_entry(code, text, weight, kind, breakdown);
                count += 1;
            }
        }
        self.finalize_sorting();
        Ok(count)
    }

    pub fn load_builtin_defaults(&mut self) {
        let builtins: &[(&str, &str, u32, &str, &str)] = &[
            ("lnln", "龍", 2680, "curated", "獨體疊音:龍(ln)+龍(ln)"),
            ("liln", "龍", 2660, "curated", "最簡部件:立(li)+龍(ln)"),
            ("yeln", "龍", 2640, "curated", "最簡部件:月(ye)+龍(ln)"),
            ("lxl2", "龍", 2620, "curated", "許氏友善:ㄌ(l)ㄨ(x)ㄥ(l)+2聲"),
            ("kmfu", "鋪", 2600, "curated", "金(km)+甫(fu)"),
            ("jnfx", "鋪", 2500, "curated", "金(jn)+甫(fx)"),
            ("jenfx", "鋪", 2680, "curated", "許氏全拼:金(jen)+甫(fx)"),
            ("jenfx1", "鋪", 2680, "curated", "許氏全拼:金(jen)+甫(fx)+1聲"),
            ("jenfx4", "鋪", 2680, "curated", "許氏全拼:金(jen)+甫(fx)+4聲"),
            ("jenfxj", "鋪", 2680, "curated", "許氏全拼:金(jen)+甫(fx)+4聲j"),
            ("jinfu", "鋪", 2660, "curated", "拼音全拼:金(jin)+甫(fu)"),
            ("ahem", "鋪", 2450, "curated", "視覺拆字:釒(ae)+甫(em)"),
            ("holn", "龕", 2680, "curated", "最簡部件:合(ho)+龍(ln)"),
            ("hgln", "龕", 2660, "curated", "許氏友善:合(hg)+龍(ln)"),
            ("hglxl", "龕", 2700, "curated", "許氏全拼:合(hg)+龍(lxl)"),
            ("hglxl1", "龕", 2700, "curated", "許氏全拼:合(hg)+龍(lxl)+1聲"),
            ("helong", "龕", 2690, "curated", "拼音全拼:合(he)+龍(long)"),
            ("helong1", "龕", 2690, "curated", "拼音全拼:合(he)+龍(long)+1聲"),
            ("helo", "龕", 2550, "curated", "合(he)+龍(lo)"),
            ("holg", "龕", 2540, "curated", "合(ho)+龍(lg)"),
            ("hgll", "龕", 2530, "curated", "許氏雙拼:合(hg)+龍(ll)"),
            ("aeod", "龕", 2480, "curated", "視覺拆字:合(aeo)+龍(d)"),
            ("rue", "明", 2680, "curated", "許氏全拼:日(r)+月(ue)"),
            ("rue2", "明", 2680, "curated", "許氏全拼:日(r)+月(ue)+2聲"),
            ("rued", "明", 2680, "curated", "許氏全拼:日(r)+月(ue)+2聲d"),
            ("reue", "明", 2670, "curated", "許氏相容:日(re)+月(ue)"),
            ("riyue", "明", 2680, "curated", "拼音全拼:日(ri)+月(yue)"),
            ("bode", "明", 2660, "curated", "視覺拆字:日(bo)+月(de)"),
            ("bd", "明", 2620, "curated", "視覺簡碼:日(b)+月(d)"),
            ("cko", "古", 2650, "curated", "許氏全拼:十(c)+口(ko)"),
            ("ckof", "古", 2660, "curated", "許氏全拼:十(c)+口(ko)+3聲f"),
            ("shikou", "古", 2650, "curated", "拼音全拼:十(shi)+口(kou)"),
            ("tio", "古", 2640, "curated", "視覺拆字:十(ti)+口(o)"),
            ("to", "古", 2600, "curated", "十(t)+口(o)+␣"),
            ("oti", "叶", 2650, "curated", "視覺拆字:口(o)+十(ti)"),
            ("txo", "杏", 2650, "curated", "視覺拆字:木(tx)+口(o)"),
            ("otx", "呆", 2650, "curated", "視覺拆字:口(o)+木(tx)"),
            ("atx", "休", 2640, "curated", "視覺拆字:亻(a)+木(tx)"),
            ("xoje", "好", 2650, "curated", "視覺拆字:女(xo)+子(je)"),
            ("txtx", "林", 2620, "curated", "視覺拆字:木(tx)+木(tx)"),
            ("ttt", "森", 2620, "curated", "視覺拆字:木(t)×3+␣"),
            ("ooo", "品", 2650, "curated", "視覺拆字:口(o)×3+␣"),
            ("oi", "中", 2600, "curated", "視覺拆字:口(o)+丨(i)"),
            ("cooo", "區", 2620, "curated", "視覺拆字:匚(c)+口×3(ooo)"),
            ("jeje", "喆", 2680, "curated", "許氏全拼:吉(je)+吉(je)"),
            ("jeje2", "喆", 2680, "curated", "許氏全拼:吉(je)+吉(je)+2聲"),
            ("jejed", "喆", 2680, "curated", "許氏全拼:吉(je)+吉(je)+2聲d"),
            ("jiji", "喆", 2680, "curated", "拼音全拼:吉(ji)+吉(ji)"),
            ("emcnam", "謝", 2680, "curated", "許氏全拼:言(em)+身(cn)+寸(am)"),
            ("emcnam4", "謝", 2680, "curated", "許氏全拼:言(em)+身(cn)+寸(am)+4聲"),
            ("emcnamj", "謝", 2680, "curated", "許氏全拼:言(em)+身(cn)+寸(am)+4聲j"),
            ("emam", "謝", 2660, "curated", "許氏首尾:言(em)+寸(am)"),
            ("yanshencun", "謝", 2670, "curated", "拼音全拼:言(yan)+身(shen)+寸(cun)"),
            ("cdoko", "喜", 2680, "curated", "許氏全拼:士(c)+豆(do)+口(ko)"),
            ("cdoko3", "喜", 2680, "curated", "許氏全拼:士(c)+豆(do)+口(ko)+3聲"),
            ("cdokof", "喜", 2680, "curated", "許氏全拼:士(c)+豆(do)+口(ko)+3聲f"),
            ("shidoukou", "喜", 2670, "curated", "拼音全拼:士(shi)+豆(dou)+口(kou)"),
            ("comx", "看", 2680, "curated", "許氏全拼:手(co)+目(mx)"),
            ("comx4", "看", 2680, "curated", "許氏全拼:手(co)+目(mx)+4聲"),
            ("comxj", "看", 2680, "curated", "許氏全拼:手(co)+目(mx)+4聲j"),
            ("shoumu", "看", 2670, "curated", "拼音全拼:手(shou)+目(mu)"),
            ("emx", "語", 2680, "curated", "許氏全拼:言(em)+吾(x)"),
            ("emx3", "語", 2680, "curated", "許氏全拼:言(em)+吾(x)+3聲"),
            ("emxf", "語", 2680, "curated", "許氏全拼:言(em)+吾(x)+3聲f"),
            ("yanwu", "語", 2670, "curated", "拼音全拼:言(yan)+吾(wu)"),
            ("jn3", "形", 2680, "curated", "最簡部件:井(jn)+彡(3畫)"),
            ("jnsn", "形", 2660, "curated", "最簡部件:井(jn)+彡(sn)"),
            ("jnse", "形", 2640, "curated", "許氏友善:井(jn)+形(se)"),
            ("jncn", "形", 2620, "curated", "許氏友善:井(jn)+彡/形(cn)"),
            ("lir4", "音", 2680, "curated", "最簡部件:立(li)+日(r)+4"),
            ("inin", "音", 2660, "curated", "獨體疊音:音(in)+音(in)"),
            ("seci", "法", 2680, "curated", "最簡部件:水(se)+去(ci)"),
            ("sefa", "法", 2660, "curated", "最簡部件:水(se)+本音法(fa)"),
            ("xeo", "肴", 2600, "curated", "乂(x)+有(eo)+␣"),
            ("xeo2", "肴", 2600, "curated", "乂(x)+有(eo)+二聲2"),
            ("xeod", "肴", 2600, "curated", "乂(x)+有(eo)+二聲d"),
            ("xeo3", "肴", 2600, "curated", "乂(x)+有(eo)+三聲3"),
            ("xeof", "肴", 2600, "curated", "乂(x)+有(eo)+三聲f"),
            ("xu", "凶", 2600, "curated", "乂(x)+凵(u)+␣"),
            ("xu1", "凶", 2600, "curated", "乂(x)+凵(u)+一聲1"),
            ("ux", "凶", 2550, "curated", "凵(u)+乂(x)+␣"),
            ("cpn", "區", 2600, "curated", "匚(c)+品(pn)+␣"),
            ("cpn3", "區", 2600, "curated", "匚(c)+品(pn)+三聲3"),
            ("cpnf", "區", 2600, "curated", "匚(c)+品(pn)+三聲f"),
            ("cko3", "區", 2600, "curated", "匚(c)+口(ko)×3"),
            ("ko3", "品", 2600, "curated", "口(ko)×3+␣"),
            ("ko33", "品", 2600, "curated", "口(ko)×3+三聲3"),
            ("ko3f", "品", 2600, "curated", "口(ko)×3+三聲f"),
            ("kokoko", "品", 2660, "curated", "許氏全拼:口(ko)×3"),
            ("koukoukou", "品", 2640, "curated", "拼音全拼:口(kou)×3"),
            ("mu3", "森", 2600, "curated", "木(mu)×3+␣"),
            ("mu31", "森", 2600, "curated", "木(mu)×3+一聲1"),
            ("mxmxmx", "森", 2650, "curated", "許氏全拼:木(mx)×3"),
            ("mumumu", "森", 2640, "curated", "拼音全拼:木(mu)×3"),
            ("mu2", "林", 2600, "curated", "木(mu)×2+␣"),
            ("mu22", "林", 2600, "curated", "木(mu)×2+二聲2"),
            ("mxmx", "林", 2640, "curated", "許氏全拼:木(mx)×2"),
            ("mumu", "林", 2630, "curated", "拼音全拼:木(mu)×2"),
            ("ri3", "晶", 2600, "curated", "日(ri)×3+␣"),
            ("ri31", "晶", 2600, "curated", "日(ri)×3+一聲1"),
            ("rrr", "晶", 2660, "curated", "許氏全拼:日(r)×3"),
            ("ririri", "晶", 2640, "curated", "拼音全拼:日(ri)×3"),
            ("cacaca", "淼", 2660, "curated", "許氏全拼:水(ca)×3"),
            ("hohoho", "焱", 2660, "curated", "許氏全拼:火(ho)×3"),
            ("hoho", "炎", 2640, "curated", "許氏全拼:火(ho)×2"),
            ("jenjenjen", "鑫", 2660, "curated", "許氏全拼:金(jen)×3"),
            ("jnjnjn", "鑫", 2650, "curated", "許氏雙拼:金(jn)×3"),
            ("ccc", "磊", 2660, "curated", "許氏全拼:石(c)×3"),
            ("rnrnrn", "眾", 2660, "curated", "許氏全拼:人(rn)×3"),
            ("rnrn", "从", 2560, "curated", "許氏全拼:人(rn)×2"),
            ("txtxtx", "垚", 2650, "curated", "許氏全拼:土(tx)×3"),
            ("ueue", "朋", 2640, "curated", "許氏全拼:月(ue)×2"),
            ("koko", "回", 2620, "curated", "許氏全拼:口(ko)×2"),
            ("koko", "呂", 2580, "curated", "許氏全拼:口(ko)×2"),
            ("vgvgvg", "轟", 2660, "curated", "許氏全拼:車(vg)×3"),
            ("jjj", "矗", 2660, "curated", "許氏全拼:直(j)×3"),
            ("tli3", "協", 2600, "curated", "十(t)+力(li)×3"),
            ("tle3", "協", 2600, "curated", "十(t)+力(le)×3"),
            ("wsm3", "蕊", 2600, "curated", "艹(w)+心(sm)×3"),
            ("wxn3", "蕊", 2600, "curated", "艹(w)+心(xn)×3"),
            ("lnln", "龍龕", 2520, "phrase2", "首前2+尾後2:龍(ln)+龕(ln)"),
            ("lnho", "龍龕", 2500, "phrase2", "部件詞碼:龍(ln)+龕(ho)"),
            ("jnr4", "形音", 2520, "phrase2", "首前2+尾後2:形(jn)+音(r4)"),
            ("jnin", "形音", 2500, "phrase2", "本音詞碼:形(jn)+音(in)"),
            ("ceru", "輸入", 2520, "phrase2", "首前2+尾後2:輸(ce)+入(ru)"),
            ("suru", "輸入", 2480, "phrase2", "本音詞碼:輸(su)+入(ru)"),
            ("tawn", "台灣", 2520, "phrase2", "首前2+尾後2:台(ta)+灣(wn)"),
            ("tixm", "台灣", 2480, "phrase2", "許氏詞碼:台(ti)+灣(xm)"),
            ("cerci", "輸入法", 2580, "phrase3", "首前2+中1+尾後2:輸(ce)+入(r)+法(ci)"),
            ("cerfa", "輸入法", 2560, "phrase3", "首前2+中1+尾後2:輸(ce)+入(r)+法(fa)"),
            ("lnhjr4", "龍龕形音", 2620, "phrase4", "首前2+中1+尾後2:龍(ln)+龕(h)+形(j)+音(r4)"),
            ("lnhjin", "龍龕形音", 2600, "phrase4", "首前2+中1+尾後2:龍(ln)+龕(h)+形(j)+音(in)"),
            ("lnhjci", "龍龕形音法", 2640, "phrase5", "6碼封頂:龍(ln)+龕(h)+形(j)+…+法(ci)"),
            ("lnhjlci", "龍龕形音法", 2640, "phrase5", "首前2+中1+尾後2:龍(ln)+龕(h)+形(j)+音(l)+法(ci)"),
            ("lnhjlfa", "龍龕形音法", 2620, "phrase5", "首前2+中1+尾後2:龍(ln)+龕(h)+形(j)+音(l)+法(fa)"),
        ];
        for &(code, text, weight, kind, bdesc) in builtins {
            self.insert_entry(code, text, weight, kind, bdesc);
        }
        self.finalize_sorting();
    }

    /// 僅回傳與給定編碼精確匹配之條目（不含前綴填充；供事後部件融合 `Shift+← + Tab` 與精確驗證使用）
    pub fn lookup_exact(&self, code: &str, is_word_mode: bool) -> Vec<LongkanEntry> {
        let clean = code.trim().trim_start_matches('-').to_ascii_lowercase();
        if clean.is_empty() {
            return Vec::new();
        }
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(exact) = self.map.get(&clean) {
            for e in exact {
                let char_len = e.text.chars().count();
                if is_word_mode && char_len < 2 {
                    continue;
                }
                if !is_word_mode && char_len >= 2 {
                    continue;
                }
                if seen.insert(e.text.clone()) {
                    results.push(e.clone());
                }
            }
        }
        results
    }

    pub fn lookup(&self, code: &str) -> Vec<LongkanEntry> {
        let raw_clean = code.trim().to_ascii_lowercase();
        if raw_clean.is_empty() {
            return Vec::new();
        }
        if raw_clean == "-" {
            return ["台灣", "輸入", "龍龕", "形音", "如喜", "智慧", "國家", "電腦"]
                .iter()
                .enumerate()
                .map(|(i, &w)| LongkanEntry {
                    code: "-".to_string(),
                    text: w.to_string(),
                    weight: (2500 - i * 20) as u32,
                    kind: "phrase2".to_string(),
                    breakdown: "常用雙字詞".to_string(),
                })
                .collect();
        }
        let is_word_mode = raw_clean.starts_with('-');
        let clean = raw_clean.trim_start_matches('-').to_string();
        if clean.is_empty() {
            return Vec::new();
        }
        if clean.contains('?') || clean.contains('*') {
            let mut w_res = self.lookup_wildcard(&clean);
            if is_word_mode {
                w_res.retain(|e| e.text.chars().count() >= 2);
            } else if w_res.iter().any(|e| e.text.chars().count() == 1) {
                w_res.retain(|e| e.text.chars().count() == 1);
            }
            return w_res;
        }
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(exact) = self.map.get(&clean) {
            for e in exact {
                let char_len = e.text.chars().count();
                if is_word_mode && char_len < 2 {
                    continue;
                }
                if !is_word_mode && char_len >= 2 {
                    continue;
                }
                if seen.insert(e.text.clone()) {
                    results.push(e.clone());
                }
            }
        }
        // 前綴預覽候選：當精確匹配未滿 15 筆且碼長未達 12 碼上限時，自動補齊後續有效前綴候選（精確匹配永遠排在最前，前綴預覽依權重緊隨其後）
        if clean.len() < 12 && results.len() < 15 {
            let mut prefix_hits = Vec::new();
            for (k, list) in &self.map {
                if k.len() > clean.len() && k.starts_with(&clean) {
                    for e in list.iter() {
                        let char_len = e.text.chars().count();
                        if (is_word_mode && char_len >= 2) || (!is_word_mode && char_len == 1) {
                            prefix_hits.push(e.clone());
                            break;
                        }
                    }
                }
            }
            prefix_hits.sort_by(|a, b| {
                let a_multi3 = is_word_mode && a.text.chars().count() >= 3;
                let b_multi3 = is_word_mode && b.text.chars().count() >= 3;
                b_multi3
                    .cmp(&a_multi3)
                    .then_with(|| b.weight.cmp(&a.weight))
            });
            for e in prefix_hits {
                if seen.insert(e.text.clone()) {
                    results.push(e);
                    if results.len() >= 30 {
                        break;
                    }
                }
            }
        }
        // 無 `-` 前導時：若該碼連前綴預覽都完全沒有任何單字，才保底回傳精確匹配之多字詞
        if !is_word_mode && results.is_empty() {
            if let Some(exact) = self.map.get(&clean) {
                for e in exact {
                    if seen.insert(e.text.clone()) {
                        results.push(e.clone());
                    }
                }
            }
        }
        results
    }

    /// 依單字模式 (`is_word_mode == false`) 或多字詞模式 (`is_word_mode == true`) 嚴格分流檢查給定字串是否為合法完整編碼或有效前綴
    pub fn has_exact_or_prefix_for_mode(&self, probe: &str, is_word_mode: bool) -> bool {
        let clean = probe.trim().trim_start_matches('-').to_ascii_lowercase();
        if clean.is_empty() || clean.len() > 12 {
            return false;
        }
        let matches_mode = |list: &Vec<LongkanEntry>| -> bool {
            list.iter().any(|e| {
                let char_len = e.text.chars().count();
                if is_word_mode {
                    char_len >= 2
                } else {
                    char_len == 1
                }
            })
        };
        if let Some(list) = self.map.get(&clean) {
            if matches_mode(list) {
                return true;
            }
        }
        self.map
            .iter()
            .any(|(k, list)| k.len() > clean.len() && k.starts_with(&clean) && matches_mode(list))
    }

    /// 判斷給定字串是否為字典中已存在的完整編碼或有效前綴（兼顧單字變長全拼與忘按 Tab 之多字詞簡碼）
    pub fn has_exact_or_prefix(&self, probe: &str) -> bool {
        let raw_clean = probe.trim().to_ascii_lowercase();
        let clean = raw_clean.trim_start_matches('-');
        if clean.is_empty() || clean.len() > 12 {
            return false;
        }
        if self.map.contains_key(clean) {
            return true;
        }
        self.map.keys().any(|k| k.starts_with(clean))
    }

    /// 判斷給定字串後是否仍可接續英文字母構成更長的同模式部件編碼
    pub fn has_longer_alpha_prefix(&self, probe: &str) -> bool {
        let raw_clean = probe.trim().to_ascii_lowercase();
        let is_word = raw_clean.starts_with('-');
        let clean = raw_clean.trim_start_matches('-');
        if clean.is_empty() {
            return false;
        }
        self.map.iter().any(|(k, list)| {
            k.len() > clean.len()
                && k.starts_with(clean)
                && k[clean.len()..]
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphabetic())
                    .unwrap_or(false)
                && list.iter().any(|e| {
                    let char_len = e.text.chars().count();
                    if is_word {
                        char_len >= 2
                    } else {
                        char_len == 1
                    }
                })
        })
    }

    pub fn lookup_wildcard(&self, pattern: &str) -> Vec<LongkanEntry> {
        let pat_chars: Vec<char> = pattern.chars().collect();
        let mut hits = Vec::new();
        for (code, entries) in &self.map {
            let code_chars: Vec<char> = code.chars().collect();
            if Self::wildcard_match(&pat_chars, &code_chars) {
                for e in entries {
                    hits.push(e.clone());
                }
            }
        }
        hits.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.text.cmp(&b.text)));
        let mut dedup = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for e in hits {
            if seen.insert(e.text.clone()) {
                dedup.push(e);
                if dedup.len() >= 45 {
                    break;
                }
            }
        }
        dedup
    }

    fn wildcard_match(pat: &[char], text: &[char]) -> bool {
        if pat.is_empty() {
            return text.is_empty();
        }
        if pat[0] == '*' {
            return Self::wildcard_match(&pat[1..], text)
                || (!text.is_empty() && Self::wildcard_match(pat, &text[1..]));
        }
        if !text.is_empty() && (pat[0] == '?' || pat[0] == text[0]) {
            return Self::wildcard_match(&pat[1..], &text[1..]);
        }
        false
    }

    pub fn reverse_lookup_char(&self, ch: char) -> Option<LongkanReverseEntry> {
        self.char_reverse.get(&ch).cloned()
    }

    pub fn reverse_lookup(&self, ch: char) -> Option<LongkanReverseEntry> {
        self.reverse_lookup_char(ch)
    }

    pub fn lookup_compose_candidates(&self, code: &str, limit: usize) -> Vec<String> {
        self.lookup(code)
            .into_iter()
            .take(limit)
            .map(|e| e.text)
            .collect()
    }

    pub fn lookup_phrase_headcode(
        &self,
        code: &str,
        lexicon: &crate::lexicon::LexiconManager,
    ) -> Vec<String> {
        let lower = code.trim().to_ascii_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        if chars.len() < 2 || chars.len() > 4 {
            return Vec::new();
        }
        if chars.iter().any(|&c| !c.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let mut matched: Vec<(String, u32)> = Vec::new();
        for entry in &lexicon.entries {
            let word_chars: Vec<char> = entry.word.chars().collect();
            if word_chars.len() != chars.len() {
                continue;
            }
            let mut ok = true;
            for (wc, &h) in word_chars.iter().zip(chars.iter()) {
                let starts = self
                    .char_reverse
                    .get(wc)
                    .map(|rev| {
                        rev.primary_code.starts_with(h)
                            || rev.alt_codes.iter().any(|ac| ac.starts_with(h))
                    })
                    .unwrap_or(false);
                if !starts {
                    ok = false;
                    break;
                }
            }
            if ok {
                matched.push((entry.word.clone(), entry.weight));
            }
        }
        matched.sort_by(|a, b| b.1.cmp(&a.1));
        matched.dedup_by(|a, b| a.0 == b.0);
        matched.into_iter().take(5).map(|(w, _)| w).collect()
    }

    pub fn format_char_coach(&self, ch: char) -> Option<String> {
        let rev = self.reverse_lookup_char(ch)?;
        if rev.alt_breakdowns.is_empty() {
            Some(format!(
                "{} [{}]",
                rev.primary_code.to_ascii_uppercase(),
                rev.breakdown
            ))
        } else {
            let alt = rev
                .alt_breakdowns
                .iter()
                .take(2)
                .map(|(code, bdesc)| {
                    if bdesc.is_empty() {
                        code.to_ascii_uppercase()
                    } else {
                        format!("{}={}", code.to_ascii_uppercase(), bdesc)
                    }
                })
                .collect::<Vec<_>>()
                .join(" / ");
            Some(format!(
                "{} [{}] (相容推論:{})",
                rev.primary_code.to_ascii_uppercase(),
                rev.breakdown,
                alt
            ))
        }
    }

    pub fn format_text_coach(&self, text: &str) -> String {
        let mut parts = Vec::new();
        for ch in text.chars() {
            if let Some(info) = self.format_char_coach(ch) {
                parts.push(format!("{}: {}", ch, info));
            }
        }
        parts.join(" ｜ ")
    }
}
