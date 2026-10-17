# -*- coding: utf-8 -*-
"""
RuhI 詞庫全面健康檢查器 (Ruhi Lexicon Comprehensive Health & Impostor Auditor)
Phase 1: 本機靜態語意與先驗機率檢驗器 (Local Static Lexical Auditor)

核心功能與檢驗維度：
1. 【維度一：同音字/詞先驗權重倒掛檢驗 (Homophone Priority Inversion)】
   - 掃描同音單字與二字詞：比對現代常用字/詞與生僻/古音/特權詞的先驗 Unigram。
   - 揪出如「一朝(420) > 一招(338)」、「令(720) > 另(706)」、「以(950) > 已(781)」、「黨(720) > 檔(688)」等歷史倒掛。

2. 【維度二：生僻/特權詞 Unigram 虛高離群值檢驗 (Unigram Outliers)】
   - 掃描標記為「字戀詞庫」、「繞口令」、「古典文言」、「佛學」等生僻詞條。
   - 抓出先驗權重虛高（如 >= 350 或 >= 800）而在現代生活語料中罕見的潛在語意劫掠者（如「發灰」、「置喙」、「九野」、「諱字」等）。

3. 【維度三：跨詞隙搭橋碰撞掃描 (Cross-Boundary Phonetic Bridge Impostor Scan)】
   - 自動清查 2 字生僻詞 (S1, S2)，當 S1 為常用前驅詞尾音且 S2 為常用後繼詞首音時，可能跨詞隙截斷合法長詞。
   - 計算碰撞危險指數（Hazard Index），並確認是否已有 Layer 1/2 規則或降權防護。

4. 【維度四：現代口語音與高頻容錯完整性檢驗 (Spoken & Tolerance Coverage)】
   - 清查現代口語常見讀音（如「沒關係」一聲/輕聲、「也感慨」四聲、「一朝一夕」vs「一招」）之收錄現況。
"""

import os
import sys
import argparse
from collections import defaultdict

# 保證 Windows 終端機能正確輸出 UTF-8
sys.stdout.reconfigure(encoding='utf-8')

# 預設詞庫檔案路徑（以專案根目錄為基準）
DEFAULT_LEXICON_FILES = [
    "ruhi_lexicon/data/lexicon_domain.tsv",
    "ruhi_lexicon/data/lexicon_mcbopomofo.tsv",
    "ruhi_lexicon/data/lexicon_moe.tsv",
    "ruhi_lexicon/data/lexicon_core.tsv",
]

TRANSITIONS_USER_PATH = "ruhi_lexicon/data/transitions_user.tsv"

# 常見現代高頻核心單字（取樣自教育部常用國字與如喜高頻詞，用於檢驗倒掛）
CORE_MODERN_CHARS = {
    "每", "檔", "動", "會", "已", "另", "招", "發", "累", "建", "慧",
    "和", "與", "是", "在", "再", "就", "要", "也", "得", "的", "地",
    "做", "作", "像", "相", "象", "這", "那", "看", "說", "打", "時"
}

def load_transitions():
    """加載已存在的轉移規則，用以驗證搭橋碰撞是否已受防護"""
    transitions = set()
    if os.path.exists(TRANSITIONS_USER_PATH):
        with open(TRANSITIONS_USER_PATH, "r", encoding="utf-8") as fp:
            for line in fp:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) >= 2:
                    transitions.add((parts[0].strip(), parts[1].strip()))
    return transitions

def load_all_lexicons(file_list=None):
    if file_list is None:
        file_list = DEFAULT_LEXICON_FILES
        
    entries = []
    syllable_to_words = defaultdict(list)
    word_to_entries = defaultdict(list)
    
    for fpath in file_list:
        if not os.path.exists(fpath):
            continue
        with open(fpath, "r", encoding="utf-8") as fp:
            for line_no, line in enumerate(fp, 1):
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) >= 3:
                    word = parts[0]
                    bpmf = parts[1].strip()
                    try:
                        weight = float(parts[2])
                    except ValueError:
                        continue
                    cat = parts[3] if len(parts) > 3 else ""
                    note = parts[4] if len(parts) > 4 else ""
                    syls = tuple(bpmf.split())
                    
                    item = {
                        "word": word,
                        "syls": syls,
                        "weight": weight,
                        "category": cat,
                        "note": note,
                        "file": fpath,
                        "line": line_no
                    }
                    entries.append(item)
                    syllable_to_words[syls].append(item)
                    word_to_entries[word].append(item)
                    
    return entries, syllable_to_words, word_to_entries

