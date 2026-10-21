/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/korean/romaja.rs - 韓語標準羅馬拼音（RR）音節切分動態規劃（N-best）

use crate::korean::jamo::{
    compose, is_compound_coda, CHOSEONG, JONGSEONG, JUNGSEONG,
};

pub const COST_SYLLABLE: u32 = 10;              // 每個音節的基礎成本 → 偏好較少音節（最長匹配）
pub const COST_NULL_ONSET_AFTER_CODA: u32 = 8;  // 前一音節有終聲、本音節零初聲 → 違反連音規律
pub const COST_COMPOUND_CODA: u32 = 5;          // 複合終聲較少見
pub const COST_PENDING_PREVIEW: u32 = 3;        // 預覽模式下，每個未完成字母的成本
pub const NBEST_K: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    Preview,
    Finalize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Syllable {
    pub l: usize,
    pub v: usize,
    pub t: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    pub syllables: Vec<Syllable>,
    pub pending: String,
    pub cost: u32,
}

#[derive(Debug, Clone)]
struct DpPath {
    syllables: Vec<Syllable>,
    span_lens: Vec<usize>,
    pending: String,
    cost: u32,
}

impl DpPath {
    fn cmp_deterministic(&self, other: &Self, mode: ParseMode) -> std::cmp::Ordering {
        if mode == ParseMode::Finalize {
            match (self.pending.is_empty(), other.pending.is_empty()) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
        }
        match self.cost.cmp(&other.cost) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        // 成本相同時的決勝規則：比較兩條路徑第一個不同的音節，取較長者（偏好左側貪婪）
        for (idx, (sa, sb)) in self.syllables.iter().zip(other.syllables.iter()).enumerate() {
            let len_a = self.span_lens.get(idx).copied().unwrap_or(0);
            let len_b = other.span_lens.get(idx).copied().unwrap_or(0);
            if len_a != len_b {
                return len_b.cmp(&len_a);
            }
            if sa != sb {
                return (sa.l, sa.v, sa.t).cmp(&(sb.l, sb.v, sb.t));
            }
        }
        match other.syllables.len().cmp(&self.syllables.len()) {
            std::cmp::Ordering::Equal => self.pending.cmp(&other.pending),
            ord => ord,
        }
    }
}

/// 在 seg[pos..] 嘗試匹配初聲 L，回傳 (l_idx, matched_byte_len, alt_cost)
fn match_onsets(seg: &str, pos: usize) -> Vec<(usize, usize, u32)> {
    let rem = &seg[pos..];
    let mut res = Vec::with_capacity(4);
    // 零初聲 L = 11
    res.push((11, 0, 0));
    for (l_idx, spec) in CHOSEONG.iter().enumerate() {
        if l_idx == 11 {
            continue;
        }
        if !spec.rr.is_empty() && rem.starts_with(spec.rr) {
            res.push((l_idx, spec.rr.len(), 0));
        }
        for &(alt, cost) in spec.alts {
            if !alt.is_empty() && rem.starts_with(alt) {
                res.push((l_idx, alt.len(), cost as u32));
            }
        }
    }
    res
}

/// 在 seg[pos..] 嘗試匹配中聲 V，回傳 (v_idx, matched_byte_len, alt_cost)
fn match_vowels(seg: &str, pos: usize) -> Vec<(usize, usize, u32)> {
    let rem = &seg[pos..];
    let mut res = Vec::with_capacity(4);
    for (v_idx, spec) in JUNGSEONG.iter().enumerate() {
        if rem.starts_with(spec.rr) {
            res.push((v_idx, spec.rr.len(), 0));
        }
        for &(alt, cost) in spec.alts {
            if rem.starts_with(alt) {
                res.push((v_idx, alt.len(), cost as u32));
            }
        }
    }
    res
}

