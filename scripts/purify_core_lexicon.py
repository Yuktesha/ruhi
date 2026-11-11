#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RuhI (如喜) 核心詞庫淨化與代數連讀變調索引生成器
檔案路徑: scripts/purify_core_lexicon.py
用途:
  1. 根絕連讀變調 (ㄧ/ㄧˊ/ㄧˋ、ㄅㄨˋ/ㄅㄨˊ、ㄍㄜˋ/˙ㄍㄜ) 與輕聲重複排列組合之冗餘列 (如「一丁不識」3列➔1列、「一個」5列➔1列)。
  2. 剔除野生隨機數字 (一七三、一七九等) 與非詞化語料庫殘片 (一下子轉涼、一些的、也改了、一不、一個小時等)。
  3. 平滑過膨脹權重 (1480 ➔ [580, 880]) 與退化變體權重 (1, 8, 24)，確立正規化尺度 [60, 1000]。
  4. 裁撤冗餘 CATEGORY 與未被引擎引用的 NOTE 欄位，確立純淨 3 欄格式 (WORD \t BOPOMOFO \t WEIGHT)。
  5. 嚴格保留古典 2 字保留詞 (權重 75，小於 90，供連續句 DAG 跨界防護)。
"""

import os
import re
import math
import shutil
import subprocess

def strip_tones(bpm):
    return bpm.replace('ˊ', '').replace('ˇ', '').replace('ˋ', '').replace('˙', '')

def norm_syllable(s):
    if s.endswith('˙'):
        return '˙' + s[:-1]
    return s

def normalize_bpm(bpm):
    return ' '.join(norm_syllable(s) for s in bpm.split())

def canonicalize_sandhi(word, bpm_str):
    chars = list(word)
    syls = bpm_str.split()
    if len(chars) != len(syls):
        return bpm_str
    
    # 由右向左推導（確保「一不做」之「不」先確立為 ㄅㄨˊ 或 ㄅㄨˋ，再決定前接之「一」為 ㄧˋ 或 ㄧˊ）
    for i in range(len(chars) - 1, -1, -1):
        ch = chars[i]
        if ch == '不':
            if i + 1 < len(syls) and syls[i + 1].endswith('ˋ'):
                syls[i] = 'ㄅㄨˊ'
            else:
                syls[i] = 'ㄅㄨˋ'
        elif ch == '一':
            is_at_end = (i == len(chars) - 1)
            is_preceded_by_num_or_prefix = (i > 0 and chars[i - 1] in '第初零〇○一二三四五六七八九十百千萬')
            is_followed_by_yi = (i + 1 < len(chars) and chars[i + 1] == '一')
            if is_at_end or is_preceded_by_num_or_prefix or is_followed_by_yi:
                syls[i] = 'ㄧ'
            elif i + 1 < len(syls):
                next_syl = syls[i + 1]
                next_ch = chars[i + 1]
                if next_syl.endswith('ˋ') or (next_ch in ('個', '下') and next_syl in ('˙ㄍㄜ', 'ㄍㄜˋ', '˙ㄒㄧㄚ', 'ㄒㄧㄚˋ')):
                    syls[i] = 'ㄧˊ'
                else:
                    syls[i] = 'ㄧˋ'
            else:
                syls[i] = 'ㄧ'
    return ' '.join(syls)

def are_readings_equivalent(word, syls_a, syls_b):
    chars = list(word)
    if len(chars) != len(syls_a) or len(chars) != len(syls_b) or len(chars) < 2:
        return syls_a == syls_b

    # 雙字疊字變調（謝謝、水水）
    if len(chars) == 2 and chars[0] == chars[1] and strip_tones(syls_a[0]) == strip_tones(syls_a[1]):
        if strip_tones(syls_b[0]) == strip_tones(syls_b[1]) and strip_tones(syls_a[0]) == strip_tones(syls_b[0]):
            return True

    for i in range(len(chars)):
        ch = chars[i]
        sa = syls_a[i]
        sb = syls_b[i]
        if sa == sb:
            continue
        if ch == '一' and sa in ('ㄧ', 'ㄧˊ', 'ㄧˋ') and sb in ('ㄧ', 'ㄧˊ', 'ㄧˋ'):
            continue
        if ch == '不' and sa in ('ㄅㄨˋ', 'ㄅㄨˊ') and sb in ('ㄅㄨˋ', 'ㄅㄨˊ'):
            continue
        if ch == '個' and sa in ('ㄍㄜˋ', '˙ㄍㄜ') and sb in ('ㄍㄜˋ', '˙ㄍㄜ'):
            continue
        if i + 1 == len(chars) and ch == '兒' and sa in ('ㄦ', 'ㄦˊ') and sb in ('ㄦ', 'ㄦˊ'):
            continue
        if i > 0 and (sa.startswith('˙') or sb.startswith('˙')) and strip_tones(sa) == strip_tones(sb):
            continue
        return False
    return True

def get_protected_words(ruhi_dir):
    protected = set()
    reach_path = os.path.join(ruhi_dir, 'ruhi_lexicon', 'data', 'reachability.tsv')
    if os.path.exists(reach_path):
        with open(reach_path, 'r', encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith('#'):
                    continue
                parts = line.split('\t')
                if parts:
                    protected.add(parts[0].strip())

    files_to_scan = [
        os.path.join(ruhi_dir, 'ruhi-core', 'src', 'engine.rs'),
        os.path.join(ruhi_dir, 'ruhi-core', 'src', 'lexicon', 'manager.rs'),
        os.path.join(ruhi_dir, 'ruhi-studio', 'src', 'tuner.rs'),
    ]
    cjk_re = re.compile(r'[\u4e00-\u9fff]+')
    for fpath in files_to_scan:
        if os.path.exists(fpath):
            with open(fpath, 'r', encoding='utf-8') as f:
                for m in cjk_re.finditer(f.read()):
                    seq = m.group(0)
                    for length in range(1, 9):
                        for start in range(len(seq) - length + 1):
                            protected.add(seq[start : start + length])
    return protected

def main():
    ruhi_dir = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
    core_tsv = os.path.join(ruhi_dir, 'ruhi_lexicon', 'data', 'lexicon_core.tsv')
    print(f"Loading files from repo: {ruhi_dir}")

    protected = get_protected_words(ruhi_dir)
    print(f"Protected tokens scanned: {len(protected)}")

    moe_words = set()
    mcb_words = set()
    try:
        out_moe = subprocess.check_output(['git', 'show', 'HEAD:ruhi_lexicon/data/lexicon_moe.tsv'], cwd=ruhi_dir).decode('utf-8')
        for line in out_moe.splitlines():
            p = line.split('\t')
            if p:
                moe_words.add(p[0].strip())
    except Exception as e:
        print(f"Warning: could not get git show lexicon_moe.tsv: {e}")

    try:
        out_mcb = subprocess.check_output(['git', 'show', 'HEAD:ruhi_lexicon/data/lexicon_mcbopomofo.tsv'], cwd=ruhi_dir).decode('utf-8')
        for line in out_mcb.splitlines():
            p = line.split('\t')
            if p:
                mcb_words.add(p[0].strip())
    except Exception as e:
        print(f"Warning: could not get git show lexicon_mcbopomofo.tsv: {e}")

    domain_words = {}
    try:
        out_domain = subprocess.check_output(['git', 'show', 'HEAD:ruhi_lexicon/data/lexicon_domain.tsv'], cwd=ruhi_dir).decode('utf-8')
        for line in out_domain.splitlines():
            p = line.split('\t')
            if len(p) >= 3 and p[2].isdigit():
                w = p[0].strip()
                domain_words[w] = max(domain_words.get(w, 0), int(p[2].strip()))
                protected.add(w)
    except Exception as e:
        print(f"Warning: could not get git show lexicon_domain.tsv: {e}")

    print(f"Attested words: moe={len(moe_words)}, mcb={len(mcb_words)}, domain={len(domain_words)}")

    num_chars = set('零〇○一二三四五六七八九十百千萬億兩點之壹貳參肆伍陸柒捌玖拾佰仟多餘')
    valid_num_words = {
        '一一','一二','一五一十','十之八九','三三兩兩','三七二十一','九九','九九八十一',
        '萬一','萬千','萬萬','千千萬萬','點點','零點','零零','之一','之二','之多','之餘','三多','三餘','一點','一點點','一兩',
        '十一','十二','十三','十四','十五','十六','十七','十八','十九','二十','三十','四十','五十',
        '六十','七十','八十','九十','一百','一千','一萬','十萬','百萬','千萬','一億','十億','百億','千億','億萬',
        '兩百','兩千','兩萬','兩億','兩兩','二二八','九二一','九一一','九一八','一一九','一零一','一萬二',
        '二十一','二十二','二十三','二十六','四十五','二千','三千','三百','五百'
    }
    explicit_drops = {
        '一十一五', '三十七八', '二十一二十二', '十十一二', '十六十五', '一丙', '一不',
        '一丁目', '一二樓', '一些事', '一併處理', '一個不小心', '一個中國政策', '一個人住',
        '一個小時', '一個小時後', '一個半', '一個字', '一個樣', '一個鼻孔', '好叫',
        '一個獵古調', '一仟多'
    }
    allowed_de = {'目的','標的','的確','有的','是的','真的','別的','女的','男的'}
    allowed_de2 = {'怪不得','免不得','捨不得','巴不得','由不得','得得'}
    allowed_ge = {'這個','那個','一個','每個','哪個','整個','別個'}
    allowed_le = {'為了','除了','算了','罷了','完了','好了','對了','極了','得了','知了','不了','大不了','免不了','受不了','少不了','怪不得','了不起'}
    unwanted_yi_heads = {'一住','一停','一做','一上','一上來','一上去','一上車','一下山','一下手','一下水','一下班','一下車','一下雨','一不作','一不做','一中一台','一中各表'}

    measure_words = set('台場群堆批雙對套組張顆隻條件份頁篇首部棟座間家所處塊片杯碗盤瓶包盒袋箱輛艘架班列節門堂屆任代輩秒分週周')

    raw_entries = []
    total_raw_lines = 0

    with open(core_tsv, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            total_raw_lines += 1
            parts = line.split('\t')
            if len(parts) < 2:
                continue
            word = parts[0].strip()
            bpm = parts[1].strip()
            weight = int(parts[2].strip()) if len(parts) >= 3 and parts[2].strip().isdigit() else 150
            cat = parts[3].strip() if len(parts) >= 4 else ""
            note = parts[4].strip() if len(parts) >= 5 else ""
            raw_entries.append({
                'word': word,
                'bpm': normalize_bpm(bpm),
                'weight': weight,
                'cat': cat,
                'note': note,
            })

    print(f"Total non-comment raw lines loaded: {len(raw_entries)}")

    # 1. 第一階段過濾：過濾野生隨機數字與非詞化語料碎片
    filtered_entries = []
    dropped_num_count = 0
    dropped_ngram_count = 0

    for e in raw_entries:
        w = e['word']
        note = e['note']
        cat = e['cat']
        wt = e['weight']

        # 過濾非標準注音字元或音節數不符之異常列（如 aup檔、各clip、含逗號之長句）
        valid_bpm_chars = set('ㄅㄆㄇㄈㄉㄊㄋㄌㄍㄎㄏㄐㄑㄒㄓㄔㄕㄖㄗㄘㄙㄧㄨㄩㄚㄛㄜㄝㄞㄟㄠㄡㄢㄣㄤㄥㄦˊˇˋ˙ ')
        if not e['bpm'] or any(c not in valid_bpm_chars for c in e['bpm']) or len(w) != len(e['bpm'].split()):
            dropped_ngram_count += 1
            continue

        # 萌典罕用古音(零複詞)
        if wt == 35 or '萌典罕用古音' in note:
            continue

        # 野生數字過濾
        if w in explicit_drops:
            dropped_num_count += 1
            continue

        if len(w) >= 2 and all(c in num_chars for c in w) and w not in valid_num_words:
            dropped_num_count += 1
            continue

        # 透明句法短語 X+的
        if len(w) >= 2 and w.endswith('的') and w not in moe_words and w not in protected and w not in allowed_de:
            dropped_ngram_count += 1
            continue

        # 透明句法短語 X+了
        if len(w) >= 2 and w.endswith('了') and w not in moe_words and w not in protected and w not in allowed_le:
            dropped_ngram_count += 1
            continue

        # 透明句法短語 X+得
        if len(w) >= 2 and w.endswith('得') and w not in moe_words and w not in protected and w not in allowed_de2:
            dropped_ngram_count += 1
            continue

        # 透明句法短語 X+個
        if len(w) >= 2 and w.endswith('個') and w not in moe_words and w not in protected and w not in allowed_ge:
            dropped_ngram_count += 1
            continue

        # 年代 / 年前 / 年後
        if re.match(r'^[零一二三四五六七八九十百千萬]+年代$', w) and w not in protected:
            dropped_ngram_count += 1
            continue
        if re.search(r'[零一二三四五六七八九]+年(前|後|生|級|代|來|度|底|初)$', w) and w not in protected:
            dropped_ngram_count += 1
            continue

        # 過濾萌典中未見於現代語料庫之長句俗諺（長度 >= 8，或長度 >= 6 且含句法助詞/口語襯字）
        if len(w) >= 6 and w not in mcb_words and w not in protected:
            if len(w) >= 8 or any(c in w for c in ('的', '了', '得', '兒', '個', '著', '過', '嗎', '呢')):
                dropped_ngram_count += 1
                continue

        # 未被保護之小麥獨有稀有殘片
        if w not in moe_words and w not in protected:
            m_occ = re.search(r'occ=(\d+)', note)
            occ_val = int(m_occ.group(1)) if m_occ else None
            if occ_val is not None:
                if occ_val <= 2 or (len(w) >= 4 and occ_val <= 5):
                    dropped_ngram_count += 1
                    continue

            # 一 + V / 量詞短語
            if w.startswith('一'):
                if len(w) == 3 and w[2] in ('來', '去', '到') and w not in ('一起來', '一再來'):
                    dropped_ngram_count += 1
                    continue
                if len(w) == 3 and w[2] in measure_words:
                    dropped_ngram_count += 1
                    continue
                if w in unwanted_yi_heads:
                    dropped_ngram_count += 1
                    continue

        filtered_entries.append(e)

    print(f"Dropped wild numbers: {dropped_num_count}, dropped n-gram phrases: {dropped_ngram_count}")
    print(f"Remaining entries after filter: {len(filtered_entries)}")

    # 2. 第二階段：按 (詞彙) 進行連讀變調與輕聲等價類聚合
    words_map = {}
    for e in filtered_entries:
        words_map.setdefault(e['word'], []).append(e)

    special_weights = {
        '的': 1000,
        '一個': 880,
        '一下': 786,
        '一句': 860,
        '不再': 591,
        '一試': 500,
        '一再': 580,
        '一丁不識': 120,
        '一個人': 649,
        '一下子': 512,
        '一五一十': 500,
        '一分一秒': 500,
        '雪上加霜': 500,
        '錦上添花': 500,
        '鋌而走險': 500,
    }

    final_entries = []

    for word, entries in words_map.items():
        # 將該詞的所有行切分成等價類群組
        groups = [] # list of lists of entries
        for e in entries:
            syls = e['bpm'].split()
            placed = False
            for grp in groups:
                rep_syls = grp[0]['bpm'].split()
                if are_readings_equivalent(word, rep_syls, syls):
                    grp.append(e)
                    placed = True
                    break
            if not placed:
                groups.append([e])

        # 如果該詞有大於 1 個等價群組，且其中某些讀音為明顯的小麥錯字（如 一個勁兒 的 ㄐㄧㄥˋ vs ㄐㄧㄣˋ）
        if len(groups) > 1:
            max_w_all = max(max(x['weight'] for x in grp) for grp in groups)
            surviving_groups = []
            for grp in groups:
                max_w_grp = max(x['weight'] for x in grp)
                # 若已有正常讀音 (>= 100) 且此群組最高分 < 80，視為訛字/低權重變體拋棄
                if max_w_all >= 100 and max_w_grp < 80:
                    continue
                # 專門檢查 一個勁兒 ㄐㄧㄥˋ 訛音
                if word == '一個勁兒' and any('ㄐㄧㄥ' in x['bpm'] for x in grp):
                    continue
                surviving_groups.append(grp)
            if surviving_groups:
                groups = surviving_groups

        # 對每個存活等價群組挑選唯一的標準規範行
        for grp in groups:
            # 決定候選代表讀音：
            # 優先權：
            # 1. 若後綴為 子、頭、們、個、下（如 一個、一下子）：偏好帶 prefix ˙ 輕聲
            # 2. 否則（如 知識、認識、告訴）：偏好本調/滿調 (無 ˙)
            # 3. 權重高者優先
            def score_candidate(cand):
                bpm = cand['bpm']
                syls = bpm.split()
                has_suffix_neutral = False
                if len(syls) >= 2:
                    last_ch = word[-1]
                    last_syl = syls[-1]
                    if last_ch in ('子', '頭', '們', '個', '下') and last_syl.startswith('˙'):
                        has_suffix_neutral = True
                    if len(word) >= 2 and word[:2] in ('一個', '一下') and len(syls) >= 2 and syls[1].startswith('˙'):
                        has_suffix_neutral = True

                has_any_neutral = any(s.startswith('˙') for s in syls)
                pref_neutral = 1 if (has_suffix_neutral or (word in ('一個', '一下子') and has_any_neutral)) else 0
                if not (word.endswith(('子', '頭', '們', '個', '下')) or word in ('一個', '一下子')):
                    # 標準詞彙偏好無輕聲
                    pref_neutral = -1 if has_any_neutral else 1

                return (cand['weight'], pref_neutral)

            best_entry = max(grp, key=score_candidate)
            base_bpm = best_entry['bpm']

            # 套用標準規範連讀變調（一、不）
            canonical_bpm = canonicalize_sandhi(word, base_bpm)

            # 計算最終正規化權重
            max_raw_weight = max(x['weight'] for x in grp)
            combined_cat = next((x['cat'] for x in grp if x['cat']), "")
            combined_note = next((x['note'] for x in grp if x['note']), "")

            # 檢查是否為無現代語料佐證之古語雙字保留詞 (moe_unattested2)
            is_unattested2 = (
                word == '舊好'
                or (
                    len(word) == 2
                    and word not in mcb_words
                    and word not in protected
                    and word not in ("參雜", "耆德", "衚衕")
                    and (combined_cat in ('moe_unattested2', 'moe', 'word2') or max_raw_weight <= 180)
                )
            )

            if word in special_weights:
                final_weight = special_weights[word]
            elif word == '句' and 'ㄐㄩˋ' in canonical_bpm:
                final_weight = 750
            elif word == '倒' and 'ㄉㄠˇ' in canonical_bpm:
                final_weight = 780
            elif word == '救好':
                final_weight = 350
            elif is_unattested2:
                final_weight = 75
            elif word in domain_words:
                dw = domain_words[word]
                if dw > 1000:
                    if dw <= 1350:
                        final_weight = int(round(580 + (dw - 1001) / 349.0 * 200))
                    else:
                        final_weight = int(round(680 + min(1.0, (dw - 1351) / 1050.0) * 200))
                else:
                    final_weight = max(100, dw)
            else:
                # 現代詞彙處理
                final_weight = max_raw_weight
                # 校正早期萌典對未見於現代語料庫之生僻成語齊頭式灌入之 550 高分，回歸標準成語基準 180
                if final_weight == 550 and word not in mcb_words and word not in protected:
                    final_weight = 180
                elif final_weight > 1000:
                    m_occ = re.search(r'occ=(\d+)', combined_note)
                    occ_val = int(m_occ.group(1)) if m_occ else 0
                    if len(word) == 4 and (combined_cat == 'idiom' or '成語' in combined_note or '《' in combined_note):
                        final_weight = 500
                    elif occ_val > 0:
                        final_weight = max(120, min(920, int(round(100 + 85 * math.log10(max(1, occ_val))))))
                    elif final_weight <= 1350:
                        final_weight = int(round(580 + (final_weight - 1001) / 349.0 * 200))
                    else:
                        final_weight = int(round(680 + min(1.0, (final_weight - 1351) / 1050.0) * 200))
                
                # 確保現代詞彙權重 >= 100，絕不掉入 < 90 的古語專屬區間
                if final_weight < 100:
                    final_weight = 100
                elif final_weight > 1000:
                    final_weight = 1000

            final_entries.append({
                'word': word,
                'bpm': canonical_bpm,
                'weight': final_weight,
            })

    # 排序：
    # Tier 0: 常用標準漢字 (\u4E00..\u9FFF) 排最前（打開檔案第 4 行即為「一」、「一一」、「一丁」、「一丁不識」）
    # Tier 1: 注音符號與「〇」
    # Tier 2: 罕用擴充漢字 CJK Extension A (\u3400..\u4DBF) 置於檔案最末段
    def word_sort_tier(w):
        c0 = w[0]
        if '\u4e00' <= c0 <= '\u9fff':
            return 0
        elif '\u3400' <= c0 <= '\u4dbf':
            return 2
        else:
            return 1

    final_entries.sort(key=lambda e: (word_sort_tier(e['word']), e['word'], -e['weight'], e['bpm']))

    # 去重（若同一 word 與 bpm 有多筆，僅保留最高權重者）
    deduped_entries = []
    seen = set()
    for e in final_entries:
        key = (e['word'], e['bpm'])
        if key not in seen:
            seen.add(key)
            deduped_entries.append(e)

    print(f"Final purified unique entries: {len(deduped_entries)}")

    # 寫入單一統整核心詞庫 (3 欄格式)
    header = [
        "# RuhI (如喜) 通用智慧型輸入法 — 單一統整核心詞庫 (Unified Core Lexicon)",
        "# 欄位格式: WORD\tBOPOMOFO\tWEIGHT",
        "# 致敬與開源授權聲明請參閱專案根目錄 THIRD_PARTY_LICENSES.md",
    ]

    target_paths = [
        core_tsv,
        os.path.join(ruhi_dir, 'data', 'lexicon_core.tsv'),
        os.path.join(ruhi_dir, 'scripts', 'data', 'lexicon_core.tsv'),
        os.path.join(ruhi_dir, 'dist', 'data', 'lexicon_core.tsv'),
    ]

    for p in target_paths:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, 'w', encoding='utf-8') as out:
            for h in header:
                out.write(h + '\n')
            for e in deduped_entries:
                out.write(f"{e['word']}\t{e['bpm']}\t{e['weight']}\n")
        print(f"Successfully synced: {p}")

    print("Stage 2 purification finished successfully!")

if __name__ == '__main__':
    main()