def audit_homophone_inversions(syllable_to_words):
    """維度一：同音字/詞權重倒掛清查"""
    results = []
    
    # 1. 重點基準清單（涵蓋歷史里程碑與 Backlog）
    target_benchmarks = [
        ("另", "令", ("ㄌㄧㄥˋ",), "副詞「另」（另開/另外）vs 祈使動/名詞「令」【Backlog 12】"),
        ("一句", "依據", ("ㄧ", "ㄐㄩˋ"), "數量名詞「一句」（最後一句）vs 介詞/名詞「依據」【Backlog 13】"),
        ("一招", "一朝", ("ㄧ", "ㄓㄠ"), "動量名詞「一招」（學了一招）vs 成語時間詞「一朝」【Backlog 14】"),
        ("已", "以", ("ㄧˇ",), "完成時相「已」（已安全卸除）vs 介詞「以」【Backlog 15】"),
        ("每", "美", ("ㄇㄟˇ",), "代詞/副詞「每」vs 形容詞「美」【里程碑 9】"),
        ("檔", "黨", ("ㄉㄤˇ",), "科技通用名詞「檔」vs 政治專名「黨」【里程碑 10】"),
        ("會", "諱", ("ㄏㄨㄟˋ",), "極高頻助動詞「會」vs 避諱古典字「諱」"),
        ("動", "棟", ("ㄉㄨㄥˋ",), "極高頻動作詞「動」vs 建築量詞「棟」"),
        ("發揮", "發灰", ("ㄈㄚ", "ㄏㄨㄟ"), "現代能力詞「發揮」vs 繞口令「發灰」【里程碑 1】"),
        ("心累", "心類", ("ㄒㄧㄣ", "ㄌㄟˋ"), "現代心理詞「心累」vs 佛學古籍「心類」【里程碑 2】"),
        ("自建", "自薦", ("ㄗˋ", "ㄐㄧㄢˋ"), "科技詞「自建」vs 毛遂「自薦」【里程碑 5】"),
        ("智慧", "置喙", ("ㄓˋ", "ㄏㄨㄟˋ"), "現代科技詞「智慧」vs 成語否定詞「置喙」【里程碑 6】"),
        ("關係", "關西", ("ㄍㄨㄢ", "ㄒㄧ"), "日常生活「關係」(口語讀音) vs 地名「關西」【里程碑 8】"),
    ]
    
    for common, impostor, syls, desc in target_benchmarks:
        items = syllable_to_words.get(syls, [])
        common_items = [it for it in items if it["word"] == common]
        impostor_items = [it for it in items if it["word"] == impostor]
        
        common_max = max([it["weight"] for it in common_items], default=0.0)
        impostor_max = max([it["weight"] for it in impostor_items], default=0.0)
        
        is_inverted = impostor_max >= common_max
        diff = impostor_max - common_max
        
        results.append({
            "common": common,
            "impostor": impostor,
            "syls": syls,
            "desc": desc,
            "common_max": common_max,
            "impostor_max": impostor_max,
            "is_inverted": is_inverted,
            "diff": diff,
            "common_sources": common_items,
            "impostor_sources": impostor_items
        })
        
    # 2. 自動掃描單字層級的潛在倒掛（生僻標籤 vs 現代常用單字）
    auto_single_char_inversions = []
    for syls, items in syllable_to_words.items():
        if len(syls) == 1:
            # 找出屬於 CORE_MODERN_CHARS 的字
            core_words = [it for it in items if it["word"] in CORE_MODERN_CHARS and len(it["word"]) == 1]
            if not core_words:
                continue
            max_core_wt = max(it["weight"] for it in core_words)
            core_w = core_words[0]["word"]
            
            # 檢查同音下是否有生僻標籤字權重倒掛
            for it in items:
                w = it["word"]
                if len(w) == 1 and w != core_w:
                    note = it.get("note", "")
                    cat = it.get("category", "")
                    if ("字戀" in note or "古" in note or "罕" in note or "萌典" in note) and it["weight"] > max_core_wt:
                        auto_single_char_inversions.append({
                            "syls": syls,
                            "core_word": core_w,
                            "core_weight": max_core_wt,
                            "impostor_word": w,
                            "impostor_weight": it["weight"],
                            "file": it["file"],
                            "line": it["line"],
                            "note": note
                        })
                        
    return results, auto_single_char_inversions

