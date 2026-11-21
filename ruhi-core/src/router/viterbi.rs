/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/viterbi.rs - 結合上下文回溯之詞網格束搜尋路由 (Context-Aware Lattice Beam Search Router)

use std::collections::HashMap;
use super::lattice::LatticeNode;
use super::morpheme_ontology::{MorphemeClass, MorphemeMask256, MorphemeOntology};

const NGRAM_WORD_BI_FLOOR: f64 = -5.5;
const NGRAM_CHAR_PER_CHAR_FLOOR: f64 = -5.0;
const NGRAM_ONO_WB_NEUTRAL: f64 = -3.5;
const NGRAM_ONO_CHAR_NEUTRAL_PER_CHAR: f64 = -2.5;

#[inline]
fn clip_ngram_char_pair(raw_cb: f64, raw_ct: f64, char_len: usize) -> (f64, f64) {
    let floor = NGRAM_CHAR_PER_CHAR_FLOOR * (char_len as f64);
    (raw_cb.max(floor), raw_ct.max(floor))
}

struct NodeEdgeGuards {
    shatters_core_len2_at_start: bool,
    shatters_strong_left_len2: bool,
    shatters_left_len2_300_if_prev_len1: bool,
    next_is_true_numeral_unit: bool,
    max_v_le_weight: u32,
}

#[inline]
fn is_true_numeral_unit_word(w: &str) -> bool {
    matches!(
        w,
        "年" | "月" | "天" | "日" | "歲" | "分" | "秒" | "萬" | "億"
            | "幾" | "多" | "餘" | "個" | "位" | "名" | "人" | "隻" | "條"
            | "張" | "本" | "件" | "次" | "回" | "種" | "項" | "組" | "套"
            | "台" | "輛" | "棟" | "間" | "家" | "座" | "篇" | "頁" | "行"
            | "步" | "點" | "倍" | "成" | "折" | "元" | "塊" | "坪" | "度"
            | "公升" | "公斤" | "公尺" | "公里" | "分鐘" | "小時"
    )
}

fn compute_node_edge_guards(
    node: &LatticeNode,
    nodes_at_start: &[LatticeNode],
    dag: &HashMap<usize, Vec<LatticeNode>>,
    backward: &LatticeBackwardProfile,
) -> NodeEdgeGuards {
    let i = node.start_idx;
    let j = node.end_idx;
    let word_len_u = node.length();

    let shatters_core_len2_at_start = word_len_u == 1
        && backward.max_len2_weight_at.get(i).copied().unwrap_or(0) >= 500
        && node.word.chars().next().is_some_and(|ch| {
            nodes_at_start
                .iter()
                .any(|m| m.length() >= 2 && m.weight >= 500 && !m.word.starts_with(ch))
        });

    let mut shatters_strong_left_len2 = false;
    let mut shatters_left_len2_300_if_prev_len1 = false;
    if word_len_u == 1 && i >= 1 {
        if let Some(ch) = node.word.chars().next() {
            if let Some(prev_ns) = dag.get(&(i - 1)) {
                let check_strong_pos = matches!(
                    node.word.as_str(),
                    "時" | "中" | "裡" | "內" | "上" | "下" | "外"
                );
                for m in prev_ns {
                    if m.length() == 2 && m.end_idx == j && !m.word.ends_with(ch) {
                        if m.weight >= 300 {
                            shatters_left_len2_300_if_prev_len1 = true;
                        }
                        if check_strong_pos && m.weight >= 450 {
                            shatters_strong_left_len2 = true;
                        }
                        if shatters_left_len2_300_if_prev_len1
                            && (!check_strong_pos || shatters_strong_left_len2)
                        {
                            break;
                        }
                    }
                }
            }
        }
    }

    let next_is_true_numeral_unit = node.word == "十"
        && dag
            .get(&j)
            .is_some_and(|next_ns| next_ns.iter().any(|m| is_true_numeral_unit_word(&m.word)));

    let max_v_le_weight = if node.word == "了" && i >= 2 && !backward.is_clause_tail_at(j) {
        dag.get(&(i - 1)).map_or(0, |prev_ns| {
            prev_ns
                .iter()
                .filter(|m| {
                    m.length() == 2
                        && m.end_idx == j
                        && m.word.ends_with('了')
                        && !matches!(
                            m.word.as_str(),
                            "為了" | "除了" | "罷了" | "算了" | "免了"
                                | "得了" | "好了" | "夠了"
                        )
                })
                .map(|m| m.weight)
                .max()
                .unwrap_or(0)
        })
    } else {
        0
    };

    NodeEdgeGuards {
        shatters_core_len2_at_start,
        shatters_strong_left_len2,
        shatters_left_len2_300_if_prev_len1,
        next_is_true_numeral_unit,
        max_v_le_weight,
    }
}

#[inline]
fn build_transition_edge_ctx<'a>(
    guards: &NodeEdgeGuards,
    node: &LatticeNode,
    prev_node: &LatticeNode,
    prev2_word: Option<&'a str>,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    backward: &LatticeBackwardProfile,
) -> super::transitions::TransitionEdgeCtx<'a> {
    let i = node.start_idx;
    let j = node.end_idx;
    let prev_len = prev_node.length();
    let min_prev_wt = if prev_node.word == "太" { 100 } else { 300 };
    let shatters_prev_left_len2 = prev_len == 1
        && i >= 2
        && prev_node.word.chars().next().is_some_and(|pch| {
            dag.get(&(i - 2)).is_some_and(|prev2_ns| {
                prev2_ns.iter().any(|m| {
                    m.length() == 2
                        && m.end_idx == i
                        && m.weight >= min_prev_wt
                        && !m.word.ends_with(pch)
                })
            })
        });
    super::transitions::TransitionEdgeCtx {
        is_clause_tail: backward.is_clause_tail_at(j),
        prev2_word,
        shatters_core_len2_at_start: guards.shatters_core_len2_at_start,
        shatters_strong_left_len2: guards.shatters_strong_left_len2,
        shatters_strong_v_le: guards.max_v_le_weight > 0
            && prev_len == 2
            && guards.max_v_le_weight > prev_node.weight,
        shatters_left_len2_300: guards.shatters_left_len2_300_if_prev_len1 && prev_len == 1,
        shatters_prev_left_len2,
        next_is_true_numeral_unit: guards.next_is_true_numeral_unit,
    }
}

