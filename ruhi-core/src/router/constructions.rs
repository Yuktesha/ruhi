/// RuhI (如喜) Input Method Framework - Rust Core
/// src/router/constructions.rs - 階層式集合槽位構式與對比並列連詞引擎 (Hierarchical Constructions & Contrastive Coordination Engine)

use std::collections::HashMap;
use crate::lexicon::LexiconEntry;
use super::lattice::LatticeNode;
use super::transitions::{TransitionEngine, WordPattern};

impl TransitionEngine {
    pub(crate) fn is_repeatable_action_verb(w: &str) -> bool {
        matches!(
            w,
            "點擊" | "點選" | "按壓" | "按下" | "執行" | "觸發" | "測試" | "呼叫"
                | "發送" | "傳送" | "讀取" | "寫入" | "更新" | "輸入" | "輸出" | "搜尋"
                | "查詢" | "計算" | "連線" | "重試" | "滾動" | "滑動" | "切換" | "載入"
                | "儲存" | "刷新" | "編譯" | "檢查" | "掃描" | "跳轉" | "移動" | "啟動"
                | "開啟" | "關閉" | "選取" | "勾選" | "框選" | "圈選" | "拜訪" | "造訪"
                | "練習" | "嘗試" | "呼吸" | "閃爍" | "震動" | "跳動" | "循環" | "迭代"
        )
    }

    pub(crate) fn is_verbal_numeral(w: &str) -> bool {
        matches!(
            w,
            "一" | "二" | "三" | "四" | "五" | "六" | "七" | "八" | "九" | "十"
                | "兩" | "幾" | "多" | "數" | "百" | "千" | "萬" | "單" | "雙"
        )
    }

    pub(crate) fn is_verbal_classifier(w: &str) -> bool {
        matches!(
            w,
            "次" | "回" | "下" | "遍" | "趟" | "番" | "聲" | "步" | "輪" | "筆"
        )
    }

    pub(crate) fn is_verbal_qp_word(w: &str) -> bool {
        let mut chars = w.chars();
        let first = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        let last = match chars.last() {
            Some(c) => c,
            None => return false,
        };
        let first_ok = matches!(
            first,
            '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
                | '兩' | '幾' | '多' | '數' | '百' | '千' | '萬' | '單' | '雙'
        );
        let last_ok = matches!(
            last,
            '次' | '回' | '下' | '遍' | '趟' | '番' | '聲' | '步' | '輪' | '筆'
                | '秒' | '分' | '天' | '年' | '頁' | '行' | '字' | '格' | '點' | '圈' | '層'
        );
        first_ok && last_ok
    }