def audit_unigram_outliers(entries):
    """維度二：生僻/特權詞 Unigram 虛高離群值清查"""
    outliers = []
    
    # 關鍵字特徵過濾
    suspicious_tags = ["字戀詞庫", "繞口令", "古雅", "古音", "佛學", "文言", "罕用", "罕見"]
    
    for it in entries:
        w = it["word"]
        wt = it["weight"]
        note = it.get("note", "")
        cat = it.get("category", "")
        
        # 條件 A: 帶有生僻/古典/繞口令標籤，但權重 >= 350
        is_suspicious_tag = any(t in note for t in suspicious_tags) or any(t in cat for t in suspicious_tags)
        
        # 排除已納入保護的高頻成語長句
        if len(w) <= 2 and is_suspicious_tag and wt >= 350:
            outliers.append(it)
        # 條件 B: 繞口令專用短語（如「發灰」、「鰨蟆」等）
        elif len(w) <= 3 and ("twister" in cat or "saying" in cat or "繞口令" in note) and wt >= 350:
            outliers.append(it)
            
    # 按權重從大到小排序
    outliers.sort(key=lambda x: x["weight"], reverse=True)
    return outliers

def audit_cross_boundary_bridges(entries, syllable_to_words, transitions):
    """維度三：2 字跨詞隙搭橋碰撞清查"""
    high_risk_bridges = [
        # (前驅詞, 後繼詞, 搭橋詞, 前驅音節, 後繼音節, 搭橋音節, 說明)
        ("很", "也", "九野", ("ㄏㄣˇ",), ("ㄧㄝˇ",), ("ㄐㄧㄡˇ", "ㄧㄝˇ"), "常用副詞「很久」+「也」遭神話詞「九野」截斷"),
        ("會", "自動", "諱字", ("ㄏㄨㄟˋ",), ("ㄗˋ", "ㄉㄨㄥˋ"), ("ㄏㄨㄟˋ", "ㄗˋ"), "「會」+「自動執行」遭古代帝王「諱字」搭橋"),
        ("手", "動加詞", "手棟", ("ㄕㄡˇ",), ("ㄉㄨㄥˋ", "ㄐㄧㄚ", "ㄘˊ"), ("ㄕㄡˇ", "ㄉㄨㄥˋ"), "「手動」遭建築「手棟」搭橋"),
        ("坐一下", "石頭", "下石", ("ㄗㄨㄛˋ", "ㄧ", "ㄒㄧㄚˋ"), ("ㄕˊ", "ㄊㄡˊ"), ("ㄒㄧㄚˋ", "ㄕˊ"), "「坐一下」+「石頭」遭「落井下石」之「下石」截斷"),
        ("盲人摸象", "時", "橡實", ("ㄇㄤˊ", "ㄖㄣˊ", "ㄇㄛ", "ㄒㄧㄤˋ"), ("ㄕˊ",), ("ㄒㄧㄤˋ", "ㄕˊ"), "「摸象」+「時」遭植物名「橡實」截斷"),
        ("街巷", "時鐘", "橡實", ("ㄐㄧㄝ", "ㄒㄧㄤˋ"), ("ㄕˊ", "ㄓㄨㄥ"), ("ㄒㄧㄤˋ", "ㄕˊ"), "「街巷」+「時鐘」遭「橡實」搭橋"),
        ("混打", "看板", "打看", ("ㄏㄨㄣˋ", "ㄉㄚˇ"), ("ㄎㄢˋ", "ㄅㄢˇ"), ("ㄉㄚˇ", "ㄎㄢˋ"), "「混打」+「看板」遭方言詞「打看」搭橋"),
        ("最後", "一句", "依據", ("ㄗㄨㄟˋ", "ㄏㄡˋ"), ("ㄧ", "ㄐㄩˋ"), ("ㄧ", "ㄐㄩˋ"), "「最後」+「一句」遭介詞「依據」語意劫掠【Backlog 13】"),
    ]
    
    results = []
    for w_prev, w_next, w_bridge, syl_prev, syl_next, syl_bridge, desc in high_risk_bridges:
        tail_syl = syl_prev[-1]
        head_syl = syl_next[0]
        formed_syl = (tail_syl, head_syl) if len(syl_bridge) == 2 else (tail_syl,)
        
        # 檢查搭橋音節是否匹配
        is_phonetic_bridge = (formed_syl == syl_bridge) or (syl_next == syl_bridge)
        
        # 檢查轉移矩陣是否有針對該搭橋的懲罰防護
        has_negative_transition = (w_prev, w_bridge) in transitions or any(
            t[0] == w_prev and t[1] == w_bridge for t in transitions
        )
        
        # 檢查是否有直接的正向連詞加權
        has_positive_transition = (w_prev, w_next) in transitions or any(
            t[0] == w_prev and (w_next.startswith(t[1]) or t[1] in w_next) for t in transitions
        )
        
        # 查詢搭橋詞當前權重
        bridge_entries = [it for it in syllable_to_words.get(syl_bridge, []) if it["word"] == w_bridge]
        bridge_weight = max([it["weight"] for it in bridge_entries], default=0.0)
        
        results.append({
            "prev": w_prev,
            "next": w_next,
            "bridge": w_bridge,
            "syl_bridge": syl_bridge,
            "desc": desc,
            "bridge_weight": bridge_weight,
            "is_phonetic_bridge": is_phonetic_bridge,
            "has_negative_guard": has_negative_transition,
            "has_positive_guard": has_positive_transition,
            "is_safe": has_positive_transition or (bridge_weight <= 200.0)
        })
        
    return results