/// 在 seg[pos..] 嘗試匹配終聲 T，回傳 (t_idx, matched_byte_len, coda_cost)
fn match_codas(seg: &str, pos: usize) -> Vec<(usize, usize, u32)> {
    let rem = &seg[pos..];
    let mut res = Vec::with_capacity(6);
    // 無終聲 T = 0
    res.push((0, 0, 0));
    for (t_idx, opt_spec) in JONGSEONG.iter().enumerate() {
        if let Some(spec) = opt_spec {
            let comp_cost = if is_compound_coda(t_idx) {
                COST_COMPOUND_CODA
            } else {
                0
            };
            if rem.starts_with(spec.rr) {
                res.push((t_idx, spec.rr.len(), comp_cost));
            }
            for &(alt, cost) in spec.alts {
                if rem.starts_with(alt) {
                    res.push((t_idx, alt.len(), comp_cost + cost as u32));
                }
            }
        }
    }
    res
}

/// 嘗試將片段尾端未完成子音／半母音轉為預覽 pending（回傳 (pending_str, pending_cost)）
fn match_tail_pending(tail: &str) -> Vec<(String, u32)> {
    let mut res = Vec::new();
    if tail.is_empty() {
        res.push((String::new(), 0));
        return res;
    }

    // 1. 單一初聲子音（如 g -> ㄱ, k -> ㅋ, j -> ㅈ, kk -> ㄲ, ch -> ㅊ）
    for (l_idx, spec) in CHOSEONG.iter().enumerate() {
        if l_idx == 11 {
            continue;
        }
        if tail == spec.rr {
            res.push((spec.compat.to_string(), COST_PENDING_PREVIEW));
        }
        for &(alt, cost) in spec.alts {
            if tail == alt {
                res.push((
                    spec.compat.to_string(),
                    COST_PENDING_PREVIEW + cost as u32,
                ));
            }
        }
    }

    // 2. 半母音過渡（"y" 或 "w"）或「初聲 + y/w」（如 anny -> 안ㄴy、hwa 打到 hw -> ㅎw）
    if tail == "y" || tail == "w" {
        res.push((tail.to_string(), COST_PENDING_PREVIEW * 2));
    } else if tail.ends_with('y') || tail.ends_with('w') {
        let onset_part = &tail[..tail.len() - 1];
        let glide_part = &tail[tail.len() - 1..];
        for (l_idx, spec) in CHOSEONG.iter().enumerate() {
            if l_idx == 11 {
                continue;
            }
            if onset_part == spec.rr {
                res.push((
                    format!("{}{}", spec.compat, glide_part),
                    COST_PENDING_PREVIEW * 2,
                ));
            }
            for &(alt, cost) in spec.alts {
                if onset_part == alt {
                    res.push((
                        format!("{}{}", spec.compat, glide_part),
                        COST_PENDING_PREVIEW * 2 + cost as u32,
                    ));
                }
            }
        }
    }

    res
}

fn prune_and_dedup(paths: &mut Vec<DpPath>, mode: ParseMode, k: usize) {
    paths.sort_by(|a, b| a.cmp_deterministic(b, mode));
    let mut unique: Vec<DpPath> = Vec::with_capacity(paths.len().min(k));
    for p in paths.drain(..) {
        if !unique
            .iter()
            .any(|u| u.syllables == p.syllables && u.pending == p.pending)
        {
            unique.push(p);
            if unique.len() >= k {
                break;
            }
        }
    }
    *paths = unique;
}

