"""
Ruhi (如喜) Input Method Framework
ruhi_lexicon.build_moedict_tsv - 從教育部重編國語辭典修訂本建構 16 萬繁體權威詞庫 TSV
"""

import os
import re
import json
import lzma
from typing import Dict, List, Set, Tuple

# 高頻核心字詞加權表
CORE_BOOST_MAP = {
    '的': 1200, '是': 1100, '在': 1000, '有': 900, '我': 850, '不': 850,
    '你': 800, '他': 800, '這': 800, '就': 750, '人': 750, '都': 750,
    '一': 750, '個': 750, '上': 750, '也': 700, '很': 700, '到': 700,
    '說': 700, '要': 700, '去': 700, '子': 700, '會': 700, '著': 700,
    '沒有': 750, '看': 650, '好': 650, '自己': 700, '這裏': 600, '這裡': 700,
    '我們': 800, '你們': 750, '他們': 750, '今天': 750, '明天': 700,
    '現在': 750, '開始': 750, '使用': 850, '進行': 800, '思考': 800,
    '打字': 800, '輸入法': 900, '自然輸入法': 1200, '許氏鍵盤': 1200,
    '中華民國': 1000, '臺灣': 900, '台灣': 900, '台北': 800, '如荷': 800, '如喜': 850,
    '人工智慧': 850, '程式': 750, '系統': 800, '軟體': 750, '硬體': 750,
    '一鼓作氣': 800, '落筆生花': 800, '神來之筆': 800, '相得益彰': 800,
}

def clean_bopomofo(raw_bpm: str) -> str:
    """清理注音字串，轉換為 Ruhi 標準注音格式"""
    if not raw_bpm:
        return ""
    # 去除括號、注釋、斜線
    bpm = re.sub(r'[\(\)（）／/]', ' ', raw_bpm)
    # 將多個空白替換為單一空白
    bpm = re.sub(r'\s+', ' ', bpm).strip()
    return bpm

def build_tsv():
    base_dir = os.path.dirname(os.path.abspath(__file__))
    output_tsv = os.path.join(base_dir, "data", "lexicon_moe.tsv")
    core_tsv = os.path.join(base_dir, "data", "lexicon_core.tsv")

    if not os.path.exists(xz_path):
        print(f"錯誤：找不到萌典檔案 {xz_path}")
        return

    print(">>> 正在讀取並解析教育部重編國語辭典 (dict-revised.json.xz)...")
    raw_data = json.loads(lzma.open(xz_path).read())
    print(f"    讀取完成，共有 {len(raw_data)} 筆原始辭目")

    entries = []
    seen_pairs: Set[Tuple[str, str]] = set()

    # 1. 先載入現有的 lexicon_core.tsv
    if os.path.exists(core_tsv):
        with open(core_tsv, "r", encoding="utf-8") as f:
            for idx, line in enumerate(f):
                if idx == 0 or not line.strip() or line.startswith("#"):
                    continue
                parts = line.strip().split("\t")
                if len(parts) >= 2:
                    word, bpm = parts[0], parts[1]
                    weight = int(parts[2]) if len(parts) > 2 and parts[2].isdigit() else 500
                    cat = parts[3] if len(parts) > 3 else "core"
                    note = parts[4] if len(parts) > 4 else ""
                    entries.append((word, bpm, weight, cat, note))
                    seen_pairs.add((word, bpm))

    # 2. 遍歷萌典
    for item in raw_data:
        title = item.get('title', '').strip()
        # 排除包含異體字編碼佔位符如 {[8ff0]} 或標點符號的標題
        if not title or '{' in title or '}' in title:
            continue
        if re.search(r'[^\u4e00-\u9fa5]', title):
            continue

        heteronyms = item.get('heteronyms', [])
        for h in heteronyms:
            bopomofo = clean_bopomofo(h.get('bopomofo', ''))
            if not bopomofo:
                continue
            
            # 檢查注音音節數是否與字數相符
            syl_count = len(bopomofo.split())
            if syl_count != len(title):
                continue

            pair = (title, bopomofo)
            if pair in seen_pairs:
                continue
            seen_pairs.add(pair)

            # 計算預設權重
            word_len = len(title)
            base_weight = 150
            if word_len == 1:
                base_weight = 200
            elif word_len == 2:
                base_weight = 250
            elif word_len == 4:
                base_weight = 220 # 成語
            elif word_len >= 3:
                base_weight = 180

            # 檢查是否有高頻提權
            weight = CORE_BOOST_MAP.get(title, base_weight)
            category = "moe"
            if word_len == 4:
                category = "idiom"

            defs = h.get('definitions', [])
            note = defs[0].get('def', '')[:30] if defs else ""
            note = note.replace("\t", " ").replace("\n", " ")

            entries.append((title, bopomofo, weight, category, note))

    print(f">>> 成功提取 {len(entries)} 筆有效繁體詞條，正在寫入 TSV...")
    os.makedirs(os.path.dirname(output_tsv), exist_ok=True)

    with open(output_tsv, "w", encoding="utf-8") as f:
        f.write("WORD\tBOPOMOFO\tWEIGHT\tCATEGORY\tNOTE\n")
        for word, bpm, weight, cat, note in entries:
            f.write(f"{word}\t{bpm}\t{weight}\t{cat}\t{note}\n")

    print(f"✅ 繁體權威詞庫建置完成！輸出位置：{output_tsv}")
    print(f"   總詞條數: {len(entries):,} 筆")

if __name__ == "__main__":
    build_tsv()