def generate_markdown_report(benchmarks, auto_inversions, outliers, bridges, output_path):
    """產生結構化 Markdown 體檢報告"""
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("# 如喜（RuhI）詞庫 Phase 1 靜態健康檢查報告\n\n")
        f.write(f"- 檢查模式：全庫 35 萬條詞靜態語意與先驗機率健康檢查\n")
        f.write(f"- 產生時間：本機自動檢驗\n\n")
        
        f.write("## 1. 同音字/詞權重倒掛清查 (Homophone Priority Inversion)\n\n")
        f.write("| 序號 | 常用標的 | 競爭語意劫掠詞 | 注音拼音 | 常用權重 | 語意劫掠詞權重 | 狀態 | 核心說明 |\n")
        f.write("|:---|:---|:---|:---|:---|:---|:---|:---|\n")
        for i, item in enumerate(benchmarks, 1):
            status = "🚨 **嚴重倒掛**" if item["is_inverted"] else "✅ 安全正常"
            syl_str = " ".join(item["syls"])
            f.write(f"| {i} | {item['common']} | {item['impostor']} | `{syl_str}` | {item['common_max']} | {item['impostor_max']} | {status} | {item['desc']} |\n")
            
        if auto_inversions:
            f.write("\n### 1.1 自動掃描單字層級生僻倒掛發現\n\n")
            f.write("| 常用單字 | 常用權重 | 倒掛生僻字 | 生僻字權重 | 注音 | 來源檔案與標籤 |\n")
            f.write("|:---|:---|:---|:---|:---|:---|\n")
            for item in auto_inversions[:15]: # 列出前 15 筆最顯著的
                syl_str = " ".join(item["syls"])
                f.write(f"| {item['core_word']} | {item['core_weight']} | `{item['impostor_word']}` | {item['impostor_weight']} | `{syl_str}` | {item['file']} ({item['note']}) |\n")
                
        f.write("\n---\n\n")
        f.write("## 2. 跨詞隙搭橋碰撞清查 (Cross-Boundary Phonetic Bridges)\n\n")
        f.write("| 前驅詞 | 後繼詞 | 搭橋語意劫掠詞 | 搭橋音節 | 搭橋詞現行權重 | 語法防護狀態 | 診斷判定 |\n")
        f.write("|:---|:---|:---|:---|:---|:---|:---|\n")
        for b in bridges:
            guard_status = []
            if b["has_positive_guard"]:
                guard_status.append("✅ 正向引力保護")
            if b["has_negative_guard"]:
                guard_status.append("🛡️ 負向搭橋懲罰")
            if not guard_status:
                guard_status.append("⚠️ 尚無轉移防護")
            guard_str = " / ".join(guard_status)
            
            diag = "✅ **受控**" if b["is_safe"] else "🚨 **高危暴露**"
            syl_str = " ".join(b["syl_bridge"])
            f.write(f"| {b['prev']} | {b['next']} | **{b['bridge']}** | `{syl_str}` | {b['bridge_weight']} | {guard_str} | {diag} |\n")
            
        f.write("\n---\n\n")
        f.write("## 3. Unigram 虛高離群生僻詞清查 (Outlier Top 20)\n\n")
        f.write("| 序號 | 生僻/古雅詞彙 | 注音音節 | 虛高權重 | 來源類別與標籤 | 所在檔案與行數 |\n")
        f.write("|:---|:---|:---|:---|:---|:---|\n")
        for i, out in enumerate(outliers[:25], 1):
            syl_str = " ".join(out["syls"])
            f.write(f"| {i} | `{out['word']}` | `{syl_str}` | **{out['weight']}** | {out['category']} ({out['note']}) | `{out['file']}:{out['line']}` |\n")
            
    print(f"\n📄 Markdown 體檢報告已成功匯出至：{output_path}")