    pub(crate) fn dag_has_verbal_qp(
        dag: &HashMap<usize, Vec<LatticeNode>>,
        end_idx: usize,
    ) -> bool {
        if let Some(next_nodes) = dag.get(&end_idx) {
            for n in next_nodes {
                if Self::is_verbal_qp_word(&n.word) {
                    return true;
                }
                if Self::is_verbal_numeral(&n.word) {
                    if let Some(after_num) = dag.get(&n.end_idx) {
                        if after_num.iter().any(|m| Self::is_verbal_classifier(&m.word)) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// 階層式集合槽位構式引擎 (Hierarchical Slot-Set Construction Engine)
    /// 支援 `[A] → [B] → [({C} → {D})]`、`[ 大 X 特 X ]`、`[更常 + 動詞]` 與 `[X] 與 [Y]` 對比並列連詞之左向回溯與右向 DAG 預看評估
    pub fn calculate_hierarchical_construction_bonus(
        &self,
        prefix_words: &[&str],
        curr_word: &str,
        curr_bopomofo: &str,
        end_idx: usize,
        dag: &HashMap<usize, Vec<LatticeNode>>,
    ) -> f64 {
        let n = prefix_words.len();
        let prev_word = if n >= 1 { prefix_words[n - 1] } else { "" };
        let prev2_word = if n >= 2 { prefix_words[n - 2] } else { "" };
        let prev3_word = if n >= 3 { prefix_words[n - 3] } else { "" };
        let mut bonus = 0.0;

        // ── 構式一：頻率全稱量化 × 動作動量構式 `[每/各/凡/逢] → [動作動詞] → [({一..十, 兩, 幾} → {次, 回, 下, 遍, 趟})]` ──
        if matches!(prev_word, "每" | "各" | "凡" | "逢" | "每當" | "每次")
            && Self::dag_has_verbal_qp(dag, end_idx)
        {
            if Self::is_repeatable_action_verb(curr_word) {
                bonus += 32.0;
            } else if matches!(
                curr_word,
                "典籍" | "古籍" | "書籍" | "戶籍" | "國籍" | "學籍" | "史籍" | "祖籍" | "秘笈" | "祕笈"
            ) {
                bonus -= 28.0;
            }
        }
        if Self::is_repeatable_action_verb(prev_word) {
            if Self::is_verbal_qp_word(curr_word) {
                bonus += 24.0;
            } else if Self::is_verbal_numeral(curr_word) {
                if let Some(next_nodes) = dag.get(&end_idx) {
                    if next_nodes.iter().any(|m| Self::is_verbal_classifier(&m.word)) {
                        bonus += 20.0;
                    }
                }
            }
        }
        if Self::is_repeatable_action_verb(prev2_word) && Self::is_verbal_numeral(prev_word) {
            if Self::is_verbal_classifier(curr_word) {
                bonus += 26.0;
            } else if matches!(curr_word, "刺" | "賜" | "伺" | "祠" | "肆" | "寺" | "嗣") {
                bonus -= 25.0;
            }
        }

        // ── 構式二：介面動作複合詞 × 容器後綴構式 `[({圈, 點, 勾, 框, 篩, 挑, 複, 全, 單, 候} → {選, 取, 列})] → [{框, 欄, 區, 鈕, 鍵, 窗, 頁, 表, 列, 項}]` ──
        let is_ui_select_root = |w: &str| {
            matches!(
                w,
                "圈" | "點" | "勾" | "框" | "篩" | "挑" | "複" | "全" | "單" | "候" | "反" | "多" | "揀"
            )
        };
        let is_ui_select_act = |w: &str| matches!(w, "選" | "取" | "列");
        let is_ui_compound_modifier = |w: &str| {
            matches!(
                w,
                "圈選" | "點選" | "勾選" | "框選" | "篩選" | "挑選" | "複選" | "全選" | "單選"
                    | "候選" | "反選" | "多選" | "選取" | "下拉" | "對話" | "輸入" | "文字"
                    | "搜尋" | "編輯" | "屬性" | "清單" | "列表" | "預覽" | "提示" | "訊息"
                    | "警告" | "確認" | "密碼" | "組合" | "工具" | "狀態" | "標題" | "側邊"
                    | "導覽" | "浮動" | "彈出"
            )
        };
        let is_ui_container_head = |w: &str| {
            matches!(
                w,
                "框" | "欄" | "區" | "鈕" | "鍵" | "窗" | "頁" | "表" | "列" | "項" | "盒" | "格"
            )
        };

        // 內層 `({圈,點,勾,框...} → {選,取,列})`
        if is_ui_select_root(prev_word) && is_ui_select_act(curr_word) {
            bonus += 28.0;
        }
        if let Some(next_nodes) = dag.get(&end_idx) {
            let next_has_select = next_nodes
                .iter()
                .any(|m| is_ui_select_act(&m.word) || m.word == "選框" || m.word == "選取");
            if next_has_select {
                if curr_word == "框" && curr_bopomofo.starts_with("ㄎㄨㄤ") {
                    bonus += 30.0;
                } else if curr_word == "圈" {
                    let has_container_after = next_nodes.iter().any(|m| {
                        m.word == "選框"
                            || (is_ui_select_act(&m.word)
                                && dag
                                    .get(&m.end_idx)
                                    .map(|k| k.iter().any(|x| is_ui_container_head(&x.word)))
                                    .unwrap_or(false))
                    });
                    if has_container_after {
                        bonus += 26.0;
                    }
                } else if matches!(curr_word, "筐" | "匡" | "哐" | "誆" | "眶") {
                    bonus -= 28.0;
                }
            }
        }

        // 外層 `[<ActionSelect>] → [{框, 欄, 區, 鈕, 鍵, 窗...}]`
        let prev_is_ui_modifier = is_ui_compound_modifier(prev_word)
            || (is_ui_select_root(prev2_word) && is_ui_select_act(prev_word));
        if prev_is_ui_modifier {
            if curr_word == "框" && curr_bopomofo.starts_with("ㄎㄨㄤ") {
                bonus += 34.0;
            } else if is_ui_container_head(curr_word) && curr_word != "框" {
                bonus += 26.0;
            } else if matches!(curr_word, "筐" | "匡" | "哐" | "誆" | "眶" | "誑") {
                bonus -= 30.0;
            }
        }

        // ── 構式三：祈使工具介詞 × 雙音節複合動作構式 `[(請/先/再/改 → 用/以/拿/靠/藉)] → [工具名詞] → [(框/圈/點/勾/拖/拉 → 選/取/曳/動)]` ──
        let is_tool_noun = |w: &str| {
            matches!(
                w,
                "滑鼠" | "游標" | "鍵盤" | "手指" | "左鍵" | "右鍵" | "中鍵" | "滾輪"
                    | "觸控板" | "觸控筆" | "手勢" | "熱鍵" | "快捷鍵" | "方向鍵" | "空白鍵"
            )
        };
        if matches!(prev_word, "請" | "先" | "再" | "改" | "可" | "要" | "只")
            && matches!(curr_word, "用" | "以" | "拿" | "靠" | "藉")
        {
            bonus += 22.0;
        }
        if matches!(
            prev_word,
            "用" | "以" | "拿" | "靠" | "藉" | "請用" | "先用" | "再用" | "改用" | "使用" | "利用" | "透過"
        ) && is_tool_noun(curr_word)
        {
            bonus += 26.0;
        }
        if is_tool_noun(prev_word) {
            if matches!(
                curr_word,
                "框選" | "圈選" | "點選" | "勾選" | "選取" | "拖曳" | "拉動" | "點擊" | "雙擊" | "右擊" | "滾動" | "移動" | "標記"
            ) {
                bonus += 32.0;
            } else if (curr_word == "框" && curr_bopomofo.starts_with("ㄎㄨㄤ"))
                || matches!(curr_word, "圈" | "點" | "勾" | "拖" | "拉")
            {
                if let Some(next_nodes) = dag.get(&end_idx) {
                    if next_nodes
                        .iter()
                        .any(|m| matches!(m.word.as_str(), "選" | "取" | "曳" | "動" | "擊"))
                    {
                        bonus += 34.0;
                    }
                }
            } else if matches!(curr_word, "筐" | "匡" | "哐" | "誆" | "眶") {
                bonus -= 32.0;
            }
        }
        if is_tool_noun(prev2_word)
            && matches!(prev_word, "框" | "圈" | "點" | "勾" | "拖" | "拉")
            && matches!(curr_word, "選" | "取" | "曳" | "動" | "擊")
        {
            bonus += 32.0;
        }

        // ── 構式四：介詞空間方位複合構式 `[{在, 於, 從, 往, 向, 自}] → [{框, 欄, 區, 窗, 頁, 表}] → [({中, 內, 裡, 外, 上, 下} → {的, 面, 邊, ∅})]` ──
        let prev_is_loc_prep = matches!(prev_word, "在" | "於" | "從" | "往" | "向" | "自")
            || prev_word.ends_with('在')
            || prev_word.ends_with('於')
            || prev_word.ends_with('從');
        if prev_is_loc_prep {
            let dag_has_localizer = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        m.word.starts_with(['中', '內', '裡', '外', '上', '下', '前', '後', '旁', '間'])
                    })
                })
                .unwrap_or(false);
            if dag_has_localizer {
                if (curr_word == "框" && curr_bopomofo.starts_with("ㄎㄨㄤ"))
                    || matches!(curr_word, "欄" | "區" | "窗" | "頁" | "表" | "列" | "格" | "盒")
                {
                    bonus += 32.0;
                } else if matches!(curr_word, "筐" | "匡" | "哐" | "誆" | "眶") {
                    bonus -= 30.0;
                }
            }
        }

        // ── 構式五：空間趨向對稱構式 `[({上, 下, 左, 右, 前, 後} → {上, 下, 左, 右, 內, 外, 方, 邊, 側, 面})] → [往/向] → [複合方位]` ──
        if matches!(curr_word, "往" | "網" | "枉") {
            let prev_is_spatial = matches!(
                prev_word,
                "上" | "下" | "左" | "右" | "前" | "後" | "東" | "西" | "南" | "北" | "內" | "外" | "裡"
                    | "由上" | "由下" | "由左" | "由右" | "從上" | "從下" | "從左" | "從右"
                    | "左上" | "左下" | "右上" | "右下" | "上方" | "下方" | "左方" | "右方"
                    | "前方" | "後方" | "上邊" | "下邊" | "左邊" | "右邊" | "左側" | "右側"
                    | "上面" | "下面" | "裡面" | "外面"
            );
            let next_is_spatial = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        m.word.starts_with([
                            '上', '下', '左', '右', '前', '後', '東', '西', '南', '北', '內', '外', '裡', '中',
                        ])
                    })
                })
                .unwrap_or(false);
            if prev_is_spatial && next_is_spatial {
                if curr_word == "往" {
                    bonus += 32.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }

        // ── 構式六：程度疑問／感嘆構式 `[{有, 到底有, 究竟有, 這麼, 那麼, 沒, 沒有, 到底, 究竟}] → [多] → [狀態形容詞/實詞]`
        //    以及「數詞/指示詞 → 量詞(朵/首/座) → 名詞」三元條件保護（徹底杜絕無數詞時「朵」搶佔「多」）──
        let prev_is_degree_lead = matches!(
            prev_word,
            "有" | "到底有" | "究竟有" | "這麼" | "那麼" | "這般" | "那般" | "沒" | "沒有" | "到底" | "究竟"
        ) || prev_word.ends_with('有');
        if prev_is_degree_lead {
            if curr_word == "多" {
                bonus += 30.0;
            } else if curr_word == "朵" {
                bonus -= 32.0;
            }
        }
        if matches!(prev_word, "朵" | "首" | "座") {
            let prev2_is_num_or_dem = WordPattern::is_numeric_token(prev2_word)
                || matches!(prev2_word, "這" | "那" | "哪" | "每" | "各" | "某" | "幾" | "兩" | "半");
            if prev2_is_num_or_dem && !self.function_words.contains(curr_word) {
                bonus += 20.0;
            }
        }

        // ── 構式七：動補狀態構式 `[動詞/狀態述語] → [得] / [V+得] → [程度副詞(十分/很/非常/太/更/最/真/超/相當/特別/格外/夠)`
        //    `或典型可能/結果補語]`（如：收羅得十分齊全、寫得非常精彩、做得很好、講得很清楚，杜絕「的 / V+的」搶佔補語標記「得 / V+得」）──
        let is_one_char_de_or_de2 = matches!(curr_word, "得" | "的")
            && !prev_word.is_empty()
            && !self.function_words.contains(prev_word)
            && !WordPattern::is_numeric_token(prev_word);
        let is_two_char_v_de_or_de2 = curr_word.chars().count() == 2
            && (curr_word.ends_with('得') || curr_word.ends_with('的'))
            && !matches!(
                curr_word,
                "覺得" | "記得" | "懂得" | "值得" | "省得" | "免得" | "懶得" | "捨得" | "認得" | "曉得" | "顯得" | "變得" | "落得" | "巴不得" | "怪不得" | "不見得" | "我的" | "你的" | "他的" | "她的" | "它的" | "誰的" | "別的" | "有的" | "是的" | "真的" | "假的" | "好的" | "對的"
            );
        if is_one_char_de_or_de2 || is_two_char_v_de_or_de2 {
            let next_is_degree_complement = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "十分" | "非常" | "相當" | "特別" | "格外" | "無比" | "極為" | "極其"
                                | "多麼" | "這麼" | "那麼" | "如此" | "很" | "太" | "更" | "最"
                                | "真" | "超" | "極" | "夠" | "挺" | "蠻" | "頗" | "好" | "快"
                                | "慢" | "齊全" | "清楚" | "明白" | "乾淨" | "徹底" | "妥當"
                                | "恰當" | "得體" | "過癮" | "痛快" | "流利" | "熟練" | "透徹"
                                | "精彩" | "漂亮" | "完美" | "出色" | "不錯" | "開心" | "高興"
                                | "快樂" | "舒服" | "自在" | "辛苦" | "累" | "慘" | "糟" | "妙"
                                | "準" | "穩" | "不可開交" | "團團轉" | "有聲有色" | "頭頭是道"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_degree_complement {
                if curr_word.ends_with('得') {
                    bonus += 28.0;
                } else if curr_word.ends_with('的') {
                    bonus -= 24.0;
                }
            }
        }

        // ── 構式八：`[ 大 X 特 X ]` 四字動量對稱構式（如：大書特書、大搞特搞、大買特買、大吃特吃、大說特說）──
        // 嚴格限定前驅具有「大」且前後動作 X 完全同一，杜絕無條件「實詞 + 特 + 實詞」將「一種特殊」拆碎為「一種特書」！
        if prev2_word == "大" && prev_word.chars().count() == 1 && curr_word == "特" {
            let next_matches_x = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == prev_word))
                .unwrap_or(false);
            if next_matches_x {
                bonus += 28.0;
            }
        }
        if prev3_word == "大" && prev_word == "特" && curr_word == prev2_word && curr_word.chars().count() == 1 {
            bonus += 30.0;
        }

        // ── 構式九：比較/程度副詞 + 「常（頻率副詞）vs 長（空間/時間維度形容詞）」雙向語境分流構式 ──
        let is_freq_verb_target = |w: &str| {
            matches!(
                w,
                "用到" | "用" | "使用" | "見" | "見到" | "看見" | "看" | "看到" | "聽到" | "聽"
                    | "有" | "會" | "出現" | "發生" | "遇到" | "碰到" | "去" | "來" | "說" | "講"
                    | "做" | "吃" | "買" | "打" | "寫" | "問" | "被" | "在" | "拿" | "找" | "跑"
                    | "走" | "出差" | "加班" | "運動" | "練習" | "聯絡" | "見面"
            )
        };
        let is_dimension_noun = |w: &str| {
            matches!(
                w,
                "時間" | "距離" | "路" | "山路" | "道路" | "路途" | "路程" | "線" | "路線"
                    | "篇幅" | "句子" | "文章" | "段落" | "歷史" | "壽命" | "假期" | "工時"
                    | "週期" | "波長" | "頭髮" | "鬍鬚" | "手指" | "腿" | "隊伍" | "橋"
                    | "河" | "隧道" | "清單" | "名單" | "這條" | "那條" | "這段" | "那段"
            ) || w.ends_with('路')
                || w.ends_with('線')
                || w.ends_with('橋')
        };
        let has_left_dimension_noun = prefix_words.iter().rev().take(4).any(|w| is_dimension_noun(w));

        if matches!(curr_word, "更常" | "更長" | "很常" | "很長" | "太常" | "太長" | "最常" | "最長") {
            let next_is_freq_verb = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| is_freq_verb_target(&m.word)))
                .unwrap_or(false);
            if next_is_freq_verb {
                if curr_word.ends_with('常') {
                    bonus += 32.0;
                } else {
                    bonus -= 28.0;
                }
            } else if has_left_dimension_noun {
                if curr_word.ends_with('長') {
                    bonus += 32.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }
        if matches!(prev_word, "更" | "最" | "很" | "太" | "不" | "相當" | "非常" | "極" | "挺" | "蠻" | "超" | "真")
            && matches!(curr_word, "常" | "長")
        {
            let next_is_freq_verb = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| is_freq_verb_target(&m.word)))
                .unwrap_or(false);
            if next_is_freq_verb {
                if curr_word == "常" {
                    bonus += 28.0;
                } else {
                    bonus -= 25.0;
                }
            } else if has_left_dimension_noun {
                if curr_word == "長" {
                    bonus += 30.0;
                } else {
                    bonus -= 26.0;
                }
            }
        }

        // ── 構式十：連詞／介詞「與」之五維語法功能與對比並列構式 `[X] 與 [Y]` ──
        // 1. 防止雙字詞（如「擋雨」、「黨羽」、「詞語」、「常語」、「彈雨」）吞噬中間的連詞「與 (ㄩˇ)」：
        //    當 2 字候選詞以「ㄩˇ」結尾，且緊接在後的音節與該 2 字詞首字同音（即輸入為 [Syl_X] + ㄩˇ + [Syl_X]）時，
        //    代表「ㄩˇ」實為對比連詞「與」，嚴禁前兩音節黏合成「X雨 / X語 / X羽」！
        if curr_word.chars().count() == 2 && curr_word != "與" && curr_bopomofo.ends_with(" ㄩˇ") {
            let first_syl = curr_bopomofo.split_whitespace().next().unwrap_or("");
            let first_base = LexiconEntry::strip_tones(first_syl);
            if !first_base.is_empty() {
                let next_repeats_first_syl = dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            if m.length() == 1 {
                                let m_base = LexiconEntry::strip_tones(&m.bopomofo);
                                m_base == first_base
                            } else {
                                false
                            }
                        })
                    })
                    .unwrap_or(false);
                if next_repeats_first_syl {
                    bonus -= 55.0;
                }
            }
        }
        // 防止後兩音節黏合成「雨X / 語X」（即 `curr_bopomofo.starts_with("ㄩˇ ")` 且前驅為同音單字）
        if curr_word.chars().count() == 2
            && curr_word != "與"
            && curr_bopomofo.starts_with("ㄩˇ ")
            && prev_word.chars().count() == 1
            && end_idx >= 3
        {
            let second_syl = curr_bopomofo.split_whitespace().nth(1).unwrap_or("");
            let second_base = LexiconEntry::strip_tones(second_syl);
            let prev_matches_second = dag
                .get(&(end_idx - 3))
                .map(|ns| {
                    ns.iter().any(|m| {
                        m.word == prev_word
                            && LexiconEntry::strip_tones(&m.bopomofo) == second_base
                    })
                })
                .unwrap_or(false);
            if prev_matches_second {
                bonus -= 55.0;
            }
        }

        // 2. 當目前候選詞為連詞「與 (ㄩˇ)」時：
        //    若前驅 X 為 1~2 字詞，且「與」之後在 DAG 中存在與 X「同音但字形相異」的對比候選詞 Y，賦予對比並列連接引力！
        if curr_word == "與" && curr_bopomofo.starts_with("ㄩˇ") && !prev_word.is_empty() {
            let prev_len = prev_word.chars().count();
            if (prev_len == 1 || prev_len == 2) && end_idx >= 1 + prev_len {
                let prev_start = end_idx - 1 - prev_len;
                let prev_base_opt = dag.get(&prev_start).and_then(|ns| {
                    ns.iter()
                        .find(|m| m.word == prev_word && m.end_idx == end_idx - 1)
                        .map(|m| LexiconEntry::strip_tones(&m.bopomofo))
                });
                if let Some(prev_base) = prev_base_opt {
                    let has_distinct_homophone_y = dag
                        .get(&end_idx)
                        .map(|ns| {
                            ns.iter().any(|m| {
                                m.word.chars().count() == prev_len
                                    && m.word != prev_word
                                    && LexiconEntry::strip_tones(&m.bopomofo) == prev_base
                            })
                        })
                        .unwrap_or(false);
                    if has_distinct_homophone_y {
                        bonus += 26.0;
                    }
                }
            }
        }

        // 3. 當前驅為連詞「與」，且再前一個詞為 X (`prev2_word`) 時：
        //    (a) 嚴格反重言約束 (Anti-Tautology Constraint `X != Y`)：嚴禁「與」字兩側出現完全相同之字詞！
        //    (b) 同音對比異字加權 (`X != Y` 且基礎音節相同)：優先選出同音異字之對比項 Y！
        if prev_word == "與" && !prev2_word.is_empty() {
            let curr_len = curr_word.chars().count();
            let prev2_len = prev2_word.chars().count();
            if curr_word == prev2_word {
                bonus -= 65.0;
            } else if curr_len == prev2_len && end_idx >= curr_len + 1 + prev2_len {
                let prev2_start = end_idx - curr_len - 1 - prev2_len;
                let prev2_end = end_idx - curr_len - 1;
                let curr_base = LexiconEntry::strip_tones(curr_bopomofo);
                let prev2_matches_base = dag
                    .get(&prev2_start)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            m.word == prev2_word
                                && m.end_idx == prev2_end
                                && LexiconEntry::strip_tones(&m.bopomofo) == curr_base
                        })
                    })
                    .unwrap_or(false);
                if prev2_matches_base {
                    bonus += 28.0;
                    if matches!(
                        (prev2_word, curr_word),
                        ("再", "在")
                            | ("不再", "不在")
                            | ("的", "得")
                            | ("黨", "檔")
                            | ("但", "淡")
                            | ("長", "常")
                            | ("就", "救")
                            | ("就好", "救好")
                            | ("詞", "辭")
                    ) {
                        bonus += 18.0;
                    }
                }
            }
        }

        // 4. 伴隨／關係介詞構式 `[與] + [NP] + [關係述語/對稱互動動詞]`：
        //    如「與此案無關」、「與系統相容」、「與團隊討論」、「與客戶溝通」
        if matches!(
            curr_word,
            "有關" | "無關" | "相容" | "不相容" | "相同" | "不同" | "相符" | "不符" | "相似" | "相近"
                | "相反" | "一致" | "不一致" | "討論" | "合作" | "結合" | "交流" | "溝通" | "聯繫"
                | "聯絡" | "比較" | "對照" | "商量" | "協商" | "配合" | "同步" | "互動" | "並存"
        ) && prefix_words.iter().rev().take(4).any(|&w| w == "與")
        {
            bonus += 22.0;
        }

        // ── 構式十一：空間框式介詞構式 `[在 / 不在] + [2字容器/場所名詞] + [方位後置詞 (中/裡/內/裡面/外面/中間/之間)]` ──
        if matches!(curr_word, "在" | "再" | "不在" | "不再") {
            let has_container_localizer = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        m.length() == 2
                            && !matches!(m.word.as_str(), "決定" | "考慮" | "討論" | "研究" | "確認" | "試試" | "看看")
                            && dag
                                .get(&m.end_idx)
                                .map(|after_ns| {
                                    after_ns.iter().any(|loc| {
                                        matches!(
                                            loc.word.as_str(),
                                            "中" | "裡" | "內" | "裡面" | "外面" | "中間" | "之間" | "之內" | "之中"
                                        )
                                    })
                                })
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if has_container_localizer {
                if matches!(curr_word, "在" | "不在") {
                    bonus += 24.0;
                } else {
                    bonus -= 20.0;
                }
            }
        }

        // ── 構式十二：處置／被動／醫療救援構式 `[把/將/被/醫生/消防隊/病患/傷患/人] ... → [救 / 救好 / 救活 / 救回 / 救回來]` vs 條件讓步 `[這樣/只要] ... → [就好]` ──
        if (curr_word.starts_with('救') || curr_word.starts_with('就'))
            && curr_bopomofo.starts_with("ㄐㄧㄡˋ")
        {
            let has_rescue_context = prefix_words.iter().rev().take(4).any(|&w| {
                matches!(
                    w,
                    "把" | "將" | "被" | "醫生" | "醫師" | "醫護" | "醫院" | "消防" | "消防隊"
                        | "救護" | "急救" | "搶救" | "病患" | "病人" | "傷患" | "患者" | "災民"
                        | "人質" | "性命" | "生命" | "一命" | "人" | "他" | "她" | "牠"
                )
            });
            let next_is_rescue_complement = curr_word.chars().count() >= 2
                || dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "活" | "回" | "回來" | "出" | "出來" | "起" | "起來" | "下" | "下來"
                                    | "命" | "人" | "治" | "援" | "火" | "災" | "急" | "兵" | "星"
                                    | "世" | "贖" | "濟" | "助"
                            )
                        })
                    })
                    .unwrap_or(false);
            let has_ba_bei_or_medical = prefix_words.iter().rev().take(4).any(|&w| {
                matches!(
                    w,
                    "把" | "將" | "被" | "醫生" | "醫師" | "醫護" | "醫院" | "消防" | "消防隊"
                        | "救護" | "急救" | "搶救" | "病患" | "病人" | "傷患" | "患者"
                )
            });
            if has_ba_bei_or_medical || (has_rescue_context && next_is_rescue_complement) {
                if curr_word.starts_with('救') {
                    bonus += 34.0;
                } else if matches!(curr_word, "就" | "就好" | "就回" | "就回來") {
                    bonus -= 28.0;
                }
            }
        }

        // ── 動態外掛三元槽位構式 (`patterns_user.jsonl` 中 slots.len() >= 3) ──
        for pat in &self.user_trigram_patterns {
            if !prev_word.is_empty() && pat.slot0.contains(prev_word) && pat.slot1.contains(curr_word) {
                let next_matches = dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|m| pat.slot2.contains(&m.word)))
                    .unwrap_or(false);
                if next_matches {
                    bonus += pat.bonus;
                }
            }
            if !prev2_word.is_empty()
                && pat.slot0.contains(prev2_word)
                && pat.slot1.contains(prev_word)
                && pat.slot2.contains(curr_word)
            {
                bonus += pat.bonus * 0.8;
            }
        }

        bonus
    }
}
