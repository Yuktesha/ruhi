#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）現代台灣正體與技術領域 OOV 詞彙自動探勘工具 (mine_oov_lexicon.py)

從商業授權安全之台灣正體純淨語料庫（例如 `eval_reports/local/d3_work/corpus.txt`）執行
低記憶體 3-Pass Apriori 統計探勘與第 4 Pass 邊界資訊熵檢核，並結合：
1. `taiwan-corpora/twfilter-tables` 與台灣技術詞彙淨化規則（零簡體／零中國網通／零地方方言字）；
2. 反透明句法短語過濾（禁止「的／得／地／了／著／過」或功能詞綴碎片進入詞庫）；
3. 最小內聚互資訊量 (`PMI_min`) 與左右鄰字資訊熵 (`min(H_L, H_R)`)；
4. 單音字自動定音 + 連讀變調正規化 (`canonicalize_sandhi`) vs 多音字獨立人工覆核佇列；
5. 現有 2–4 字低權重常用詞 (`100 <= weight <= 240`, `count >= 400`) 校準稽核清單。

用法：
    python scripts/mine_oov_lexicon.py ^
      --corpus eval_reports/local/d3_work/corpus.txt ^
      --lexicon ruhi_lexicon/data/lexicon_core.tsv ^
      --out-dir eval_reports/local/oov_wave_b
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import re
import sys
import time

SCRIPT_DIR = Path(__file__).resolve().parent
PYDEPS_DIR = SCRIPT_DIR.parent / "eval_reports" / "local" / "pydeps"
for _p in (str(SCRIPT_DIR), str(PYDEPS_DIR)):
    if _p not in sys.path and Path(_p).exists():
        sys.path.insert(0, _p)

from opencc import OpenCC  # noqa: E402
from purify_core_lexicon import canonicalize_sandhi, normalize_bpm  # noqa: E402
from train_ngram_lm import (  # noqa: E402
    OCR_GLITCH_BLOCK_CHARS,
    RE_CJK_RUN,
    TWFILTER_BASE_URL,
    download_text_url,
    parse_single_chars,
)

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

DEFAULT_CORPUS = SCRIPT_DIR.parent / "eval_reports" / "local" / "d3_work" / "corpus.txt"
DEFAULT_LEXICON = SCRIPT_DIR.parent / "ruhi_lexicon" / "data" / "lexicon_core.tsv"
DEFAULT_REJECTED = SCRIPT_DIR.parent / "ruhi_lexicon" / "data" / "rejected_readings.tsv"
DEFAULT_IMPOSTORS = SCRIPT_DIR.parent / "ruhi_lexicon" / "data" / "common_archaic_impostors.tsv"
DEFAULT_OUT_DIR = SCRIPT_DIR.parent / "eval_reports" / "local" / "oov_wave_b"
DEFAULT_TWFILTER_CACHE = SCRIPT_DIR.parent / "eval_reports" / "local" / "twfilter_cache"

# 明確排除未收錄或偶漏網之中國大陸資訊工程／網通黑話（僅列台灣正體嚴禁取代詞）
PRC_TECH_BLOCKLIST: tuple[str, ...] = (
    "服務器", "內存", "緩存", "算法", "代碼", "默認", "數組", "對象", "接口",
    "軟件", "硬件", "網絡", "屏幕", "視頻", "打印", "線程", "進程", "內核",
    "異步", "句柄", "指針", "模塊", "插件", "組件", "數據庫", "數據集", "字節",
    "分布式", "集群", "優盤", "顯卡", "聲卡", "網卡", "硬盤", "光驅", "主板",
    "寬帶", "聯網", "調試", "激活", "博客", "兼容", "殺毒", "軟盤", "函數庫",
    "類庫", "控件", "變量", "常量", "字段", "回調", "協程", "調用", "網關",
    "擁堵", "運營", "西蘭花",
    "寬鬆",  # 僅針對技術慣誤；若無需排除可移除—Wait：寬鬆是台灣常用合法中文！已於下移除
)
PRC_TECH_BLOCKLIST = tuple(t for t in PRC_TECH_BLOCKLIST if t != "寬鬆")