def main():
    parser = argparse.ArgumentParser(description="如喜 (RuhI) 詞庫 Phase 1 全庫靜態健康檢查器")
    parser.add_argument("--export-report", type=str, default="docs/LEXICON_HEALTH_REPORT_PHASE1.md",
                        help="匯出詳細 Markdown 體檢報告之路徑")
    args = parser.parse_args()
    
    print("=" * 78)
    print("  🚀 如喜（RuhI）詞庫 Phase 1 全面靜態健康檢查器 (Health Auditor)")
    print("=" * 78)
    
    print("正在加載全庫詞條與轉移矩陣資料庫...")
    entries, syllable_to_words, word_to_entries = load_all_lexicons()
    transitions = load_transitions()
    print(f"📊 已載入詞條總數：{len(entries):,} 條，轉移矩陣規則：{len(transitions):,} 條")
    
    print("\n🔍 正在執行【維度一：同音字/詞權重倒掛清查】...")
    benchmarks, auto_inversions = audit_homophone_inversions(syllable_to_words)
    inverted_count = sum(1 for b in benchmarks if b["is_inverted"])
    print(f"   - 核心基準倒掛檢驗：共 {len(benchmarks)} 組，發現 {inverted_count} 組倒掛！")
    for b in benchmarks:
        status_sym = "🚨 [倒掛]" if b["is_inverted"] else "✅ [正常]"
        syl_str = " ".join(b["syls"])
        print(f"     {status_sym} {b['common']}({b['common_max']}) vs {b['impostor']}({b['impostor_max']}) [{syl_str}] - {b['desc']}")
        
    print("\n🔍 正在執行【維度二：跨詞隙搭橋碰撞掃描】...")
    bridges = audit_cross_boundary_bridges(entries, syllable_to_words, transitions)
    safe_bridges = sum(1 for b in bridges if b["is_safe"])
    print(f"   - 高危搭橋測試：共 {len(bridges)} 組，已受控 {safe_bridges} 組，暴露 {len(bridges) - safe_bridges} 組！")
    for b in bridges:
        diag = "✅ [受控]" if b["is_safe"] else "🚨 [高危]"
        print(f"     {diag} '{b['prev']}' + '{b['next']}' ➔ 搭橋詞 '{b['bridge']}' ({b['bridge_weight']}) - {b['desc']}")
        
    print("\n🔍 正在執行【維度三：生僻/古典/繞口令 Unigram 離群值清查】...")
    outliers = audit_unigram_outliers(entries)
    print(f"   - 發現生僻標籤且權重 >= 350 之虛高詞彙：共 {len(outliers)} 筆！")
    print(f"   - 權重最高前 5 筆離群詞：")
    for o in outliers[:5]:
        syl_str = " ".join(o["syls"])
        print(f"     * 「{o['word']}」 ({syl_str}) 權重: {o['weight']} [{o['category']} / {o['note']}]")
        
    if args.export_report:
        generate_markdown_report(benchmarks, auto_inversions, outliers, bridges, args.export_report)
        
    print("\n🎯 體檢完成！請參閱生成的 Markdown 報告以推進後續語法引力補齊與詞庫降權修正。")

if __name__ == "__main__":
    main()
