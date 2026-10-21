/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/viterbi.rs - 結合上下文回溯之詞網格束搜尋路由 (Context-Aware Lattice Beam Search Router)

use std::collections::HashMap;
use super::lattice::LatticeNode;

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
        let initial_word = prefix_word.unwrap_or("");
        let dummy = LatticeNode::new(0, 0, initial_word, 1, "");
        trellis[0].push(TrellisState {
            node: dummy,
            score: 0.0,
            prev_boundary: 0,
            prev_state_idx: 0,
        });

        for i in 0..n_syllables {
            if trellis[i].is_empty() {
                continue;
            }

            if let Some(nodes) = dag.get(&i) {
                let (w_freq, w_mchar, w_coh3, w_coh4, w_coord_yu, w_coord_he) = if let Some(ref eng) = transitions {
                    (
                        eng.feature_weights.get("freq_ln").copied().unwrap_or(1.0),
                        eng.feature_weights.get("multi_char_len").copied().unwrap_or(1.0),
                        eng.feature_weights.get("cohesion_len3").copied().unwrap_or(1.0),
                        eng.feature_weights.get("cohesion_len4").copied().unwrap_or(1.0),
                        eng.feature_weights.get("coord_prior").copied().unwrap_or(1.0),
                        eng.feature_weights.get("coord_he_huo_prior").copied().unwrap_or(0.0),
                    )
                } else {
                    (1.0, 1.0, 1.0, 1.0, 1.0, 0.0)
                };

                for node in nodes {
                    let j = node.end_idx;
                    let word_len = node.length() as f64;
                    let base_weight = (std::cmp::max(1, node.weight)) as f64;

                    let freq_score = base_weight.ln() * w_freq;
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
                                let lookback_slice = &lookback_buf[MAX_LOOKBACK - lb_count..MAX_LOOKBACK];
                                b += engine.calculate_vo_lookback_bonus(lookback_slice, &node.word);
                                b += engine.calculate_hierarchical_construction_bonus(
                                    lookback_slice,
                                    &node.word,
                                    &node.bopomofo,
                                    j,
                                    dag,
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
            .max_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap_or(std::cmp::Ordering::Equal))
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
        let (path, score) = self.solve_best_path(n_syllables, &constrained_dag, transitions);
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

        let mut prefix_words: Vec<&str> = Vec::with_capacity(path.len());
        for (idx, node) in path.iter().enumerate() {
            let word_len = node.length() as f64;
            let base_weight = (std::cmp::max(1, node.weight)) as f64;
            *features.entry("freq_ln".to_string()).or_insert(0.0) += base_weight.ln();
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
