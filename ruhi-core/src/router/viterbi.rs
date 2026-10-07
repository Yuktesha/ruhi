/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/viterbi.rs - 雙向 Viterbi 最優路徑動態規劃路由

use std::collections::HashMap;
use super::lattice::LatticeNode;

pub struct ViterbiRouter {
    pub syllable_base_score: f64,
    pub multi_char_bonus: f64,
}

impl Default for ViterbiRouter {
    fn default() -> Self {
        Self {
            syllable_base_score: 10.0,
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
                for node in nodes {
                    let j = node.end_idx;
                    let word_len = node.length() as f64;
                    let base_weight = (std::cmp::max(1, node.weight)) as f64;

                    let syllable_score = word_len * self.syllable_base_score;
                    let freq_score = base_weight.ln();
                    let length_bonus = if word_len > 1.0 {
                        let cohesion_extra = if word_len >= 4.0 {
                            16.0
                        } else if word_len >= 3.0 {
                            8.0
                        } else {
                            0.0
                        };
                        (word_len - 1.0) * self.multi_char_bonus + cohesion_extra
                    } else {
                        0.0
                    };
                    let self_score = syllable_score + freq_score + length_bonus;

                    let mut best_incoming_score = -f64::INFINITY;
                    let mut best_prev_idx = 0;

                    for (prev_idx, prev_state) in trellis[i].iter().enumerate() {
                        let trans_bonus = if let Some(ref mut engine) = transitions {
                            if !prev_state.node.word.is_empty() {
                                engine.calculate_transition_bonus(
                                    &prev_state.node.word,
                                    &prev_state.node.bopomofo,
                                    &node.word,
                                    &node.bopomofo,
                                )
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        let total = prev_state.score + trans_bonus + self_score;
                        if total > best_incoming_score {
                            best_incoming_score = total;
                            best_prev_idx = prev_idx;
                        }
                    }

                    if best_incoming_score > -f64::INFINITY {
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

            // 束搜尋剪枝 (Beam Search Pruning)：保留每一邊界最高分前 32 個狀態，確保效能與內存安全
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
}
