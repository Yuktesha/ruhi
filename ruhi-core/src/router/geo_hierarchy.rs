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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoPostalMatch {
    pub country: String,
    pub postal_code: String,
    pub matched_hierarchy: Vec<String>,
    pub formatted_with_postal: String,
}

pub struct CjkGeoHierarchy {
    /// entity name (e.g. "基隆市", "七堵區", "東京都", "江南區") -> list of GeoEntityMeta
    entities_by_name: HashMap<String, Vec<GeoEntityMeta>>,
    /// parent_entity -> set of child entities
    parent_to_children: HashMap<String, HashSet<String>>,
    /// parent_entity -> ordered list of children for prefix/sub-token matching
    parent_children_list: HashMap<String, Vec<String>>,
    /// Ordered list of L1 root entities for character-level prefix matching (e.g. "首"+"爾"+"特別"+"市")
    l1_roots_list: Vec<String>,
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
                }
            }

            if parent != "ROOT" {
                let p_set = parent_to_children.entry(parent.clone()).or_default();
                let p_vec = parent_children_list.entry(parent.clone()).or_default();
                if p_set.insert(entity.clone()) {
                    p_vec.push(entity.clone());
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
                });
        }

        Self {
            entities_by_name,
            parent_to_children,
            parent_children_list,
            l1_roots_list,
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
        let cleaned: Vec<&str> = slice.iter().map(|w| Self::clean_token(w)).collect();

        for i in 0..cleaned.len() {
            let mut acc = String::new();
            for j in i..(i + 6).min(cleaned.len()) {
                let tok = cleaned[j];
                if tok.is_empty() {
                    break;
                }
                acc.push_str(tok);
                if self.parent_to_children.contains_key(&acc) {
                    if !active.iter().any(|x| x == &acc) {
                        active.push(acc.clone());
                    }
                }
            }
        }

        // Also extract L1/L2 anchors embedded inside committed multi-entity prefix chunks (e.g., "基隆市中正區和憲里")
        let joined_all: String = cleaned.iter().copied().collect();
        if !joined_all.is_empty() {
            for l1 in &self.l1_roots_list {
                if joined_all.contains(l1.as_str()) {
                    if !active.iter().any(|x| x == l1) {
                        active.push(l1.clone());
                    }
                    if let Some(children) = self.parent_children_list.get(l1) {
                        for ch in children {
                            if joined_all.contains(ch.as_str())
                                && self.parent_to_children.contains_key(ch)
                            {
                                if !active.iter().any(|x| x == ch) {
                                    active.push(ch.clone());
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
        let Some(children) = self.parent_children_list.get(parent_entity) else {
            return false;
        };
        if prefix_str.is_empty() {
            for ch in children {
                if Self::dag_can_spell(dag, start_idx, ch) {
                    return true;
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

        // 2. Continuation of `entity_str` from the last 1..=4 tokens of `history_tokens`:
        let n = history_tokens.len();
        for k in 1..=4.min(n) {
            let mut prefix_built = String::new();
            for &t in &history_tokens[n - k..] {
                prefix_built.push_str(t);
            }
            if prefix_built.is_empty() {
                continue;
            }
            if let Some(after_hist) = entity_str.strip_prefix(&prefix_built) {
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

    /// Compute the conditional geo/postal/cadastral/household hierarchy bonus for `curr_word`.
    /// Returns `0.0` immediately when no geo anchor is present in `prefix_words` and `curr_word`
    /// is not initiating a verified L1->L2 geo cascade.
    pub fn score_geo_hierarchy_bonus(
        &self,
        prefix_words: &[&str],
        curr_word: &str,
        _curr_bopomofo: &str,
        end_idx: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        let curr_clean = Self::clean_token(curr_word);
        if curr_clean.is_empty() {
            return 0.0;
        }

        let mut bonus = 0.0;
        let window_start = prefix_words.len().saturating_sub(24);
        let cleaned_history: Vec<&str> = prefix_words[window_start..]
            .iter()
            .map(|w| Self::clean_token(w))
            .filter(|w| !w.is_empty())
            .collect();

        // ---------------------------------------------------------------------
        // A. Forward Lookahead Activation for L1 Root Anchor (including multi-token L1 roots
        //    such as "首"+"爾"+"特別"+"市", "釜山"+"廣"+"域"+"市", "仁"+"川"+"廣"+"域"+"市",
        //    "濟"+"州"+"特別"+"自治"+"道") when followed by one of its L2 children in `dag`!
        // ---------------------------------------------------------------------
        for l1 in &self.l1_roots_list {
            let l1_str = l1.as_str();
            // Case A1: `curr_clean` starts or is `l1_str`
            if let Some(rem_l1) = l1_str.strip_prefix(curr_clean) {
                if self.dag_can_spell_prefix_then_any_child(dag, end_idx, rem_l1, l1_str) {
                    bonus += 68.0;
                    break;
                }
            }
            // Case A2: `cleaned_history` ends with a prefix of `l1_str` and `curr_clean` continues/completes `l1_str`
            let hn = cleaned_history.len();
            let mut matched_l1_cont = false;
            for k in 1..=4.min(hn) {
                let mut hist_pfx = String::new();
                for &t in &cleaned_history[hn - k..] {
                    hist_pfx.push_str(t);
                }
                if let Some(after_hist) = l1_str.strip_prefix(&hist_pfx) {
                    if let Some(rem_l1) = after_hist.strip_prefix(curr_clean) {
                        if self.dag_can_spell_prefix_then_any_child(dag, end_idx, rem_l1, l1_str) {
                            bonus += 70.0;
                            matched_l1_cont = true;
                            break;
                        }
                    }
                }
            }
            if matched_l1_cont {
                break;
            }
        }

        // Collect all active parent anchors from `prefix_words`
        let mut active_anchors = self.collect_active_anchors(prefix_words);

        // Expand `active_anchors` with any L2 child that has already been spelled in `cleaned_history`
        // under an active L1 anchor (so L3 grandchildren wake up even if L2 was split into multiple tokens!).
        let tail_joined: String = cleaned_history.iter().copied().collect();
        let initial_anchors = active_anchors.clone();
        for anc in &initial_anchors {
            if let Some(children) = self.parent_children_list.get(anc) {
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

        if active_anchors.is_empty() {
            return bonus;
        }

        // ---------------------------------------------------------------------
        // B. Downstream Child Branch Wakeup (L1 -> L2 -> L3 Villages/Streets/Sections/Dongs/Chos)
        // ---------------------------------------------------------------------
        let mut matched_child_entity: Option<&str> = None;
        let mut completed_child_entity: Option<&str> = None;

        'anchor_loop: for anchor in active_anchors.iter().rev() {
            let Some(children) = self.parent_children_list.get(anchor) else {
                continue;
            };
            for child in children {
                let child_str = child.as_str();
                if let Some(is_complete) = Self::matches_entity_path(
                    &cleaned_history,
                    curr_clean,
                    end_idx,
                    dag,
                    child_str,
                ) {
                    bonus += if is_complete { 82.0 } else { 78.0 };
                    matched_child_entity = Some(child_str);
                    if is_complete {
                        completed_child_entity = Some(child_str);
                    }
                    break 'anchor_loop;
                }
            }
        }

        // ---------------------------------------------------------------------
        // C. Cross-Token Boundary Protection AFTER an Address Entity!
        //    Fixes the #1 failure mode discovered in Batch 19:
        //    When a road/section/village/dong/ward ends with "路", "街", "段", "里", "洞", "町", "區", "市"
        //    and is followed by conjunction "與" (ㄩˇ) or "和" / "及" / "的" / "旁":
        //    - Prevent 2-char impostor words "露雨" (ㄌㄨˋ ㄩˇ), "斷語" (ㄉㄨㄢˋ ㄩˇ),
        //      "侗語" (ㄉㄨㄥˋ ㄩˇ), "日本語" (when followed by another country/city),
        //      "中斷" (ㄓㄨㄥ ㄉㄨㄢˋ instead of 城中段), "接到" (ㄐㄧㄝ ㄉㄠˋ instead of 街道),
        //      "大到" (ㄉㄚˋ ㄉㄠˋ instead of 大道), "不屈" (ㄅㄨˋ ㄑㄩ instead of 埗區),
        //      "理屈" (ㄌㄧˇ ㄑㄩ instead of 里區)!
        // ---------------------------------------------------------------------
        let last_word = cleaned_history.last().copied().unwrap_or("");
        let prev2_word = if cleaned_history.len() >= 2 {
            cleaned_history[cleaned_history.len() - 2]
        } else {
            ""
        };

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
                    bonus += 68.0;
                } else {
                    bonus -= 68.0;
                }
            }
        }

        // 2. Penalize cross-boundary 2-char impostor words ("露雨", "斷語", "侗語", "接到", "大到", "不屈", "理屈", "中斷", "普世", "浴室", "時段", "新版", "只原", "美食接")
        //    when an address anchor is active!
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
            bonus -= 92.0;
        }
        if curr_clean == "中斷" && last_word == "城" {
            bonus -= 90.0;
        }
        if curr_clean == "時段" && last_word == "鶯歌" {
            bonus -= 95.0;
        }
        if curr_clean == "新版"
            && dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|n| n.word == "段"))
                .unwrap_or(false)
        {
            bonus -= 95.0;
        }

        // 3. Strongly boost multi-char administrative suffixes & street/plaza heads
        if matches!(
            curr_clean,
            "街道"
                | "大道"
                | "特別市"
                | "廣域市"
                | "自治道"
                | "特別行政區"
                | "街區"
                | "美食街"
                | "老街"
                | "商圈"
                | "外灘"
        ) {
            bonus += 68.0;
        }
        if matches!(curr_clean, "街" | "接" | "皆" | "階")
            && (matches!(last_word, "美食" | "精品" | "商店街" | "年貨" | "地下" | "步行" | "老" | "官也" | "也" | "自治")
                || dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|n| matches!(n.word.as_str(), "區" | "頭" | "道" | "坊" | "角" | "景" | "拍" | "口" | "尾")))
                    .unwrap_or(false))
        {
            if curr_clean == "街" {
                bonus += 72.0;
            } else {
                bonus -= 68.0;
            }
        }
        if matches!(curr_clean, "灘" | "攤" | "癱")
            && matches!(last_word, "外" | "沙" | "海" | "淺" | "泥" | "河")
        {
            if curr_clean == "灘" {
                bonus += 72.0;
            } else {
                bonus -= 68.0;
            }
        }

        // ---------------------------------------------------------------------
        // D. Address / Cadastral / Household Structural Slot & Homophone Guard
        // ---------------------------------------------------------------------
        let is_numeral_end = last_word.ends_with(|c: char| {
            matches!(
                c,
                '一' | '二'
                    | '三'
                    | '四'
                    | '五'
                    | '六'
                    | '七'
                    | '八'
                    | '九'
                    | '十'
                    | '百'
                    | '零'
                    | '〇'
                    | '1'
                    | '2'
                    | '3'
                    | '4'
                    | '5'
                    | '6'
                    | '7'
                    | '8'
                    | '9'
                    | '0'
            )
        });

        let prev_is_road_or_section = last_word.ends_with('路')
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
            || prev2_word.ends_with('號');

        if is_numeral_end {
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
            ) {
                bonus += 84.0;
            } else if matches!(
                curr_clean,
                "林" | "臨" | "淋" | "琳" | "麟" | "斷" | "鍛" | "露" | "錄" | "接" | "皆" | "向" | "項" | "好" | "耗" | "流" | "留"
            ) || curr_clean.starts_with('臨')
            {
                bonus -= 88.0;
            }
        }

        if prev_is_road_or_section {
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
                bonus += 64.0;
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
            bonus += 62.0;
        }

        // Homophone guard inside active geo hierarchy:
        if matched_child_entity.is_some() {
            if matches!(
                curr_clean,
                "裡" | "理" | "斷" | "鍛" | "動" | "凍" | "露" | "接" | "挺" | "屈"
            ) {
                bonus -= 68.0;
            }
        }

        bonus
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
        let c: String = cond.chars().filter(|ch| !ch.is_whitespace()).collect();
        if c.is_empty() {
            return false;
        }
        if c == "全" {
            return true;
        }

        // 1. Lane-based rule (e.g. "2巷全", "2巷至10巷", "單1巷至9巷")
        if c.contains('巷') {
            let Some(l) = lane else {
                return false;
            };
            let (parity_ok, rem) = if let Some(r) = c.strip_prefix('單') {
                (l % 2 == 1, r)
            } else if let Some(r) = c.strip_prefix('雙') {
                (l % 2 == 0, r)
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
        } else if let Some(base) = rem.strip_suffix('號') {
            if let Some(target) = Self::parse_cjk_or_arabic_u32(base) {
                return n == target;
            }
        }
        false
    }

    /// Resolve a Chunghwa Post 3+3 postal code specification (which may contain delivery range rules
    /// like `"202008:單全;202009:2巷全,雙4號以上"`) against the address suffix `after_entity`.
    fn resolve_3plus3_delivery_range(postal_spec: &str, after_entity: &str) -> String {
        if !postal_spec.contains(':') && !postal_spec.contains(';') {
            return postal_spec.to_string();
        }
        let lane = Self::extract_number_before_unit(after_entity, '巷');
        let house_no = Self::extract_number_before_unit(after_entity, '號');

        let mut default_code: Option<String> = None;
        for clause in postal_spec.split(';') {
            let clause = clause.trim();
            if clause.is_empty() {
                continue;
            }
            if let Some((code_part, conds_part)) = clause.split_once(':') {
                let code = code_part.trim();
                if default_code.is_none() {
                    default_code = Some(code.to_string());
                }
                for cond in conds_part.split(',') {
                    if Self::matches_3plus3_condition(cond, lane, house_no) {
                        return code.to_string();
                    }
                }
            } else if default_code.is_none() {
                default_code = Some(clause.to_string());
            }
        }
        default_code.unwrap_or_else(|| postal_spec.to_string())
    }

    /// Resolve the most specific postal code and hierarchy path from a composed sentence or address prefix.
    pub fn resolve_postal_code_for_sentence(&self, sentence: &str) -> Option<GeoPostalMatch> {
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut best_match: Option<(usize, GeoEntityMeta, Vec<String>, String)> = None;

        for metas in self.entities_by_name.values() {
            for meta in metas {
                if meta.postal_code.is_empty() {
                    continue;
                }
                if let Some(pos) = trimmed.find(&meta.entity) {
                    let parent_present = meta.parent == "ROOT"
                        || trimmed[..pos].contains(&meta.parent)
                        || (meta.country == "TW"
                            && meta.parent.starts_with('臺')
                            && trimmed[..pos].contains(&meta.parent.replacen('臺', "台", 1)))
                        || (meta.country == "TW"
                            && meta.parent.starts_with('台')
                            && trimmed[..pos].contains(&meta.parent.replacen('台', "臺", 1)));

                    let is_unique_name = metas.len() == 1;
                    let allow_match = if meta.level == "L3" && meta.children.is_empty() {
                        parent_present
                    } else {
                        meta.parent == "ROOT" || parent_present || is_unique_name
                    };

                    if allow_match {
                        let mut chain = Vec::new();
                        if meta.parent != "ROOT" && parent_present {
                            chain.push(meta.parent.clone());
                        }
                        chain.push(meta.entity.clone());

                        let after_entity = &trimmed[pos + meta.entity.len()..];
                        for ch in &meta.children {
                            if after_entity.contains(ch.as_str()) {
                                chain.push(ch.clone());
                                break;
                            }
                        }

                        let base_spec = match meta.level.as_str() {
                            "L3" => {
                                // In Chunghwa Post 3+3, road + delivery-range is more specific than village (里)
                                let cat_bonus = match meta.category.as_str() {
                                    "road" => 40,
                                    "cadastral_section" => 20,
                                    _ => 0,
                                };
                                300 + cat_bonus + chain.len() * 10 + meta.entity.len()
                            }
                            "L2" => {
                                200 + (if parent_present { 50 } else { 0 })
                                    + chain.len() * 10
                                    + meta.entity.len()
                            }
                            _ => 100 + chain.len() * 10 + meta.entity.len(),
                        };
                        let specificity = base_spec * 1000 + pos;

                        if best_match
                            .as_ref()
                            .map_or(true, |(best_score, _, _, _)| specificity > *best_score)
                        {
                            let resolved_code = Self::resolve_3plus3_delivery_range(
                                &meta.postal_code,
                                after_entity,
                            );
                            best_match = Some((specificity, meta.clone(), chain, resolved_code));
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
    }

    #[test]
    fn test_keelung_cascade_wakeup_household_postal_cadastral() {
        let geo = CjkGeoHierarchy::global();

        let dag_qidu = dummy_dag_from_tokens(2, &["區", "百", "福", "里"]);
        let score_qidu =
            geo.score_geo_hierarchy_bonus(&["基隆市"], "七堵", "", 2, &dag_qidu);
        assert!(
            score_qidu >= 70.0,
            "Expected strong wakeup for '七堵' under '基隆市', got {}",
            score_qidu
        );

        let dag_baifu = dummy_dag_from_tokens(3, &["里"]);
        let score_baifu =
            geo.score_geo_hierarchy_bonus(&["基隆市", "七堵區"], "百福", "", 3, &dag_baifu);
        assert!(
            score_baifu >= 70.0,
            "Expected strong wakeup for '百福' + '里' under '基隆市七堵區', got {}",
            score_baifu
        );

        let dag_maling = dummy_dag_from_tokens(3, &["陵", "坑", "段"]);
        let score_maling =
            geo.score_geo_hierarchy_bonus(&["基隆市", "七堵區"], "瑪", "", 3, &dag_maling);
        assert!(
            score_maling >= 70.0,
            "Expected strong wakeup for cadastral section '瑪陵坑段' under '基隆市七堵區', got {}",
            score_maling
        );

        let postal = geo
            .resolve_postal_code_for_sentence("基隆市七堵區百福里明德一路")
            .expect("Should resolve postal code for Keelung Qidu");
        assert_eq!(postal.country, "TW");
        assert_eq!(postal.postal_code, "206007");
        assert_eq!(
            postal.formatted_with_postal,
            "206007 基隆市七堵區百福里明德一路"
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
        assert_eq!(postal_banqiao.postal_code, "220001");
    }

    #[test]
    fn test_japan_korea_cn_hk_mo_cascade_and_postal() {
        let geo = CjkGeoHierarchy::global();

        let dag_jp = dummy_dag_from_tokens(3, &["之", "內"]);
        let score_jp =
            geo.score_geo_hierarchy_bonus(&["東京都", "千代田區"], "丸", "", 3, &dag_jp);
        assert!(score_jp >= 70.0, "Expected wakeup for 丸之內 under 東京都千代田區");
        let postal_jp = geo
            .resolve_postal_code_for_sentence("東京都千代田區丸之內")
            .expect("Should resolve JP postal code");
        assert_eq!(postal_jp.postal_code, "〒100");

        let dag_kr = dummy_dag_from_tokens(3, &["鷗", "亭", "洞"]);
        let score_kr =
            geo.score_geo_hierarchy_bonus(&["首爾特別市", "江南區"], "狎", "", 3, &dag_kr);
        assert!(score_kr >= 70.0, "Expected wakeup for 狎鷗亭洞 under 首爾特別市江南區");
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
    }
}
