//! 康熙字典筆畫 × 三才五格 × 81 數理零詞庫姓名與公司名推論模組 (Onomastics Engine)
//!
//! 對齊傳統熊崎氏姓名學與康熙字典部首筆畫折算規範，
//! 提供 O(1) 筆畫查表、五格（天格、人格、地格、外格、總格）與三才五行吉凶評估，
//! 支援零詞庫漢語姓名識別、同句前文指代、跨句 session 記憶與企業商號筆畫推論。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use crate::router::lattice::LatticeNode;

/// Layer-A 使用者自建詞庫動態人名註冊表（透過 OnceLock + RwLock 維持 RuhiEngine / LexiconManager ABI 凍結）
#[derive(Debug, Default, Clone)]
pub struct UserOnomasticRegistry {
    pub full_names: HashSet<String>,
    pub surnames: HashSet<String>,
    pub given_names: HashSet<String>,
    pub pairs: Vec<(String, String)>,
}

impl UserOnomasticRegistry {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.surnames.is_empty() && self.given_names.is_empty() && self.full_names.is_empty()
    }
}

static USER_ONOMASTIC_REGISTRY: OnceLock<RwLock<UserOnomasticRegistry>> = OnceLock::new();
static USER_ONOMASTIC_ACTIVE: AtomicBool = AtomicBool::new(false);

#[inline]
fn user_registry_lock() -> &'static RwLock<UserOnomasticRegistry> {
    USER_ONOMASTIC_REGISTRY.get_or_init(|| RwLock::new(UserOnomasticRegistry::default()))
}

pub fn register_user_onomastic_name(surname: &str, given: &str) {
    let s = surname.trim();
    let g = given.trim();
    if s.is_empty() || g.is_empty() {
        return;
    }
    if let Ok(mut reg) = user_registry_lock().write() {
        reg.full_names.insert(format!("{}{}", s, g));
        reg.surnames.insert(s.to_string());
        reg.given_names.insert(g.to_string());
        let pair = (s.to_string(), g.to_string());
        if !reg.pairs.contains(&pair) {
            reg.pairs.push(pair);
        }
        USER_ONOMASTIC_ACTIVE.store(true, Ordering::Release);
    }
}

pub fn clear_user_onomastic_registry() {
    USER_ONOMASTIC_ACTIVE.store(false, Ordering::Release);
    if let Some(lock) = USER_ONOMASTIC_REGISTRY.get() {
        if let Ok(mut reg) = lock.write() {
            reg.full_names.clear();
            reg.surnames.clear();
            reg.given_names.clear();
            reg.pairs.clear();
        }
    }
}

#[inline]
fn is_user_registered_surname(s_ch: char) -> bool {
    if !USER_ONOMASTIC_ACTIVE.load(Ordering::Acquire) {
        return false;
    }
    let Some(reg_lock) = USER_ONOMASTIC_REGISTRY.get() else {
        return false;
    };
    let Ok(reg) = reg_lock.read() else {
        return false;
    };
    let mut buf = [0u8; 4];
    let s = s_ch.encode_utf8(&mut buf);
    reg.surnames.contains(s)
}

/// BMP 漢字康熙字典筆畫二進位查表 (U+4E00..=U+9FFF, 共 20,992 bytes)
const KANGXI_STROKES_BMP: &[u8; 20992] = include_bytes!("../../../ruhi_lexicon/data/kangxi_strokes_bmp.bin");

/// O(1) 康熙標準筆畫查詢
#[inline]
pub fn kangxi_strokes(ch: char) -> Option<u8> {
    if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
        let st = KANGXI_STROKES_BMP[(ch as usize) - 0x4E00];
        if st > 0 {
            Some(st)
        } else {
            None
        }
    } else {
        None
    }
}

/// 熊崎氏 81 數理吉凶評級
/// 回傳值：+2 = 大吉，+1 = 吉，0 = 半吉／吉帶凶，-2 = 凶
pub fn numerology_81_tier(n: u16) -> i8 {
    if n == 0 {
        return 0;
    }
    let m = if n > 81 {
        ((n - 1) % 80) + 1
    } else {
        n
    };
    match m {
        1 | 3 | 5 | 6 | 7 | 8 | 11 | 13 | 15 | 16 | 21 | 23 | 24 | 25 | 29 | 31 | 32 | 33
        | 35 | 37 | 39 | 41 | 45 | 47 | 48 | 52 | 63 | 65 | 67 | 68 | 81 => 2,
        17 | 18 | 38 | 57 | 61 => 1,
        27 | 30 | 40 | 42 | 43 | 49 | 50 | 51 | 53 | 55 | 58 | 71 | 72 | 73 | 75 | 77 | 78 => 0,
        _ => -2,
    }
}

/// 三才五行相生比和評分
/// 尾數五行：1,2 木(0)；3,4 火(1)；5,6 土(2)；7,8 金(3)；9,0 水(4)
pub fn three_talents_score(tian: u16, ren: u16, di: u16) -> f64 {
    let elem = |x: u16| -> u8 {
        match x % 10 {
            1 | 2 => 0, // 木
            3 | 4 => 1, // 火
            5 | 6 => 2, // 土
            7 | 8 => 3, // 金
            _ => 4,     // 水
        }
    };
    let e_t = elem(tian);
    let e_r = elem(ren);
    let e_d = elem(di);

    // 檢查 (a, b) 關係：1 = 生/和，-1 = 剋
    let rel = |a: u8, b: u8| -> i8 {
        if a == b || (a + 1) % 5 == b || (b + 1) % 5 == a {
            1
        } else if (a + 2) % 5 == b || (b + 2) % 5 == a {
            -1
        } else {
            0
        }
    };

    let r_tr = rel(e_t, e_r);
    let r_rd = rel(e_r, e_d);

    if r_tr >= 1 && r_rd >= 1 {
        4.5
    } else if r_tr < 0 && r_rd < 0 {
        -4.5
    } else if r_tr < 0 || r_rd < 0 {
        -1.5
    } else {
        0.0
    }
}

/// 計算單姓雙名之五格與數理總評
/// 單姓雙名：天格 = s + 1, 人格 = s + g1, 地格 = g1 + g2, 總格 = s + g1 + g2, 外格 = g2 + 1
pub fn evaluate_single_surname_double_name(
    s_ch: char,
    g1_ch: char,
    g2_ch: char,
) -> Option<(f64, i8)> {
    let s = kangxi_strokes(s_ch)? as u16;
    let g1 = kangxi_strokes(g1_ch)? as u16;
    let g2 = kangxi_strokes(g2_ch)? as u16;

    let tian = s + 1;
    let ren = s + g1;
    let di = g1 + g2;
    let zong = s + g1 + g2;
    let wai = g2 + 1;

    let t_ren = numerology_81_tier(ren);
    let t_di = numerology_81_tier(di);
    let t_zong = numerology_81_tier(zong);
    let t_wai = numerology_81_tier(wai);

    let base_num_score = (t_ren as f64 * 2.2)
        + (t_di as f64 * 2.0)
        + (t_zong as f64 * 2.2)
        + (t_wai as f64 * 1.0);

    let three_talents = three_talents_score(tian, ren, di);
    let total_score = base_num_score + three_talents;
    let overall_tier = if three_talents <= -4.0 {
        -2 // 天地雙剋，大凶，不可成吉
    } else if t_ren < 0 || t_zong < 0 {
        -2 // 人格或總格大凶
    } else if t_ren >= 1 && t_di >= 1 && t_zong >= 1 {
        if three_talents >= 0.0 {
            2
        } else if t_wai >= 1 {
            1
        } else {
            0
        }
    } else if t_ren >= 0 && t_di >= 0 && t_zong >= 0 && (t_ren + t_di + t_zong) >= 2 {
        if three_talents >= 0.0 {
            1
        } else {
            0
        }
    } else {
        0
    };

    Some((total_score, overall_tier))
}

/// 台灣常見單姓（Tier A 常見姓氏庫，排除高頻虛詞）
pub const TIER_A_SURNAMES: &str = "陳林黃張李王吳劉蔡楊許鄭謝郭洪曾邱廖賴周徐蘇葉莊呂江何蕭羅高簡朱鍾施游詹沈彭胡余盧潘顏梁趙柯翁魏方孫戴范宋鄧杜侯曹薛傅丁溫紀蔣歐藍唐馬董石卓程姚康馮古姜湯汪白田涂鄒巫尤龔嚴韓袁金童陸夏柳邵錢莫秦賈韋萬龐樊伍倪祝葛苗熊崔譚黎龍顧孟雷賀殷畢齊尹屈駱凌霍虞裴喬閻耿寇饒闕辜阮戚岑";

/// 常見複姓
pub const COMPOUND_SURNAMES: &[&str] = &[
    "歐陽", "司馬", "上官", "諸葛", "司徒", "慕容", "皇甫", "范姜", "張簡",
];

/// 台灣常見取名漢字（500+ 字）
pub const NAMING_REGISTER_CHARS: &str =
    "玟蓉羽宸宥妍彤睿怡欣雅婷冠宇柏翰承恩軒品妤婕萱涵瑋琪瑜哲豪銘杰翔誠祐慈慧淑芬貞玲嘉宏昇偉強俊傑志明華榮宗憲政賢凱鈞廷威霖謙諺澤澄希晴柔筠筑甯潔瑩臻璇薇蕾靜雯霓霏霞露靖韻馨驊騏駿騰麟博智仁義禮信忠孝佩珊君芳萍娟敏麗秀美惠玉蘭梅竹菊蓮芷芸芯芮苡芊語諾翊詠歆晨曦昕曜昱昶晏晟暉暄映昭昀昊昂昌盛瑞祥禎祺福祿壽康泰順豐裕富貴鈺錦鎧鋒銳鎮鏡鑫淼焱垚喆沛沁沐泓洵浚浩海清淵淳添湘湛湜湞源溥溱溶滂滄滋滌瀅瀚瀛瀞瀟瀠瑟瑣瑤瑪瑭瑾璀璁璃璉璋璐璞璟璨璿瓊瓏瓔瓚羅若庭佑彥辰倫琳儀穎襄洋詩聖亨江衛兆申赫非巧啟啓楷漢";

/// 台灣高頻核心取名用字子集（用於同音雙名皆屬取名庫時之優先序微調，如 庭佑 > 廷祐、玟蓉 > 玟榮）
pub const CORE_NAMING_REGISTER_CHARS: &str =
    "玟蓉羽宸妍彤睿怡欣雅婷冠宇柏翰承恩軒品妤婕萱涵瑋琪瑜哲豪銘杰翔慈慧淑芬貞玲嘉宏俊傑志凱鈞威霖謙澤澄希晴柔筠筑甯潔瑩臻璇薇靜雯靖韻馨佩珊君芳萍娟敏秀惠芷芸芯芮語諾翊詠歆晨曦昕曜昱瑞祥禎祺羅若庭佑彥辰倫琳儀穎詩聖亨兆申";

/// 傳統漢語與台灣常見輩字用字（須搭配明確左側引介詞或右側稱謂／言說錨點，且另一字屬 NAMING_REGISTER_CHARS 方啟用）
pub const GENERATIONAL_GIVEN_CHARS: &str =
    "克文伯仲叔季家國世永秉崇德良正光孟紹振耀慶建邦守傳繼顯達道學欽敬睦敦錫之";

/// 非人名功能字排除庫
pub const NON_GIVEN_NAME_CHARS: &str =
    "的得地了著過嗎呢吧啊呀喔欸啦囉唄我你他她它們這那哪誰什麼幾個位名次種樣裡上下中內外前後是有沒不別很太更最都也還就才又再被把給讓從在到對向跟和與或及等來去進出回起開掉完走件條張顆隻支台輛棟間歲年月日天時分秒元塊錢吱呱喵汪嘎啪嘩哼呸啐啃咬吞吐哭吵罵屁屎尿鬼妖魔屍毒病痛死殺傻笨蠢瘋瞎爛臭髒醜";

