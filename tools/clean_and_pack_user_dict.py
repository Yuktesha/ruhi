# -*- coding: utf-8 -*-
"""
tools/clean_and_pack_user_dict.py
如喜 (RuhI) 使用者詞庫全面清查、吸收、剔除與外掛化工具

落實最高架構守則：
1. 吸收：優質通用現代詞與哲學詞彙吸收至 ruhi_lexicon/data/lexicon_domain.tsv
2. 剔除：亂碼、錯別字、ngram 殘留碎片徹底清出
3. 外掛：特定哲學宗教語境詞彙打包為 ruhi_lexicon/data/packs/pack_philosophy_ananda.tsv
4. 純淨：ruhi_sandbox/user_dict.tsv 徹底清空為發布預設狀態 (Zero Default)
"""

import os
import sys
import io
import re
import shutil

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
USER_DICT_PATH = os.path.join(REPO_ROOT, "ruhi_sandbox", "user_dict.tsv")
DOMAIN_DICT_PATH = os.path.join(REPO_ROOT, "ruhi_lexicon", "data", "lexicon_domain.tsv")
PACKS_DIR = os.path.join(REPO_ROOT, "ruhi_lexicon", "data", "packs")
ANANDA_PACK_PATH = os.path.join(PACKS_DIR, "pack_philosophy_ananda.tsv")