#[inline]
fn is_single_tier_a_surname(w: &str) -> bool {
    let mut it = w.chars();
    matches!((it.next(), it.next()), (Some(c), None) if super::onomastics::is_tier_a_surname(c))
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn is_onomastic_edge_active(
    ono_enabled: bool,
    lookback_slice: &[&str],
    ono_ctx: Option<&super::onomastics::OnomasticPrefixCtx>,
    backward: &LatticeBackwardProfile,
    node: &LatticeNode,
    prev_w: &str,
    prev2_w: Option<&str>,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    session_given_names: &std::collections::HashSet<String>,
) -> bool {
    if !ono_enabled || node.length() > 2 {
        return false;
    }
    let maybe_ono = is_single_tier_a_surname(prev_w)
        || prev2_w.is_some_and(is_single_tier_a_surname)
        || session_given_names.contains(&node.word)
        || (!session_given_names.is_empty() && prev_w.chars().count() == 1 && node.length() == 1);
    if !maybe_ono {
        return false;
    }
    super::onomastics::score_onomastic_bonus_with_ctx(
        lookback_slice,
        ono_ctx,
        Some(backward),
        &node.word,
        &node.bopomofo,
        node.end_idx,
        dag,
        session_given_names,
    )
    .abs()
        > 1e-9
}

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
    pub company_suffix_at: Vec<bool>,
    pub max_len2_weight_at: Vec<u32>,
    pub max_len3_weight_at: Vec<u32>,
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
            company_suffix_at: Vec::new(),
            max_len2_weight_at: Vec::new(),
            max_len3_weight_at: Vec::new(),
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
        let mut company_suffix_at = vec![false; n_syllables + 2];
        let mut max_len2_weight_at = vec![0u32; n_syllables + 2];
        let mut max_len3_weight_at = vec![0u32; n_syllables + 2];

        let (w_freq_c1, w_freq_cmp, w_mchar, w_coh3, w_coh4, w_class) = if let Some(eng) = transitions {
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
                eng.feature_weights.get("classical_register").copied().unwrap_or(1.0),
            )
        } else {
            (1.0, 1.0, 1.0, 1.0, 1.0, 1.0)
        };

        let ont = MorphemeOntology::global();
        let classical = super::classical_register::ClassicalRegisterProfile::detect(
            router.classical_mode,
            n_syllables,
            dag,
        );

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
                if node.weight >= 180 {
                    let c_len = node.word.chars().count();
                    if c_len >= 3 {
                        if node.weight > max_len3_weight_at[pos] {
                            max_len3_weight_at[pos] = node.weight;
                        }
                        if node.weight > max_len2_weight_at[pos] {
                            max_len2_weight_at[pos] = node.weight;
                        }
                    } else if c_len >= 2 && node.weight > max_len2_weight_at[pos] {
                        max_len2_weight_at[pos] = node.weight;
                    }
                }
                if super::onomastics::is_company_suffix_word(&node.word) {
                    company_suffix_at[pos] = true;
                }

                let j = node.end_idx;
                if j <= pos || j > n_syllables {
                    continue;
                }
                if beta_score[j].is_infinite() {
                    continue;
                }

                    let word_len_u = node.length();
                    let word_len = word_len_u as f64;
                    let is_session_name =
                        transitions.is_some_and(|eng| eng.session_given_names.contains(&node.word));
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
                        (word_len - 1.0)
                            * router.effective_multi_char_bonus(
                                &node.word,
                                word_len_u,
                                node.weight,
                                is_session_name,
                            )
                            * w_mchar
                            + cohesion_extra
                    } else {
                        0.0
                    };
                let archaic_penalty = if crate::router::archaic_table::is_archaic_polyphonic(&node.word, &node.bopomofo) {
                    -45.0
                } else {
                    0.0
                };
                let classical_node = classical.node_score_delta(node, n_syllables) * w_class;
                let self_score = freq_score + length_bonus + archaic_penalty + classical_node;

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
            company_suffix_at,
            max_len2_weight_at,
            max_len3_weight_at,
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

/// 判斷多字詞是否屬於高透明度之純句法拼接衍生詞（排除語意已詞彙化之原子名詞與複數人稱代詞），
/// 供 S5 結構化階梯加成使用（保留 100% 選單可達性，適度壓制其跨界搶佔權重）。
#[inline]
pub fn is_narrow_transparent_derived(word: &str) -> bool {
    let mut it = word.chars();
    let Some(c0) = it.next() else { return false };
    let Some(c1) = it.next() else { return false };
    match it.next() {
        None => {
            if matches!(c0, '我' | '你' | '妳' | '他' | '她' | '它' | '牠' | '祂') && c1 != '們' {
                return true;
            }
            if c1 == '的' && !matches!(word, "目的" | "標的" | "別的" | "有的" | "是的" | "新的") {
                return true;
            }
            if matches!(c1, '嗎' | '呢' | '吧' | '啊' | '呀' | '嘛' | '啦') {
                return true;
            }
            matches!(word, "沒辦" | "沒關" | "沒想")
        }
        Some(c2) => {
            if it.next().is_some() {
                return false;
            }
            if matches!(
                word,
                "不見得"
                    | "怪不得"
                    | "恨不得"
                    | "由不得"
                    | "捨不得"
                    | "了不得"
                    | "免不了"
                    | "少不了"
                    | "受不了"
                    | "算不了"
                    | "大不了"
                    | "省不了"
                    | "逃不了"
                    | "免不得"
                    | "來得及"
                    | "不得不"
                    | "不得已"
                    | "不得了"
                    | "對不起"
                    | "了不起"
            ) {
                return false;
            }
            matches!(c2, '的' | '得' | '了' | '著' | '過') || matches!(c1, '得' | '了')
        }
    }
}

/// Optional policy for tiered or weight-modulated `multi_char_bonus` evaluation.
/// Production default uses `MultiCharBonusPolicy::production_s5()`.
#[derive(Debug, Clone, Default)]
pub struct MultiCharBonusPolicy {
    pub weight_ramp_low: Option<u32>,
    pub weight_ramp_high: Option<u32>,
    pub weight_ramp_min_bonus: f64,
    pub use_narrow_derived_tier: bool,
    pub derived_words: Option<std::sync::Arc<std::collections::HashSet<String>>>,
    pub derived_bonus: f64,
}