SPAM_BLOCK_SUBSTRS: tuple[str, ...] = (
    "包養", "裸聊", "啪啪", "熟女", "人妻", "約炮", "外約", "援交",
    "色情", "情色", "自慰", "做愛", "口交", "無碼", "淫", "嫖", "妓",
    "賭博", "博弈網", "娛樂城", "高利貸",
)

# 構詞與句法界線防護字集
FUNC_CHARS_STRICT = frozenset(
    "的得地了著過嗎呢吧啊呀嘛啦喔哦囉個"
    "很最太更也都就才卻把將被給讓和與及或而但因於之其所以從向對在再是有沒不會要能可"
    "我你妳他她它牠這那哪每各某此該"
)
BOUNDARY_FUNC_CHARS = frozenset(
    "第條款項起下內外較應並還且裡裏上前後者講做去來到越元歲萬億千百隻篇首段句件"
)
ALL_TWO_CHAR_FUNC_BLOCK = FUNC_CHARS_STRICT | BOUNDARY_FUNC_CHARS
PARTICLE_INTERIOR_CHARS = frozenset("的得地了著過嗎呢吧啊呀嘛啦喔哦囉個")
NUMERAL_ALL_CHARS = frozenset("零〇○一二三四五六七八九十百千萬億兩點第初多餘壹貳參肆伍陸柒捌玖拾佰仟年年月日號歲名位次個條項篇版期週")
RE_CJK_BMP_ONLY = re.compile(r"[\u4E00-\u9FFF]+")


class CachedTaiwanPurityFilter:
    """具本機磁碟快取與 OpenCC 兩岸異字檢核之台灣正體純度過濾器。"""

    def __init__(self, cache_dir: Path):
        cache_dir.mkdir(parents=True, exist_ok=True)
        self.cc_s2t = OpenCC("s2t")
        self.cc_s2twp = OpenCC("s2twp")
        files = [
            "moe_common.txt",
            "moe_secondary.txt",
            "moe_exception.txt",
            "variants_used_in_taiwan.txt",
            "simplified_only.txt",
            "cantonese.txt",
            "converted_orthography.txt",
            "mainland_hard.tsv",
            "regional_hard.tsv",
            "mainland_exceptions.tsv",
        ]
        texts: dict[str, str] = {}
        for fname in files:
            cpath = cache_dir / fname
            if cpath.is_file() and cpath.stat().st_size > 0:
                texts[fname] = cpath.read_text(encoding="utf-8", errors="replace")
            else:
                print(f"[twfilter] 下載 {fname} ...", flush=True)
                content = download_text_url(f"{TWFILTER_BASE_URL}/{fname}")
                cpath.write_text(content, encoding="utf-8")
                texts[fname] = content

        moe_common = parse_single_chars(texts["moe_common.txt"])
        moe_secondary = parse_single_chars(texts["moe_secondary.txt"])
        moe_exception = parse_single_chars(texts["moe_exception.txt"])
        variants_tw = parse_single_chars(texts["variants_used_in_taiwan.txt"])
        simplified_only = parse_single_chars(texts["simplified_only.txt"])
        cantonese = parse_single_chars(texts["cantonese.txt"])
        converted_orth = parse_single_chars(texts["converted_orthography.txt"])

        self.block_chars = frozenset(simplified_only | cantonese | converted_orth | OCR_GLITCH_BLOCK_CHARS)
        self.allowed_chars = frozenset((moe_common | moe_secondary | moe_exception | variants_tw) - self.block_chars)

        hard_terms: set[str] = set()
        for fname in ("mainland_hard.tsv", "regional_hard.tsv"):
            for line in texts[fname].splitlines():
                s = line.strip()
                if not s or s.startswith("#"):
                    continue
                term = s.split("\t", 1)[0].strip()
                if len(term) >= 2:
                    hard_terms.add(term)

        exceptions_map: dict[str, list[tuple[str, int]]] = defaultdict(list)
        for line in texts["mainland_exceptions.tsv"].splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            cols = [c.strip() for c in s.split("\t") if c.strip()]
            if len(cols) >= 2:
                term = cols[0]
                for exc in cols[1:]:
                    start = 0
                    while True:
                        idx = exc.find(term, start)
                        if idx < 0:
                            break
                        exceptions_map[term].append((exc, idx))
                        start = idx + 1

        by_first: dict[str, list[str]] = defaultdict(list)
        for t in hard_terms:
            by_first[t[0]].append(t)
        self.hard_by_first = {k: tuple(sorted(v, key=len, reverse=True)) for k, v in by_first.items()}
        self.hard_first_chars = frozenset(self.hard_by_first.keys())
        self.exceptions_map = {k: tuple(v) for k, v in exceptions_map.items()}
        print(
            f"[twfilter] 就緒：允許字={len(self.allowed_chars)} | 黑名單詞={len(hard_terms)}",
            flush=True,
        )

    def is_taiwan_clean(self, word: str) -> bool:
        if any(ch not in self.allowed_chars for ch in word):
            return False
        if any(prc in word for prc in PRC_TECH_BLOCKLIST):
            return False
        if any(sp in word for sp in SPAM_BLOCK_SUBSTRS):
            return False
        if self.cc_s2t.convert(word) != word or self.cc_s2twp.convert(word) != word:
            return False
        if not self.hard_first_chars.isdisjoint(word):
            for i, ch in enumerate(word):
                terms = self.hard_by_first.get(ch)
                if not terms:
                    continue
                for term in terms:
                    if word.startswith(term, i):
                        excs = self.exceptions_map.get(term)
                        exempt = False
                        if excs:
                            for exc_str, off in excs:
                                pos = i - off
                                if pos >= 0 and word.startswith(exc_str, pos):
                                    exempt = True
                                    break
                        if not exempt:
                            return False
        return True