def main():
    print("=== 如喜使用者詞庫全面清查與淨化程序啟動 ===")
    os.makedirs(PACKS_DIR, exist_ok=True)

    if not os.path.exists(USER_DICT_PATH):
        print(f"錯誤：找不到 {USER_DICT_PATH}")
        return

    # 1. 備份現有 user_dict.tsv
    backup_path = USER_DICT_PATH + ".backup_pre_clean_20261006"
    shutil.copy2(USER_DICT_PATH, backup_path)
    print(f"已建立原始使用者詞庫備份：{backup_path}")

    # 2. 讀取現有基礎詞庫
    base_words = set()
    for fname in ["lexicon_moe.tsv", "lexicon_mcbopomofo.tsv", "lexicon_domain.tsv"]:
        p = os.path.join(REPO_ROOT, "ruhi_lexicon", "data", fname)
        if os.path.exists(p):
            with open(p, "r", encoding="utf-8") as f:
                for line in f:
                    if line.strip() and not line.startswith("#"):
                        base_words.add(line.strip().split("\t")[0])

    # 3. 讀取 user_dict.tsv
    with open(USER_DICT_PATH, "r", encoding="utf-8") as f:
        raw_rows = [l.strip().split("\t") for l in f if l.strip() and not l.startswith("#")]

    print(f"原始 user_dict 條目數：{len(raw_rows)}")

    # 虛詞碎片檢測
    particles_end = set(['的', '地', '得', '和', '與', '或', '是', '在', '了', '過', '向', '到', '把', '被', '自', '從', '你', '我', '他', '她', '它', '這', '那', '祂', '祂們'])
    particles_start = set(['的', '地', '得', '是', '在', '了', '過', '和', '與', '或', '把', '被', '從', '到', '這', '那', '祂'])
    valid_exceptions = set(['了悟', '不再', '不必', '正是', '即是', '總是', '但是', '若是', '或是', '甚至', '如何', '為何', '為了', '除了', '在於', '在於此', '這就是'])

    typo_words = set([
        '震漢', '照版', '任合', '類是', '做著', '我覺', '對未', '力竭地', '竭地', '各別', '案兩下',
        '脩試晬', '何祂', '意指陷', '前程支薪', '應拙', '應巷', '以極', '羽翮做', '類渴', '線漢',
        '覆圉', '他圉', '法秘', '以純', '奇字深語', '計一', '誤認直覺', '豐譯', '故盡', '未知道',
        '許會', '即古', '頓織', '達才', '漠視至', '源緊緊相繫', '試煉及考驗', '花盛', '世界帶'
    ])

    # 精選優質詞彙 (吸收至 lexicon_domain.tsv)
    absorb_map = {
        '如喜輸入法': ('ㄖㄨˊ ㄒㄧˇ ㄕㄨ ㄖㄨˋ ㄈㄚˇ', 1700, 'domain', '輸入法名稱'),
        '沙盒': ('ㄕㄚ ㄏㄜˊ', 1400, 'domain', '電腦安全與測試環境'),
        '大宇宙': ('ㄉㄚˋ ㄩˇ ㄓㄡˋ', 1300, 'domain', '哲學與天文概念'),
        '小宇宙': ('ㄒㄧㄠˇ ㄩˇ ㄓㄡˋ', 1300, 'domain', '哲學與心理概念'),
        '生命真諦': ('ㄕㄥ ㄇㄧㄥˋ ㄓㄣ ㄉㄧˋ', 1350, 'domain', '哲學與人生名詞'),
        '心靈科學': ('ㄒㄧㄣ ㄌㄧㄥˊ ㄎㄜ ㄒㄩㄝˊ', 1400, 'domain', '心理與認知學術名詞'),
        '科學觀': ('ㄎㄜ ㄒㄩㄝˊ ㄍㄨㄢ', 1350, 'domain', '哲學認識論'),
        '靈修': ('ㄌㄧㄥˊ ㄒㄧㄡ', 1300, 'domain', '心靈修行'),
        '唱頌': ('ㄔㄤˋ ㄙㄨㄥˋ', 1300, 'domain', '音樂與宗教吟唱'),
        '粗鈍': ('ㄘㄨ ㄉㄨㄣˋ', 1250, 'domain', '哲學描述'),
        '細緻': ('ㄒㄧˋ ㄓˋ', 1350, 'domain', '形容精緻精細'),
        '觀想': ('ㄍㄨㄢ ㄒㄧㄤˇ', 1300, 'domain', '心理專注與冥想'),
        '禮敬': ('ㄌㄧˇ ㄐㄧㄥˋ', 1300, 'domain', '敬重禮拜'),
        '脈輪': ('ㄇㄞˋ ㄌㄨㄣˊ', 1300, 'domain', '身心能量中心'),
        '高聲疾呼': ('ㄍㄠ ㄕㄥ ㄐㄧˊ ㄏㄨ', 1400, 'domain', '成語'),
        '預行編目': ('ㄩˋ ㄒㄧㄥˊ ㄅㄧㄢ ㄇㄨˋ', 1350, 'domain', '圖書館學名詞'),
        '耆那教': ('ㄑㄧˊ ㄋㄚˋ ㄐㄧㄠˋ', 1300, 'domain', '東方宗教'),
        '新倫理學': ('ㄒㄧㄣ ㄌㄨㄣˊ ㄌㄧˇ ㄒㄩㄝˊ', 1350, 'domain', '哲學術語'),
        '社經': ('ㄕㄜˋ ㄐㄧㄥ', 1350, 'domain', '社會經濟簡稱'),
        '聖諦': ('ㄕㄥˋ ㄉㄧˋ', 1300, 'domain', '佛教哲學術語'),
        '大法會': ('ㄉㄚˋ ㄈㄚˇ ㄏㄨㄟˋ', 1350, 'domain', '宗教集會'),
        '濃濁度': ('ㄋㄨㄥˊ ㄓㄨㄛˊ ㄉㄨˋ', 1300, 'domain', '物理與化學術語'),
        '碳質食糧': ('ㄊㄢˋ ㄓˊ ㄕˊ ㄌㄧㄤˊ', 1300, 'domain', '營養與生理名詞'),
        '習靜': ('ㄒㄧˊ ㄐㄧㄥˋ', 1300, 'domain', '靜心練習'),
        '殊音': ('ㄕㄨ ㄧㄣ', 1250, 'domain', '特殊音聲')
    }

    purged_count = 0
    absorbed_entries = []
    pack_entries = []

    for r in raw_rows:
        w = r[0]
        pinyin = r[1] if len(r) > 1 else ''
        wt = r[2] if len(r) > 2 else '1200'
        tag = r[3] if len(r) > 3 else 'domain'
        note = r[4] if len(r) > 4 else ''

        # 1. 亂碼、英文字、錯別字
        if '?' in w or '?' in pinyin or re.search(r'[a-zA-Z]', w) or w in typo_words:
            purged_count += 1
            continue

        # 2. 虛詞/ngram 殘留碎片
        if len(w) == 2 and w not in valid_exceptions and (w[0] in particles_start or w[1] in particles_end):
            purged_count += 1
            continue

        if len(w) >= 3 and w not in valid_exceptions and (w[0] in particles_start or w[-1] in particles_end):
            purged_count += 1
            continue

        # 3. 基礎庫已收錄
        if w in base_words and w not in absorb_map:
            purged_count += 1
            continue

        # 4. 吸收至 domain
        if w in absorb_map:
            p, norm_wt, t, n = absorb_map[w]
            absorbed_entries.append((w, p, norm_wt, t, n))
            continue

        # 5. 阿南達瑪迦哲學專用詞（歸入選配外掛詞庫，權重正規化為 900~1200）
        safe_wt = min(int(wt), 1200) if wt.isdigit() else 1200
        safe_wt = max(safe_wt, 900)
        pack_entries.append((w, pinyin, safe_wt, 'pack_philosophy', note or '阿南達瑪迦哲學選配詞條'))

    print(f"清查完成：")
    print(f"  - 剔除廢棄（錯字/亂碼/碎片/已存在）：{purged_count} 筆")
    print(f"  - 吸收至系統領域詞庫：{len(absorbed_entries)} 筆")
    print(f"  - 打包為可裝卸外掛詞庫：{len(pack_entries)} 筆")

    # 4. 寫入 lexicon_domain.tsv (吸收)
    existing_domain_words = set()
    with open(DOMAIN_DICT_PATH, "r", encoding="utf-8") as f:
        existing_domain_lines = f.readlines()
        for line in existing_domain_lines:
            if line.strip() and not line.startswith("#"):
                existing_domain_words.add(line.strip().split("\t")[0])

    new_absorb_count = 0
    with open(DOMAIN_DICT_PATH, "a", encoding="utf-8", newline="\n") as f:
        f.write("\n# === 2026-10-06 使用者詞庫精選吸收之優質現代詞 ===\n")
        for item in absorbed_entries:
            w, p, wt, t, n = item
            if w not in existing_domain_words:
                f.write(f"{w}\t{p}\t{wt}\t{t}\t{n}\n")
                new_absorb_count += 1
                existing_domain_words.add(w)

    print(f"成功將 {new_absorb_count} 筆優質現代詞收編至 {DOMAIN_DICT_PATH}")

    # 5. 輸出可裝卸外掛詞庫 pack_philosophy_ananda.tsv
    with open(ANANDA_PACK_PATH, "w", encoding="utf-8", newline="\n") as f:
        f.write("# 如喜 (RuhI) 專業/語境選配詞庫包 (Pluggable Lexicon Pack)\n")
        f.write("# 領域：阿南達瑪迦心靈哲學與專用術語 (Philosophy & Spirituality)\n")
        f.write("# 格式：詞彙\\t注音\\t權重\\t標籤\\t備註\n")
        f.write("# 說明：此詞庫為獨立外掛包，權重已正規化至 900~1200 安全範圍，支援手動/自動裝卸\n#\n")
        for item in pack_entries:
            w, p, wt, t, n = item
            f.write(f"{w}\t{p}\t{wt}\t{t}\t{n}\n")

    print(f"成功建立可裝卸專業外掛詞庫：{ANANDA_PACK_PATH} ({len(pack_entries)} 筆)")

    # 6. 徹底清空發布時的 user_dict.tsv
    with open(USER_DICT_PATH, "w", encoding="utf-8", newline="\n") as f:
        f.write("# 如喜 (RuhI) 使用者自建詞庫 (User Dictionary)\n")
        f.write("# 格式: 詞彙\t注音\t權重\t標籤\t備註\n")
        f.write("# 發布狀態：純淨空檔 (Clean Zero-Default)\n")
        f.write("# 說明：系統開箱即用已由內建詞庫覆蓋；使用者日常輸入自建詞將自動儲存於此\n#\n")

    print(f"成功將使用者自建詞庫重設為純淨發布狀態：{USER_DICT_PATH} (0 條殘留)")
    print("=== 全體清查、吸收、剔除與外掛化作業順利達成！ ===")

if __name__ == "__main__":
    main()
