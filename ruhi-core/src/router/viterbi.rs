/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/viterbi.rs - 結合上下文回溯之詞網格束搜尋路由 (Context-Aware Lattice Beam Search Router)

use std::collections::HashMap;
use super::lattice::LatticeNode;
use super::morpheme_ontology::{MorphemeClass, MorphemeMask256, MorphemeOntology};

/// 詞網格後向動態規劃預掃描輪廓 (`LatticeBackwardProfile`)
/// 於前向束搜尋啟動前，以線性時間 $O(E)$ 自句尾 `n_syllables` 向句首反向計算：
/// 1. 每個音節邊界 `pos` 之最佳後綴路徑位勢 `beta_score[pos]`；
/// 2. 過濾低分同音雜訊後之競爭性右鄰 256-bit 語素語意遮罩 `competitive_right_mask[pos]`；
/// 3. 最佳右鄰首詞 `best_right_word[pos]`；
/// 4. 句尾／語氣詞穿透句尾價態標記 `is_clause_tail[pos]` 與 `is_clause_tail_or_particle[pos]`。
#[derive(Debug, Clone)]
pub struct LatticeBackwardProfile {
    pub n_syllables: usize,
    pub beta_score: Vec<f64>,
    pub competitive_right_mask: Vec<MorphemeMask256>,
    pub best_right_word: Vec<String>,
    pub is_clause_tail: Vec<bool>,
    pub is_clause_tail_or_particle: Vec<bool>,
}

impl LatticeBackwardProfile {
    pub fn empty() -> Self {
        Self {
            n_syllables: 0,
            beta_score: Vec::new(),
            competitive_right_mask: Vec::new(),
            best_right_word: Vec::new(),
            is_clause_tail: Vec::new(),
            is_clause_tail_or_particle: Vec::new(),
        }
    }

