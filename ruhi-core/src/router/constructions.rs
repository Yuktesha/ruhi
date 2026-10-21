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

    pub(crate) fn is_pseudo_cleft_focus_verb(w: &str) -> bool {
        let is_core = |s: &str| {
            matches!(
                s,
                "選" | "買" | "賣" | "看" | "聽" | "讀" | "寫" | "說" | "講" | "問"
                    | "答" | "要" | "想" | "做" | "吃" | "喝" | "穿" | "玩" | "用" | "找"
                    | "求" | "談" | "開" | "學" | "教" | "考" | "比" | "拼" | "靠" | "憑"
                    | "缺" | "怕" | "愛" | "指" | "投" | "盼" | "爭" | "護" | "守" | "救"
                    | "挑" | "換" | "改" | "修" | "調" | "算" | "猜" | "記" | "認" | "懂"
                    | "信"
            )
        };
        if is_core(w) {
            return true;
        }
        if w.chars().count() == 2 {
            let mut cs = w.chars();
            let c0 = cs.next().unwrap();
            let c1 = cs.next().unwrap().to_string();
            if matches!(c0, '不' | '沒' | '最' | '只' | '所' | '真') && is_core(&c1) {
                return true;
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
        if matches!(prev_word, "朵" | "首" | "座" | "種" | "段") {
            let prev2_is_num_or_dem = WordPattern::is_numeric_token(prev2_word)
                || matches!(prev2_word, "這" | "那" | "哪" | "每" | "各" | "某" | "幾" | "兩" | "半");
            if prev2_is_num_or_dem && !self.function_words.contains(curr_word) {
                bonus += 16.0;
            }
        }
        if prev_word == "間" {
            let prev2_is_num_or_dem = WordPattern::is_numeric_token(prev2_word)
                || matches!(prev2_word, "這" | "那" | "哪" | "每" | "某" | "幾" | "兩");
            let curr_is_room_or_building = matches!(
                curr_word,
                "房" | "房間" | "屋" | "屋子" | "店" | "店面" | "教室" | "學校" | "公司"
                    | "醫院" | "工廠" | "餐廳" | "套房" | "雅房" | "臥室" | "廁所" | "浴室"
                    | "廚房" | "書房" | "客房" | "病房" | "牢房" | "廟" | "寺廟" | "教堂"
            );
            if prev2_is_num_or_dem && curr_is_room_or_building {
                bonus += 16.0;
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
                                | "很快" | "很好" | "很慢" | "很遠" | "很高" | "很棒" | "很美"
                                | "很帥" | "很準" | "很穩" | "很累" | "很慘"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_head_noun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "距離" | "速度" | "時間" | "方向" | "路線" | "姿勢" | "樣子" | "事"
                                | "事情" | "人" | "東西" | "地方" | "時候" | "問題" | "結果"
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
            } else if next_is_head_noun {
                if curr_word.ends_with('的') {
                    bonus += 26.0;
                } else if curr_word.ends_with('得') {
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

        // 2. 當目前候選詞為對比首項 X（後接「與 + 同音異字 Y」，或「再 + V + 與 + 在 + V」）時：
        //    賦予對比首項 X 語法保護引力，防止前驅「是 / 如果」之實詞規則將功能詞「再/的」錯推為罕見實詞「載/洅/底」！
        if matches!(
            curr_word,
            "再" | "不再" | "的" | "黨" | "但" | "長" | "就" | "就好" | "詞"
        ) {
            let next_is_yu_homophone_pair = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|yu| {
                        yu.word == "與"
                            && dag
                                .get(&yu.end_idx)
                                .map(|after_yu| {
                                    after_yu.iter().any(|y| {
                                        matches!(
                                            (curr_word, y.word.as_str()),
                                            ("再", "在")
                                                | ("不再", "不在")
                                                | ("的", "得")
                                                | ("黨", "檔")
                                                | ("但", "淡")
                                                | ("長", "常")
                                                | ("就", "救")
                                                | ("就好", "救好")
                                                | ("詞", "辭")
                                        )
                                    })
                                })
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            let next_is_v_yu_zai_v = curr_word == "再"
                && dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|v1| {
                            v1.length() == 1
                                && dag
                                    .get(&v1.end_idx)
                                    .map(|after_v1| {
                                        after_v1.iter().any(|yu| {
                                            yu.word == "與"
                                                && dag
                                                    .get(&yu.end_idx)
                                                    .map(|after_yu| {
                                                        after_yu.iter().any(|z2| {
                                                            matches!(z2.word.as_str(), "在" | "再")
                                                                && dag
                                                                    .get(&z2.end_idx)
                                                                    .map(|after_z2| {
                                                                        after_z2
                                                                            .iter()
                                                                            .any(|v2| v2.word == v1.word)
                                                                    })
                                                                    .unwrap_or(false)
                                                        })
                                                    })
                                                    .unwrap_or(false)
                                        })
                                    })
                                    .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
            if next_is_yu_homophone_pair || next_is_v_yu_zai_v {
                bonus += 32.0;
            }
        }

        // 2(b). 當目前候選詞為連詞「與 (ㄩˇ)」時：
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
            // 支援組合式 `[再/在] + [V] + 與 + [在/再] + [V]` 對比並列構式（免建假雙字詞「再想/在想」）：
            if prev_len == 1 && matches!(prev2_word, "再" | "在") {
                let next_has_opposite_zai_and_same_v = dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(m.word.as_str(), "再" | "在")
                                && m.word != prev2_word
                                && dag
                                    .get(&m.end_idx)
                                    .map(|after_ns| after_ns.iter().any(|v| v.word == prev_word))
                                    .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                if next_has_opposite_zai_and_same_v {
                    bonus += 28.0;
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
            } else if curr_len == 1
                && matches!(curr_word, "再" | "在")
                && matches!(prev3_word, "再" | "在")
            {
                // 組合式 `[再/在] + [V] + 與 + [在/再] + [V]` 對稱反重言與異字加權：
                let next_matches_prev2_v = dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|v| v.word == prev2_word))
                    .unwrap_or(false);
                if next_matches_prev2_v {
                    if curr_word == prev3_word {
                        bonus -= 65.0;
                    } else {
                        bonus += 46.0;
                    }
                }
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

        // ── 構式十一：空間／時間框式介詞構式 `[在 / 不在] + [1~2個名詞/修飾詞節點] + [方位後置詞 (中/裡/內/上/下/裡面/外面/中間/之間)]` ──
        if matches!(curr_word, "在" | "再" | "不在" | "不再") {
            let is_locative_postposition = |w: &str| {
                matches!(
                    w,
                    "中" | "裡" | "內" | "上" | "下" | "裡面" | "外面" | "中間" | "之間" | "之內" | "之中"
                )
            };
            let is_excluded_verb = |w: &str| {
                matches!(
                    w,
                    "決定" | "考慮" | "討論" | "研究" | "確認" | "試試" | "看看" | "想想"
                )
            };
            let has_container_localizer = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m1| {
                        !is_excluded_verb(&m1.word)
                            && dag
                                .get(&m1.end_idx)
                                .map(|after_m1| {
                                    after_m1.iter().any(|m2| {
                                        (m1.length() >= 2 && is_locative_postposition(&m2.word))
                                            || (!is_excluded_verb(&m2.word)
                                                && dag
                                                    .get(&m2.end_idx)
                                                    .map(|after_m2| {
                                                        after_m2
                                                            .iter()
                                                            .any(|m3| is_locative_postposition(&m3.word))
                                                    })
                                                    .unwrap_or(false))
                                    })
                                })
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if has_container_localizer {
                if matches!(curr_word, "在" | "不在") {
                    bonus += 28.0;
                } else {
                    bonus -= 24.0;
                }
            }
        }

        // ── 構式十一(b)：「在（進行體／處所介詞）」vs.「再（重複／承接／祈使／假設副詞）」三元語法結構雙向分流 ──
        //    核心原則：不依賴「在想、再想、在看、再看」等假雙字詞，純粹以 `[左語境] ➔ [在 / 再] ➔ [動詞 V] ➔ [右語境]` 語法槽位進行結構性判別！
        if matches!(curr_word, "在" | "再") {
            let is_progressive_left = matches!(
                prev_word,
                "我" | "你" | "妳" | "他" | "她" | "它" | "牠" | "我們" | "你們" | "他們"
                    | "大家" | "別人" | "有人" | "沒人" | "誰" | "心裡" | "腦中" | "正" | "正在"
                    | "還" | "都" | "一直" | "總" | "總是" | "依然" | "仍然" | "剛才" | "剛剛"
                    | "當時" | "那時" | "現在" | "目前" | "到底" | "究竟" | "其實" | "只是"
                    | "明明" | "居然" | "竟然"
            );
            let effective_prev = if matches!(prev_word, "，" | "," | "、" | "；" | ";") {
                prev2_word
            } else {
                prev_word
            };
            let is_iterative_left = matches!(
                effective_prev,
                "請" | "煩請" | "麻煩" | "先" | "後" | "然後" | "接著" | "才" | "就" | "又"
                    | "會" | "要" | "還要" | "想" | "想要" | "能" | "能夠" | "可以" | "應該"
                    | "必須" | "得" | "別" | "不要" | "不用" | "不必" | "無須" | "不" | "沒"
                    | "明天" | "改天" | "下次" | "稍後" | "等一下" | "晚點" | "回頭" | "事後"
                    | "之後" | "以後" | "重新" | "從頭" | "如何" | "怎麼" | "立刻" | "馬上"
                    | "趕快" | "趕緊" | "順便" | "不妨" | "歡迎"
            );
            // 檢查是否處於 `[再/在 + V] 與 [在/再 + V]` 或 `[再/在] 與 [在/再]` 對比並列構式（對比構式由構式十主導）
            let is_contrastive_coordination = prev_word == "與"
                || dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            m.word == "與"
                                || (m.length() == 1
                                    && dag
                                        .get(&m.end_idx)
                                        .map(|after_v| after_v.iter().any(|y| y.word == "與"))
                                        .unwrap_or(false))
                        })
                    })
                    .unwrap_or(false);

            if !is_contrastive_coordination {
                // 檢查緊接在「在/再」之後是否為指示代詞或處所名詞（如：在這個、在那個、在這裡、在那裡；副詞「再」絕不可直接修飾指示詞/處所名詞）
                let next_is_demonstrative_or_locative = dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "這個" | "那個" | "這些" | "那些" | "這" | "那" | "這裡" | "那裡"
                                    | "這邊" | "那邊" | "哪裡" | "哪邊" | "此" | "何處" | "家"
                                    | "學校" | "公司" | "現場" | "線上" | "網路" | "桌面" | "背景"
                                    | "後台" | "本地" | "雲端"
                            )
                        })
                    })
                    .unwrap_or(false);

                // 檢查緊接在「在/再」之後是否為後接嘗試/動量補語（一下、一次、一回、一遍、一趟、一點、看看、試試、想想）、天生重複/決策性動詞（想想、看看、試試、來、去、決定）或兼語致使動詞（讓、請、叫、派、換、交給）
                let next_has_iterative_cue = dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "想想" | "看看" | "試試" | "問問" | "找找" | "說說" | "算算"
                                    | "來" | "去" | "決定" | "讓" | "請" | "叫" | "派" | "換"
                                    | "交給"
                            ) || dag
                                .get(&m.end_idx)
                                .map(|after_ns| {
                                    after_ns.iter().any(|comp| {
                                        matches!(
                                            comp.word.as_str(),
                                            "一下" | "一次" | "一回" | "一遍" | "一趟" | "一點"
                                                | "一些" | "看看" | "試試" | "想想"
                                        )
                                    })
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                // 檢查緊接在「在/再」之後是否為可持續性認知／活動動詞
                let next_has_progressive_verb = dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "想" | "看" | "做" | "說" | "講" | "聽" | "寫" | "讀" | "吃"
                                    | "喝" | "睡" | "玩" | "笑" | "哭" | "等" | "找" | "忙"
                                    | "問" | "算" | "用" | "跑" | "走" | "學" | "教" | "買"
                                    | "賣" | "穿" | "唱" | "跳" | "畫" | "煮" | "洗" | "拍"
                                    | "聊" | "談" | "吵" | "鬧" | "考慮" | "思考" | "討論"
                                    | "研究" | "工作" | "上班" | "上課" | "開會" | "讀書"
                                    | "吃飯" | "睡覺" | "休息" | "聊天" | "運動" | "觀察"
                                    | "等待" | "尋找" | "處理" | "進行" | "使用" | "測試"
                                    | "說話" | "講話" | "看書" | "聽歌" | "開車" | "散步"
                                    | "洗澡" | "煮飯"
                            )
                        })
                    })
                    .unwrap_or(false);

                if next_is_demonstrative_or_locative {
                    if curr_word == "在" {
                        bonus += 28.0;
                    } else {
                        bonus -= 26.0;
                    }
                } else if is_iterative_left || next_has_iterative_cue {
                    if curr_word == "再" {
                        bonus += 26.0;
                    } else {
                        bonus -= 22.0;
                    }
                } else if is_progressive_left && next_has_progressive_verb {
                    if curr_word == "在" {
                        bonus += 26.0;
                    } else {
                        bonus -= 22.0;
                    }
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

        // ── 構式十三：正反疑問對稱構式 `[X] → [不] → [X]`（限定合法單音節謂詞，如：對不對、好不好、是不是、行不行、快不快）──
        let is_valid_a_not_a_pred = |w: &str| {
            matches!(
                w,
                "對" | "好" | "是" | "會" | "能" | "行" | "要" | "去" | "來"
                    | "懂" | "快" | "慢" | "多" | "少" | "大" | "小" | "真"
                    | "假" | "甜" | "辣" | "貴" | "遠" | "近" | "像" | "敢"
                    | "肯" | "該" | "算" | "信" | "愛" | "想" | "看" | "吃"
            )
        };
        if curr_word == "不" && is_valid_a_not_a_pred(prev_word) {
            let next_repeats_prev = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == prev_word))
                .unwrap_or(false);
            if next_repeats_prev {
                bonus += 16.0;
            }
        }
        if prev_word == "不" && prev2_word == curr_word && is_valid_a_not_a_pred(curr_word) {
            bonus += 20.0;
        }

        // ── 構式十四：同音詞彙與動補構式之語法槽位雙向分流（如：`[馬/車] → [騎] → [得] → [很好]` vs. `[什麼/一代] → [耆德] → [老人/長者]`）──
        if curr_word == "耆德" {
            let has_left_determiner = prefix_words.iter().rev().take(3).any(|&w| {
                matches!(
                    w,
                    "什麼" | "不是" | "並非" | "一代" | "鄉里" | "堪稱" | "尊為" | "這位" | "那位"
                )
            });
            let next_is_elder_noun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "老人" | "長者" | "老者" | "老人家" | "宿儒" | "碩望" | "賢達" | "前輩" | "耆老" | "風範"
                        )
                    })
                })
                .unwrap_or(false);
            let has_left_mount = prefix_words.iter().rev().take(3).any(|&w| {
                matches!(
                    w,
                    "馬" | "車" | "單車" | "腳踏車" | "機車" | "摩托車" | "鐵馬" | "重機"
                )
            });
            let next_is_degree = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "很" | "很好" | "很快" | "很穩" | "非常" | "太" | "真" | "超" | "好" | "快" | "穩" | "順" | "飽"
                        )
                    })
                })
                .unwrap_or(false);
            if has_left_determiner && next_is_elder_noun {
                bonus += 42.0;
            } else if next_is_elder_noun {
                bonus += 34.0;
            } else if has_left_mount || next_is_degree {
                bonus -= 42.0;
            }
        }
        if curr_word == "騎" {
            let has_left_mount = matches!(
                prev_word,
                "馬" | "車" | "單車" | "腳踏車" | "機車" | "摩托車" | "鐵馬" | "重機"
            );
            let next_has_de = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "得" | "到" | "飽")))
                .unwrap_or(false);
            if has_left_mount && next_has_de {
                bonus += 34.0;
            }
        }

        // ── 構式十四(b)：「孝道」vs.「笑 + 到 + 飽」之論元選擇限制與詞法包攝消歧（如：「他因為恪守孝道飽受抨擊，根本被笑到飽」）──
        let is_bao_subsumed_at = |boundary: usize| -> bool {
            dag.get(&boundary)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "飽受" | "飽經" | "飽嘗" | "飽嚐" | "飽含" | "飽覽" | "飽讀"
                        )
                    })
                })
                .unwrap_or(false)
        };
        let has_left_normative_verb = prefix_words.iter().rev().take(3).any(|&w| {
            matches!(
                w,
                "恪守" | "謹守" | "奉行" | "秉持" | "實踐" | "發揚" | "弘揚" | "講求" | "重視" | "違背" | "固守" | "盡"
            )
        });
        let has_left_passive_marker = prefix_words.iter().rev().take(2).any(|&w| {
            matches!(w, "被" | "讓人" | "被人" | "給人" | "害他" | "害我")
        });

        if curr_word == "孝道" {
            let next_is_baoshou = is_bao_subsumed_at(end_idx);
            let next_is_standalone_bao = !next_is_baoshou
                && dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|m| m.word == "飽"))
                    .unwrap_or(false);
            if has_left_normative_verb || next_is_baoshou {
                bonus += 42.0;
            } else if has_left_passive_marker || next_is_standalone_bao {
                bonus -= 45.0;
            }
        }
        if curr_word == "笑" {
            if has_left_normative_verb {
                bonus -= 42.0;
            } else if has_left_passive_marker {
                bonus += 34.0;
            }
        }

        // ── 構式十五：準分裂句名物化焦點判斷構式 `[(不/沒/最/只/所)? + X] → [的] → [是]` ──
        // 其中 X ∈ {選, 買, 賣, 看, 聽, 讀, 寫, 說, 講, 問, 要, 想, 做, 吃, 喝, 穿, 用, 找, 投, 盼, 爭, 怕, 愛...}
        // 例：「選舉選的是一個對治理方式的盼望」、「我們不買的是黑心商品」、「他要的是一個機會」
        if matches!(curr_word, "不" | "沒" | "最" | "只" | "所" | "部" | "步" | "簿") {
            let next_is_x_de = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|x| {
                        (x.length() == 1
                            && Self::is_pseudo_cleft_focus_verb(&x.word)
                            && dag
                                .get(&x.end_idx)
                                .map(|after_x| {
                                    after_x
                                        .iter()
                                        .any(|d| d.word == "的" || d.word == "的是")
                                })
                                .unwrap_or(false))
                            || (x.word.chars().count() == 2
                                && x.word.ends_with('的')
                                && Self::is_pseudo_cleft_focus_verb(
                                    &x.word.chars().next().unwrap().to_string(),
                                ))
                    })
                })
                .unwrap_or(false);
            if next_is_x_de {
                if matches!(curr_word, "不" | "沒" | "最" | "只" | "所") {
                    bonus += 28.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }
        let prev_is_x_de = prev_word.chars().count() == 2
            && prev_word.ends_with('的')
            && Self::is_pseudo_cleft_focus_verb(&prev_word.chars().next().unwrap().to_string());
        if curr_word == "的" && Self::is_pseudo_cleft_focus_verb(prev_word) {
            let next_has_shi = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "是" || m.word == "不是"))
                .unwrap_or(false);
            if next_has_shi {
                bonus += 28.0;
                if matches!(prev2_word, "不" | "沒" | "最" | "只" | "所" | "真") {
                    bonus += 4.0;
                }
            }
        }
        if (prev_word == "的" && Self::is_pseudo_cleft_focus_verb(prev2_word)) || prev_is_x_de {
            if matches!(curr_word, "是" | "不是") {
                bonus += 32.0;
                if matches!(prev3_word, "不" | "沒" | "最" | "只" | "所" | "真") {
                    bonus += 4.0;
                }
            } else if curr_word == "試" || curr_word.starts_with("試一") {
                bonus -= 34.0;
            }
        }
        if curr_word == "的是" && Self::is_pseudo_cleft_focus_verb(prev_word) {
            bonus += 30.0;
            if matches!(prev2_word, "不" | "沒" | "最" | "只" | "所" | "真") {
                bonus += 4.0;
            }
        }

        // ── 構式十六：現代漢語嵌偶對稱四字構式總表 (`[X 好 X 滿]`、`[又 X 又 Y]`、`[越 X 越 Y]`、`[不 X 不 Y]`、`[半 X 半 Y]`、`[自 X 自 Y]`) ──
        // 1. `[X 好 X 滿]`（如：做好做滿、買好買滿、吃好喝滿、說好說滿、看好看滿、寫好寫滿、領好領滿、充好充滿、睡好睡滿、玩好玩滿）
        let is_hao_man_verb = |w: &str| {
            matches!(
                w,
                "做" | "買" | "吃" | "喝" | "看" | "聽" | "讀" | "寫" | "說" | "講"
                    | "玩" | "睡" | "領" | "充" | "拿" | "拍" | "學" | "練" | "載" | "裝"
                    | "塞" | "補" | "用" | "開" | "坐" | "給" | "刷" | "存" | "收"
            )
        };
        if curr_word == "好" && is_hao_man_verb(prev_word) {
            let next_has_v_man = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        (m.word == prev_word || (prev_word == "吃" && m.word == "喝"))
                            && dag
                                .get(&m.end_idx)
                                .map(|after_v| after_v.iter().any(|k| k.word == "滿"))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if next_has_v_man {
                bonus += 28.0;
            }
        }
        if prev_word == "好"
            && (curr_word == prev2_word || (prev2_word == "吃" && curr_word == "喝"))
            && is_hao_man_verb(curr_word)
        {
            let next_is_man = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "滿"))
                .unwrap_or(false);
            if next_is_man {
                bonus += 30.0;
            }
        }
        if curr_word == "滿"
            && prev2_word == "好"
            && (prev_word == prev3_word || (prev3_word == "吃" && prev_word == "喝"))
            && is_hao_man_verb(prev_word)
        {
            bonus += 32.0;
        }
        // 當詞庫已將前兩字凝聚為雙字詞（如「做好」、「買好」、「吃好」、「寫好」）時，銜接「[X] + [滿]」並抑制雙字同音詞「坐滿 / 完滿」搶佔
        if prev_word == "做好" && curr_word == "坐滿" {
            bonus -= 38.0;
        }
        if prev_word == "玩好" && curr_word == "完滿" {
            bonus -= 38.0;
        }
        if prev_word.chars().count() == 2 && prev_word.ends_with('好') {
            let v0 = prev_word.chars().next().unwrap().to_string();
            if is_hao_man_verb(&v0) && (curr_word == v0 || (v0 == "吃" && curr_word == "喝")) {
                let next_is_man = dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|m| m.word == "滿"))
                    .unwrap_or(false);
                if next_is_man {
                    bonus += 34.0;
                }
            }
        }
        if curr_word == "滿"
            && prev2_word.chars().count() == 2
            && prev2_word.ends_with('好')
            && is_hao_man_verb(prev_word)
        {
            let v0 = prev2_word.chars().next().unwrap().to_string();
            if prev_word == v0 || (v0 == "吃" && prev_word == "喝") {
                bonus += 36.0;
            }
        }
        // 當「做好做滿」後緊接「做到飽」時，確保選「做 / 做到」而非「坐 / 坐到」
        if prev_word == "滿" && prev2_word == "做" {
            if curr_word == "做" || curr_word == "做到" {
                bonus += 28.0;
            } else if curr_word == "坐" || curr_word == "坐到" {
                bonus -= 28.0;
            }
        }

        // 2. `[越 X 越 Y]` / `[又 X 又 Y]` / `[半 X 半 Y]` / `[自 X 自 Y]` 嵌偶對稱構式（防「月/躍」、「右/幼」、「伴/辦」、「字/次」同音搶佔）
        if matches!(curr_word, "越" | "又" | "半" | "自")
            && prev2_word == curr_word
            && prev_word.chars().count() == 1
        {
            bonus += 28.0;
        }
        if matches!(curr_word, "月" | "躍" | "右" | "幼" | "伴" | "辦" | "字")
            && prev_word.chars().count() == 1
            && matches!(
                (prev2_word, curr_word),
                ("越", "月")
                    | ("越", "躍")
                    | ("又", "右")
                    | ("又", "幼")
                    | ("半", "伴")
                    | ("半", "辦")
                    | ("自", "字")
            )
        {
            bonus -= 30.0;
        }

        // ── 構式十七：《現代漢語八百詞》核心關聯呼應與框式構式總表 ──
        // 1. 全稱焦點隱含構式 `[連] + [NP/V] + [都/也]`（防「連」誤選「聯/憐」、「都」誤選「兜」）
        if matches!(curr_word, "都" | "也")
            && prefix_words.iter().rev().take(4).any(|&w| w == "連" || w.starts_with('連'))
        {
            bonus += 26.0;
        } else if curr_word == "兜"
            && prefix_words.iter().rev().take(4).any(|&w| w == "連" || w.starts_with('連'))
        {
            bonus -= 30.0;
        }
        // 2. 平比／異同構式 `[跟/和/與/同/像] + [NP] + [一樣/相同/差不多/不同/相反]`
        if matches!(
            curr_word,
            "一樣" | "相同" | "差不多" | "不同" | "相反" | "類似" | "相近"
        ) && prefix_words
            .iter()
            .rev()
            .take(4)
            .any(|&w| matches!(w, "跟" | "和" | "與" | "同" | "像" | "好像" | "就像"))
        {
            bonus += 24.0;
        }
        // 3. 條件／讓步／必要關聯呼應構式：
        //    - `[只有/唯有/除非/直到] ... [才]`（防「財/材/裁」搶佔）
        //    - `[只要/一旦/既然/如果/要是] ... [就]`（防「舊/救/舅」搶佔）
        //    - `[雖然/儘管/就算/即使/固然] ... [卻/但/還是]`（防「確/雀」搶佔）
        if matches!(curr_word, "才" | "財" | "材" | "裁")
            && prefix_words
                .iter()
                .rev()
                .take(5)
                .any(|&w| matches!(w, "只有" | "唯有" | "除非" | "直到" | "必須" | "要"))
        {
            if curr_word == "才" {
                bonus += 26.0;
            } else {
                bonus -= 28.0;
            }
        }
        if matches!(curr_word, "卻" | "確" | "雀")
            && prefix_words
                .iter()
                .rev()
                .take(5)
                .any(|&w| matches!(w, "雖然" | "雖" | "儘管" | "固然" | "就算" | "即使" | "明明" | "反而"))
        {
            if curr_word == "卻" {
                bonus += 28.0;
            } else {
                bonus -= 28.0;
            }
        }

        // ── 構式十八：中研院 Sinica 動詞論元結構矩陣 (VE/VF/VD) 與 Li & Thompson 長句拓撲序列 ──
        // 1. Li & Thompson 跨狀語拓撲預判：`[施事主語] + [時間/處所狀語 (昨天/剛才/以前/今天...)] + [(不)X] + [的] + [是]`
        let is_temporal_adverbial = |w: &str| {
            matches!(
                w,
                "昨天" | "今天" | "明天" | "剛才" | "剛剛" | "以前" | "過去" | "之前" | "上次"
                    | "這次" | "當時" | "那時" | "本來" | "原本" | "一直" | "平常" | "最後"
            )
        };
        if is_temporal_adverbial(prev_word) && Self::is_pseudo_cleft_focus_verb(curr_word) {
            let next_has_de_shi = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|d| {
                        d.word == "的是"
                            || (d.word == "的"
                                && dag
                                    .get(&d.end_idx)
                                    .map(|after_d| {
                                        after_d
                                            .iter()
                                            .any(|s| s.word == "是" || s.word == "不是")
                                    })
                                    .unwrap_or(false))
                    })
                })
                .unwrap_or(false);
            if next_has_de_shi {
                bonus += 28.0;
            }
        }
        // 2. Sinica CKIP `VE`（稱言／認知／感知句賓動詞）後接子句主語或功能助動詞引力：
        let is_sinica_ve_verb = |w: &str| {
            matches!(
                w,
                "覺得" | "認為" | "發現" | "知道" | "希望" | "建議" | "表示" | "擔心" | "懷疑"
                    | "決定" | "相信" | "聽說" | "記得" | "忘記" | "確定" | "確認" | "證明"
                    | "說明" | "宣稱" | "強調" | "指出" | "承認" | "否認"
            )
        };
        if is_sinica_ve_verb(prev_word)
            && matches!(
                curr_word,
                "我" | "你" | "他" | "她" | "它" | "我們" | "你們" | "他們" | "這" | "那"
                    | "這個" | "那個" | "是" | "不是" | "會" | "不會" | "能" | "不能"
                    | "可以" | "應該" | "要" | "不要" | "很" | "太" | "更" | "最" | "都"
                    | "已經" | "還" | "再" | "在"
            )
        {
            bonus += 18.0;
        }
        // 3. Sinica CKIP `VF`（謂賓動詞）後接動作謂詞槽位引力：
        let is_sinica_vf_verb = |w: &str| {
            matches!(
                w,
                "喜歡" | "開始" | "繼續" | "準備" | "打算" | "嘗試" | "試圖" | "停止" | "避免"
                    | "負責" | "擅長" | "習慣" | "樂意" | "願意" | "拒絕" | "放棄" | "堅持"
            )
        };
        if is_sinica_vf_verb(prev_word)
            && (Self::is_pseudo_cleft_focus_verb(curr_word)
                || Self::is_repeatable_action_verb(curr_word))
        {
            bonus += 20.0;
        }

        // ── 構式十九：現代漢語同音易混字組之跨距框式介詞、時間量詞與論元槽位分流總表 (`C04` ~ `C21`) ──
        // 1. `C04` 狀中助詞「地」vs. 定中助詞「的」分流（如：「開心地笑了」vs.「開心的事」）：
        let prev_is_manner_adj = matches!(
            prev_word,
            "開心" | "高興" | "快樂" | "傷心" | "難過" | "興奮" | "緊張" | "得意" | "驕傲"
                | "安靜" | "冷靜" | "禮貌" | "溫柔" | "熱情" | "大聲" | "小聲" | "輕聲"
                | "用力" | "努力" | "認真" | "仔細" | "小心" | "偷偷" | "悄悄" | "慢慢"
                | "漸漸" | "默默" | "靜靜" | "緊緊" | "輕輕" | "深深" | "遠遠" | "早早"
                | "乖乖" | "好好" | "微笑" | "生氣" | "憤怒" | "驚訝" | "好奇"
        );
        if prev_is_manner_adj && matches!(curr_word, "地" | "的") && curr_bopomofo.starts_with("ㄉㄜ") {
            let next_is_manner_verb = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "笑" | "笑了" | "哭" | "哭了" | "說" | "說道" | "走" | "走了" | "跑"
                                | "跳" | "看" | "看著" | "聽" | "聽著" | "點頭" | "搖頭" | "揮手"
                                | "離開" | "回答" | "問" | "喊" | "叫" | "唱" | "吃"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_event_noun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "事" | "事情" | "人" | "時候" | "日子" | "時光" | "笑容" | "樣子"
                                | "模樣" | "表情" | "心情" | "消息" | "結局" | "回憶" | "地方"
                                | "東西" | "生活"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_manner_verb {
                if curr_word == "地" {
                    bonus += 34.0;
                } else {
                    bonus -= 28.0;
                }
            } else if next_is_event_noun {
                if curr_word == "的" {
                    bonus += 30.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 2. `C05` 框式介詞構式 `[以] + [1~3個詞元] + [為由/為主/為例/為準/為限/為榮/為恥/為生/來看/而言]` vs. 時間完成副詞 `[已] + [離開/完成...]`：
        if matches!(curr_word, "以" | "已") {
            let is_wei_frame_closer = |w: &str| {
                matches!(
                    w,
                    "為由" | "為主" | "為例" | "為準" | "為限" | "為榮" | "為恥" | "為生"
                        | "為傲" | "為首" | "為題" | "為名" | "為戒" | "為重" | "為念"
                        | "為基礎" | "為中心" | "為目標" | "為前提" | "來看" | "而言" | "來說"
                )
            };
            let has_yi_wei_frame = dag
                .get(&end_idx)
                .map(|n1_list| {
                    n1_list.iter().any(|n1| {
                        is_wei_frame_closer(&n1.word)
                            || dag
                                .get(&n1.end_idx)
                                .map(|n2_list| {
                                    n2_list.iter().any(|n2| {
                                        is_wei_frame_closer(&n2.word)
                                            || (n2.word == "為"
                                                && dag
                                                    .get(&n2.end_idx)
                                                    .map(|n3_list| {
                                                        n3_list.iter().any(|n3| {
                                                            matches!(
                                                                n3.word.as_str(),
                                                                "由" | "主" | "例" | "準" | "限" | "榮" | "恥" | "生" | "首"
                                                            )
                                                        })
                                                    })
                                                    .unwrap_or(false))
                                            || dag
                                                .get(&n2.end_idx)
                                                .map(|n3_list| {
                                                    n3_list.iter().any(|n3| is_wei_frame_closer(&n3.word))
                                                })
                                                .unwrap_or(false)
                                    })
                                })
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            let next_is_telic_verb = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "離開" | "完成" | "結束" | "成為" | "進入" | "超過" | "抵達" | "收到"
                                | "決定" | "通過" | "下班" | "出發" | "去世" | "結婚" | "畢業"
                        )
                    })
                })
                .unwrap_or(false);
            if has_yi_wei_frame {
                if curr_word == "以" {
                    bonus += 38.0;
                } else {
                    bonus -= 34.0;
                }
            } else if next_is_telic_verb {
                if curr_word == "已" {
                    bonus += 28.0;
                } else {
                    bonus -= 24.0;
                }
            }
        }

        // 3. `C06` 疑問代詞「哪裡 (`ㄋㄚˇ ㄌㄧˇ`)」vs. 遠指代詞「那裡 (`ㄋㄚˋ ㄌㄧˇ`)」（壓抑教育部古破音「那 `ㄋㄚˇ`」干擾現代疑問句）：
        if matches!(curr_word, "那" | "那裡" | "那邊" | "那兒") && curr_bopomofo.starts_with("ㄋㄚˇ") {
            bonus -= 38.0;
        }
        if matches!(curr_word, "哪" | "哪裡" | "哪邊" | "哪兒") && curr_bopomofo.starts_with("ㄋㄚˇ") {
            bonus += 20.0;
        }

        // 4. `C07` 時間量詞跨界防劫掠構式：`[數詞 (兩/一/三/半/幾...)] + [小時] + [後]` vs. 單稱名詞 `[小時候]`：
        if WordPattern::is_numeric_token(prev_word) {
            if curr_word == "小時候" {
                bonus -= 65.0;
            } else if curr_word == "小時" {
                bonus += 32.0;
            }
        }
        if prev_word == "小時" && WordPattern::is_numeric_token(prev2_word) {
            if curr_word == "後" {
                bonus += 32.0;
            } else if curr_word == "候" {
                bonus -= 35.0;
            }
        }

        // 5. `C08` 「做 + 具體事作賓語（做功課、做家事）」vs.「作 + 文言/雙音節詞素（作文課、作業、作品）」：
        if matches!(curr_word, "做" | "作") {
            let next_is_zuo_concrete = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "功課" | "家事" | "工作" | "生意" | "運動" | "決定" | "準備" | "選擇"
                                | "筆記" | "實驗" | "手術" | "夢" | "飯" | "菜"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_zuo_literary = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "文" | "文課" | "業" | "品" | "家" | "曲" | "詞" | "戰" | "弊" | "廢" | "祟" | "證"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_zuo_concrete {
                if curr_word == "做" {
                    bonus += 30.0;
                } else {
                    bonus -= 28.0;
                }
            } else if next_is_zuo_literary {
                if curr_word == "作" {
                    bonus += 30.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 6. `C09` 情態副詞「必須 + 謂詞（必須去）」vs. 非謂形容詞「必需 + 名詞後綴（必需品）」：
        if matches!(curr_word, "必須" | "必需") {
            let next_is_pin_or_de = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "品" | "的" | "物" | "營養")))
                .unwrap_or(false);
            if next_is_pin_or_de {
                if curr_word == "必需" {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            } else {
                if curr_word == "必須" {
                    bonus += 26.0;
                } else {
                    bonus -= 26.0;
                }
            }
        }

        // 7. `C10` 穿戴動詞「戴 + 穿戴物 + (出門/上班)」vs. 攜帶給予雙賓/代勞構式「(幫我/替我/順便) + 帶 + 穿戴物 + (給我/給你/給他/來/去/走)」：
        if matches!(curr_word, "戴" | "帶") {
            let is_wearable = |w: &str| {
                matches!(
                    w,
                    "口罩" | "眼鏡" | "帽子" | "安全帽" | "耳機" | "手錶" | "戒指" | "項鍊"
                        | "手套" | "圍巾" | "領帶" | "隱形眼鏡" | "頭盔" | "護目鏡"
                )
            };
            let is_delivery_complement = |w: &str| {
                matches!(
                    w,
                    "給我" | "給你" | "給他" | "給她" | "給我們" | "給" | "來" | "去" | "走"
                        | "回" | "回來" | "回去" | "進來" | "出去"
                )
            };
            let has_fetching_prefix = prefix_words.iter().rev().take(3).any(|&w| {
                matches!(
                    w,
                    "幫" | "幫我" | "幫你" | "幫他" | "替" | "替我" | "順便" | "多" | "忘記" | "忘了"
                )
            });
            if let Some(next_nodes) = dag.get(&end_idx) {
                let mut has_wearable_delivery = false;
                let mut has_wearable_only = false;
                for m in next_nodes {
                    if is_wearable(&m.word) {
                        let followed_by_delivery = dag
                            .get(&m.end_idx)
                            .map(|after_m| {
                                after_m.iter().any(|k| {
                                    is_delivery_complement(&k.word) || k.word.starts_with('給')
                                })
                            })
                            .unwrap_or(false);
                        if followed_by_delivery || has_fetching_prefix {
                            has_wearable_delivery = true;
                        } else {
                            has_wearable_only = true;
                        }
                    }
                }
                if has_wearable_delivery {
                    if curr_word == "帶" {
                        bonus += 36.0;
                    } else {
                        bonus -= 32.0;
                    }
                } else if has_wearable_only {
                    if curr_word == "戴" {
                        bonus += 32.0;
                    } else {
                        bonus -= 28.0;
                    }
                }
            }
        }

        // 8. `C11` 句末疑問助詞「嗎」vs. 顯然確認/無奈語氣助詞「嘛」（如：「這樣好嗎」vs.「這也沒辦法嘛 / 就說了嘛」）：
        if matches!(curr_word, "嗎" | "嘛") {
            let has_obvious_modal_left = matches!(
                prev_word,
                "說了" | "就說了" | "算了" | "好吧" | "對吧" | "沒辦法" | "辦法" | "也沒辦法"
            ) || (prev_word == "了" && matches!(prev2_word, "說" | "就說"))
                || prefix_words.iter().rev().take(4).any(|&w| {
                    matches!(
                        w,
                        "就說" | "本來就" | "明明就" | "沒辦法" | "也沒" | "這也"
                    )
                });
            if has_obvious_modal_left {
                if curr_word == "嘛" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 9. `C14` 伴隨介詞「跟 + 人稱代詞（跟我來）」vs. 副詞「根本（根本不是）」：
        if matches!(curr_word, "跟" | "根") {
            let next_is_pronoun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "我" | "你" | "他" | "她" | "我們" | "你們" | "他們" | "大家" | "誰"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_pronoun {
                if curr_word == "跟" {
                    bonus += 34.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 10. `C15` 處所名詞「心裡（心裡有數）」vs. 屬性名詞「心理（心理學）」：
        if matches!(curr_word, "心裡" | "心理") {
            let next_is_locative_pred = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "有數" | "有底" | "有鬼" | "有事" | "有" | "想" | "難過" | "高興"
                                | "清楚" | "明白" | "不安" | "害怕" | "很" | "卻" | "都" | "也"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_psych_noun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "學" | "學家" | "醫生" | "醫師" | "師" | "諮商" | "輔導" | "治療"
                                | "準備" | "狀態" | "壓力" | "健康" | "建設" | "素質" | "戰"
                                | "測驗" | "陰影" | "創傷" | "作用" | "因素"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_locative_pred {
                if curr_word == "心裡" {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            } else if next_is_psych_noun {
                if curr_word == "心理" {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }

        // 11. `C16` 言談傳達及物動詞「反映（向主管反映問題）」vs. 刺激回應名詞/不及物動詞「反應（他的反應很快）」：
        if matches!(curr_word, "反映" | "反應") {
            let has_report_left = prefix_words
                .iter()
                .rev()
                .take(3)
                .any(|&w| matches!(w, "向" | "對" | "跟" | "替" | "如實" | "向上"));
            let next_is_report_obj = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "問題" | "意見" | "情況" | "實情" | "心聲" | "需求" | "民意" | "現況" | "事實" | "出來"
                        )
                    })
                })
                .unwrap_or(false);
            let has_reaction_left = prefix_words.iter().rev().take(3).any(|&w| {
                matches!(
                    w,
                    "他的" | "她的" | "我的" | "你的" | "身體" | "市場" | "第一" | "直覺"
                        | "過敏" | "化學" | "連鎖" | "不良" | "本能" | "沒"
                )
            });
            let next_is_reaction_pred = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "很快" | "很慢" | "快" | "慢" | "熱烈" | "激烈" | "過度" | "冷淡"
                                | "兩極" | "不及" | "過來" | "時間" | "速度" | "爐" | "很"
                        )
                    })
                })
                .unwrap_or(false);
            if has_report_left || next_is_report_obj {
                if curr_word == "反映" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            } else if has_reaction_left || next_is_reaction_pred {
                if curr_word == "反應" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 12. `C17` 法律道德權益「權利（人民有投票的權利）」vs. 政治支配力量「權力（總統的權力很大）」：
        if matches!(curr_word, "權利" | "權力") {
            let has_rights_ctx = prefix_words.iter().rev().take(5).any(|&w| {
                matches!(
                    w,
                    "人民" | "公民" | "選民" | "勞工" | "民眾" | "個人" | "兒童" | "消費者"
                        | "人權" | "基本" | "投票" | "選舉" | "罷免" | "言論" | "受教" | "生存"
                        | "工作" | "隱私" | "著作" | "專利" | "財產" | "享有" | "保障" | "維護"
                        | "爭取" | "放棄" | "剝奪" | "侵害" | "主張" | "義務"
                )
            });
            let has_power_ctx = prefix_words.iter().rev().take(5).any(|&w| {
                matches!(
                    w,
                    "總統" | "政府" | "國家" | "元首" | "國王" | "皇帝" | "君主" | "獨裁"
                        | "國會" | "法院" | "警察" | "主管" | "官員" | "高層" | "政治" | "行政"
                        | "立法" | "司法" | "公" | "掌握" | "行使" | "濫用" | "集中" | "下放"
                        | "制衡" | "移交" | "分立" | "鬥爭" | "核心" | "中樞"
                )
            }) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "很大" | "過大" | "太大" | "傲慢" | "腐敗" | "核心" | "制衡" | "鬥爭"
                        )
                    })
                })
                .unwrap_or(false);
            if has_rights_ctx && !has_power_ctx {
                if curr_word == "權利" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            } else if has_power_ctx && !has_rights_ctx {
                if curr_word == "權力" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 13. `C18` 名詞「紀錄（打破世界紀錄）」vs. 動詞「記錄（請記錄下來）」：
        if matches!(curr_word, "紀錄" | "記錄") {
            let has_record_noun_ctx = prefix_words.iter().rev().take(4).any(|&w| {
                matches!(
                    w,
                    "打破" | "創下" | "刷新" | "保持" | "締造" | "破" | "創" | "世界" | "全國"
                        | "奧運" | "歷史" | "大會" | "最高" | "最低" | "最新" | "最佳" | "個人"
                        | "生涯" | "不良" | "犯罪" | "前科" | "出勤" | "通話" | "瀏覽" | "交易"
                        | "醫療" | "觀看"
                )
            }) || dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "片" | "保持人")))
                .unwrap_or(false);
            let has_record_verb_ctx = prefix_words.iter().rev().take(3).any(|&w| {
                matches!(
                    w,
                    "請" | "先" | "再" | "會" | "要" | "能" | "已" | "詳細" | "忠實" | "完整"
                        | "清楚" | "隨時" | "據實" | "幫忙" | "負責" | "自動" | "手動" | "親自"
                )
            }) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "下來" | "下" | "著" | "了" | "過" | "過程" | "點滴" | "生活" | "數據" | "資料"
                        )
                    })
                })
                .unwrap_or(false);
            if has_record_noun_ctx && !has_record_verb_ctx {
                if curr_word == "紀錄" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            } else if has_record_verb_ctx && !has_record_noun_ctx {
                if curr_word == "記錄" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 14. `C20` 距離趨近「走近 + 數量/觀察補語（走近一點、走近一看）」vs. 空間進入「走進 + 處所名詞（走進房間、走進教室）」：
        if matches!(curr_word, "走近" | "走進") || (prev_word == "走" && matches!(curr_word, "近" | "進")) {
            let next_is_proximity_comp = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "一點" | "一些" | "一看" | "細看" | "瞧瞧" | "看看" | "身邊" | "身旁"
                                | "他" | "她" | "我" | "你"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_enclosure_noun = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "房間" | "教室" | "大廳" | "辦公室" | "會場" | "店裡" | "家門" | "大門"
                                | "心裡" | "歷史" | "世界" | "廚房" | "客廳" | "臥室" | "電梯"
                                | "車廂" | "森林" | "巷子" | "校園" | "社會" | "去" | "來"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_proximity_comp {
                if matches!(curr_word, "走近" | "近") {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            } else if next_is_enclosure_noun {
                if matches!(curr_word, "走進" | "進") {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }

        // ── 動態外掛三元槽位構式 (`patterns_user.jsonl` 中 slots.len() >= 3) ──
        for pat in &self.user_trigram_patterns {
            if !prev_word.is_empty()
                && pat.slot0.contains(prev_word)
                && pat.slot1.contains(curr_word)
                && !self
                    .excluded_pairs
                    .contains(&(prev_word.to_string(), curr_word.to_string()))
            {
                let next_subsumed = pat.name == "v_dao_bao_satiation_3d_array" && is_bao_subsumed_at(end_idx);
                let next_matches = !next_subsumed
                    && dag
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
                && !self
                    .excluded_pairs
                    .contains(&(prev2_word.to_string(), prev_word.to_string()))
                && !self
                    .excluded_pairs
                    .contains(&(prev_word.to_string(), curr_word.to_string()))
            {
                let curr_subsumed = pat.name == "v_dao_bao_satiation_3d_array"
                    && curr_word == "飽"
                    && end_idx >= 1
                    && is_bao_subsumed_at(end_idx - 1);
                if !curr_subsumed {
                    if pat.slot2.contains(curr_word) {
                        bonus += pat.bonus;
                    } else if pat.name == "v_dao_bao_satiation_3d_array"
                        && matches!(curr_word, "保" | "寶" | "堡" | "葆")
                    {
                        bonus -= pat.bonus;
                    }
                }
            }
            // 當 slot0 + slot1 在詞庫中已凝聚為雙字詞（如「笑到」、「看到」、「吃到」）時，與後繼 slot2（如「飽」）共同構成完整三元構式
            if curr_word.chars().count() == 2 {
                let mut chars = curr_word.chars();
                let c0 = chars.next().unwrap().to_string();
                let c1 = chars.next().unwrap().to_string();
                if pat.slot0.contains(&c0) && pat.slot1.contains(&c1) {
                    let next_subsumed = pat.name == "v_dao_bao_satiation_3d_array" && is_bao_subsumed_at(end_idx);
                    let next_matches = !next_subsumed
                        && dag
                            .get(&end_idx)
                            .map(|ns| ns.iter().any(|m| pat.slot2.contains(&m.word)))
                            .unwrap_or(false);
                    if next_matches {
                        bonus += pat.bonus;
                    }
                }
            }
            if prev_word.chars().count() == 2 && pat.slot2.contains(curr_word) {
                let mut chars = prev_word.chars();
                let p0 = chars.next().unwrap().to_string();
                let p1 = chars.next().unwrap().to_string();
                let curr_subsumed = pat.name == "v_dao_bao_satiation_3d_array"
                    && curr_word == "飽"
                    && end_idx >= 1
                    && is_bao_subsumed_at(end_idx - 1);
                if !curr_subsumed && pat.slot0.contains(&p0) && pat.slot1.contains(&p1) {
                    bonus += pat.bonus;
                }
            }
        }

        // 21. `C01` 處所介詞構式 `[在] + [家] + [休息/睡覺/工作/上班/讀書/吃飯/看書]` vs. 累加構式 `[再] + [加] + [數量/名詞]`：
        if matches!(curr_word, "在家" | "再加")
            || (matches!(prev_word, "在" | "再") && matches!(curr_word, "家" | "加"))
        {
            let next_is_home_activity = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "休息" | "睡覺" | "工作" | "上班" | "讀書" | "吃飯" | "看書"
                                | "煮飯" | "帶小孩" | "待著" | "陪" | "裡" | "中"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_home_activity {
                if curr_word == "在家" || (prev_word == "在" && curr_word == "家") {
                    bonus += 32.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 22. `C05` 第二人稱代詞主語 `[你] + [去/來/在/有/是/要/想/看/說/做]` vs. 古雅動詞「擬去」：
        if curr_word == "擬去" {
            bonus -= 45.0;
        } else if matches!(curr_word, "你" | "擬") {
            let next_is_common_pred = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "去" | "來" | "在" | "有" | "是" | "要" | "想" | "看" | "說" | "做"
                                | "走" | "吃" | "喝" | "睡" | "買" | "知" | "知道" | "覺得"
                                | "好" | "呢" | "嗎"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_common_pred {
                if curr_word == "你" {
                    bonus += 28.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 23. `C12` 姿態處所構式 `[坐在] / [坐] + [在] + [椅子/沙發/座位/地上/床/旁邊]` vs. 誤字「作在/做在/座在」：
        if curr_word == "坐在" {
            bonus += 34.0;
        } else if matches!(curr_word, "坐" | "作" | "做" | "座") {
            let next_is_zai_loc = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "在" || m.word.starts_with('在')))
                .unwrap_or(false);
            if next_is_zai_loc {
                if curr_word == "坐" {
                    bonus += 32.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 24. `C13` 推測情態構式 `[好像/似乎/快/將] + [要] + [下雨/下/去/來/走/開始]` vs. 名詞「藥」：
        if matches!(curr_word, "要" | "藥") && matches!(prev_word, "好像" | "似乎" | "快" | "將" | "正" | "還" | "都" | "也" | "又") {
            let next_is_vp = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "下雨" | "下雪" | "下" | "去" | "來" | "走" | "開始" | "結束" | "出發"
                                | "睡" | "睡覺" | "吃" | "吃飯" | "看" | "買" | "做" | "說"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_vp {
                if curr_word == "要" {
                    bonus += 32.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }

        // 25. `C17` 對象介詞構式 `[向] + [主管/老師/老闆/大家/政府/法院/客戶/他/你/我] + [反映/報告/說明/道歉/請教/學習/提出]` vs. 量詞「項」：
        if matches!(curr_word, "向" | "項" | "像" | "相") {
            let next_is_target_person_or_org = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "主管" | "老師" | "老闆" | "長官" | "警察" | "法院" | "政府" | "大家"
                                | "客戶" | "公司" | "學校" | "外界" | "社會" | "大眾" | "媒體"
                                | "父母" | "朋友" | "對方" | "他" | "她" | "你" | "您" | "我" | "我們"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_target_person_or_org && !WordPattern::is_numeric_token(prev_word) && !matches!(prev_word, "這" | "那" | "哪" | "每" | "各") {
                if curr_word == "向" {
                    bonus += 34.0;
                } else {
                    bonus -= 28.0;
                }
            }
        }

        // 26. `C20` 程度肯斷副詞 `[絕對] + [沒/沒有/沒問題/不/不會/不能/不是/是/會/能/可以]` vs. 碎拆「覺 + 對」：
        if curr_word == "絕對" {
            let next_is_modal_or_neg = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "沒" | "沒有" | "沒問題" | "不" | "不會" | "不能" | "不可" | "不可能"
                                | "不是" | "是" | "會" | "能" | "可以" | "好" | "對" | "行" | "要"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_modal_or_neg {
                bonus += 34.0;
            }
        } else if prev_word == "覺" && curr_word == "對" {
            bonus -= 32.0;
        }

        // ── 構式二十：實戰長句高歧義構式精準消歧（261021 使用者極端造句集） ──
        // 27. 篇章指稱與語句傳遞構式：`[以下/上述/下列/這段/這些/那段/幾句] + [文句] + [我/你/直接/貼/寫/輸入/修改]` vs. 實體辦公用品「文具」：
        if matches!(curr_word, "文句" | "文具") {
            let prev_is_discourse_mod = matches!(
                prev_word,
                "以下" | "上述" | "下列" | "這段" | "那段" | "這些" | "那些" | "幾句" | "這句"
                    | "那句" | "本段" | "極端" | "測試" | "輸入"
            );
            let next_is_discourse_pred = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "我" | "你" | "您" | "我們" | "還是" | "直接" | "貼" | "貼給" | "寫"
                                | "輸入" | "修改" | "分析" | "結構" | "通順" | "語意"
                        )
                    })
                })
                .unwrap_or(false);
            let has_stationery_cue = matches!(prev_word, "買" | "購買" | "賣" | "批發" | "辦公室" | "學生")
                || dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "店" | "盒" | "行" | "控" | "用品" | "專賣" | "清單" | "列表"
                                    | "目錄" | "採購" | "一覽" | "筆" | "紙" | "類"
                            )
                        })
                    })
                    .unwrap_or(false);
            if has_stationery_cue {
                if curr_word == "文具" {
                    bonus += 34.0;
                } else {
                    bonus -= 28.0;
                }
            } else if prev_is_discourse_mod || next_is_discourse_pred {
                if curr_word == "文句" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 28. 媒體登台動賓構式 `[不斷/經常/頻頻/一直/常/去/來] + [上] + [節目/通告/新聞/鏡頭/熱搜/頭條/台/場/線/課/班/網/市]` vs. 副詞「尚」：
        if matches!(curr_word, "上" | "尚") {
            let next_is_stage_or_media_np = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "節目" | "通告" | "新聞" | "鏡頭" | "熱搜" | "頭條" | "版面" | "台"
                                | "場" | "線" | "課" | "班" | "網" | "市" | "學" | "街" | "路"
                                | "演" | "映" | "架" | "傳" | "載"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_stage_or_media_np {
                if curr_word == "上" {
                    bonus += 36.0;
                } else {
                    bonus -= 34.0;
                }
            }
        }

        // 29. 極性程度補語構式 `[動詞：跌/吃/玩/看/睡/笑/哭/買/罵/聊/喝/唱/跳/說] + [個夠 / 個痛快 / 個飽]` 與指標漲跌主謂構式 `[民調/股價/房價/匯率/行情] ... ➔ [跌]` vs. 器物名詞「碟」：
        if matches!(curr_word, "跌" | "碟" | "疊" | "蝶") {
            let next_is_extreme_complement = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "個夠" | "個痛快" | "個飽" | "破" | "停" | "幅" | "落" | "入" | "谷"
                                | "倒" | "傷" | "價" | "勢" | "宕"
                        ) || (m.word == "個"
                            && dag
                                .get(&m.end_idx)
                                .map(|after_ge| {
                                    after_ge.iter().any(|g| matches!(g.word.as_str(), "夠" | "痛快" | "飽"))
                                })
                                .unwrap_or(false))
                    })
                })
                .unwrap_or(false);
            let has_metric_subject = prefix_words.iter().rev().take(6).any(|&w| {
                matches!(
                    w,
                    "民調" | "股價" | "股市" | "房價" | "物價" | "油價" | "金價" | "匯率" | "指數"
                        | "市值" | "支持度" | "聲勢" | "行情" | "成績" | "氣溫" | "溫度"
                )
            });
            if next_is_extreme_complement || has_metric_subject {
                if curr_word == "跌" {
                    bonus += 38.0;
                } else {
                    bonus -= 36.0;
                }
            }
        }

        // 30. 數位社群觸控動賓構式 `[滑] + [脆/手機/平板/螢幕/社群/動態/臉書/貼文/網頁/短片/影片]` vs. 專名/形容詞「華 / 猾」：
        if curr_word == "滑脆" {
            bonus += 36.0;
        } else if matches!(curr_word, "滑" | "華" | "譁" | "猾") {
            let next_is_digital_feed = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "脆" | "手機" | "平板" | "螢幕" | "社群" | "動態" | "臉書" | "貼文"
                                | "網頁" | "短片" | "影片" | "到" | "過" | "動" | "鼠"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_digital_feed {
                if curr_word == "滑" {
                    bonus += 36.0;
                } else {
                    bonus -= 34.0;
                }
            }
        }

        // 31. `2+2` 雙音複合詞跨界正交保護：`[另一個/這個/那個/一個/本] + [對話] + [分析/處理/評估/審核/討論/紀錄/視窗]` vs. 錯位跨界搶佔「對(1) + 劃分(2) + 吸(1)」：
        if curr_word == "對話" {
            let prev_is_det = matches!(
                prev_word,
                "一個" | "另一個" | "這個" | "那個" | "哪個" | "每個" | "本" | "進行" | "開啟" | "這段"
            );
            let next_is_dialog_pred = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "分析" | "處理" | "評估" | "審核" | "討論" | "紀錄" | "視窗" | "方塊"
                                | "框" | "內容" | "摘要" | "模型" | "系統" | "中"
                        )
                    })
                })
                .unwrap_or(false);
            if prev_is_det || next_is_dialog_pred {
                bonus += 38.0;
            }
        } else if prev_word == "對話" && curr_word == "分析" {
            bonus += 36.0;
        } else if prev_word == "對" && curr_word == "劃分" {
            let next_is_broken_xi = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(m.word.as_str(), "吸" | "析" | "西" | "希" | "惜" | "息" | "悉" | "昔")
                    })
                })
                .unwrap_or(false);
            if next_is_broken_xi {
                bonus -= 46.0;
            }
        }

        // 32. 起點介詞框架 `[從] + [已有/既有/現有/文獻/基礎/頭/零/小/此/現在/今天/過去/古/中/內/外/前/後/上/下]` vs. 量詞/名詞「叢」：
        if matches!(curr_word, "從" | "叢" | "匆" | "聰" | "蔥") {
            let prev_is_clause_boundary = prev_word.is_empty()
                || matches!(prev_word, "，" | "。" | "；" | "！" | "？" | "," | "." | ";" | "先" | "再" | "並" | "而" | "且" | "是" | "要" | "能" | "會");
            let next_is_source_np = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "已有" | "既有" | "現有" | "過去" | "傳統" | "文獻" | "基礎" | "頭"
                                | "零" | "小" | "此" | "現在" | "今天" | "昨天" | "明天" | "以前"
                                | "那時" | "這裡" | "那裡" | "哪裡" | "中" | "內" | "外" | "上"
                                | "下" | "前" | "後" | "來" | "未" | "不" | "沒" | "事" | "政"
                        )
                    })
                })
                .unwrap_or(false);
            let prev_is_numeral = WordPattern::is_numeric_token(prev_word)
                || matches!(prev_word, "一" | "兩" | "幾" | "數" | "草" | "花" | "樹" | "灌木");
            if (prev_is_clause_boundary || next_is_source_np) && !prev_is_numeral {
                if curr_word == "從" {
                    bonus += 36.0;
                } else if curr_word == "叢" {
                    bonus -= 38.0;
                }
            }
        }

        // 33. 雙音動詞動量黏著 `[分享/討論/介紹/解釋/說明/研究/分析/檢查/測試/確認/體驗/展示] + [一下]`、跨語種 ASCII 領屬助詞 `[<ASCII Identifier>] + [的] + [最新/成果/NP]` 與最高級時態定語 `[最新] + [成果/版本/消息/進度/研究]`：
        if matches!(
            curr_word,
            "分享" | "討論" | "介紹" | "解釋" | "說明" | "研究" | "分析" | "檢查" | "測試"
                | "確認" | "體驗" | "展示" | "思考" | "觀察" | "整理" | "補充"
        ) {
            let next_is_yixia = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "一下"))
                .unwrap_or(false);
            if next_is_yixia {
                bonus += 36.0;
            }
        } else if prev_word == "分" && matches!(curr_word, "想" | "享" | "響" | "餉") {
            let next_is_yixia = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "一下"))
                .unwrap_or(false);
            if next_is_yixia {
                bonus -= 44.0;
            }
        }

        // 跨語種領屬結構：當前詞為「的/得/地」，且前接純英數識別符（如 GAA、AI、LLM、Rust、API）或後接「最新/最大/最佳/最... + 名詞」時，強制鎖定領屬助詞「的」
        if matches!(curr_word, "的" | "得" | "地") {
            let prev_is_ascii_identifier = !prev_word.is_empty()
                && prev_word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            let next_is_superlative_np = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "最新" | "最高" | "最大" | "最佳" | "最好" | "最終" | "最後" | "最先"
                                | "成果" | "結果" | "版本" | "消息" | "進度" | "發展" | "研究"
                                | "技術"
                        )
                    })
                })
                .unwrap_or(false);
            if prev_is_ascii_identifier || next_is_superlative_np {
                if curr_word == "的" {
                    bonus += 38.0;
                } else {
                    bonus -= 36.0;
                }
            }
        }

        // 最高級時態定語 `[最新] + [成果/版本/消息/進度/研究/技術/發展/動態/消息/資料/消息]` vs. 碎拆「最 + 心 / 辛 / 薪 / 欣」：
        if curr_word == "最新" {
            let next_is_achievement_or_np = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "成果" | "版本" | "消息" | "進度" | "研究" | "技術" | "發展" | "動態"
                                | "資料" | "數據" | "報告" | "結果" | "作品" | "功能" | "突破"
                                | "款" | "型"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_achievement_or_np || prev_word == "的" {
                bonus += 36.0;
            }
        } else if prev_word == "最" && matches!(curr_word, "心" | "辛" | "薪" | "欣" | "馨" | "鋅") {
            bonus -= 42.0;
        }

        // 34. 文具與書寫工具語意場構式（`鉛筆 / 鋼筆 / 橡皮擦`）及動結式勝負補語構式（`[為什麼/為何/怎麼] + [會] + [搶/比/打/跑/賭/爭/認/服] + [輸/贏]`）：
        if matches!(curr_word, "鉛筆" | "橡皮擦" | "鋼筆" | "原子筆" | "毛筆" | "自動鉛筆") {
            let has_stationery_field = prefix_words.iter().rev().take(8).any(|&w| {
                matches!(
                    w,
                    "文具" | "清單" | "鉛筆" | "鋼筆" | "原子筆" | "毛筆" | "橡皮擦" | "尺"
                        | "書包" | "鉛筆盒" | "削" | "買" | "帶" | "用"
                )
            }) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "、" | "鋼筆" | "鉛筆" | "橡皮擦" | "原子筆" | "尺" | "盒" | "芯"
                        )
                    })
                })
                .unwrap_or(false);
            if has_stationery_field {
                bonus += 34.0;
            }
        } else if prev_word == "橡皮" && curr_word == "擦" {
            bonus += 34.0;
        } else if prev_word == "相比" && curr_word == "擦" {
            bonus -= 46.0;
        }

        // 疑問原因副詞 `[為什麼/為甚麼/為何/怎麼/難道/到底/究竟]` ➔ 情態助動詞 `[會]` vs. 姓氏/名詞 `[惠/匯/繪/慧/賄]`：
        if matches!(
            prev_word,
            "為什麼" | "為甚麼" | "為何" | "怎麼" | "怎樣" | "難道" | "到底" | "究竟"
        ) && matches!(curr_word, "會" | "惠" | "匯" | "繪" | "慧" | "賄" | "晦" | "穢")
        {
            if curr_word == "會" {
                bonus += 36.0;
            } else {
                bonus -= 38.0;
            }
        }

        // 動結式勝負補語構式：`[搶/比/打/跑/賭/爭/競/認/服/不] + [輸/贏]`（如：搶輸、比輸、打輸、跑輸、認輸、服輸、不輸）vs. 碎拆「搶 + 書 / 梳 / 蔬 / 叔」：
        if curr_word == "搶輸" || curr_word == "搶贏" {
            bonus += 34.0;
        } else if matches!(
            prev_word,
            "搶" | "比" | "跑" | "賭" | "爭" | "競" | "認" | "服"
        ) && matches!(curr_word, "輸" | "書" | "梳" | "蔬" | "叔" | "殊" | "舒" | "疏")
        {
            let prev2_is_modal_or_neg = matches!(
                prev2_word,
                "會" | "不會" | "能" | "不能" | "要" | "別" | "沒" | "沒有" | "又" | "都"
                    | "卻" | "竟" | "竟然" | "居然" | "還是" | "不"
            );
            let next_is_comp_target = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "給" | "了" | "過" | "「" | "『" | "他" | "她" | "你" | "我"
                                | "人" | "別人" | "對手" | "？" | "。" | "，"
                        )
                    })
                })
                .unwrap_or(false)
                || dag.get(&end_idx).map(|ns| ns.is_empty()).unwrap_or(true);
            if prev2_is_modal_or_neg || next_is_comp_target || matches!(prev_word, "認" | "服" | "比" | "賭") {
                if curr_word == "輸" {
                    bonus += 36.0;
                } else {
                    bonus -= 34.0;
                }
            }
        }

        // ── 構式二十一：語意調校工作室實戰四組同音構式雙向分流（一般/一班、確定/卻定、勘誤/刊物、雙句/雙具） ──
        // 1. 「一般（常態屬性/總括副詞）」vs.「一班（班次/班級數量詞）」雙向分流：
        if matches!(curr_word, "一般" | "一班")
            || (prev_word == "一" && matches!(curr_word, "般" | "班"))
        {
            let next_is_class_or_transport = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "公車" | "客運" | "列車" | "火車" | "捷運" | "高鐵" | "班機" | "飛機"
                                | "船" | "渡輪" | "校車" | "專車" | "車" | "學生" | "同學"
                                | "人馬" | "士兵" | "戰士" | "幹部" | "兵"
                        )
                    })
                })
                .unwrap_or(false);
            let next_is_general_attr_target = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "使用者" | "人" | "人們" | "民眾" | "大眾" | "百姓" | "市民" | "讀者"
                                | "消費者" | "客戶" | "用戶" | "家庭" | "企業" | "公司" | "情況"
                                | "狀況" | "情形" | "狀態" | "水準" | "標準" | "原則" | "規定"
                                | "規則" | "做法" | "作法" | "方式" | "方法" | "而言" | "來說"
                                | "說來" | "看來" | "認為" | "都" | "也" | "會"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_class_or_transport {
                if curr_word == "一班" || curr_word == "班" {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            } else if next_is_general_attr_target {
                if curr_word == "一般" || curr_word == "般" {
                    bonus += 34.0;
                } else {
                    bonus -= 30.0;
                }
            }
        }

        // 2. 「確定（認知心理動詞）」vs.「卻 + 定（轉折副詞＋單字動詞）」雙向分流：
        if curr_word == "確定" {
            let followed_by_ding_xia = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(m.word.as_str(), "下" | "下來" | "下心" | "居")
                    })
                })
                .unwrap_or(false);
            let next_is_modal_or_clause = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "可以" | "能" | "能夠" | "會" | "不會" | "要" | "不要" | "是"
                                | "不是" | "有" | "沒有" | "沒" | "不" | "已" | "已經" | "一下"
                                | "了" | "嗎" | "好" | "好了"
                        )
                    })
                })
                .unwrap_or(false);
            if followed_by_ding_xia {
                bonus -= 36.0;
            } else if next_is_modal_or_clause
                || matches!(
                    prev_word,
                    "使用者" | "民眾" | "系統" | "大家" | "我們" | "你們" | "他們" | "我"
                        | "你" | "您" | "他" | "她" | "先" | "已" | "已經" | "很" | "不"
                        | "沒" | "還" | "都" | "也" | "能" | "可" | "可以"
                )
            {
                bonus += 32.0;
            }
        } else if prev_word == "卻" && (curr_word == "定" || curr_word == "定下") {
            let next_is_valid_queding_idiom = curr_word == "定下"
                || dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(m.word.as_str(), "下" | "下來" | "下心" | "居" | "在" | "於")
                        })
                    })
                    .unwrap_or(false);
            if next_is_valid_queding_idiom {
                bonus += 34.0;
            } else {
                bonus -= 32.0;
            }
        }

        // 2a. 全稱總括副詞 `[眾人/人人/萬眾/舉世/草木/比比] ➔ [皆/接/階]` 與動趨補語 `[定下/靜下/安下/沉下/放下] ➔ [心來/心]`：
        if matches!(curr_word, "皆" | "接" | "階" | "街")
            && matches!(prev_word, "眾人" | "人人" | "萬眾" | "舉世" | "草木" | "比比")
        {
            if curr_word == "皆" {
                bonus += 38.0;
            } else {
                bonus -= 32.0;
            }
        }
        if matches!(curr_word, "心" | "心來" | "新" | "新來" | "薪")
            && (matches!(prev_word, "定下" | "靜下" | "安下" | "沉下" | "放下" | "狠下" | "橫下")
                || (prev_word == "下" && matches!(prev2_word, "定" | "靜" | "安" | "沉" | "放" | "狠" | "橫")))
        {
            let followed_by_lai = curr_word.ends_with('來')
                || dag
                    .get(&end_idx)
                    .map(|ns| ns.iter().any(|m| m.word == "來"))
                    .unwrap_or(false);
            if followed_by_lai {
                if matches!(curr_word, "心" | "心來") {
                    bonus += 38.0;
                } else {
                    bonus -= 34.0;
                }
            }
        }

        // 2b. 交通工具「可以 + 載 + 多少/乘客/學生/人」vs. 副詞「再」分流：
        if matches!(curr_word, "載" | "再" | "在") && matches!(prev_word, "可以" | "能" | "能夠" | "可" | "會") {
            let next_is_passenger_or_quantity = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "多少" | "多" | "幾" | "幾個" | "幾位" | "幾名" | "很多" | "許多" | "滿"
                                | "客" | "人" | "學生" | "乘客" | "貨"
                        )
                    })
                })
                .unwrap_or(false);
            let prev2_is_vehicle = matches!(
                prev2_word,
                "公車" | "客運" | "列車" | "火車" | "捷運" | "高鐵" | "班機" | "飛機" | "船"
                    | "渡輪" | "校車" | "專車" | "車"
            );
            if next_is_passenger_or_quantity || prev2_is_vehicle {
                if curr_word == "載" {
                    bonus += 52.0;
                } else {
                    bonus -= 46.0;
                }
            }
        }
        if prev_word == "載"
            && matches!(
                curr_word,
                "多少" | "幾" | "幾個" | "幾位" | "幾名" | "滿" | "客" | "人" | "學生" | "乘客" | "貨"
            )
        {
            bonus += 26.0;
        }

        // 3. 「勘誤（文字校勘訂正）」vs.「刊物（出版讀物）」雙向語境分流：
        if matches!(curr_word, "勘誤" | "刊物") {
            let left_anchor = if matches!(prev_word, "與" | "和" | "及" | "跟" | "或" | "的" | "之") {
                prev2_word
            } else {
                prev_word
            };
            let is_errata_context = matches!(
                left_anchor,
                "錯誤" | "錯字" | "訛誤" | "疏漏" | "筆誤" | "錯別字" | "原文" | "貼入"
                    | "輸入" | "進行" | "執行" | "負責" | "對照" | "校對" | "修訂" | "語意"
            ) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "校對" | "訂正" | "比對" | "對照" | "修正" | "表" | "清單" | "雙句"
                                | "單句" | "紀錄" | "記錄"
                        )
                    })
                })
                .unwrap_or(false);
            let is_publication_context = matches!(
                left_anchor,
                "書籍" | "圖書" | "報紙" | "雜誌" | "期刊" | "讀物" | "出版" | "發行" | "定期"
                    | "學術" | "政府" | "兒童" | "宣導" | "閱讀" | "訂閱" | "收藏" | "許多"
                    | "各類" | "各種" | "一份" | "一本" | "這份" | "這本"
            ) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| matches!(m.word.as_str(), "出版" | "發行" | "編輯"))
                })
                .unwrap_or(false);

            if is_errata_context && !is_publication_context {
                if curr_word == "勘誤" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            } else if is_publication_context && !is_errata_context {
                if curr_word == "刊物" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }

        // 4. 「雙句（對偶/校對語句對）」vs.「雙具（雙引擎/雙器物量詞）」雙向分流：
        if matches!(curr_word, "雙句" | "雙具")
            || (prev_word == "雙" && matches!(curr_word, "句" | "具"))
        {
            let is_sentence_pair_ctx = matches!(
                prev_word,
                "校對" | "對齊" | "比對" | "對照" | "貼入" | "輸入" | "分析" | "翻譯" | "修改"
                    | "勘誤" | "範例" | "對偶" | "平行" | "前後" | "上下" | "正反"
            ) || (prev_word == "雙"
                && matches!(
                    prev2_word,
                    "校對" | "對齊" | "比對" | "對照" | "貼入" | "輸入" | "分析" | "翻譯"
                        | "修改" | "勘誤" | "範例" | "對偶" | "平行" | "前後" | "上下" | "正反"
                ))
                || dag
                    .get(&end_idx)
                    .map(|ns| {
                        ns.iter().any(|m| {
                            matches!(
                                m.word.as_str(),
                                "對齊" | "比對" | "校對" | "對照" | "分析"
                            )
                        })
                    })
                    .unwrap_or(false);
            let is_apparatus_ctx = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "引擎" | "馬達" | "渦輪" | "發動機" | "推進器" | "屍體" | "遺體"
                                | "棺木"
                        )
                    })
                })
                .unwrap_or(false);

            if is_sentence_pair_ctx && !is_apparatus_ctx {
                if curr_word == "雙句" || curr_word == "句" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            } else if is_apparatus_ctx {
                if curr_word == "雙具" || curr_word == "具" {
                    bonus += 36.0;
                } else {
                    bonus -= 32.0;
                }
            }
        }
        if curr_word == "配備"
            && (matches!(
                prev_word,
                "飛機" | "戰機" | "客機" | "班機" | "車輛" | "汽車" | "列車" | "系統" | "裝置"
                    | "電腦" | "手機" | "標準" | "基本"
            ) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter()
                        .any(|m| matches!(m.word.as_str(), "了" | "有" | "雙具" | "雙" | "兩具"))
                })
                .unwrap_or(false))
        {
            bonus += 34.0;
        }

        // ── 構式二十二：261021 語意分析六組實戰句深層構式（大易就幾乎忘光光了 / 交到vs交道 / 句末對嗎 / 真的有必要 / 再度靈魂拷問 / 在實做的語意調整工作室僅限漢語） ──
        // 1. 專名主語 × 焦點副詞「就」× 程度副詞「幾乎」× 完耗結果補語「忘光(光)了」構式（防「大 + 憶舊」與「望 + 光光」跨界劫掠）：
        if matches!(
            curr_word,
            "大易" | "倉頡" | "行列" | "許氏" | "嘸蝦米" | "無蝦米" | "許氏鍵盤"
        ) {
            let next_is_focus_or_device = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "就" | "都" | "也" | "才" | "卻" | "已經" | "輸入法" | "鍵盤" | "字根" | "碼表"
                        )
                    })
                })
                .unwrap_or(false);
            if next_is_focus_or_device {
                bonus += 34.0;
            }
        }
        if prev_word == "大" && curr_word == "憶舊" {
            bonus -= 46.0;
        }
        if matches!(
            prev_word,
            "大易" | "倉頡" | "行列" | "許氏" | "嘸蝦米" | "無蝦米" | "注音" | "速成" | "拼音"
        ) && matches!(curr_word, "就" | "都" | "也" | "才" | "卻")
        {
            bonus += 32.0;
        }
        if matches!(curr_word, "忘光光" | "忘光") {
            bonus += 36.0;
        } else if matches!(curr_word, "忘" | "望" | "旺" | "妄") {
            let next_is_guang = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "光" | "光光" | "掉" | "了")))
                .unwrap_or(false);
            if next_is_guang || matches!(prev_word, "幾乎" | "差點" | "快" | "快要" | "全都" | "通通" | "完全") {
                if curr_word == "忘" {
                    bonus += 36.0;
                } else if next_is_guang {
                    bonus -= 38.0;
                }
            }
        }
        if prev_word == "忘" && matches!(curr_word, "光" | "光光") {
            bonus += 34.0;
        } else if prev_word == "望" && matches!(curr_word, "光" | "光光") {
            bonus -= 38.0;
        }

        // 2. 受事移轉動補「交到 + [受事/處所] + [手上/手裡/手中]」vs. 離合動賓「打 + 交道」雙向分流：
        if prev_word == "可" && curr_word == "已" {
            bonus -= 42.0;
        }
        if matches!(curr_word, "交到" | "交道") {
            let is_verb_complement_ctx = matches!(
                prev_word,
                "可以" | "能" | "能夠" | "會" | "要" | "把" | "將" | "已" | "已經" | "先" | "再" | "轉" | "送" | "遞" | "移" | "提" | "呈"
            ) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "使用者" | "客戶" | "民眾" | "讀者" | "消費者" | "大家" | "人們"
                                | "人" | "你" | "您" | "我" | "他" | "她" | "我們" | "你們"
                                | "他們" | "手" | "手上" | "手裡" | "手中" | "哪裡" | "那裡"
                                | "這裡" | "對方" | "銀行" | "法院" | "政府" | "警局" | "學校"
                        )
                    })
                })
                .unwrap_or(false);
            let is_jiaodao_noun_ctx = matches!(
                prev_word,
                "打" | "常打" | "不打" | "少打" | "愛打" | "打過" | "平" | "露水"
            );
            if is_verb_complement_ctx && !is_jiaodao_noun_ctx {
                if curr_word == "交到" {
                    bonus += 38.0;
                } else {
                    bonus -= 36.0;
                }
            } else if is_jiaodao_noun_ctx {
                if curr_word == "交道" {
                    bonus += 38.0;
                } else {
                    bonus -= 36.0;
                }
            }
        }

        // 3. 句末附加問句肯定謂詞「對 / 是 / 好」+ 疑問助詞「嗎 / 吧 / 呢」構式（防標點後無前驅時「隊嗎」搶佔「對嗎」）：
        if matches!(curr_word, "對" | "隊" | "兌") {
            let next_is_question_particle = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter()
                        .any(|m| matches!(m.word.as_str(), "嗎" | "吧" | "呢" | "呀" | "喔" | "啦" | "嘛"))
                })
                .unwrap_or(false);
            if next_is_question_particle {
                if curr_word == "對" {
                    bonus += 38.0;
                } else {
                    bonus -= 42.0;
                }
            }
        }

        // 4. 情態存在必要性構式「[真的/實在/難道/哪/是否]? + [有/沒/沒有/無/毫無] + [必要] + [這樣/大費周章]?」：
        if curr_word == "必要"
            && matches!(
                prev_word,
                "有" | "沒" | "沒有" | "無" | "毫無" | "大有" | "很有" | "頗有" | "實有" | "確有" | "不" | "是"
            )
        {
            bonus += 38.0;
        }
        if curr_word == "有"
            && dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| m.word == "必要"))
                .unwrap_or(false)
        {
            bonus += 34.0;
        }

        // 5. 重複時間副詞「再度/再次」vs. 進行介詞「在 + 度(假/過)」雙向分流與「靈魂拷問」構式：
        if curr_word == "再度" {
            let is_zaidu_ctx = matches!(
                prev_word,
                "剛剛" | "剛才" | "剛" | "又" | "今天" | "今日" | "昨天" | "近日" | "最近"
                    | "已" | "已經" | "將" | "會" | "卻" | "也" | "又將" | "我" | "他" | "她"
            ) || dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(
                            m.word.as_str(),
                            "靈魂" | "靈魂拷問" | "拷問" | "被" | "遭" | "面臨" | "陷入" | "上演"
                                | "掀起" | "引發" | "出現" | "成為" | "發起" | "展開" | "進行"
                                | "挑戰" | "創下" | "刷新" | "突破" | "回到" | "傳來" | "發生"
                        )
                    })
                })
                .unwrap_or(false);
            if is_zaidu_ctx {
                bonus += 38.0;
            }
        } else if prev_word == "在" && matches!(curr_word, "度假" | "渡假" | "度蜜月") {
            bonus += 42.0;
        } else if prev_word == "再度" && curr_word == "假" {
            bonus -= 45.0;
        } else if prev_word == "在" && curr_word == "度" {
            let next_is_vacation = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "假" | "蜜月" | "過" | "日")))
                .unwrap_or(false);
            if !next_is_vacation {
                bonus -= 38.0;
            }
        }
        if curr_word == "靈魂拷問" {
            bonus += 38.0;
        } else if matches!(curr_word, "拷問" | "考問") {
            if matches!(
                prev_word,
                "靈魂" | "良心" | "道德" | "嚴刑" | "逼供" | "審訊" | "自我" | "再度" | "反覆"
            ) {
                if curr_word == "拷問" {
                    bonus += 38.0;
                } else {
                    bonus -= 36.0;
                }
            }
        }

        // 6. 進行體定語「在實做/實作的」、引號專名「語意調整/調校工作室」與範圍限定謂詞「僅限/只限 + 漢語」構式：
        if matches!(curr_word, "實做" | "實作") {
            let is_shizuo_ctx = matches!(
                prev_word,
                "在" | "正在" | "所" | "目前" | "已" | "已經" | "開始" | "負責" | "進行" | "親自"
            ) || dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "的" | "中" | "上" | "時" | "出")))
                .unwrap_or(false);
            if is_shizuo_ctx {
                bonus += 38.0;
            }
        } else if prev_word == "在" && curr_word == "時" {
            let next_is_zuo = dag
                .get(&end_idx)
                .map(|ns| ns.iter().any(|m| matches!(m.word.as_str(), "做" | "做的" | "作" | "作的")))
                .unwrap_or(false);
            if next_is_zuo {
                bonus -= 40.0;
            }
        }
        if matches!(curr_word, "語意調校" | "語意調整") {
            bonus += 38.0;
        } else if curr_word == "語意" {
            let next_is_tiao = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        matches!(m.word.as_str(), "調整" | "調校" | "分析" | "規則" | "模型" | "引擎")
                    })
                })
                .unwrap_or(false);
            if next_is_tiao {
                bonus += 38.0;
            }
        } else if curr_word == "與" {
            let next_is_yitiao_zheng = dag
                .get(&end_idx)
                .map(|ns| {
                    ns.iter().any(|m| {
                        m.word == "一條"
                            && dag
                                .get(&m.end_idx)
                                .map(|after| after.iter().any(|z| matches!(z.word.as_str(), "整" | "校" | "教")))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if next_is_yitiao_zheng {
                bonus -= 48.0;
            }
        }
        if curr_word == "工作室" {
            if matches!(
                prev_word,
                "調整" | "調校" | "語意調整" | "語意調校" | "個人" | "設計" | "開發" | "攝影" | "音樂" | "藝術"
            ) {
                bonus += 38.0;
            }
        } else if prev_word == "工作" && curr_word == "是" {
            if matches!(prev2_word, "調整" | "調校" | "整" | "校" | "語意調整" | "語意調校") {
                bonus -= 42.0;
            }
        }
        if matches!(curr_word, "僅限" | "僅限於" | "只限" | "只限於") {
            bonus += 38.0;
        } else if prev_word == "僅" && matches!(curr_word, "限" | "現" | "獻" | "線" | "縣") {
            if curr_word == "限" {
                bonus += 36.0;
            } else {
                bonus -= 38.0;
            }
        }

        bonus
    }
}