/// 對單一無 `-` 片段執行音節切分動態規劃
fn parse_segment_nbest(seg: &str, mode: ParseMode, k: usize) -> Vec<DpPath> {
    let n = seg.len();
    if n == 0 {
        return vec![DpPath {
            syllables: Vec::new(),
            span_lens: Vec::new(),
            pending: String::new(),
            cost: 0,
        }];
    }

    // dp[pos][has_coda (0 or 1)]
    let mut dp: Vec<[Vec<DpPath>; 2]> = (0..=n).map(|_| [Vec::new(), Vec::new()]).collect();
    dp[0][0].push(DpPath {
        syllables: Vec::new(),
        span_lens: Vec::new(),
        pending: String::new(),
        cost: 0,
    });

    for i in 0..n {
        if dp[i][0].is_empty() && dp[i][1].is_empty() {
            continue;
        }
        if !seg.is_char_boundary(i) {
            continue;
        }

        let onsets = match_onsets(seg, i);
        for (l_idx, l_len, l_cost) in onsets {
            let i_v = i + l_len;
            if i_v >= n || !seg.is_char_boundary(i_v) {
                continue;
            }
            let vowels = match_vowels(seg, i_v);
            for (v_idx, v_len, v_cost) in vowels {
                let i_t = i_v + v_len;
                if i_t > n || !seg.is_char_boundary(i_t) {
                    continue;
                }
                let codas = match_codas(seg, i_t);
                for (t_idx, t_len, t_cost) in codas {
                    let j = i_t + t_len;
                    if j > n {
                        continue;
                    }
                    let span_len = j - i;
                    let next_has_coda = usize::from(t_idx > 0);
                    let base_step_cost = COST_SYLLABLE + l_cost + v_cost + t_cost;

                    for prev_coda in 0..=1 {
                        if dp[i][prev_coda].is_empty() {
                            continue;
                        }
                        let liaison_cost = if prev_coda == 1 && l_idx == 11 {
                            COST_NULL_ONSET_AFTER_CODA
                        } else {
                            0
                        };
                        let step_cost = base_step_cost + liaison_cost;
                        let prev_paths = dp[i][prev_coda].clone();
                        for p in prev_paths {
                            let mut next_syls = p.syllables;
                            next_syls.push(Syllable {
                                l: l_idx,
                                v: v_idx,
                                t: t_idx,
                            });
                            let mut next_spans = p.span_lens;
                            next_spans.push(span_len);
                            dp[j][next_has_coda].push(DpPath {
                                syllables: next_syls,
                                span_lens: next_spans,
                                pending: String::new(),
                                cost: p.cost + step_cost,
                            });
                        }
                    }
                }
            }
        }

        for next_pos in (i + 1)..=n {
            for coda_flag in 0..=1 {
                if dp[next_pos][coda_flag].len() > k * 2 {
                    prune_and_dedup(&mut dp[next_pos][coda_flag], ParseMode::Preview, k);
                }
            }
        }
    }

    for pos in 0..=n {
        for coda_flag in 0..=1 {
            if !dp[pos][coda_flag].is_empty() {
                prune_and_dedup(&mut dp[pos][coda_flag], ParseMode::Preview, k);
            }
        }
    }

    let mut results: Vec<DpPath> = Vec::new();

    // 收集所有位置 i (0..=n) 搭配合法尾端 pending 的候選
    for i in 0..=n {
        if !seg.is_char_boundary(i) {
            continue;
        }
        let tail = &seg[i..];
        let tail_matches = match_tail_pending(tail);
        if tail_matches.is_empty() {
            continue;
        }
        for coda_flag in 0..=1 {
            for p in &dp[i][coda_flag] {
                for (pend_str, pend_cost) in &tail_matches {
                    results.push(DpPath {
                        syllables: p.syllables.clone(),
                        span_lens: p.span_lens.clone(),
                        pending: pend_str.clone(),
                        cost: p.cost + *pend_cost,
                    });
                }
            }
        }
    }

    // 若遇到無法辨識之字元（如 "xyz"、"123"）導致無任何完整路徑，取最遠可達位置並原樣保留剩餘字元
    if results.is_empty() {
        for i in (0..=n).rev() {
            if !seg.is_char_boundary(i) {
                continue;
            }
            if !dp[i][0].is_empty() || !dp[i][1].is_empty() {
                let raw_tail = &seg[i..];
                for coda_flag in 0..=1 {
                    for p in &dp[i][coda_flag] {
                        results.push(DpPath {
                            syllables: p.syllables.clone(),
                            span_lens: p.span_lens.clone(),
                            pending: raw_tail.to_string(),
                            cost: p.cost + (raw_tail.len() as u32) * COST_SYLLABLE,
                        });
                    }
                }
                break;
            }
        }
    }

    prune_and_dedup(&mut results, mode, k);
    results
}