/// 姓名引介詞
pub const NAME_INTRO_WORDS: &[&str] = &[
    "我是", "我叫", "叫", "名叫", "姓名", "名字", "在下", "敝人", "找", "請", "請問", "謝謝",
    "感謝", "聯絡", "作者", "作家", "記者", "學者", "教授", "老師", "醫師", "律師", "立委", "委員", "市長", "縣長", "議員", "院長", "校長", "部長", "局長", "委託", "尋找", "由",
    "老婆", "太太", "內人", "愛妻", "老公", "外子", "妻子", "丈夫",
    "女兒", "小女", "愛女", "長女", "次女", "么女", "獨生女",
    "兒子", "犬子", "小犬", "長子", "次子", "么子", "獨子",
    "女婿", "賢婿", "媳婦", "兒媳",
    "爸爸", "父親", "家父", "老爸", "媽媽", "母親", "家母", "老媽",
    "哥哥", "兄長", "大哥", "二哥", "弟弟", "小弟", "老弟",
    "姊姊", "姐姐", "大姊", "大姐", "二姊", "二姐", "妹妹", "小妹",
    "孫子", "長孫", "孫女", "外孫", "外孫女",
    "爺爺", "祖父", "奶奶", "祖母", "外公", "外婆", "阿公", "阿嬤",
    "岳父", "岳母", "公公", "婆婆",
    "伯父", "伯伯", "叔叔", "舅舅", "姑姑", "阿姨",
];

pub const MALE_KINSHIP_INTROS: &[&str] = &[
    "老公", "外子", "丈夫",
    "兒子", "犬子", "小犬", "長子", "次子", "么子", "獨子",
    "女婿", "賢婿",
    "爸爸", "父親", "家父", "老爸",
    "哥哥", "兄長", "大哥", "二哥", "弟弟", "小弟", "老弟",
    "孫子", "長孫", "外孫",
    "爺爺", "祖父", "外公", "阿公", "岳父", "公公",
    "伯父", "伯伯", "叔叔", "舅舅",
];

pub const FEMALE_KINSHIP_INTROS: &[&str] = &[
    "老婆", "太太", "內人", "愛妻", "妻子",
    "女兒", "小女", "愛女", "長女", "次女", "么女", "獨生女",
    "媳婦", "兒媳",
    "媽媽", "母親", "家母", "老媽",
    "姊姊", "姐姐", "大姊", "大姐", "二姊", "二姐", "妹妹", "小妹",
    "孫女", "外孫女",
    "奶奶", "祖母", "外婆", "阿嬤", "岳母", "婆婆",
    "姑姑", "阿姨",
];

pub const FEMALE_ONLY_GIVEN_CHARS: &str =
    "婷妍妤婕娟娥婉嫻嬌媚妃姬嫦姍妮娜妹姊姐娘姑媽婆嫂姨嬸蓉萱薇蕾淑芬芳萍麗美秀貞靜雯霞露馨珊蘭梅菊蓮芷芸芯芮芊璇瑩筑筠柔";

pub const MALE_ONLY_GIVEN_CHARS: &str =
    "豪雄偉強壯剛猛勇威震霆霸龍虎彪駿騏驊麟鵬傑俊哲宏凱鈞翰柏冠宗憲政賢昇昌盛鋒銳鎧鎮鑫宇";

/// 姓名右側稱謂、言說與句界標點
pub const NAME_RIGHT_CONTEXTS: &[&str] = &[
    "先生", "小姐", "女士", "老師", "教授", "醫師", "律師", "經理", "主任", "立委", "委員",
    "市長", "縣長", "議員", "同學", "董事長", "總經理", "執行長", "前輩", "院長", "校長",
    "局長", "部長", "專員",
    "說", "表示", "指出", "認為", "覺得", "聲稱", "直言",
    "，", "、", "。", "！", "？", "：", "；",
];

#[inline]
fn has_explicit_honorific_or_speech_at(
    dag: &HashMap<usize, Vec<LatticeNode>>,
    pos: usize,
) -> bool {
    dag.get(&pos).is_some_and(|right_nodes| {
        right_nodes.iter().any(|node| {
            matches!(
                node.word.as_str(),
                "先生" | "小姐" | "女士" | "老師" | "教授" | "醫師" | "律師" | "經理"
                    | "主任" | "立委" | "委員" | "市長" | "縣長" | "議員" | "同學"
                    | "董事長" | "總經理" | "執行長" | "前輩" | "院長" | "校長" | "局長"
                    | "部長" | "專員"
                    | "說" | "表示" | "指出" | "認為" | "覺得" | "聲稱" | "直言"
            ) || node.word.starts_with("先生")
                || node.word.starts_with("小姐")
                || node.word.starts_with("女士")
                || node.word.starts_with("老師")
                || node.word.starts_with("教授")
                || node.word.starts_with("醫師")
                || node.word.starts_with("律師")
                || node.word.starts_with("立委")
                || node.word.starts_with("市長")
                || node.word.starts_with("委員")
        })
    })
}

/// 孤立音節三連音子句主語錨點：當 [s_idx .. s_idx+3] 內部無任何雙字詞重疊（weight < 150）且外邊界無跨界詞（weight < 220），
/// 左側為句首／子句連詞／命題態度謂語（含「認定了」），右側緊接繫詞／情態詞／疑問焦點（是／是否／會／作了…）時，准予進入高門檻姓名推論
#[inline]
fn is_orphan_clause_subject_anchor(
    s_idx: usize,
    prefix_words_before_s: &[&str],
    dag: &HashMap<usize, Vec<LatticeNode>>,
) -> bool {
    let right_idx = s_idx + 3;
    let left_ok = if s_idx == 0 {
        true
    } else if let Some(&last_w) = prefix_words_before_s.last() {
        matches!(
            last_w,
            "因為" | "由於" | "所以" | "既然" | "如果" | "假如" | "雖然" | "儘管"
                | "認為" | "覺得" | "相信" | "認定" | "發現" | "證明" | "肯定" | "知道"
                | "聽說" | "希望" | "看到" | "看見"
        ) || (last_w == "了"
            && prefix_words_before_s
                .len()
                .checked_sub(2)
                .and_then(|i| prefix_words_before_s.get(i))
                .is_some_and(|&w2| {
                    matches!(
                        w2,
                        "認定" | "證明" | "發現" | "看到" | "看見" | "聽見" | "想起"
                    )
                }))
    } else {
        false
    };
    if !left_ok {
        return false;
    }

    let right_ok = dag.get(&right_idx).is_some_and(|ns| {
        ns.iter().any(|node| {
            matches!(
                node.word.as_str(),
                "是" | "是否" | "為" | "在" | "有" | "會" | "能" | "能夠" | "可" | "可以"
                    | "將" | "已" | "已經" | "也" | "都" | "卻" | "曾" | "曾經" | "作"
                    | "做" | "當" | "選" | "參選"
            )
        })
    });
    if !right_ok {
        return false;
    }

    let prev_cross = s_idx >= 1
        && dag.get(&(s_idx - 1)).is_some_and(|ns| {
            ns.iter()
                .any(|m| m.end_idx > s_idx && m.length() >= 2 && m.weight >= 220)
        });
    if prev_cross {
        return false;
    }
    for p in [s_idx, s_idx + 1] {
        if dag
            .get(&p)
            .is_some_and(|ns| ns.iter().any(|m| m.length() >= 2 && m.weight >= 150))
        {
            return false;
        }
    }
    if dag.get(&(s_idx + 2)).is_some_and(|ns| {
        ns.iter()
            .any(|m| m.end_idx > right_idx && m.length() >= 2 && m.weight >= 220)
    }) {
        return false;
    }
    true
}

