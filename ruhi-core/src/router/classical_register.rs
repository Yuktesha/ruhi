/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/classical_register.rs - 路線 C：文言單字本體語境與虛詞句法槽位引擎（Route C Prototype）
///
/// 實作三項正交機制，在不修改核心詞庫單字先驗權重的前提下，強化文言與高密度古漢語語境的單字消歧能力：
/// 1. 支柱一（`transition_score_delta`）：文言虛詞句法槽位與對偶共現轉移場（之、乎、者、也、矣、焉、哉、於、以、而、其、乃、則、亦、勿、斯、為、必、欲 與 X可X/X而X 對稱）。
/// 2. 支柱二（`node_score_delta`, len=1）：古典高頻單字語體先驗動態校正，抑制現代白話同音干擾字（煙、野、椅、蟻、嗎、呢等）。
/// 3. 支柱三（`node_score_delta`, len=2）：現代高頻功能性雙字詞跨界劫掠壓制（保留明確古漢語雙字詞白名單，並對跨接核心虛詞樞紐之現代雙字詞施加邊界懲罰）。

use std::collections::HashMap;

use super::lattice::LatticeNode;

/// 文言單字語境模組之運作模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassicalRegisterMode {
    /// 自動偵測：具備嚴格現代白話否定條件與多重古典特徵累積門檻（預設模式）。
    #[default]
    Auto,
    /// 停用文言單字語境模組（供對照基準評估使用）。
    ModernOnly,
    /// 使用者顯式啟用文言輔助模式（強制將語體強度設為 1.0）。
    ClassicalExplicit,
}

impl ClassicalRegisterMode {
    /// 由設定檔字串解析文言單字語境運作模式（預設為 `Auto`）。
    pub fn from_config_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "modern_only" | "modern" => Self::ModernOnly,
            "explicit" | "classical_explicit" => Self::ClassicalExplicit,
            _ => Self::Auto,
        }
    }

    /// 轉換為設定檔持久化字串表示（`"auto"` / `"off"` / `"explicit"`）。
    pub fn as_config_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ModernOnly => "off",
            Self::ClassicalExplicit => "explicit",
        }
    }
}

/// 單句層級之文言單字語境偵測輪廓。
#[derive(Debug, Clone)]
pub struct ClassicalRegisterProfile {
    pub active: bool,
    pub strength: f64,
    pub classical_score: u32,
    pub pivot_mask: Vec<bool>,
}

impl ClassicalRegisterProfile {
    /// 建立未啟動之空輪廓。
    pub fn inactive(pivot_mask: Vec<bool>, classical_score: u32) -> Self {
        Self {
            active: false,
            strength: 0.0,
            classical_score,
            pivot_mask,
        }
    }