/// 傳回依成本遞增排序的前 `k` 種切分
pub fn parse_nbest(raw: &str, mode: ParseMode, k: usize) -> Vec<Parse> {
    if raw.is_empty() || k == 0 {
        return Vec::new();
    }
    let lower = raw.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('-').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Vec::new();
    }

    let mut combined: Vec<DpPath> = vec![DpPath {
        syllables: Vec::new(),
        span_lens: Vec::new(),
        pending: String::new(),
        cost: 0,
    }];

    let num_segs = segments.len();
    for (idx, seg) in segments.into_iter().enumerate() {
        let seg_mode = if idx + 1 < num_segs {
            ParseMode::Finalize
        } else {
            mode
        };
        let seg_paths = parse_segment_nbest(seg, seg_mode, k);
        let mut next_combined = Vec::with_capacity(combined.len() * seg_paths.len());
        for prefix in &combined {
            for sp in &seg_paths {
                let mut syls = prefix.syllables.clone();
                let mut spans = prefix.span_lens.clone();
                let mut pend = String::new();
                if prefix.pending.is_empty() {
                    syls.extend(sp.syllables.iter().cloned());
                    spans.extend(sp.span_lens.iter().copied());
                    pend = sp.pending.clone();
                } else {
                    pend.push_str(&prefix.pending);
                    let sp_rendered = render(&Parse {
                        syllables: sp.syllables.clone(),
                        pending: sp.pending.clone(),
                        cost: sp.cost,
                    });
                    pend.push_str(&sp_rendered);
                }
                next_combined.push(DpPath {
                    syllables: syls,
                    span_lens: spans,
                    pending: pend,
                    cost: prefix.cost + sp.cost,
                });
            }
        }
        prune_and_dedup(&mut next_combined, seg_mode, k);
        combined = next_combined;
    }

    prune_and_dedup(&mut combined, mode, k);
    combined
        .into_iter()
        .map(|p| Parse {
            syllables: p.syllables,
            pending: p.pending,
            cost: p.cost,
        })
        .collect()
}

/// 將 Parse 渲染為諺文字串（音節 + pending）
pub fn render(p: &Parse) -> String {
    let mut out = String::with_capacity(p.syllables.len() * 3 + p.pending.len());
    for s in &p.syllables {
        if let Some(ch) = compose(s.l, s.v, s.t) {
            out.push(ch);
        }
    }
    out.push_str(&p.pending);
    out
}

