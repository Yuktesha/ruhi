// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/router/discourse_reranker.rs — 純 Rust 雙向代詞與有生性語篇重排序器 (`v0.9` Stage B1)
//
// 針對第三人稱代詞（「他／她／它／牠」及其複數「他們／她們／它們／牠們」）在同音字詞網格中之長距消歧需求，
// 掃描 N-Best 候選路徑之整句雙向線索（同句指代一致性、配偶方向不對稱防護、動物詞與量詞、
// 左側先行詞／無生屬性與局部受事／主事謂語、人類行為與認知謂語），
// 嚴格限定調整幅值於 `[-6.0, +6.0]`，零 COM ABI 變動且零外部推論依賴。

use std::collections::HashMap;

use super::lattice::LatticeNode;

/// N-Best 相對最高分容許邊際（超過此差距的候選路徑不參與語篇翻盤）
pub const RERANK_SCORE_MARGIN: f64 = 12.0;
/// 單句語篇加權絕對上限
const DISCOURSE_DELTA_CAP: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PronounKind {
    He,
    She,
    It,
    Animal,
}

#[inline]
fn classify_pronoun(word: &str) -> Option<PronounKind> {
    match word {
        "他" | "他們" => Some(PronounKind::He),
        "她" | "她們" => Some(PronounKind::She),
        "它" | "它們" => Some(PronounKind::It),
        "牠" | "牠們" => Some(PronounKind::Animal),
        _ => None,
    }
}

/// 快速檢查詞網格是否包含第三人稱代詞（「他／她／它／牠」或其複數）
#[inline]
pub fn dag_has_third_person_pronoun(dag: &HashMap<usize, Vec<LatticeNode>>) -> bool {
    dag.values()
        .any(|nodes| nodes.iter().any(|n| classify_pronoun(&n.word).is_some()))
}

const MIXED_PERSON_CUES: &[&str] = &[
    "男女", "夫婦", "夫妻", "情侶", "兄妹", "姐弟", "姊弟", "父母", "爸媽", "兩人", "兩位", "雙方",
    "彼此", "互相",
];

/// 一般女性稱謂／角色詞（不含配偶名詞，避免「養育妻子／告訴其妻子」誤判主詞為陰性）
const FEMALE_GENERAL_CUES: &[&str] = &[
    "媽媽", "母親", "小姐", "女士", "女孩", "女生", "女子", "女人", "女兒", "姐姐", "姊姊",
    "妹妹", "奶奶", "外婆", "阿姨", "姑姑", "新娘", "公主", "女王", "女神", "女星", "少女",
    "孕婦", "姑娘", "閨蜜", "母后", "婆婆", "媳婦", "嫂子", "大嫂",
];

/// 配偶類女性名詞：僅於代詞左側出現或為代詞繫詞賓語時方指向陰性主體
const FEMALE_SPOUSE_CUES: &[&str] = &[
    "妻子", "太太", "老婆", "女友", "女朋友", "未婚妻",
];

const MALE_CUES: &[&str] = &[
    "先生", "男人", "男孩", "男生", "男子", "父親", "爸爸", "兒子", "丈夫", "老公", "新郎",
    "國王", "王子", "男星", "男友", "男朋友", "未婚夫", "紳士",
];

const ANIMAL_SUBSTR_CUES: &[&str] = &[
    "小狗", "小貓", "貓咪", "小花貓", "長尾貓", "兔子", "老虎", "獅子", "大象", "猴子", "烏龜",
    "青蛙", "蝴蝶", "蜜蜂", "寵物", "動物", "毛小孩", "鯨魚", "海豚", "企鵝", "恐龍", "老鷹",
    "麻雀", "鴿子", "公雞", "母雞", "山羊", "綿羊", "野狼", "狐狸", "松鼠", "刺蝟", "倉鼠",
    "鸚鵡", "金魚", "鯉魚", "鱷魚", "袋鼠", "熊貓", "小鳥", "家畜", "牲畜", "禽獸",
];

const ANIMAL_EXACT_TOKENS: &[&str] = &[
    "狗", "貓", "鳥", "豬", "羊", "牛", "馬", "雞", "鴨", "鵝", "魚", "蛇", "狼", "熊", "鹿", "虎",
    "獅", "象", "猴", "兔", "鼠", "龜", "蛙", "蜂", "蝶", "鷹", "鶴",
];

const ANIMAL_CLASSIFIERS: &[&str] = &["隻", "匹", "頭"];

/// 嚴格屬於無生事物或抽象概念之領屬本質屬性（排除人物共有之「優點／缺點」）
const INANIMATE_ATTR_AFTER_DE: &[&str] = &[
    "意思", "意義", "本質", "作用", "效果", "用途", "功能", "原理", "結構", "形式", "版本",
    "顏色", "形狀", "大小", "重量", "體積", "輪廓", "反作用力", "嚴重性", "對象", "成分",
    "性質", "特性", "特徵", "本質上",
];

const HUMAN_ATTR_AFTER_DE: &[&str] = &[
    "眼睛", "眼神", "身體", "手", "臉", "嘴", "心情", "憤怒", "脾氣", "個性", "性格", "名字",
    "姓名", "工作", "生活", "妻", "妻子", "老婆", "丈夫", "老公", "孩子", "父親", "母親", "爸爸",
    "媽媽", "兒子", "女兒", "老師", "學生", "朋友", "健康", "意見", "想法", "作品", "優點", "缺點",
];

const AUX_ADVERB_SKIP: &[&str] = &[
    "只", "只是", "僅", "僅僅", "就", "也", "都", "還", "仍", "將", "終將", "已", "已經", "曾",
    "曾經", "正在", "在", "可以", "能", "不能", "無法", "必須", "應該", "勢必", "會", "不會",
    "不", "並", "完全", "正",
];

