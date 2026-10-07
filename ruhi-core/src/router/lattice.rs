/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/lattice.rs - 詞網格圖 (Word Lattice DAG) 建構器

use std::collections::HashMap;
use crate::fsm::HsuSyllable;
use crate::lexicon::LexiconManager;

#[derive(Debug, Clone)]
pub struct LatticeNode {
    pub start_idx: usize,
    pub end_idx: usize,
    pub word: String,
    pub weight: u32,
    pub bopomofo: String,
}

impl LatticeNode {
    pub fn new(start_idx: usize, end_idx: usize, word: &str, weight: u32, bopomofo: &str) -> Self {
        Self {
            start_idx,
            end_idx,
            word: word.to_string(),
            weight,
            bopomofo: bopomofo.to_string(),
        }
    }

    pub fn length(&self) -> usize {
        self.end_idx - self.start_idx
    }
}

pub struct LatticeDAG<'a> {
    pub lexicon: &'a LexiconManager,
}

impl<'a> LatticeDAG<'a> {
    pub fn new(lexicon: &'a LexiconManager) -> Self {
        Self { lexicon }
    }

    pub fn build_dag(&self, syllables: &[HsuSyllable], exact_tone: bool) -> HashMap<usize, Vec<LatticeNode>> {
        let n = syllables.len();
        let mut dag: HashMap<usize, Vec<LatticeNode>> = HashMap::new();
        for i in 0..n {
            dag.insert(i, Vec::new());
        }

        let max_word_len = std::cmp::min(10, n);

        for i in 0..n {
            for length in 1..=std::cmp::min(max_word_len, n - i) {
                let j = i + length;
                let sub_syls = &syllables[i..j];

                let bpm_str: String = sub_syls.iter()
                    .map(|s| s.bopomofo_display())
                    .collect::<Vec<String>>()
                    .join(" ");

                let matched = self.lexicon.lookup_bopomofo(&bpm_str, exact_tone);
                if matched.is_empty() {
                    // 若精確聲調查無任何匹配，僅在單字音節（length == 1）且非明確輕聲時自動回退到無聲調查詢
                    // 避免多字詞跨聲調誤判覆蓋使用者的精確聲調單字組合（如 鏨刀 覆蓋 讚+到）
                    // 特別注意：明確輕聲（tone == '5'，如 is -> ˙ㄞ、as -> ˙ㄘ）若無匹配字，嚴禁退回無聲調強行湊出「愛、此」等破壞歐文混打之字！
                    if length == 1 && sub_syls[0].tone != '5' {
                        let notone_matched = self.lexicon.lookup_bopomofo(&bpm_str, false);
                        for entry in notone_matched {
                            let discounted_weight = (entry.weight / 2).max(10);
                            let node = LatticeNode::new(i, j, &entry.word, discounted_weight, &entry.bopomofo);
                            dag.get_mut(&i).unwrap().push(node);
                        }
                    }
                } else {
                    for entry in matched {
                        let node = LatticeNode::new(i, j, &entry.word, entry.weight, &entry.bopomofo);
                        dag.get_mut(&i).unwrap().push(node);
                    }
                }

                // ── 形態學動態構式: AABB 疊字生成 (如 實實際際、老老實實、乾乾淨淨、神神秘秘、密密實實) ──
                if length == 4 && sub_syls[0] == sub_syls[1] && sub_syls[2] == sub_syls[3] {
                    let s1_bpm = sub_syls[0].bopomofo_display();
                    let s2_bpm = sub_syls[2].bopomofo_display();
                    let base_bpm = format!("{} {}", s1_bpm, s2_bpm);
                    let base_words = self.lexicon.lookup_bopomofo(&base_bpm, exact_tone);
                    for bw in base_words {
                        let chars: Vec<char> = bw.word.chars().collect();
                        if chars.len() == 2 {
                            let aabb_word = format!("{}{}{}{}", chars[0], chars[0], chars[1], chars[1]);
                            if !dag.get(&i).unwrap().iter().any(|node| node.word == aabb_word && node.length() == 4) {
                                let aabb_weight = bw.weight.saturating_add(150);
                                let aabb_bpm = bpm_str.clone();
                                dag.get_mut(&i).unwrap().push(LatticeNode::new(i, j, &aabb_word, aabb_weight, &aabb_bpm));
                            }
                        }
                    }
                }
            }

            // 若該位置找不到任何詞，補上注音符號作為後備節點
            if dag.get(&i).unwrap().is_empty() {
                let single_syl = &syllables[i];
                let display = single_syl.bopomofo_display();
                dag.get_mut(&i).unwrap().push(LatticeNode::new(i, i + 1, &display, 1, &display));
            }
        }

        dag
    }
}