def load_lexicon_and_guards(
    lexicon_path: Path, rejected_path: Path, impostors_path: Path
):
    rejected_pairs: set[tuple[str, str]] = set()
    rejected_words: set[str] = set()
    if rejected_path.is_file():
        for line in rejected_path.read_text(encoding="utf-8").splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            p = s.split("\t")
            if len(p) >= 2:
                w = p[0].strip()
                b = p[1].strip()
                if b == "*":
                    rejected_words.add(w)
                else:
                    rejected_pairs.add((w, normalize_bpm(b)))

    impostor_pairs: set[tuple[str, str]] = set()
    if impostors_path.is_file():
        for line in impostors_path.read_text(encoding="utf-8").splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            p = s.split("\t")
            if len(p) >= 2:
                impostor_pairs.add((p[0].strip(), normalize_bpm(p[1].strip())))

    existing_words: dict[str, list[tuple[str, int]]] = defaultdict(list)
    char_readings: dict[str, list[tuple[str, int]]] = defaultdict(list)

    with lexicon_path.open("r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            p = s.split("\t")
            if len(p) < 3:
                continue
            w = p[0].strip()
            b = normalize_bpm(p[1].strip())
            wt = int(p[2].strip())
            existing_words[w].append((b, wt))
            if len(w) == 1:
                if w not in rejected_words and (w, b) not in rejected_pairs and (w, b) not in impostor_pairs:
                    char_readings[w].append((b, wt))

    # 判定嚴格單音字與多音候選讀音排序
    monophonic_map: dict[str, str] = {}
    char_sorted_readings: dict[str, list[tuple[str, int]]] = {}
    for ch, rlist in char_readings.items():
        # 按權重由高至低合併同音
        merged: dict[str, int] = {}
        for b, wt in rlist:
            if wt > merged.get(b, 0):
                merged[b] = wt
        ordered = sorted(merged.items(), key=lambda kv: (-kv[1], kv[0]))
        if not ordered:
            continue
        char_sorted_readings[ch] = ordered
        max_w = ordered[0][1]
        filtered = [
            (b, wt)
            for b, wt in ordered
            if not (max_w >= 300 and wt < 120)
        ]
        if not filtered:
            filtered = ordered[:1]
        if len(filtered) == 1:
            monophonic_map[ch] = filtered[0][0]
        else:
            w1 = filtered[0][1]
            w2 = filtered[1][1]
            if w1 >= 5 * w2 and w2 < 180:
                monophonic_map[ch] = filtered[0][0]

    return existing_words, rejected_words, rejected_pairs, monophonic_map, char_sorted_readings


def is_syntactic_or_numeral_fragment(word: str) -> bool:
    n = len(word)
    if all(ch in NUMERAL_ALL_CHARS for ch in word):
        return True
    # 常見日期或序數碎片（第X／X年／X月／X日／X名）
    if re.search(r"^[一二三四五六七八九十零〇兩百千]+[年月日號名位個條項屆代期次]|^第[一二三四五六七八九十零]+", word):
        return True
    if n == 2:
        if any(ch in ALL_TWO_CHAR_FUNC_BLOCK for ch in word):
            return True
    else:
        if (
            word[0] in ALL_TWO_CHAR_FUNC_BLOCK
            or word[-1] in ALL_TWO_CHAR_FUNC_BLOCK
        ):
            return True
        if any(ch in PARTICLE_INTERIOR_CHARS for ch in word):
            return True
    return False


def shannon_entropy_and_max_dom(counts: Counter) -> tuple[float, float]:
    tot = sum(counts.values())
    if tot <= 0:
        return 0.0, 1.0
    ent = 0.0
    max_cnt = 0
    for c in counts.values():
        if c > max_cnt:
            max_cnt = c
        p = c / tot
        ent -= p * math.log2(p)
    return ent, max_cnt / tot


def iter_cjk_runs(corpus_path: Path):
    with corpus_path.open("r", encoding="utf-8", errors="replace") as f:
        for line in f:
            for m in RE_CJK_BMP_ONLY.finditer(line):
                run = m.group(0)
                if len(run) >= 2:
                    yield run
                elif len(run) == 1:
                    yield run


def main() -> int:
    parser = argparse.ArgumentParser(description="如喜 (RuhI) 語料驅動 OOV 詞彙統計探勘器")
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--lexicon", type=Path, default=DEFAULT_LEXICON)
    parser.add_argument("--rejected", type=Path, default=DEFAULT_REJECTED)
    parser.add_argument("--impostors", type=Path, default=DEFAULT_IMPOSTORS)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--twfilter-cache", type=Path, default=DEFAULT_TWFILTER_CACHE)
    parser.add_argument("--apriori-min", type=int, default=40)
    parser.add_argument("--min-weight", type=int, default=160)
    parser.add_argument("--max-weight", type=int, default=340)
    args = parser.parse_args()

    t0 = time.time()
    purity = CachedTaiwanPurityFilter(args.twfilter_cache)
    existing_words, rejected_words, rejected_pairs, mono_map, char_readings = load_lexicon_and_guards(
        args.lexicon, args.rejected, args.impostors
    )
    existing_strong220 = frozenset(
        w for w, rlist in existing_words.items() if len(w) >= 1 and max(wt for _, wt in rlist) >= 220
    )
    print(
        f"[lexicon] 已載入現有詞條數={len(existing_words)} | 單字音表={len(char_readings)} | 嚴格單音字={len(mono_map)}",
        flush=True,
    )

    # Pass 1: 統計 c1 與 c2
    print("[pass 1/4] 統計 unigram 與 bigram ...", flush=True)
    c1: Counter[str] = Counter()
    c2: Counter[str] = Counter()
    n_chars = 0
    for run in iter_cjk_runs(args.corpus):
        n_chars += len(run)
        c1.update(run)
        L = len(run)
        if L >= 2:
            for i in range(L - 1):
                c2[run[i : i + 2]] += 1
    freq2 = {w: c for w, c in c2.items() if c >= args.apriori_min}
    del c2
    print(f"[pass 1/4] 完成 ({time.time() - t0:.1f}s)：總字數={n_chars:,} | freq2 (>={args.apriori_min})={len(freq2):,}", flush=True)

    # Pass 2: 統計 Apriori 修剪後之 c3
    print("[pass 2/4] 統計 Apriori 3-gram ...", flush=True)
    c3: Counter[str] = Counter()
    f2_has = freq2.__contains__
    for run in iter_cjk_runs(args.corpus):
        L = len(run)
        if L < 3:
            continue
        prev2 = f2_has(run[0:2])
        for i in range(L - 2):
            cur2 = f2_has(run[i + 1 : i + 3])
            if prev2 and cur2:
                c3[run[i : i + 3]] += 1
            prev2 = cur2
    freq3 = {w: c for w, c in c3.items() if c >= args.apriori_min}
    del c3
    print(f"[pass 2/4] 完成 ({time.time() - t0:.1f}s)：freq3 (>={args.apriori_min})={len(freq3):,}", flush=True)

    # Pass 3: 統計 Apriori 修剪後之 c4
    print("[pass 3/4] 統計 Apriori 4-gram ...", flush=True)
    c4: Counter[str] = Counter()
    f3_has = freq3.__contains__
    for run in iter_cjk_runs(args.corpus):
        L = len(run)
        if L < 4:
            continue
        prev3 = f3_has(run[0:3])
        for i in range(L - 3):
            cur3 = f3_has(run[i + 1 : i + 4])
            if prev3 and cur3:
                c4[run[i : i + 4]] += 1
            prev3 = cur3
    freq4 = {w: c for w, c in c4.items() if c >= args.apriori_min}
    del c4
    print(f"[pass 3/4] 完成 ({time.time() - t0:.1f}s)：freq4 (>={args.apriori_min})={len(freq4):,}", flush=True)

    # 篩選 OOV 候選集（PMI + 台灣純度 + 句法短語防護）
    log2_n = math.log2(n_chars)

    def count_of(seg: str) -> int:
        k = len(seg)
        if k == 1:
            return c1.get(seg, 0)
        if k == 2:
            return freq2.get(seg, 0)
        if k == 3:
            return freq3.get(seg, 0)
        if k == 4:
            return freq4.get(seg, 0)
        return 0

    def min_pmi(word: str, cnt: int) -> float:
        m = len(word)
        best = 999.0
        log2_w = math.log2(cnt)
        for k in range(1, m):
            cl = count_of(word[:k])
            cr = count_of(word[k:])
            if cl <= 0 or cr <= 0:
                return -999.0
            pmi = log2_w + log2_n - math.log2(cl) - math.log2(cr)
            if pmi < best:
                best = pmi
        return best

    def is_strong_two_word_concat(word: str) -> bool:
        m = len(word)
        for k in range(1, m):
            if word[:k] in existing_strong220 and word[k:] in existing_strong220:
                return True
        return False

    min_freq_by_len = {2: 80, 3: 50, 4: 40}
    min_pmi_by_len = {2: 4.0, 3: 4.5, 4: 5.0}
    cands_info: dict[str, tuple[int, float]] = {}

    for m, fdict in ((2, freq2), (3, freq3), (4, freq4)):
        fmin = min_freq_by_len[m]
        pmin = min_pmi_by_len[m]
        for w, cnt in fdict.items():
            if cnt < fmin:
                continue
            if w in existing_words or w in rejected_words:
                continue
            if not purity.is_taiwan_clean(w):
                continue
            if is_syntactic_or_numeral_fragment(w):
                continue
            pmi = min_pmi(w, cnt)
            if pmi < pmin:
                continue
            if m in (3, 4) and is_strong_two_word_concat(w):
                if pmi < 7.5 or cnt < 250:
                    continue
            cands_info[w] = (cnt, pmi)

    # 先預篩需於 Pass 4 一併驗證邊界熵與優勢比之既有低權重候選詞
    under_cands: dict[str, tuple[int, str, int]] = {}
    for w, rlist in existing_words.items():
        if len(w) not in (2, 3, 4):
            continue
        max_wt = max(wt for _, wt in rlist)
        if max_wt < 100 or max_wt > 240:
            continue
        if "臺" in w:
            tw_var = w.replace("臺", "台")
            if tw_var in existing_words and max(wt for _, wt in existing_words[tw_var]) >= 300:
                continue
        cnt = count_of(w)
        if cnt < 400:
            continue
        if not purity.is_taiwan_clean(w) or is_syntactic_or_numeral_fragment(w):
            continue
        best_bpm = max(rlist, key=lambda kv: kv[1])[0]
        under_cands[w] = (cnt, best_bpm, max_wt)

    all_pass4_words = set(cands_info.keys()) | set(under_cands.keys())
    cand_s2 = frozenset(w for w in all_pass4_words if len(w) == 2)
    cand_s3 = frozenset(w for w in all_pass4_words if len(w) == 3)
    cand_s4 = frozenset(w for w in all_pass4_words if len(w) == 4)
    print(
        f"[filter] 進入邊界熵驗證候選數：OOV={len(cands_info):,} (len2={sum(1 for w in cands_info if len(w)==2):,}, len3={sum(1 for w in cands_info if len(w)==3):,}, len4={sum(1 for w in cands_info if len(w)==4):,}) | 低權重稽核={len(under_cands):,}",
        flush=True,
    )

    # Pass 4: 計算候選詞與低權重稽核詞之左右鄰字資訊熵與優勢比
    print("[pass 4/4] 掃描候選詞左右鄰字邊界熵與優勢比 ...", flush=True)
    left_nbrs: dict[str, Counter[str]] = defaultdict(Counter)
    right_nbrs: dict[str, Counter[str]] = defaultdict(Counter)
    s2_has = cand_s2.__contains__
    s3_has = cand_s3.__contains__
    s4_has = cand_s4.__contains__

    for run in iter_cjk_runs(args.corpus):
        L = len(run)
        if L < 2:
            continue
        for i in range(L - 1):
            lc = "^" if i == 0 else run[i - 1]
            w2 = run[i : i + 2]
            if s2_has(w2):
                left_nbrs[w2][lc] += 1
                right_nbrs[w2]["$" if i + 2 == L else run[i + 2]] += 1
            if i + 3 <= L:
                w3 = run[i : i + 3]
                if s3_has(w3):
                    left_nbrs[w3][lc] += 1
                    right_nbrs[w3]["$" if i + 3 == L else run[i + 3]] += 1
            if i + 4 <= L:
                w4 = run[i : i + 4]
                if s4_has(w4):
                    left_nbrs[w4][lc] += 1
                    right_nbrs[w4]["$" if i + 4 == L else run[i + 4]] += 1

    min_w_cfg = int(args.min_weight)
    max_w_cfg = int(args.max_weight)
    span_w = max(1.0, float(max_w_cfg - min_w_cfg))

    def map_weight(cnt: int) -> int:
        # 40..40000 (log10 span ~3.0) 平滑映射至 [min_w_cfg, max_w_cfg]
        frac = (math.log10(max(cnt, 40)) - math.log10(40.0)) / 2.6
        raw = min_w_cfg + span_w * max(0.0, min(1.0, frac))
        return max(min_w_cfg, min(max_w_cfg, int(round(raw))))

    auto_mono_rows: list[tuple[float, str, str, int, int, float, float, float]] = []
    poly_rows: list[tuple[float, str, str, str, int, int, float, float, float]] = []

    for w, (cnt, pmi) in cands_info.items():
        ent_l, dom_l = shannon_entropy_and_max_dom(left_nbrs[w])
        ent_r, dom_r = shannon_entropy_and_max_dom(right_nbrs[w])
        min_ent = min(ent_l, ent_r)
        max_dom = max(dom_l, dom_r)
        req_ent = 2.2 if len(w) == 2 else 1.8
        if min_ent < req_ent or max_dom > 0.45:
            continue
        if any(ch not in char_readings or not char_readings[ch] for ch in w):
            continue

        score_rank = cnt * pmi
        wt = map_weight(cnt)
        if all(ch in mono_map for ch in w):
            raw_bpm = " ".join(mono_map[ch] for ch in w)
            bpm = normalize_bpm(canonicalize_sandhi(w, raw_bpm))
            if (w, bpm) in rejected_pairs:
                continue
            auto_mono_rows.append((score_rank, w, bpm, wt, cnt, pmi, min_ent, max_dom))
        else:
            guessed_parts = []
            poly_notes = []
            for ch in w:
                rlist = char_readings[ch]
                guessed_parts.append(rlist[0][0])
                if ch not in mono_map:
                    opts = "/".join(f"{b}:{w0}" for b, w0 in rlist[:3])
                    poly_notes.append(f"{ch}[{opts}]")
            raw_bpm = " ".join(guessed_parts)
            bpm = normalize_bpm(canonicalize_sandhi(w, raw_bpm))
            if (w, bpm) in rejected_pairs:
                continue
            poly_rows.append((score_rank, w, bpm, ";".join(poly_notes), wt, cnt, pmi, min_ent, max_dom))

    auto_mono_rows.sort(key=lambda r: -r[0])
    poly_rows.sort(key=lambda r: -r[0])

    # 稽核現有低權重詞條 (套用同等級邊界優勢比 <= 0.45 與邊界熵檢核，排除詞根殘片)
    underweighted_rows: list[tuple[int, str, str, int, int, float, float]] = []
    for w, (cnt, best_bpm, old_wt) in under_cands.items():
        ent_l, dom_l = shannon_entropy_and_max_dom(left_nbrs[w])
        ent_r, dom_r = shannon_entropy_and_max_dom(right_nbrs[w])
        min_ent = min(ent_l, ent_r)
        max_dom = max(dom_l, dom_r)
        req_ent = 2.2 if len(w) == 2 else 1.8
        if min_ent < req_ent or max_dom > 0.45:
            continue
        suggested_wt = max(old_wt, min(320, map_weight(cnt)))
        if suggested_wt > old_wt:
            underweighted_rows.append((cnt, w, best_bpm, old_wt, suggested_wt, min_ent, max_dom))
    underweighted_rows.sort(key=lambda r: -r[0])

    args.out_dir.mkdir(parents=True, exist_ok=True)
    p_mono = args.out_dir / "auto_monophonic.tsv"
    p_poly = args.out_dir / "polyphonic_review.tsv"
    p_under = args.out_dir / "underweighted_audit.tsv"
    p_sum = args.out_dir / "summary.json"

    with p_mono.open("w", encoding="utf-8") as f:
        f.write("# word\tbopomofo\tweight\tcount\tpmi\tmin_ent\tmax_dom\n")
        for _, w, bpm, wt, cnt, pmi, min_ent, max_dom in auto_mono_rows:
            f.write(f"{w}\t{bpm}\t{wt}\t{cnt}\t{pmi:.3f}\t{min_ent:.3f}\t{max_dom:.3f}\n")

    with p_poly.open("w", encoding="utf-8") as f:
        f.write("# word\tguessed_bopomofo\tpoly_info\tweight\tcount\tpmi\tmin_ent\tmax_dom\n")
        for _, w, bpm, info, wt, cnt, pmi, min_ent, max_dom in poly_rows[:300]:
            f.write(f"{w}\t{bpm}\t{info}\t{wt}\t{cnt}\t{pmi:.3f}\t{min_ent:.3f}\t{max_dom:.3f}\n")

    with p_under.open("w", encoding="utf-8") as f:
        f.write("# word\tbopomofo\told_weight\tsuggested_weight\tcorpus_count\tmin_ent\tmax_dom\n")
        for cnt, w, bpm, old_wt, sug_wt, min_ent, max_dom in underweighted_rows:
            f.write(f"{w}\t{bpm}\t{old_wt}\t{sug_wt}\t{cnt}\t{min_ent:.3f}\t{max_dom:.3f}\n")

    summary = {
        "elapsed_secs": round(time.time() - t0, 2),
        "corpus_cjk_chars": n_chars,
        "auto_monophonic_count": len(auto_mono_rows),
        "polyphonic_candidates_total": len(poly_rows),
        "polyphonic_review_top": min(len(poly_rows), 300),
        "underweighted_audit_count": len(underweighted_rows),
    }
    p_sum.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[done] 探勘完成 ({summary['elapsed_secs']}s): {json.dumps(summary, ensure_ascii=False)}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