const COPULA_WORDS: &[&str] = &[
    "是", "就是", "只是", "為", "成為", "將成為", "終將成為", "變成", "並非", "不是", "並不是",
    "是個", "並非是",
];

const HUMAN_ROLE_CUES: &[&str] = &[
    "人", "人類", "知識份子", "知識分子", "科學家", "工程師", "老師", "教師", "醫生", "醫師",
    "教授", "學生", "同學", "朋友", "同事", "老闆", "主管", "員工", "專家", "學者", "作家",
    "作者", "記者", "警察", "法官", "律師", "總統", "部長", "市長", "議員", "顧客", "客人",
    "司機", "選手", "導演", "演員", "歌手", "病人", "患者", "嬰兒", "小嬰兒", "小孩", "孩子",
    "老人", "人物", "天才", "人才", "高手", "英雄", "聖人", "修行人", "僧侶", "死者", "敵人",
    "人選",
];

const INANIMATE_COMPLEMENT_NOUNS: &[&str] = &[
    "世界", "音根", "香蕉皮", "表達", "依據", "子音", "母音", "客體", "物質", "系統", "程式",
    "軟體", "硬體", "工具", "方法", "方案", "計畫", "計劃", "問題", "現象", "概念", "理論",
    "原則", "規則", "制度", "法律", "政策", "模型", "架構", "平台", "產品", "商品", "物品",
    "物件", "東西", "材料", "星球", "宇宙", "本體", "環境", "版本", "形式", "顏色", "結果",
    "原因", "理念", "行為", "影子", "部分", "部份", "植物", "樹木", "花朵", "森林", "海洋",
    "河流", "食物", "書籍", "文章", "檔案", "電腦", "手機", "網站", "網頁", "網路", "機器",
    "電池", "裝置", "設備", "字音", "符號", "詞彙", "字詞", "答案", "結論", "證據", "基礎",
    "條件", "過程", "媒介", "橋樑", "機制",
];

const INANIMATE_PREDICATE_CUES: &[&str] = &[
    "誤導", "分割", "被分割", "分成", "歸類", "發生作用", "無關", "立足", "攝取", "轉變",
    "轉變成", "聯結", "連結", "助益", "運作", "運行", "包含", "含有", "具有", "提供", "支援",
    "採用", "基於", "屬於", "位於", "儲存", "消耗", "發光", "發熱", "融化", "蒸發", "燃燒",
    "損壞", "故障", "上市", "推出", "代表著", "指的", "是指", "表達", "變成一", "隨著",
    "帶領", "產生",
];

const HUMAN_PREDICATE_CUES: &[&str] = &[
    "說", "告訴", "回答", "詢問", "抗議", "逃稅", "控告", "笑", "哭", "喊", "想", "覺得",
    "認為", "相信", "以為", "希望", "決定", "打算", "願意", "喜歡", "愛", "討厭", "恨",
    "害怕", "擔心", "生氣", "開心", "高興", "難過", "傷心", "感動", "驚訝", "後悔", "忘記",
    "記得", "學習", "工作", "上班", "退休", "畢業", "結婚", "離婚", "懷孕", "去世", "過世",
    "死了", "生病", "受傷", "吃飯", "睡覺", "起床", "回家", "出門", "旅遊", "聊天", "道歉",
    "感謝", "原諒", "照顧", "陪伴", "拒絕", "答應", "反對", "批評", "稱讚", "抱怨", "提醒",
    "邀請", "見面", "親自", "測試", "享受", "贖罪", "從事", "受過", "稍等", "心不在焉",
    "看著書", "看書", "啟蒙", "攜帶", "抓住", "平和",
];

/// 有生主體專屬生理／狀態謂語（人類與動物皆可，但無生事物「它」不適用；僅用於阻擋跨句無生先行詞誤導）
const ANIMATE_PREDICATE_CUES: &[&str] = &[
    "在睡", "沒睡", "睡覺", "睡著", "睡了", "睡醒", "醒了", "很餓", "好餓", "吃飽", "肚子餓",
    "曬太陽",
];

const HUMAN_MORTALITY_ANTECEDENTS: &[&str] = &["死了", "去世", "過世", "死者", "遺體"];
const BURIAL_DISPOSAL_PREDS: &[&str] = &["火葬", "火葬場", "埋了"];

const INANIMATE_OBJ_VERBS: &[&str] = &[
    "使用", "安裝", "下載", "上傳", "開啟", "打開", "關閉", "執行", "修改", "編輯", "刪除",
    "複製", "貼上", "儲存", "備份", "更新", "升級", "移除", "設定", "調整", "修復", "修理",
    "購買", "賣掉", "拿起", "放下", "放進", "放入", "裝進", "搬動", "擦拭", "清洗", "閱讀",
    "播放", "列印", "解析", "編譯", "部署", "製造", "生產", "發明", "研發", "打造", "拆解",
    "組裝", "保存", "收藏", "達到", "接受", "仰賴", "回憶起", "給予", "給與", "感覺到",
    "立足在", "加在", "發現", "誤認", "認識",
];

/// 「把／將 + 它 + 處置／釋義謂語」專屬無生受事處置動作
const BA_JIANG_INANIMATE_PREDS: &[&str] = &[
    "解釋", "解視", "打開", "開啟", "放進", "放入", "關閉", "刪除", "保存",
];