    pub fn compute(
        router: &ViterbiRouter,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: Option<&super::transitions::TransitionEngine>,
    ) -> Self {
        if n_syllables == 0 {
            return Self::empty();
        }

        let mut beta_score = vec![-f64::INFINITY; n_syllables + 1];
        beta_score[n_syllables] = 0.0;
        let mut competitive_right_mask = vec![MorphemeMask256::EMPTY; n_syllables + 1];
        let mut best_right_word = vec![String::new(); n_syllables + 1];
        let mut best_right_bpm = vec![String::new(); n_syllables + 1];
        let mut is_clause_tail = vec![false; n_syllables + 1];
        is_clause_tail[n_syllables] = true;
        let mut is_clause_tail_or_particle = vec![false; n_syllables + 1];
        is_clause_tail_or_particle[n_syllables] = true;

        let (w_freq_c1, w_freq_cmp, w_mchar, w_coh3, w_coh4) = if let Some(eng) = transitions {
            (
                eng.feature_weights
                    .get("freq_ln_char1")
                    .or_else(|| eng.feature_weights.get("freq_ln"))
                    .copied()
                    .unwrap_or(1.0),
                eng.feature_weights
                    .get("freq_ln_compound")
                    .or_else(|| eng.feature_weights.get("freq_ln"))
                    .copied()
                    .unwrap_or(1.0),
                eng.feature_weights.get("multi_char_len").copied().unwrap_or(1.0),
                eng.feature_weights.get("cohesion_len3").copied().unwrap_or(1.0),
                eng.feature_weights.get("cohesion_len4").copied().unwrap_or(1.0),
            )
        } else {
            (1.0, 1.0, 1.0, 1.0, 1.0)
        };

        let ont = MorphemeOntology::global();

        for pos in (0..n_syllables).rev() {
            let nodes = match dag.get(&pos) {
                Some(ns) => ns,
                None => continue,
            };

            let mut scored_edges: Vec<(f64, &LatticeNode)> = Vec::with_capacity(nodes.len());
            let mut best_cand = -f64::INFINITY;
            let mut best_word = String::new();
            let mut best_bpm = String::new();

            for node in nodes {
                let j = node.end_idx;
                if j <= pos || j > n_syllables {
                    continue;
                }
                if beta_score[j].is_infinite() {
                    continue;
                }

                let word_len = node.length() as f64;
                let base_weight = (std::cmp::max(1, node.weight)) as f64;
                let freq_score = base_weight.ln() * if word_len > 1.0 { w_freq_cmp } else { w_freq_c1 };
                let length_bonus = if word_len > 1.0 {
                    let cohesion_extra = if word_len >= 4.0 {
                        16.0 * w_coh4
                    } else if word_len >= 3.0 {
                        8.0 * w_coh3
                    } else {
                        0.0
                    };
                    (word_len - 1.0) * router.multi_char_bonus * w_mchar + cohesion_extra
                } else {
                    0.0
                };
                let archaic_penalty = if crate::router::archaic_table::is_archaic_polyphonic(&node.word, &node.bopomofo) {
                    -45.0
                } else {
                    0.0
                };
                let self_score = freq_score + length_bonus + archaic_penalty;

                let right_bigram = if j < n_syllables && !best_right_word[j].is_empty() {
                    transitions.map_or(0.0, |eng| {
                        eng.peek_static_bigram_bonus(
                            &node.word,
                            &node.bopomofo,
                            &best_right_word[j],
                            &best_right_bpm[j],
                        )
                    })
                } else {
                    0.0
                };

                let cand = self_score + right_bigram + beta_score[j];
                if cand > best_cand {
                    best_cand = cand;
                    best_word = node.word.clone();
                    best_bpm = node.bopomofo.clone();
                }
                scored_edges.push((cand, node));
            }

            if best_cand > -f64::INFINITY {
                beta_score[pos] = best_cand;
                best_right_word[pos] = best_word;
                best_right_bpm[pos] = best_bpm;

                const MARGIN: f64 = 14.0;
                for (cand, node) in scored_edges {
                    if cand >= best_cand - MARGIN {
                        let m = ont.mask_of(&node.word);
                        competitive_right_mask[pos] |= m;
                        if m.intersects(MorphemeClass::SentenceFinalParticle.bit())
                            && (is_clause_tail[node.end_idx] || is_clause_tail_or_particle[node.end_idx])
                        {
                            is_clause_tail_or_particle[pos] = true;
                        }
                    }
                }
            }
        }

        Self {
            n_syllables,
            beta_score,
            competitive_right_mask,
            best_right_word,
            is_clause_tail,
            is_clause_tail_or_particle,
        }
    }

    #[inline]
    pub fn competitive_mask_at(&self, pos: usize) -> MorphemeMask256 {
        self.competitive_right_mask
            .get(pos)
            .copied()
            .unwrap_or(MorphemeMask256::EMPTY)
    }

    #[inline]
    pub fn right_has_class(&self, pos: usize, cls: MorphemeClass) -> bool {
        self.competitive_mask_at(pos).intersects(cls.bit())
    }

    #[inline]
    pub fn right_has_any(&self, pos: usize, mask: MorphemeMask256) -> bool {
        self.competitive_mask_at(pos).intersects(mask)
    }