impl MultiCharBonusPolicy {
    /// S5 統計驗證勝出之零過擬合多字詞階梯加成策略：
    /// - 雙字詞（排除對話期間臨時登錄人名）權重 100→250 平滑線性梯度 20.0→28.0
    /// - 狹義透明衍生詞（`is_narrow_transparent_derived`）單字組總降幅上限為 8.0（雙字詞 20.0、三字詞以上按邊界均分）
    pub fn production_s5() -> Self {
        Self {
            weight_ramp_low: Some(100),
            weight_ramp_high: Some(250),
            weight_ramp_min_bonus: 20.0,
            use_narrow_derived_tier: true,
            derived_words: None,
            derived_bonus: 20.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViterbiRouter {
    pub syllable_base_score: f64,
    pub multi_char_bonus: f64,
    pub bonus_policy: Option<MultiCharBonusPolicy>,
    pub classical_mode: super::classical_register::ClassicalRegisterMode,
}

impl Default for ViterbiRouter {
    fn default() -> Self {
        Self {
            syllable_base_score: 0.0,
            multi_char_bonus: 28.0,
            bonus_policy: Some(MultiCharBonusPolicy::production_s5()),
            classical_mode: super::classical_register::ClassicalRegisterMode::Auto,
        }
    }
}

#[derive(Clone)]
struct ForwardTrellisState {
    node: LatticeNode,
    score: f64,
    prev_boundary: usize,
    prev_state_idx: usize,
    incoming: Vec<(usize, usize, f64)>,
}

impl ViterbiRouter {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn effective_multi_char_bonus(
        &self,
        word: &str,
        word_len: usize,
        weight: u32,
        is_session_name: bool,
    ) -> f64 {
        let Some(ref pol) = self.bonus_policy else {
            return self.multi_char_bonus;
        };
        let mut b = self.multi_char_bonus;
        if word_len == 2 && !is_session_name {
            if let (Some(lo), Some(hi)) = (pol.weight_ramp_low, pol.weight_ramp_high) {
                if hi > lo {
                    let t = if weight <= lo {
                        0.0
                    } else if weight >= hi {
                        1.0
                    } else {
                        (weight - lo) as f64 / (hi - lo) as f64
                    };
                    b = pol.weight_ramp_min_bonus + t * (b - pol.weight_ramp_min_bonus);
                }
            }
        }
        let is_demoted_derived = (pol.use_narrow_derived_tier && is_narrow_transparent_derived(word))
            || pol.derived_words.as_ref().is_some_and(|dset| dset.contains(word));
        if is_demoted_derived && pol.derived_bonus < self.multi_char_bonus {
            let max_total_demotion = self.multi_char_bonus - pol.derived_bonus;
            let capped_b = if word_len >= 3 {
                self.multi_char_bonus - max_total_demotion / ((word_len - 1) as f64)
            } else {
                pol.derived_bonus
            };
            if capped_b < b {
                b = capped_b;
            }
        }
        b
    }

    pub fn solve_best_path(&self, n_syllables: usize, dag: &HashMap<usize, Vec<LatticeNode>>, transitions: Option<&mut super::transitions::TransitionEngine>) -> (Vec<LatticeNode>, f64) {
        self.solve_best_path_with_prefix(n_syllables, dag, transitions, None)
    }

    pub fn solve_best_path_with_prefix(&self, n_syllables: usize, dag: &HashMap<usize, Vec<LatticeNode>>, mut transitions: Option<&mut super::transitions::TransitionEngine>, prefix_word: Option<&str>) -> (Vec<LatticeNode>, f64) {
        if n_syllables == 0 {
            return (Vec::new(), 0.0);
        }

        let want_discourse = n_syllables >= 4
            && !transitions
                .as_deref()
                .is_some_and(|eng| eng.is_rule_disabled("discourse_reranker"))
            && super::discourse_reranker::dag_has_third_person_pronoun(dag);

        let (trellis, classical_active) =
            self.run_forward_trellis(n_syllables, dag, &mut transitions, prefix_word, want_discourse);

        if want_discourse && !classical_active {
            let nbest = Self::extract_nbest_from_trellis(&trellis, n_syllables, 16);
            if !nbest.is_empty() {
                return super::discourse_reranker::rerank_nbest(nbest);
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

    pub fn solve_nbest_paths(
        &self,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: Option<&mut super::transitions::TransitionEngine>,
        top_k: usize,
    ) -> Vec<(Vec<LatticeNode>, f64)> {
        self.solve_nbest_paths_with_prefix(n_syllables, dag, transitions, None, top_k)
    }

    pub fn solve_nbest_paths_with_prefix(
        &self,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        mut transitions: Option<&mut super::transitions::TransitionEngine>,
        prefix_word: Option<&str>,
        top_k: usize,
    ) -> Vec<(Vec<LatticeNode>, f64)> {
        if n_syllables == 0 || top_k == 0 {
            return Vec::new();
        }
        let (trellis, _) =
            self.run_forward_trellis(n_syllables, dag, &mut transitions, prefix_word, true);
        Self::extract_nbest_from_trellis(&trellis, n_syllables, top_k)
    }

    fn run_forward_trellis(
        &self,
        n_syllables: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
        transitions: &mut Option<&mut super::transitions::TransitionEngine>,
        prefix_word: Option<&str>,
        collect_nbest_edges: bool,
    ) -> (Vec<Vec<ForwardTrellisState>>, bool) {
        let mut trellis: Vec<Vec<ForwardTrellisState>> = vec![Vec::new(); n_syllables + 1];
        let raw_prefix = prefix_word.unwrap_or("");
        let (geo_ctx_prefix, initial_word) = match raw_prefix.rsplit_once('\t') {
            Some((geo_ctx, last_w)) => (geo_ctx, last_w),
            None => ("", raw_prefix),
        };
        let dummy = LatticeNode::new(0, 0, initial_word, 1, "");
        trellis[0].push(ForwardTrellisState {
            node: dummy,
            score: 0.0,
            prev_boundary: 0,
            prev_state_idx: 0,
            incoming: Vec::new(),
        });

        if let Some(ref mut eng) = transitions {
            eng.maybe_reload();
        }
        let backward = LatticeBackwardProfile::compute(self, n_syllables, dag, transitions.as_deref());
        let classical = super::classical_register::ClassicalRegisterProfile::detect(
            self.classical_mode,
            n_syllables,
            dag,
        );
        let cache_lm_snap = transitions
            .as_deref()
            .filter(|eng| !eng.is_rule_disabled("cache_lm"))
            .and_then(|eng| super::cache_lm::snapshot_if_non_empty(eng.cache_lm_key()));

        for i in 0..n_syllables {
            if trellis[i].is_empty() {
                continue;
            }

            if let Some(nodes) = dag.get(&i) {
                let (
                    w_freq_c1,
                    w_freq_cmp,
                    w_mchar,
                    w_coh3,
                    w_coh4,
                    w_coord_yu,
                    w_coord_he,
                    w_class,
                    w_ng_wb,
                    w_ng_cb,
                    w_ng_ct,
                ) = if let Some(ref eng) = transitions {
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
                        eng.feature_weights.get("classical_register").copied().unwrap_or(1.0),
                        eng.feature_weights.get("ngram_word_bi").copied().unwrap_or(0.0),
                        eng.feature_weights.get("ngram_char_bi").copied().unwrap_or(0.0),
                        eng.feature_weights.get("ngram_char_tri").copied().unwrap_or(0.0),
                    )
                } else {
                    (1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0)
                };
                let initial_has_cjk = initial_word
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                let allow_ngram = (n_syllables >= 3 || (n_syllables >= 2 && initial_has_cjk))
                    && !classical.active;
                let ngram_lm_ref = if allow_ngram {
                    transitions
                        .as_ref()
                        .filter(|eng| !eng.is_rule_disabled("ngram_lm"))
                        .and_then(|eng| eng.ngram_lm.as_ref().cloned())
                } else {
                    None
                };

                let (trellis_past, trellis_future) = trellis.split_at_mut(i + 1);
                let p_count = trellis_past[i].len();
                const MAX_LOOKBACK: usize = 24;
                let mut lookback_bufs: [[&str; MAX_LOOKBACK]; 32] = [[""; MAX_LOOKBACK]; 32];
                let mut lookback_lens: [usize; 32] = [0usize; 32];
                for prev_idx in 0..p_count.min(32) {
                    let prev_state = &trellis_past[i][prev_idx];
                    if !prev_state.node.word.is_empty() {
                        let mut lb_count = 0usize;
                        lookback_bufs[prev_idx][MAX_LOOKBACK - 1 - lb_count] = prev_state.node.word.as_str();
                        lb_count += 1;

                        let mut cur_b = prev_state.prev_boundary;
                        let mut cur_s = prev_state.prev_state_idx;
                        while lb_count < MAX_LOOKBACK && cur_b > 0 {
                            let st = &trellis_past[cur_b][cur_s];
                            if !st.node.word.is_empty() {
                                lookback_bufs[prev_idx][MAX_LOOKBACK - 1 - lb_count] = st.node.word.as_str();
                                lb_count += 1;
                            }
                            if st.prev_boundary == cur_b && st.prev_state_idx == cur_s {
                                break;
                            }
                            cur_b = st.prev_boundary;
                            cur_s = st.prev_state_idx;
                        }
                        if lb_count < MAX_LOOKBACK && cur_b == 0 && !trellis_past[0].is_empty() {
                            let st0 = &trellis_past[0][0];
                            if !st0.node.word.is_empty() {
                                lookback_bufs[prev_idx][MAX_LOOKBACK - 1 - lb_count] = st0.node.word.as_str();
                                lb_count += 1;
                            }
                        }
                        if !geo_ctx_prefix.is_empty() && cur_b == 0 && lb_count < MAX_LOOKBACK {
                            lookback_bufs[prev_idx][MAX_LOOKBACK - 1 - lb_count] = geo_ctx_prefix;
                            lb_count += 1;
                        }
                        lookback_lens[prev_idx] = lb_count;
                    }
                }

                let geo = super::geo_hierarchy::CjkGeoHierarchy::global();
                let geo_enabled = transitions
                    .as_ref()
                    .map_or(false, |eng| !eng.is_rule_disabled("geo_hierarchy"));
                let mut geo_ctxs: [Option<super::geo_hierarchy::GeoPrefixCtx<'_, '_>>; 32] =
                    std::array::from_fn(|_| None);
                let empty_geo_ctx = if geo_enabled
                    && trellis_past[i].iter().any(|s| s.node.word.is_empty())
                {
                    Some(geo.prepare_prefix_ctx(&[]))
                } else {
                    None
                };
                if geo_enabled {
                    for prev_idx in 0..p_count.min(32) {
                        if !trellis_past[i][prev_idx].node.word.is_empty() {
                            let lb_count = lookback_lens[prev_idx];
                            geo_ctxs[prev_idx] =
                                Some(geo.prepare_prefix_ctx(&lookback_bufs[prev_idx][MAX_LOOKBACK - lb_count..MAX_LOOKBACK]));
                        }
                    }
                }

                let ono_enabled = transitions
                    .as_ref()
                    .map_or(false, |eng| !eng.is_rule_disabled("onomastics"));
                let mut ono_ctxs: [super::onomastics::OnomasticPrefixCtx; 32] =
                    [Default::default(); 32];
                if ono_enabled {
                    for prev_idx in 0..p_count.min(32) {
                        if !trellis_past[i][prev_idx].node.word.is_empty() {
                            let lb_count = lookback_lens[prev_idx];
                            if lb_count >= 2 {
                                ono_ctxs[prev_idx] =
                                    super::onomastics::prepare_onomastic_prefix_ctx(
                                        &lookback_bufs[prev_idx][MAX_LOOKBACK - lb_count..MAX_LOOKBACK],
                                    );
                            }
                        }
                    }
                }

                let mut prev_archaic = [false; 32];
                for prev_idx in 0..p_count.min(32) {
                    let st = &trellis_past[i][prev_idx];
                    if !st.node.word.is_empty() {
                        prev_archaic[prev_idx] =
                            super::archaic_table::is_archaic_polyphonic(&st.node.word, &st.node.bopomofo);
                    }
                }

                for node in nodes {
                    let j = node.end_idx;
                    if j <= i {
                        continue;
                    }
                    let curr_archaic =
                        super::archaic_table::is_archaic_polyphonic(&node.word, &node.bopomofo);
                    let word_len_u = node.length();
                    let word_len = word_len_u as f64;
                    let is_session_name = transitions
                        .as_ref()
                        .is_some_and(|eng| eng.session_given_names.contains(&node.word));
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
                        (word_len - 1.0)
                            * self.effective_multi_char_bonus(
                                &node.word,
                                word_len_u,
                                node.weight,
                                is_session_name,
                            )
                            * w_mchar
                            + cohesion_extra
                    } else {
                        0.0
                    };
                    let classical_node = classical.node_score_delta(node, n_syllables) * w_class;
                    let self_score = freq_score + length_bonus + classical_node;

                    let node_guards = compute_node_edge_guards(node, nodes, dag, &backward);

                    let is_coord_conj = matches!(node.word.as_str(), "與" | "和" | "或");
                    let mut best_incoming_score = -f64::INFINITY;
                    let mut best_prev_idx = 0;
                    let mut coord_candidates: Vec<(f64, usize, f64)> = Vec::new();
                    let mut nbest_edges: Vec<(f64, usize, f64)> = if collect_nbest_edges {
                        Vec::with_capacity(trellis_past[i].len().min(32))
                    } else {
                        Vec::new()
                    };

                    for (prev_idx, prev_state) in trellis_past[i].iter().enumerate() {
                        let prev2_word = if !prev_state.node.word.is_empty()
                            && i > 0
                            && prev_idx < 32
                            && lookback_lens[prev_idx] >= 2
                        {
                            Some(lookback_bufs[prev_idx][MAX_LOOKBACK - 2])
                        } else {
                            None
                        };
                        let classical_trans = classical.transition_score_delta(
                            &prev_state.node.word,
                            prev2_word,
                            node,
                            n_syllables,
                            dag,
                        ) * w_class;
                        let lb_count = if prev_idx < 32 { lookback_lens[prev_idx] } else { 0 };
                        let single_prev = [prev_state.node.word.as_str()];
                        let lookback_slice: &[&str] = if !prev_state.node.word.is_empty() {
                            if prev_idx < 32 {
                                &lookback_bufs[prev_idx][MAX_LOOKBACK - lb_count..MAX_LOOKBACK]
                            } else {
                                &single_prev
                            }
                        } else {
                            &[]
                        };
                        let ono_edge_active = if ngram_lm_ref.is_some() {
                            if let Some(ref eng) = transitions {
                                let octx = if !prev_state.node.word.is_empty() && prev_idx < 32 {
                                    Some(&ono_ctxs[prev_idx])
                                } else {
                                    None
                                };
                                is_onomastic_edge_active(
                                    ono_enabled,
                                    lookback_slice,
                                    octx,
                                    &backward,
                                    node,
                                    &prev_state.node.word,
                                    prev2_word,
                                    dag,
                                    &eng.session_given_names,
                                )
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        let ngram_trans = if let Some(ref lm) = ngram_lm_ref {
                            if !ono_edge_active {
                                let mut ng = 0.0;
                                if w_ng_wb.abs() > 1e-12 {
                                    let wb = lm
                                        .score_word_bigram(&prev_state.node.word, &node.word)
                                        .max(NGRAM_WORD_BI_FLOOR);
                                    ng += w_ng_wb * wb;
                                }
                                if w_ng_cb.abs() > 1e-12 || w_ng_ct.abs() > 1e-12 {
                                    let (raw_cb, raw_ct) = lm.score_char_boundary(
                                        prev2_word,
                                        &prev_state.node.word,
                                        &node.word,
                                    );
                                    let (cb, ct) = clip_ngram_char_pair(raw_cb, raw_ct, word_len_u);
                                    ng += w_ng_cb * cb + w_ng_ct * ct;
                                }
                                ng
                            } else {
                                let c_neutral =
                                    NGRAM_ONO_CHAR_NEUTRAL_PER_CHAR * (word_len_u as f64);
                                w_ng_wb * NGRAM_ONO_WB_NEUTRAL + (w_ng_cb + w_ng_ct) * c_neutral
                            }
                        } else {
                            0.0
                        };
                        let trans_bonus = if let Some(ref mut engine) = transitions {
                            if !prev_state.node.word.is_empty() {
                                let p_arch = if prev_idx < 32 {
                                    prev_archaic[prev_idx]
                                } else {
                                    super::archaic_table::is_archaic_polyphonic(
                                        &prev_state.node.word,
                                        &prev_state.node.bopomofo,
                                    )
                                };
                                let edge_ctx = build_transition_edge_ctx(
                                    &node_guards,
                                    node,
                                    &prev_state.node,
                                    prev2_word,
                                    dag,
                                    &backward,
                                );
                                let mut b = engine.calculate_transition_bonus_with_ctx(
                                    curr_archaic || p_arch,
                                    &prev_state.node.word,
                                    &prev_state.node.bopomofo,
                                    &node.word,
                                    &node.bopomofo,
                                    edge_ctx,
                                );
                                b += engine.eval_functional_collocation_guards(
                                    prev2_word,
                                    &prev_state.node.word,
                                    &node.word,
                                    &node.bopomofo,
                                    i,
                                    j,
                                    dag,
                                    &backward,
                                );

                                b += engine.calculate_vo_lookback_bonus(lookback_slice, &node.word);
                                let gctx = if prev_idx < 32 {
                                    geo_ctxs[prev_idx].as_ref()
                                } else {
                                    None
                                };
                                let octx = if prev_idx < 32 {
                                    Some(&ono_ctxs[prev_idx])
                                } else {
                                    None
                                };
                                b += engine.calculate_hierarchical_construction_bonus_with_ctxs(
                                    lookback_slice,
                                    gctx,
                                    octx,
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
                                let mut b = engine.calculate_hierarchical_construction_bonus_with_ctxs(
                                    &[],
                                    empty_geo_ctx.as_ref(),
                                    None,
                                    &node.word,
                                    &node.bopomofo,
                                    j,
                                    dag,
                                    &backward,
                                );
                                b += engine.eval_functional_collocation_guards(
                                    None,
                                    "",
                                    &node.word,
                                    &node.bopomofo,
                                    i,
                                    j,
                                    dag,
                                    &backward,
                                );
                                b
                            }
                        } else {
                            0.0
                        };

                        let cache_bonus = if let Some(ref clm) = cache_lm_snap {
                            clm.score_edge(&prev_state.node.word, &node.word, j, n_syllables, dag)
                        } else {
                            0.0
                        };

                        let total = prev_state.score
                            + trans_bonus
                            + cache_bonus
                            + classical_trans
                            + ngram_trans
                            + self_score;
                        if total > -f64::INFINITY {
                            let edge_score = total - prev_state.score;
                            if is_coord_conj {
                                coord_candidates.push((total, prev_idx, edge_score));
                            }
                            if collect_nbest_edges {
                                nbest_edges.push((total, prev_idx, edge_score));
                            }
                        }
                        if total > best_incoming_score {
                            best_incoming_score = total;
                            best_prev_idx = prev_idx;
                        }
                    }

                    let future_slot = &mut trellis_future[j - (i + 1)];
                    if is_coord_conj && !coord_candidates.is_empty() {
                        coord_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                        for (score, prev_idx, edge_score) in coord_candidates.into_iter().take(4) {
                            future_slot.push(ForwardTrellisState {
                                node: node.clone(),
                                score,
                                prev_boundary: i,
                                prev_state_idx: prev_idx,
                                incoming: if collect_nbest_edges {
                                    vec![(i, prev_idx, edge_score)]
                                } else {
                                    Vec::new()
                                },
                            });
                        }
                    } else if best_incoming_score > -f64::INFINITY {
                        let top_edges: Vec<(usize, usize, f64)> = if collect_nbest_edges {
                            nbest_edges.sort_by(|a, b| {
                                b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
                            });
                            nbest_edges
                                .into_iter()
                                .take(6)
                                .map(|(_, pidx, es)| (i, pidx, es))
                                .collect()
                        } else {
                            Vec::new()
                        };
                        // 若同一邊界已有相同字詞，保留最高分前驅路徑
                        if let Some(existing) = future_slot.iter_mut().find(|s| s.node.word == node.word) {
                            if best_incoming_score > existing.score {
                                existing.score = best_incoming_score;
                                existing.prev_boundary = i;
                                existing.prev_state_idx = best_prev_idx;
                                existing.node = node.clone();
                            }
                            if collect_nbest_edges {
                                for (pb, ps, es) in top_edges {
                                    if let Some(slot_e) = existing
                                        .incoming
                                        .iter_mut()
                                        .find(|e| e.0 == pb && e.1 == ps)
                                    {
                                        if es > slot_e.2 {
                                            slot_e.2 = es;
                                        }
                                    } else {
                                        existing.incoming.push((pb, ps, es));
                                    }
                                }
                                existing.incoming.sort_by(|a, b| {
                                    let sa = trellis_past[a.0][a.1].score + a.2;
                                    let sb = trellis_past[b.0][b.1].score + b.2;
                                    sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                existing.incoming.truncate(6);
                            }
                        } else {
                            future_slot.push(ForwardTrellisState {
                                node: node.clone(),
                                score: best_incoming_score,
                                prev_boundary: i,
                                prev_state_idx: best_prev_idx,
                                incoming: top_edges,
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

        (trellis, classical.active)
    }

    fn extract_nbest_from_trellis(
        trellis: &[Vec<ForwardTrellisState>],
        n_syllables: usize,
        top_k: usize,
    ) -> Vec<(Vec<LatticeNode>, f64)> {
        use std::cmp::Ordering;
        use std::collections::{BinaryHeap, HashSet};

        #[derive(Clone)]
        struct AStarItem {
            f_score: f64,
            g_suffix: f64,
            boundary: usize,
            state_idx: usize,
            rev_states: Vec<(usize, usize)>,
        }
        impl PartialEq for AStarItem {
            fn eq(&self, other: &Self) -> bool {
                self.f_score.total_cmp(&other.f_score) == Ordering::Equal
                    && self.g_suffix.total_cmp(&other.g_suffix) == Ordering::Equal
                    && self.boundary == other.boundary
                    && self.state_idx == other.state_idx
            }
        }
        impl Eq for AStarItem {}
        impl Ord for AStarItem {
            fn cmp(&self, other: &Self) -> Ordering {
                self.f_score
                    .total_cmp(&other.f_score)
                    .then_with(|| self.g_suffix.total_cmp(&other.g_suffix))
                    .then_with(|| other.boundary.cmp(&self.boundary))
                    .then_with(|| other.state_idx.cmp(&self.state_idx))
            }
        }
        impl PartialOrd for AStarItem {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        if n_syllables == 0 || top_k == 0 || trellis.get(n_syllables).is_none_or(|v| v.is_empty()) {
            return Vec::new();
        }

        const MAX_ASTAR_POPS: usize = 2048;
        let mut heap = BinaryHeap::new();
        for (s_idx, st) in trellis[n_syllables].iter().enumerate() {
            if st.score.is_finite() {
                heap.push(AStarItem {
                    f_score: st.score,
                    g_suffix: 0.0,
                    boundary: n_syllables,
                    state_idx: s_idx,
                    rev_states: Vec::new(),
                });
            }
        }

        let mut results: Vec<(Vec<LatticeNode>, f64)> = Vec::with_capacity(top_k);
        let mut seen_surface: HashSet<String> = HashSet::with_capacity(top_k * 2);
        let mut pops = 0usize;

        while let Some(item) = heap.pop() {
            if pops >= MAX_ASTAR_POPS || results.len() >= top_k {
                break;
            }
            pops += 1;

            if item.boundary == 0 {
                let mut path = Vec::with_capacity(item.rev_states.len());
                let mut surf = String::new();
                for &(b, s) in item.rev_states.iter().rev() {
                    let node = &trellis[b][s].node;
                    surf.push_str(&node.word);
                    path.push(node.clone());
                }
                if seen_surface.insert(surf) {
                    results.push((path, item.g_suffix));
                }
                continue;
            }

            let st = &trellis[item.boundary][item.state_idx];
            let mut next_rev = item.rev_states;
            next_rev.push((item.boundary, item.state_idx));

            if st.incoming.is_empty() {
                if st.prev_boundary < item.boundary {
                    let pb = st.prev_boundary;
                    let ps = st.prev_state_idx;
                    let edge_score = st.score - trellis[pb][ps].score;
                    let child_g = item.g_suffix + edge_score;
                    let child_f = child_g + trellis[pb][ps].score;
                    if child_f.is_finite() {
                        heap.push(AStarItem {
                            f_score: child_f,
                            g_suffix: child_g,
                            boundary: pb,
                            state_idx: ps,
                            rev_states: next_rev,
                        });
                    }
                }
            } else {
                for &(pb, ps, edge_score) in &st.incoming {
                    if pb >= item.boundary {
                        continue;
                    }
                    let child_g = item.g_suffix + edge_score;
                    let child_f = child_g + trellis[pb][ps].score;
                    if child_f.is_finite() {
                        heap.push(AStarItem {
                            f_score: child_f,
                            g_suffix: child_g,
                            boundary: pb,
                            state_idx: ps,
                            rev_states: next_rev.clone(),
                        });
                    }
                }
            }
        }

        results
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
        let classical = super::classical_register::ClassicalRegisterProfile::detect(
            self.classical_mode,
            n_syl,
            dag,
        );

        let mut prefix_words: Vec<&str> = Vec::with_capacity(path.len());
        let ono_enabled = !transitions.is_rule_disabled("onomastics");
        let allow_ngram = n_syl >= 3 && !classical.active;
        let ngram_lm_ref = if allow_ngram && !transitions.is_rule_disabled("ngram_lm") {
            transitions.ngram_lm.clone()
        } else {
            None
        };
        for (idx, node) in path.iter().enumerate() {
            let c_node = classical.node_score_delta(node, n_syl);
            let prev_w = if idx > 0 { path[idx - 1].word.as_str() } else { "" };
            let prev2_w = if idx >= 2 { Some(path[idx - 2].word.as_str()) } else { None };
            let c_trans = classical.transition_score_delta(prev_w, prev2_w, node, n_syl, dag);
            let c_total = c_node + c_trans;
            if c_total.abs() > 1e-12 {
                *features.entry("classical_register".to_string()).or_insert(0.0) += c_total;
            }
            let word_len_u = node.length();
            if let Some(ref lm) = ngram_lm_ref {
                let lb_start = prefix_words.len().saturating_sub(24);
                let lb_slice = &prefix_words[lb_start..];
                let ono_active = is_onomastic_edge_active(
                    ono_enabled,
                    lb_slice,
                    None,
                    &backward,
                    node,
                    prev_w,
                    prev2_w,
                    dag,
                    &transitions.session_given_names,
                );
                let (wb, cb, ct) = if !ono_active {
                    let wb = lm
                        .score_word_bigram(prev_w, &node.word)
                        .max(NGRAM_WORD_BI_FLOOR);
                    let (raw_cb, raw_ct) = lm.score_char_boundary(prev2_w, prev_w, &node.word);
                    let (cb, ct) = clip_ngram_char_pair(raw_cb, raw_ct, word_len_u);
                    (wb, cb, ct)
                } else {
                    let c_neutral = NGRAM_ONO_CHAR_NEUTRAL_PER_CHAR * (word_len_u as f64);
                    (NGRAM_ONO_WB_NEUTRAL, c_neutral, c_neutral)
                };
                *features.entry("ngram_word_bi".to_string()).or_insert(0.0) += wb;
                *features.entry("ngram_char_bi".to_string()).or_insert(0.0) += cb;
                *features.entry("ngram_char_tri".to_string()).or_insert(0.0) += ct;
            }
            let word_len = word_len_u as f64;
            let is_session_name = transitions.session_given_names.contains(&node.word);
            let base_weight = (std::cmp::max(1, node.weight)) as f64;
            if word_len > 1.0 {
                *features.entry("freq_ln_compound".to_string()).or_insert(0.0) += base_weight.ln();
            } else {
                *features.entry("freq_ln_char1".to_string()).or_insert(0.0) += base_weight.ln();
            }
            if word_len > 1.0 {
                *features.entry("multi_char_len".to_string()).or_insert(0.0) +=
                    (word_len - 1.0)
                        * self.effective_multi_char_bonus(
                            &node.word,
                            word_len_u,
                            node.weight,
                            is_session_name,
                        );
                if word_len >= 4.0 {
                    *features.entry("cohesion_len4".to_string()).or_insert(0.0) += 16.0;
                } else if word_len >= 3.0 {
                    *features.entry("cohesion_len3".to_string()).or_insert(0.0) += 8.0;
                }
            }

            if idx > 0 {
                let prev_node = &path[idx - 1];
                let nodes_at_start = dag
                    .get(&node.start_idx)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let node_guards = compute_node_edge_guards(node, nodes_at_start, dag, &backward);
                let edge_ctx = build_transition_edge_ctx(
                    &node_guards,
                    node,
                    prev_node,
                    prev2_w,
                    dag,
                    &backward,
                );
                let curr_archaic =
                    super::archaic_table::is_archaic_polyphonic(&node.word, &node.bopomofo);
                let prev_archaic = super::archaic_table::is_archaic_polyphonic(
                    &prev_node.word,
                    &prev_node.bopomofo,
                );
                let _ = transitions.calculate_transition_bonus_with_ctx(
                    curr_archaic || prev_archaic,
                    &prev_node.word,
                    &prev_node.bopomofo,
                    &node.word,
                    &node.bopomofo,
                    edge_ctx,
                );
                let _ = transitions.eval_functional_collocation_guards(
                    prev2_w,
                    &prev_node.word,
                    &node.word,
                    &node.bopomofo,
                    node.start_idx,
                    node.end_idx,
                    dag,
                    &backward,
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
                let _ = transitions.eval_functional_collocation_guards(
                    None,
                    "",
                    &node.word,
                    &node.bopomofo,
                    node.start_idx,
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

    #[test]
    fn test_classical_register_dot_product_equivalence_261120() {
        let mut router = ViterbiRouter::default();
        router.classical_mode = super::super::classical_register::ClassicalRegisterMode::ClassicalExplicit;
        let mut eng = crate::router::transitions::TransitionEngine::new();
        eng.feature_weights.insert("freq_ln_char1".to_string(), 1.04);
        eng.feature_weights.insert("freq_ln_compound".to_string(), 1.09);
        eng.feature_weights.insert("multi_char_len".to_string(), 1.01);
        eng.feature_weights.insert("classical_register".to_string(), 1.07);

        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        dag.insert(0, vec![LatticeNode::new(0, 1, "知", 500, "ㄓ"), LatticeNode::new(0, 1, "之", 500, "ㄓ")]);
        dag.insert(1, vec![LatticeNode::new(1, 2, "之", 500, "ㄓ"), LatticeNode::new(1, 2, "支", 400, "ㄓ")]);
        dag.insert(2, vec![LatticeNode::new(2, 3, "為", 500, "ㄨㄟˊ")]);
        dag.insert(3, vec![LatticeNode::new(3, 4, "知", 500, "ㄓ")]);
        dag.insert(4, vec![LatticeNode::new(4, 5, "之", 500, "ㄓ")]);
        dag.insert(5, vec![LatticeNode::new(5, 6, "也", 600, "ㄧㄝˇ"), LatticeNode::new(5, 6, "野", 600, "ㄧㄝˇ")]);

        let (path, best_score) = router.solve_best_path(6, &dag, Some(&mut eng));
        let joined: String = path.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(joined, "知之為知之也");

        let (feats, _) = router.extract_path_features_and_triggers(&path, &dag, &mut eng);
        assert!(feats.get("classical_register").copied().unwrap_or(0.0) > 0.0);
        let dot: f64 = feats
            .iter()
            .map(|(k, v)| v * eng.feature_weights.get(k).copied().unwrap_or(1.0))
            .sum();
        assert!(
            (best_score - dot).abs() < 1e-9,
            "classical explicit best_score ({}) != dot product ({}), diff = {}",
            best_score,
            dot,
            (best_score - dot).abs()
        );
    }

    #[test]
    fn test_ngram_lm_dot_product_equivalence_261121() {
        let mut tsv_path = std::env::temp_dir();
        tsv_path.push(format!("ruhi_ngram_viterbi_dp_{}.tsv", std::process::id()));
        let mut bin_path = std::env::temp_dir();
        bin_path.push(format!("ruhi_ngram_viterbi_dp_{}.bin", std::process::id()));

        let tsv_data = concat!(
            "WU\t<s>\t-9.0\t-0.1\n",
            "WU\t<unk>\t-8.5\t0.0\n",
            "WU\t台灣\t-2.1\t-0.2\n",
            "WU\t台\t-3.2\t-0.3\n",
            "WU\t灣\t-3.4\t-0.3\n",
            "WU\t語音\t-2.4\t-0.2\n",
            "WB\t<s>\t台灣\t-1.4\n",
            "WB\t台灣\t語音\t-1.6\n",
            "CU\t<s>\t-9.0\t-0.1\n",
            "CU\t<unk>\t-8.5\t0.0\n",
            "CU\t台\t-2.0\t-0.2\n",
            "CU\t灣\t-2.3\t-0.2\n",
            "CU\t語\t-2.1\t-0.2\n",
            "CU\t音\t-2.2\t-0.2\n",
            "CB\t<s>\t台\t-1.3\t-0.15\n",
            "CB\t台\t灣\t-0.4\t-0.1\n",
            "CB\t灣\t語\t-1.9\t-0.2\n",
            "CB\t語\t音\t-0.6\t-0.1\n",
            "CT\t<s>\t台\t灣\t-0.35\n",
            "CT\t台\t灣\t語\t-1.2\n",
            "CT\t灣\t語\t音\t-0.45\n",
        );
        std::fs::write(&tsv_path, tsv_data).unwrap();
        crate::router::ngram_lm::compile_arpa_tsv_to_bin(&tsv_path, &bin_path).unwrap();

        let router = ViterbiRouter::default();
        let mut eng = crate::router::transitions::TransitionEngine::new();
        eng.disabled_rules.remove("ngram_lm");
        eng.load_ngram_lm_bin(&bin_path).unwrap();
        eng.feature_weights.insert("freq_ln_char1".to_string(), 1.04);
        eng.feature_weights.insert("freq_ln_compound".to_string(), 1.09);
        eng.feature_weights.insert("multi_char_len".to_string(), 0.97);
        eng.feature_weights.insert("ngram_word_bi".to_string(), 1.2);
        eng.feature_weights.insert("ngram_char_bi".to_string(), 0.8);
        eng.feature_weights.insert("ngram_char_tri".to_string(), 1.5);

        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        dag.insert(0, vec![
            LatticeNode::new(0, 2, "台灣", 800, "ㄊㄞˊ ㄨㄢ"),
            LatticeNode::new(0, 1, "台", 400, "ㄊㄞˊ"),
        ]);
        dag.insert(1, vec![
            LatticeNode::new(1, 2, "灣", 350, "ㄨㄢ"),
        ]);
        dag.insert(2, vec![
            LatticeNode::new(2, 4, "語音", 700, "ㄩˇ ㄧㄣ"),
        ]);

        let (path, best_score) = router.solve_best_path(4, &dag, Some(&mut eng));
        let joined: String = path.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(joined, "台灣語音");

        let (feats, _) = router.extract_path_features_and_triggers(&path, &dag, &mut eng);
        assert!(feats.get("ngram_word_bi").copied().unwrap_or(0.0) < 0.0);
        assert!(feats.get("ngram_char_bi").copied().unwrap_or(0.0) < 0.0);
        assert!(feats.get("ngram_char_tri").copied().unwrap_or(0.0) < 0.0);

        let dot: f64 = feats
            .iter()
            .map(|(k, v)| {
                let def = if k.starts_with("ngram_") || k == "coord_he_huo_prior" {
                    0.0
                } else {
                    1.0
                };
                v * eng.feature_weights.get(k).copied().unwrap_or(def)
            })
            .sum();
        assert!(
            (best_score - dot).abs() < 1e-9,
            "ngram best_score ({}) != dot product ({}), diff = {}",
            best_score,
            dot,
            (best_score - dot).abs()
        );

        let mut eng_disabled = eng.clone();
        eng_disabled.disabled_rules.insert("ngram_lm".to_string());
        let (_p_dis, score_dis) = router.solve_best_path(4, &dag, Some(&mut eng_disabled));

        let mut eng_none = eng.clone();
        eng_none.ngram_lm = None;
        let (_p_none, score_none) = router.solve_best_path(4, &dag, Some(&mut eng_none));

        assert!(
            (score_dis - score_none).abs() < 1e-12,
            "disabling ngram_lm ({}) should equal ngram_lm = None ({})",
            score_dis,
            score_none
        );

        // Guard 1: 2-syllable context-free DAG must NOT invoke N-gram
        let mut dag2: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        dag2.insert(0, vec![
            LatticeNode::new(0, 2, "台灣", 800, "ㄊㄞˊ ㄨㄢ"),
            LatticeNode::new(0, 1, "台", 400, "ㄊㄞˊ"),
        ]);
        dag2.insert(1, vec![
            LatticeNode::new(1, 2, "灣", 350, "ㄨㄢ"),
        ]);
        let (path2, _) = router.solve_best_path(2, &dag2, Some(&mut eng));
        let (feats2, _) = router.extract_path_features_and_triggers(&path2, &dag2, &mut eng);
        assert_eq!(feats2.get("ngram_word_bi").copied().unwrap_or(0.0), 0.0);

        // Guard 2: ClassicalExplicit mode must NOT invoke modern N-gram
        let mut router_cls = router.clone();
        router_cls.classical_mode = super::super::classical_register::ClassicalRegisterMode::ClassicalExplicit;
        let (path_cls, _) = router_cls.solve_best_path(4, &dag, Some(&mut eng));
        let (feats_cls, _) = router_cls.extract_path_features_and_triggers(&path_cls, &dag, &mut eng);
        assert_eq!(feats_cls.get("ngram_word_bi").copied().unwrap_or(0.0), 0.0);

        let _ = std::fs::remove_file(&tsv_path);
        let _ = std::fs::remove_file(&bin_path);
    }

    #[test]
    fn test_backward_astar_nbest_top1_equivalence_and_pronoun_candidates_261122() {
        let router = ViterbiRouter::default();
        let mut eng = crate::router::transitions::TransitionEngine::new();
        eng.disabled_rules.insert("discourse_reranker".to_string());

        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        dag.insert(0, vec![LatticeNode::new(0, 1, "是", 600, "ㄕˋ"), LatticeNode::new(0, 1, "事", 300, "ㄕˋ")]);
        dag.insert(1, vec![
            LatticeNode::new(1, 2, "他", 500, "ㄊㄚ"),
            LatticeNode::new(1, 2, "她", 490, "ㄊㄚ"),
            LatticeNode::new(1, 2, "它", 480, "ㄊㄚ"),
            LatticeNode::new(1, 2, "牠", 420, "ㄊㄚ"),
        ]);
        dag.insert(2, vec![LatticeNode::new(2, 3, "們", 700, "˙ㄇㄣ")]);
        dag.insert(3, vec![
            LatticeNode::new(3, 5, "意思", 900, "ㄧˋ ˙ㄙ"),
            LatticeNode::new(3, 4, "意", 120, "ㄧˋ"),
        ]);
        dag.insert(4, vec![LatticeNode::new(4, 5, "思", 120, "˙ㄙ")]);

        let (best_path, best_score) = router.solve_best_path(5, &dag, Some(&mut eng));
        let nbest = router.solve_nbest_paths(5, &dag, Some(&mut eng), 16);
        assert!(!nbest.is_empty());
        assert!(
            (nbest[0].1 - best_score).abs() < 1e-9,
            "nbest[0] score ({}) must equal Viterbi Top-1 ({})",
            nbest[0].1,
            best_score
        );
        let best_str: String = best_path.iter().map(|n| n.word.as_str()).collect();
        let nbest0_str: String = nbest[0].0.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(nbest0_str, best_str);

        let mut seen = std::collections::HashSet::new();
        for i in 0..nbest.len() {
            let s: String = nbest[i].0.iter().map(|n| n.word.as_str()).collect();
            assert!(seen.insert(s), "nbest must contain unique surface paths");
            if i > 0 {
                assert!(
                    nbest[i - 1].1 >= nbest[i].1 - 1e-9,
                    "nbest scores must be monotonically non-increasing"
                );
            }
        }
        assert!(seen.contains("是他們意思"));
        assert!(seen.contains("是她們意思"));
        assert!(seen.contains("是它們意思"));
        assert!(seen.contains("是牠們意思"));
    }
}