const INANIMATE_ANTECEDENTS: &[&str] = &[
    "偈子", "版本", "物體", "行星", "星球", "行動", "反作用力", "軟體", "系統", "程式",
    "工具", "方法", "方案", "問題", "現象", "電腦", "手機", "檔案", "裝置", "機器", "食物",
    "玩具", "文章", "書籍", "宇宙", "本體", "理念", "行為", "知識", "神經纖維", "神經細胞",
    "香蕉皮", "音根", "子音", "客體", "物質",
];

/// 跨句前文補充無生／技術載體先行詞，供跨句「它」指代消歧使用（僅限具體器物／軟體／媒體載體，排除廣泛抽象名詞以免誤傷下句）
const CROSS_SENTENCE_INANIMATE_CUES: &[&str] = &[
    "伺服器", "資料庫", "演算法", "框架", "套件", "模組", "服務", "引擎", "輸入法", "晶片",
    "硬碟", "記憶體", "螢幕", "鍵盤", "滑鼠", "耳機", "相機", "汽車", "車輛", "大樓", "建築",
    "電影", "遊戲", "咖啡", "藥物", "專案", "程式碼", "網站", "網頁", "系統", "軟體", "硬體",
    "工具", "檔案", "裝置", "設備", "機器", "電腦", "手機", "程式", "產品", "文章", "書籍",
    "電池",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefixAntecedent {
    Animal,
    Female,
    Male,
    Inanimate,
}

#[inline]
fn max_end_pos(text: &str, patterns: &[&str], best: &mut Option<usize>) {
    for &pat in patterns {
        if let Some(pos) = text.rfind(pat) {
            let end = pos + pat.len();
            if best.map_or(true, |b| end > b) {
                *best = Some(end);
            }
        }
    }
}

/// 由跨句前文右側最近位置掃描顯性實體先行詞，並防止「其他」或「馬路／牛肉麵」誤觸發
fn detect_latest_prefix_antecedent(prefix_text: &str) -> Option<PrefixAntecedent> {
    if prefix_text.trim().is_empty() {
        return None;
    }
    let mut animal_pos: Option<usize> = None;
    let mut female_pos: Option<usize> = None;
    let mut male_pos: Option<usize> = None;
    let mut inanimate_pos: Option<usize> = None;

    max_end_pos(prefix_text, ANIMAL_SUBSTR_CUES, &mut animal_pos);
    max_end_pos(prefix_text, &["牠"], &mut animal_pos);

    max_end_pos(prefix_text, FEMALE_GENERAL_CUES, &mut female_pos);
    max_end_pos(prefix_text, FEMALE_SPOUSE_CUES, &mut female_pos);
    max_end_pos(prefix_text, &["她"], &mut female_pos);

    max_end_pos(prefix_text, MALE_CUES, &mut male_pos);

    max_end_pos(prefix_text, CROSS_SENTENCE_INANIMATE_CUES, &mut inanimate_pos);
    max_end_pos(prefix_text, &["它"], &mut inanimate_pos);

    let chars: Vec<(usize, char)> = prefix_text.char_indices().collect();
    for i in 0..chars.len() {
        let (byte_idx, ch) = chars[i];
        // 量詞錨定動物名詞檢出（例如「一隻狗」、「一隻小黑貓」、「那匹馬」），避免「馬路／牛肉麵」誤判
        if matches!(
            ch,
            '一' | '二' | '兩' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '這' | '那' | '每'
        ) {
            if let Some(&(_, cls)) = chars.get(i + 1) {
                if matches!(cls, '隻' | '匹' | '頭') {
                    for k in (i + 2)..chars.len().min(i + 6) {
                        let (k_byte, k_ch) = chars[k];
                        if !('\u{4E00}'..='\u{9FFF}').contains(&k_ch) {
                            break;
                        }
                        let mut tok_buf = [0u8; 4];
                        let tok_str: &str = k_ch.encode_utf8(&mut tok_buf);
                        if ANIMAL_EXACT_TOKENS.contains(&tok_str) {
                            let end = k_byte + k_ch.len_utf8();
                            if animal_pos.map_or(true, |b| end > b) {
                                animal_pos = Some(end);
                            }
                            break;
                        }
                    }
                }
            }
        }
        // 獨立「他」先行詞（排除「其他／吉他／他山之石／他人」等複合構詞誤觸發）
        if ch == '他' {
            let prev_c = i.checked_sub(1).map(|j| chars[j].1);
            let next_c = chars.get(i + 1).map(|&(_, c)| c);
            if !matches!(prev_c, Some('其' | '吉' | '利' | '排' | '無'))
                && !matches!(next_c, Some('山' | '鄉' | '日' | '處' | '國' | '人' | '者'))
            {
                let end = byte_idx + ch.len_utf8();
                if male_pos.map_or(true, |b| end > b) {
                    male_pos = Some(end);
                }
            }
        }
    }

    // 同位置平手優先序：Animal > Female > Inanimate > Male
    let mut best: Option<(usize, u8, PrefixAntecedent)> = None;
    for (pos_opt, prio, kind) in [
        (animal_pos, 4u8, PrefixAntecedent::Animal),
        (female_pos, 3u8, PrefixAntecedent::Female),
        (inanimate_pos, 2u8, PrefixAntecedent::Inanimate),
        (male_pos, 1u8, PrefixAntecedent::Male),
    ] {
        if let Some(pos) = pos_opt {
            if best.map_or(true, |(bp, bpr, _)| pos > bp || (pos == bp && prio > bpr)) {
                best = Some((pos, prio, kind));
            }
        }
    }
    best.map(|(_, _, k)| k)
}

fn contains_any_substr(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|&p| text.contains(p))
}

fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|&p| text.starts_with(p))
}

