#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜 (RuhI) ✦ 人工餵料器 (Corpus Feeder) 
深度整合：RuhMark (如標) 語料清洗引擎 ⇄ 如喜本地 AI 錯字防火牆與轉移學習
=============================================================================
管線架構：
1. 收集語料：穿透 Windows 捷徑 (.lnk) 直達本體文字/目錄 (零複製)
2. RuhMark 清洗：呼叫 C:\\ATGprjs\\RuhMark 進行 URL/HTML 過濾、標點常規化、引號保護斷句，
   並於 C:\\ATGprjs\\RuhOS\\ruhi\\corpus_cleaned\\ 產生正規化副本
3. 錯字除魅：以教育部易混淆矩陣 (Typo Firewall) 攔截可能錯字
4. 智慧萃取：提煉大十風格的高頻個人轉移 (Bigram Transitions)
5. 雙向賦能：熱寫入 transitions_user.tsv，讓如喜輸入法即刻掌握大十文風！
"""

import argparse
import json
import os
import re
import sys
from collections import Counter

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_DIR = os.path.abspath(os.path.join(BASE_DIR, ".."))
CORPUS_FEEDER_DIR = os.path.join(RUHI_DIR, "corpus_feeder")
CORPUS_CLEANED_DIR = os.path.join(RUHI_DIR, "corpus_cleaned")
LEXICON_PATH = os.path.join(RUHI_DIR, "ruhi_lexicon", "data", "lexicon_moe.tsv")
TRANSITIONS_PATH = os.path.join(RUHI_DIR, "ruhi_lexicon", "data", "transitions_user.tsv")
RUHMARK_DIR = os.path.abspath("C:/ATGprjs/RuhMark")

# 動態引入 RuhMark (如標)
RUHMARK_AVAILABLE = False
try:
    if RUHMARK_DIR not in sys.path:
        sys.path.insert(0, RUHMARK_DIR)
    from ruhmark import RuhMarkPipeline, CleanOptions, CorpusProfiler
    RUHMARK_AVAILABLE = True
except Exception as e:
    print(f"⚠️ RuhMark 模組載入提示: {e}，將使用備用輕量清洗模式。")

# ==============================================================================
# 1. 錯字除魅防火牆矩陣 (Typo Firewall Matrix)
# ==============================================================================
COMMON_TYPO_PAIRS = {
    "按步就班": "按部就班",
    "再接在勵": "再接再厲",
    "再接再勵": "再接再厲",
    "再接在厲": "再接再厲",
    "以經": "已經",
    "以偏蓋全": "以偏概全",
    "一偏概全": "以偏概全",
    "變本加利": "變本加厲",
    "不逕而走": "不徑而走",
    "穿流不息": "川流不息",
    "急燥": "急躁",
    "名列前矛": "名列前茅",
    "一愁莫展": "一籌莫展",
    "迫不急待": "迫不及待",
    "濫芋充數": "濫竽充數",
    "走頭無路": "走投無路",
    "委屈求全": "委曲求全",
    "默守成規": "墨守成規",
    "談笑風聲": "談笑風生",
    "消聲匿跡": "銷聲匿跡",
    "震憾": "震撼",
    "趨之若騖": "趨之若鶩",
    "坐陣": "坐鎮",
    "甘敗下風": "甘拜下風",
    "名信片": "明信片",
    "莫明其妙": "莫名其妙",
    "鬼計多端": "詭計多端",
    "絕不認輸": "決不認輸",
    "寒喧": "寒暄",
    "脈膊": "脈搏",
    "挖角": "挖腳",
    "針貶": "針砭",
    "蜂湧而至": "蜂擁而至",
    "趨吉避兇": "趨吉避凶",
    "迫害妄想": "被害妄想",
}

# ==============================================================================
# 2. Windows 捷徑 (.lnk) 智慧穿透解析
# ==============================================================================
def resolve_windows_shortcut(path):
    try:
        import win32com.client
        shell = win32com.client.Dispatch("WScript.Shell")
        shortcut = shell.CreateShortcut(path)
        target = shortcut.TargetPath
        if target and os.path.exists(target):
            return target
    except Exception:
        pass
    return None

def collect_corpus_files(root_dir):
    collected = []
    supported_exts = {'.txt', '.md', '.markdown', '.docx', '.odt', '.json', '.csv'}

    if not os.path.exists(root_dir):
        return collected

    for item in os.listdir(root_dir):
        item_path = os.path.join(root_dir, item)
        if item.lower() == "readme.txt":
            continue

        if item.lower().endswith(".lnk"):
            target = resolve_windows_shortcut(item_path)
            if target and os.path.exists(target):
                if os.path.isfile(target):
                    ext = os.path.splitext(target)[1].lower()
                    if ext in supported_exts:
                        collected.append((target, f"捷徑 ➔ {item}"))
                elif os.path.isdir(target):
                    for r, _, files in os.walk(target):
                        for f in files:
                            if os.path.splitext(f)[1].lower() in supported_exts:
                                collected.append((os.path.join(r, f), f"資料夾捷徑 [{item}] ➔ {f}"))
        elif os.path.isdir(item_path):
            for r, _, files in os.walk(item_path):
                for f in files:
                    if os.path.splitext(f)[1].lower() in supported_exts:
                        collected.append((os.path.join(r, f), f"目錄檔案 ➔ {f}"))
        elif os.path.isfile(item_path):
            if os.path.splitext(item_path)[1].lower() in supported_exts:
                collected.append((item_path, f"本體檔案 ➔ {item}"))

    return collected

def is_hex_red(c_val):
    if not c_val: return False
    c = str(c_val).strip('#').lower()
    if c == 'red': return True
    if len(c) == 6:
        try:
            r, g, b = int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16)
            return r > 140 and g < 100 and b < 100
        except Exception: pass
    return False

def read_text_robust(path, red_only=True):
    """
    容錯讀取純文字檔案。
    針對 .docx 與 .odt，若 red_only=True，則只精確萃取大十親筆標註的紅色中文 (過濾 Google 機翻與雜質)
    """
    ext_lower = os.path.splitext(path)[1].lower()
    
    # 1. 處理 .docx
    if ext_lower == ".docx":
        import zipfile
        import xml.etree.ElementTree as ET
        try:
            with zipfile.ZipFile(path) as z:
                xml_data = z.read("word/document.xml")
                root = ET.fromstring(xml_data)
                paragraphs = []
                for p_elem in root.iter("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}p"):
                    line_parts = []
                    for r_elem in p_elem.iter("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}r"):
                        color_elem = r_elem.find(".//{http://schemas.openxmlformats.org/wordprocessingml/2006/main}color")
                        c_val = color_elem.attrib.get("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}val", "") if color_elem is not None else ""
                        
                        # 若 red_only，只保留紅色文字
                        if red_only and not is_hex_red(c_val):
                            continue
                            
                        for t in r_elem.iter("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}t"):
                            if t.text:
                                line_parts.append(t.text)
                    line = "".join(line_parts).strip()
                    # 必須包含中文字
                    if line and any('\u4e00' <= ch <= '\u9fa5' for ch in line):
                        paragraphs.append(line)
                return "\n".join(paragraphs)
        except Exception as e:
            print(f"讀取 docx 異常: {path}", e)
            return ""

    # 2. 處理 .odt
    elif ext_lower == ".odt":
        import zipfile
        import xml.etree.ElementTree as ET
        try:
            with zipfile.ZipFile(path) as z:
                xml_data = z.read("content.xml")
                root = ET.fromstring(xml_data)
                red_styles = set()
                for style in root.iter("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}style"):
                    s_name = style.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}name", "")
                    for tp in style.iter("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}text-properties"):
                        color = tp.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0}color", "")
                        if is_hex_red(color):
                            red_styles.add(s_name)
                
                paragraphs = []
                for elem in root.iter():
                    if elem.tag.endswith("}p") or elem.tag.endswith("}span"):
                        s_name = elem.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:text:1.0}style-name", "")
                        if not red_only or s_name in red_styles:
                            txt = "".join(elem.itertext()).strip()
                            if txt and any('\u4e00' <= ch <= '\u9fa5' for ch in txt):
                                paragraphs.append(txt)
                return "\n".join(paragraphs)
        except Exception as e:
            print(f"讀取 odt 異常: {path}", e)
            return ""

    # 3. 一般純文字檔案 (.txt, .md 等)
    for enc in ['utf-8', 'utf-8-sig', 'cp950', 'gbk']:
        try:
            with open(path, 'r', encoding=enc) as f:
                raw_lines = f.readlines()
            chinese_lines = [l.strip() for l in raw_lines if any('\u4e00' <= ch <= '\u9fa5' for ch in l)]
            return "\n".join(chinese_lines)
        except Exception:
            continue
    return ""

# ==============================================================================
# 3. RuhMark 深度清洗與產生標準化副本
# ==============================================================================
def clean_corpus_with_ruhmark(source_files, output_dir=CORPUS_CLEANED_DIR, red_only=True, progress_cb=None):
    """調用 RuhMark 進行高規格清洗，並在 output_dir 產生乾淨副本"""
    os.makedirs(output_dir, exist_ok=True)
    cleaned_records = []
    total_files = len(source_files)

    if RUHMARK_AVAILABLE:
        from ruhmark.models import CorpusItem
        pipeline = RuhMarkPipeline(
            options=CleanOptions(
                remove_urls=True,
                remove_html_tags=True,
                filter_news_boilerplates=True,
                protect_dialog_quotes=True,
                compress_repeated_punctuation=True,
            )
        )
        profiler = CorpusProfiler()

        print(f"\n⚡ 啟動 RuhMark (如標) 深度清洗流水線 (紅色親筆精粹模式: {'開' if red_only else '關'})...")
        for idx, (src_path, desc) in enumerate(source_files, 1):
            if progress_cb and (idx % 15 == 0 or idx == total_files):
                progress_cb(idx, total_files, desc)
            raw_text = read_text_robust(src_path, red_only=red_only)
            if not raw_text.strip():
                continue

            # 執行高規格清洗與引號保護切句
            sentences = pipeline.clean_single_text(raw_text)
            clean_body = "\n".join(sentences)

            # 產出乾淨副本
            base_name = os.path.splitext(os.path.basename(src_path))[0]
            clean_filename = f"{base_name}.cleaned.txt"
            clean_path = os.path.join(output_dir, clean_filename)

            with open(clean_path, "w", encoding="utf-8") as f:
                f.write(clean_body)

            # 評估語料指標 (以 RuhMark CorpusItem)
            c_item = CorpusItem(corpus_id=clean_filename, raw_text=raw_text, clean_text=clean_body)
            p_item = profiler.profile_item(c_item)

            print(f"  ✨ [RuhMark 清洗完成] {desc} ➔ {clean_filename}")
            print(f"     📊 句子數: {len(sentences)} | 字符數: {p_item.char_len} | 難度: {p_item.difficulty.value} | 詞庫支援: {'✅ 100%可行' if p_item.feasible_ruhi else '⚠️ 含生僻字'}")

            cleaned_records.append({
                "clean_path": clean_path,
                "sentences": sentences,
                "text": clean_body,
                "profile": p_item
            })
    else:
        # 降級保底清洗
        print("\nℹ️ 執行基礎標準化清洗...")
        for src_path, desc in source_files:
            raw_text = read_text_robust(src_path)
            clean_body = re.sub(r'[\r\n\t]+', '\n', raw_text.strip())
            base_name = os.path.splitext(os.path.basename(src_path))[0]
            clean_filename = f"{base_name}.cleaned.txt"
            clean_path = os.path.join(output_dir, clean_filename)
            with open(clean_path, "w", encoding="utf-8") as f:
                f.write(clean_body)
            cleaned_records.append({
                "clean_path": clean_path,
                "sentences": [s.strip() for s in clean_body.split('\n') if s.strip()],
                "text": clean_body,
                "profile": None
            })

    return cleaned_records

# ==============================================================================
# 4. 如喜 (RuhI) 本地語意與轉移矩陣萃取器
# ==============================================================================
class RuhiPersonalLearner:
    def __init__(self):
        self.known_words = set()
        self.load_known_lexicon()

    def load_known_lexicon(self):
        if os.path.exists(LEXICON_PATH):
            with open(LEXICON_PATH, "r", encoding="utf-8") as f:
                for line in f:
                    parts = line.strip().split("\t")
                    if parts:
                        self.known_words.add(parts[0])

    def audit_and_learn(self, sentences):
        typo_hits = []
        bigram_counts = Counter()
        new_word_candidates = Counter()

        # 1. 錯字防火牆 (具備上下文消歧防偽，防止如「可以經由」、「以經典」等被誤判)
        for sent in sentences:
            for typo, correct in COMMON_TYPO_PAIRS.items():
                if typo in sent:
                    is_false_alarm = False
                    if typo == "以經":
                        start = 0
                        all_fp = True
                        while True:
                            idx = sent.find(typo, start)
                            if idx == -1:
                                break
                            pre_char = sent[idx-1] if idx > 0 else ""
                            post_char = sent[idx+2] if idx+2 < len(sent) else ""
                            if pre_char in ("可", "得", "所", "何", "足") or post_char in ("典", "濟", "驗", "歷", "常", "營", "商", "書", "費", "手", "緯", "絡", "由", "過", "紀", "世"):
                                pass
                            else:
                                all_fp = False
                                break
                            start = idx + len(typo)
                        if all_fp:
                            is_false_alarm = True

                    if not is_false_alarm:
                        typo_hits.append({
                            "wrong": typo,
                            "correct": correct,
                            "context": sent
                        })

        # 2. 語意切分與 Bigram 轉移萃取
        for sent in sentences:
            words = []
            i = 0
            while i < len(sent):
                matched = False
                for wlen in (4, 3, 2):
                    sub = sent[i:i+wlen]
                    if sub in self.known_words:
                        words.append(sub)
                        i += wlen
                        matched = True
                        break
                if not matched:
                    if len(sent[i:]) >= 2:
                        two_char = sent[i:i+2]
                        if re.match(r'^[\u4e00-\u9fa5]{2}$', two_char):
                            new_word_candidates[two_char] += 1
                    i += 1

            for idx in range(len(words) - 1):
                w1 = words[idx]
                w2 = words[idx + 1]
                bigram_counts[(w1, w2)] += 1

        return {
            "total_sentences": len(sentences),
            "typos": typo_hits,
            "bigrams": bigram_counts,
            "new_candidates": new_word_candidates
        }

    def print_audit_report(self, report, source_name=""):
        print("\n" + "="*80)
        print(f"  ✦ 如喜 (RuhI) ✦ 語意學習審查報告: {source_name}")
        print("="*80)
        print(f"📊 總分析語句數量: {report['total_sentences']} 句")

        print(f"\n🛡️  【本地 AI 錯字防火牆掃描】：")
        if report["typos"]:
            seen_typo = set()
            for t in report["typos"]:
                key = (t["wrong"], t["correct"])
                if key not in seen_typo:
                    seen_typo.add(key)
                    print(f"  ❌ 「{t['wrong']}」 ➔ 建議修正為 💡「{t['correct']}」 (語境例句: ...{t['context'][:30]}...)")
        else:
            print("  🎉 太棒了！文筆嚴謹，未檢測出任何常見同音別字或錯混詞。")

        print(f"\n🟢 【個人高頻語意轉移搭配 Top 10】 (將吸收至 transitions_user.tsv)：")
        top_bigrams = report["bigrams"].most_common(10)
        if top_bigrams:
            for (w1, w2), cnt in top_bigrams:
                print(f"  🔗 「{w1}」 ➔ 「{w2}」 (出現 {cnt} 次 · 轉移加權: +{min(35.0, 15.0 + cnt*2.0):.1f})")
        else:
            print("  ℹ️ 尚無頻率 >= 2 的高頻二元搭配。")

        print(f"\n🔵 【潛在新創/大十專用活詞推薦 Top 10】 (頻率 >= 2)：")
        top_new = [(w, c) for w, c in report["new_candidates"].most_common(20) if c >= 2][:10]
        if top_new:
            for w, cnt in top_new:
                print(f"  ✨ 【{w}】 (出現 {cnt} 次 · 推薦評估收錄)")
        else:
            print("  ℹ️ 未發現頻率較高之未收錄新詞。")
        print("="*80 + "\n")

    def commit_transitions(self, report, min_freq=2):
        to_commit = []
        for (w1, w2), cnt in report["bigrams"].items():
            if cnt >= min_freq and len(w1) >= 2 and len(w2) >= 2:
                bonus = min(35.0, 15.0 + cnt * 2.0)
                to_commit.append((w1, w2, bonus))

        if not to_commit:
            print("    ℹ️ 沒有達到頻率門檻 (>=2) 的二元詞對需要寫入。")
            return 0

        existing = set()
        if os.path.exists(TRANSITIONS_PATH):
            with open(TRANSITIONS_PATH, "r", encoding="utf-8") as f:
                for line in f:
                    if not line.startswith("#") and "\t" in line:
                        p = line.strip().split("\t")
                        if len(p) >= 2:
                            existing.add((p[0], p[1]))

        new_lines = []
        for w1, w2, bonus in to_commit:
            if (w1, w2) not in existing:
                new_lines.append(f"{w1}\t{w2}\t{bonus:.1f}\tdoc_learning\t2026-09-19\n")
                existing.add((w1, w2))

        if new_lines:
            with open(TRANSITIONS_PATH, "a", encoding="utf-8") as f:
                f.writelines(new_lines)
            print(f">>> [OK] 已將 {len(new_lines)} 條個人專屬語意轉移安全寫入: {TRANSITIONS_PATH}")
            return len(new_lines)
        return 0

# ==============================================================================
# Main Entry Point
# ==============================================================================
def main():
    parser = argparse.ArgumentParser(description="如喜人工餵料器 (整合 RuhMark 深度清洗)")
    parser.add_argument("--file", type=str, help="欲學習的單一檔案路徑 (.txt, .md)")
    parser.add_argument("--dir", type=str, default=CORPUS_FEEDER_DIR, help="餵料資料夾路徑")
    parser.add_argument("--auto-commit", action="store_true", help="自動將合格轉移寫入個人詞庫")
    args = parser.parse_args()

    # 1. 收集語料來源 (支援 .lnk 捷徑穿透)
    if args.file:
        real_path = resolve_windows_shortcut(args.file) or args.file
        sources = [(real_path, os.path.basename(real_path))]
    else:
        print(f"\n🔎 正在掃描人工餵料目錄: {args.dir} ...")
        sources = collect_corpus_files(args.dir)

    if not sources:
        print(f"ℹ️ 目前在 {args.dir} 中尚未發現任何語料檔案或捷徑。")
        print("💡 貼心提示：您可以將寫作資料夾的「捷徑 (.lnk)」直接貼進這個資料夾！")
        return

    print(f"📂 成功鎖定 {len(sources)} 份語料來源（含 Windows 捷徑穿透）。")

    # 2. 調用 RuhMark 進行深度清洗並產生副本
    cleaned_records = clean_corpus_with_ruhmark(sources, output_dir=CORPUS_CLEANED_DIR)

    all_sentences = []
    for rec in cleaned_records:
        all_sentences.extend(rec["sentences"])

    # 3. 如喜語意除魅與轉移萃取
    learner = RuhiPersonalLearner()
    report = learner.audit_and_learn(all_sentences)
    learner.print_audit_report(report, source_name=f"RuhMark 清洗副本彙整 (共 {len(cleaned_records)} 份檔案)")

    # 4. 寫入如喜神經矩陣
    if args.auto_commit:
        learner.commit_transitions(report, min_freq=2)
    else:
        print("💡 若確認將上述個人高頻轉移寫入如喜，請加上參數: python ruhi_doc_learner.py --auto-commit")

if __name__ == '__main__':
    main()