    /// 掃描詞網格與操作模式，決定是否啟動路線 C 文言單字語境加成與邊界懲罰。
    pub fn detect(
        mode: ClassicalRegisterMode,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> Self {
        let mut pivot_mask = vec![false; n_syllables];
        for (pos, has_pivot) in pivot_mask.iter_mut().enumerate() {
            if let Some(nodes) = dag.get(&pos) {
                *has_pivot = nodes
                    .iter()
                    .any(|n| n.length() == 1 && is_classical_pivot_char(&n.word));
            }
        }

        match mode {
            ClassicalRegisterMode::ModernOnly => Self::inactive(pivot_mask, 0),
            ClassicalRegisterMode::ClassicalExplicit => {
                let active = n_syllables >= 2;
                Self {
                    active,
                    strength: if active { 1.0 } else { 0.0 },
                    classical_score: if active { 5 } else { 0 },
                    pivot_mask,
                }
            }
            ClassicalRegisterMode::Auto => {
                // 嚴格現代白話否定條件 1：句長不足 6 音節不啟動，避免短片語誤判。
                if n_syllables < 6 {
                    return Self::inactive(pivot_mask, 0);
                }

                // 嚴格現代白話否定條件 2 & 3：任何輕聲「˙」讀音或高頻現代白話指標詞皆立即否定。
                for nodes in dag.values() {
                    for node in nodes {
                        if node.bopomofo.split_whitespace().any(|s| s.starts_with('˙')) {
                            return Self::inactive(pivot_mask, 0);
                        }
                        if node.length() >= 2 && node.weight >= 350 && is_modern_anchor(&node.word) {
                            return Self::inactive(pivot_mask, 0);
                        }
                    }
                }

                let mut score: u32 = 0;
                let mut has_clause_final_particle = false;

                // 正向訊號 1：句末古典語氣虛詞（也、哉、矣、焉、乎）
                let last_pos = n_syllables - 1;
                if let Some(last_nodes) = dag.get(&last_pos) {
                    let has_last_c1 = |w: &str, bpm: &str| {
                        last_nodes
                            .iter()
                            .any(|n| n.length() == 1 && n.word == w && n.bopomofo == bpm)
                    };
                    let has_covering_modern_ending = |words: &[&str]| -> bool {
                        if last_pos == 0 {
                            return false;
                        }
                        for start in 0..last_pos {
                            if let Some(nodes) = dag.get(&start) {
                                if nodes.iter().any(|n| {
                                    n.end_idx == n_syllables && words.contains(&n.word.as_str())
                                }) {
                                    return true;
                                }
                            }
                        }
                        false
                    };

                    if has_last_c1("也", "ㄧㄝˇ") {
                        score += 3;
                        has_clause_final_particle = true;
                    } else if has_last_c1("哉", "ㄗㄞ") {
                        score += 3;
                        has_clause_final_particle = true;
                    } else if has_last_c1("矣", "ㄧˇ")
                        && last_pos > 0
                        && !has_covering_modern_ending(&[
                            "可以", "所以", "座椅", "木椅", "輪椅", "螞蟻",
                        ])
                    {
                        score += 2;
                        has_clause_final_particle = true;
                    } else if has_last_c1("焉", "ㄧㄢ")
                        && !has_covering_modern_ending(&[
                            "抽煙", "吸煙", "香煙", "黑煙", "濃煙", "無煙",
                        ])
                    {
                        score += 2;
                        has_clause_final_particle = true;
                    } else if has_last_c1("乎", "ㄏㄨ")
                        && !has_covering_modern_ending(&[
                            "在乎", "似乎", "幾乎", "招呼", "近乎", "關乎", "合乎",
                        ])
                    {
                        score += 2;
                        has_clause_final_particle = true;
                    }
                }

                // 預先統計非古典雙字詞（wt >= 420）涵蓋位置
                let mut modern_covered = vec![false; n_syllables];
                for nodes in dag.values() {
                    for node in nodes {
                        if node.length() >= 2
                            && node.weight >= 420
                            && !is_classical_disyllable(&node.word)
                        {
                            for cov in modern_covered
                                .iter_mut()
                                .take(node.end_idx.min(n_syllables))
                                .skip(node.start_idx)
                            {
                                *cov = true;
                            }
                        }
                    }
                }

                // 預先標記單個位置是否被高權重（wt >= 420）非古典雙字詞覆蓋
                let mut pivot_blocked = vec![false; n_syllables];
                for nodes in dag.values() {
                    for node in nodes {
                        if node.length() == 2
                            && node.weight >= 420
                            && !is_classical_disyllable(&node.word)
                        {
                            for blk in pivot_blocked
                                .iter_mut()
                                .take(node.end_idx.min(n_syllables))
                                .skip(node.start_idx)
                            {
                                *blk = true;
                            }
                        }
                    }
                }

                // 正向訊號 2：句中未被高權重現代雙字詞覆蓋之核心古典虛詞／樞紐位置
                let mut internal_pivots = 0u32;
                for pos in 1..last_pos {
                    if pivot_blocked[pos] {
                        continue;
                    }
                    if let Some(nodes) = dag.get(&pos) {
                        let is_internal_pivot = nodes.iter().any(|n| {
                            n.length() == 1
                                && matches!(
                                    (n.word.as_str(), n.bopomofo.as_str()),
                                    ("而", "ㄦˊ")
                                        | ("於", "ㄩˊ")
                                        | ("之", "ㄓ")
                                        | ("以", "ㄧˇ")
                                        | ("其", "ㄑㄧˊ")
                                        | ("者", "ㄓㄜˇ")
                                        | ("則", "ㄗㄜˊ")
                                        | ("乃", "ㄋㄞˇ")
                                        | ("亦", "ㄧˋ")
                                        | ("勿", "ㄨˋ")
                                        | ("斯", "ㄙ")
                                        | ("為", "ㄨㄟˊ")
                                        | ("必", "ㄅㄧˋ")
                                        | ("欲", "ㄩˋ")
                                )
                        });
                        if is_internal_pivot {
                            internal_pivots += 1;
                        }
                    }
                }
                score += internal_pivots.min(4);

                // 正向訊號 3：平行對偶音節迴聲（固定步距重複或 X可X／X而X 結構）
                let mut syls: Vec<&str> = Vec::with_capacity(n_syllables);
                for pos in 0..n_syllables {
                    let s = dag
                        .get(&pos)
                        .and_then(|ns| ns.iter().find(|n| n.length() == 1))
                        .map(|n| n.bopomofo.as_str())
                        .unwrap_or("");
                    syls.push(s);
                }
                let mut has_stride_echo = false;
                for k in 2..=7usize {
                    if k >= n_syllables {
                        break;
                    }
                    let mut matches_at_stride = 0usize;
                    for p in 0..(n_syllables - k) {
                        if !syls[p].is_empty() && syls[p] == syls[p + k] {
                            matches_at_stride += 1;
                        }
                    }
                    if matches_at_stride >= 2 {
                        has_stride_echo = true;
                        break;
                    }
                }
                let has_aba_symmetry = (0..n_syllables.saturating_sub(2)).any(|p| {
                    !syls[p].is_empty()
                        && syls[p] == syls[p + 2]
                        && matches!(syls[p + 1], "ㄎㄜˇ" | "ㄦˊ")
                });
                if has_stride_echo {
                    score += 2;
                }
                if has_aba_symmetry {
                    score += 2;
                }

                // 正向訊號 4：非古典現代複合詞低涵蓋率
                let covered_cnt = modern_covered.iter().filter(|&&c| c).count();
                if (covered_cnt as f64) / (n_syllables as f64) < 0.50 {
                    score += 1;
                }

                let active = (has_clause_final_particle && score >= 3) || score >= 4;
                let strength = if active {
                    ((score as f64 - 2.0) / 3.0).clamp(0.65, 1.0)
                } else {
                    0.0
                };
                Self {
                    active,
                    strength,
                    classical_score: score,
                    pivot_mask,
                }
            }
        }
    }

    /// 支柱二與支柱三：單字古典語體先驗校正與現代雙字詞跨界劫掠壓制。
    pub fn node_score_delta(&self, node: &LatticeNode, n_syllables: usize) -> f64 {
        if !self.active {
            return 0.0;
        }
        let s = self.strength;
        let len = node.length();
        let w = node.word.as_str();

        if len == 1 {
            let mut delta = 0.0f64;
            if node.end_idx == n_syllables {
                delta += match w {
                    "也" | "哉" => 18.0 * s,
                    "矣" | "焉" => 16.0 * s,
                    "乎" => 14.0 * s,
                    "野" | "冶" | "耶" | "煙" | "淹" | "醃" | "燕" | "災" | "栽" | "呼" | "忽"
                    | "狐" | "壺" | "椅" | "蟻" => -12.0 * s,
                    _ => 0.0,
                };
            }

            if is_classical_core_char(w) {
                delta += 5.0 * s;
            }
            if is_ultra_classical_char(w) {
                delta += 3.0 * s;
            }
            if is_modern_colloquial_distractor_char(w) {
                delta -= 6.0 * s;
            }
            return delta.clamp(-22.0, 24.0);
        }

        if len == 2 {
            if is_classical_disyllable(w) {
                // 保護低權重古典雙字詞（wt < 380），避免在 S5 梯度下被拆散為單字高頻替代詞
                if node.weight < 380 {
                    return 6.0 * s;
                }
                return 0.0;
            }
            let mut pen = -12.0 * s;
            let touches_left = self.pivot_mask.get(node.start_idx).copied().unwrap_or(false);
            let touches_right = node.end_idx > 0
                && self
                    .pivot_mask
                    .get(node.end_idx - 1)
                    .copied()
                    .unwrap_or(false);
            if touches_left || touches_right {
                pen -= 14.0 * s;
            }
            return pen.clamp(-26.0, 0.0);
        }

        0.0
    }

    /// 支柱一：文言虛詞句法槽位與對偶共現轉移場。
    pub fn transition_score_delta(
        &self,
        prev_word: &str,
        prev2_word: Option<&str>,
        node: &LatticeNode,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        if !self.active {
            return 0.0;
        }
        let s = self.strength;
        let curr = node.word.as_str();
        let right = dag.get(&node.end_idx);
        let right_has_any = |candidates: &[&str]| -> bool {
            right.is_some_and(|ns| ns.iter().any(|m| candidates.contains(&m.word.as_str())))
        };
        let mut delta = 0.0f64;

        // 1. 平行對偶重疊構式（道可道、名可名、憂而憂、樂而樂、非常道／名）
        if matches!(prev_word, "可" | "而")
            && node.length() == 1
            && prev2_word.is_some_and(|p2| p2 == curr)
        {
            delta += 14.0 * s;
        }
        if curr == "可" && node.start_idx >= 1 && node.end_idx < n_syllables {
            let left_syl = dag
                .get(&(node.start_idx - 1))
                .and_then(|ns| ns.iter().find(|n| n.length() == 1))
                .map(|n| n.bopomofo.as_str());
            let right_syl = right
                .and_then(|ns| ns.iter().find(|n| n.length() == 1))
                .map(|n| n.bopomofo.as_str());
            if let (Some(ls), Some(rs)) = (left_syl, right_syl) {
                if !ls.is_empty() && ls == rs {
                    delta += 10.0 * s;
                }
            }
        }
        if node.length() == 1
            && node.end_idx + 1 < n_syllables
            && right_has_any(&["可"])
            && matches!(
                curr,
                "道" | "名" | "言" | "法" | "行" | "學" | "知" | "思" | "為" | "守" | "傳" | "修"
            )
            && !node.bopomofo.is_empty()
            && dag
                .get(&(node.end_idx + 1))
                .and_then(|ns| ns.iter().find(|n| n.length() == 1))
                .is_some_and(|n| n.bopomofo == node.bopomofo)
        {
            delta += 8.0 * s;
        }
        if prev_word == "非常" {
            if matches!(curr, "道" | "名" | "人" | "事" | "物" | "理") {
                delta += 12.0 * s;
            } else if curr == "到" {
                delta -= 10.0 * s;
            }
        }

        // 2. 動詞「知」與代詞／結構助詞「之」（ㄓ）句法互補
        if curr == "知" {
            if right_has_any(&[
                "之", "新", "道", "己", "音", "命", "止", "恥", "足", "天", "人", "其", "所",
            ]) || matches!(
                prev_word,
                "不" | "未" | "無" | "莫" | "能" | "可" | "欲" | "必" | "而" | "是" | "皆" | "孰"
            ) {
                delta += 10.0 * s;
            }
            if right_has_any(&["之"]) {
                delta += 4.0 * s;
            }
        }
        if prev_word == "知" && matches!(curr, "新" | "己" | "音" | "命" | "止" | "恥" | "足") {
            delta += 10.0 * s;
        }

        if curr == "之" {
            if right_has_any(&["之"])
                || matches!(prev_word, "不" | "未" | "而")
                || (prev_word == "是" && right_has_any(&["也", "矣", "焉", "乎", "哉"]))
            {
                delta -= 6.0 * s;
            } else if node.start_idx > 0
                && matches!(
                    prev_word,
                    "知" | "古"
                        | "天下"
                        | "學"
                        | "憂"
                        | "樂"
                        | "意"
                        | "蓮"
                        | "次"
                        | "千"
                        | "千里"
                        | "萬"
                        | "失"
                        | "言"
                        | "行"
                        | "道"
                        | "德"
                        | "仁"
                        | "義"
                        | "君"
                        | "民"
                        | "師"
                        | "人"
                        | "生"
                        | "死"
                        | "愛"
                        | "謂"
                        | "聞"
                        | "見"
                        | "求"
                        | "得"
                        | "守"
                        | "治"
                        | "修"
                        | "出"
                        | "入"
                        | "夫"
                        | "蝨"
                        | "處"
                        | "心"
                        | "身"
                        | "家"
                        | "國"
                        | "世"
                )
            {
                delta += 10.0 * s;
            } else if prev_word == "為" && !right_has_any(&["之"]) {
                delta += 6.0 * s;
            }
        }
        if prev_word == "之" {
            if matches!(
                curr,
                "為" | "學"
                    | "憂"
                    | "樂"
                    | "意"
                    | "出"
                    | "行"
                    | "道"
                    | "間"
                    | "中"
                    | "內"
                    | "外"
                    | "上"
                    | "下"
                    | "初"
                    | "終"
                    | "本"
                    | "末"
                    | "所"
                    | "者"
                    | "心"
                    | "志"
                    | "德"
                    | "才"
                    | "力"
                    | "理"
                    | "法"
                    | "事"
                    | "物"
                    | "人"
                    | "民"
                    | "國"
                    | "家"
                    | "身"
                    | "命"
                    | "毫"
                    | "毫釐"
                    | "學者"
            ) {
                delta += 8.0 * s;
            } else if matches!(curr, "織" | "汁" | "枝" | "脂") {
                delta -= 8.0 * s;
            }
        }

        // 3. 古典連詞「而」（ㄦˊ）
        if node.start_idx > 0 && node.end_idx < n_syllables {
            if curr == "而" {
                delta += 10.0 * s;
            } else if matches!(curr, "兒" | "耳" | "爾" | "二") {
                delta -= 8.0 * s;
            }
        }
        if prev_word == "而"
            && matches!(
                curr,
                "不" | "知"
                    | "憂"
                    | "樂"
                    | "死"
                    | "生"
                    | "後"
                    | "勝"
                    | "好"
                    | "思"
                    | "學"
                    | "行"
                    | "立"
                    | "成"
                    | "已"
                    | "止"
                    | "同"
                    | "和"
                    | "染"
                    | "知新"
                    | "不同"
                    | "不和"
                    | "不染"
                    | "不思"
                    | "不學"
            )
        {
            delta += 8.0 * s;
        }

        // 4. 古典介詞「於」（ㄩˊ）
        let prev_is_yu_head = matches!(
            prev_word,
            "生" | "死"
                | "降"
                | "任"
                | "大任"
                | "耕"
                | "躬耕"
                | "施"
                | "始"
                | "出"
                | "入"
                | "止"
                | "立"
                | "居"
                | "處"
                | "行"
                | "學"
                | "問"
                | "求"
                | "取"
                | "歸"
                | "勝"
                | "優"
                | "劣"
                | "近"
                | "遠"
                | "精"
                | "荒"
                | "成"
                | "毀"
                | "忠"
                | "孝"
                | "安"
                | "樂"
                | "憂"
                | "患"
                | "存"
                | "亡"
                | "志"
                | "遊"
                | "惑"
                | "免"
                | "困"
                | "限"
        );
        if node.start_idx > 0 && node.end_idx < n_syllables {
            if curr == "於" {
                delta += 10.0 * s;
                if prev_is_yu_head {
                    delta += 8.0 * s;
                }
            } else if prev_is_yu_head
                && matches!(curr, "魚" | "余" | "餘" | "愚" | "漁" | "愉" | "逾" | "虞" | "娛")
            {
                delta -= 10.0 * s;
            }
        }
        if prev_word == "於"
            && matches!(
                curr,
                "憂患"
                    | "安樂"
                    | "斯人"
                    | "斯"
                    | "南陽"
                    | "人"
                    | "足下"
                    | "藍"
                    | "勤"
                    | "嬉"
                    | "思"
                    | "隨"
                    | "天"
                    | "地"
                    | "道"
                    | "德"
                    | "民"
                    | "心"
                    | "身"
                    | "家"
                    | "國"
                    | "世"
                    | "時"
                    | "此"
                    | "是"
            )
        {
            delta += 8.0 * s;
        }

        // 5. 古典介詞「以」（ㄧˇ）
        let yi_ctx = matches!(
            prev_word,
            "不" | "所" | "可" | "無" | "何" | "足" | "是" | "故" | "謬"
        ) || right_has_any(&[
            "物", "己", "德", "禮", "仁", "義", "道", "法", "誠", "信", "心", "身", "民", "人",
            "千里", "千", "為",
        ]);
        if yi_ctx {
            if curr == "以" {
                delta += 12.0 * s;
            } else if matches!(curr, "已" | "椅" | "蟻" | "乙") {
                delta -= 10.0 * s;
            }
        }
        if prev_word == "以"
            && matches!(
                curr,
                "物" | "己"
                    | "為"
                    | "千里"
                    | "德"
                    | "禮"
                    | "仁"
                    | "義"
                    | "道"
                    | "法"
                    | "誠"
                    | "信"
                    | "心"
                    | "身"
                    | "民"
                    | "人"
                    | "傳道"
                    | "為師"
            )
        {
            delta += 8.0 * s;
        }

        // 6. 古典名物化後綴「者」（ㄓㄜˇ）
        if node.start_idx > 0 && prev_word.chars().count() == 1 {
            if curr == "者" {
                delta += 10.0 * s;
            } else if matches!(curr, "這" | "折" | "哲" | "浙" | "遮") {
                delta -= 10.0 * s;
            }
        }

        // 7. 代詞「其」（ㄑㄧˊ）、連詞「則」（ㄗㄜˊ）、副詞「亦」（ㄧˋ）
        if node.start_idx > 0 && node.end_idx < n_syllables {
            if curr == "其" {
                delta += 10.0 * s;
            } else if matches!(
                prev_word,
                "善" | "利"
                    | "知"
                    | "見"
                    | "聞"
                    | "觀"
                    | "得"
                    | "失"
                    | "守"
                    | "修"
                    | "正"
                    | "盡"
                    | "窮"
                    | "改"
                    | "忘"
                    | "求"
                    | "取"
                    | "用"
                    | "從"
                    | "成"
                    | "敗"
                    | "為"
                    | "無"
                    | "有"
                    | "不"
                    | "必"
                    | "若"
                    | "則"
            ) && matches!(curr, "期" | "奇" | "騎" | "棋" | "旗" | "齊" | "歧" | "企" | "啟")
            {
                delta -= 8.0 * s;
            }
            if curr == "則" {
                delta += 10.0 * s;
            } else if matches!(curr, "責" | "擇" | "澤" | "賊") {
                delta -= 8.0 * s;
            }
        }
        if prev_word == "其" {
            if matches!(
                curr,
                "事" | "器"
                    | "道"
                    | "德"
                    | "心"
                    | "志"
                    | "身"
                    | "名"
                    | "言"
                    | "行"
                    | "力"
                    | "才"
                    | "智"
                    | "人"
                    | "民"
                    | "亡"
                    | "中"
                    | "間"
                    | "本"
                    | "末"
                    | "實"
                    | "理"
                    | "法"
                    | "義"
            ) {
                delta += 8.0 * s;
            } else if curr == "是" {
                delta -= 8.0 * s;
            }
        }
        if node.end_idx < n_syllables
            && right_has_any(&[
                "能", "可", "不", "無", "有", "然", "如", "若", "復", "自", "相", "未", "非",
                "足", "必", "當", "將", "欲", "知", "為",
            ])
        {
            if curr == "亦" {
                delta += 10.0 * s;
            } else if matches!(
                curr,
                "意" | "義" | "易" | "異" | "議" | "藝" | "益" | "億" | "憶" | "譯"
            ) {
                delta -= 8.0 * s;
            }
        }

        // 8. 禁止副詞「勿」、指示詞「斯」、判斷繫詞「為」（ㄨㄟˊ）、能願副詞「必／欲」與常用搭配
        if right_has_any(&[
            "施", "言", "視", "聽", "動", "忘", "失", "棄", "急", "驕", "欺", "貪", "妄", "輕",
            "怠", "疑", "為", "作", "行", "取", "求",
        ]) {
            if curr == "勿" {
                delta += 14.0 * s;
            } else if matches!(curr, "物" | "務" | "誤" | "霧" | "悟")
                && right_has_any(&["施", "言", "視", "聽", "動", "忘", "棄"])
            {
                delta -= 10.0 * s;
            }
        }
        if right_has_any(&["人", "世", "文", "時", "民", "道", "言"]) {
            if curr == "斯" {
                delta += 12.0 * s;
            } else if matches!(curr, "私" | "司" | "思" | "絲") {
                delta -= 10.0 * s;
            }
        }
        if prev_word == "斯" && matches!(curr, "人" | "世" | "文" | "時" | "民" | "道" | "言") {
            delta += 6.0 * s;
        }
        let wei_prev = matches!(
            prev_word,
            "之" | "知"
                | "不知"
                | "可以"
                | "以"
                | "民"
                | "君"
                | "人"
                | "天"
                | "學"
                | "不"
                | "無"
                | "何"
                | "所"
                | "皆"
                | "相"
                | "互"
        );
        if node.start_idx > 0 {
            if curr == "為" && node.bopomofo == "ㄨㄟˊ" {
                delta += 8.0 * s;
                if wei_prev {
                    delta += 12.0 * s;
                }
            } else if matches!(
                prev_word,
                "之" | "不知" | "可以" | "民" | "君"
            ) && matches!(curr, "圍" | "違" | "危" | "微" | "唯" | "維" | "韋")
            {
                delta -= 8.0 * s;
            }
        }
        if right_has_any(&[
            "有", "先", "須", "能", "可", "信", "果", "勝", "敗", "亡", "興", "成", "得", "失",
            "行", "至", "知", "見", "生", "死",
        ]) {
            if curr == "必" {
                delta += 10.0 * s;
            } else if matches!(curr, "避" | "幣" | "閉" | "畢" | "弊" | "壁") {
                delta -= 8.0 * s;
            }
        }
        if right_has_any(&[
            "善", "窮", "知", "行", "為", "求", "得", "取", "成", "立", "達", "明", "治", "安",
            "平", "勝", "生", "死", "言", "問", "學", "修", "施",
        ]) {
            if curr == "欲" {
                delta += 10.0 * s;
            } else if matches!(
                curr,
                "遇" | "育" | "預" | "域" | "玉" | "獄" | "御" | "愈" | "寓" | "譽" | "裕"
            ) {
                delta -= 8.0 * s;
            }
        }
        if prev_word == "不" && curr == "知" && right_has_any(&["之", "為", "新", "足", "命", "止", "恥"]) {
            delta += 8.0 * s;
        }
        if prev_word == "在" && curr == "乎" {
            delta += 10.0 * s;
        }

        // 9. 古典平行認知（X者見X）、舟楫載物、反義對比繫詞（X為貴…Y為輕）與毫釐差分
        let epistemic_stem = prev_word
            .strip_suffix("者")
            .or_else(|| if prev_word == "者" { prev2_word } else { None });
        if let Some(stem) = epistemic_stem {
            if matches!(
                stem,
                "仁" | "智" | "賢" | "勇" | "善" | "道" | "德" | "忠" | "義"
            ) {
                if curr == "見" && right_has_any(&[stem]) {
                    delta += 16.0 * s;
                } else if curr == "建置" {
                    delta -= 12.0 * s;
                }
            }
        }
        if prev_word == "見" {
            let two_back_stem = prev2_word.and_then(|w| w.strip_suffix("者"));
            if let Some(stem) = two_back_stem {
                if curr == stem {
                    delta += 18.0 * s;
                }
            } else if prev2_word == Some("者")
                && matches!(
                    curr,
                    "仁" | "智" | "賢" | "勇" | "善" | "道" | "德" | "忠" | "義"
                )
            {
                delta += 16.0 * s;
            }
        }

        if matches!(prev_word, "能" | "可" | "以" | "足" | "水" | "亦")
            && matches!(curr, "載舟" | "覆舟" | "載物")
        {
            delta += 12.0 * s;
        }
        if curr == "載" && right_has_any(&["舟", "物", "道", "德"]) {
            delta += 10.0 * s;
        } else if curr == "覆" && right_has_any(&["舟", "亡", "滅"]) {
            delta += 10.0 * s;
        } else if curr == "在" && right_has_any(&["舟", "物"]) {
            delta -= 10.0 * s;
        }

        if prev_word == "為" {
            if matches!(
                prev2_word,
                Some("君" | "身" | "財" | "利" | "名" | "物" | "威" | "罰" | "言" | "器")
            ) {
                if curr == "輕" {
                    delta += 14.0 * s;
                } else if matches!(curr, "清" | "青" | "卿" | "傾" | "氫") {
                    delta -= 10.0 * s;
                }
            } else if matches!(
                prev2_word,
                Some("民" | "和" | "禮" | "義" | "信" | "誠" | "仁" | "德" | "道" | "孝")
            ) {
                if curr == "貴" {
                    delta += 12.0 * s;
                } else if matches!(curr, "桂" | "跪" | "櫃" | "軌" | "鬼") {
                    delta -= 8.0 * s;
                }
            }
        }
        if prev_word == "社稷" && curr == "次" && right_has_any(&["之"]) {
            delta += 10.0 * s;
        } else if prev_word == "次" && curr == "之" {
            delta += 8.0 * s;
        }

        if ((prev_word == "之" && prev2_word == Some("失")) || prev_word == "失之")
            && (curr == "毫釐" || (curr == "毫" && right_has_any(&["釐"])))
        {
            delta += 16.0 * s;
        }
        if matches!(curr, "失" | "師")
            && right.is_some_and(|ns| {
                ns.iter().any(|m| {
                    m.word == "之"
                        && dag.get(&m.end_idx).is_some_and(|n2| {
                            n2.iter().any(|x| {
                                matches!(x.word.as_str(), "毫釐" | "毫" | "天下" | "人心")
                            })
                        })
                })
            })
        {
            if curr == "失" {
                delta += 18.0 * s;
            } else {
                delta -= 14.0 * s;
            }
        }
        if prev_word == "毫" {
            if curr == "釐" {
                delta += 12.0 * s;
            } else if matches!(curr, "梨" | "離" | "黎" | "狸" | "漓") {
                delta -= 10.0 * s;
            }
        }
        if (prev_word == "毫釐" || (prev_word == "釐" && prev2_word == Some("毫")))
            && curr == "謬"
        {
            delta += 10.0 * s;
        }
        if prev_word == "謬" && curr == "以" {
            delta += 10.0 * s;
        } else if prev_word == "以" && curr == "千里" {
            delta += 8.0 * s;
        }
        if curr == "臣" && right_has_any(&["本", "聞", "願", "請", "愚", "不", "以"]) {
            delta += 10.0 * s;
        }
        if curr == "布衣"
            && (matches!(
                prev_word,
                "本" | "乃" | "雖" | "為" | "皆" | "以" | "自" | "原" | "起"
            ) || right_has_any(&["躬耕", "之", "也", "而"]))
        {
            delta += 20.0 * s;
        } else if prev_word == "本" && curr == "不" && right_has_any(&["衣"]) {
            delta -= 12.0 * s;
        }

        delta.clamp(-30.0, 30.0)
    }
}

#[inline]
fn is_classical_pivot_char(w: &str) -> bool {
    matches!(
        w,
        "之" | "乎"
            | "者"
            | "也"
            | "矣"
            | "焉"
            | "哉"
            | "於"
            | "以"
            | "而"
            | "其"
            | "乃"
            | "則"
            | "亦"
            | "勿"
            | "所"
            | "必"
            | "欲"
            | "為"
            | "非"
            | "可"
            | "不"
            | "能"
            | "無"
            | "吾"
            | "予"
            | "斯"
            | "故"
            | "雖"
    )
}

#[inline]
fn is_modern_anchor(w: &str) -> bool {
    matches!(
        w,
        "什麼"
            | "怎麼"
            | "我們"
            | "你們"
            | "他們"
            | "她們"
            | "它們"
            | "沒有"
            | "這個"
            | "那個"
            | "這些"
            | "那些"
            | "因為"
            | "但是"
            | "如果"
            | "覺得"
            | "需要"
            | "應該"
            | "已經"
            | "正在"
            | "系統"
            | "電腦"
            | "手機"
            | "網路"
            | "軟體"
            | "硬體"
            | "程式"
            | "資料"
            | "模型"
            | "公司"
            | "台灣"
            | "臺灣"
            | "政府"
            | "媒體"
            | "經濟"
            | "社會"
            | "研究"
            | "發展"
            | "問題"
            | "工作"
            | "時間"
            | "現在"
            | "今天"
            | "明天"
            | "昨天"
            | "大家"
            | "自己"
            | "東西"
            | "地方"
            | "時候"
            | "一下"
            | "一直"
            | "一起"
            | "一點"
            | "引擎"
            | "庇護"
            | "瑜伽"
            | "細胞"
            | "纖維"
            | "記憶"
            | "控制"
            | "完全"
            | "給予"
            | "能夠"
    )
}

#[inline]
fn is_classical_core_char(w: &str) -> bool {
    matches!(
        w,
        "之" | "乎"
            | "者"
            | "也"
            | "矣"
            | "焉"
            | "哉"
            | "於"
            | "以"
            | "而"
            | "其"
            | "乃"
            | "則"
            | "亦"
            | "勿"
            | "斯"
            | "吾"
            | "予"
            | "汝"
            | "弗"
            | "毋"
            | "豈"
            | "莫"
            | "咸"
            | "猶"
            | "躬"
            | "淤"
            | "稷"
            | "翁"
            | "君"
            | "臣"
            | "師"
            | "道"
            | "德"
            | "仁"
            | "義"
            | "禮"
            | "智"
            | "信"
            | "善"
            | "惡"
            | "憂"
            | "樂"
            | "惑"
            | "染"
            | "耕"
            | "施"
            | "載"
            | "覆"
            | "舟"
            | "器"
            | "貴"
            | "輕"
            | "窮"
            | "欲"
            | "必"
            | "故"
            | "雖"
            | "唯"
            | "惟"
            | "為"
            | "非"
            | "無"
            | "有"
            | "知"
            | "學"
            | "行"
            | "言"
            | "思"
            | "修"
            | "治"
            | "平"
            | "安"
            | "患"
            | "生"
            | "死"
            | "存"
            | "亡"
            | "興"
            | "廢"
            | "成"
            | "敗"
            | "得"
            | "失"
            | "始"
            | "終"
            | "古"
            | "今"
            | "天"
            | "地"
            | "民"
            | "國"
            | "社"
            | "布"
            | "衣"
            | "蓮"
            | "泥"
            | "醉"
            | "酒"
            | "山"
            | "水"
            | "目"
            | "樓"
            | "足"
            | "名"
            | "常"
            | "物"
            | "喜"
            | "悲"
            | "己"
            | "獨"
            | "愛"
            | "溫"
            | "新"
            | "降"
            | "任"
            | "利"
            | "事"
            | "傳"
            | "受"
            | "業"
            | "解"
            | "和"
            | "同"
            | "先"
            | "後"
            | "遠"
            | "近"
            | "明"
            | "清"
            | "虛"
            | "實"
            | "動"
            | "靜"
            | "剛"
            | "柔"
            | "進"
            | "退"
            | "罔"
            | "殆"
            | "青"
            | "藍"
            | "勝"
            | "坦"
            | "蕩"
            | "戚"
            | "敏"
            | "好"
            | "恥"
            | "問"
            | "勤"
            | "荒"
            | "嬉"
            | "毀"
            | "隨"
            | "果"
            | "毫"
            | "釐"
            | "謬"
            | "匹"
            | "夫"
            | "責"
            | "人"
            | "不"
            | "工"
            | "長"
            | "是"
            | "可"
            | "在"
            | "心"
            | "身"
            | "家"
            | "世"
            | "時"
            | "一"
            | "三"
            | "千"
            | "萬"
            | "上"
            | "下"
            | "中"
            | "子"
            | "王"
            | "士"
            | "文"
            | "武"
            | "書"
            | "詩"
    )
}

#[inline]
fn is_ultra_classical_char(w: &str) -> bool {
    matches!(
        w,
        "矣" | "焉"
            | "哉"
            | "勿"
            | "吾"
            | "予"
            | "汝"
            | "弗"
            | "毋"
            | "豈"
            | "咸"
            | "淤"
            | "稷"
            | "翁"
            | "罔"
            | "殆"
            | "釐"
            | "謬"
            | "嬉"
    )
}

#[inline]
fn is_modern_colloquial_distractor_char(w: &str) -> bool {
    matches!(
        w,
        "煙" | "野"
            | "耶"
            | "椰"
            | "嗎"
            | "呢"
            | "吧"
            | "啊"
            | "啦"
            | "嘛"
            | "呀"
            | "哇"
            | "喔"
            | "這"
            | "那"
            | "哪"
            | "個"
            | "們"
            | "啥"
            | "咱"
            | "俺"
            | "搞"
            | "弄"
            | "逛"
            | "聊"
            | "酷"
            | "帥"
            | "萌"
            | "爸"
            | "媽"
            | "哥"
            | "姐"
            | "妹"
            | "弟"
            | "輛"
            | "瓶"
            | "罐"
            | "盒"
            | "袋"
            | "箱"
            | "卡"
            | "碼"
            | "廠"
            | "店"
            | "巷"
            | "椅"
            | "蟻"
            | "醃"
            | "淹"
            | "豬"
            | "蛋"
            | "麵"
            | "鍋"
            | "桌"
            | "錶"
            | "塑"
            | "膠"
            | "電"
            | "腦"
            | "訊"
            | "網"
            | "頁"
            | "檔"
            | "鍵"
            | "鼠"
            | "碟"
            | "頻"
            | "播"
            | "拍"
            | "私"
            | "司"
            | "絲"
    )
}

#[inline]
pub fn is_classical_disyllable(w: &str) -> bool {
    matches!(
        w,
        "天下"
            | "所以"
            | "可以"
            | "非常"
            | "君子"
            | "小人"
            | "憂患"
            | "安樂"
            | "社稷"
            | "山水"
            | "之間"
            | "淤泥"
            | "不染"
            | "布衣"
            | "躬耕"
            | "南陽"
            | "斯人"
            | "大任"
            | "千里"
            | "足下"
            | "傳道"
            | "受業"
            | "解惑"
            | "溫故"
            | "知新"
            | "學者"
            | "醉翁"
            | "之意"
            | "不在"
            | "在乎"
            | "三人"
            | "載舟"
            | "覆舟"
            | "一層"
            | "更上"
            | "不知"
            | "不同"
            | "不和"
            | "不欲"
            | "不喜"
            | "不悲"
            | "不屈"
            | "不移"
            | "不可"
            | "不能"
            | "不敢"
            | "不忍"
            | "不為"
            | "不失"
            | "不言"
            | "不倦"
            | "不厭"
            | "不殆"
            | "不惑"
            | "不朽"
            | "仁義"
            | "道德"
            | "禮樂"
            | "忠孝"
            | "廉恥"
            | "聖賢"
            | "賢者"
            | "智者"
            | "仁者"
            | "勇者"
            | "古人"
            | "今人"
            | "世人"
            | "眾人"
            | "聖人"
            | "賢人"
            | "哲人"
            | "故人"
            | "友人"
            | "夫人"
            | "大人"
            | "庶人"
            | "匹夫"
            | "丈夫"
            | "天地"
            | "日月"
            | "星辰"
            | "風雨"
            | "霜雪"
            | "春夏"
            | "秋冬"
            | "江山"
            | "河山"
            | "江河"
            | "湖海"
            | "草木"
            | "花鳥"
            | "詩書"
            | "文章"
            | "典籍"
            | "春秋"
            | "戰國"
            | "秦漢"
            | "唐宋"
            | "明清"
            | "四海"
            | "九州"
            | "中原"
            | "江南"
            | "朝廷"
            | "廟堂"
            | "江湖"
            | "黎民"
            | "百姓"
            | "蒼生"
            | "萬物"
            | "萬民"
            | "千古"
            | "百年"
            | "大道"
            | "大義"
            | "大德"
            | "大志"
            | "大業"
            | "正道"
            | "正義"
            | "正氣"
            | "中庸"
            | "至誠"
            | "至善"
            | "無為"
            | "無私"
            | "無窮"
            | "無盡"
            | "有道"
            | "有德"
            | "有志"
            | "有責"
            | "自然"
            | "逍遙"
            | "清明"
            | "清廉"
            | "高潔"
            | "從容"
            | "淡泊"
            | "寧靜"
            | "致遠"
            | "明志"
            | "知己"
            | "知音"
            | "知足"
            | "慎獨"
            | "篤行"
            | "明辨"
            | "審問"
            | "博學"
            | "好學"
            | "勤學"
            | "力行"
            | "師道"
            | "弟子"
            | "功名"
            | "利祿"
            | "富貴"
            | "貧賤"
            | "榮辱"
            | "得失"
            | "成敗"
            | "興亡"
            | "存亡"
            | "生死"
            | "禍福"
            | "是非"
            | "善惡"
            | "美醜"
            | "真偽"
            | "虛實"
            | "動靜"
            | "剛柔"
            | "輕重"
            | "先後"
            | "本末"
            | "始終"
            | "遠近"
            | "高低"
            | "上下"
            | "內外"
            | "古今"
            | "進退"
            | "取捨"
            | "悲歡"
            | "喜怒"
            | "哀樂"
            | "寒暑"
            | "陰陽"
            | "乾坤"
            | "自強"
            | "不息"
            | "厚德"
            | "載物"
            | "克己"
            | "復禮"
            | "忠恕"
            | "謙遜"
            | "恭敬"
            | "勤儉"
            | "節儉"
            | "廉潔"
            | "公正"
            | "正直"
            | "斯文"
            | "風雅"
            | "雅正"
            | "典雅"
            | "毫釐"
            | "坦蕩"
            | "下問"
            | "生於"
            | "死於"
            | "出於"
            | "始於"
            | "施於"
            | "必有"
            | "不以"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classical_register_mode_boundaries() {
        let mut dag = HashMap::new();
        dag.insert(0, vec![LatticeNode::new(0, 1, "於", 300, "ㄩˊ")]);
        dag.insert(1, vec![LatticeNode::new(1, 2, "此", 400, "ㄘˇ")]);

        let modern_off =
            ClassicalRegisterProfile::detect(ClassicalRegisterMode::ModernOnly, 2, &dag);
        assert!(!modern_off.active);
        assert_eq!(modern_off.node_score_delta(&LatticeNode::new(0, 1, "也", 500, "ㄧㄝˇ"), 2), 0.0);

        let auto_short = ClassicalRegisterProfile::detect(ClassicalRegisterMode::Auto, 2, &dag);
        assert!(!auto_short.active);

        let explicit =
            ClassicalRegisterProfile::detect(ClassicalRegisterMode::ClassicalExplicit, 2, &dag);
        assert!(explicit.active);
        assert_eq!(explicit.strength, 1.0);

        assert_eq!(
            ClassicalRegisterMode::from_config_str("auto"),
            ClassicalRegisterMode::Auto
        );
        assert_eq!(
            ClassicalRegisterMode::from_config_str("off"),
            ClassicalRegisterMode::ModernOnly
        );
        assert_eq!(
            ClassicalRegisterMode::from_config_str("explicit"),
            ClassicalRegisterMode::ClassicalExplicit
        );
        assert_eq!(
            ClassicalRegisterMode::from_config_str("unexpected"),
            ClassicalRegisterMode::Auto
        );
        assert_eq!(ClassicalRegisterMode::Auto.as_config_str(), "auto");
        assert_eq!(ClassicalRegisterMode::ModernOnly.as_config_str(), "off");
        assert_eq!(ClassicalRegisterMode::ClassicalExplicit.as_config_str(), "explicit");
    }
}