/// 計算單一候選詞路徑之雙向代詞／有生性語篇評分（已限幅於 `[-6.0, +6.0]`）
pub fn score_discourse_sentence(nodes: &[LatticeNode]) -> f64 {
    score_discourse_sentence_with_prefix(nodes, "")
}

/// 結合跨句前文先行詞之雙向代詞／有生性語篇評分（已限幅於 `[-6.0, +6.0]`）
pub fn score_discourse_sentence_with_prefix(nodes: &[LatticeNode], prefix_text: &str) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let prefix_ant = if prefix_text.is_empty() {
        None
    } else {
        detect_latest_prefix_antecedent(prefix_text)
    };

    let mut pron_indices: Vec<(usize, PronounKind)> = Vec::new();
    let mut surface = String::new();
    for (idx, n) in nodes.iter().enumerate() {
        surface.push_str(&n.word);
        if let Some(pk) = classify_pronoun(&n.word) {
            pron_indices.push((idx, pk));
        }
    }
    if pron_indices.is_empty() {
        return 0.0;
    }

    // 標記受 `[代詞 + 的]` 領屬修飾的緊鄰右側詞位置，避免「他的妻子」誤判代詞本身為陰性
    let mut possessed_slots = vec![false; nodes.len()];
    for &(p_idx, _) in &pron_indices {
        if nodes.get(p_idx + 1).is_some_and(|n| n.word == "的") {
            if p_idx + 2 < nodes.len() {
                possessed_slots[p_idx + 2] = true;
            }
            if p_idx + 3 < nodes.len() && nodes[p_idx + 2].word.chars().count() == 1 {
                possessed_slots[p_idx + 3] = true;
            }
        }
    }

    // 預先識別具獨立無生受事標記之「把／將 + 代詞 + 打開／開啟／關閉／放進…」代詞位置
    let mut is_distinct_ba_jiang_inanimate = vec![false; pron_indices.len()];
    for (k, &(p_idx, _)) in pron_indices.iter().enumerate() {
        if p_idx >= 1 && matches!(nodes[p_idx - 1].word.as_str(), "把" | "將") {
            let mut pred_idx = p_idx + 1;
            let mut skipped = 0usize;
            while pred_idx < nodes.len() && skipped < 2 && AUX_ADVERB_SKIP.contains(&nodes[pred_idx].word.as_str()) {
                pred_idx += 1;
                skipped += 1;
            }
            let rw: String = nodes.iter().skip(pred_idx).take(4).map(|n| n.word.as_str()).collect();
            if starts_with_any(&rw, BA_JIANG_INANIMATE_PREDS) {
                is_distinct_ba_jiang_inanimate[k] = true;
            }
        }
    }

    // 彙整全句非領屬一般女性／男性／動物線索（動物線索記下節點索引，防止「他馬騎得很好」主詞誤判）
    let mut has_general_female = false;
    let mut has_male = false;
    let mut spouse_female_indices: Vec<usize> = Vec::new();
    let mut animal_cue_indices: Vec<usize> = Vec::new();
    for (idx, n) in nodes.iter().enumerate() {
        if possessed_slots[idx] {
            continue;
        }
        let w = n.word.as_str();
        let pair_w = if idx + 1 < nodes.len() && !possessed_slots[idx + 1] {
            Some(format!("{}{}", w, nodes[idx + 1].word))
        } else {
            None
        };
        if contains_any_substr(w, FEMALE_GENERAL_CUES)
            || pair_w.as_deref().is_some_and(|pw| contains_any_substr(pw, FEMALE_GENERAL_CUES))
        {
            has_general_female = true;
        }
        if contains_any_substr(w, FEMALE_SPOUSE_CUES)
            || pair_w.as_deref().is_some_and(|pw| contains_any_substr(pw, FEMALE_SPOUSE_CUES))
        {
            spouse_female_indices.push(idx);
        }
        if contains_any_substr(w, MALE_CUES)
            || pair_w.as_deref().is_some_and(|pw| contains_any_substr(pw, MALE_CUES))
        {
            has_male = true;
        }
        if contains_any_substr(w, ANIMAL_SUBSTR_CUES)
            || ANIMAL_EXACT_TOKENS.contains(&w)
            || pair_w.as_deref().is_some_and(|pw| contains_any_substr(pw, ANIMAL_SUBSTR_CUES))
            || (idx > 0
                && (ANIMAL_CLASSIFIERS.contains(&w) || n.word.ends_with('隻'))
                && contains_any_substr(&surface, ANIMAL_SUBSTR_CUES))
        {
            animal_cue_indices.push(idx);
        }
    }

    let mut total_score = 0.0;

    // 1. 同句可共指代詞指代一致性（排除明顯獨立的「把／將 + 它 + 打開」無生受事）
    let coref_prons: Vec<(usize, PronounKind)> = pron_indices
        .iter()
        .enumerate()
        .filter(|&(k, _)| !is_distinct_ba_jiang_inanimate[k])
        .map(|(_, &p)| p)
        .collect();
    let coref_has_he = coref_prons.iter().any(|&(_, k)| k == PronounKind::He);
    if coref_prons.len() >= 2 && !contains_any_substr(&surface, MIXED_PERSON_CUES) {
        let first_kind = coref_prons[0].1;
        let all_same = coref_prons.iter().all(|&(_, k)| k == first_kind);
        if all_same {
            let bonus = 1.6 * ((coref_prons.len() - 1).min(2) as f64);
            total_score += bonus;
        } else {
            let any_female_cue = has_general_female || !spouse_female_indices.is_empty();
            let has_human = coref_prons
                .iter()
                .any(|&(_, k)| matches!(k, PronounKind::He | PronounKind::She));
            let has_non_human = coref_prons
                .iter()
                .any(|&(_, k)| matches!(k, PronounKind::It | PronounKind::Animal));
            if has_human && has_non_human {
                total_score -= 2.2;
            } else if !any_female_cue {
                total_score -= 2.4;
            } else {
                total_score -= 0.9;
            }
        }
    }

    // 2. 逐一代詞評估雙向局部與句法環境
    for &(p_idx, p_kind) in &pron_indices {
        let mut right_ctx = String::new();
        for n in nodes.iter().skip(p_idx + 1).take(7) {
            if classify_pronoun(&n.word).is_some() {
                break;
            }
            right_ctx.push_str(&n.word);
        }

        // 僅掃描代詞左側 [0..p_idx] 作為先行詞上下文，避免右側賓語名詞反向誤導主詞
        let left_full: String = nodes[..p_idx].iter().map(|n| n.word.as_str()).collect();
        let left_start = p_idx.saturating_sub(6);
        let left_ctx: String = nodes[left_start..p_idx].iter().map(|n| n.word.as_str()).collect();

        let is_de_poss = nodes.get(p_idx + 1).is_some_and(|n| n.word == "的");
        let after_de: String = if is_de_poss {
            nodes
                .iter()
                .skip(p_idx + 2)
                .take(4)
                .map(|n| n.word.as_str())
                .collect()
        } else {
            String::new()
        };

        // 跳過緊鄰代詞後的副詞／助動詞以對齊核心謂語
        let mut pred_idx = p_idx + 1;
        let mut skipped = 0usize;
        while pred_idx < nodes.len() && skipped < 2 && AUX_ADVERB_SKIP.contains(&nodes[pred_idx].word.as_str()) {
            pred_idx += 1;
            skipped += 1;
        }
        let pred_window: String = nodes
            .iter()
            .skip(pred_idx)
            .take(5)
            .map(|n| n.word.as_str())
            .collect();
        let starts_copula = nodes
            .get(pred_idx)
            .is_some_and(|n| COPULA_WORDS.contains(&n.word.as_str()))
            || (pred_idx + 1 < nodes.len()
                && COPULA_WORDS.contains(&format!("{}{}", nodes[pred_idx].word, nodes[pred_idx + 1].word).as_str()));

        let copula_human_np = starts_copula && contains_any_substr(&pred_window, HUMAN_ROLE_CUES);
        let copula_inanimate_np = starts_copula
            && !copula_human_np
            && contains_any_substr(&pred_window, INANIMATE_COMPLEMENT_NOUNS);

        // 配偶名詞不對稱防護：僅當配偶名詞位於 p_idx 左側或為 p_idx 直接繫詞表語時才計入陰性指代
        let has_spouse_female_for_p = spouse_female_indices.iter().any(|&s_idx| s_idx < p_idx)
            || (starts_copula && contains_any_substr(&pred_window, FEMALE_SPOUSE_CUES));
        let has_female = has_general_female || has_spouse_female_for_p;

        // 檢視緊鄰代詞左側之及物／處置／稱謂動詞
        let left1 = if p_idx >= 1 { nodes[p_idx - 1].word.as_str() } else { "" };
        let left2 = if p_idx >= 2 {
            format!("{}{}", nodes[p_idx - 2].word, left1)
        } else {
            String::new()
        };
        let is_unambiguous_call_it = left1 == "叫" && left_ctx.contains("不再叫");
        let is_ba_jiang_inanimate = matches!(left1, "把" | "將") && starts_with_any(&pred_window, BA_JIANG_INANIMATE_PREDS);
        let obj_of_inanimate_verb = !left_ctx.contains("火葬場")
            && (INANIMATE_OBJ_VERBS.contains(&left1)
                || INANIMATE_OBJ_VERBS.contains(&left2.as_str())
                || (p_idx >= 2
                    && nodes[p_idx - 1].word == "起"
                    && nodes[p_idx - 2].word == "回憶")
                || (left1 == "與" && right_ctx.contains("無關")));

        // (A) 動物指代：僅當動物先行詞位於代詞左側或為直接繫詞／動物量詞補語時觸發（防止「他馬騎得很好」主賓混淆）
        let has_animal_for_p = animal_cue_indices.iter().any(|&a_idx| a_idx < p_idx)
            || (starts_copula
                && (contains_any_substr(&pred_window, ANIMAL_SUBSTR_CUES)
                    || ANIMAL_EXACT_TOKENS.iter().any(|&tok| pred_window.contains(tok))));
        if has_animal_for_p && !copula_human_np {
            match p_kind {
                PronounKind::Animal => total_score += 2.2,
                PronounKind::It | PronounKind::He | PronounKind::She => total_score -= 0.7,
            }
            continue;
        }

        // (B) 陰性女性線索
        if has_female && !has_male {
            match p_kind {
                PronounKind::She => total_score += 2.0,
                PronounKind::He => total_score -= 0.8,
                PronounKind::It | PronounKind::Animal => total_score -= 1.0,
            }
        } else if has_male && !has_female {
            match p_kind {
                PronounKind::He => total_score += 1.4,
                PronounKind::She => total_score -= 1.0,
                PronounKind::It | PronounKind::Animal => total_score -= 0.8,
            }
        }

        // (C) 有生人稱 vs 無生事物局部／左側證據整合
        let mut it_evidence = 0.0f64;
        let mut human_evidence = 0.0f64;

        if is_de_poss && contains_any_substr(&after_de, INANIMATE_ATTR_AFTER_DE) {
            it_evidence += 2.2;
        }
        if is_de_poss && contains_any_substr(&after_de, HUMAN_ATTR_AFTER_DE) {
            human_evidence += 2.0;
        }
        if copula_inanimate_np {
            it_evidence += 2.2;
        }
        if copula_human_np {
            human_evidence += 2.4;
        }
        if is_unambiguous_call_it && !has_female {
            it_evidence += 2.0;
        }
        if is_ba_jiang_inanimate {
            it_evidence += 2.2;
        }
        if obj_of_inanimate_verb
            && !has_female
            && !has_male
            && !contains_any_substr(&pred_window, HUMAN_PREDICATE_CUES)
        {
            it_evidence += 1.8;
        }
        if !is_de_poss && contains_any_substr(&pred_window, INANIMATE_PREDICATE_CUES) {
            it_evidence += 1.6;
        }
        if !is_de_poss && contains_any_substr(&pred_window, HUMAN_PREDICATE_CUES) {
            human_evidence += 2.0;
        }
        if contains_any_substr(&left_full, HUMAN_MORTALITY_ANTECEDENTS)
            && contains_any_substr(&pred_window, BURIAL_DISPOSAL_PREDS)
        {
            human_evidence += 2.0;
        }
        let has_left_inanimate_antecedent = contains_any_substr(&left_full, INANIMATE_ANTECEDENTS);
        let is_animate_pred = !is_de_poss
            && (contains_any_substr(&right_ctx, ANIMATE_PREDICATE_CUES)
                || contains_any_substr(&pred_window, ANIMATE_PREDICATE_CUES));
        if has_left_inanimate_antecedent
            && human_evidence < 0.5
            && !copula_human_np
            && !has_female
            && !has_male
        {
            it_evidence += 1.4;
        }

        // 若本句無顯性局部先行詞且有跨句先行詞，補上溫和跨句有生性／性別先驗（不蓋過局部人類、有生或無生謂語）
        if let Some(p_ant) = prefix_ant {
            if !has_animal_for_p && !has_female && !has_male {
                match p_ant {
                    PrefixAntecedent::Animal => {
                        if !copula_human_np && human_evidence < 0.5 && it_evidence < 0.5 {
                            match p_kind {
                                PronounKind::Animal => total_score += 5.0,
                                PronounKind::It | PronounKind::He | PronounKind::She => {
                                    total_score -= 2.5;
                                }
                            }
                            continue;
                        }
                    }
                    PrefixAntecedent::Female => {
                        if !copula_inanimate_np && it_evidence < 0.5 {
                            match p_kind {
                                PronounKind::She => total_score += 2.0,
                                PronounKind::He => total_score -= 0.8,
                                PronounKind::It | PronounKind::Animal => total_score -= 1.0,
                            }
                        }
                    }
                    PrefixAntecedent::Male => {
                        if !copula_inanimate_np && it_evidence < 0.5 {
                            match p_kind {
                                PronounKind::He => total_score += 1.4,
                                PronounKind::She => total_score -= 1.0,
                                PronounKind::It | PronounKind::Animal => total_score -= 0.8,
                            }
                        }
                    }
                    PrefixAntecedent::Inanimate => {
                        if !copula_human_np
                            && human_evidence < 0.5
                            && !has_left_inanimate_antecedent
                            && !is_animate_pred
                            && !is_de_poss
                        {
                            it_evidence += 2.0;
                        }
                    }
                }
            }
        }

        if it_evidence > human_evidence + 0.5 {
            let delta = (it_evidence - human_evidence).min(2.6);
            match p_kind {
                PronounKind::It => total_score += delta,
                PronounKind::He | PronounKind::She | PronounKind::Animal => total_score -= delta * 0.4,
            }
        } else if human_evidence > it_evidence + 0.5 {
            let delta = (human_evidence - it_evidence).min(2.6);
            match p_kind {
                PronounKind::He => total_score += delta,
                PronounKind::She if has_female || (!has_male && !coref_has_he) => {
                    total_score += delta * 0.5
                }
                PronounKind::It | PronounKind::Animal => total_score -= delta * 0.45,
                _ => {}
            }
        }
    }

    total_score.clamp(-DISCOURSE_DELTA_CAP, DISCOURSE_DELTA_CAP)
}