    #[inline]
    pub fn best_right_word_at(&self, pos: usize) -> &str {
        self.best_right_word
            .get(pos)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    #[inline]
    pub fn is_clause_tail_at(&self, pos: usize) -> bool {
        pos >= self.n_syllables || self.is_clause_tail.get(pos).copied().unwrap_or(true)
    }

    #[inline]
    pub fn is_clause_tail_or_particle_at(&self, pos: usize) -> bool {
        pos >= self.n_syllables || self.is_clause_tail_or_particle.get(pos).copied().unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub struct ViterbiRouter {
    pub syllable_base_score: f64,
    pub multi_char_bonus: f64,
}

impl Default for ViterbiRouter {
    fn default() -> Self {
        Self {
            syllable_base_score: 0.0,
            multi_char_bonus: 28.0,
        }
    }
}

impl ViterbiRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn solve_best_path(&self, n_syllables: usize, dag: &HashMap<usize, Vec<LatticeNode>>, transitions: Option<&mut super::transitions::TransitionEngine>) -> (Vec<LatticeNode>, f64) {
        self.solve_best_path_with_prefix(n_syllables, dag, transitions, None)
    }

    pub fn solve_best_path_with_prefix(&self, n_syllables: usize, dag: &HashMap<usize, Vec<LatticeNode>>, mut transitions: Option<&mut super::transitions::TransitionEngine>, prefix_word: Option<&str>) -> (Vec<LatticeNode>, f64) {
        if n_syllables == 0 {
            return (Vec::new(), 0.0);
        }

        #[derive(Clone)]
        struct TrellisState {
            node: LatticeNode,
            score: f64,
            prev_boundary: usize,
            prev_state_idx: usize,
        }

        let mut trellis: Vec<Vec<TrellisState>> = vec![Vec::new(); n_syllables + 1];
        let raw_prefix = prefix_word.unwrap_or("");
        let (geo_ctx_prefix, initial_word) = match raw_prefix.rsplit_once('\t') {
            Some((geo_ctx, last_w)) => (geo_ctx, last_w),
            None => ("", raw_prefix),
        };
        let dummy = LatticeNode::new(0, 0, initial_word, 1, "");
        trellis[0].push(TrellisState {
            node: dummy,
            score: 0.0,
            prev_boundary: 0,
            prev_state_idx: 0,
        });

        if let Some(ref mut eng) = transitions {
            eng.maybe_reload();
        }
        let backward = LatticeBackwardProfile::compute(self, n_syllables, dag, transitions.as_deref());

        for i in 0..n_syllables {
            if trellis[i].is_empty() {
                continue;
            }

            if let Some(nodes) = dag.get(&i) {
                let (w_freq_c1, w_freq_cmp, w_mchar, w_coh3, w_coh4, w_coord_yu, w_coord_he) = if let Some(ref eng) = transitions {
                    (
                        eng.feature_weights
                            .get("freq_ln_char1")
                            .or_else(|| eng.feature_weights.get("freq_ln"))
                            .copied()
                            .unwrap_or(1.0),
                        eng.feature_weights
                            .get("freq_ln_compound")
                            .or_else(|| eng.feature_weights.get("freq_ln"))
                            .copied()
                            .unwrap_or(1.0),
                        eng.feature_weights.get("multi_char_len").copied().unwrap_or(1.0),
                        eng.feature_weights.get("cohesion_len3").copied().unwrap_or(1.0),
                        eng.feature_weights.get("cohesion_len4").copied().unwrap_or(1.0),
                        eng.feature_weights.get("coord_prior").copied().unwrap_or(1.0),
                        eng.feature_weights.get("coord_he_huo_prior").copied().unwrap_or(0.0),
                    )
                } else {
                    (1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0)
                };

                for node in nodes {
                    let j = node.end_idx;
                    let word_len = node.length() as f64;
                    let base_weight = (std::cmp::max(1, node.weight)) as f64;

                    let freq_score = base_weight.ln() * if word_len > 1.0 { w_freq_cmp } else { w_freq_c1 };
                    let length_bonus = if word_len > 1.0 {
                        let cohesion_extra = if word_len >= 4.0 {
                            16.0 * w_coh4
                        } else if word_len >= 3.0 {
                            8.0 * w_coh3
                        } else {
                            0.0
                        };
                        (word_len - 1.0) * self.multi_char_bonus * w_mchar + cohesion_extra
                    } else {
                        0.0
                    };
                    let self_score = freq_score + length_bonus;

                    let is_coord_conj = matches!(node.word.as_str(), "與" | "和" | "或");
                    let mut best_incoming_score = -f64::INFINITY;
                    let mut best_prev_idx = 0;
                    let mut coord_candidates: Vec<(f64, usize)> = Vec::new();

                    for (prev_idx, prev_state) in trellis[i].iter().enumerate() {
                        let trans_bonus = if let Some(ref mut engine) = transitions {
                            if !prev_state.node.word.is_empty() {
                                let mut b = engine.calculate_transition_bonus(
                                    &prev_state.node.word,
                                    &prev_state.node.bopomofo,
                                    &node.word,
                                    &node.bopomofo,
                                );

                                // Layer 3: 沿 trellis 的 prev_boundary 與 prev_state_idx 往前輕量回溯最多 12 個前驅詞（棧上固定緩衝陣列，零 Heap 配置）
                                const MAX_LOOKBACK: usize = 12;
                                let mut lookback_buf: [&str; MAX_LOOKBACK] = [""; MAX_LOOKBACK];
                                let mut lb_count = 0;
                                lookback_buf[MAX_LOOKBACK - 1 - lb_count] = prev_state.node.word.as_str();
                                lb_count += 1;

                                let mut cur_b = prev_state.prev_boundary;
                                let mut cur_s = prev_state.prev_state_idx;
                                while lb_count < MAX_LOOKBACK && cur_b > 0 {
                                    let st = &trellis[cur_b][cur_s];
                                    if !st.node.word.is_empty() {
                                        lookback_buf[MAX_LOOKBACK - 1 - lb_count] = st.node.word.as_str();
                                        lb_count += 1;
                                    }
                                    if st.prev_boundary == cur_b && st.prev_state_idx == cur_s {
                                        break;
                                    }
                                    cur_b = st.prev_boundary;
                                    cur_s = st.prev_state_idx;
                                }
                                if lb_count < MAX_LOOKBACK && cur_b == 0 && !trellis[0].is_empty() {
                                    let st0 = &trellis[0][0];
                                    if !st0.node.word.is_empty() {
                                        lookback_buf[MAX_LOOKBACK - 1 - lb_count] = st0.node.word.as_str();
                                        lb_count += 1;
                                    }
                                }
                                if !geo_ctx_prefix.is_empty() && cur_b == 0 && lb_count < MAX_LOOKBACK {
                                    lookback_buf[MAX_LOOKBACK - 1 - lb_count] = geo_ctx_prefix;
                                    lb_count += 1;
                                }
                                let lookback_slice = &lookback_buf[MAX_LOOKBACK - lb_count..MAX_LOOKBACK];
                                b += engine.calculate_vo_lookback_bonus(lookback_slice, &node.word);
                                b += engine.calculate_hierarchical_construction_bonus(
                                    lookback_slice,
                                    &node.word,
                                    &node.bopomofo,
                                    j,
                                    dag,
                                    &backward,
                                );
                                if is_coord_conj {
                                    // 並列連詞前驅詞頻特徵：使高頻正字 X 優先置於並列連詞之前
                                    let scale = if node.word == "與" { 0.05 * w_coord_yu } else { 0.05 * w_coord_he };
                                    b += scale * (std::cmp::max(1, prev_state.node.weight) as f64).ln();
                                }
                                b
                            } else {
                                engine.calculate_hierarchical_construction_bonus(
                                    &[],
                                    &node.word,
                                    &node.bopomofo,
                                    j,
                                    dag,
                                    &backward,
                                )
                            }
                        } else {
                            0.0
                        };

                        let total = prev_state.score + trans_bonus + self_score;
                        if is_coord_conj && total > -f64::INFINITY {
                            coord_candidates.push((total, prev_idx));
                        }
                        if total > best_incoming_score {
                            best_incoming_score = total;
                            best_prev_idx = prev_idx;
                        }
                    }

                    if is_coord_conj && !coord_candidates.is_empty() {
                        coord_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                        for (score, prev_idx) in coord_candidates.into_iter().take(4) {
                            trellis[j].push(TrellisState {
                                node: node.clone(),
                                score,
                                prev_boundary: i,
                                prev_state_idx: prev_idx,
                            });
                        }
                    } else if best_incoming_score > -f64::INFINITY {
                        // 若同一邊界已有相同字詞，保留最高分前驅路徑
                        if let Some(existing) = trellis[j].iter_mut().find(|s| s.node.word == node.word) {
                            if best_incoming_score > existing.score {
                                existing.score = best_incoming_score;
                                existing.prev_boundary = i;
                                existing.prev_state_idx = best_prev_idx;
                                existing.node = node.clone();
                            }
                        } else {
                            trellis[j].push(TrellisState {
                                node: node.clone(),
                                score: best_incoming_score,
                                prev_boundary: i,
                                prev_state_idx: best_prev_idx,
                            });
                        }
                    }
                }
            }

            // 束搜尋剪枝 (Beam Search Pruning)：保留每一邊界最高分前 32 個狀態，確保效能與記憶體安全
            if trellis[i + 1].len() > 32 {
                trellis[i + 1].sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                trellis[i + 1].truncate(32);
            }
        }

        let mut best_nodes = Vec::new();
        if let Some((best_idx, best_state)) = trellis[n_syllables]
            .iter()
            .enumerate()
            .max_by(|a, b| {
                a.1.score
                    .partial_cmp(&b.1.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.0.cmp(&a.0))
            })
        {
            let final_score = best_state.score;
            let mut cur_boundary = n_syllables;
            let mut cur_idx = best_idx;

            while cur_boundary > 0 {
                let state = &trellis[cur_boundary][cur_idx];
                best_nodes.push(state.node.clone());
                let next_b = state.prev_boundary;
                let next_i = state.prev_state_idx;
                cur_boundary = next_b;
                cur_idx = next_i;
            }

            best_nodes.reverse();
            (best_nodes, final_score)
        } else {
            (Vec::new(), 0.0)
        }
    }

    pub fn get_candidate_alternatives(&self, dag: &HashMap<usize, Vec<LatticeNode>>, start_idx: usize) -> Vec<String> {
        if let Some(nodes) = dag.get(&start_idx) {
            let mut sorted = nodes.clone();
            sorted.sort_by(|a, b| {
                b.length().cmp(&a.length()).then_with(|| b.weight.cmp(&a.weight))
            });

            let mut candidates = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for n in sorted {
                if seen.insert(n.word.clone()) {
                    candidates.push(n.word);
                }
            }
            return candidates;
        }
        Vec::new()
    }

    /// 強制對齊 (Forced Alignment)：將詞網格限制為與目標句子字元完全相符之節點，解出黃金正確路徑
    pub fn solve_forced_alignment(
        &self,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: Option<&mut super::transitions::TransitionEngine>,
        target_chars: &[char],
    ) -> Option<(Vec<LatticeNode>, f64)> {
        self.solve_forced_alignment_with_prefix(n_syllables, dag, transitions, target_chars, None)
    }

    /// 帶前綴語境之強制對齊 (Forced Alignment with Prefix Context)
    pub fn solve_forced_alignment_with_prefix(
        &self,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: Option<&mut super::transitions::TransitionEngine>,
        target_chars: &[char],
        prefix_word: Option<&str>,
    ) -> Option<(Vec<LatticeNode>, f64)> {
        if target_chars.len() != n_syllables || n_syllables == 0 {
            return None;
        }
        let mut constrained_dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        for (&start_idx, nodes) in dag {
            if start_idx >= n_syllables {
                continue;
            }
            let mut valid = Vec::new();
            for node in nodes {
                let end_idx = node.end_idx;
                if end_idx <= n_syllables && end_idx > start_idx {
                    let expected: String = target_chars[start_idx..end_idx].iter().collect();
                    if node.word == expected {
                        valid.push(node.clone());
                    }
                }
            }
            if !valid.is_empty() {
                constrained_dag.insert(start_idx, valid);
            }
        }
        let (path, score) =
            self.solve_best_path_with_prefix(n_syllables, &constrained_dag, transitions, prefix_word);
        let joined: String = path.iter().map(|n| n.word.as_str()).collect();
        let target_str: String = target_chars.iter().collect();
        if !path.is_empty() && joined == target_str {
            Some((path, score))
        } else {
            None
        }
    }

    /// 擷取指定路徑之線性特徵向量 $f_k(\text{path})$ 與最佳路徑規則觸發清單（供消融分析與平均結構化感知器校準使用）
    pub fn extract_path_features_and_triggers(
        &self,
        path: &[LatticeNode],
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: &mut super::transitions::TransitionEngine,
    ) -> (HashMap<String, f64>, Vec<String>) {
        let mut features: HashMap<String, f64> = HashMap::new();
        let mut triggered_rules: Vec<String> = Vec::new();
        if path.is_empty() {
            return (features, triggered_rules);
        }

        let prev_trace = transitions.active_trace.take();
        transitions.active_trace = Some(Vec::new());

        let n_syl = path.last().map(|n| n.end_idx).unwrap_or(0);
        let backward = LatticeBackwardProfile::compute(self, n_syl, dag, Some(&*transitions));

        let mut prefix_words: Vec<&str> = Vec::with_capacity(path.len());
        for (idx, node) in path.iter().enumerate() {
            let word_len = node.length() as f64;
            let base_weight = (std::cmp::max(1, node.weight)) as f64;
            if word_len > 1.0 {
                *features.entry("freq_ln_compound".to_string()).or_insert(0.0) += base_weight.ln();
            } else {
                *features.entry("freq_ln_char1".to_string()).or_insert(0.0) += base_weight.ln();
            }
            if word_len > 1.0 {
                *features.entry("multi_char_len".to_string()).or_insert(0.0) +=
                    (word_len - 1.0) * self.multi_char_bonus;
                if word_len >= 4.0 {
                    *features.entry("cohesion_len4".to_string()).or_insert(0.0) += 16.0;
                } else if word_len >= 3.0 {
                    *features.entry("cohesion_len3".to_string()).or_insert(0.0) += 8.0;
                }
            }

            if idx > 0 {
                let prev_node = &path[idx - 1];
                let _ = transitions.calculate_transition_bonus(
                    &prev_node.word,
                    &prev_node.bopomofo,
                    &node.word,
                    &node.bopomofo,
                );
                let lb_start = prefix_words.len().saturating_sub(12);
                let lb_slice = &prefix_words[lb_start..];
                let _ = transitions.calculate_vo_lookback_bonus(lb_slice, &node.word);
                let _ = transitions.calculate_hierarchical_construction_bonus(
                    lb_slice,
                    &node.word,
                    &node.bopomofo,
                    node.end_idx,
                    dag,
                    &backward,
                );
                if node.word == "與" {
                    *features.entry("coord_prior".to_string()).or_insert(0.0) +=
                        0.05 * (std::cmp::max(1, prev_node.weight) as f64).ln();
                } else if matches!(node.word.as_str(), "和" | "或") {
                    *features.entry("coord_he_huo_prior".to_string()).or_insert(0.0) +=
                        0.05 * (std::cmp::max(1, prev_node.weight) as f64).ln();
                }
            } else {
                let _ = transitions.calculate_hierarchical_construction_bonus(
                    &[],
                    &node.word,
                    &node.bopomofo,
                    node.end_idx,
                    dag,
                    &backward,
                );
            }
            prefix_words.push(node.word.as_str());
        }

        if let Some(trace_events) = transitions.active_trace.take() {
            for (rname, fam, raw_score) in trace_events {
                *features.entry(fam).or_insert(0.0) += raw_score;
                if !triggered_rules.contains(&rname) {
                    triggered_rules.push(rname);
                }
            }
        }
        transitions.active_trace = prev_trace;

        (features, triggered_rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lattice_backward_profile_prunes_noisy_homophones_261111() {
        let router = ViterbiRouter::default();
        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();

        // 構造音節網格：
        // pos 0..1: "樹" (weight 200, MC::TreeHead) vs "數" (weight 200, MC::QuantitativeCountMod)
        // pos 1..3: "部份" (weight 8000, 2-char compound, high score, MC::UiHierarchyAnchor)
        // pos 1..2: "部" (weight 10, low score 1-char homophone noise)
        // pos 2..3: "份" (weight 10)
        // pos 3..4: "吧" (weight 5000, MC::SentenceFinalParticle)
        dag.insert(0, vec![
            LatticeNode::new(0, 1, "樹", 200, "ㄕㄨˋ"),
            LatticeNode::new(0, 1, "數", 200, "ㄕㄨˋ"),
        ]);
        dag.insert(1, vec![
            LatticeNode::new(1, 3, "部份", 8000, "ㄅㄨˋ ㄈㄣˋ"),
            LatticeNode::new(1, 2, "部", 10, "ㄅㄨˋ"),
        ]);
        dag.insert(2, vec![
            LatticeNode::new(2, 3, "份", 10, "ㄈㄣˋ"),
        ]);
        dag.insert(3, vec![
            LatticeNode::new(3, 4, "吧", 5000, "˙ㄅㄚ"),
        ]);

        let backward = LatticeBackwardProfile::compute(&router, 4, &dag, None);

        // 1. 驗證 pos 3（"吧"）直達句尾，其前之 pos 3 具備 clause tail
        assert!(backward.is_clause_tail_at(4));
        assert!(backward.is_clause_tail_or_particle_at(4));
        assert_eq!(backward.best_right_word_at(3), "吧");
        assert!(backward.right_has_class(3, MorphemeClass::SentenceFinalParticle));
        // pos 3 緊接語氣詞且直達句尾 ➔ pos 3 標記為 is_clause_tail_or_particle
        assert!(backward.is_clause_tail_or_particle_at(3));

        // 2. 驗證 pos 1 之最佳右鄰詞為雙字高分詞「部份」而非低分單字「部」
        assert_eq!(backward.best_right_word_at(1), "部份");
        assert!(backward.right_has_class(1, MorphemeClass::UiHierarchyAnchor));

        // 3. 驗證 pos 1..3 高分跨躍使 pos 0 之最佳後綴路徑正確收斂
        assert!(!backward.best_right_word_at(0).is_empty());
    }

    #[test]
    fn test_linear_model_dot_product_equivalence_261111() {
        let router = ViterbiRouter::default();
        let mut eng = crate::router::transitions::TransitionEngine::new();
        eng.feature_weights.insert("freq_ln_char1".to_string(), 1.03);
        eng.feature_weights.insert("freq_ln_compound".to_string(), 1.11);
        eng.feature_weights.insert("multi_char_len".to_string(), 0.99);
        eng.feature_weights.insert("cohesion_len3".to_string(), 1.02);
        eng.feature_weights.insert("coord_prior".to_string(), 1.05);
        eng.feature_weights.insert("construction_topological_core".to_string(), 0.96);
        eng.feature_weights.insert("construction_ctx31_bifurcation".to_string(), 1.04);
        eng.feature_weights.insert("family_core_syntax".to_string(), 0.95);

        // 構造包含 1-char, 2-char, "與" 等不同結構特徵的 DAG
        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        dag.insert(0, vec![
            LatticeNode::new(0, 2, "貓咪", 5000, "ㄇㄠ ㄇㄧ"),
            LatticeNode::new(0, 1, "貓", 2000, "ㄇㄠ"),
        ]);
        dag.insert(1, vec![
            LatticeNode::new(1, 2, "咪", 500, "ㄇㄧ"),
        ]);
        dag.insert(2, vec![
            LatticeNode::new(2, 3, "與", 3000, "ㄩˇ"),
        ]);
        dag.insert(3, vec![
            LatticeNode::new(3, 4, "狗", 2000, "ㄍㄡˇ"),
        ]);

        let (path, best_score) = router.solve_best_path(4, &dag, Some(&mut eng));
        assert!(!path.is_empty());
        let (feats, _) = router.extract_path_features_and_triggers(&path, &dag, &mut eng);
        let dot: f64 = feats
            .iter()
            .map(|(k, v)| v * eng.feature_weights.get(k).copied().unwrap_or(1.0))
            .sum();

        assert!(
            (best_score - dot).abs() < 1e-9,
            "best_score ({}) != dot product ({}), diff = {}",
            best_score,
            dot,
            (best_score - dot).abs()
        );
    }
}
