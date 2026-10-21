/// RuhI (如喜) Input Method Framework - Pure Rust Core
/// src/korean/jamo.rs - 韓語標準羅馬拼音（RR）與諺文字母（Jamo）代數合成對照層

/// 單一字母規格
#[derive(Debug, Clone, Copy)]
pub struct JamoSpec {
    /// 相容字母（Hangul Compatibility Jamo, U+3131..U+318E），供未完成音節預覽
    pub compat: char,
    /// RR 標準／唯一拼寫（成本 0）
    pub rr: &'static str,
    /// 替代拼寫與附加成本
    pub alts: &'static [(&'static str, u8)],
}

const fn j(compat: char, rr: &'static str, alts: &'static [(&'static str, u8)]) -> JamoSpec {
    JamoSpec { compat, rr, alts }
}

/// 初聲表 CHOSEONG（索引 0..=18，順序與 Unicode 標準一致）
pub const CHOSEONG: [JamoSpec; 19] = [
    j('ㄱ', "g", &[("k", 3)]),          // 0: 舊式拼法 kimchi
    j('ㄲ', "kk", &[("gg", 1)]),        // 1
    j('ㄴ', "n", &[]),                  // 2
    j('ㄷ', "d", &[("t", 3)]),          // 3
    j('ㄸ', "tt", &[("dd", 1)]),        // 4
    j('ㄹ', "r", &[("l", 0)]),          // 5: l 不與其他初聲衝突，視同標準拼寫
    j('ㅁ', "m", &[]),                  // 6
    j('ㅂ', "b", &[("p", 3), ("v", 2)]),// 7
    j('ㅃ', "pp", &[("bb", 1)]),        // 8
    j('ㅅ', "s", &[("sh", 2)]),         // 9
    j('ㅆ', "ss", &[]),                 // 10
    j('ㅇ', "", &[]),                   // 11: 零初聲（無聲）
    j('ㅈ', "j", &[("z", 2)]),          // 12
    j('ㅉ', "jj", &[]),                 // 13
    j('ㅊ', "ch", &[]),                 // 14
    j('ㅋ', "k", &[]),                  // 15
    j('ㅌ', "t", &[]),                  // 16
    j('ㅍ', "p", &[("f", 2)]),          // 17
    j('ㅎ', "h", &[]),                  // 18
];

/// 中聲表 JUNGSEONG（索引 0..=20，順序與 Unicode 標準一致）
pub const JUNGSEONG: [JamoSpec; 21] = [
    j('ㅏ', "a", &[]),                  // 0
    j('ㅐ', "ae", &[]),                 // 1
    j('ㅑ', "ya", &[]),                 // 2
    j('ㅒ', "yae", &[]),                // 3
    j('ㅓ', "eo", &[]),                 // 4
    j('ㅔ', "e", &[]),                  // 5
    j('ㅕ', "yeo", &[]),                // 6
    j('ㅖ', "ye", &[]),                 // 7
    j('ㅗ', "o", &[]),                  // 8
    j('ㅘ', "wa", &[]),                 // 9
    j('ㅙ', "wae", &[]),                // 10
    j('ㅚ', "oe", &[]),                 // 11
    j('ㅛ', "yo", &[]),                 // 12
    j('ㅜ', "u", &[("oo", 2)]),         // 13
    j('ㅝ', "wo", &[("weo", 1)]),       // 14
    j('ㅞ', "we", &[]),                 // 15
    j('ㅟ', "wi", &[]),                 // 16
    j('ㅠ', "yu", &[]),                 // 17
    j('ㅡ', "eu", &[]),                 // 18
    j('ㅢ', "ui", &[("eui", 1)]),       // 19
    j('ㅣ', "i", &[]),                  // 20
];

/// 終聲表 JONGSEONG（索引 0..=27；T=0 為 None，代表無終聲）
pub const JONGSEONG: [Option<JamoSpec>; 28] = [
    None,                                       // 0
    Some(j('ㄱ', "k", &[("g", 1)])),            // 1
    Some(j('ㄲ', "kk", &[("k", 4)])),           // 2
    Some(j('ㄳ', "ks", &[("gs", 1)])),          // 3 (複合終聲)
    Some(j('ㄴ', "n", &[])),                    // 4
    Some(j('ㄵ', "nj", &[])),                   // 5 (複合終聲)
    Some(j('ㄶ', "nh", &[])),                   // 6 (複合終聲)
    Some(j('ㄷ', "d", &[("t", 3)])),            // 7
    Some(j('ㄹ', "l", &[("r", 1)])),            // 8
    Some(j('ㄺ', "lk", &[("lg", 1)])),          // 9 (複合終聲)
    Some(j('ㄻ', "lm", &[])),                   // 10 (複合終聲)
    Some(j('ㄼ', "lb", &[("lp", 3)])),          // 11 (複合終聲)
    Some(j('ㄽ', "ls", &[])),                   // 12 (複合終聲)
    Some(j('ㄾ', "lt", &[])),                   // 13 (複合終聲)
    Some(j('ㄿ', "lp", &[])),                   // 14 (複合終聲)
    Some(j('ㅀ', "lh", &[])),                   // 15 (複合終聲)
    Some(j('ㅁ', "m", &[])),                    // 16
    Some(j('ㅂ', "p", &[("b", 1)])),            // 17
    Some(j('ㅄ', "ps", &[("bs", 1)])),          // 18 (複合終聲)
    Some(j('ㅅ', "s", &[("t", 1)])),            // 19
    Some(j('ㅆ', "ss", &[("t", 2)])),           // 20
    Some(j('ㅇ', "ng", &[])),                   // 21
    Some(j('ㅈ', "j", &[("t", 3)])),            // 22
    Some(j('ㅊ', "ch", &[("t", 3)])),           // 23
    Some(j('ㅋ', "kh", &[("k", 3)])),           // 24
    Some(j('ㅌ', "th", &[("t", 4)])),           // 25
    Some(j('ㅍ', "ph", &[("p", 3)])),           // 26
    Some(j('ㅎ', "h", &[("t", 4)])),            // 27
];

pub const S_BASE: u32 = 0xAC00;
pub const L_COUNT: u32 = 19;
pub const V_COUNT: u32 = 21;
pub const T_COUNT: u32 = 28;
pub const N_COUNT: u32 = V_COUNT * T_COUNT; // 588
pub const S_COUNT: u32 = L_COUNT * N_COUNT; // 11172

/// 判斷終聲索引 T 是否為複合終聲（T = 3, 5, 6, 9..=15, 18）
pub fn is_compound_coda(t: usize) -> bool {
    matches!(t, 3 | 5 | 6 | 9..=15 | 18)
}

/// 將 (L, V, T) 合成為諺文音節；索引越界時回傳 None
pub fn compose(l: usize, v: usize, t: usize) -> Option<char> {
    if l >= 19 || v >= 21 || t >= 28 {
        return None;
    }
    let code = S_BASE + ((l as u32 * V_COUNT + v as u32) * T_COUNT) + t as u32;
    char::from_u32(code)
}

/// 將諺文音節拆解為 (L, V, T)
pub fn decompose(c: char) -> Option<(usize, usize, usize)> {
    let code = c as u32;
    let s = code.checked_sub(S_BASE)?;
    if s >= S_COUNT {
        return None;
    }
    let l = (s / N_COUNT) as usize;
    let v = ((s % N_COUNT) / T_COUNT) as usize;
    let t = (s % T_COUNT) as usize;
    Some((l, v, t))
}

/// 判斷字元是否為完整諺文音節（U+AC00..=U+D7A3）
pub fn is_hangul_syllable(c: char) -> bool {
    let code = c as u32;
    (S_BASE..S_BASE + S_COUNT).contains(&code)
}

/// 判斷「前一音節終聲 `prev_t` + 本音節初聲 `l`」的連寫字串，是否能在其他位置切開，
/// 且切開後左段仍是合法終聲、右段仍是合法初聲（皆以成本 0 的唯一拼寫計）。
/// 若可以，DP 會出現成本相同的另一種切分，必須以 `-` 明確標示邊界。
/// 例：無終聲 + `kk`（가까 `ga|kka` vs `gak|ka`）、`k` + `kk`（각까 `gak|kka` vs `gakk|ka`）、`s` + `ss`（갓싸）。
fn boundary_can_shift(prev_t: usize, l: usize) -> bool {
    let t_rr = JONGSEONG[prev_t].map(|s| s.rr).unwrap_or("");
    let combined = format!("{}{}", t_rr, CHOSEONG[l].rr);
    (1..combined.len())
        .filter(|&p| p != t_rr.len())
        .any(|p| {
            let (left, right) = combined.split_at(p);
            JONGSEONG.iter().flatten().any(|s| s.rr == left)
                && CHOSEONG.iter().any(|s| !s.rr.is_empty() && s.rr == right)
        })
}

/// 諺文 → 唯一拼寫（逐音節、逐字母使用 `rr` 欄位）。
/// 在以下邊界插入 `-`，以保證 `romaja_to_hangul(to_romaja(w), Finalize) == w`：
/// 1. 非詞首的零初聲（ㅇ）音節之前（例：항을 → `hang-eul`、서울 → `seo-ul`、가에 → `ga-e`）
/// 2. 終聲＋初聲的連寫可在別處切開成另一組合法終聲＋初聲時（見 [`boundary_can_shift`]）
/// 3. 前一音節為複合終聲時（避免複合終聲的 +5 成本使 DP 改把後半拆入下一音節）
pub fn to_romaja(hangul: &str) -> String {
    let mut out = String::new();
    let mut prev_syllable: Option<(usize, usize, usize)> = None;

    for c in hangul.chars() {
        if let Some((l, v, t)) = decompose(c) {
            if let Some((_prev_l, _prev_v, prev_t)) = prev_syllable {
                let need_hyphen =
                    l == 11 || boundary_can_shift(prev_t, l) || is_compound_coda(prev_t);

                if need_hyphen {
                    out.push('-');
                }
            }

            out.push_str(CHOSEONG[l].rr);
            out.push_str(JUNGSEONG[v].rr);
            if let Some(t_spec) = JONGSEONG[t] {
                out.push_str(t_spec.rr);
            }
            prev_syllable = Some((l, v, t));
        } else {
            out.push(c);
            prev_syllable = None;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_compose_specific_syllables() {
        assert_eq!(compose(18, 0, 4), Some('한'));
        assert_eq!(compose(0, 0, 0), Some('가'));
        assert_eq!(compose(18, 20, 27), Some('힣'));
        assert_eq!(compose(19, 0, 0), None);
        assert_eq!(compose(0, 21, 0), None);
        assert_eq!(compose(0, 0, 28), None);
    }

    #[test]
    fn test_all_11172_syllables_roundtrip() {
        let mut count = 0;
        for l in 0..19 {
            for v in 0..21 {
                for t in 0..28 {
                    let ch = compose(l, v, t).expect("compose failed for valid indices");
                    assert!(is_hangul_syllable(ch));
                    let (dl, dv, dt) = decompose(ch).expect("decompose failed for valid syllable");
                    assert_eq!((l, v, t), (dl, dv, dt));
                    count += 1;
                }
            }
        }
        assert_eq!(count, 11_172);
    }

    #[test]
    fn test_rr_uniqueness_within_each_table() {
        let mut onset_set = HashSet::new();
        for (idx, spec) in CHOSEONG.iter().enumerate() {
            if idx == 11 {
                assert_eq!(spec.rr, "");
                continue;
            }
            assert!(
                onset_set.insert(spec.rr),
                "Duplicate CHOSEONG rr: {}",
                spec.rr
            );
        }

        let mut vowel_set = HashSet::new();
        for spec in JUNGSEONG.iter() {
            assert!(!spec.rr.is_empty());
            assert!(
                vowel_set.insert(spec.rr),
                "Duplicate JUNGSEONG rr: {}",
                spec.rr
            );
        }

        let mut coda_set = HashSet::new();
        for spec in JONGSEONG.iter().flatten() {
            assert!(!spec.rr.is_empty());
            assert!(
                coda_set.insert(spec.rr),
                "Duplicate JONGSEONG rr: {}",
                spec.rr
            );
        }
    }

    #[test]
    fn test_compat_jamo_range() {
        for spec in CHOSEONG.iter() {
            let code = spec.compat as u32;
            assert!(
                (0x3131..=0x318E).contains(&code),
                "CHOSEONG compat {} out of range",
                spec.compat
            );
        }
        for spec in JUNGSEONG.iter() {
            let code = spec.compat as u32;
            assert!(
                (0x3131..=0x318E).contains(&code),
                "JUNGSEONG compat {} out of range",
                spec.compat
            );
        }
        for spec in JONGSEONG.iter().flatten() {
            let code = spec.compat as u32;
            assert!(
                (0x3131..=0x318E).contains(&code),
                "JONGSEONG compat {} out of range",
                spec.compat
            );
        }
    }

    #[test]
    fn test_to_romaja_examples() {
        assert_eq!(to_romaja("한글"), "hangeul");
        assert_eq!(to_romaja("항을"), "hang-eul");
        assert_eq!(to_romaja("서울"), "seo-ul");
        assert_eq!(to_romaja("가에"), "ga-e");
        assert_eq!(to_romaja("감사합니다"), "gamsahapnida");
    }
}