/// 自 Backward-A* Top-N 候選中以雙向語篇評分重排序並回傳勝出路徑與分數
pub fn rerank_nbest(candidates: Vec<(Vec<LatticeNode>, f64)>) -> (Vec<LatticeNode>, f64) {
    rerank_nbest_with_prefix(candidates, "")
}

/// 結合跨句前文線索自 Backward-A* Top-N 候選中執行雙向語篇重排序
pub fn rerank_nbest_with_prefix(
    candidates: Vec<(Vec<LatticeNode>, f64)>,
    prefix_text: &str,
) -> (Vec<LatticeNode>, f64) {
    let Some(first) = candidates.first() else {
        return (Vec::new(), 0.0);
    };
    let top1_score = first.1;
    let mut best_idx = 0usize;
    let mut best_combined = f64::NEG_INFINITY;

    for (idx, (nodes, base_score)) in candidates.iter().enumerate() {
        if top1_score - *base_score > RERANK_SCORE_MARGIN {
            continue;
        }
        let delta = score_discourse_sentence_with_prefix(nodes, prefix_text);
        let combined = *base_score + delta;
        if combined > best_combined {
            best_combined = combined;
            best_idx = idx;
        }
    }

    for (idx, (nodes, base_score)) in candidates.into_iter().enumerate() {
        if idx == best_idx {
            let adjusted = base_score + score_discourse_sentence_with_prefix(&nodes, prefix_text);
            return (nodes, adjusted);
        }
    }
    (Vec::new(), 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(words: &[&str]) -> Vec<LatticeNode> {
        let mut pos = 0usize;
        words
            .iter()
            .map(|&w| {
                let len = w.chars().count().max(1);
                let n = LatticeNode::new(pos, pos + len, w, 500, "");
                pos += len;
                n
            })
            .collect()
    }

    #[test]
    fn test_discourse_reranker_intra_sentence_coreference_and_human_mortality() {
        let good = mk(&["我們", "以為", "他", "死了", "把", "他", "送進", "火葬場", "燒了", "或", "把", "他", "埋了"]);
        let mid = mk(&["我們", "以為", "她", "死了", "把", "他", "送進", "火葬場", "燒了", "或", "把", "他", "埋了"]);
        let bad = mk(&["我們", "以為", "她", "死了", "把", "它", "送進", "火葬場", "燒了", "或", "把", "它", "埋了"]);
        let s_good = score_discourse_sentence(&good);
        let s_mid = score_discourse_sentence(&mid);
        let s_bad = score_discourse_sentence(&bad);
        assert!(
            s_good > s_mid + 2.0 && s_good > s_bad + 2.0,
            "expected homogeneous human coreference > mixed ({} vs mid={}, bad={})",
            s_good,
            s_mid,
            s_bad
        );
    }

    #[test]
    fn test_discourse_reranker_inanimate_vs_human_possessor_protection() {
        let pos_human = mk(&["他", "的", "新", "作品"]);
        let pos_wrong = mk(&["它", "的", "新", "作品"]);
        assert!(
            score_discourse_sentence(&pos_human) > score_discourse_sentence(&pos_wrong),
            "human possessor 他的新作品 must outscore 它的新作品"
        );

        let advantage_he = mk(&["他", "的", "優點", "比", "你們", "多"]);
        let advantage_it = mk(&["它", "的", "優點", "比", "你們", "多"]);
        assert!(
            score_discourse_sentence(&advantage_he) > score_discourse_sentence(&advantage_it),
            "它的優點 must NOT override 他的優點"
        );

        let test_human = mk(&["他", "在", "測試", "新", "功能"]);
        let test_it = mk(&["它", "在", "測試", "新", "功能"]);
        assert!(score_discourse_sentence(&test_human) > score_discourse_sentence(&test_it));

        let it_def = mk(&["它", "的", "意思", "是指", "核心"]);
        let he_def = mk(&["他", "的", "意思", "是指", "核心"]);
        assert!(score_discourse_sentence(&it_def) > score_discourse_sentence(&he_def));
    }

    #[test]
    fn test_discourse_reranker_spouse_asymmetry_and_left_antecedent_protection() {
        // 1. 右側賓語含「妻子」不得將主詞「他」誤改為「她」
        let spouse_obj_he = mk(&["他", "必須", "擔負起", "養育", "妻子", "前夫", "子女", "的", "責任"]);
        let spouse_obj_she = mk(&["她", "必須", "擔負起", "養育", "妻子", "前夫", "子女", "的", "責任"]);
        assert!(
            score_discourse_sentence(&spouse_obj_he) >= score_discourse_sentence(&spouse_obj_she),
            "right spouse object must NOT prefer 她 over 他"
        );

        // 2. 右側受事「神經細胞」不得將「他們」誤改為「它們」
        let neuro_he = mk(&["他們", "都", "仍", "能", "適切", "地", "控制著", "他們", "的", "神經細胞"]);
        let neuro_it = mk(&["它們", "都", "仍", "能", "適切", "地", "控制著", "它們", "的", "神經細胞"]);
        assert!(
            score_discourse_sentence(&neuro_he) >= score_discourse_sentence(&neuro_it),
            "right-side inanimate noun must NOT override 他們"
        );

        // 3. 「人類將它解釋為表現慈悲」與「也就是說它必須有適當的意義」應偏好「它」
        let explain_it = mk(&["人類", "將", "它", "解釋", "為", "表現", "慈悲"]);
        let explain_he = mk(&["人類", "將", "他", "解釋", "為", "表現", "慈悲"]);
        assert!(score_discourse_sentence(&explain_it) > score_discourse_sentence(&explain_he));

        let yjs_it = mk(&["也就是", "說", "它", "必須", "有", "適當", "的", "意義"]);
        let yjs_he = mk(&["也就是", "說", "他", "必須", "有", "適當", "的", "意義"]);
        assert!(score_discourse_sentence(&yjs_it) >= score_discourse_sentence(&yjs_he));

        // 4. 「告訴他...可以把他/它打開」雙指代不同對象豁免
        let open_good = mk(&["並", "告訴", "他", "可以", "把", "它", "打開"]);
        let open_bad = mk(&["並", "告訴", "他", "可以", "把", "他", "打開"]);
        assert!(score_discourse_sentence(&open_good) > score_discourse_sentence(&open_bad));
    }

    #[test]
    fn test_discourse_reranker_female_and_animal_cues() {
        let she_sent = mk(&["妻子", "告訴", "她", "這件事"]);
        let he_sent = mk(&["妻子", "告訴", "他", "這件事"]);
        assert!(score_discourse_sentence(&she_sent) > score_discourse_sentence(&he_sent));

        let animal_ta = mk(&["那隻", "小貓", "走過來", "牠", "很高興"]);
        let animal_he = mk(&["那隻", "小貓", "走過來", "他", "很高興"]);
        assert!(score_discourse_sentence(&animal_ta) > score_discourse_sentence(&animal_he));
    }

    #[test]
    fn test_rerank_respects_margin_and_clamp() {
        let low = mk(&["妻子", "告訴", "她", "這件事"]);
        let high = mk(&["妻子", "告訴", "他", "這件事"]);
        assert!(score_discourse_sentence(&low).abs() <= DISCOURSE_DELTA_CAP);
        let (winner, _) = rerank_nbest(vec![(high.clone(), 0.0), (low.clone(), -15.0)]);
        let w_str: String = winner.iter().map(|n| n.word.as_str()).collect();
        assert_eq!(w_str, "妻子告訴他這件事");
    }

    #[test]
    fn test_cross_sentence_entity_tracking_and_false_positive_guards() {
        // 1. 跨句無生技術載體（「伺服器」 -> 「它」）
        let it_srv = mk(&["它", "現在", "依然", "十分", "可靠"]);
        let he_srv = mk(&["他", "現在", "依然", "十分", "可靠"]);
        assert!(
            score_discourse_sentence_with_prefix(&it_srv, "這台新買的伺服器效能非常穩定。")
                > score_discourse_sentence_with_prefix(&he_srv, "這台新買的伺服器效能非常穩定。"),
            "cross-sentence 伺服器 must favor 它 over 他"
        );

        // 2. 跨句陰性稱謂（「妹妹」 -> 「她」）
        let she_sis = mk(&["她", "帶了", "很多", "伴手禮"]);
        let he_sis = mk(&["他", "帶了", "很多", "伴手禮"]);
        assert!(
            score_discourse_sentence_with_prefix(&she_sis, "我妹妹剛從日本回來。")
                > score_discourse_sentence_with_prefix(&he_sis, "我妹妹剛從日本回來。"),
            "cross-sentence 妹妹 must favor 她 over 他"
        );

        // 3. 跨句動物（「小花貓」 -> 「牠」）
        let anim_cat = mk(&["牠", "每天", "都", "在", "陽台", "曬太陽"]);
        let he_cat = mk(&["他", "每天", "都", "在", "陽台", "曬太陽"]);
        assert!(
            score_discourse_sentence_with_prefix(&anim_cat, "鄰居養了一隻小花貓。")
                > score_discourse_sentence_with_prefix(&he_cat, "鄰居養了一隻小花貓。"),
            "cross-sentence 小花貓 must favor 牠 over 他"
        );

        // 4. 防誤判防護：「其他伺服器」不得將「他」視為男性；「馬路／牛肉麵」不得觸發動物
        assert!(
            score_discourse_sentence_with_prefix(&it_srv, "其他伺服器的效能很穩。")
                > score_discourse_sentence_with_prefix(&he_srv, "其他伺服器的效能很穩。"),
            "其他 must NOT register as Male antecedent"
        );
        assert_eq!(
            detect_latest_prefix_antecedent("我走過馬路去買牛肉麵。"),
            None,
            "馬路／牛肉麵 must NOT falsely trigger Animal"
        );

        // 5. 局部人類認知／言語謂語優先權：「她說這支手機很好用」不被前句「手機」反向覆寫
        let she_local = mk(&["她", "說", "這支", "手機", "很好用"]);
        let it_local = mk(&["它", "說", "這支", "手機", "很好用"]);
        assert!(
            score_discourse_sentence_with_prefix(&she_local, "姊姊買了一台新手機。")
                > score_discourse_sentence_with_prefix(&it_local, "姊姊買了一台新手機。"),
            "local human speech predicate 說 must still beat cross-sentence inanimate antecedent"
        );

        // 6. 話題轉移近鄰優先（「妹妹」 -> 「伺服器」 -> 以最新「伺服器」為主）
        let it_shift = mk(&["它", "運作", "非常", "穩定"]);
        let she_shift = mk(&["她", "運作", "非常", "穩定"]);
        assert!(
            score_discourse_sentence_with_prefix(
                &it_shift,
                "我妹妹剛從日本回來。她買了一台新伺服器。",
            ) > score_discourse_sentence_with_prefix(
                &she_shift,
                "我妹妹剛從日本回來。她買了一台新伺服器。",
            ),
            "most recent antecedent (伺服器) must win on topic shift"
        );

        // 7. 有生謂語防護：「還在睡」（即使拆為「還」「在」「睡」單字節點）不得被前句無生事物翻為「它還在睡」，動物前提仍保留「牠還在睡」
        let he_sleep = mk(&["他", "還", "在", "睡"]);
        let it_sleep = mk(&["它", "還", "在", "睡"]);
        let anim_sleep = mk(&["牠", "還", "在", "睡"]);
        assert!(
            score_discourse_sentence_with_prefix(&he_sleep, "這台新買的伺服器效能非常穩定。")
                >= score_discourse_sentence_with_prefix(&it_sleep, "這台新買的伺服器效能非常穩定。"),
            "animate predicate 在睡 must NOT be flipped to 它 by preceding inanimate noun"
        );
        assert!(
            score_discourse_sentence_with_prefix(&anim_sleep, "鄰居養了一隻小花貓。")
                > score_discourse_sentence_with_prefix(&he_sleep, "鄰居養了一隻小花貓。"),
            "animal antecedent 小花貓 must still favor 牠還在睡 over 他還在睡"
        );

        // 8. 領屬雙向防護：「他的反應很快」不被前句設備或抽象詞彙翻為「它的反應很快」
        let he_react = mk(&["他", "的", "反應", "很快"]);
        let it_react = mk(&["它", "的", "反應", "很快"]);
        for pref in ["這台新買的伺服器效能非常穩定。", "而不需要建立一堆假詞彙。"] {
            assert!(
                score_discourse_sentence_with_prefix(&he_react, pref)
                    >= score_discourse_sentence_with_prefix(&it_react, pref),
                "possessive 他的反應很快 must NOT be flipped by prefix {pref}"
            );
        }
    }
}