/// 最佳切分之諺文字串（含 pending）
pub fn romaja_to_hangul(raw: &str, mode: ParseMode) -> String {
    if raw.is_empty() {
        return String::new();
    }
    parse_nbest(raw, mode, NBEST_K)
        .first()
        .map(render)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::korean::jamo::to_romaja;

    #[test]
    fn test_p2_twenty_core_cases() {
        // 1. 基本組字
        assert_eq!(
            romaja_to_hangul("annyeonghaseyo", ParseMode::Finalize),
            "안녕하세요"
        );
        // 2. 切分歧義與 N-best
        assert_eq!(romaja_to_hangul("hangeul", ParseMode::Finalize), "한글");
        let hangeul_nbest: Vec<String> = parse_nbest("hangeul", ParseMode::Finalize, NBEST_K)
            .iter()
            .map(render)
            .collect();
        assert!(
            hangeul_nbest.contains(&"항을".to_string()),
            "hangeul N-best should contain 항을, got {:?}",
            hangeul_nbest
        );

        // 3. '-' 強制切分
        assert_eq!(romaja_to_hangul("hang-eul", ParseMode::Finalize), "항을");
        // 4. 連音偏好：不可為 한아
        assert_eq!(romaja_to_hangul("hana", ParseMode::Finalize), "하나");
        // 5. 사랑해
        assert_eq!(romaja_to_hangul("saranghae", ParseMode::Finalize), "사랑해");
        // 6. 서울
        assert_eq!(romaja_to_hangul("seoul", ParseMode::Finalize), "서울");
        // 7. 학교
        assert_eq!(romaja_to_hangul("hakgyo", ParseMode::Finalize), "학교");
        // 8. 雙子音初聲 까치
        assert_eq!(romaja_to_hangul("kkachi", ParseMode::Finalize), "까치");
        // 9. 회의
        assert_eq!(romaja_to_hangul("hoeui", ParseMode::Finalize), "회의");
        // 10. a-e -> 아에
        assert_eq!(romaja_to_hangul("a-e", ParseMode::Finalize), "아에");
        // 11. ae -> 애
        assert_eq!(romaja_to_hangul("ae", ParseMode::Finalize), "애");
        // 12. bukhan -> 북한（不可為 붘안）
        assert_eq!(romaja_to_hangul("bukhan", ParseMode::Finalize), "북한");
        // 13. dalk (Finalize) -> 닭
        assert_eq!(romaja_to_hangul("dalk", ParseMode::Finalize), "닭");
        // 14. dalk (Preview) -> 달ㅋ
        assert_eq!(romaja_to_hangul("dalk", ParseMode::Preview), "달ㅋ");
        // 15. hanj (Preview) -> 한ㅈ（不可為 핝）
        assert_eq!(romaja_to_hangul("hanj", ParseMode::Preview), "한ㅈ");
        // 16. g (Preview) -> ㄱ
        assert_eq!(romaja_to_hangul("g", ParseMode::Preview), "ㄱ");
        // 17. gaeul (Finalize) -> 개울（成本相同，左側貪婪決勝），且 N-best 含 가을
        assert_eq!(romaja_to_hangul("gaeul", ParseMode::Finalize), "개울");
        let gaeul_nbest: Vec<String> = parse_nbest("gaeul", ParseMode::Finalize, NBEST_K)
            .iter()
            .map(render)
            .collect();
        assert!(
            gaeul_nbest.contains(&"가을".to_string()),
            "gaeul N-best should contain 가을, got {:?}",
            gaeul_nbest
        );

        // 18. kimchi (Finalize) -> 킴치（規則層），N-best 含 김치
        assert_eq!(romaja_to_hangul("kimchi", ParseMode::Finalize), "킴치");
        let kimchi_nbest: Vec<String> = parse_nbest("kimchi", ParseMode::Finalize, NBEST_K)
            .iter()
            .map(render)
            .collect();
        assert!(
            kimchi_nbest.contains(&"김치".to_string()),
            "kimchi N-best should contain 김치, got {:?}",
            kimchi_nbest
        );

        // 19. gamsahamnida (Finalize) -> 감사함니다（規則層）
        assert_eq!(
            romaja_to_hangul("gamsahamnida", ParseMode::Finalize),
            "감사함니다"
        );

        // 20. xyz 與空字串健全性
        assert_eq!(romaja_to_hangul("xyz", ParseMode::Preview), "xyz");
        assert_eq!(romaja_to_hangul("", ParseMode::Preview), "");
        assert_eq!(romaja_to_hangul("", ParseMode::Finalize), "");
    }

    #[test]
    fn test_50_words_roundtrip() {
        const WORDS: &[&str] = &[
            "안녕하세요", "감사합니다", "죄송합니다", "사랑해요", "반갑습니다",
            "한글", "한국어", "대한민국", "서울", "부산",
            "제주도", "인천", "공항", "지하철", "기차역",
            "버스", "택시", "호텔", "화장실", "병원",
            "약국", "경찰서", "은행", "환전", "현금",
            "카드", "영수증", "식당", "메뉴", "물",
            "김치", "불고기", "비빔밥", "떡볶이", "삼겹살",
            "치킨", "맥주", "소주", "커피", "녹차",
            "아메리카노", "맛있어요", "얼마예요", "주세요", "여기요",
            "하나", "둘", "셋", "넷", "다섯",
            "여섯", "일곱", "여덟", "아홉", "열",
            "가을", "겨울", "봄", "여름", "하늘",
            "바다", "산", "닭", "삶", "값",
            "앉다", "읽다", "없다", "부엌", "밭",
            "앞", "토끼", "오빠", "아빠", "언니",
        ];
        assert!(WORDS.len() >= 50);
        for &w in WORDS {
            let rom = to_romaja(w);
            let round = romaja_to_hangul(&rom, ParseMode::Finalize);
            assert_eq!(
                round, w,
                "Round-trip failed for {} -> {} -> {}",
                w, rom, round
            );
        }
    }

    #[test]
    fn test_performance_40_chars() {
        let input = "annyeonghaseyogamsahamnidasaranghaeyohangeul";
        assert!(input.len() >= 40);
        let start = std::time::Instant::now();
        let res = parse_nbest(input, ParseMode::Finalize, NBEST_K);
        let elapsed = start.elapsed();
        assert!(!res.is_empty());
        assert!(
            elapsed.as_millis() < 50,
            "parse_nbest took too long: {:?}",
            elapsed
        );
    }
}