#[inline]
fn roster_right_ok(
    g2_end: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> bool {
    g2_end >= n_syllables
        || dag.get(&g2_end).is_some_and(|nodes| {
            nodes.iter().any(|n| {
                n.word == "\n"
                    || NAME_RIGHT_CONTEXTS.iter().any(|&r| n.word.starts_with(r))
                    || matches!(
                        n.word.as_str(),
                        "," | "." | "!" | "?" | ";" | ":" | "（" | "(" | "／" | "/"
                            | "和" | "與" | "跟" | "以及" | "或" | "還有" | "等"
                            | "啊" | "呢" | "嗎" | "吧" | "喔" | "啦"
                    )
            })
        })
}

#[inline]
fn roster_context_info(
    prefix_words_before_s: &[&str],
    s_idx: usize,
    g2_end: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> (bool, bool, bool) {
    let enum_lead_before_s = prefix_words_before_s.last().is_some_and(|&w_prev| {
        matches!(
            w_prev,
            "像是" | "比如" | "例如" | "還有什麼"
        ) || (w_prev == "什麼"
            && prefix_words_before_s.len() >= 2
            && prefix_words_before_s[prefix_words_before_s.len() - 2].ends_with("還有"))
    });

    let non_tag_left_ok = prefix_words_before_s.is_empty()
        || enum_lead_before_s
        || prefix_words_before_s.last().is_some_and(|&w_prev| {
            w_prev.starts_with("__ROSTER")
                || w_prev == "\n"
                || w_prev.ends_with([
                    '：', ':', '、', '，', ',', '。', '！', '？', '；', '.', '!', '?', ';',
                    '\n', '\r', '有', '是', '為', '含', '括',
                ])
                || matches!(
                    w_prev,
                    "一個" | "一位" | "一名" | "還有" | "以及" | "與" | "和" | "跟"
                        | "包括" | "介紹" | "歡迎" | "像是" | "比如" | "例如"
                        | "隊長" | "組長" | "班長"
                        | "另一個" | "另一位" | "副隊長"
                        | "第一位" | "第二位" | "第三位"
                        | "第一個" | "第二個"
                )
        });

    if s_idx > 0 && !non_tag_left_ok {
        return (false, false, false);
    }
    if !roster_right_ok(g2_end, dag, n_syllables) {
        return (false, false, false);
    }

    let mut has_tag_f = false;
    let mut has_tag_m = false;
    let mut has_tag_any = false;
    let mut intra_f = false;
    let mut intra_m = false;
    let mut intra_any = enum_lead_before_s;

    for &w in prefix_words_before_s.iter().rev() {
        match w {
            "__ROSTER_F__" => {
                has_tag_f = true;
                has_tag_any = true;
            }
            "__ROSTER_M__" => {
                has_tag_m = true;
                has_tag_any = true;
            }
            "__ROSTER__" | "__ROSTER_COOL__" => {
                has_tag_any = true;
            }
            "女兒" | "姊妹" | "姐妹" => {
                intra_f = true;
                intra_any = true;
            }
            "兒子" | "兄弟" => {
                intra_m = true;
                intra_any = true;
            }
            "隊員" | "成員" | "團員" | "組員" | "班員" | "球員" | "選手" | "名單"
            | "人員" | "人選" | "同學" | "校友" | "來賓" | "貴賓" | "嘉賓"
            | "講者" | "與會者" | "出席者" | "候選人" | "得獎者" | "獲獎者"
            | "我們班" | "全班" | "本班" | "孩子" => {
                intra_any = true;
            }
            _ => {}
        }
    }

    let left_slot_ok = non_tag_left_ok || (has_tag_any && s_idx == 0);
    (
        (has_tag_any || intra_any) && left_slot_ok,
        has_tag_f || intra_f,
        has_tag_m || intra_m,
    )
}

#[inline]
fn is_clean_punct_bounded_gen_name(
    s_ch: char,
    s_idx: usize,
    prefix_words_before_s: &[&str],
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> bool {
    if !tier_a_surname_rank(s_ch).is_some_and(|r| r <= 50) {
        return false;
    }
    let g2_end = s_idx + 3;
    let left_boundary = s_idx == 0
        || prefix_words_before_s.last().is_some_and(|w| {
            w.starts_with("__ROSTER")
                || w.ends_with([
                    '，', '。', '！', '？', '、', '；', '：', ',', '.', '!', '?', ';', ':',
                    '\n',
                ])
        });
    if !left_boundary {
        return false;
    }
    let right_boundary = g2_end == n_syllables
        || dag.get(&g2_end).is_some_and(|ns| {
            ns.iter().any(|n| {
                matches!(
                    n.word.as_str(),
                    "，" | "、" | "。" | "！" | "？" | "：" | "；" | "," | "." | "!" | "?"
                        | ";" | ":" | "\n"
                )
            })
        });
    if !right_boundary {
        return false;
    }
    for p in [s_idx, s_idx + 1] {
        if dag
            .get(&p)
            .is_some_and(|ns| ns.iter().any(|m| m.length() >= 2 && m.weight >= 150))
        {
            return false;
        }
    }
    true
}

/// 判斷 [s_idx .. s_idx + 3] 是否位於合法之姓名語境 (Context Admissibility Gate)
pub fn is_onomastic_context_admissible(
    s_idx: usize,
    prefix_words_before_s: &[&str],
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> bool {
    let right_idx = s_idx + 3;

    // 1. 左側引介詞（如 我是、我叫、找、請、問、作者、由 等）
    let has_intro = if let Some(last_w) = prefix_words_before_s.last() {
        NAME_INTRO_WORDS.iter().any(|&w| last_w.ends_with(w))
    } else {
        false
    };
    if has_intro {
        return true;
    }

    // 2. 右側稱謂或言說謂語 (位於 s_idx + 3)
    if has_explicit_honorific_or_speech_at(dag, right_idx) {
        return true;
    }

    if is_orphan_clause_subject_anchor(s_idx, prefix_words_before_s, dag) {
        return true;
    }

    if roster_context_info(prefix_words_before_s, s_idx, right_idx, dag, n_syllables).0 {
        return true;
    }

    // 3. 獨立人名短句或標點子句：
    if s_idx == 0 {
        // (a) 獨立 3 字人名輸入（如直接敲擊「吳玟蓉」）
        if n_syllables == 3 {
            return true;
        }
        // (b) 句首 3 字人名且緊接標點符號（如「吳玟蓉，……」）
        if let Some(right_nodes) = dag.get(&right_idx) {
            let next_is_punct = right_nodes.iter().any(|n| {
                matches!(
                    n.word.as_str(),
                    "，" | "、" | "。" | "！" | "？" | "：" | "；" | "," | "." | "!" | "?"
                        | ":" | ";" | "\n"
                )
            });
            if next_is_punct {
                return true;
            }
        }
    } else {
        // s_idx > 0：若左側前接標點符號（子句開頭）
        let preceded_by_punct = prefix_words_before_s.last().map_or(false, |w| {
            w.ends_with('，')
                || w.ends_with('。')
                || w.ends_with('！')
                || w.ends_with('？')
                || w.ends_with('、')
                || w.ends_with('；')
                || w.ends_with('：')
                || w.ends_with(',')
                || w.ends_with('.')
                || w.ends_with('\n')
                || w.ends_with('!')
                || w.ends_with('?')
                || w.ends_with(';')
                || w.ends_with(':')
        });
        if preceded_by_punct {
            if right_idx == n_syllables {
                return true;
            }
            if let Some(right_nodes) = dag.get(&right_idx) {
                let next_is_punct = right_nodes.iter().any(|n| {
                    matches!(
                        n.word.as_str(),
                        "，" | "、" | "。" | "！" | "？" | "：" | "；" | "," | "." | "!" | "?"
                            | ":" | ";" | "\n"
                    )
                });
                if next_is_punct {
                    return true;
                }
            }
        }
    }

    false
}

/// 商號吉祥語素
pub const COMMERCIAL_BRAND_CHARS: &str =
    "宏昇昌盛鑫鼎瑞祥富邦聯發緯創康泰順豐榮耀華碩鴻海廣達仁寶奇美長榮陽明萬海遠東新光國泰元大凱基永豐玉山中信統一正新巨大研華威剛群聯瑞昱聯詠欣興穩懋大立嘉澤信驊力旺創意金銀寶吉興茂益冠捷達億慶豐盛隆泰通安成";

static TIER_A_SURNAMES_SET: std::sync::OnceLock<HashSet<char>> = std::sync::OnceLock::new();
static COMMERCIAL_BRAND_CHARS_SET: std::sync::OnceLock<HashSet<char>> = std::sync::OnceLock::new();

struct OnomasticCharBits {
    tier_a_surnames: [u64; 1024],
    naming_register: [u64; 1024],
    core_naming_register: [u64; 1024],
    generational_given: [u64; 1024],
    female_only_given: [u64; 1024],
    male_only_given: [u64; 1024],
    non_given_name: [u64; 1024],
    commercial_brand: [u64; 1024],
    company_suffix_first: [u64; 1024],
    surname_rank: Box<[u8; 65536]>,
}

impl OnomasticCharBits {
    fn from_chars_str(s: &str) -> [u64; 1024] {
        let mut bits = [0u64; 1024];
        for ch in s.chars() {
            let cp = ch as u32;
            if cp < 0x10000 {
                bits[(cp >> 6) as usize] |= 1u64 << (cp & 63);
            }
        }
        bits
    }

    #[inline]
    fn has(bits: &[u64; 1024], fallback_str: &str, ch: char) -> bool {
        let cp = ch as u32;
        if cp < 0x10000 {
            ((bits[(cp >> 6) as usize] >> (cp & 63)) & 1) != 0
        } else {
            fallback_str.contains(ch)
        }
    }
}

static ONOMASTIC_BITS: std::sync::OnceLock<OnomasticCharBits> = std::sync::OnceLock::new();

#[inline]
fn get_onomastic_bits() -> &'static OnomasticCharBits {
    ONOMASTIC_BITS.get_or_init(|| {
        let mut company_first = [0u64; 1024];
        for &sfx in COMPANY_INDUSTRY_SUFFIXES {
            if let Some(ch) = sfx.chars().next() {
                let cp = ch as u32;
                if cp < 0x10000 {
                    company_first[(cp >> 6) as usize] |= 1u64 << (cp & 63);
                }
            }
        }
        let mut surname_rank = Box::new([0u8; 65536]);
        for (idx, ch) in TIER_A_SURNAMES.chars().enumerate() {
            let cp = ch as u32;
            if cp < 0x10000 && surname_rank[cp as usize] == 0 {
                surname_rank[cp as usize] = (idx + 1).min(255) as u8;
            }
        }
        OnomasticCharBits {
            tier_a_surnames: OnomasticCharBits::from_chars_str(TIER_A_SURNAMES),
            naming_register: OnomasticCharBits::from_chars_str(NAMING_REGISTER_CHARS),
            core_naming_register: OnomasticCharBits::from_chars_str(CORE_NAMING_REGISTER_CHARS),
            generational_given: OnomasticCharBits::from_chars_str(GENERATIONAL_GIVEN_CHARS),
            female_only_given: OnomasticCharBits::from_chars_str(FEMALE_ONLY_GIVEN_CHARS),
            male_only_given: OnomasticCharBits::from_chars_str(MALE_ONLY_GIVEN_CHARS),
            non_given_name: OnomasticCharBits::from_chars_str(NON_GIVEN_NAME_CHARS),
            commercial_brand: OnomasticCharBits::from_chars_str(COMMERCIAL_BRAND_CHARS),
            company_suffix_first: company_first,
            surname_rank,
        }
    })
}

#[inline]
pub fn is_tier_a_surname(c: char) -> bool {
    let _ = &TIER_A_SURNAMES_SET;
    OnomasticCharBits::has(&get_onomastic_bits().tier_a_surnames, TIER_A_SURNAMES, c)
}

#[inline]
pub fn tier_a_surname_rank(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 0x10000 {
        let r = get_onomastic_bits().surname_rank[cp as usize];
        if r > 0 { Some(r) } else { None }
    } else {
        None
    }
}

#[inline]
fn surname_demographic_prior(s_ch: char) -> (f64, f64) {
    match tier_a_surname_rank(s_ch) {
        Some(1..=50) => (3.5, 4.0),
        Some(51..=80) => (2.0, 2.5),
        Some(81..=115) => (0.5, 0.5),
        Some(_) => (-1.0, -1.5),
        None => (0.0, 0.0),
    }
}

#[inline]
pub fn is_naming_register_char(c: char) -> bool {
    OnomasticCharBits::has(&get_onomastic_bits().naming_register, NAMING_REGISTER_CHARS, c)
}

#[inline]
pub fn is_core_naming_register_char(c: char) -> bool {
    OnomasticCharBits::has(
        &get_onomastic_bits().core_naming_register,
        CORE_NAMING_REGISTER_CHARS,
        c,
    )
}

#[inline]
pub fn is_generational_given_char(c: char) -> bool {
    OnomasticCharBits::has(
        &get_onomastic_bits().generational_given,
        GENERATIONAL_GIVEN_CHARS,
        c,
    )
}

#[inline]
fn is_female_only_given_char(c: char) -> bool {
    OnomasticCharBits::has(
        &get_onomastic_bits().female_only_given,
        FEMALE_ONLY_GIVEN_CHARS,
        c,
    )
}

#[inline]
fn is_male_only_given_char(c: char) -> bool {
    OnomasticCharBits::has(
        &get_onomastic_bits().male_only_given,
        MALE_ONLY_GIVEN_CHARS,
        c,
    )
}

#[inline]
pub fn is_non_given_name_char(c: char) -> bool {
    !('\u{4E00}'..='\u{9FFF}').contains(&c)
        || OnomasticCharBits::has(&get_onomastic_bits().non_given_name, NON_GIVEN_NAME_CHARS, c)
}

#[inline]
pub fn is_commercial_brand_char(c: char) -> bool {
    let _ = &COMMERCIAL_BRAND_CHARS_SET;
    OnomasticCharBits::has(&get_onomastic_bits().commercial_brand, COMMERCIAL_BRAND_CHARS, c)
}

#[inline]
fn is_company_suffix_first_char(c: char) -> bool {
    let cp = c as u32;
    if cp < 0x10000 {
        ((get_onomastic_bits().company_suffix_first[(cp >> 6) as usize] >> (cp & 63)) & 1) != 0
    } else {
        false
    }
}

/// 企業組織與產業後綴
pub const COMPANY_INDUSTRY_SUFFIXES: &[&str] = &[
    "科技", "實業", "興業", "企業", "生技", "光電", "電子", "半導體", "精密",
    "工業", "建設", "營造", "開發", "創投", "控股", "金控", "資訊", "數位",
    "國際", "貿易", "物流", "航運", "有限公司", "股份有限公司", "企業社",
];

/// 判斷 [s_idx .. s_idx + 3] 是否為前有親屬／引介稱謂且右側以稱謂／標點／句尾閉合之同位語人名框
pub fn is_bracketed_appositive_name_span(
    s_idx: usize,
    prefix_words_before_s: &[&str],
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> bool {
    let has_intro = prefix_words_before_s.last().map_or(false, |last_w| {
        NAME_INTRO_WORDS.iter().any(|&w| last_w.ends_with(w))
    });
    if !has_intro {
        return false;
    }
    let right_idx = s_idx + 3;
    if right_idx == n_syllables {
        return true;
    }
    if right_idx > n_syllables {
        return false;
    }
    dag.get(&right_idx).is_some_and(|nodes| {
        nodes.iter().any(|n| {
            NAME_RIGHT_CONTEXTS.iter().any(|&r| n.word.starts_with(r))
                || matches!(n.word.as_str(), "," | "." | "!" | "?" | ";" | ":")
        })
    })
}

/// 檢查是否通過詞彙複合詞否決閘門 (Lexical Veto)
pub fn passes_lexical_veto(
    s_idx: usize,
    g1_idx: usize,
    g2_end: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
) -> bool {
    passes_lexical_veto_with_span(s_idx, g1_idx, g2_end, dag, n_syllables, None, false)
}

/// 快速詞彙複合詞否決閘門（支援 backward profile 查表）
pub fn passes_lexical_veto_fast(
    s_idx: usize,
    g1_idx: usize,
    g2_end: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
    backward: Option<&crate::router::viterbi::LatticeBackwardProfile>,
) -> bool {
    passes_lexical_veto_with_span(s_idx, g1_idx, g2_end, dag, n_syllables, backward, false)
}

/// 詞彙複合詞否決閘門（支援前後同位語夾框邊界放寬，對一般語境維持嚴格否決）
pub fn passes_lexical_veto_with_span(
    s_idx: usize,
    g1_idx: usize,
    g2_end: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    n_syllables: usize,
    backward: Option<&crate::router::viterbi::LatticeBackwardProfile>,
    bracketed_appositive: bool,
) -> bool {
    let sg1_limit: u32 = if bracketed_appositive { 340 } else { 220 };
    let g1g2_limit: u32 = if bracketed_appositive { 340 } else { 220 };
    if let Some(bw) = backward {
        if bw.n_syllables > 0 && !bw.max_len2_weight_at.is_empty() {
            if bw.max_len3_weight_at.get(s_idx).copied().unwrap_or(0) >= 180 {
                return false;
            }
            if bw.max_len2_weight_at.get(g1_idx).copied().unwrap_or(0) >= g1g2_limit {
                return false;
            }
            if bw.max_len2_weight_at.get(s_idx).copied().unwrap_or(0) >= sg1_limit {
                return false;
            }
            if g2_end < n_syllables
                && g2_end > 0
                && bw.max_len2_weight_at.get(g2_end - 1).copied().unwrap_or(0) >= 320
            {
                return false;
            }
            return true;
        }
    }

    // 1. 若 dag 中存在任何跨越 s_idx 內部邊界的 3~4+ 字成語／複合詞（weight >= 180）➔ 否決
    if let Some(nodes) = dag.get(&s_idx) {
        for node in nodes {
            if node.word.chars().count() >= 3 && node.weight >= 180 {
                return false;
            }
        }
    }

    // 2. 若 dag.get(&g1_idx) 存在 length() >= 2 且 weight >= g1g2_limit 之常用雙字詞（如問題、文化、解析、所謂）➔ 否決
    if let Some(nodes) = dag.get(&g1_idx) {
        for node in nodes {
            if node.word.chars().count() >= 2 && node.weight >= g1g2_limit {
                return false;
            }
        }
    }

    // 3. 若 dag.get(&s_idx) 存在 length() >= 2 且 weight >= sg1_limit 之常用雙字詞（如無論、陳述、黃金、張開）➔ 否決
    if let Some(nodes) = dag.get(&s_idx) {
        for node in nodes {
            if node.word.chars().count() >= 2 && node.weight >= sg1_limit {
                return false;
            }
        }
    }

    // 4. 若 g2_end < n_syllables 且 dag.get(&(g2_end - 1)) 存在 length() >= 2 且 weight >= 320 之跨界詞➔ 否決
    if g2_end < n_syllables && g2_end > 0 {
        if let Some(nodes) = dag.get(&(g2_end - 1)) {
            for node in nodes {
                if node.word.chars().count() >= 2 && node.weight >= 320 {
                    return false;
                }
            }
        }
    }

    true
}

/// 判斷是否為合理之送出人名，供 Session 快取採集
pub fn is_plausible_committed_personal_name(
    surname: &str,
    g1: &str,
    g2: &str,
    left_ctx: &str,
    right_ctx: &str,
    lexicon: &crate::lexicon::LexiconManager,
) -> bool {
    let s_ch = match surname.chars().next() {
        Some(c) => c,
        None => return false,
    };
    let g1_ch = match g1.chars().next() {
        Some(c) => c,
        None => return false,
    };
    let g2_ch = match g2.chars().next() {
        Some(c) => c,
        None => return false,
    };

    if !is_tier_a_surname(s_ch) {
        return false;
    }
    if is_non_given_name_char(g1_ch) || is_non_given_name_char(g2_ch) {
        return false;
    }

    let has_intro = NAME_INTRO_WORDS.iter().any(|&w| left_ctx.ends_with(w));
    let has_right = NAME_RIGHT_CONTEXTS.iter().any(|&w| right_ctx.starts_with(w));
    let sg1_limit: u32 = if has_intro && has_right { 320 } else { 220 };

    // 檢查 [S·G1] 或 [G1·G2] 是否為常用雙字詞
    let sg1 = format!("{}{}", s_ch, g1_ch);
    let g1g2 = format!("{}{}", g1_ch, g2_ch);
    if let Some(ids) = lexicon.word_index.get(&sg1) {
        if ids.iter().any(|&eid| lexicon.entries.get(eid).map_or(false, |e| e.weight >= sg1_limit)) {
            return false;
        }
    }
    if let Some(ids) = lexicon.word_index.get(&g1g2) {
        if ids.iter().any(|&eid| lexicon.entries.get(eid).map_or(false, |e| e.weight >= 220)) {
            return false;
        }
    }

    let g1_in_reg = is_naming_register_char(g1_ch);
    let g2_in_reg = is_naming_register_char(g2_ch);
    let gen_reg_pair = (is_generational_given_char(g1_ch) && g2_in_reg)
        || (g1_in_reg && is_generational_given_char(g2_ch));
    if let Some((score, tier)) = evaluate_single_surname_double_name(s_ch, g1_ch, g2_ch) {
        if has_intro || has_right {
            tier >= 0 || score >= 0.0 || (g1_in_reg && g2_in_reg) || gen_reg_pair
        } else {
            (g1_in_reg && g2_in_reg && score >= 0.0)
                || ((g1_in_reg || g2_in_reg || gen_reg_pair) && tier >= 1 && score >= 8.0)
                || ((g1_in_reg || g2_in_reg) && tier >= 1 && score > 2.0)
        }
    } else {
        has_intro && (g1_in_reg || g2_in_reg)
    }
}

#[inline]
pub(crate) fn is_company_suffix_word(word: &str) -> bool {
    if let Some(c0) = word.chars().next() {
        is_company_suffix_first_char(c0)
            && COMPANY_INDUSTRY_SUFFIXES.iter().any(|&sfx| word.starts_with(sfx))
    } else {
        false
    }
}

#[inline]
fn dag_has_company_suffix_at(dag: &HashMap<usize, Vec<LatticeNode>>, pos: usize) -> bool {
    if let Some(nodes) = dag.get(&pos) {
        nodes.iter().any(|node| is_company_suffix_word(&node.word))
    } else {
        false
    }
}

#[derive(Clone, Copy, Default)]
pub struct OnomasticPrefixCtx {
    pub coref_names: [(char, char); 12],
    pub coref_len: usize,
    pub coref_surnames: [char; 12],
    pub coref_surnames_len: usize,
}

pub fn prepare_onomastic_prefix_ctx(prefix_words: &[&str]) -> OnomasticPrefixCtx {
    let mut ctx = OnomasticPrefixCtx::default();
    let n_p = prefix_words.len();
    if n_p == 0 {
        return ctx;
    }
    for i in 0..n_p {
        let w = prefix_words[i].trim_end_matches(|c| {
            matches!(
                c,
                '，' | '。' | '、' | '；' | '：' | '！' | '？' | ',' | '.' | ';' | ':' | '!' | '?'
            )
        });
        let mut detected_chars: Option<(Option<char>, char, char)> = None;
        let mut char_count = 0;
        let mut c0 = '\0';
        let mut c1 = '\0';
        let mut c2 = '\0';
        for (idx, ch) in w.chars().enumerate() {
            if idx == 0 { c0 = ch; }
            else if idx == 1 { c1 = ch; }
            else if idx == 2 { c2 = ch; }
            char_count += 1;
            if char_count > 3 { break; }
        }
        if char_count == 3
            && is_tier_a_surname(c0)
            && !is_non_given_name_char(c1)
            && !is_non_given_name_char(c2)
            && (is_naming_register_char(c1) || is_naming_register_char(c2))
        {
            detected_chars = Some((Some(c0), c1, c2));
        } else if i + 1 < n_p {
            let s_candidate = prefix_words[i];
            let mut s_it = s_candidate.chars();
            let (is_valid_surname, single_s) = match (s_it.next(), s_it.next()) {
                (Some(s_c0), None) => (is_tier_a_surname(s_c0), Some(s_c0)),
                (Some(_), Some(_)) => (COMPOUND_SURNAMES.contains(&s_candidate), None),
                _ => (false, None),
            };
            if is_valid_surname {
                let g12_trimmed = prefix_words[i + 1].trim_end_matches(|c| {
                    matches!(
                        c,
                        '，' | '。' | '、' | '；' | '：' | '！' | '？' | ',' | '.' | ';' | ':' | '!' | '?'
                    )
                });
                let mut g12_it = g12_trimmed.chars();
                if let (Some(g1_c), Some(g2_c), None) = (g12_it.next(), g12_it.next(), g12_it.next()) {
                    if !is_non_given_name_char(g1_c)
                        && !is_non_given_name_char(g2_c)
                        && (is_naming_register_char(g1_c) || is_naming_register_char(g2_c))
                    {
                        detected_chars = Some((single_s, g1_c, g2_c));
                    }
                } else if i + 2 < n_p {
                    let g1_candidate = prefix_words[i + 1];
                    let g2_candidate = prefix_words[i + 2].trim_end_matches(|c| {
                        matches!(
                            c,
                            '，' | '。' | '、' | '；' | '：' | '！' | '？' | ',' | '.' | ';' | ':' | '!' | '?'
                        )
                    });
                    let mut g1_it = g1_candidate.chars();
                    let mut g2_it = g2_candidate.chars();
                    if let (Some(g1_c), None, Some(g2_c), None) =
                        (g1_it.next(), g1_it.next(), g2_it.next(), g2_it.next())
                    {
                        if !is_non_given_name_char(g1_c)
                            && !is_non_given_name_char(g2_c)
                            && (is_naming_register_char(g1_c) || is_naming_register_char(g2_c))
                        {
                            detected_chars = Some((single_s, g1_c, g2_c));
                        }
                    }
                }
            }
        }

        if let Some((opt_s, dg1, dg2)) = detected_chars {
            if ctx.coref_len < ctx.coref_names.len() {
                ctx.coref_names[ctx.coref_len] = (dg1, dg2);
                ctx.coref_len += 1;
            }
            if let Some(s_ch) = opt_s {
                if ctx.coref_surnames_len < ctx.coref_surnames.len()
                    && !ctx.coref_surnames[..ctx.coref_surnames_len].contains(&s_ch)
                {
                    ctx.coref_surnames[ctx.coref_surnames_len] = s_ch;
                    ctx.coref_surnames_len += 1;
                }
            }
        }
    }
    ctx
}

/// 姓名學與商號推論 Viterbi 評分入口
pub fn score_onomastic_bonus(
    prefix_words: &[&str],
    curr_word: &str,
    curr_bopomofo: &str,
    end_idx: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    session_given_names: &HashSet<String>,
) -> f64 {
    score_onomastic_bonus_with_ctx(
        prefix_words,
        None,
        None,
        curr_word,
        curr_bopomofo,
        end_idx,
        dag,
        session_given_names,
    )
}

/// 姓名學與商號推論 Viterbi 評分入口（支援前驅指代快取與後向邊界快取）
pub fn score_onomastic_bonus_with_ctx(
    prefix_words: &[&str],
    ono_ctx: Option<&OnomasticPrefixCtx>,
    backward: Option<&crate::router::viterbi::LatticeBackwardProfile>,
    curr_word: &str,
    _curr_bopomofo: &str,
    end_idx: usize,
    dag: &HashMap<usize, Vec<LatticeNode>>,
    session_given_names: &HashSet<String>,
) -> f64 {
    let n_p = prefix_words.len();
    let prev_word = if n_p >= 1 { prefix_words[n_p - 1] } else { "" };
    let prev2_word = if n_p >= 2 { prefix_words[n_p - 2] } else { "" };

    let (curr_c0, curr_c1, curr_char_len) = {
        let mut it = curr_word.chars();
        match (it.next(), it.next(), it.next()) {
            (None, _, _) => ('\0', '\0', 0usize),
            (Some(c0), None, _) => (c0, '\0', 1usize),
            (Some(c0), Some(c1), None) => (c0, c1, 2usize),
            (Some(c0), Some(c1), Some(_)) => (c0, c1, 3usize),
        }
    };

    let mut bonus = 0.0;

    let fallback_ctx: OnomasticPrefixCtx;
    let active_ctx: Option<&OnomasticPrefixCtx> =
        if n_p >= 1 && (curr_char_len == 1 || curr_char_len == 2) {
            if let Some(ctx) = ono_ctx {
                Some(ctx)
            } else {
                fallback_ctx = prepare_onomastic_prefix_ctx(prefix_words);
                Some(&fallback_ctx)
            }
        } else {
            None
        };

    let has_bw = backward.map_or(false, |b| b.n_syllables > 0 && !b.company_suffix_at.is_empty());
    let has_company_suffix_pos = |pos: usize| -> bool {
        if has_bw {
            backward.unwrap().company_suffix_at.get(pos).copied().unwrap_or(false)
        } else {
            dag_has_company_suffix_at(dag, pos)
        }
    };
    let has_attested_len2_at = |pos: usize| -> bool {
        if has_bw {
            backward.unwrap().max_len2_weight_at.get(pos).copied().unwrap_or(0) >= 220
        } else {
            dag.get(&pos).map_or(false, |ns| {
                ns.iter().any(|n| n.word.chars().count() >= 2 && n.weight >= 220)
            })
        }
    };

    let mut n_syllables_cache: Option<usize> = None;
    let mut get_n_syllables = || -> usize {
        if let Some(v) = n_syllables_cache {
            v
        } else {
            let v = if has_bw {
                backward.unwrap().n_syllables.max(end_idx)
            } else {
                dag.keys().max().copied().map(|m| m + 1).unwrap_or(0).max(end_idx)
            };
            n_syllables_cache = Some(v);
            v
        }
    };

    // ── 0. 跨句 Session 名字快取加成 ──
    if !session_given_names.is_empty() {
        let n_syl_0 = get_n_syllables();
        let session_full_name_ok =
            |s_idx: usize, g1_idx: usize, g2_end: usize, p_before_s: &[&str]| -> bool {
                let in_roster_0 =
                    roster_context_info(p_before_s, s_idx, g2_end, dag, n_syl_0).0;
                let has_intro_0 = p_before_s
                    .last()
                    .is_some_and(|pw| NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w)));
                let bracketed_0 =
                    is_bracketed_appositive_name_span(s_idx, p_before_s, dag, n_syl_0);
                (in_roster_0 || is_onomastic_context_admissible(s_idx, p_before_s, dag, n_syl_0))
                    && passes_lexical_veto_with_span(
                        s_idx,
                        g1_idx,
                        g2_end,
                        dag,
                        n_syl_0,
                        backward,
                        in_roster_0 || has_intro_0 || bracketed_0,
                    )
            };
        if curr_char_len == 2 && session_given_names.contains(curr_word) {
            let g1_idx = end_idx.saturating_sub(2);
            let mut pit = prev_word.chars();
            if let (Some(s_ch), None) = (pit.next(), pit.next()) {
                if is_tier_a_surname(s_ch) && end_idx >= 3 {
                    let s_idx = end_idx - 3;
                    let p_before_s = if n_p >= 1 { &prefix_words[..n_p - 1] } else { &[] };
                    if session_full_name_ok(s_idx, g1_idx, end_idx, p_before_s) {
                        let mut s0_bonus = 22.0;
                        let (_, mut demo_b) = surname_demographic_prior(s_ch);
                        if is_user_registered_surname(s_ch) {
                            demo_b = demo_b.max(0.0) + 2.0;
                        }
                        let has_intro_0 = p_before_s.last().map_or(false, |pw| {
                            NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w))
                        });
                        let coref_b = if has_intro_0
                            && active_ctx.is_some_and(|ctx| {
                                ctx.coref_surnames[..ctx.coref_surnames_len].contains(&s_ch)
                            }) {
                            3.0
                        } else {
                            0.0
                        };
                        s0_bonus += demo_b + coref_b;
                        if let Some((score, tier)) =
                            evaluate_single_surname_double_name(s_ch, curr_c0, curr_c1)
                        {
                            if tier >= 2 {
                                s0_bonus += 2.0;
                            }
                            s0_bonus += (score * 0.06).clamp(-1.2, 1.8);
                        }
                        bonus += s0_bonus.clamp(-35.0, 35.0);
                    }
                }
            }
        }
        if !prev_word.is_empty() && curr_char_len == 1 && prev_word.chars().count() == 1 {
            let g1_idx = end_idx.saturating_sub(2);
            let combined = format!("{}{}", prev_word, curr_word);
            if session_given_names.contains(&combined) {
                let prev2_is_surname = {
                    let mut it2 = prev2_word.chars();
                    matches!((it2.next(), it2.next()), (Some(c), None) if is_tier_a_surname(c))
                };
                let ok_0b = if prev2_is_surname && end_idx >= 3 {
                    let s_idx = end_idx - 3;
                    let p_before_s = if n_p >= 2 { &prefix_words[..n_p - 2] } else { &[] };
                    session_full_name_ok(s_idx, g1_idx, end_idx, p_before_s)
                } else {
                    !has_attested_len2_at(g1_idx)
                        && (prev2_word.is_empty()
                            || NAME_INTRO_WORDS.iter().any(|&w| prev2_word.ends_with(w)))
                        && (end_idx == n_syl_0
                            || has_explicit_honorific_or_speech_at(dag, end_idx))
                };
                if ok_0b {
                    bonus += 20.0;
                }
            }
        }
        if curr_char_len == 1 {
            let g1_idx = end_idx.saturating_sub(1);
            let prev_is_surname = {
                let mut it1 = prev_word.chars();
                matches!((it1.next(), it1.next()), (Some(c), None) if is_tier_a_surname(c))
            };
            let ok_0c = if prev_is_surname && end_idx >= 2 {
                let s_idx = end_idx - 2;
                let g2_end = end_idx + 1;
                let p_before_s = if n_p >= 1 { &prefix_words[..n_p - 1] } else { &[] };
                session_full_name_ok(s_idx, g1_idx, g2_end, p_before_s)
            } else {
                !has_attested_len2_at(g1_idx)
                    && (prev_word.is_empty()
                        || NAME_INTRO_WORDS.iter().any(|&w| prev_word.ends_with(w)))
                    && (end_idx + 1 == n_syl_0
                        || has_explicit_honorific_or_speech_at(dag, end_idx + 1))
            };
            if ok_0c {
                // 若當前單字恰為 session 中某個 2 字名字的 G1，且右側 dag 有對應 G2（若 s_idx 已有該 3 字全名節點則不拆散）
                for g_name in session_given_names {
                    let mut g_chars = g_name.chars();
                    if let (Some(c1), Some(c2)) = (g_chars.next(), g_chars.next()) {
                        if curr_c0 == c1 {
                            let has_exact_len3_name = prev_is_surname
                                && end_idx >= 2
                                && dag.get(&(end_idx - 2)).is_some_and(|nodes| {
                                    nodes.iter().any(|n| {
                                        n.length() == 3
                                            && n.word.starts_with(prev_word)
                                            && n.word.ends_with(g_name.as_str())
                                    })
                                });
                            if has_exact_len3_name {
                                continue;
                            }
                            if let Some(next_nodes) = dag.get(&end_idx) {
                                if next_nodes.iter().any(|node| node.word.starts_with(c2)) {
                                    bonus += 6.0;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 1. 同句內前文指代 (Intra-Sentence Coreference) ──
    // 掃描 prefix_words 中是否存在完整姓名 [S, G1, G2] 或已識別名字
    if n_p >= 2 && (curr_char_len == 1 || curr_char_len == 2) {
        let curr_chars_pair: Option<(char, char)> = if curr_char_len == 2 {
            Some((curr_c0, curr_c1))
        } else {
            None
        };
        let curr_single: Option<char> = if curr_char_len == 1 {
            Some(curr_c0)
        } else {
            None
        };
        let prev_single: Option<char> = {
            let mut it = prev_word.chars();
            match (it.next(), it.next()) {
                (Some(a), None) => Some(a),
                _ => None,
            }
        };

        if let Some(ctx) = active_ctx {
            for &(dg1, dg2) in &ctx.coref_names[..ctx.coref_len] {
                if curr_chars_pair == Some((dg1, dg2)) {
                    bonus += 22.0;
                } else if prev_single == Some(dg1) && curr_single == Some(dg2) {
                    bonus += 20.0;
                } else if curr_single == Some(dg1) {
                    if let Some(next_nodes) = dag.get(&end_idx) {
                        if next_nodes.iter().any(|node| node.word.starts_with(dg2)) {
                            bonus += 14.0;
                        }
                    }
                }
            }
        }
    }

    // ── 2. 企業商號推論閘門 (Company Brand Gate) ──
    // 檢查右側是否緊鄰企業產業後綴（如 科技、實業、電子 等）
    let has_company_suffix_after = (curr_char_len == 1 || curr_char_len == 2)
        && end_idx >= 2
        && has_company_suffix_pos(end_idx);

    if has_company_suffix_after && end_idx >= 2 {
        let b_idx = end_idx - 2;
        let has_attested_modifier = has_attested_len2_at(b_idx);
        if !has_attested_modifier {
            if curr_char_len == 2 {
                let b1 = curr_c0;
                let b2 = curr_c1;
                let s1 = kangxi_strokes(b1).unwrap_or(0) as u16;
                let s2 = kangxi_strokes(b2).unwrap_or(0) as u16;
                let zong = s1 + s2;
                let num_tier = numerology_81_tier(zong);

                let b1_in_reg = is_commercial_brand_char(b1);
                let b2_in_reg = is_commercial_brand_char(b2);

                if b1_in_reg && b2_in_reg {
                    let t_bonus = if num_tier >= 2 { 8.0 } else if num_tier >= 1 { 4.5 } else { 0.0 };
                    bonus += 14.0 + t_bonus;
                } else if b1_in_reg || b2_in_reg {
                    bonus += 6.0;
                } else {
                    // 非商號常用字之低權重罕見詞（如「紅生」wt=75），扣分
                    bonus -= 14.0;
                }
            } else if curr_char_len == 1 && {
                let mut pit = prev_word.chars();
                matches!((pit.next(), pit.next()), (Some(_), None))
            } {
                // Step B2: 位於商號第 2 字 (curr_word 為 B2, prev_word 為 B1, 右側緊鄰企業後綴)
                let b1 = prev_word.chars().next().unwrap();
                let b2 = curr_c0;
                let s1 = kangxi_strokes(b1).unwrap_or(0) as u16;
                let s2 = kangxi_strokes(b2).unwrap_or(0) as u16;
                let zong = s1 + s2;
                let num_tier = numerology_81_tier(zong);

                let b1_in_reg = is_commercial_brand_char(b1);
                let b2_in_reg = is_commercial_brand_char(b2);

                if b1_in_reg && b2_in_reg {
                    let t_bonus = if num_tier >= 2 { 10.0 } else if num_tier >= 1 { 6.0 } else { 0.0 };
                    bonus += 16.0 + t_bonus;
                } else if b1_in_reg || b2_in_reg {
                    let t_bonus = if num_tier >= 2 { 5.0 } else if num_tier >= 1 { 2.5 } else { 0.0 };
                    bonus += 8.0 + t_bonus;
                } else {
                    bonus -= 8.0;
                }
            }
        }
    } else if curr_char_len == 1 && end_idx >= 1 && has_company_suffix_pos(end_idx + 1) {
        // Step B1: 位於商號第 1 字 (curr_word 為 B1, 預看右側 B2 + 企業後綴)
        let b_idx = end_idx - 1;
        let has_attested_modifier = has_attested_len2_at(b_idx);
        if !has_attested_modifier {
            let b1 = curr_c0;
            let b1_in_reg = is_commercial_brand_char(b1);
            let s1 = kangxi_strokes(b1).unwrap_or(0) as u16;
            if let Some(next_nodes) = dag.get(&end_idx) {
                for n2 in next_nodes {
                    let mut n2_it = n2.word.chars();
                    if let (Some(b2), None) = (n2_it.next(), n2_it.next()) {
                        let s2 = kangxi_strokes(b2).unwrap_or(0) as u16;
                        let num_tier = numerology_81_tier(s1 + s2);
                        let b2_in_reg = is_commercial_brand_char(b2);

                        if b1_in_reg && b2_in_reg {
                            bonus += if num_tier >= 1 { 14.0 } else { 10.0 };
                            break;
                        } else if b1_in_reg {
                            bonus += 7.0;
                            break;
                        }
                    }
                }
            }
        }
    }

    // ── 3. 零詞庫單姓雙名人名推論（Step S、Step A 與 Step B）──

    // ── 0b. Layer-A 使用者自建人名註冊表投影（姓氏＋尊稱／單稱名字／名字＋親屬同輩，嚴防誤傷慣用「大事」）──
    if USER_ONOMASTIC_ACTIVE.load(Ordering::Acquire) {
        if let Some(reg_lock) = USER_ONOMASTIC_REGISTRY.get() {
            if let Ok(reg) = reg_lock.read() {
                if !reg.is_empty() {
                const HONORIFICS_RIGHT: &[&str] = &[
                    "先生", "小姐", "女士", "老師", "教授", "醫師", "律師", "經理", "主任", "委員",
                    "同學", "院長", "校長", "部長", "局長", "處長", "總裁", "教練", "學長", "學姊",
                    "助教", "店長",
                ];
                const KINSHIP_PEER_RIGHT: &[&str] = &[
                    "哥", "兄", "弟", "姊", "姐", "妹", "老師", "學長", "學姊", "助教", "同學",
                    "醫師", "先生", "小姐",
                ];
                const GIVEN_LEFT_INTROS: &[&str] = &[
                    "找", "問", "叫", "喊", "請", "跟", "和", "對", "向", "給", "替", "幫", "由",
                    "讓", "陪", "約", "見", "通知", "聯絡", "告訴", "感謝", "謝謝", "我是", "我叫",
                    "恭喜", "詢問", "拜託", "邀請", "傳給",
                ];
                const GIVEN_RIGHT_VERBS: &[&str] = &[
                    "說", "問", "答", "講", "告訴", "回", "來", "去", "走", "跑", "笑", "看", "想",
                    "覺得", "認為", "表示", "指出", "寫", "做", "弄", "打", "吃", "喝", "住", "在",
                    "是", "有", "沒", "會", "能", "要", "幫", "跟", "給", "送", "買", "拿", "找",
                    "請", "都", "也", "又", "還", "就", "啊", "呢", "嗎", "吧", "喔",
                ];

                let curr_start_idx = end_idx.saturating_sub(curr_char_len);
                let dag_has_reg_given_here = dag.get(&curr_start_idx).is_some_and(|ns| {
                    ns.iter().any(|m| reg.given_names.contains(m.word.as_str()))
                });
                let non_name_event_left = matches!(
                    prev_word,
                    "國家" | "人生" | "頭等" | "這件" | "一件" | "哪件" | "幾件" | "件" | "種"
                        | "樁" | "天下" | "終身" | "婚姻" | "政治" | "國際" | "世界" | "家庭"
                        | "家裡" | "公家" | "正經" | "重要" | "重大" | "緊要" | "什麼" | "沒什"
                        | "多大" | "大事" | "出" | "生" | "成" | "幹" | "做" | "辦" | "聊"
                        | "談" | "提"
                ) || prev_word.ends_with('件')
                    || prev_word.ends_with("什麼")
                    || (prev2_word == "國" && prev_word == "家")
                    || (prev2_word == "人" && prev_word == "生")
                    || (prev2_word == "頭" && prev_word == "等")
                    || (matches!(prev_word, "是" | "的" | "了" | "個")
                        && (matches!(
                            prev2_word,
                            "國家" | "人生" | "頭等" | "這件" | "一件" | "重大" | "重要"
                                | "什麼" | "件"
                        ) || prev2_word.ends_with('件')));
                let non_name_event_right = dag.get(&end_idx).is_some_and(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "紀" | "記" | "年表" | "臨頭" | "化小" | "已去" | "不妙" | "不好"
                                | "底定"
                        )
                    })
                });
                let is_event_dashi_ctx = non_name_event_left || non_name_event_right;

                let mut reg_bonus: f64 = 0.0;

                // (2b) 無生命／慣用「大事」等常見詞嚴格守護
                if is_event_dashi_ctx && dag_has_reg_given_here {
                    if reg.given_names.contains(curr_word) {
                        reg_bonus -= 35.0;
                    } else if curr_word == "大事" {
                        reg_bonus += 34.0;
                    }
                }

                // (1) 姓氏＋尊稱（如 曹先生/小姐/老師/醫師/主任/教授/女士/同學）
                if reg.surnames.contains(curr_word) {
                    let right_has_honorific = dag.get(&end_idx).is_some_and(|ns| {
                        ns.iter().any(|m| {
                            HONORIFICS_RIGHT.contains(&m.word.as_str())
                                || HONORIFICS_RIGHT.iter().any(|&t| {
                                    m.word.chars().count() == 1
                                        && t.starts_with(m.word.as_str())
                                        && dag.get(&m.end_idx).is_some_and(|ns2| {
                                            ns2.iter().any(|m2| t.ends_with(m2.word.as_str()))
                                        })
                                })
                        })
                    });
                    if right_has_honorific {
                        reg_bonus += 26.0;
                    }
                }
                if reg.surnames.contains(prev_word) && HONORIFICS_RIGHT.contains(&curr_word) {
                    reg_bonus += 26.0;
                }

                // (3) 名字＋親屬／同輩稱謂（如 大十哥/兄/弟/姊/姐/妹/老師/學長/學姊）
                if reg.given_names.contains(curr_word) && !is_event_dashi_ctx {
                    let right_has_kinship = dag.get(&end_idx).is_some_and(|ns| {
                        ns.iter().any(|m| KINSHIP_PEER_RIGHT.contains(&m.word.as_str()))
                    });
                    if right_has_kinship {
                        reg_bonus += 28.0;
                    }
                }
                if reg.given_names.contains(prev_word) && KINSHIP_PEER_RIGHT.contains(&curr_word) {
                    if curr_word == "姐" && _curr_bopomofo == "ㄐㄧㄝˇ" {
                        reg_bonus += 32.0;
                    } else if curr_word == "姊" && _curr_bopomofo == "ㄐㄧㄝˇ" {
                        reg_bonus += 18.0;
                    } else {
                        reg_bonus += 30.0;
                    }
                }

                // (2a) 單稱名字（對話／動詞賓語／主語／獨詞：找大十、問大十、大十說、大十）
                if reg.given_names.contains(curr_word) && !is_event_dashi_ctx {
                    let has_person_left = GIVEN_LEFT_INTROS
                        .iter()
                        .any(|&w| prev_word == w || prev_word.ends_with(w));
                    let has_person_right = dag.get(&end_idx).is_some_and(|ns| {
                        ns.iter().any(|m| GIVEN_RIGHT_VERBS.contains(&m.word.as_str()))
                    });
                    let is_standalone = curr_start_idx == 0 && end_idx == get_n_syllables();
                    let is_clause_head = (curr_start_idx == 0
                        || matches!(prev_word, "，" | "、" | "：" | "「" | ","))
                        && (has_person_right || end_idx == get_n_syllables());
                    if has_person_left || has_person_right || is_standalone || is_clause_head {
                        reg_bonus += 24.0;
                    }
                }
                if GIVEN_LEFT_INTROS.contains(&curr_word)
                    && dag.get(&end_idx).is_some_and(|ns| {
                        ns.iter().any(|m| reg.given_names.contains(m.word.as_str()))
                    })
                {
                    reg_bonus += 14.0;
                }
                if reg.given_names.contains(prev_word) && GIVEN_RIGHT_VERBS.contains(&curr_word) {
                    reg_bonus += 16.0;
                }

                // 完整姓名強化（註冊過之全名本身，如「曹大十」）
                if reg.full_names.contains(curr_word) {
                    reg_bonus += 24.0;
                }

                bonus += reg_bonus.clamp(-35.0, 35.0);
                }
            }
        }
    }

    // Step S: 位於 S 節點 (curr_word 為 1 字姓，預看右側 G1 與 G2)
    if let Some(s_ch) = {
        let mut it = curr_word.chars();
        match (it.next(), it.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    } {
        if is_tier_a_surname(s_ch) {
            let n_syllables = get_n_syllables();
            let s_idx = end_idx.saturating_sub(1);
            let bracketed_s =
                is_bracketed_appositive_name_span(s_idx, prefix_words, dag, n_syllables);
            let (in_roster_s, _roster_f_s, _roster_m_s) =
                roster_context_info(prefix_words, s_idx, s_idx + 3, dag, n_syllables);
            let has_intro_s = prefix_words.last().map_or(false, |pw| {
                NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w))
            });
            let sg1_limit_s: u32 = if bracketed_s || in_roster_s { 340 } else { 220 };
            let s_idx_clean = if backward.is_some_and(|bw| {
                bw.n_syllables > 0 && !bw.max_len2_weight_at.is_empty()
            }) {
                let bw = backward.unwrap();
                bw.max_len3_weight_at.get(s_idx).copied().unwrap_or(0) < 180
                    && bw.max_len2_weight_at.get(s_idx).copied().unwrap_or(0) < sg1_limit_s
            } else {
                dag.get(&s_idx).map_or(true, |ns| {
                    !ns.iter().any(|n| {
                        let c = n.word.chars().count();
                        (c >= 3 && n.weight >= 180) || (c == 2 && n.weight >= sg1_limit_s)
                    })
                })
            };
            if (in_roster_s || is_onomastic_context_admissible(s_idx, prefix_words, dag, n_syllables))
                && s_idx_clean
            {
                if let Some(g1_nodes) = dag.get(&end_idx) {
                    let mut found_good_name = false;
                    let mut found_both_reg_name = false;
                    if !session_given_names.is_empty()
                        || active_ctx.is_some_and(|ctx| ctx.coref_len > 0)
                    {
                        for n_g in g1_nodes {
                            let mut git = n_g.word.chars();
                            if let (Some(g1_ch), Some(g2_ch), None) =
                                (git.next(), git.next(), git.next())
                            {
                                let in_session_or_coref =
                                    session_given_names.contains(n_g.word.as_str())
                                        || active_ctx.is_some_and(|ctx| {
                                            ctx.coref_names[..ctx.coref_len]
                                                .contains(&(g1_ch, g2_ch))
                                        });
                                if in_session_or_coref
                                    && !is_non_given_name_char(g1_ch)
                                    && !is_non_given_name_char(g2_ch)
                                {
                                    found_good_name = true;
                                    found_both_reg_name = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !found_good_name
                        && passes_lexical_veto_with_span(
                            s_idx,
                            end_idx,
                            end_idx + 2,
                            dag,
                            n_syllables,
                            backward,
                            bracketed_s || in_roster_s,
                        )
                    {
                        let has_anchor_s =
                            has_intro_s || has_explicit_honorific_or_speech_at(dag, s_idx + 3);
                        let orphan_subj_s = !has_anchor_s
                            && is_orphan_clause_subject_anchor(s_idx, prefix_words, dag);
                        let clean_punct_gen_s = is_clean_punct_bounded_gen_name(
                            s_ch,
                            s_idx,
                            prefix_words,
                            dag,
                            n_syllables,
                        );
                        for n1 in g1_nodes {
                            if n1.word.chars().count() == 1 {
                                let g1_ch = n1.word.chars().next().unwrap();
                                if !is_non_given_name_char(g1_ch)
                                    && (is_naming_register_char(g1_ch)
                                        || ((has_anchor_s
                                            || in_roster_s
                                            || orphan_subj_s
                                            || clean_punct_gen_s)
                                            && is_generational_given_char(g1_ch)))
                                {
                                    let g1_in_reg = is_naming_register_char(g1_ch);
                                    let g1_in_gen = is_generational_given_char(g1_ch);
                                    let g1_end = end_idx + 1;
                                    if let Some(g2_nodes) = dag.get(&g1_end) {
                                        for n2 in g2_nodes {
                                            if n2.word.chars().count() == 1 {
                                                let g2_ch = n2.word.chars().next().unwrap();
                                                if !is_non_given_name_char(g2_ch) {
                                                    if let Some((score, tier)) =
                                                        evaluate_single_surname_double_name(
                                                            s_ch, g1_ch, g2_ch,
                                                        )
                                                    {
                                                        let g2_in_reg =
                                                            is_naming_register_char(g2_ch);
                                                        let gen_reg_s = (has_anchor_s
                                                            || in_roster_s
                                                            || clean_punct_gen_s
                                                            || (orphan_subj_s
                                                                && tier >= 1
                                                                && score >= 8.0))
                                                            && ((g1_in_gen && g2_in_reg)
                                                                || (g1_in_reg
                                                                    && is_generational_given_char(
                                                                        g2_ch,
                                                                    )));
                                                        if (g1_in_reg && g2_in_reg) || gen_reg_s {
                                                            found_good_name = true;
                                                            found_both_reg_name = true;
                                                            break;
                                                        } else if g1_in_reg
                                                            && tier >= 1
                                                            && score > 0.0
                                                            && n1.weight >= 380
                                                            && (g2_in_reg || n2.weight >= 380)
                                                        {
                                                            found_good_name = true;
                                                            if g2_in_reg {
                                                                found_both_reg_name = true;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if found_both_reg_name {
                                break;
                            }
                        }
                    }
                    if found_good_name {
                        let (mut demo_s, _) = surname_demographic_prior(s_ch);
                        if is_user_registered_surname(s_ch) {
                            demo_s = demo_s.max(0.0) + 2.0;
                        }
                        let coref_s = if has_intro_s
                            && active_ctx.is_some_and(|ctx| {
                                ctx.coref_surnames[..ctx.coref_surnames_len].contains(&s_ch)
                            }) {
                            2.5
                        } else {
                            0.0
                        };
                        let intro_reg_s = if has_intro_s && found_both_reg_name {
                            2.0
                        } else {
                            0.0
                        };
                        let roster_s_bonus = if in_roster_s { 2.0 } else { 0.0 };
                        bonus += 6.0 + intro_reg_s + demo_s + coref_s + roster_s_bonus;
                    }
                }
            }
        }
    }

    // Step A: 位於 G1 節點 (curr_word 為 1 字名首，prev_word 為 1 字姓)
    if let (Some(s_ch), Some(g1_ch)) = (
        {
            let mut it = prev_word.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        },
        {
            let mut it = curr_word.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        },
    ) {
        if is_tier_a_surname(s_ch)
            && !is_non_given_name_char(g1_ch)
            && (is_naming_register_char(g1_ch) || is_generational_given_char(g1_ch))
        {
            let n_syllables = get_n_syllables();
            let s_idx = end_idx.saturating_sub(2);
            let p_before_s = if prefix_words.len() >= 1 {
                &prefix_words[..prefix_words.len() - 1]
            } else {
                &[]
            };
            let bracketed_a =
                is_bracketed_appositive_name_span(s_idx, p_before_s, dag, n_syllables);
            let (in_roster_a, roster_f_a, roster_m_a) =
                roster_context_info(p_before_s, s_idx, s_idx + 3, dag, n_syllables);
            let has_intro_a = p_before_s.last().map_or(false, |pw| {
                NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w))
            });
            let has_anchor_a =
                has_intro_a || has_explicit_honorific_or_speech_at(dag, s_idx + 3);
            let orphan_subj_a =
                !has_anchor_a && is_orphan_clause_subject_anchor(s_idx, p_before_s, dag);
            let clean_punct_gen_a =
                is_clean_punct_bounded_gen_name(s_ch, s_idx, p_before_s, dag, n_syllables);
            let g1_in_reg = is_naming_register_char(g1_ch);
            let g1_in_gen = is_generational_given_char(g1_ch);
            let male_intro_a = roster_m_a
                || p_before_s.last().map_or(false, |pw| {
                    MALE_KINSHIP_INTROS.iter().any(|&w| pw.ends_with(w))
                });
            let female_intro_a = !male_intro_a
                && (roster_f_a
                    || p_before_s.last().map_or(false, |pw| {
                        FEMALE_KINSHIP_INTROS.iter().any(|&w| pw.ends_with(w))
                    }));
            if (g1_in_reg || has_anchor_a || in_roster_a || orphan_subj_a || clean_punct_gen_a)
                && (in_roster_a
                    || is_onomastic_context_admissible(s_idx, p_before_s, dag, n_syllables))
                && passes_lexical_veto_with_span(
                    s_idx,
                    end_idx.saturating_sub(1),
                    end_idx + 1,
                    dag,
                    n_syllables,
                    backward,
                    bracketed_a || in_roster_a,
                )
            {
                if let Some(next_nodes) = dag.get(&end_idx) {
                    let mut best_step_a = 0.0;
                    for node in next_nodes {
                        if node.word.chars().count() == 1 {
                            let g2_ch = node.word.chars().next().unwrap();
                            if !is_non_given_name_char(g2_ch) {
                                if let Some((score, tier)) =
                                    evaluate_single_surname_double_name(s_ch, g1_ch, g2_ch)
                                {
                                    let g2_in_reg = is_naming_register_char(g2_ch);
                                    let gen_reg_a = (has_anchor_a
                                        || in_roster_a
                                        || clean_punct_gen_a
                                        || (orphan_subj_a && tier >= 1 && score >= 8.0))
                                        && ((g1_in_gen && g2_in_reg)
                                            || (g1_in_reg && is_generational_given_char(g2_ch)));
                                    if (g1_in_reg && g2_in_reg) || gen_reg_a {
                                        let core_a = (if is_core_naming_register_char(g1_ch) { 0.4 } else { 0.0 })
                                            + (if is_core_naming_register_char(g2_ch)
                                                && (g1_in_reg || score >= 0.0)
                                            {
                                                0.3
                                            } else {
                                                0.0
                                            })
                                            - (if g2_ch == '語' { 0.8 } else { 0.0 });
                                        let gender_pen_a = if (male_intro_a
                                            && (is_female_only_given_char(g1_ch)
                                                || is_female_only_given_char(g2_ch)))
                                            || (female_intro_a
                                                && (is_male_only_given_char(g1_ch)
                                                    || is_male_only_given_char(g2_ch)))
                                        {
                                            -3.5
                                        } else {
                                            0.0
                                        };
                                        let tier_adj_a = if gen_reg_a && tier >= 1 { 1.5 } else { 0.0 };
                                        let a_bonus = 9.5
                                            + if has_intro_a { 2.5 } else { 0.0 }
                                            + core_a
                                            + gender_pen_a
                                            + tier_adj_a
                                            + (score * 0.04).clamp(-1.0, 1.2);
                                        if a_bonus > best_step_a {
                                            best_step_a = a_bonus;
                                        }
                                    } else if g1_in_reg
                                        && tier >= 1
                                        && score > 0.0
                                        && (g2_in_reg || node.weight >= 380)
                                    {
                                        let a_bonus = if tier >= 2 { 8.0 } else { 5.0 } + 3.5;
                                        if a_bonus > best_step_a {
                                            best_step_a = a_bonus;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    bonus += best_step_a;
                }
            }
        }
    }

    // Step C: 台灣傳統冠夫姓／雙姓「阿X」人名構式 ([S1][S2]["阿"][G] 如 潘李阿玉, [S]["阿"][G] 如 潘阿玉)
    let mut matched_ah = false;
    let is_ah_candidate = (curr_char_len == 1
        && (curr_word == "阿"
            || is_tier_a_surname(curr_c0)
            || (prev_word == "阿"
                && is_naming_register_char(curr_c0)
                && !is_non_given_name_char(curr_c0))))
        || (curr_char_len == 2
            && curr_c0 == '阿'
            && is_naming_register_char(curr_c1)
            && !is_non_given_name_char(curr_c1));
    if is_ah_candidate {
        let is_single_surname_w = |w: &str| -> bool {
            let mut it = w.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => is_tier_a_surname(c),
                _ => false,
            }
        };
        let is_single_ah_given_w = |w: &str| -> bool {
            let mut it = w.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => is_naming_register_char(c) && !is_non_given_name_char(c),
                _ => false,
            }
        };
        let is_ah_given_pair = |w: &str| -> bool {
            let mut it = w.chars();
            match (it.next(), it.next(), it.next()) {
                (Some('阿'), Some(g), None) => {
                    is_naming_register_char(g) && !is_non_given_name_char(g)
                }
                _ => false,
            }
        };
        let span_clean_220 = |start: usize, len: usize| -> bool {
            let left_clean = start == 0
                || !dag.get(&(start - 1)).is_some_and(|ns| {
                    ns.iter().any(|m| m.end_idx > start && m.weight >= 220)
                });
            left_clean
                && (start..start + len).all(|pos| {
                    !dag.get(&pos).is_some_and(|ns| {
                        ns.iter().any(|m| {
                            m.length() >= 2
                                && m.weight >= 220
                                && !(pos == start + len - 2
                                    && m.length() == 2
                                    && is_ah_given_pair(&m.word))
                        })
                    })
                })
        };
        let ah_ctx_ok = |s_start: usize, span_len: usize, p_before: &[&str], n_syl: usize| -> bool {
            roster_context_info(p_before, s_start, s_start + span_len, dag, n_syl).0
                || p_before
                    .last()
                    .is_some_and(|pw| NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w)))
                || has_explicit_honorific_or_speech_at(dag, s_start + span_len)
                || (s_start == 0 && n_syl == span_len)
        };

        if curr_char_len == 2 {
            // 2-char ephemeral/session ["阿"G]
            if end_idx >= 4
                && n_p >= 2
                && is_single_surname_w(prev_word)
                && is_single_surname_w(prev2_word)
            {
                let s_start = end_idx - 4;
                if span_clean_220(s_start, 4)
                    && ah_ctx_ok(s_start, 4, &prefix_words[..n_p - 2], get_n_syllables())
                {
                    bonus += 26.0;
                    matched_ah = true;
                }
            } else if end_idx >= 3
                && n_p >= 1
                && is_single_surname_w(prev_word)
                && !is_single_surname_w(prev2_word)
            {
                let s_start = end_idx - 3;
                if span_clean_220(s_start, 3)
                    && ah_ctx_ok(s_start, 3, &prefix_words[..n_p - 1], get_n_syllables())
                {
                    bonus += 24.0;
                    matched_ah = true;
                }
            }
        } else {
        let mut matched_4char_ah = false;

        // 4-char: [S1][S2]["阿"][G] (or [S1][S2]["阿G"])
        if end_idx >= 1 && is_tier_a_surname(curr_c0) {
            let s_start = end_idx - 1;
            let has_4 = dag.get(&end_idx).is_some_and(|ns1| {
                ns1.iter().any(|n1| is_single_surname_w(&n1.word))
            }) && dag.get(&(end_idx + 1)).is_some_and(|ns2| {
                ns2.iter().any(|n2| {
                    is_ah_given_pair(&n2.word)
                        || (n2.word == "阿"
                            && dag.get(&(end_idx + 2)).is_some_and(|ns3| {
                                ns3.iter().any(|n3| is_single_ah_given_w(&n3.word))
                            }))
                })
            });
            if has_4
                && span_clean_220(s_start, 4)
                && ah_ctx_ok(s_start, 4, prefix_words, get_n_syllables())
            {
                bonus += 9.0;
                matched_4char_ah = true;
                matched_ah = true;
            }
        }
        if !matched_4char_ah
            && end_idx >= 2
            && n_p >= 1
            && is_tier_a_surname(curr_c0)
            && is_single_surname_w(prev_word)
        {
            let s_start = end_idx - 2;
            let has_4 = dag.get(&end_idx).is_some_and(|ns2| {
                ns2.iter().any(|n2| {
                    is_ah_given_pair(&n2.word)
                        || (n2.word == "阿"
                            && dag.get(&(end_idx + 1)).is_some_and(|ns3| {
                                ns3.iter().any(|n3| is_single_ah_given_w(&n3.word))
                            }))
                })
            });
            if has_4
                && span_clean_220(s_start, 4)
                && ah_ctx_ok(s_start, 4, &prefix_words[..n_p - 1], get_n_syllables())
            {
                bonus += 22.0;
                matched_4char_ah = true;
                matched_ah = true;
            }
        }
        if !matched_4char_ah
            && end_idx >= 3
            && n_p >= 2
            && curr_word == "阿"
            && is_single_surname_w(prev_word)
            && is_single_surname_w(prev2_word)
        {
            let s_start = end_idx - 3;
            let has_4 = dag
                .get(&end_idx)
                .is_some_and(|ns3| ns3.iter().any(|n3| is_single_ah_given_w(&n3.word)));
            if has_4
                && span_clean_220(s_start, 4)
                && ah_ctx_ok(s_start, 4, &prefix_words[..n_p - 2], get_n_syllables())
            {
                bonus += 16.0;
                matched_4char_ah = true;
                matched_ah = true;
            }
        }
        if !matched_4char_ah
            && end_idx >= 4
            && n_p >= 3
            && is_naming_register_char(curr_c0)
            && !is_non_given_name_char(curr_c0)
            && prev_word == "阿"
            && is_single_surname_w(prev2_word)
            && is_single_surname_w(prefix_words[n_p - 3])
        {
            let s_start = end_idx - 4;
            if span_clean_220(s_start, 4)
                && ah_ctx_ok(s_start, 4, &prefix_words[..n_p - 3], get_n_syllables())
            {
                bonus += 18.0;
                matched_4char_ah = true;
                matched_ah = true;
            }
        }

        // 3-char: [S]["阿"][G] (僅在未命中 4 字雙姓冠夫姓構式時啟用)
        if !matched_4char_ah {
            if end_idx >= 1 && is_tier_a_surname(curr_c0) && !is_single_surname_w(prev_word) {
                let s_start = end_idx - 1;
                let has_3 = dag.get(&end_idx).is_some_and(|ns1| {
                    ns1.iter().any(|n1| {
                        is_ah_given_pair(&n1.word)
                            || (n1.word == "阿"
                                && dag.get(&(end_idx + 1)).is_some_and(|ns2| {
                                    ns2.iter().any(|n2| is_single_ah_given_w(&n2.word))
                                }))
                    })
                });
                if has_3
                    && span_clean_220(s_start, 3)
                    && ah_ctx_ok(s_start, 3, prefix_words, get_n_syllables())
                {
                    bonus += 8.0;
                    matched_ah = true;
                }
            } else if end_idx >= 2
                && n_p >= 1
                && curr_word == "阿"
                && is_single_surname_w(prev_word)
                && !is_single_surname_w(prev2_word)
            {
                let s_start = end_idx - 2;
                let has_3 = dag
                    .get(&end_idx)
                    .is_some_and(|ns2| ns2.iter().any(|n2| is_single_ah_given_w(&n2.word)));
                if has_3
                    && span_clean_220(s_start, 3)
                    && ah_ctx_ok(s_start, 3, &prefix_words[..n_p - 1], get_n_syllables())
                {
                    bonus += 14.0;
                    matched_ah = true;
                }
            } else if end_idx >= 3
                && n_p >= 2
                && is_naming_register_char(curr_c0)
                && !is_non_given_name_char(curr_c0)
                && prev_word == "阿"
                && is_single_surname_w(prev2_word)
                && !prefix_words
                    .get(n_p.wrapping_sub(3))
                    .is_some_and(|&w| is_single_surname_w(w))
            {
                let s_start = end_idx - 3;
                if span_clean_220(s_start, 3)
                    && ah_ctx_ok(s_start, 3, &prefix_words[..n_p - 2], get_n_syllables())
                {
                    bonus += 16.0;
                    matched_ah = true;
                }
            }
        }
        }
    }

    // Step B: 位於 G2 節點 (curr_word 為 G2, prev_word 為 G1, prev2_word 為 S)
    if !matched_ah {
    if let (Some(s_ch), Some(g1_ch), Some(g2_ch)) = (
        {
            let mut it = prev2_word.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        },
        {
            let mut it = prev_word.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        },
        {
            let mut it = curr_word.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        },
    ) {
        if is_tier_a_surname(s_ch)
            && !is_non_given_name_char(g1_ch)
            && !is_non_given_name_char(g2_ch)
        {
            let n_syllables = get_n_syllables();
            let s_idx = end_idx.saturating_sub(3);
            let g1_idx = end_idx.saturating_sub(2);
            let p_before_s = if prefix_words.len() >= 2 {
                &prefix_words[..prefix_words.len() - 2]
            } else {
                &[]
            };
            let bracketed_b =
                is_bracketed_appositive_name_span(s_idx, p_before_s, dag, n_syllables);
            let (in_roster_b, roster_f_b, roster_m_b) =
                roster_context_info(p_before_s, s_idx, end_idx, dag, n_syllables);
            if (in_roster_b || is_onomastic_context_admissible(s_idx, p_before_s, dag, n_syllables))
                && passes_lexical_veto_with_span(
                    s_idx,
                    g1_idx,
                    end_idx,
                    dag,
                    n_syllables,
                    backward,
                    bracketed_b || in_roster_b,
                )
            {
                if let Some((score, tier)) = evaluate_single_surname_double_name(s_ch, g1_ch, g2_ch) {
                    let g1_in_reg = is_naming_register_char(g1_ch);
                    let g2_in_reg = is_naming_register_char(g2_ch);
                    let has_intro = p_before_s.last().map_or(false, |pw| {
                        NAME_INTRO_WORDS.iter().any(|&w| pw.ends_with(w))
                    });
                    let has_right = end_idx == n_syllables
                        || dag.get(&end_idx).is_some_and(|next_nodes| {
                            next_nodes.iter().any(|node| {
                                NAME_RIGHT_CONTEXTS.iter().any(|&r| node.word.starts_with(r))
                                    || matches!(
                                        node.word.as_str(),
                                        "," | "." | "!" | "?" | ";" | ":"
                                    )
                            })
                        });
                    let (_, mut demo_b) = surname_demographic_prior(s_ch);
                    if is_user_registered_surname(s_ch) {
                        demo_b = demo_b.max(0.0) + 2.0;
                    }
                    let coref_b = if has_intro
                        && active_ctx.is_some_and(|ctx| {
                            ctx.coref_surnames[..ctx.coref_surnames_len].contains(&s_ch)
                        }) {
                        3.0
                    } else {
                        0.0
                    };
                    let has_anchor_b =
                        has_intro || has_explicit_honorific_or_speech_at(dag, end_idx);
                    let orphan_subj_b =
                        !has_anchor_b && is_orphan_clause_subject_anchor(s_idx, p_before_s, dag);
                    let clean_punct_gen_b =
                        is_clean_punct_bounded_gen_name(s_ch, s_idx, p_before_s, dag, n_syllables);
                    let gen_reg_b = (has_anchor_b
                        || in_roster_b
                        || clean_punct_gen_b
                        || (orphan_subj_b && tier >= 1 && score >= 8.0))
                        && ((is_generational_given_char(g1_ch) && g2_in_reg)
                            || (g1_in_reg && is_generational_given_char(g2_ch)));

                    if (g1_in_reg && g2_in_reg) || gen_reg_b {
                        let male_intro_b = roster_m_b
                            || p_before_s.last().map_or(false, |pw| {
                                MALE_KINSHIP_INTROS.iter().any(|&w| pw.ends_with(w))
                            });
                        let female_intro_b = !male_intro_b
                            && (roster_f_b
                                || p_before_s.last().map_or(false, |pw| {
                                    FEMALE_KINSHIP_INTROS.iter().any(|&w| pw.ends_with(w))
                                }));
                        let mut name_bonus = 11.0 + demo_b + coref_b;
                        if is_core_naming_register_char(g1_ch) {
                            name_bonus += 0.6;
                        }
                        if is_core_naming_register_char(g2_ch) && (g1_in_reg || score >= 0.0) {
                            name_bonus += 0.6;
                        }
                        if g2_ch == '語' {
                            name_bonus -= 0.8;
                        }
                        if in_roster_b {
                            name_bonus += 3.0;
                        }
                        if tier >= 2 {
                            name_bonus += 2.0;
                        } else if gen_reg_b && tier == 1 {
                            name_bonus += 1.5;
                        }
                        if orphan_subj_b && tier >= 1 {
                            name_bonus += 2.5;
                        }
                        if (male_intro_b
                            && (is_female_only_given_char(g1_ch)
                                || is_female_only_given_char(g2_ch)))
                            || (female_intro_b
                                && (is_male_only_given_char(g1_ch)
                                    || is_male_only_given_char(g2_ch)))
                        {
                            name_bonus -= 4.5;
                        }
                        if has_intro {
                            name_bonus += 4.0;
                        }
                        if has_right {
                            name_bonus += 3.0;
                        }
                        if s_idx == 0 && end_idx == n_syllables {
                            name_bonus += 4.0;
                        }
                        let score_term = (score * 0.06).clamp(-1.2, 1.8);
                        bonus += (name_bonus + score_term).clamp(-25.0, 28.0);
                    } else {
                        let g1_wt_ok = g1_in_reg
                            || dag.get(&g1_idx).is_some_and(|ns| {
                                ns.iter().any(|m| m.word == prev_word && m.weight >= 380)
                            });
                        let g2_wt_ok = g2_in_reg
                            || dag.get(&(end_idx - 1)).is_some_and(|ns| {
                                ns.iter().any(|m| m.word == curr_word && m.weight >= 380)
                            });
                        if !g1_wt_ok || !g2_wt_ok {
                            bonus -= 12.0;
                        } else {
                            let mut name_bonus = 0.0;
                            if tier < 0 {
                                if (g1_in_reg || g2_in_reg)
                                    && (has_anchor_b || in_roster_b)
                                    && score > -10.0
                                {
                                    name_bonus += 1.0;
                                } else {
                                    name_bonus -= 16.0;
                                }
                            } else if g1_in_reg || g2_in_reg {
                                if tier >= 2 {
                                    name_bonus += 3.0;
                                } else if tier >= 1 {
                                    name_bonus += 1.5;
                                }
                                name_bonus += demo_b * 0.5;
                            } else {
                                name_bonus -= 10.0;
                            }

                            if tier >= 0
                                || ((g1_in_reg || g2_in_reg)
                                    && (has_anchor_b || in_roster_b)
                                    && score > -10.0)
                            {
                                if has_intro {
                                    name_bonus += 4.0;
                                }
                                if has_right {
                                    name_bonus += 3.0;
                                }
                                if in_roster_b {
                                    name_bonus += 2.5;
                                }
                            }

                            let score_term = (score * 0.4).clamp(-6.0, 7.0);
                            bonus += (name_bonus + score_term).clamp(-25.0, 28.0);
                        }
                    }
                }
            }
        }
    }
    }

    bonus.clamp(-35.0, 35.0)
}

/// 判斷給定詞條是否為常見尊稱或職稱（防止誤將「王先生」「林小姐」等『姓氏＋尊稱』登錄為個人名字）
pub fn is_honorific_or_title_suffix(w: &str) -> bool {
    matches!(
        w,
        "先生" | "小姐" | "女士" | "太太" | "夫人" | "博士" | "老師" | "教授" | "主任"
            | "立委" | "委員" | "市長" | "縣長" | "議員" | "經理" | "醫師" | "律師"
            | "院長" | "校長" | "部長" | "局長" | "處長"
            | "總裁" | "教練" | "助教" | "店長" | "同學" | "學長" | "學姊" | "學姐" | "學弟"
            | "學妹"
    ) || w.ends_with("經理")
        || w.ends_with("事長")
        || w.ends_with("醫師")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kangxi_strokes_lookup() {
        assert_eq!(kangxi_strokes('吳'), Some(7));
        assert_eq!(kangxi_strokes('玟'), Some(9));
        assert_eq!(kangxi_strokes('蓉'), Some(16));
        assert_eq!(kangxi_strokes('無'), Some(12));
        assert_eq!(kangxi_strokes('文'), Some(4));
        assert_eq!(kangxi_strokes('容'), Some(10));
        assert_eq!(kangxi_strokes('曹'), Some(11));
        assert_eq!(kangxi_strokes('大'), Some(3));
        assert_eq!(kangxi_strokes('十'), Some(10));
        assert_eq!(kangxi_strokes('宏'), Some(7));
        assert_eq!(kangxi_strokes('昇'), Some(8));
    }

    #[test]
    fn test_wu_wen_rong_numerology() {
        let (score_good, tier_good) = evaluate_single_surname_double_name('吳', '玟', '蓉').unwrap();
        let (score_homo, tier_homo) = evaluate_single_surname_double_name('吳', '文', '容').unwrap();
        let (score_homo2, tier_homo2) = evaluate_single_surname_double_name('吳', '文', '榮').unwrap();
        assert!(score_good > score_homo);
        assert!(score_good > score_homo2);
        assert!(tier_good >= 1);
        assert!(tier_homo <= 0);
        assert!(tier_homo2 <= 0);
    }

    #[test]
    fn test_wu_wen_rong_bonus() {
        let dag = std::collections::HashMap::new();
        let session = std::collections::HashSet::new();
        let bonus_min = score_onomastic_bonus(&["吳", "玟"], "蓉", "", 3, &dag, &session);
        let bonus_wen = score_onomastic_bonus(&["吳", "炆"], "蓉", "", 3, &dag, &session);
        println!("bonus_min: {}, bonus_wen: {}", bonus_min, bonus_wen);
        assert!(bonus_min > bonus_wen, "玟 should beat 炆: min={}, wen={}", bonus_min, bonus_wen);
    }
}
