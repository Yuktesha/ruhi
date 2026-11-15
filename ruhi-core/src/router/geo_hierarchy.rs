use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use crate::router::lattice::LatticeNode;

const EMBEDDED_CJK_GEO_TSV: &str = include_str!("../../../ruhi_lexicon/data/cjk_geo_hierarchy.tsv");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoEntityMeta {
    pub country: String,
    pub level: String,
    pub parent: String,
    pub entity: String,
    pub postal_code: String,
    pub category: String,
    pub children: Vec<String>,
    pub l1_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPostalMatch {
    pub country: String,
    pub postal_code: String,
    pub matched_hierarchy: Vec<String>,
    pub formatted_with_postal: String,
}

#[derive(Clone)]
struct BmpCharBits([u64; 1024]);

impl BmpCharBits {
    fn new() -> Self {
        Self([0u64; 1024])
    }

    #[inline]
    fn insert(&mut self, ch: char) {
        let cp = ch as u32;
        if cp < 0x10000 {
            self.0[(cp >> 6) as usize] |= 1u64 << (cp & 63);
        }
    }

    #[inline]
    fn contains(&self, ch: char) -> bool {
        let cp = ch as u32;
        if cp < 0x10000 {
            ((self.0[(cp >> 6) as usize] >> (cp & 63)) & 1) != 0
        } else {
            true
        }
    }
}

#[inline]
fn concat_tokens_buf<'a>(tokens: &[&str], stack: &'a mut [u8], heap: &'a mut String) -> &'a str {
    let total_len: usize = tokens.iter().map(|t| t.len()).sum();
    if total_len <= stack.len() {
        let mut pos = 0usize;
        for t in tokens {
            let b = t.as_bytes();
            stack[pos..pos + b.len()].copy_from_slice(b);
            pos += b.len();
        }
        std::str::from_utf8(&stack[..pos]).unwrap_or("")
    } else {
        heap.clear();
        heap.reserve(total_len);
        for t in tokens {
            heap.push_str(t);
        }
        heap.as_str()
    }
}

pub struct GeoPrefixCtx<'g, 'p> {
    pub cleaned_history_buf: [&'p str; 24],
    pub hlen: usize,
    pub active_anchors: Vec<String>,
    pub stem_l1_anchors: Vec<String>,
    pub a2_cands: [(&'g str, &'g str); 32],
    pub a2_len: usize,
    pub a2_overflow: bool,
    pub last_word: &'p str,
    pub prev2_word: &'p str,
    pub is_numeral_end: bool,
    pub prev_is_road_or_section: bool,
    pub allow_building_unit: bool,
}

pub struct CjkGeoHierarchy {
    /// entity name (e.g. "基隆市", "七堵區", "東京都", "江南區") -> list of GeoEntityMeta
    pub entities_by_name: HashMap<String, Vec<GeoEntityMeta>>,
    /// parent_entity -> set of child entities
    pub parent_to_children: HashMap<String, HashSet<String>>,
    /// parent_entity -> ordered list of children for prefix/sub-token matching
    pub parent_children_list: HashMap<String, Vec<String>>,
    /// Ordered list of L1 root entities for character-level prefix matching (e.g. "首"+"爾"+"特別"+"市")
    pub l1_roots_list: Vec<String>,
    /// O(1) L1 membership set
    pub l1_roots_set: HashSet<String>,
    /// O(1) first-char index for L1 roots
    pub l1_roots_by_first_char: HashMap<char, Vec<String>>,
    /// O(1) first-char index for L1 stems (`strip_suffix('市'|'縣')`): first_char -> Vec<(stem, full_l1)>
    pub l1_stems_by_first_char: HashMap<char, Vec<(String, String)>>,
    /// O(1) BMP first-char bitset of all `parent_to_children` keys
    parent_first_bits: BmpCharBits,
    /// O(1) BMP first-char bitset of all `l1_roots_list` entries
    l1_first_bits: BmpCharBits,
    /// O(1) BMP first-char bitset of all L1 stems
    l1_stem_first_bits: BmpCharBits,
    /// O(1) 首字索引：(parent_entity, first_char) -> 子節點清單
    pub children_by_parent_first_char: HashMap<(String, char), Vec<String>>,
    /// O(1) 零配置巢狀首字索引：parent_entity -> first_char -> 子節點清單
    pub children_by_parent_nested: HashMap<String, HashMap<char, Vec<String>>>,
    /// O(1) 首字索引：(l1_root, l2_district, first_char) -> 子節點清單
    pub children_by_l1_l2_first_char: HashMap<(String, String, char), Vec<String>>,
    /// O(1) 首字索引：(l1_root, first_char) -> 路街段清單
    pub roads_by_l1_first_char: HashMap<(String, char), Vec<String>>,
    /// O(1) 零配置巢狀首字索引：l1_root -> first_char -> 路街段清單
    pub roads_by_l1_nested: HashMap<String, HashMap<char, Vec<String>>>,
}

static GLOBAL_CJK_GEO: OnceLock<CjkGeoHierarchy> = OnceLock::new();

impl CjkGeoHierarchy {
    pub fn global() -> &'static CjkGeoHierarchy {
        GLOBAL_CJK_GEO.get_or_init(Self::load_default)
    }

    pub fn load_default() -> Self {
        let candidate_paths = [
            "data/cjk_geo_hierarchy.tsv",
            "../data/cjk_geo_hierarchy.tsv",
            "../../data/cjk_geo_hierarchy.tsv",
            "ruhi_lexicon/data/cjk_geo_hierarchy.tsv",
            "../ruhi_lexicon/data/cjk_geo_hierarchy.tsv",
        ];
        for p in candidate_paths {
            let path = Path::new(p);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if !content.trim().is_empty() {
                        return Self::from_tsv_str(&content);
                    }
                }
            }
        }
        Self::from_tsv_str(EMBEDDED_CJK_GEO_TSV)
    }

    pub fn from_tsv_str(tsv: &str) -> Self {
        let mut entities_by_name: HashMap<String, Vec<GeoEntityMeta>> = HashMap::new();
        let mut parent_to_children: HashMap<String, HashSet<String>> = HashMap::new();
        let mut parent_children_list: HashMap<String, Vec<String>> = HashMap::new();
        let mut l1_roots: HashSet<String> = HashSet::new();
        let mut l1_roots_list: Vec<String> = Vec::new();
        let mut children_by_parent_first_char: HashMap<(String, char), Vec<String>> = HashMap::new();
        let mut children_by_l1_l2_first_char: HashMap<(String, String, char), Vec<String>> = HashMap::new();
        let mut roads_by_l1_first_char: HashMap<(String, char), Vec<String>> = HashMap::new();

        let push_fc = |map: &mut HashMap<(String, char), Vec<String>>, key: (String, char), val: &str| {
            let list = map.entry(key).or_default();
            if !list.iter().any(|x| x == val) {
                list.push(val.to_string());
            }
        };

        let push_l1_l2_fc = |map: &mut HashMap<(String, String, char), Vec<String>>, key: (String, String, char), val: &str| {
            let list = map.entry(key).or_default();
            if !list.iter().any(|x| x == val) {
                list.push(val.to_string());
            }
        };

        for raw_line in tsv.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 6 {
                continue;
            }
            let country = parts[0].trim().to_string();
            let level = parts[1].trim().to_string();
            let parent = parts[2].trim().to_string();
            let entity = parts[3].trim().to_string();
            let postal_code = parts[4].trim().to_string();
            let category = parts[5].trim().to_string();
            let children: Vec<String> = parts
                .get(6)
                .copied()
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let l1_root = parts.get(7).copied().unwrap_or("").trim().to_string();

            if level == "L1" || parent == "ROOT" {
                if l1_roots.insert(entity.clone()) {
                    l1_roots_list.push(entity.clone());
                }
            }

            if !children.is_empty() {
                let child_set = parent_to_children.entry(entity.clone()).or_default();
                let child_vec = parent_children_list.entry(entity.clone()).or_default();
                for ch in &children {
                    if child_set.insert(ch.clone()) {
                        child_vec.push(ch.clone());
                    }
                    if let Some(fc) = ch.chars().next() {
                        push_fc(&mut children_by_parent_first_char, (entity.clone(), fc), ch);
                    }
                }
            }

            if parent != "ROOT" {
                let p_set = parent_to_children.entry(parent.clone()).or_default();
                let p_vec = parent_children_list.entry(parent.clone()).or_default();
                if p_set.insert(entity.clone()) {
                    p_vec.push(entity.clone());
                }
                if let Some(fc) = entity.chars().next() {
                    push_fc(&mut children_by_parent_first_char, (parent.clone(), fc), &entity);
                }
            }

            if level == "L3" && !l1_root.is_empty() {
                if let Some(fc) = entity.chars().next() {
                    push_l1_l2_fc(&mut children_by_l1_l2_first_char, (l1_root.clone(), parent.clone(), fc), &entity);
                    if category == "road" || entity.ends_with('路') || entity.ends_with('街') || entity.ends_with('道') || entity.ends_with('段') {
                        push_fc(&mut roads_by_l1_first_char, (l1_root.clone(), fc), &entity);
                        if l1_root.starts_with('臺') {
                            push_fc(&mut roads_by_l1_first_char, (l1_root.replacen('臺', "台", 1), fc), &entity);
                        } else if l1_root.starts_with('台') {
                            push_fc(&mut roads_by_l1_first_char, (l1_root.replacen('台', "臺", 1), fc), &entity);
                        }
                    }
                }
            }

            entities_by_name
                .entry(entity.clone())
                .or_default()
                .push(GeoEntityMeta {
                    country,
                    level,
                    parent,
                    entity,
                    postal_code,
                    category,
                    children,
                    l1_root,
                });
        }

        let mut l1_roots_by_first_char: HashMap<char, Vec<String>> = HashMap::new();
        let mut l1_stems_by_first_char: HashMap<char, Vec<(String, String)>> = HashMap::new();
        let mut parent_first_bits = BmpCharBits::new();
        let mut l1_first_bits = BmpCharBits::new();
        let mut l1_stem_first_bits = BmpCharBits::new();

        for l1 in &l1_roots_list {
            if let Some(fc) = l1.chars().next() {
                l1_first_bits.insert(fc);
                l1_roots_by_first_char.entry(fc).or_default().push(l1.clone());
            }
            if let Some(stem) = l1.strip_suffix('市').or_else(|| l1.strip_suffix('縣')) {
                if stem.chars().count() >= 2 {
                    if let Some(fc) = stem.chars().next() {
                        l1_stem_first_bits.insert(fc);
                        l1_stems_by_first_char
                            .entry(fc)
                            .or_default()
                            .push((stem.to_string(), l1.clone()));
                    }
                }
            }
        }
        for p in parent_to_children.keys() {
            if let Some(fc) = p.chars().next() {
                parent_first_bits.insert(fc);
            }
        }

        let mut children_by_parent_nested: HashMap<String, HashMap<char, Vec<String>>> = HashMap::new();
        for ((p, ch), list) in &children_by_parent_first_char {
            children_by_parent_nested
                .entry(p.clone())
                .or_default()
                .insert(*ch, list.clone());
        }
        let mut roads_by_l1_nested: HashMap<String, HashMap<char, Vec<String>>> = HashMap::new();
        for ((l1, ch), list) in &roads_by_l1_first_char {
            roads_by_l1_nested
                .entry(l1.clone())
                .or_default()
                .insert(*ch, list.clone());
        }

        Self {
            entities_by_name,
            parent_to_children,
            parent_children_list,
            l1_roots_list,
            l1_roots_set: l1_roots,
            l1_roots_by_first_char,
            l1_stems_by_first_char,
            parent_first_bits,
            l1_first_bits,
            l1_stem_first_bits,
            children_by_parent_first_char,
            children_by_parent_nested,
            children_by_l1_l2_first_char,
            roads_by_l1_first_char,
            roads_by_l1_nested,
        }
    }

    #[inline]
    fn clean_token(s: &str) -> &str {
        s.trim_matches(|c: char| {
            matches!(
                c,
                '，' | '。'
                    | '！'
                    | '？'
                    | '；'
                    | '：'
                    | '、'
                    | '「'
                    | '」'
                    | '『'
                    | '』'
                    | '（'
                    | '）'
                    | '('
                    | ')'
                    | ','
                    | '.'
                    | ' '
            )
        })
    }

    /// Extract active administrative parent anchors from `prefix_words`.
    /// Supports 1-token, 2-token, 3-token, 4-token, and 5-token concatenations
    /// (e.g., `["基隆市"]`, `["七堵", "區"]`, `["首", "爾", "特別", "市"]`, `["濟", "州", "特別", "自", "治", "道"]`).
    pub fn collect_active_anchors(&self, prefix_words: &[&str]) -> Vec<String> {
        let mut active: Vec<String> = Vec::new();
        if prefix_words.is_empty() {
            return active;
        }

        let window_start = prefix_words.len().saturating_sub(24);
        let slice = &prefix_words[window_start..];
        let mut cleaned_buf: [&str; 24] = [""; 24];
        let mut clen = 0usize;
        for w in slice {
            if clen < 24 {
                cleaned_buf[clen] = Self::clean_token(w);
                clen += 1;
            }
        }
        let cleaned = &cleaned_buf[..clen];

        let mut stack_acc = [0u8; 256];
        let mut heap_acc = String::new();
        for i in 0..clen {
            let Some(fc0) = cleaned[i].chars().next() else {
                continue;
            };
            if !self.parent_first_bits.contains(fc0) {
                continue;
            }
            let max_j = (i + 6).min(clen);
            let mut end = i;
            while end < max_j && !cleaned[end].is_empty() {
                end += 1;
                let acc_str = concat_tokens_buf(&cleaned[i..end], &mut stack_acc, &mut heap_acc);
                if self.parent_to_children.contains_key(acc_str) {
                    if !active.iter().any(|x| x == acc_str) {
                        active.push(acc_str.to_string());
                    }
                }
            }
        }

        // Also extract L1/L2 anchors embedded inside committed multi-entity prefix chunks (e.g., "基隆市中正區和憲里")
        let has_l1_first = cleaned
            .iter()
            .any(|t| t.chars().any(|c| self.l1_first_bits.contains(c)));
        if has_l1_first {
            let mut stack_all = [0u8; 512];
            let mut heap_all = String::new();
            let joined_all = concat_tokens_buf(cleaned, &mut stack_all, &mut heap_all);
            if !joined_all.is_empty() {
                let mut seen_fc = ['\0'; 24];
                let mut seen_fc_len = 0usize;
                for ch in joined_all.chars() {
                    if !self.l1_first_bits.contains(ch) {
                        continue;
                    }
                    if seen_fc[..seen_fc_len].contains(&ch) {
                        continue;
                    }
                    if seen_fc_len < seen_fc.len() {
                        seen_fc[seen_fc_len] = ch;
                        seen_fc_len += 1;
                    }
                    if let Some(l1_cands) = self.l1_roots_by_first_char.get(&ch) {
                        for l1 in l1_cands {
                            if joined_all.contains(l1.as_str()) {
                                if !active.iter().any(|x| x == l1) {
                                    active.push(l1.clone());
                                }
                                if let Some(children) = self.parent_children_list.get(l1) {
                                    for ch_ent in children {
                                        if joined_all.contains(ch_ent.as_str())
                                            && self.parent_to_children.contains_key(ch_ent)
                                        {
                                            if !active.iter().any(|x| x == ch_ent) {
                                                active.push(ch_ent.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        active
    }

    /// Check whether the lattice DAG starting at `start_idx` can spell the exact character sequence `target`.
    fn dag_can_spell(
        dag: &HashMap<usize, Vec<LatticeNode>>,
        start_idx: usize,
        target: &str,
    ) -> bool {
        if target.is_empty() {
            return true;
        }
        let Some(nodes) = dag.get(&start_idx) else {
            return false;
        };
        for node in nodes {
            let w = Self::clean_token(&node.word);
            if w.is_empty() {
                continue;
            }
            if let Some(rest) = target.strip_prefix(w) {
                if rest.is_empty() || Self::dag_can_spell(dag, node.end_idx, rest) {
                    return true;
                }
            }
        }
        false
    }

    /// Check whether `dag` starting at `start_idx` can spell `prefix_str` followed immediately by at least one child of `parent_entity`.
    fn dag_can_spell_prefix_then_any_child(
        &self,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        start_idx: usize,
        prefix_str: &str,
        parent_entity: &str,
    ) -> bool {
        if !self.parent_children_list.contains_key(parent_entity) {
            return false;
        }
        if prefix_str.is_empty() {
            let Some(nodes) = dag.get(&start_idx) else {
                return false;
            };
            let Some(by_fc) = self.children_by_parent_nested.get(parent_entity) else {
                return false;
            };
            for node in nodes {
                let w = Self::clean_token(&node.word);
                let Some(ch0) = w.chars().next() else {
                    continue;
                };
                if let Some(cand_children) = by_fc.get(&ch0) {
                    for ch in cand_children {
                        if Self::dag_can_spell(dag, start_idx, ch) {
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        let Some(nodes) = dag.get(&start_idx) else {
            return false;
        };
        for node in nodes {
            let w = Self::clean_token(&node.word);
            if w.is_empty() {
                continue;
            }
            if let Some(rest) = prefix_str.strip_prefix(w) {
                if self.dag_can_spell_prefix_then_any_child(dag, node.end_idx, rest, parent_entity)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if `curr_clean` (at `end_idx` in `dag`, with recent history `history_str`) is on the exact
    /// character path of `entity_str`.
    /// Crucially requires that `curr_clean` aligns with a token boundary in `history_str` (not a random mid-word slice),
    /// or starts fresh at the beginning of `entity_str`.
    fn matches_entity_path(
        history_tokens: &[&str],
        curr_clean: &str,
        end_idx: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        entity_str: &str,
    ) -> Option<bool> {
        // Returns Some(is_completing_token) if `curr_clean` is on the exact path of `entity_str`.
        // 1. Fresh start of `entity_str`: `curr_clean` is either the full `entity_str` or a prefix of `entity_str`
        //    whose remainder is spellable in `dag`.
        if curr_clean == entity_str {
            return Some(true);
        }
        if let Some(rem) = entity_str.strip_prefix(curr_clean) {
            if !rem.is_empty() && Self::dag_can_spell(dag, end_idx, rem) {
                return Some(false);
            }
        }

        // 2. Continuation of `entity_str` from the last 1..=2 tokens of `history_tokens`:
        let n = history_tokens.len();
        let mut stack_pb = [0u8; 192];
        let mut heap_pb = String::new();
        for k in 1..=2.min(n) {
            let prefix_built = concat_tokens_buf(&history_tokens[n - k..], &mut stack_pb, &mut heap_pb);
            if prefix_built.is_empty() {
                continue;
            }
            if let Some(after_hist) = entity_str.strip_prefix(prefix_built) {
                if let Some(rem) = after_hist.strip_prefix(curr_clean) {
                    if rem.is_empty() {
                        return Some(true);
                    }
                    if Self::dag_can_spell(dag, end_idx, rem) {
                        return Some(false);
                    }
                }
            }
        }
        None
    }

    /// Precompute prefix-dependent geo anchors and A2/suffix state once per lookback state.
    pub fn prepare_prefix_ctx<'g, 'p>(&'g self, prefix_words: &[&'p str]) -> GeoPrefixCtx<'g, 'p> {
        let window_start = prefix_words.len().saturating_sub(24);
        let mut cleaned_history_buf: [&'p str; 24] = [""; 24];
        let mut hlen = 0usize;
        for w in &prefix_words[window_start..] {
            let cw = Self::clean_token(w);
            if !cw.is_empty() && hlen < 24 {
                cleaned_history_buf[hlen] = cw;
                hlen += 1;
            }
        }
        let cleaned_history = &cleaned_history_buf[..hlen];

        let mut a2_cands: [(&'g str, &'g str); 32] = [("", ""); 32];
        let mut a2_len = 0usize;
        let mut a2_overflow = false;
        let mut stack_pfx = [0u8; 192];
        let mut heap_pfx = String::new();
        for k in 1..=4.min(hlen) {
            if let Some(h_ch0) = cleaned_history[hlen - k].chars().next() {
                if !self.l1_first_bits.contains(h_ch0) {
                    continue;
                }
                if let Some(l1_cands) = self.l1_roots_by_first_char.get(&h_ch0) {
                    let hist_pfx = concat_tokens_buf(&cleaned_history[hlen - k..], &mut stack_pfx, &mut heap_pfx);
                    for l1 in l1_cands {
                        let l1_str = l1.as_str();
                        if let Some(after_hist) = l1_str.strip_prefix(hist_pfx) {
                            if a2_len < a2_cands.len() {
                                a2_cands[a2_len] = (after_hist, l1_str);
                                a2_len += 1;
                            } else {
                                a2_overflow = true;
                            }
                        }
                    }
                }
            }
        }

        let mut active_anchors = self.collect_active_anchors(prefix_words);
        let mut stem_l1_anchors: Vec<String> = Vec::new();
        let tail_start = hlen.saturating_sub(4);
        let tail_slice = &cleaned_history[tail_start..];
        let has_stem_first = tail_slice
            .iter()
            .any(|t| t.chars().any(|c| self.l1_stem_first_bits.contains(c)));
        if has_stem_first {
            let mut stack_tail = [0u8; 256];
            let mut heap_tail = String::new();
            let recent_tail = concat_tokens_buf(tail_slice, &mut stack_tail, &mut heap_tail);
            if !recent_tail.is_empty() {
                let mut seen_fc = ['\0'; 16];
                let mut seen_fc_len = 0usize;
                for ch in recent_tail.chars() {
                    if !self.l1_stem_first_bits.contains(ch) {
                        continue;
                    }
                    if seen_fc[..seen_fc_len].contains(&ch) {
                        continue;
                    }
                    if seen_fc_len < seen_fc.len() {
                        seen_fc[seen_fc_len] = ch;
                        seen_fc_len += 1;
                    }
                    if let Some(stems) = self.l1_stems_by_first_char.get(&ch) {
                        for (stem, l1) in stems {
                            if recent_tail.contains(stem.as_str())
                                && !active_anchors.contains(l1)
                                && !stem_l1_anchors.contains(l1)
                            {
                                stem_l1_anchors.push(l1.clone());
                            }
                        }
                    }
                }
            }
        }

        if !active_anchors.is_empty() {
            let mut stack_tj = [0u8; 512];
            let mut heap_tj = String::new();
            let tail_joined = concat_tokens_buf(cleaned_history, &mut stack_tj, &mut heap_tj);
            let initial_anchors_len = active_anchors.len();
            for anc_idx in 0..initial_anchors_len {
                let anc = active_anchors[anc_idx].clone();
                if let Some(children) = self.parent_children_list.get(&anc) {
                    for ch in children {
                        if tail_joined.contains(ch.as_str()) && self.parent_to_children.contains_key(ch)
                        {
                            if !active_anchors.contains(ch) {
                                active_anchors.push(ch.clone());
                            }
                        }
                    }
                }
            }
        }

        let last_word = cleaned_history.last().copied().unwrap_or("");
        let prev2_word = if hlen >= 2 {
            cleaned_history[hlen - 2]
        } else {
            ""
        };
        let is_numeral_end = (last_word.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            && last_word.chars().all(|c| c.is_ascii_alphanumeric()))
            || last_word.ends_with(|c: char| {
                matches!(
                    c,
                    '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
                        | '百' | '零' | '〇'
                        | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | '0'
                )
            });
        let is_spatial_closer = matches!(
            last_word,
            "裡" | "內" | "中" | "口" | "旁" | "邊" | "外" | "上" | "下" | "前" | "後"
                | "對面" | "裡面" | "外面" | "附近"
        );
        let prev_is_road_or_section = !is_spatial_closer
            && (last_word.ends_with('路')
                || last_word.ends_with('街')
                || last_word.ends_with("大道")
                || last_word.ends_with('段')
                || last_word.ends_with('里')
                || last_word.ends_with('村')
                || last_word.ends_with('鄰')
                || last_word.ends_with('巷')
                || last_word.ends_with('弄')
                || last_word.ends_with('號')
                || last_word.ends_with('樓')
                || last_word.ends_with('町')
                || last_word.ends_with("丁目")
                || last_word.ends_with('洞')
                || prev2_word.ends_with('路')
                || prev2_word.ends_with('街')
                || prev2_word.ends_with('段')
                || prev2_word.ends_with('里')
                || prev2_word.ends_with('巷')
                || prev2_word.ends_with('弄')
                || prev2_word.ends_with('號'));
        let is_ascii_num = !last_word.is_empty() && last_word.chars().all(|c| c.is_ascii_alphanumeric());
        let allow_building_unit = is_ascii_num || !active_anchors.is_empty() || prev_is_road_or_section;

        GeoPrefixCtx {
            cleaned_history_buf,
            hlen,
            active_anchors,
            stem_l1_anchors,
            a2_cands,
            a2_len,
            a2_overflow,
            last_word,
            prev2_word,
            is_numeral_end,
            prev_is_road_or_section,
            allow_building_unit,
        }
    }

    /// Compute the conditional geo/postal/cadastral/household hierarchy bonus for `curr_word`.
    /// Returns `0.0` immediately when no geo anchor is present in `prefix_words` and `curr_word`
    /// is not initiating a verified L1->L2 geo cascade.
    pub fn score_geo_hierarchy_bonus(
        &self,
        prefix_words: &[&str],
        curr_word: &str,
        curr_bopomofo: &str,
        end_idx: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        let ctx = self.prepare_prefix_ctx(prefix_words);
        self.score_geo_hierarchy_with_ctx(&ctx, curr_word, curr_bopomofo, end_idx, dag)
    }

    pub fn score_geo_hierarchy_with_ctx(
        &self,
        ctx: &GeoPrefixCtx<'_, '_>,
        curr_word: &str,
        _curr_bopomofo: &str,
        end_idx: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        let curr_clean = Self::clean_token(curr_word);
        if curr_clean.is_empty() {
            return 0.0;
        }

        let mut bonus: f64 = 0.0;
        let cleaned_history = &ctx.cleaned_history_buf[..ctx.hlen];

        // ---------------------------------------------------------------------
        // A. Forward Lookahead Activation for L1 Root Anchor
        // ---------------------------------------------------------------------
        let mut matched_a = false;
        if let Some(c0) = curr_clean.chars().next() {
            if self.l1_first_bits.contains(c0) {
                if let Some(l1_cands) = self.l1_roots_by_first_char.get(&c0) {
                    for l1 in l1_cands {
                        let l1_str = l1.as_str();
                        if let Some(rem_l1) = l1_str.strip_prefix(curr_clean) {
                            if self.dag_can_spell_prefix_then_any_child(dag, end_idx, rem_l1, l1_str)
                                || (curr_clean.chars().count() >= 2
                                    && matches!(rem_l1, "市" | "縣")
                                    && self.dag_can_spell_prefix_then_any_child(dag, end_idx, "", l1_str))
                            {
                                bonus += 32.0;
                                matched_a = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if !matched_a {
            if !ctx.a2_overflow {
                for &(after_hist, l1_str) in &ctx.a2_cands[..ctx.a2_len] {
                    if let Some(rem_l1) = after_hist.strip_prefix(curr_clean) {
                        if self.dag_can_spell_prefix_then_any_child(dag, end_idx, rem_l1, l1_str)
                            || (curr_clean.chars().count() >= 2
                                && matches!(rem_l1, "市" | "縣")
                                && self.dag_can_spell_prefix_then_any_child(dag, end_idx, "", l1_str))
                        {
                            bonus += 34.0;
                            break;
                        }
                    }
                }
            } else {
                let hn = cleaned_history.len();
                let mut stack_pfx = [0u8; 192];
                let mut heap_pfx = String::new();
                'a2_outer: for k in 1..=4.min(hn) {
                    if let Some(h_ch0) = cleaned_history[hn - k].chars().next() {
                        if !self.l1_first_bits.contains(h_ch0) {
                            continue;
                        }
                        if let Some(l1_cands) = self.l1_roots_by_first_char.get(&h_ch0) {
                            let hist_pfx = concat_tokens_buf(&cleaned_history[hn - k..], &mut stack_pfx, &mut heap_pfx);
                            for l1 in l1_cands {
                                let l1_str = l1.as_str();
                                if let Some(after_hist) = l1_str.strip_prefix(hist_pfx) {
                                    if let Some(rem_l1) = after_hist.strip_prefix(curr_clean) {
                                        if self.dag_can_spell_prefix_then_any_child(dag, end_idx, rem_l1, l1_str)
                                            || (curr_clean.chars().count() >= 2
                                                && matches!(rem_l1, "市" | "縣")
                                                && self.dag_can_spell_prefix_then_any_child(dag, end_idx, "", l1_str))
                                        {
                                            bonus += 34.0;
                                            break 'a2_outer;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // A-2. Prevent "基隆河" from hijacking "基隆(市)和一路 / 和二路 / 和三路" when 市 is omitted!
        if curr_clean == "基隆河"
            && (Self::dag_can_spell(dag, end_idx, "一路")
                || Self::dag_can_spell(dag, end_idx, "二路")
                || Self::dag_can_spell(dag, end_idx, "三路"))
        {
            bonus -= 35.0;
        }

        let active_anchors = &ctx.active_anchors;
        let stem_l1_anchors = &ctx.stem_l1_anchors;

        let mut matched_child_entity: Option<&str> = None;
        let mut completed_child_entity: Option<&str> = None;

        if !active_anchors.is_empty() || !stem_l1_anchors.is_empty() {
        // ---------------------------------------------------------------------
        // B. Downstream Child Branch Wakeup (L1 -> L2 -> L3 Villages/Streets/Sections/Dongs/Chos)
        // ---------------------------------------------------------------------
        let mut stack_pfx_b = [0u8; 192];
        let mut heap_pfx_b = String::new();
        'anchor_loop: for anchor in stem_l1_anchors.iter().chain(active_anchors.iter()).rev() {
            let cand_children: Vec<&String> = if let Some(ch0) = curr_clean.chars().next() {
                let mut cands = Vec::new();
                let by_fc = self.children_by_parent_nested.get(anchor.as_str());
                if let Some(list) = by_fc.and_then(|m| m.get(&ch0)) {
                    cands.extend(list);
                }
                let hn = cleaned_history.len();
                for k in (1..=2.min(hn)).rev() {
                    let pfx = concat_tokens_buf(&cleaned_history[hn - k..], &mut stack_pfx_b, &mut heap_pfx_b);
                    if let Some(hist_ch0) = pfx.chars().next() {
                        if hist_ch0 != ch0 {
                            if let Some(list) = by_fc.and_then(|m| m.get(&hist_ch0)) {
                                for item in list {
                                    if !cands.contains(&item) {
                                        cands.push(item);
                                    }
                                }
                            }
                        }
                    }
                }
                cands
            } else {
                self.parent_children_list.get(anchor).map(|v| v.iter().collect()).unwrap_or_default()
            };

            for child in cand_children {
                let child_str = child.as_str();
                if let Some(is_complete) = Self::matches_entity_path(
                    cleaned_history,
                    curr_clean,
                    end_idx,
                    dag,
                    child_str,
                ) {
                    bonus += if is_complete { 34.0 } else { 32.0 };
                    matched_child_entity = Some(child_str);
                    if is_complete {
                        completed_child_entity = Some(child_str);
                    }
                    break 'anchor_loop;
                }
            }
        }

        // ---------------------------------------------------------------------
        // B2. District-omitted L1 -> L3 Cascade Wakeup
        // ---------------------------------------------------------------------
        if matched_child_entity.is_none() {
            'l1_skip_loop: for anchor in stem_l1_anchors.iter().chain(active_anchors.iter()).rev() {
                if !self.l1_roots_set.contains(anchor) {
                    continue;
                }
                let cand_roads: Vec<&String> = if let Some(ch0) = curr_clean.chars().next() {
                    let mut cands = Vec::new();
                    let by_fc = self.roads_by_l1_nested.get(anchor.as_str());
                    if let Some(list) = by_fc.and_then(|m| m.get(&ch0)) {
                        cands.extend(list);
                    }
                    let hn = cleaned_history.len();
                    for k in (1..=2.min(hn)).rev() {
                        let pfx = concat_tokens_buf(&cleaned_history[hn - k..], &mut stack_pfx_b, &mut heap_pfx_b);
                        if !anchor.starts_with(pfx) {
                            if let Some(hist_ch0) = pfx.chars().next() {
                                if hist_ch0 != ch0 {
                                    if let Some(list) = by_fc.and_then(|m| m.get(&hist_ch0)) {
                                        for item in list {
                                            if !cands.contains(&item) {
                                                cands.push(item);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cands
                } else {
                    Vec::new()
                };

                for l3 in cand_roads {
                    let l3_str = l3.as_str();
                    if let Some(is_complete) = Self::matches_entity_path(
                        cleaned_history,
                        curr_clean,
                        end_idx,
                        dag,
                        l3_str,
                    ) {
                        bonus += if is_complete { 34.0 } else { 32.0 };
                        matched_child_entity = Some(l3_str);
                        if is_complete {
                            completed_child_entity = Some(l3_str);
                        }
                        break 'l1_skip_loop;
                    }
                }
            }
        }
        }

        let last_word = ctx.last_word;
        let prev2_word = ctx.prev2_word;

        // ---------------------------------------------------------------------
        // C. Cross-Token Boundary Protection AFTER an Address Entity!
        // ---------------------------------------------------------------------
        if !active_anchors.is_empty() || matched_child_entity.is_some() {

        // 1. Protect conjunction "與" (ㄩˇ) vs "語/雨/羽" right after an address entity or inside a geo sentence
        if matches!(curr_clean, "與" | "語" | "雨" | "羽") {
            let prev_is_geo_or_place = completed_child_entity.is_some()
                || last_word.ends_with('路')
                || last_word.ends_with('街')
                || last_word.ends_with("大道")
                || last_word.ends_with("街道")
                || last_word.ends_with('段')
                || last_word.ends_with('里')
                || last_word.ends_with('村')
                || last_word.ends_with('區')
                || last_word.ends_with('市')
                || last_word.ends_with('縣')
                || last_word.ends_with('町')
                || last_word.ends_with('洞')
                || last_word.ends_with('邑')
                || last_word.ends_with('門')
                || last_word.ends_with('內')
                || last_word.ends_with('原')
                || last_word.ends_with('澤')
                || last_word.ends_with('園')
                || last_word.ends_with('山')
                || last_word.ends_with('橋')
                || last_word.ends_with('通')
                || last_word.ends_with('神')
                || last_word.ends_with('環')
                || last_word.ends_with('咀')
                || last_word.ends_with('灣')
                || last_word.ends_with('嶼')
                || last_word.ends_with('梯')
                || last_word.ends_with("演出");
            if prev_is_geo_or_place {
                if curr_clean == "與" {
                    bonus += 32.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 2. Penalize cross-boundary 2-char impostor words when an address anchor is active!
        if matches!(
            curr_clean,
            "露雨"
                | "斷語"
                | "侗語"
                | "接到"
                | "大到"
                | "不屈"
                | "理屈"
                | "普世"
                | "浴室"
                | "只員"
                | "只原"
                | "彈動"
                | "三傾動"
                | "迷蹲到"
                | "當載"
                | "自製"
                | "臨河"
                | "臨海"
                | "臨街"
                | "臨路"
                | "河一"
        ) {
            bonus -= 35.0;
        }
        if curr_clean == "中斷" && last_word == "城" {
            bonus -= 35.0;
        }
        if curr_clean == "時段" && last_word == "鶯歌" {
            bonus -= 35.0;
        }
        if curr_clean == "新版"
            && dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|n| n.word == "段"))
                .unwrap_or(false)
        {
            bonus -= 35.0;
        }
        }

        // 3. Strongly boost multi-char administrative suffixes & street/plaza heads
        if matches!(
            curr_clean,
            "街道"
                | "大道"
                | "特別市"
                | "廣域市"
                | "自治道"
                | "自治區"
                | "特別行政區"
                | "大字"
                | "北側"
                | "南側"
                | "東側"
                | "西側"
                | "街區"
                | "美食街"
                | "老街"
                | "商圈"
                | "外灘"
        ) {
            bonus += 32.0;
        }
        if matches!(curr_clean, "街" | "接" | "皆" | "階")
            && (matches!(last_word, "美食" | "精品" | "商店" | "年貨" | "地下" | "步行" | "老" | "官也" | "也" | "自治")
                || last_word.ends_with("店街")
                || dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|n| matches!(n.word.as_str(), "區" | "頭" | "道" | "坊" | "角" | "景" | "拍" | "口" | "尾")))
                    .unwrap_or(false))
        {
            if curr_clean == "街" {
                bonus += 34.0;
            } else {
                bonus -= 32.0;
            }
        }
        if matches!(curr_clean, "灘" | "攤" | "癱")
            && matches!(last_word, "外" | "沙" | "海" | "淺" | "泥" | "河")
        {
            if curr_clean == "灘" {
                bonus += 34.0;
            } else {
                bonus -= 32.0;
            }
        }

        // ---------------------------------------------------------------------
        // D. Address / Cadastral / Household Structural Slot & Homophone Guard
        // ---------------------------------------------------------------------
        let has_upstream_geo_anchor = !active_anchors.is_empty()
            || !stem_l1_anchors.is_empty()
            || matched_child_entity.is_some()
            || ctx.prev_is_road_or_section;
        if ctx.is_numeral_end && has_upstream_geo_anchor {
            let allow_building_unit = ctx.allow_building_unit;
            if matches!(
                curr_clean,
                "鄰" | "段"
                    | "路"
                    | "街"
                    | "巷"
                    | "弄"
                    | "衖"
                    | "號"
                    | "樓"
                    | "丁目"
                    | "番"
                    | "番地"
                    | "地號"
                    | "建號"
                    | "號樓"
            ) || (allow_building_unit && matches!(curr_clean, "期" | "座" | "室")) {
                bonus += 35.0;
            } else if matches!(
                curr_clean,
                "林" | "臨" | "淋" | "琳" | "麟" | "斷" | "鍛" | "露" | "錄" | "接" | "皆" | "向" | "項" | "好" | "耗" | "流" | "留"
            ) || (allow_building_unit && matches!(curr_clean, "做" | "作" | "其" | "棋"))
                || curr_clean.starts_with('臨')
            {
                bonus -= 35.0;
            }
        }

        if ctx.last_word == "某"
            && (ctx.prev2_word.ends_with('路')
                || ctx.prev2_word.ends_with('街')
                || ctx.prev2_word.ends_with('段')
                || ctx.prev2_word.ends_with('巷')
                || ctx.prev2_word.ends_with('弄')
                || ctx.prev2_word.ends_with('市')
                || ctx.prev2_word.ends_with('區')
                || ctx.prev2_word.ends_with('里'))
        {
            if matches!(curr_clean, "巷" | "弄" | "段" | "號" | "鄰") {
                bonus += 28.0;
            } else if matches!(curr_clean, "項" | "向") {
                bonus -= 20.0;
            }
        }

        if ctx.prev_is_road_or_section {
            if matches!(
                curr_clean,
                "小段"
                    | "地號"
                    | "建號"
                    | "一段"
                    | "二段"
                    | "三段"
                    | "四段"
                    | "五段"
                    | "六段"
                    | "七段"
                    | "八段"
                    | "九段"
                    | "一丁目"
                    | "二丁目"
                    | "三丁目"
                    | "四丁目"
                    | "五丁目"
                    | "一街"
                    | "二街"
                    | "三街"
            ) {
                bonus += 32.0;
            }
        }

        // Cadastral & Household Registration domain terms when an administrative anchor is active
        if matches!(
            curr_clean,
            "戶政事務所"
                | "地政事務所"
                | "區公所"
                | "鄉公所"
                | "鎮公所"
                | "市公所"
                | "土地登記"
                | "建物登記"
                | "地籍圖"
                | "地籍謄本"
                | "土地謄本"
                | "所有權部"
                | "他項權利部"
                | "標示部"
                | "門牌號碼"
                | "門牌整編"
                | "戶籍地址"
                | "通訊地址"
                | "郵遞區號"
                | "界址"
                | "指界"
                | "重測"
                | "町家"
        ) {
            bonus += 30.0;
        }

        // Homophone guard inside active geo hierarchy:
        if (last_word.len() == 1
            && last_word.chars().all(|c| c.is_ascii_alphabetic())
            && (prev2_word.ends_with('樓') || prev2_word.ends_with('層') || prev2_word.ends_with('座')))
            || (last_word.len() >= 2
                && last_word.ends_with(|c: char| c.is_ascii_alphabetic())
                && last_word[..last_word.len() - 1].ends_with(['樓', '層', '座']))
        {
            if curr_clean == "室" {
                bonus += 35.0;
            } else if matches!(curr_clean, "是" | "事" | "市" | "式" | "試" | "視") {
                bonus -= 34.0;
            }
        }
        if matches!(curr_clean, "字" | "自")
            && (last_word == "田代" || prev2_word == "大字" || last_word.ends_with('村'))
        {
            if curr_clean == "字" {
                bonus += 34.0;
            } else {
                bonus -= 32.0;
            }
        }
        if matched_child_entity.is_some() {
            if matches!(
                curr_clean,
                "裡" | "理" | "斷" | "鍛" | "動" | "凍" | "露" | "接" | "挺" | "屈"
            ) {
                bonus -= 32.0;
            }
        }

        bonus.clamp(-35.0, 35.0)
    }

    /// Parse an Arabic, fullwidth, or Chinese numeral string into `u32`.
    fn parse_cjk_or_arabic_u32(s: &str) -> Option<u32> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut ascii_digits = String::new();
        let mut all_digits = true;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                ascii_digits.push(ch);
            } else if ('０'..='９').contains(&ch) {
                let d = (ch as u32 - '０' as u32) as u8 + b'0';
                ascii_digits.push(d as char);
            } else {
                all_digits = false;
                break;
            }
        }
        if all_digits && !ascii_digits.is_empty() {
            return ascii_digits.parse::<u32>().ok();
        }

        // Basic Chinese numeral parser (1..999)
        let mut total = 0u32;
        let mut current = 0u32;
        let mut any = false;
        for ch in trimmed.chars() {
            match ch {
                '零' | '〇' => {
                    any = true;
                }
                '一' => {
                    current = 1;
                    any = true;
                }
                '二' | '兩' => {
                    current = 2;
                    any = true;
                }
                '三' => {
                    current = 3;
                    any = true;
                }
                '四' => {
                    current = 4;
                    any = true;
                }
                '五' => {
                    current = 5;
                    any = true;
                }
                '六' => {
                    current = 6;
                    any = true;
                }
                '七' => {
                    current = 7;
                    any = true;
                }
                '八' => {
                    current = 8;
                    any = true;
                }
                '九' => {
                    current = 9;
                    any = true;
                }
                '十' => {
                    total += if current == 0 { 10 } else { current * 10 };
                    current = 0;
                    any = true;
                }
                '百' => {
                    total += if current == 0 { 100 } else { current * 100 };
                    current = 0;
                    any = true;
                }
                _ => return None,
            }
        }
        if any {
            Some(total + current)
        } else {
            None
        }
    }

    /// Extract the number immediately preceding `unit` (e.g. `'巷'` or `'號'`) in `after_entity`.
    fn extract_number_before_unit(after_entity: &str, unit: char) -> Option<u32> {
        let unit_pos = after_entity.find(unit)?;
        let before = &after_entity[..unit_pos];
        // For `'號'`, if there is a sub-number like `13之1號`, extract the primary number before `'之'`
        let target_slice = if unit == '號' {
            if let Some(zhi_pos) = before.rfind('之') {
                let after_zhi = &before[zhi_pos + '之'.len_utf8()..];
                if Self::parse_cjk_or_arabic_u32(after_zhi).is_some() {
                    &before[..zhi_pos]
                } else {
                    before
                }
            } else {
                before
            }
        } else {
            before
        };

        let num_chars: Vec<char> = target_slice
            .chars()
            .rev()
            .take_while(|&ch| {
                ch.is_ascii_digit()
                    || ('０'..='９').contains(&ch)
                    || matches!(
                        ch,
                        '零' | '〇' | '一' | '二' | '兩' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百'
                    )
            })
            .collect();
        if num_chars.is_empty() {
            return None;
        }
        let num_str: String = num_chars.into_iter().rev().collect();
        Self::parse_cjk_or_arabic_u32(&num_str)
    }

    /// Evaluate a single Chunghwa Post 3+3 delivery range condition (e.g. `"單全"`, `"2巷全"`, `"雙4號以上"`, `"單1號至105號"`).
    fn matches_3plus3_condition(cond: &str, lane: Option<u32>, house_no: Option<u32>) -> bool {
        let cond_clean = cond.split('|').next().unwrap_or(cond);
        let c: String = cond_clean.chars().filter(|ch| !ch.is_whitespace()).collect();
        if c.is_empty() {
            return false;
        }
        if c == "全" {
            return true;
        }

        // 1. Lane-based rule (e.g. "2巷全", "26巷全", "101巷單61號以下", "216巷雙38號以下")
        if c.contains('巷') {
            let Some(l) = lane else {
                return false;
            };
            let (parity_ok, rem) = if let Some(r) = c.strip_prefix('單') {
                (l % 2 == 1, r)
            } else if let Some(r) = c.strip_prefix('雙') {
                (l % 2 == 0, r)
            } else if let Some(r) = c.strip_prefix('連') {
                (true, r)
            } else {
                (true, c.as_str())
            };
            if !parity_ok {
                return false;
            }
            let rem = rem.strip_suffix('全').unwrap_or(rem);
            if let Some(base) = rem.strip_suffix("巷以上") {
                if let Some(start) = Self::parse_cjk_or_arabic_u32(base) {
                    return l >= start;
                }
            } else if let Some(base) = rem.strip_suffix("巷以下") {
                if let Some(end) = Self::parse_cjk_or_arabic_u32(base) {
                    return l <= end;
                }
            } else if let Some((a_str, b_str)) = rem.split_once("巷至") {
                let b_clean = b_str.strip_suffix('巷').unwrap_or(b_str);
                if let (Some(a), Some(b)) = (
                    Self::parse_cjk_or_arabic_u32(a_str),
                    Self::parse_cjk_or_arabic_u32(b_clean),
                ) {
                    return l >= a && l <= b;
                }
            } else if let Some(base) = rem.strip_suffix('巷') {
                if let Some(target) = Self::parse_cjk_or_arabic_u32(base) {
                    return l == target;
                }
            } else if let Some(idx) = rem.find('巷') {
                let lane_part = &rem[..idx];
                let after_lane = &rem[idx + '巷'.len_utf8()..];
                if let Some(target) = Self::parse_cjk_or_arabic_u32(lane_part) {
                    if l != target {
                        return false;
                    }
                    if after_lane.is_empty() || after_lane == "全" {
                        return true;
                    }
                    return Self::matches_3plus3_condition(after_lane, None, house_no);
                }
            }
            return false;
        }

        // 2. Main-road house-number rule (e.g. "單全", "雙全", "雙4號以上", "單1號至105號")
        if lane.is_some() {
            return false;
        }
        let Some(n) = house_no else {
            return false;
        };
        let (parity_ok, rem) = if let Some(r) = c.strip_prefix('單') {
            (n % 2 == 1, r)
        } else if let Some(r) = c.strip_prefix('雙') {
            (n % 2 == 0, r)
        } else if let Some(r) = c.strip_prefix('連') {
            (true, r)
        } else {
            (true, c.as_str())
        };
        if !parity_ok {
            return false;
        }
        if rem == "全" {
            return true;
        }
        if let Some(base) = rem.strip_suffix("號以上") {
            if let Some(start) = Self::parse_cjk_or_arabic_u32(base) {
                return n >= start;
            }
        } else if let Some(base) = rem
            .strip_suffix("號含以下")
            .or_else(|| rem.strip_suffix("號以下"))
        {
            if let Some(end) = Self::parse_cjk_or_arabic_u32(base) {
                return n <= end;
            }
        } else if let Some((a_str, b_str)) = rem.split_once("號至") {
            let b_clean = b_str.strip_suffix('號').unwrap_or(b_str);
            if let (Some(a), Some(b)) = (
                Self::parse_cjk_or_arabic_u32(a_str),
                Self::parse_cjk_or_arabic_u32(b_clean),
            ) {
                return n >= a && n <= b;
            }
        } else if let Some(base) = rem.strip_suffix('號').or_else(|| rem.strip_suffix("號含附號")) {
            if let Some(target) = Self::parse_cjk_or_arabic_u32(base) {
                return n == target;
            }
        }
        false
    }

    /// Resolve a Chunghwa Post 3+3 postal code specification (which may contain delivery range rules
    /// like `"202008:單全;202009:2巷全,雙4號以上"`) against the address suffix `after_entity`.
    /// Returns `(resolved_code, matched_specific_rule)`.
    pub fn resolve_3plus3_delivery_range(postal_spec: &str, after_entity: &str) -> (String, bool) {
        if !postal_spec.contains(':') && !postal_spec.contains(';') {
            return (postal_spec.to_string(), true);
        }
        let lane = Self::extract_number_before_unit(after_entity, '巷');
        let house_no = Self::extract_number_before_unit(after_entity, '號');

        let mut default_code: Option<String> = None;
        let mut first_non_dept_code: Option<String> = None;
        for clause in postal_spec.split(';') {
            let clause = clause.trim();
            if clause.is_empty() {
                continue;
            }
            if let Some((code_part, conds_part)) = clause.split_once(':') {
                let code = code_part.trim();
                let is_dept = conds_part.contains('|');
                if default_code.is_none() {
                    default_code = Some(code.to_string());
                }
                if first_non_dept_code.is_none() && !is_dept {
                    first_non_dept_code = Some(code.to_string());
                }
                for cond in conds_part.split(',') {
                    if Self::matches_3plus3_condition(cond, lane, house_no) {
                        return (code.to_string(), true);
                    }
                }
            } else {
                if default_code.is_none() {
                    default_code = Some(clause.to_string());
                }
                if first_non_dept_code.is_none() {
                    first_non_dept_code = Some(clause.to_string());
                }
            }
        }
        (first_non_dept_code.or(default_code).unwrap_or_else(|| postal_spec.to_string()), false)
    }

    fn text_contains_tw_norm(haystack: &str, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        if haystack.contains(needle) {
            return true;
        }
        if needle.starts_with('臺') && haystack.contains(&needle.replacen('臺', "台", 1)) {
            return true;
        }
        if needle.starts_with('台') && haystack.contains(&needle.replacen('台', "臺", 1)) {
            return true;
        }
        false
    }

    fn text_contains_tw_l1_or_stem(haystack: &str, l1: &str) -> bool {
        if Self::text_contains_tw_norm(haystack, l1) {
            return true;
        }
        if let Some(stem) = l1.strip_suffix('市').or_else(|| l1.strip_suffix('縣')) {
            if stem.chars().count() >= 2 && Self::text_contains_tw_norm(haystack, stem) {
                return true;
            }
        }
        false
    }

    fn normalize_hijacked_tw_road(s: &str) -> std::borrow::Cow<'_, str> {
        for (hijacked, canonical) in [
            ("基隆是河一路", "基隆市和一路"),
            ("基隆是河二路", "基隆市和二路"),
            ("基隆是河三路", "基隆市和三路"),
            ("基隆市河一路", "基隆市和一路"),
            ("基隆市河二路", "基隆市和二路"),
            ("基隆市河三路", "基隆市和三路"),
            ("基隆河一路", "基隆和一路"),
            ("基隆河二路", "基隆和二路"),
            ("基隆河三路", "基隆和三路"),
        ] {
            if s.contains(hijacked) {
                return std::borrow::Cow::Owned(s.replace(hijacked, canonical));
            }
        }
        std::borrow::Cow::Borrowed(s)
    }

    fn has_address_number_suffix(after_entity: &str) -> bool {
        let trimmed = after_entity.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        trimmed.contains('號')
            || trimmed.contains('巷')
            || trimmed.contains('弄')
            || trimmed.contains('樓')
            || trimmed.contains('鄰')
    }

    fn find_l1_grandparents_for_l3(&self, l3_meta: &GeoEntityMeta) -> Vec<String> {
        let mut l1_list = Vec::new();
        if l3_meta.level != "L3" {
            return l1_list;
        }
        if let Some(l2_metas) = self.entities_by_name.get(&l3_meta.parent) {
            for l2_meta in l2_metas {
                if l2_meta.level != "L2" || l2_meta.country != l3_meta.country {
                    continue;
                }
                let owns_l3 = l2_meta.children.iter().any(|c| c == &l3_meta.entity)
                    || (l3_meta.country == "TW"
                        && l2_meta.postal_code.len() >= 3
                        && l3_meta.postal_code.len() >= 3
                        && l2_meta.postal_code[..3] == l3_meta.postal_code[..3]);
                if owns_l3 && l2_meta.parent != "ROOT" && !l1_list.contains(&l2_meta.parent) {
                    l1_list.push(l2_meta.parent.clone());
                }
            }
        }
        l1_list
    }

    /// Resolve the most specific postal code and hierarchy path from a composed sentence or address prefix.
    pub fn resolve_postal_code_for_sentence(&self, sentence: &str) -> Option<GeoPostalMatch> {
        let norm_cow = Self::normalize_hijacked_tw_road(sentence.trim());
        let trimmed = norm_cow.as_ref();
        if trimmed.is_empty() {
            return None;
        }

        let mut best_match: Option<(i32, GeoEntityMeta, Vec<String>, String)> = None;

        // 子字串掃描演算法：以輸入句為基準滑動搜尋 O(L^2)，避免對 53,156 節點全表掃描
        let chars: Vec<char> = trimmed.chars().collect();
        let n = chars.len();
        let mut seen_keys: HashSet<String> = HashSet::new();

        for len in 2..=n.min(16) {
            for i in 0..=(n - len) {
                let sub: String = chars[i..i + len].iter().collect();
                let mut candidate_keys = vec![sub.clone()];
                if sub.contains('台') {
                    candidate_keys.push(sub.replace('台', "臺"));
                } else if sub.contains('臺') {
                    candidate_keys.push(sub.replace('臺', "台"));
                }

                for key in candidate_keys {
                    if let Some(metas) = self.entities_by_name.get(&key) {
                        for meta in metas {
                            if meta.postal_code.is_empty() {
                                continue;
                            }
                            let dedup = format!("{}:{}:{}", meta.country, meta.parent, meta.entity);
                            if !seen_keys.insert(dedup) {
                                continue;
                            }
                            let Some(pos) = trimmed.find(&meta.entity).or_else(|| {
                                if meta.entity.contains('臺') {
                                    trimmed.find(&meta.entity.replace('臺', "台"))
                                } else if meta.entity.contains('台') {
                                    trimmed.find(&meta.entity.replace('台', "臺"))
                                } else {
                                    None
                                }
                            }) else {
                                continue;
                            };

                            let parent_present = meta.parent == "ROOT"
                                || trimmed[..pos].contains(&meta.parent)
                                || (meta.country == "TW"
                                    && meta.parent.starts_with('臺')
                                    && trimmed[..pos].contains(&meta.parent.replacen('臺', "台", 1)))
                                || (meta.country == "TW"
                                    && meta.parent.starts_with('台')
                                    && trimmed[..pos].contains(&meta.parent.replacen('台', "臺", 1)));

                            let is_unique_name = self.entities_by_name.get(&meta.entity).map(|v| v.len()).unwrap_or(0) <= 5;
                            let prefix_before = &trimmed[..pos];
                            let after_entity = &trimmed[pos + meta.entity.len()..];
                            let l1_grandparents = self.find_l1_grandparents_for_l3(meta);
                            let grandparent_present = l1_grandparents
                                .iter()
                                .any(|l1| Self::text_contains_tw_l1_or_stem(prefix_before, l1));
                            let any_l1_in_prefix = self
                                .l1_roots_list
                                .iter()
                                .any(|l1| Self::text_contains_tw_l1_or_stem(prefix_before, l1));
                            let conflicting_l1_present = if meta.level == "L3" {
                                !l1_grandparents.is_empty() && !grandparent_present && any_l1_in_prefix
                            } else if meta.level == "L2" {
                                !parent_present && any_l1_in_prefix
                            } else {
                                false
                            };
                            let conflicting_l2_present = meta.level == "L3"
                                && !parent_present
                                && grandparent_present
                                && l1_grandparents.iter().any(|l1| {
                                    self.parent_children_list.get(l1).map_or(false, |l2s| {
                                        l2s.iter()
                                            .any(|l2| Self::text_contains_tw_norm(prefix_before, l2))
                                    })
                                });
                            let allow_match = if conflicting_l1_present || conflicting_l2_present {
                                false
                            } else if meta.level == "L3" && meta.children.is_empty() {
                                parent_present
                                    || grandparent_present
                                    || (is_unique_name && Self::has_address_number_suffix(after_entity))
                            } else {
                                meta.parent == "ROOT" || parent_present || is_unique_name
                            };

                            if allow_match {
                                let mut chain = Vec::new();
                                if let Some(l1) = l1_grandparents
                                    .iter()
                                    .find(|l1| Self::text_contains_tw_l1_or_stem(prefix_before, l1))
                                {
                                    chain.push(l1.clone());
                                }
                                if meta.parent != "ROOT" && parent_present {
                                    chain.push(meta.parent.clone());
                                }
                                chain.push(meta.entity.clone());

                                for ch in &meta.children {
                                    if after_entity.contains(ch.as_str()) {
                                        chain.push(ch.clone());
                                        break;
                                    }
                                }

                                let (mut resolved_code, mut rule_matched) = Self::resolve_3plus3_delivery_range(
                                    &meta.postal_code,
                                    after_entity,
                                );

                                if !rule_matched && meta.postal_code.contains(':') {
                                    // 若路段未依門牌號碼精確命中，但前綴包含特定村里，且該村里之 3+3 郵遞區號存在於此路段子句中，優先採用該村里對應之投遞區段代碼！
                                    let chars_pb: Vec<char> = prefix_before.chars().collect();
                                    let npb = chars_pb.len();
                                    'village_search: for sub_len in 2..=4.min(npb) {
                                        for st in 0..=(npb - sub_len) {
                                            let sub_name: String = chars_pb[st..st + sub_len].iter().collect();
                                            if let Some(v_metas) = self.entities_by_name.get(&sub_name) {
                                                for vm in v_metas {
                                                    if vm.country == meta.country && vm.level == "L3" && vm.category == "village" && vm.postal_code.len() == 6 && !vm.postal_code.ends_with("000") {
                                                        let target_code = &vm.postal_code;
                                                        if meta.postal_code.split(';').any(|clause| clause.trim().split_once(':').map_or(false, |(c, _)| c.trim() == target_code)) {
                                                            resolved_code = target_code.clone();
                                                            rule_matched = true;
                                                            break 'village_search;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                let rule_bonus: i32 = if meta.country == "TW"
                                    && Self::has_address_number_suffix(after_entity)
                                    && meta.postal_code.contains(':')
                                {
                                    if rule_matched { 120 } else { -90 }
                                } else {
                                    0
                                };

                                let base_spec = match meta.level.as_str() {
                                    "L3" => {
                                        let cat_bonus = match meta.category.as_str() {
                                            "road" => 40,
                                            "cadastral_section" => 20,
                                            _ => 0,
                                        };
                                        let hier_bonus = (if parent_present { 30 } else { 0 })
                                            + (if grandparent_present { 15 } else { 0 });
                                        300 + cat_bonus + hier_bonus + rule_bonus + (chain.len() as i32 * 10) + meta.entity.len() as i32
                                    }
                                    "L2" => {
                                        200 + (if parent_present { 50 } else { 0 })
                                            + (chain.len() as i32 * 10)
                                            + meta.entity.len() as i32
                                    }
                                    _ => 100 + (chain.len() as i32 * 10) + meta.entity.len() as i32,
                                };
                                let specificity = base_spec * 1000 + pos as i32;

                                if best_match
                                    .as_ref()
                                    .map_or(true, |(best_score, _, _, _)| specificity > *best_score)
                                {
                                    best_match = Some((specificity, meta.clone(), chain, resolved_code));
                                }
                            }
                        }
                    }
                }
            }
        }

        let (_, meta, chain, resolved_code) = best_match?;
        if trimmed.starts_with(&resolved_code) {
            return Some(GeoPostalMatch {
                country: meta.country,
                postal_code: resolved_code,
                matched_hierarchy: chain,
                formatted_with_postal: trimmed.to_string(),
            });
        }

        let formatted_with_postal = format!("{} {}", resolved_code, trimmed);
        Some(GeoPostalMatch {
            country: meta.country,
            postal_code: resolved_code,
            matched_hierarchy: chain,
            formatted_with_postal,
        })
    }
    /// 將標記之地址片段展開為補齊「縣市 (`L1`) ＋ 鄉鎮市區 (`L2`) ＋ 村里 (`里/村`)」之戶籍地址候選清單。
    pub fn expand_household_address_candidates(&self, sentence: &str) -> Vec<String> {
        self.expand_household_address_candidates_with_limit(sentence, 4)
    }

    /// 將標記之地址片段展開為補齊「縣市 (`L1`) ＋ 鄉鎮市區 (`L2`) ＋ 村里 (`里/村`)」之戶籍地址候選清單（支援村里數量上限設定）。
    pub fn expand_household_address_candidates_with_limit(&self, sentence: &str, max_villages: usize) -> Vec<String> {
        let norm_cow = Self::normalize_hijacked_tw_road(sentence.trim());
        let trimmed = norm_cow.as_ref();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let mut digit_byte_len = 0;
        let mut digit_count = 0;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit()
                || ('\u{FF10}'..='\u{FF19}').contains(&ch)
                || ch == '-'
                || ch == '〒'
            {
                digit_byte_len += ch.len_utf8();
                if ch.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(&ch) {
                    digit_count += 1;
                }
            } else {
                break;
            }
        }
        let (had_leading_postal, leading_sep, addr_body) =
            if (3..=6).contains(&digit_count) && digit_byte_len < trimmed.len() {
                let rest = &trimmed[digit_byte_len..];
                let trimmed_rest = rest.trim_start();
                let sep = if trimmed_rest.len() < rest.len() {
                    " "
                } else {
                    ""
                };
                (true, sep, trimmed_rest)
            } else {
                (false, "", trimmed)
            };

        if addr_body.is_empty() {
            return Vec::new();
        }

        let mut best_match: Option<(i32, usize, GeoEntityMeta, String)> = None;
        let chars: Vec<char> = addr_body.chars().collect();
        let n = chars.len();
        let mut seen_keys: HashSet<String> = HashSet::new();

        for len in 2..=n.min(16) {
            for i in 0..=(n - len) {
                let sub: String = chars[i..i + len].iter().collect();
                let mut candidate_keys = vec![sub.clone()];
                if sub.contains('台') {
                    candidate_keys.push(sub.replace('台', "臺"));
                } else if sub.contains('臺') {
                    candidate_keys.push(sub.replace('臺', "台"));
                }

                for key in candidate_keys {
                    if let Some(metas) = self.entities_by_name.get(&key) {
                        for meta in metas {
                            if meta.postal_code.is_empty() {
                                continue;
                            }
                            let dedup = format!("{}:{}:{}", meta.country, meta.parent, meta.entity);
                            if !seen_keys.insert(dedup) {
                                continue;
                            }
                            let Some(pos) = addr_body.find(&meta.entity).or_else(|| {
                                if meta.entity.contains('臺') {
                                    addr_body.find(&meta.entity.replace('臺', "台"))
                                } else if meta.entity.contains('台') {
                                    addr_body.find(&meta.entity.replace('台', "臺"))
                                } else {
                                    None
                                }
                            }) else {
                                continue;
                            };

                            let parent_present = meta.parent == "ROOT"
                                || Self::text_contains_tw_l1_or_stem(&addr_body[..pos], &meta.parent);
                            let is_unique_name = self.entities_by_name.get(&meta.entity).map(|v| v.len()).unwrap_or(0) <= 5;
                            let prefix_before = &addr_body[..pos];
                            let after_entity = &addr_body[pos + meta.entity.len()..];
                            let l1_grandparents = self.find_l1_grandparents_for_l3(meta);
                            let grandparent_present = l1_grandparents
                                .iter()
                                .any(|l1| Self::text_contains_tw_l1_or_stem(prefix_before, l1));
                            let any_l1_in_prefix = self
                                .l1_roots_list
                                .iter()
                                .any(|l1| Self::text_contains_tw_l1_or_stem(prefix_before, l1));
                            let conflicting_l1_present = if meta.level == "L3" {
                                !l1_grandparents.is_empty() && !grandparent_present && any_l1_in_prefix
                            } else if meta.level == "L2" {
                                !parent_present && any_l1_in_prefix
                            } else {
                                false
                            };
                            let conflicting_l2_present = meta.level == "L3"
                                && !parent_present
                                && grandparent_present
                                && l1_grandparents.iter().any(|l1| {
                                    self.parent_children_list.get(l1).map_or(false, |l2s| {
                                        l2s.iter()
                                            .any(|l2| Self::text_contains_tw_norm(prefix_before, l2))
                                    })
                                });
                            let allow_match = if conflicting_l1_present || conflicting_l2_present {
                                false
                            } else if meta.level == "L3" && meta.children.is_empty() {
                                parent_present
                                    || grandparent_present
                                    || (is_unique_name && Self::has_address_number_suffix(after_entity))
                            } else {
                                meta.parent == "ROOT" || parent_present || is_unique_name
                            };
                            if allow_match {
                                let (mut resolved_code, mut rule_matched) = Self::resolve_3plus3_delivery_range(
                                    &meta.postal_code,
                                    after_entity,
                                );

                                if !rule_matched && meta.postal_code.contains(':') {
                                    // 若路段未依門牌號碼精確命中，但前綴包含特定村里，且該村里之 3+3 郵遞區號存在於此路段子句中，優先採用該村里對應之投遞區段代碼！
                                    let chars_pb: Vec<char> = prefix_before.chars().collect();
                                    let npb = chars_pb.len();
                                    'village_search: for sub_len in 2..=4.min(npb) {
                                        for st in 0..=(npb - sub_len) {
                                            let sub_name: String = chars_pb[st..st + sub_len].iter().collect();
                                            if let Some(v_metas) = self.entities_by_name.get(&sub_name) {
                                                for vm in v_metas {
                                                    if vm.country == meta.country && vm.level == "L3" && vm.category == "village" && vm.postal_code.len() == 6 && !vm.postal_code.ends_with("000") {
                                                        let target_code = &vm.postal_code;
                                                        if meta.postal_code.split(';').any(|clause| clause.trim().split_once(':').map_or(false, |(c, _)| c.trim() == target_code)) {
                                                            resolved_code = target_code.clone();
                                                            rule_matched = true;
                                                            break 'village_search;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                let rule_bonus: i32 = if meta.country == "TW"
                                    && Self::has_address_number_suffix(after_entity)
                                    && meta.postal_code.contains(':')
                                {
                                    if rule_matched { 120 } else { -90 }
                                } else {
                                    0
                                };

                                let base_spec = match meta.level.as_str() {
                                    "L3" => {
                                        let cat_bonus = match meta.category.as_str() {
                                            "road" => 40,
                                            "cadastral_section" => 20,
                                            _ => 0,
                                        };
                                        let hier_bonus = (if parent_present { 30 } else { 0 })
                                            + (if grandparent_present { 15 } else { 0 });
                                        300 + cat_bonus + hier_bonus + rule_bonus + meta.entity.len() as i32
                                    }
                                    "L2" => 200 + (if parent_present { 50 } else { 0 }) + meta.entity.len() as i32,
                                    _ => 100 + meta.entity.len() as i32,
                                };
                                let specificity = base_spec * 1000 + pos as i32;
                                if best_match
                                    .as_ref()
                                    .map_or(true, |(best_score, _, _, _)| specificity > *best_score)
                                {
                                    best_match = Some((specificity, pos, meta.clone(), resolved_code));
                                }
                            }
                        }
                    }
                }
            }
        }
        let (_, pos, meta, resolved_code) = match best_match {
            Some(m) => m,
            None => return Vec::new(),
        };

        if meta.level != "L3" {
            return Vec::new();
        }

        let prefix_before = &addr_body[..pos];
        let road_and_after = &addr_body[pos..];
        let l2_name = &meta.parent;
        let l1_grandparents = self.find_l1_grandparents_for_l3(&meta);
        let l1_name = l1_grandparents
            .iter()
            .find_map(|l1| {
                if prefix_before.contains(l1.as_str()) {
                    Some(l1.clone())
                } else if l1.starts_with('臺')
                    && prefix_before.contains(&l1.replacen('臺', "台", 1))
                {
                    Some(l1.replacen('臺', "台", 1))
                } else if l1.starts_with('台')
                    && prefix_before.contains(&l1.replacen('台', "臺", 1))
                {
                    Some(l1.replacen('台', "臺", 1))
                } else {
                    None
                }
            })
            .or_else(|| l1_grandparents.first().cloned())
            .unwrap_or_default();

        let existing_lin = if let Some(lin_idx) = prefix_before.find('鄰') {
            let before_lin = &prefix_before[..lin_idx];
            let num_chars: Vec<char> = before_lin
                .chars()
                .rev()
                .take_while(|&c| {
                    c.is_ascii_digit()
                        || ('\u{FF10}'..='\u{FF19}').contains(&c)
                        || matches!(
                            c,
                            '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百'
                        )
                })
                .collect();
            if !num_chars.is_empty() {
                let mut s: String = num_chars.into_iter().rev().collect();
                s.push('鄰');
                s
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let mut cands = Vec::new();
        let push_unique = |list: &mut Vec<String>, item: String| {
            if !item.is_empty() && !list.contains(&item) {
                list.push(item);
            }
        };

        if meta.category == "village" {
            let no_postal = format!("{}{}{}", l1_name, l2_name, road_and_after);
            let with_postal = format!(
                "{}{}{}{}{}",
                resolved_code, leading_sep, l1_name, l2_name, road_and_after
            );
            if had_leading_postal {
                push_unique(&mut cands, with_postal);
                push_unique(&mut cands, no_postal);
            } else {
                push_unique(&mut cands, no_postal);
                push_unique(&mut cands, with_postal);
            }
            return cands;
        }

        let r_stem = meta
            .entity
            .trim_end_matches('路')
            .trim_end_matches('街')
            .trim_end_matches('道')
            .trim_end_matches('段');

        let mut existing_village: Option<String> = None;
        if let Some(l2_children) = self.parent_children_list.get(l2_name) {
            for ch in l2_children {
                if (ch.ends_with('里') || ch.ends_with('村')) && prefix_before.contains(ch.as_str())
                {
                    existing_village = Some(ch.clone());
                    break;
                }
            }
        }

        let mut scored_villages: Vec<(i32, usize, String)> = Vec::new();
        if let Some(l2_metas) = self.entities_by_name.get(l2_name) {
            for l2_m in l2_metas {
                if l2_m.level != "L2" || !Self::text_contains_tw_norm(&l1_name, &l2_m.parent) {
                    continue;
                }
                for (ord_idx, child_name) in l2_m.children.iter().enumerate() {
                    if let Some(c_metas) = self.entities_by_name.get(child_name) {
                        for cm in c_metas {
                            if cm.level == "L3"
                                && cm.parent == *l2_name
                                && cm.category == "village"
                            {
                                let v_stem = cm
                                    .entity
                                    .trim_end_matches('里')
                                    .trim_end_matches('村');
                                let mut score = 0i32;
                                if cm.postal_code == resolved_code {
                                    score += 35;
                                }
                                if !v_stem.is_empty() && v_stem == r_stem {
                                    score += 25;
                                } else if v_stem.chars().next().is_some()
                                    && v_stem.chars().next() == r_stem.chars().next()
                                {
                                    score += 10;
                                }
                                scored_villages.push((score, ord_idx, cm.entity.clone()));
                            }
                        }
                    }
                }
                break;
            }
        }

        scored_villages.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        let mut matched_villages: Vec<String> = Vec::new();
        if let Some(ref ev) = existing_village {
            matched_villages.push(ev.clone());
        }
        let has_high_conf = scored_villages.iter().any(|(s, _, _)| *s >= 25);
        for (s, _, v_name) in &scored_villages {
            if has_high_conf && *s < 25 {
                continue;
            }
            if !matched_villages.contains(v_name) {
                matched_villages.push(v_name.clone());
                if matched_villages.len() >= max_villages {
                    break;
                }
            }
        }

        let no_postal_district = format!("{}{}{}{}", l1_name, l2_name, existing_lin, road_and_after);
        let with_postal_district = format!(
            "{}{}{}{}{}{}",
            resolved_code, leading_sep, l1_name, l2_name, existing_lin, road_and_after
        );

        if had_leading_postal {
            for v in &matched_villages {
                push_unique(
                    &mut cands,
                    format!(
                        "{}{}{}{}{}{}{}",
                        resolved_code, leading_sep, l1_name, l2_name, v, existing_lin, road_and_after
                    ),
                );
            }
            push_unique(&mut cands, with_postal_district);
            for v in &matched_villages {
                push_unique(
                    &mut cands,
                    format!("{}{}{}{}{}", l1_name, l2_name, v, existing_lin, road_and_after),
                );
            }
        } else {
            for v in &matched_villages {
                push_unique(
                    &mut cands,
                    format!("{}{}{}{}{}", l1_name, l2_name, v, existing_lin, road_and_after),
                );
            }
            if no_postal_district != addr_body {
                push_unique(&mut cands, no_postal_district);
            }
            for v in &matched_villages {
                push_unique(
                    &mut cands,
                    format!(
                        "{}{}{}{}{}{}{}",
                        resolved_code, leading_sep, l1_name, l2_name, v, existing_lin, road_and_after
                    ),
                );
            }
            push_unique(&mut cands, with_postal_district);
        }

        cands
    }

    fn find_exact_road_match(
        &self,
        opt_l1: Option<&str>,
        opt_l2: Option<&str>,
        road_name: &str,
    ) -> Option<Vec<GeoEntityMeta>> {
        let metas = self.entities_by_name.get(road_name)?;
        let mut matched = Vec::new();
        for m in metas {
            if m.level == "L3" && (m.category == "road" || m.category.is_empty()) {
                if let Some(l2) = opt_l2 {
                    if m.parent == l2 {
                        matched.push(m.clone());
                    }
                } else if let Some(l1) = opt_l1 {
                    let grandparents = self.find_l1_grandparents_for_l3(m);
                    if grandparents.iter().any(|gp| Self::text_contains_tw_norm(l1, gp) || Self::text_contains_tw_norm(gp, l1)) {
                        matched.push(m.clone());
                    }
                } else {
                    matched.push(m.clone());
                }
            }
        }
        if !matched.is_empty() {
            Some(matched)
        } else {
            None
        }
    }

    fn find_postal_specs_for_road(
        &self,
        opt_l1: Option<&str>,
        opt_l2: Option<&str>,
        road_name: &str,
    ) -> Vec<String> {
        let mut specs = Vec::new();
        if let Some(metas) = self.find_exact_road_match(opt_l1, opt_l2, road_name) {
            for m in metas {
                if !m.postal_code.is_empty() && !specs.contains(&m.postal_code) {
                    specs.push(m.postal_code);
                }
            }
        }
        specs
    }

    fn find_longer_road_suffixes(
        &self,
        opt_l1: Option<&str>,
        opt_l2: Option<&str>,
        road_prefix: &str,
    ) -> Vec<String> {
        let mut suffixes = Vec::new();
        if road_prefix.is_empty() {
            return suffixes;
        }

        if let Some(l2) = opt_l2 {
            if let Some(children) = self.parent_children_list.get(l2) {
                for ch in children {
                    if ch.starts_with(road_prefix) && ch.len() > road_prefix.len() {
                        let sfx = ch[road_prefix.len()..].to_string();
                        if !suffixes.contains(&sfx) {
                            suffixes.push(sfx);
                        }
                    }
                }
            }
        } else if let Some(l1) = opt_l1 {
            if let Some(fc) = road_prefix.chars().next() {
                let l1_norm = l1.to_string();
                if let Some(roads) = self.roads_by_l1_first_char.get(&(l1_norm.clone(), fc)) {
                    for r in roads {
                        if r.starts_with(road_prefix) && r.len() > road_prefix.len() {
                            let sfx = r[road_prefix.len()..].to_string();
                            if !suffixes.contains(&sfx) {
                                suffixes.push(sfx);
                            }
                        }
                    }
                }
            }
        }
        suffixes
    }

    fn push_unique_address_cand(cands: &mut Vec<String>, item: String) {
        if !item.is_empty() && !cands.contains(&item) {
            cands.push(item);
        }
    }

    /// 解析地址輸入狀態並產生 Tab 鍵聯想動作（支援階層路名／段／巷／弄續寫與門牌 3+3 郵遞區號／完整戶籍地址補完）
    pub fn resolve_address_associative_action(
        &self,
        chars: &[char],
        cur: usize,
    ) -> Option<((usize, usize), Vec<String>)> {
        let p_end = cur.min(chars.len());
        if p_end == 0 {
            return None;
        }

        let is_addr_char = |c: char| -> bool {
            if c.is_whitespace() {
                return false;
            }
            if matches!(
                c,
                '：' | ':' | ',' | '，' | '。' | '！' | '!' | '？' | '?' | '（' | '）'
                    | '(' | ')' | '「' | '」' | '『' | '』' | '、' | '；' | ';' | '\n' | '\r'
            ) {
                return false;
            }
            true
        };

        let mut raw_start = p_end;
        while raw_start > 0 && is_addr_char(chars[raw_start - 1]) {
            raw_start -= 1;
        }
        if raw_start == p_end {
            return None;
        }

        let raw_window: String = chars[raw_start..p_end].iter().collect();

        // 尋找最早出現的台灣／CJK 地址錨點起點 addr_start
        // 1. 先找任何 l1 in self.l1_roots_list（支援 台 ⇄ 臺）
        let mut best_l1_match: Option<(usize, String)> = None;
        for l1 in &self.l1_roots_list {
            let l1_norm = l1.clone();
            let mut variants = vec![l1_norm];
            if l1.contains('臺') {
                variants.push(l1.replace('臺', "台"));
            } else if l1.contains('台') {
                variants.push(l1.replace('台', "臺"));
            }

            for v in variants {
                if let Some(byte_pos) = raw_window.find(&v) {
                    let char_pos = raw_window[..byte_pos].chars().count();
                    let abs_char_pos = raw_start + char_pos;
                    if best_l1_match.as_ref().map_or(true, |(p, _)| abs_char_pos < *p) {
                        best_l1_match = Some((abs_char_pos, v));
                    }
                }
            }
        }

        let addr_start = if let Some((mut pos, _)) = best_l1_match {
            // 若 l1 前方緊鄰 3~6 碼郵遞區號數字，將數字一併納入 addr_start
            let mut digit_start = pos;
            while digit_start > raw_start
                && (chars[digit_start - 1].is_ascii_digit()
                    || ('\u{FF10}'..='\u{FF19}').contains(&chars[digit_start - 1])
                    || chars[digit_start - 1] == '-')
            {
                digit_start -= 1;
            }
            let digit_count = chars[digit_start..pos]
                .iter()
                .filter(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(c))
                .count();
            if (3..=6).contains(&digit_count) {
                pos = digit_start;
            }
            pos
        } else {
            // 若無 l1，僅在 raw_window 開頭即為明確唯一 L2 行政區（如 "大安區", "板橋區", "七堵區"）時採用 raw_start
            let mut matched_l2 = false;
            for (name, metas) in &self.entities_by_name {
                if metas.iter().any(|m| m.level == "L2") && raw_window.starts_with(name) {
                    matched_l2 = true;
                    break;
                }
            }
            if matched_l2 {
                raw_start
            } else {
                return None;
            }
        };

        let addr_slice: String = chars[addr_start..p_end].iter().collect();
        if addr_slice.is_empty() {
            return None;
        }

        // 檢查情境 A: 門牌號碼已輸入至「號」結尾（且尚未以 6 碼郵遞區號開頭）
        let has_leading_6digit_postal = {
            let digit_count = addr_slice
                .chars()
                .take(6)
                .filter(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(c))
                .count();
            digit_count == 6
        };

        let ends_with_house_number = if addr_slice.ends_with('號') {
            let chars_as: Vec<char> = addr_slice.chars().collect();
            let n_as = chars_as.len();
            if n_as >= 2 {
                let prev_c = chars_as[n_as - 2];
                prev_c.is_ascii_digit()
                    || ('\u{FF10}'..='\u{FF19}').contains(&prev_c)
                    || matches!(
                        prev_c,
                        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '-'
                    )
            } else {
                false
            }
        } else {
            false
        };

        if ends_with_house_number && !has_leading_6digit_postal {
            // 情境 A — 門牌號碼已輸入至「號」結尾
            let replace_range = (addr_start, p_end);
            let mut cands = Vec::new();

            let opt_postal = self.resolve_postal_code_for_sentence(&addr_slice);
            let hh_cands = self.expand_household_address_candidates_with_limit(&addr_slice, 12);

            if let Some(pm) = opt_postal {
                let postal_code = pm.postal_code;
                // 檢查 addr_slice 是否包含 L2 行政區
                let has_l2_in_slice = self.parent_children_list.values().any(|children| {
                    children.iter().any(|c| (c.ends_with('區') || c.ends_with('市') || c.ends_with('鎮') || c.ends_with('鄉')) && addr_slice.contains(c.as_str()))
                });

                if !has_l2_in_slice {
                    // 省略行政區（如 "台北市忠孝東路四段295號"），從 hh_cands 尋找補齊行政區之項目
                    let filled_with_postal = hh_cands.iter().find(|s| s.starts_with(&postal_code) && !s.contains('里') && !s.contains('村')).cloned();
                    let filled_no_postal = hh_cands.iter().find(|s| !s.starts_with(&postal_code) && !s.contains('里') && !s.contains('村')).cloned();

                    if let Some(fwp) = filled_with_postal {
                        Self::push_unique_address_cand(&mut cands, fwp);
                    } else {
                        Self::push_unique_address_cand(&mut cands, format!("{}{}", postal_code, addr_slice));
                    }
                    Self::push_unique_address_cand(&mut cands, format!("{}{}", postal_code, addr_slice));
                    if let Some(fnp) = filled_no_postal {
                        Self::push_unique_address_cand(&mut cands, fnp);
                    }
                } else {
                    Self::push_unique_address_cand(&mut cands, format!("{}{}", postal_code, addr_slice));
                }

                // 放入前 3 筆帶郵遞區號之完整戶籍地址與不帶郵遞區號之戶籍地址
                for h in &hh_cands {
                    if h.starts_with(&postal_code) {
                        Self::push_unique_address_cand(&mut cands, h.clone());
                        if cands.len() >= 4 {
                            break;
                        }
                    }
                }
                for h in &hh_cands {
                    if !h.starts_with(&postal_code) {
                        Self::push_unique_address_cand(&mut cands, h.clone());
                        if cands.len() >= 5 {
                            break;
                        }
                    }
                }
            }

            // 樓層／之號續寫
            Self::push_unique_address_cand(&mut cands, format!("{}1樓", addr_slice));
            Self::push_unique_address_cand(&mut cands, format!("{}2樓", addr_slice));
            Self::push_unique_address_cand(&mut cands, format!("{}3樓", addr_slice));
            Self::push_unique_address_cand(&mut cands, format!("{}之", addr_slice));

            // 其餘完整戶籍地址與其他樓層
            for h in hh_cands {
                Self::push_unique_address_cand(&mut cands, h);
            }
            Self::push_unique_address_cand(&mut cands, format!("{}4樓", addr_slice));
            Self::push_unique_address_cand(&mut cands, format!("{}5樓", addr_slice));
            Self::push_unique_address_cand(&mut cands, format!("{}地下1樓", addr_slice));

            return Some((replace_range, cands));
        }

        // 情境 B — 地址階層／路名／段／巷／弄漸進式續寫
        let replace_range = (p_end, p_end);
        let mut cands = Vec::new();

        // 解析 addr_slice 的結構 (opt_l1, opt_l2, opt_village, after_tail)
        let (opt_l1, rem_after_l1) = {
            let mut found_l1 = None;
            for l1 in &self.l1_roots_list {
                if addr_slice.starts_with(l1.as_str()) {
                    found_l1 = Some((l1.clone(), &addr_slice[l1.len()..]));
                    break;
                } else if l1.contains('臺') && addr_slice.starts_with(&l1.replace('臺', "台")) {
                    let v = l1.replace('臺', "台");
                    found_l1 = Some((l1.clone(), &addr_slice[v.len()..]));
                    break;
                } else if l1.contains('台') && addr_slice.starts_with(&l1.replace('台', "臺")) {
                    let v = l1.replace('台', "臺");
                    found_l1 = Some((l1.clone(), &addr_slice[v.len()..]));
                    break;
                }
            }
            if let Some((l1, rem)) = found_l1 {
                (Some(l1), rem)
            } else {
                (None, addr_slice.as_str())
            }
        };

        // 1. 剛輸入完 L1（如 "台北市"，rem_after_l1 為空）
        if let Some(ref l1) = opt_l1 {
            if rem_after_l1.is_empty() {
                if let Some(l2_list) = self.parent_children_list.get(l1) {
                    for l2 in l2_list {
                        Self::push_unique_address_cand(&mut cands, l2.clone());
                    }
                }
                return Some((replace_range, cands));
            }
        }

        // 檢查是否輸入了部分 L2 前綴，或完整 L2
        let (opt_l2, rem_after_l2) = if let Some(ref l1) = opt_l1 {
            let mut matched_l2 = None;
            if let Some(l2_list) = self.parent_children_list.get(l1) {
                for l2 in l2_list {
                    if rem_after_l1.starts_with(l2.as_str()) {
                        matched_l2 = Some((l2.clone(), &rem_after_l1[l2.len()..]));
                        break;
                    }
                }
            }
            if let Some(res) = matched_l2 {
                (Some(res.0), res.1)
            } else {
                (None, rem_after_l1)
            }
        } else {
            // 無 L1，檢查是否直接以 L2 開頭
            let mut matched_l2 = None;
            for (name, metas) in &self.entities_by_name {
                if metas.iter().any(|m| m.level == "L2") && rem_after_l1.starts_with(name.as_str()) {
                    matched_l2 = Some((name.clone(), &rem_after_l1[name.len()..]));
                    break;
                }
            }
            if let Some(res) = matched_l2 {
                (Some(res.0), res.1)
            } else {
                (None, rem_after_l1)
            }
        };

        // 2. L1 + 部分 L2 前綴（如 "台北市大安" -> ["區"]；"台北市大" -> ["同區", "安區"]）
        if opt_l2.is_none() && opt_l1.is_some() {
            let l1 = opt_l1.as_ref().unwrap();
            let mut prefix_matched_l2 = Vec::new();
            if let Some(l2_list) = self.parent_children_list.get(l1) {
                for l2 in l2_list {
                    if l2.starts_with(rem_after_l1) && l2.len() > rem_after_l1.len() {
                        prefix_matched_l2.push(l2[rem_after_l1.len()..].to_string());
                    }
                }
            }
            if !prefix_matched_l2.is_empty() {
                for suffix in prefix_matched_l2 {
                    Self::push_unique_address_cand(&mut cands, suffix);
                }
                // 同時檢查是否有省略行政區之路名以該前綴開頭
                if let Some(fc) = rem_after_l1.chars().next() {
                    if let Some(roads) = self.roads_by_l1_first_char.get(&(l1.clone(), fc)) {
                        for r in roads {
                            if r.starts_with(rem_after_l1) && r.len() > rem_after_l1.len() {
                                Self::push_unique_address_cand(&mut cands, r[rem_after_l1.len()..].to_string());
                            }
                        }
                    }
                }
                return Some((replace_range, cands));
            }
        }

        // 3. 剛輸入完 L1 + L2（如 "台北市大安區"，rem_after_l2 為空）
        if let Some(ref l2) = opt_l2 {
            if rem_after_l2.is_empty() {
                if let Some(children) = self.parent_children_list.get(l2) {
                    let mut roads_da_dao = Vec::new();
                    let mut roads_lu_duan = Vec::new();
                    let mut roads_jie = Vec::new();
                    let mut villages = Vec::new();

                    for ch in children {
                        if ch.ends_with('里') || ch.ends_with('村') {
                            villages.push(ch.clone());
                        } else if ch.contains("大道") {
                            roads_da_dao.push(ch.clone());
                        } else if ch.contains('路') || ch.contains('段') {
                            roads_lu_duan.push(ch.clone());
                        } else if ch.contains('街') {
                            roads_jie.push(ch.clone());
                        } else {
                            roads_lu_duan.push(ch.clone());
                        }
                    }

                    for r in roads_da_dao {
                        Self::push_unique_address_cand(&mut cands, r);
                    }
                    for r in roads_lu_duan.into_iter().take(25) {
                        Self::push_unique_address_cand(&mut cands, r);
                    }
                    for r in roads_jie.into_iter().take(10) {
                        Self::push_unique_address_cand(&mut cands, r);
                    }
                    for v in villages.into_iter().take(15) {
                        Self::push_unique_address_cand(&mut cands, v);
                    }
                }
                return Some((replace_range, cands));
            }
        }

        // 4. 剛輸入完村里或鄰（如 "台北市大安區華聲里" 或 "台北市大安區華聲里11鄰"）
        let (opt_village, rem_after_village) = if let Some(ref l2) = opt_l2 {
            let mut matched_v = None;
            if let Some(children) = self.parent_children_list.get(l2) {
                for ch in children {
                    if (ch.ends_with('里') || ch.ends_with('村')) && rem_after_l2.starts_with(ch.as_str()) {
                        matched_v = Some((ch.clone(), &rem_after_l2[ch.len()..]));
                        break;
                    }
                }
            }
            if let Some(res) = matched_v {
                (Some(res.0), res.1)
            } else {
                (None, rem_after_l2)
            }
        } else {
            (None, rem_after_l2)
        };

        if let Some(ref _v) = opt_village {
            // 檢查是否停在村里結尾、村里後純數字、或村里+數字鄰
            let rem_v_chars: Vec<char> = rem_after_village.chars().collect();
            if rem_after_village.is_empty() {
                Self::push_unique_address_cand(&mut cands, "鄰".to_string());
                Self::push_unique_address_cand(&mut cands, "1鄰".to_string());
                if let Some(ref l2) = opt_l2 {
                    if let Some(children) = self.parent_children_list.get(l2) {
                        for ch in children {
                            if !ch.ends_with('里') && !ch.ends_with('村') {
                                Self::push_unique_address_cand(&mut cands, ch.clone());
                                if cands.len() >= 30 {
                                    break;
                                }
                            }
                        }
                    }
                }
                return Some((replace_range, cands));
            } else if rem_v_chars.iter().all(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(c) || matches!(c, '一'..='九' | '十')) {
                Self::push_unique_address_cand(&mut cands, "鄰".to_string());
                return Some((replace_range, cands));
            } else if rem_after_village.ends_with('鄰') {
                if let Some(ref l2) = opt_l2 {
                    if let Some(children) = self.parent_children_list.get(l2) {
                        for ch in children {
                            if !ch.ends_with('里') && !ch.ends_with('村') {
                                Self::push_unique_address_cand(&mut cands, ch.clone());
                                if cands.len() >= 30 {
                                    break;
                                }
                            }
                        }
                    }
                }
                return Some((replace_range, cands));
            }
        }

        // 5~9. 路街段、巷、弄、門牌續寫
        let road_candidate_text = if opt_village.is_some() {
            // 若有村里，剝除村里與鄰之後的部分作為路街分析
            let mut t = rem_after_village;
            if let Some(idx) = t.find('鄰') {
                t = &t[idx + '鄰'.len_utf8()..];
            }
            t
        } else {
            rem_after_l2
        };

        // 9. 門牌後尾端數字或之號續寫（如 "...295號1" -> ["樓", "樓之1", "樓之2"]；"...295號之" -> ["1", "2", "3"]）
        if let Some(idx) = road_candidate_text.rfind('號') {
            let tail_after_hao = &road_candidate_text[idx + '號'.len_utf8()..];
            if tail_after_hao.starts_with('之') {
                let tail_after_zhi = &tail_after_hao['之'.len_utf8()..];
                if tail_after_zhi.is_empty() {
                    Self::push_unique_address_cand(&mut cands, "1".to_string());
                    Self::push_unique_address_cand(&mut cands, "2".to_string());
                    Self::push_unique_address_cand(&mut cands, "3".to_string());
                    return Some((replace_range, cands));
                }
            } else if !tail_after_hao.is_empty() && tail_after_hao.chars().all(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(&c)) {
                Self::push_unique_address_cand(&mut cands, "樓".to_string());
                Self::push_unique_address_cand(&mut cands, "樓之1".to_string());
                Self::push_unique_address_cand(&mut cands, "樓之2".to_string());
                return Some((replace_range, cands));
            }
        }

        // 8. 已輸入路街段巷弄＋尾端純數字（如 "...忠孝東路四段295"，尚未打「號／巷／弄」）
        let trailing_digits: String = road_candidate_text
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(c) || matches!(c, '一'..='九' | '十' | '百'))
            .collect();
        if !trailing_digits.is_empty() && trailing_digits.chars().count() < road_candidate_text.chars().count() {
            let prefix_before_digits = &road_candidate_text[..road_candidate_text.len() - trailing_digits.len()];
            if prefix_before_digits.ends_with('段')
                || prefix_before_digits.ends_with('路')
                || prefix_before_digits.ends_with('街')
                || prefix_before_digits.ends_with('巷')
                || prefix_before_digits.ends_with('弄')
                || prefix_before_digits.ends_with("大道")
            {
                Self::push_unique_address_cand(&mut cands, "號".to_string());
                Self::push_unique_address_cand(&mut cands, "巷".to_string());
                Self::push_unique_address_cand(&mut cands, "弄".to_string());
                Self::push_unique_address_cand(&mut cands, "號1樓".to_string());
                Self::push_unique_address_cand(&mut cands, "號2樓".to_string());
                Self::push_unique_address_cand(&mut cands, "號3樓".to_string());
                return Some((replace_range, cands));
            }
        }

        // 7. 已輸入 <路街段><N>巷（如 "...忠孝東路四段216巷"）
        if let Some(lane_idx) = road_candidate_text.rfind('巷') {
            let road_before_lane = &road_candidate_text[..lane_idx];
            let lane_text = &road_candidate_text[..lane_idx + '巷'.len_utf8()];
            // 擷取巷號數字
            let lane_num = Self::extract_number_before_unit(lane_text, '巷');
            if let Some(ln) = lane_num {
                // 找出該路段對應的 L3 實體之 postal_spec
                let matched_postal_specs = self.find_postal_specs_for_road(opt_l1.as_deref(), opt_l2.as_deref(), road_before_lane);
                let mut nongs = Vec::new();
                for spec in matched_postal_specs {
                    for clause in spec.split(';') {
                        if clause.contains(&format!("{}巷", ln)) {
                            // 擷取 (\d+)弄
                            let mut cur_num = 0u32;
                            let mut has_digit = false;
                            for c in clause.chars() {
                                if c.is_ascii_digit() {
                                    cur_num = cur_num * 10 + (c as u32 - '0' as u32);
                                    has_digit = true;
                                } else if c == '弄' {
                                    if has_digit && cur_num > 0 && !nongs.contains(&cur_num) {
                                        nongs.push(cur_num);
                                    }
                                    cur_num = 0;
                                    has_digit = false;
                                } else {
                                    cur_num = 0;
                                    has_digit = false;
                                }
                            }
                        }
                    }
                }
                nongs.sort_unstable();
                Self::push_unique_address_cand(&mut cands, "號".to_string());
                Self::push_unique_address_cand(&mut cands, "弄".to_string());
                for ng in nongs {
                    Self::push_unique_address_cand(&mut cands, format!("{}弄", ng));
                }
                return Some((replace_range, cands));
            }
        }

        // 6. 剛輸入完完整 L3 路／街／段（如 "台北市大安區忠孝東路四段" 或 "台北市忠孝東路四段"）
        let exact_road_match = self.find_exact_road_match(opt_l1.as_deref(), opt_l2.as_deref(), road_candidate_text);
        if let Some(matched_metas) = exact_road_match {
            // 若同時有更長之同前綴路名（如段數），先放段數後綴
            let longer_suffixes = self.find_longer_road_suffixes(opt_l1.as_deref(), opt_l2.as_deref(), road_candidate_text);
            for sfx in longer_suffixes {
                Self::push_unique_address_cand(&mut cands, sfx);
            }

            // 擷取真實巷號清單
            let mut lanes = Vec::new();
            for meta in &matched_metas {
                for clause in meta.postal_code.split(';') {
                    let mut cur_num = 0u32;
                    let mut has_digit = false;
                    for c in clause.chars() {
                        if c.is_ascii_digit() {
                            cur_num = cur_num * 10 + (c as u32 - '0' as u32);
                            has_digit = true;
                        } else if c == '巷' {
                            if has_digit && cur_num > 0 && !lanes.contains(&cur_num) {
                                lanes.push(cur_num);
                            }
                            cur_num = 0;
                            has_digit = false;
                        } else {
                            cur_num = 0;
                            has_digit = false;
                        }
                    }
                }
            }
            lanes.sort_unstable();

            Self::push_unique_address_cand(&mut cands, "號".to_string());
            Self::push_unique_address_cand(&mut cands, "巷".to_string());
            for ln in lanes {
                Self::push_unique_address_cand(&mut cands, format!("{}巷", ln));
            }
            return Some((replace_range, cands));
        }

        // 5. 已輸入部分路名／村里前綴（如 "台北市大安區忠孝"、"台北市忠孝"、"台北市大安區華聲"）
        let mut prefix_candidates = self.find_longer_road_suffixes(opt_l1.as_deref(), opt_l2.as_deref(), road_candidate_text);
        // 如果也匹配村里前綴
        if let Some(ref l2) = opt_l2 {
            if let Some(children) = self.parent_children_list.get(l2) {
                for ch in children {
                    if (ch.ends_with('里') || ch.ends_with('村')) && ch.starts_with(road_candidate_text) && ch.len() > road_candidate_text.len() {
                        prefix_candidates.push(ch[road_candidate_text.len()..].to_string());
                    }
                }
            }
        }

        if !prefix_candidates.is_empty() {
            // 依段數自然順序與長度排序
            let section_order = |s: &str| -> usize {
                if s.contains("一段") || s.contains("1段") { 1 }
                else if s.contains("二段") || s.contains("2段") { 2 }
                else if s.contains("三段") || s.contains("3段") { 3 }
                else if s.contains("四段") || s.contains("4段") { 4 }
                else if s.contains("五段") || s.contains("5段") { 5 }
                else if s.contains("六段") || s.contains("6段") { 6 }
                else if s.contains("七段") || s.contains("7段") { 7 }
                else if s.contains("八段") || s.contains("8段") { 8 }
                else if s.contains("九段") || s.contains("9段") { 9 }
                else { 99 }
            };
            prefix_candidates.sort_by(|a, b| {
                section_order(a).cmp(&section_order(b)).then_with(|| a.len().cmp(&b.len()))
            });

            for cand in prefix_candidates {
                Self::push_unique_address_cand(&mut cands, cand);
            }
            return Some((replace_range, cands));
        }

        None
    }

    #[allow(dead_code)]
    fn _unused_postal_tail(&self, resolved_code: String, meta: GeoEntityMeta, chain: Vec<String>, trimmed: &str) -> Option<GeoPostalMatch> {
        let formatted_with_postal = format!("{} {}", resolved_code, trimmed);
        Some(GeoPostalMatch {
            country: meta.country,
            postal_code: resolved_code,
            matched_hierarchy: chain,
            formatted_with_postal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_dag_from_tokens(start_idx: usize, tokens: &[&str]) -> HashMap<usize, Vec<LatticeNode>> {
        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        let mut idx = start_idx;
        for &tok in tokens {
            let next_idx = idx + 1;
            dag.entry(idx).or_default().push(LatticeNode {
                word: tok.to_string(),
                bopomofo: String::new(),
                weight: 0,
                start_idx: idx,
                end_idx: next_idx,
            });
            idx = next_idx;
        }
        dag
    }

    #[test]
    fn test_geo_hierarchy_dormant_when_no_anchor() {
        let geo = CjkGeoHierarchy::global();
        let dag = HashMap::new();
        let score = geo.score_geo_hierarchy_bonus(&["今天", "天氣", "真好"], "百福", "", 4, &dag);
        assert_eq!(score, 0.0, "Geo hierarchy must remain 100% dormant when no anchor is active");
        assert_eq!(
            geo.score_geo_hierarchy_bonus(&["很喜歡", "五"], "弄", "", 3, &dag),
            0.0,
            "is_numeral_end must remain dormant without upstream geo/road anchor"
        );
        assert_eq!(
            geo.score_geo_hierarchy_bonus(&["一"], "臨", "", 2, &dag),
            0.0,
            "is_numeral_end penalty must remain dormant without upstream geo/road anchor"
        );
    }

    #[test]
    fn test_keelung_cascade_wakeup_household_postal_cadastral() {
        let geo = CjkGeoHierarchy::global();

        let dag_qidu = dummy_dag_from_tokens(2, &["區", "百", "福", "里"]);
        let score_qidu =
            geo.score_geo_hierarchy_bonus(&["基隆市"], "七堵", "", 2, &dag_qidu);
        assert!(
            score_qidu >= 30.0,
            "Expected strong wakeup for '七堵' under '基隆市', got {}",
            score_qidu
        );

        let dag_baifu = dummy_dag_from_tokens(3, &["里"]);
        let score_baifu =
            geo.score_geo_hierarchy_bonus(&["基隆市", "七堵區"], "百福", "", 3, &dag_baifu);
        assert!(
            score_baifu >= 30.0,
            "Expected strong wakeup for '百福' + '里' under '基隆市七堵區', got {}",
            score_baifu
        );

        let dag_maling = dummy_dag_from_tokens(3, &["陵", "坑", "段"]);
        let score_maling =
            geo.score_geo_hierarchy_bonus(&["基隆市", "七堵區"], "瑪", "", 3, &dag_maling);
        assert!(
            score_maling >= 30.0,
            "Expected strong wakeup for cadastral section '瑪陵坑段' under '基隆市七堵區', got {}",
            score_maling
        );

        let postal = geo
            .resolve_postal_code_for_sentence("基隆市七堵區百福里明德一路")
            .expect("Should resolve postal code for Keelung Qidu");
        assert_eq!(postal.country, "TW");
        assert_eq!(postal.postal_code, "206005");
        assert_eq!(
            postal.formatted_with_postal,
            "206005 基隆市七堵區百福里明德一路"
        );

        let postal_zhengbin = geo
            .resolve_postal_code_for_sentence("基隆市中正區正濱里中正路")
            .expect("Should resolve 3+3 postal code for Keelung Zhongzheng Zhengbin");
        assert_eq!(postal_zhengbin.postal_code, "202008");

        // Chunghwa Post 3+3 delivery range rules for 基隆市中正區和一路:
        // - 單全 -> 202008 (e.g. 13號)
        // - 2巷全 -> 202009 (e.g. 2巷1號)
        // - 雙 4號以上 -> 202009 (e.g. 4號, 16號)
        let postal_heyi_odd = geo
            .resolve_postal_code_for_sentence("基隆市中正區和憲里1鄰和一路13號")
            .expect("Should resolve 3+3 postal code for Heyi Rd odd number 13號");
        assert_eq!(postal_heyi_odd.postal_code, "202008");

        let postal_heyi_lane2 = geo
            .resolve_postal_code_for_sentence("基隆市中正區和憲里1鄰和一路2巷1號")
            .expect("Should resolve 3+3 postal code for Heyi Rd Lane 2");
        assert_eq!(postal_heyi_lane2.postal_code, "202009");

        let postal_heyi_even = geo
            .resolve_postal_code_for_sentence("基隆市中正區和憲里1鄰和一路4號")
            .expect("Should resolve 3+3 postal code for Heyi Rd even number >= 4");
        assert_eq!(postal_heyi_even.postal_code, "202009");

        let postal_banqiao = geo
            .resolve_postal_code_for_sentence("新北市板橋區流芳里文化路一段")
            .expect("Should resolve 3+3 postal code for Banqiao Wenhua Rd");
        assert_eq!(postal_banqiao.postal_code, "220052");

        // 任務四：驗證全國 79,876 筆 3+3 郵遞區號投遞段與村里架構
        let postal_xiangfeng = geo
            .resolve_postal_code_for_sentence("基隆市中正區祥豐街75號")
            .expect("Should resolve 3+3 postal code for Xiangfeng St 75號");
        assert_eq!(postal_xiangfeng.postal_code, "202005");

        let postal_zhongxiao = geo
            .resolve_postal_code_for_sentence("臺北市大安區華聲里11鄰忠孝東路四段295號")
            .expect("Should resolve 3+3 postal code for Zhongxiao E Rd Sec 4 295號");
        assert_eq!(postal_zhongxiao.postal_code, "106078");

        let exp_zhongxiao = geo.expand_household_address_candidates("臺北市大安區華聲里11鄰忠孝東路四段295號");
        assert!(exp_zhongxiao.iter().any(|c| c.contains("106078")));
    }

    #[test]
    fn test_hsu_daan_zhongxiao_full_sentence_geo_flow() {
        let mut engine = crate::engine::RuhiEngine::new_with_data_dir("../ruhi_lexicon/data");
        engine.set_keyboard_mode("hsu");
        engine.reset_buffer();
        // tidbafcjdrjm vu hxrdcl lef 11 lendjxl cewj dxl lxj sj dxmj 295 hwj
        let mut out = String::new();
        for ch in "tidbafcjdrjm vu hxrdcl lef 11 lendjxl cewj dxl lxj sj dxmj 295 hwj ".chars() {
            let r = engine.process_key(ch);
            if !r.committed_chunk.is_empty() {
                out.push_str(&r.committed_chunk);
            }
        }
        out.push_str(&engine.composed_sentence);
        assert_eq!(out, "台北市大安區華聲里11鄰忠孝東路四段295號");
        let geo = CjkGeoHierarchy::global();
        let postal = geo.resolve_postal_code_for_sentence(&out).expect("Should resolve postal code");
        assert_eq!(postal.postal_code, "106078");

        // 驗證使用者回報之「省略里鄰」與「省略行政區（跨大安區／信義區門牌區段自動消歧）」案例：
        assert_eq!(
            geo.resolve_postal_code_for_sentence("台北市大安區忠孝東路四段295號")
                .expect("Should resolve Daan Zhongxiao E Rd Sec 4 295號")
                .postal_code,
            "106078"
        );
        assert_eq!(
            geo.resolve_postal_code_for_sentence("台北市忠孝東路四段295號")
                .expect("Should resolve district-omitted Taipei Zhongxiao E Rd Sec 4 295號")
                .postal_code,
            "106078"
        );
        assert_eq!(
            geo.resolve_postal_code_for_sentence("台北市忠孝東路四段345號")
                .expect("Should resolve district-omitted Xinyi Zhongxiao E Rd Sec 4 345號")
                .postal_code,
            "110055"
        );
    }

    #[test]
    fn test_japan_korea_cn_hk_mo_cascade_and_postal() {
        let geo = CjkGeoHierarchy::global();

        let dag_jp = dummy_dag_from_tokens(3, &["之", "內"]);
        let score_jp =
            geo.score_geo_hierarchy_bonus(&["東京都", "千代田區"], "丸", "", 3, &dag_jp);
        assert!(score_jp >= 30.0, "Expected wakeup for 丸之內 under 東京都千代田區");
        let postal_jp = geo
            .resolve_postal_code_for_sentence("東京都千代田區丸之內")
            .expect("Should resolve JP postal code");
        assert_eq!(postal_jp.postal_code, "〒100");

        let dag_kr = dummy_dag_from_tokens(3, &["鷗", "亭", "洞"]);
        let score_kr =
            geo.score_geo_hierarchy_bonus(&["首爾特別市", "江南區"], "狎", "", 3, &dag_kr);
        assert!(score_kr >= 30.0, "Expected wakeup for 狎鷗亭洞 under 首爾特別市江南區");
        let postal_kr = geo
            .resolve_postal_code_for_sentence("首爾特別市江南區狎鷗亭洞")
            .expect("Should resolve KR postal code");
        assert_eq!(postal_kr.postal_code, "06000");

        let postal_hk = geo
            .resolve_postal_code_for_sentence("香港特別行政區深水埗區長沙灣")
            .expect("Should resolve HK postal code");
        assert_eq!(postal_hk.postal_code, "999077");

        let postal_cn = geo
            .resolve_postal_code_for_sentence("廣東省深圳市南山區粵海街道")
            .expect("Should resolve CN postal code");
        assert_eq!(postal_cn.postal_code, "518052");

        // Omitting 區, 里, 鄰 for 基隆市和二路 & 基隆市和一路:
        for addr in [
            "基隆市和二路15號",
            "基隆市中正區和二路1號",
            "基隆市中正區社寮里和二路1號",
            "中正區和二路1號",
            "和二路15號",
        ] {
            let p = geo
                .resolve_postal_code_for_sentence(addr)
                .unwrap_or_else(|| panic!("Should resolve postal code for {}", addr));
            assert_eq!(
                p.postal_code, "202009",
                "Expected 202009 for {}, got {}",
                addr, p.postal_code
            );
        }
        assert_eq!(
            geo.resolve_postal_code_for_sentence("基隆市和一路13號")
                .unwrap()
                .postal_code,
            "202008"
        );
        assert_eq!(
            geo.resolve_postal_code_for_sentence("基隆市和一路2巷1號")
                .unwrap()
                .postal_code,
            "202009"
        );
        assert_eq!(
            geo.resolve_postal_code_for_sentence("基隆市和一路4號")
                .unwrap()
                .postal_code,
            "202009"
        );
        let dag_heer = dummy_dag_from_tokens(2, &["二", "路"]);
        let score_he = geo.score_geo_hierarchy_bonus(&["基隆市"], "和", "", 2, &dag_heer);
        assert!(
            score_he >= 30.0,
            "Expected wakeup for '和' of '和二路' under '基隆市' even when '中正區' is omitted, got {}",
            score_he
        );

        // Household registration address expansion (Shift+Arrow mark + Down):
        let exp_heer = geo.expand_household_address_candidates("基隆市和二路15號");
        assert!(
            exp_heer.first().map(|s| s.as_str()) == Some("基隆市中正區社寮里和二路15號"),
            "Expected #1 candidate '基隆市中正區社寮里和二路15號', got {:?}",
            exp_heer
        );
        assert!(exp_heer.contains(&"基隆市中正區平寮里和二路15號".to_string()));
        assert!(exp_heer.contains(&"202009基隆市中正區社寮里和二路15號".to_string()));

        let exp_heyi = geo.expand_household_address_candidates("基隆市和一路13號");
        assert_eq!(
            exp_heyi.first().map(|s| s.as_str()),
            Some("基隆市中正區和憲里和一路13號")
        );
        assert!(exp_heyi.contains(&"202008基隆市中正區和憲里和一路13號".to_string()));

        let exp_with_postal = geo.expand_household_address_candidates("200000基隆市和二路15號");
        assert_eq!(
            exp_with_postal.first().map(|s| s.as_str()),
            Some("202009基隆市中正區社寮里和二路15號")
        );
    }
}
