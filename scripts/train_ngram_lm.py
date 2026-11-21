#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）商業授權安全之台灣正體中文統計 N-gram 訓練管線 (train_ngram_lm.py)

以純商業授權安全語料（ODC-BY / OGDL-1.0 / MIT / CC-BY-4.0 / Apache-2.0）串流建置
字元級 (1-gram / 2-gram / 3-gram) 與詞彙級 (1-gram / 2-gram) Modified Kneser-Ney (MKN)
語言模型，並輸出供 `ruhi_ngbin` 編譯為 `RLNG0001` 二進位檔之壓縮 ARPA TSV。

核心設計規範：
1. 台灣正體純度與 OCR 亂碼過濾 (`taiwan-corpora/twfilter-tables`)：
   - 僅保留台灣教育部常用字、次常用字、例外字與台灣慣用異體字表交集，
     並明確扣除簡體專用字、粵語方言字、轉換正字字表及已知立法院 PDF OCR 誤碼（`帄`、`頇`、`拕`）。
   - 依據 `mainland_hard.tsv` 與 `regional_hard.tsv` 排除中國大陸／地區性特有用語，
     並完整遵循 `mainland_exceptions.tsv` 例外長詞保護；保留 `erhua_headed`/`erhua_tailed` 合法詞彙。
   - 依 `--dev-hashes`（僅含 CJK 字串 SHA-256 雜湊）排除任何開發評測重疊句段。
2. 訓練與推論一致之斷詞對齊：
   - 直接移植 `ruhi-core/src/router/viterbi.rs` 之 `MultiCharBonusPolicy::production_s5()`
     進行動態規劃斷詞，確保 N-gram 詞邊界統計與執行期詞格一致。
3. 低記憶體整數封裝計數與精確保留集重正化回退：
   - 採用整數鍵封裝 `Counter` 及分階段釋放策略，兼顧單機或 Colab 記憶體上限。
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import functools
import gc
import gzip
import hashlib
import heapq
import json
import math
import os
from pathlib import Path
import re
import sys
import time
import urllib.request
from urllib.parse import urlparse

BOS_WORD = "<s>"
UNK_WORD = "<unk>"
BOS_CHAR_ID = 0

LOG10_MIN = -11.5
LOG10_MAX = 1.5

TWFILTER_BASE_URL = "https://huggingface.co/datasets/taiwan-corpora/twfilter-tables/raw/main"

ATTRIBUTION_NOTICES = [
    "數位發展部，CNS11643中文標準交換碼全字庫網站",
    "OpenCC — https://github.com/BYVoid/OpenCC, Apache License 2.0",
    "taiwan-corpora/twfilter-tables (OGDL-Taiwan-1.0 / Apache-2.0 / MIT)",
]

FINEWEB_BLOCKED_SUBSTRINGS = (
    "typing.tw",
    "anandamarga",
    "sarkar",
    "prout",
    "wikipedia",
    "wiktionary",
    "wikisource",
    "wikimedia",
    "ptt.cc",
)

OCR_GLITCH_BLOCK_CHARS = frozenset({"帄", "頇", "拕"})

# 與 ruhi-core/src/router/viterbi.rs::is_narrow_transparent_derived 嚴格對齊
LEXICALIZED_3 = frozenset({
    "不見得", "怪不得", "恨不得", "由不得", "捨不得", "了不得",
    "免不了", "少不了", "受不了", "算不了", "大不了", "省不了",
    "逃不了", "免不得", "來得及", "不得不", "不得已", "不得了",
    "對不起", "了不起",
})
NOT_DERIVED_DE = frozenset({"目的", "標的", "別的", "有的", "是的", "新的"})
DERIVED_PRONOUN_PREFIX = frozenset("我你妳他她它牠祂")
DERIVED_PRT_SUFFIX2 = frozenset("嗎呢吧啊呀嘛啦")
DERIVED_FIXED2 = frozenset({"沒辦", "沒關", "沒想"})
DERIVED_SUFFIX3 = frozenset("的得了著過")
DERIVED_MID3 = frozenset("得了")

RE_SENT_SPLIT = re.compile(r"[。！？!?；;\r\n]+")
RE_CJK_RUN = re.compile(r"[\u3007\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\U00020000-\U0002ebef]+")
RE_CJK_ONLY = re.compile(r"[^\u3007\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\U00020000-\U0002ebef]+")


def open_maybe_gz_text(path: str | Path, mode: str = "rt"):
    p = str(path)
    if p.endswith(".gz"):
        return gzip.open(p, mode, encoding="utf-8")
    return open(p, mode, encoding="utf-8", errors="replace")


def download_text_url(url: str, retries: int = 3) -> str:
    last_err: Exception | None = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "RuhI-NgramTrainer/1.0"})
            with urllib.request.urlopen(req, timeout=30) as resp:
                return resp.read().decode("utf-8", errors="replace")
        except Exception as exc:
            last_err = exc
            time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"下載 {url} 失敗: {last_err}")


def parse_single_chars(text: str) -> set[str]:
    out: set[str] = set()
    for line in text.splitlines():
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        ch = s.split("\t", 1)[0].strip()
        if len(ch) == 1:
            out.add(ch)
    return out


class TaiwanPurityFilter:
    """依據 twfilter-tables 與 dev 雜湊排除非台灣正體及評測重疊句段。"""

    def __init__(self, dev_hashes: set[str]):
        print("[purity] 下載 taiwan-corpora/twfilter-tables 字表...", flush=True)
        moe_common = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/moe_common.txt"))
        moe_secondary = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/moe_secondary.txt"))
        moe_exception = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/moe_exception.txt"))
        variants_tw = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/variants_used_in_taiwan.txt"))

        simplified_only = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/simplified_only.txt"))
        cantonese = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/cantonese.txt"))
        converted_orth = parse_single_chars(download_text_url(f"{TWFILTER_BASE_URL}/converted_orthography.txt"))

        self.explicit_block_chars = frozenset(
            simplified_only | cantonese | converted_orth | OCR_GLITCH_BLOCK_CHARS
        )
        base_allow = moe_common | moe_secondary | moe_exception | variants_tw
        self.allowed_cjk_chars = frozenset(base_allow - self.explicit_block_chars)

        hard_terms: set[str] = set()
        for fname in ("mainland_hard.tsv", "regional_hard.tsv"):
            raw = download_text_url(f"{TWFILTER_BASE_URL}/{fname}")
            for line in raw.splitlines():
                s = line.strip()
                if not s or s.startswith("#"):
                    continue
                term = s.split("\t", 1)[0].strip()
                if len(term) >= 2:
                    hard_terms.add(term)

        exceptions_map: dict[str, list[tuple[str, int]]] = defaultdict(list)
        exc_raw = download_text_url(f"{TWFILTER_BASE_URL}/mainland_exceptions.tsv")
        for line in exc_raw.splitlines():
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
        self.hard_by_first: dict[str, tuple[str, ...]] = {
            k: tuple(sorted(v, key=len, reverse=True)) for k, v in by_first.items()
        }
        self.hard_first_chars = frozenset(self.hard_by_first.keys())
        self.exceptions_map: dict[str, tuple[tuple[str, int], ...]] = {
            k: tuple(v) for k, v in exceptions_map.items()
        }
        self.dev_hashes = dev_hashes
        self.clause_freq: dict[int, int] = {}

        self.stats: dict[str, int] = {
            "allowlist_chars": len(self.allowed_cjk_chars),
            "blocklist_chars": len(self.explicit_block_chars),
            "hard_terms_count": len(hard_terms),
            "dev_hashes_loaded": len(dev_hashes),
            "clauses_scanned": 0,
            "clauses_kept": 0,
            "clean_chars_kept": 0,
            "rejected_length": 0,
            "rejected_non_moe_or_blockchar": 0,
            "rejected_mainland_or_regional": 0,
            "rejected_dev_overlap": 0,
            "rejected_boilerplate_cap": 0,
        }
        print(
            f"[purity] 載入完成：允許字數={len(self.allowed_cjk_chars)} | "
            f"阻擋字數={len(self.explicit_block_chars)} | 詞彙黑名單={len(hard_terms)}",
            flush=True,
        )

    def _matches_unexempt_hard_term(self, clause: str) -> bool:
        if self.hard_first_chars.isdisjoint(clause):
            return False
        hard_by_first = self.hard_by_first
        ex_map = self.exceptions_map
        for i, ch in enumerate(clause):
            terms = hard_by_first.get(ch)
            if not terms:
                continue
            for term in terms:
                if clause.startswith(term, i):
                    excs = ex_map.get(term)
                    exempt = False
                    if excs:
                        for exc_str, off in excs:
                            pos = i - off
                            if pos >= 0 and clause.startswith(exc_str, pos):
                                exempt = True
                                break
                    if not exempt:
                        return True
        return False

    def extract_clean_clauses(self, text: str) -> list[str]:
        if not text:
            return []
        out: list[str] = []
        allowed = self.allowed_cjk_chars
        dev_hashes = self.dev_hashes
        clause_freq = self.clause_freq

        for sent in RE_SENT_SPLIT.split(text):
            if not sent:
                continue
            if dev_hashes:
                sent_cjk = RE_CJK_ONLY.sub("", sent)
                if len(sent_cjk) >= 2:
                    sh = hashlib.sha256(sent_cjk.encode("utf-8")).hexdigest()
                    if sh in dev_hashes:
                        self.stats["rejected_dev_overlap"] += 1
                        continue

            for m in RE_CJK_RUN.finditer(sent):
                clause = m.group(0)
                self.stats["clauses_scanned"] += 1
                clen = len(clause)
                if clen < 2 or clen > 60:
                    self.stats["rejected_length"] += 1
                    continue

                if not all(ch in allowed for ch in clause):
                    self.stats["rejected_non_moe_or_blockchar"] += 1
                    continue

                if self._matches_unexempt_hard_term(clause):
                    self.stats["rejected_mainland_or_regional"] += 1
                    continue

                if dev_hashes:
                    chash = hashlib.sha256(clause.encode("utf-8")).hexdigest()
                    if chash in dev_hashes:
                        self.stats["rejected_dev_overlap"] += 1
                        continue

                dkey = int.from_bytes(
                    hashlib.blake2b(clause.encode("utf-8"), digest_size=8).digest(), "little"
                )
                prev_cnt = clause_freq.get(dkey, 0)
                if prev_cnt >= 20:
                    self.stats["rejected_boilerplate_cap"] += 1
                    continue
                clause_freq[dkey] = prev_cnt + 1
                if len(clause_freq) > 3_000_000:
                    # 僅淘汰單次出現之偶發鍵，保留高頻模板以維持去重上限效用
                    clause_freq = {k: v for k, v in clause_freq.items() if v > 1}
                    self.clause_freq = clause_freq

                self.stats["clauses_kept"] += 1
                self.stats["clean_chars_kept"] += clen
                out.append(clause)
        return out


# ── S5 動態規劃斷詞器（嚴格對齊 ruhi-core/src/router/viterbi.rs） ──────────────────

def is_narrow_transparent_derived(word: str) -> bool:
    n = len(word)
    if n == 2:
        c0, c1 = word[0], word[1]
        if c0 in DERIVED_PRONOUN_PREFIX and c1 != "們":
            return True
        if c1 == "的" and word not in NOT_DERIVED_DE:
            return True
        if c1 in DERIVED_PRT_SUFFIX2:
            return True
        return word in DERIVED_FIXED2
    if n == 3:
        if word in LEXICALIZED_3:
            return False
        return (word[2] in DERIVED_SUFFIX3) or (word[1] in DERIVED_MID3)
    return False


def effective_multi_char_bonus_s5(word: str, word_len: int, weight: int) -> float:
    b = 28.0
    if word_len == 2:
        if weight <= 100:
            b = 20.0
        elif weight >= 250:
            b = 28.0
        else:
            b = 20.0 + ((weight - 100) / 150.0) * 8.0
    if is_narrow_transparent_derived(word):
        capped = (28.0 - 8.0 / (word_len - 1)) if word_len >= 3 else 20.0
        if capped < b:
            b = capped
    return b


class LexiconSegmenter:
    """基於 lexicon_core.tsv 與 S5 權重策略之快速動態規劃斷詞器。"""

    def __init__(self, lexicon_path: str | Path):
        self.word_weights: dict[str, int] = {}
        with open_maybe_gz_text(lexicon_path, "rt") as f:
            for line in f:
                if not line or line.startswith("#"):
                    continue
                parts = line.rstrip("\r\n").split("\t")
                if len(parts) < 3:
                    continue
                w = parts[0].strip()
                if not w:
                    continue
                try:
                    wt = int(parts[2])
                except ValueError:
                    continue
                if wt > self.word_weights.get(w, 0):
                    self.word_weights[w] = wt

        # ID 0 = <s>, ID 1 = <unk>, ID 2.. = lexicon_core 詞彙
        self.id_to_word: list[str] = [BOS_WORD, UNK_WORD]
        self.word_to_id: dict[str, int] = {BOS_WORD: 0, UNK_WORD: 1}
        self.id_weight: list[int] = [0, 0]
        self.seg_scores: dict[str, float] = {}

        for w, wt in self.word_weights.items():
            if w in (BOS_WORD, UNK_WORD):
                continue
            wid = len(self.id_to_word)
            self.id_to_word.append(w)
            self.word_to_id[w] = wid
            self.id_weight.append(wt)

            wlen = len(w)
            if wlen == 1:
                self.seg_scores[w] = math.log(max(1, wt))
            elif wlen == 2 and wt >= 90 and not is_narrow_transparent_derived(w):
                self.seg_scores[w] = math.log(max(1, wt)) + effective_multi_char_bonus_s5(w, 2, wt)
            elif 3 <= wlen <= 4 and (
                (wt >= 250 and not any(c in "的得地" for c in w)) or (w in LEXICALIZED_3)
            ):
                coh = 16.0 if wlen == 4 else 8.0
                self.seg_scores[w] = (
                    math.log(max(1, wt))
                    + (wlen - 1) * effective_multi_char_bonus_s5(w, wlen, wt)
                    + coh
                )

        if len(self.id_to_word) >= (1 << 20):
            raise RuntimeError(f"詞彙數量 {len(self.id_to_word)} 超出 20-bit 封裝上限")

    def segment_words(self, clause: str) -> list[str]:
        n = len(clause)
        dp = [-1e18] * (n + 1)
        dp[0] = 0.0
        back = [1] * (n + 1)
        scores = self.seg_scores

        for i in range(n):
            base = dp[i]
            if base < -1e17:
                continue
            max_l = 4 if (n - i) >= 4 else (n - i)
            for L in range(1, max_l + 1):
                sub = clause[i : i + L]
                sc = scores.get(sub)
                if sc is None:
                    if L == 1:
                        sc = -10.0
                    else:
                        continue
                cand = base + sc
                if cand > dp[i + L]:
                    dp[i + L] = cand
                    back[i + L] = L

        out: list[str] = []
        pos = n
        while pos > 0:
            L = back[pos]
            out.append(clause[pos - L : pos])
            pos -= L
        out.reverse()
        return out

    @functools.lru_cache(maxsize=200_000)
    def segment_to_ids(self, clause: str) -> tuple[int, ...]:
        w2i = self.word_to_id
        return tuple(w2i.get(w, 1) for w in self.segment_words(clause))


# ── N-gram 統計累加器 ──────────────────────────────────────────────────────────

class NgramAccumulator:
    """使用整數鍵封裝統計詞彙與字元 N-gram 頻率。"""

    def __init__(self, segmenter: LexiconSegmenter):
        self.segmenter = segmenter
        self.word_uni: Counter[int] = Counter()
        self.word_bi: Counter[int] = Counter()

        self.char_to_id: dict[str, int] = {}
        self.id_to_char: list[str] = ["<s>"]  # index 0 = BOS_CHAR
        self.char_uni: Counter[int] = Counter()
        self.char_bi: Counter[int] = Counter()
        self.char_tri: Counter[int] = Counter()

    def _get_char_id(self, ch: str) -> int:
        cid = self.char_to_id.get(ch)
        if cid is None:
            cid = len(self.id_to_char)
            self.char_to_id[ch] = cid
            self.id_to_char.append(ch)
        return cid

    def ingest_clauses(self, clauses: list[str]) -> None:
        seg_fn = self.segmenter.segment_to_ids
        w_uni = self.word_uni
        w_bi = self.word_bi
        c_uni = self.char_uni
        c_bi = self.char_bi
        c_tri = self.char_tri

        for clause in clauses:
            # 1. 詞彙級 1-gram 與 2-gram
            wids = seg_fn(clause)
            w_uni[0] += 1  # <s>
            prev_wid = 0
            for wid in wids:
                w_uni[wid] += 1
                w_bi[(prev_wid << 20) | wid] += 1
                prev_wid = wid

            # 2. 字元級 1-gram / 2-gram / 3-gram
            c_uni[0] += 1
            p2 = 0
            p1 = 0
            for ch in clause:
                cid = self._get_char_id(ch)
                c_uni[cid] += 1
                c_bi[(p1 << 16) | cid] += 1
                c_tri[(p2 << 32) | (p1 << 16) | cid] += 1
                p2 = p1
                p1 = cid


# ── 語料萃取工具 ──────────────────────────────────────────────────────────────

def extract_strings_recursive(obj, out: list[str], depth: int = 0) -> None:
    if depth > 6 or obj is None:
        return
    if isinstance(obj, str):
        if len(obj) >= 2:
            out.append(obj)
    elif isinstance(obj, list):
        for item in obj:
            extract_strings_recursive(item, out, depth + 1)
    elif isinstance(obj, dict):
        for val in obj.values():
            extract_strings_recursive(val, out, depth + 1)


def is_valid_fineweb_tw_url(url: str) -> bool:
    if not url:
        return False
    low = url.lower()
    if any(b in low for b in FINEWEB_BLOCKED_SUBSTRINGS):
        return False
    try:
        host = (urlparse(url).hostname or "").lower()
    except Exception:
        return False
    return host.endswith(".tw")


class CorpusIngestor:
    def __init__(
        self,
        purity: TaiwanPurityFilter,
        acc: NgramAccumulator,
        target_chars: int,
        fineweb_timeout_sec: int,
    ):
        self.purity = purity
        self.acc = acc
        self.target_chars = target_chars
        self.fineweb_timeout_sec = fineweb_timeout_sec
        self.source_stats: dict[str, dict] = {}
        self.total_clean_chars = 0

    def _feed_text(self, source_key: str, text: str, cap: int) -> int:
        if not text:
            return 0
        rem_source = cap - self.source_stats[source_key]["clean_chars"]
        rem_total = self.target_chars - self.total_clean_chars
        if rem_source <= 0 or rem_total <= 0:
            return 0
        clauses = self.purity.extract_clean_clauses(text)
        if not clauses:
            return 0
        accepted: list[str] = []
        added_chars = 0
        limit = min(rem_source, rem_total)
        for cl in clauses:
            if added_chars + len(cl) > limit and accepted:
                break
            accepted.append(cl)
            added_chars += len(cl)
            if added_chars >= limit:
                break
        if accepted:
            self.acc.ingest_clauses(accepted)
            self.source_stats[source_key]["clean_chars"] += added_chars
            self.source_stats[source_key]["clauses"] += len(accepted)
            self.total_clean_chars += added_chars
        return added_chars

    def _init_source(self, key: str, repo: str, license_name: str, cap: int) -> dict:
        entry = {
            "repo": repo,
            "license": license_name,
            "cap_chars": cap,
            "docs_scanned": 0,
            "clauses": 0,
            "clean_chars": 0,
            "status": "pending",
            "error": None,
        }
        self.source_stats[key] = entry
        return entry

    def run_fineweb_tw(self, cap: int = 40_000_000) -> None:
        st = self._init_source("fineweb_tw", "HuggingFaceFW/fineweb-2 (cmn_Hani, .tw only)", "ODC-BY-1.0", cap)
        t0 = time.monotonic()
        tw_docs = 0
        try:
            from huggingface_hub import HfFileSystem
            import pyarrow.parquet as pq

            hffs = HfFileSystem()
            for shard_idx in range(6):
                if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                    break
                if (time.monotonic() - t0) >= self.fineweb_timeout_sec:
                    st["status"] = "timeout_partial"
                    break
                shard_path = f"datasets/HuggingFaceFW/fineweb-2/data/cmn_Hani/train/000_{shard_idx:05d}.parquet"
                print(f"[fineweb] 串流分片 {shard_path}...", flush=True)
                with hffs.open(shard_path, "rb") as f:
                    pf = pq.ParquetFile(f)
                    for batch in pf.iter_batches(batch_size=2048, columns=["url", "text"]):
                        if (time.monotonic() - t0) >= self.fineweb_timeout_sec:
                            st["status"] = "timeout_partial"
                            break
                        urls = batch.column("url").to_pylist()
                        texts = batch.column("text").to_pylist()
                        for u, txt in zip(urls, texts):
                            st["docs_scanned"] += 1
                            if is_valid_fineweb_tw_url(u or ""):
                                tw_docs += 1
                                self._feed_text("fineweb_tw", txt or "", cap)
                            if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                                break
                        if st["docs_scanned"] % 50_000 < 2048:
                            print(
                                f"  [fineweb] scanned={st['docs_scanned']:,} tw_docs={tw_docs:,} "
                                f"clean_chars={st['clean_chars']:,} ({time.monotonic()-t0:.0f}s)",
                                flush=True,
                            )
                        if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                            break
            if st["status"] == "pending":
                st["status"] = "ok"
        except Exception as exc:
            st["error"] = str(exc)
            print(f"[fineweb] HfFileSystem 串流異常 ({exc})，嘗試 datasets fallback...", flush=True)
            if st["docs_scanned"] == 0:
                try:
                    from datasets import load_dataset

                    ds = load_dataset(
                        "HuggingFaceFW/fineweb-2",
                        name="cmn_Hani",
                        split="train",
                        streaming=True,
                    )
                    for row in ds:
                        if (time.monotonic() - t0) >= self.fineweb_timeout_sec:
                            st["status"] = "timeout_partial"
                            break
                        st["docs_scanned"] += 1
                        u = row.get("url", "") or ""
                        if is_valid_fineweb_tw_url(u):
                            tw_docs += 1
                            self._feed_text("fineweb_tw", row.get("text", "") or "", cap)
                        if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                            st["status"] = "ok"
                            break
                    if st["status"] == "pending":
                        st["status"] = "ok"
                except Exception as exc2:
                    st["status"] = "error"
                    st["error"] = f"{exc} | fallback: {exc2}"
            else:
                st["status"] = "partial_after_error"
        st["tw_docs_matched"] = tw_docs
        st["elapsed_sec"] = round(time.monotonic() - t0, 1)
        print(
            f"[source] fineweb_tw: status={st['status']} clean_chars={st['clean_chars']:,} "
            f"tw_docs={tw_docs:,} ({st['elapsed_sec']}s)",
            flush=True,
        )

    def run_ivod(self, cap: int = 14_000_000) -> None:
        st = self._init_source("ivod_fine_tune", "openfun/ivod-fine-tune", "CC-BY-4.0", cap)
        try:
            from huggingface_hub import HfApi, hf_hub_download

            api = HfApi()
            files = sorted(
                f for f in api.list_repo_files("openfun/ivod-fine-tune", repo_type="dataset")
                if f.startswith("data/") and f.endswith("-train.json")
            )
            for idx_f, fname in enumerate(files, 1):
                if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                    break
                local_path = None
                try:
                    local_path = hf_hub_download(
                        repo_id="openfun/ivod-fine-tune",
                        filename=fname,
                        repo_type="dataset",
                    )
                    with open(local_path, "r", encoding="utf-8", errors="replace") as f:
                        data = json.load(f)
                    if isinstance(data, list):
                        for item in data:
                            st["docs_scanned"] += 1
                            if isinstance(item, dict):
                                self._feed_text("ivod_fine_tune", item.get("text", "") or "", cap)
                            elif isinstance(item, str):
                                self._feed_text("ivod_fine_tune", item, cap)
                            if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                                break
                except Exception:
                    continue
                finally:
                    if local_path and os.path.exists(local_path):
                        try:
                            os.remove(local_path)
                        except OSError:
                            pass
                if idx_f % 50 == 0:
                    print(
                        f"  [ivod] files={idx_f}/{len(files)} clean_chars={st['clean_chars']:,}",
                        flush=True,
                    )
            st["status"] = "ok"
        except Exception as exc:
            st["status"] = "error"
            st["error"] = str(exc)
        print(f"[source] ivod_fine_tune: status={st['status']} clean_chars={st['clean_chars']:,}", flush=True)

    def run_ly_interpellation(self, cap: int = 4_000_000) -> None:
        st = self._init_source("tw_ly_interpellation", "openfun/tw-ly-interpellation", "CC-BY-4.0", cap)
        try:
            from huggingface_hub import hf_hub_download

            for term in ("interpellation-11.jsonl", "interpellation-10.jsonl", "interpellation-09.jsonl", "interpellation-08.jsonl"):
                if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                    break
                local_path = None
                try:
                    local_path = hf_hub_download(
                        repo_id="openfun/tw-ly-interpellation",
                        filename=term,
                        repo_type="dataset",
                    )
                    with open(local_path, "r", encoding="utf-8", errors="replace") as f:
                        for line in f:
                            line = line.strip()
                            if not line:
                                continue
                            try:
                                row = json.loads(line)
                            except Exception:
                                continue
                            st["docs_scanned"] += 1
                            for field in ("事由", "說明", "text", "content"):
                                val = row.get(field)
                                if isinstance(val, str) and val:
                                    self._feed_text("tw_ly_interpellation", val, cap)
                            if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                                break
                except Exception:
                    continue
                finally:
                    if local_path and os.path.exists(local_path):
                        try:
                            os.remove(local_path)
                        except OSError:
                            pass
            st["status"] = "ok"
        except Exception as exc:
            st["status"] = "error"
            st["error"] = str(exc)

    def run_ly_bill(self, cap: int = 2_000_000) -> None:
        st = self._init_source("tw_ly_bill", "openfun/tw-ly-bill", "CC-BY-4.0", cap)
        try:
            from huggingface_hub import HfApi, hf_hub_download

            api = HfApi()
            files = sorted(
                (f for f in api.list_repo_files("openfun/tw-ly-bill", repo_type="dataset") if f.endswith(".jsonl")),
                reverse=True,
            )
            for fname in files:
                if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                    break
                local_path = None
                try:
                    local_path = hf_hub_download(
                        repo_id="openfun/tw-ly-bill",
                        filename=fname,
                        repo_type="dataset",
                    )
                    with open(local_path, "r", encoding="utf-8", errors="replace") as f:
                        for line in f:
                            line = line.strip()
                            if not line:
                                continue
                            try:
                                row = json.loads(line)
                            except Exception:
                                continue
                            st["docs_scanned"] += 1
                            buf: list[str] = []
                            if isinstance(row, dict):
                                for k in ("案由", "說明", "法條", "主旨", "text", "content"):
                                    if isinstance(row.get(k), str):
                                        buf.append(row[k])
                                if not buf:
                                    extract_strings_recursive(row, buf)
                            for s in buf:
                                if len(s) >= 8:
                                    self._feed_text("tw_ly_bill", s, cap)
                            if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                                break
                except Exception:
                    continue
                finally:
                    if local_path and os.path.exists(local_path):
                        try:
                            os.remove(local_path)
                        except OSError:
                            pass
            st["status"] = "ok"
        except Exception as exc:
            st["status"] = "error"
            st["error"] = str(exc)

    def run_parquet_source(
        self,
        key: str,
        repo: str,
        filename: str,
        license_name: str,
        cap: int,
        preferred_cols: tuple[str, ...] | None = None,
    ) -> None:
        st = self._init_source(key, repo, license_name, cap)
        local_path = None
        try:
            from huggingface_hub import hf_hub_download
            import pyarrow.parquet as pq

            local_path = hf_hub_download(repo_id=repo, filename=filename, repo_type="dataset")
            pf = pq.ParquetFile(local_path)
            avail = pf.schema_arrow.names
            cols = [c for c in (preferred_cols or avail) if c in avail]
            if not cols:
                cols = avail
            for batch in pf.iter_batches(batch_size=1024, columns=cols):
                d = batch.to_pydict()
                n_rows = len(next(iter(d.values()))) if d else 0
                for idx in range(n_rows):
                    st["docs_scanned"] += 1
                    buf: list[str] = []
                    for col in cols:
                        extract_strings_recursive(d[col][idx], buf)
                    for s in buf:
                        self._feed_text(key, s, cap)
                    if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                        break
                if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                    break
            st["status"] = "ok"
        except Exception as exc:
            st["status"] = "error"
            st["error"] = str(exc)
        finally:
            if local_path and os.path.exists(local_path):
                try:
                    os.remove(local_path)
                except OSError:
                    pass
        print(f"[source] {key}: status={st['status']} clean_chars={st['clean_chars']:,}", flush=True)

    def run_jsonl_source(
        self,
        key: str,
        repo: str,
        filename: str,
        license_name: str,
        cap: int,
        preferred_cols: tuple[str, ...] = ("text", "content"),
    ) -> None:
        st = self._init_source(key, repo, license_name, cap)
        local_path = None
        try:
            from huggingface_hub import hf_hub_download

            local_path = hf_hub_download(repo_id=repo, filename=filename, repo_type="dataset")
            with open(local_path, "r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        row = json.loads(line)
                    except Exception:
                        continue
                    st["docs_scanned"] += 1
                    buf: list[str] = []
                    if isinstance(row, dict):
                        for col in preferred_cols:
                            val = row.get(col)
                            if isinstance(val, str):
                                buf.append(val)
                        if not buf:
                            extract_strings_recursive(row, buf)
                    for s in buf:
                        self._feed_text(key, s, cap)
                    if st["clean_chars"] >= cap or self.total_clean_chars >= self.target_chars:
                        break
            st["status"] = "ok"
        except Exception as exc:
            st["status"] = "error"
            st["error"] = str(exc)
        finally:
            if local_path and os.path.exists(local_path):
                try:
                    os.remove(local_path)
                except OSError:
                    pass
        print(f"[source] {key}: status={st['status']} clean_chars={st['clean_chars']:,}", flush=True)


# ── Modified Kneser-Ney (MKN) 平滑與精確重正化輸出 ────────────────────────────────

def compute_mkn_discounts(counts_iter) -> tuple[float, float, float]:
    n1 = n2 = n3 = n4 = 0
    for c in counts_iter:
        if c == 1:
            n1 += 1
        elif c == 2:
            n2 += 1
        elif c == 3:
            n3 += 1
        elif c == 4:
            n4 += 1
    if n1 <= 0 or n2 <= 0 or (n1 + 2 * n2) <= 0:
        return 0.75, 1.0, 1.25
    y = n1 / (n1 + 2.0 * n2)
    d1 = 1.0 - 2.0 * y * (n2 / n1)
    d2 = (2.0 - 3.0 * y * (n3 / n2)) if (n2 > 0 and n3 > 0) else 1.0
    d3 = (3.0 - 4.0 * y * (n4 / n3)) if (n3 > 0 and n4 > 0) else 1.25

    if not (0.05 < d1 < 0.95):
        d1 = 0.75
    if not (0.05 < d2 < 1.95):
        d2 = 1.0
    if not (0.05 < d3 < 2.95):
        d3 = 1.25
    return d1, d2, d3


def clamp_log10(prob_or_ratio: float) -> float:
    if prob_or_ratio <= 0.0:
        return LOG10_MIN
    val = math.log10(prob_or_ratio)
    if val < LOG10_MIN:
        return LOG10_MIN
    if val > LOG10_MAX:
        return LOG10_MAX
    return val


def emit_word_level_and_free(
    out_file,
    segmenter: LexiconSegmenter,
    acc: NgramAccumulator,
    max_word_bi: int,
) -> dict:
    """計算詞彙級 MKN (WU + WB)、輸出 TSV 並立即釋放詞彙雙元組字典。"""
    t0 = time.monotonic()
    d1, d2, d3 = compute_mkn_discounts(acc.word_bi.values())
    vocab_size = len(segmenter.id_to_word)

    ctx_total = [0] * vocab_size
    ctx_n1 = [0] * vocab_size
    ctx_n2 = [0] * vocab_size
    ctx_n3p = [0] * vocab_size
    cont_in = [0] * vocab_size

    candidates: list[tuple[int, int]] = []
    for key, cnt in acc.word_bi.items():
        p_id = key >> 20
        c_id = key & 0xFFFFF
        ctx_total[p_id] += cnt
        if cnt == 1:
            ctx_n1[p_id] += 1
        elif cnt == 2:
            ctx_n2[p_id] += 1
        else:
            ctx_n3p[p_id] += 1
        if c_id != 0:
            cont_in[c_id] += 1
        if cnt >= 3 and c_id != 0:
            candidates.append((cnt, key))

    kept_words: set[int] = {0, 1}
    word_uni = acc.word_uni
    for wid in range(2, vocab_size):
        if word_uni.get(wid, 0) >= 2:
            kept_words.add(wid)
            continue
        w = segmenter.id_to_word[wid]
        if len(w) == 1 or segmenter.id_weight[wid] >= 150:
            kept_words.add(wid)

    # 正規化詞彙一元機率（含 lexicon floor 與 <unk>）
    uni_mass: dict[int, float] = {}
    total_mass = 0.0
    for wid in kept_words:
        if wid == 0:
            continue
        if wid == 1:
            m = max(0.002, float(cont_in[1]))
        elif cont_in[wid] > 0:
            m = float(cont_in[wid])
        else:
            m = 0.005 * max(1, segmenter.id_weight[wid]) / 150.0
        uni_mass[wid] = m
        total_mass += m

    p_uni = {wid: m / total_mass for wid, m in uni_mass.items()}

    # 挑選前 max_word_bi 個高頻雙元組
    top_wb = heapq.nlargest(
        max_word_bi,
        (
            (cnt, -((k >> 20)), -(k & 0xFFFFF), k)
            for cnt, k in candidates
            if (k >> 20) in kept_words and (k & 0xFFFFF) in kept_words
        ),
    )
    del candidates
    acc.word_bi.clear()
    acc.word_uni.clear()
    gc.collect()

    grouped_wb: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for cnt, _np, _nc, key in top_wb:
        grouped_wb[key >> 20].append((key & 0xFFFFF, cnt))
    del top_wb

    bow_word: dict[int, float] = {}
    wb_lines_count = 0
    wb_buffer: list[str] = []
    id_to_word = segmenter.id_to_word

    for p_id, succs in grouped_wb.items():
        tot = ctx_total[p_id]
        if tot <= 0:
            bow_word[p_id] = 0.0
            continue
        gamma = (d1 * ctx_n1[p_id] + d2 * ctx_n2[p_id] + d3 * ctx_n3p[p_id]) / tot
        raw_probs: list[tuple[int, float]] = []
        sum_p = 0.0
        sum_lo = 0.0
        for c_id, cnt in succs:
            d = d1 if cnt == 1 else (d2 if cnt == 2 else d3)
            p_kn = (cnt - d) / tot + gamma * p_uni[c_id]
            p_kn = max(1e-11, min(0.999999, p_kn))
            raw_probs.append((c_id, p_kn))
            sum_p += p_kn
            sum_lo += p_uni[c_id]

        if sum_p >= 0.999999:
            scale = 0.99999 / sum_p
            raw_probs = [(c_id, p * scale) for c_id, p in raw_probs]
            sum_p = 0.99999

        rem_hi = max(1e-12, 1.0 - sum_p)
        rem_lo = max(1e-12, 1.0 - sum_lo)
        bow_word[p_id] = clamp_log10(rem_hi / rem_lo)

        prev_str = id_to_word[p_id]
        for c_id, p_kn in raw_probs:
            wb_buffer.append(f"WB\t{prev_str}\t{id_to_word[c_id]}\t{clamp_log10(p_kn):.6f}\n")
            wb_lines_count += 1

    # 先寫 WU 再寫 WB
    wu_count = 0
    out_file.write(f"WU\t{BOS_WORD}\t{LOG10_MIN:.6f}\t{bow_word.get(0, 0.0):.6f}\n")
    wu_count += 1
    for wid in sorted(kept_words):
        if wid == 0:
            continue
        wstr = id_to_word[wid]
        lp = clamp_log10(p_uni[wid])
        lb = bow_word.get(wid, 0.0)
        out_file.write(f"WU\t{wstr}\t{lp:.6f}\t{lb:.6f}\n")
        wu_count += 1

    out_file.writelines(wb_buffer)
    del wb_buffer, grouped_wb, bow_word, ctx_total, ctx_n1, ctx_n2, ctx_n3p, cont_in
    gc.collect()

    return {
        "word_discounts": [round(d1, 4), round(d2, 4), round(d3, 4)],
        "word_vocab_emitted": wu_count,
        "word_bigrams_emitted": wb_lines_count,
        "elapsed_sec": round(time.monotonic() - t0, 1),
    }


def emit_char_level(
    out_file,
    acc: NgramAccumulator,
    max_char_vocab: int,
    max_char_bi: int,
    max_char_tri: int,
) -> dict:
    """計算字元級 MKN (CU + CB + CT) 並輸出 TSV。"""
    t0 = time.monotonic()
    id_to_char = acc.id_to_char
    num_chars = len(id_to_char)

    # 1. 保留 char_vocab (<s> + <= max_char_vocab 個高頻字 + <unk>)
    top_chars = heapq.nlargest(
        max_char_vocab,
        ((cnt, -cid, cid) for cid, cnt in acc.char_uni.items() if cid != 0 and cnt >= 2),
    )
    kept_char_ids: set[int] = {0}
    for _cnt, _ncid, cid in top_chars:
        kept_char_ids.add(cid)

    # 2. 字元雙元組折扣與 context 統計
    d2_1, d2_2, d2_3 = compute_mkn_discounts(acc.char_bi.values())
    cb_total = [0] * num_chars
    cb_n1 = [0] * num_chars
    cb_n2 = [0] * num_chars
    cb_n3p = [0] * num_chars
    char_cont_in = [0] * num_chars
    cb_cands: list[tuple[int, int]] = []

    for key, cnt in acc.char_bi.items():
        c1 = key >> 16
        c2 = key & 0xFFFF
        cb_total[c1] += cnt
        if cnt == 1:
            cb_n1[c1] += 1
        elif cnt == 2:
            cb_n2[c1] += 1
        else:
            cb_n3p[c1] += 1
        if c2 != 0:
            char_cont_in[c2] += 1
        if cnt >= 3 and c2 != 0 and (c1 in kept_char_ids) and (c2 in kept_char_ids):
            cb_cands.append((cnt, key))

    c_uni_mass: dict[int, float] = {}
    tot_c_mass = 0.0
    for cid in kept_char_ids:
        if cid == 0:
            continue
        m = float(char_cont_in[cid]) if char_cont_in[cid] > 0 else 0.05
        c_uni_mass[cid] = m
        tot_c_mass += m
    unk_char_mass = 0.005
    tot_c_mass += unk_char_mass
    p_char_uni = {cid: m / tot_c_mass for cid, m in c_uni_mass.items()}
    p_char_unk = unk_char_mass / tot_c_mass

    top_cb = heapq.nlargest(max_char_bi, ((cnt, -k, k) for cnt, k in cb_cands))
    del cb_cands
    acc.char_bi.clear()
    acc.char_uni.clear()
    gc.collect()

    retained_cb_set: set[int] = set()
    grouped_cb: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for cnt, _nk, key in top_cb:
        retained_cb_set.add(key)
        grouped_cb[key >> 16].append((key & 0xFFFF, cnt))
    del top_cb

    cb_prob_map: dict[int, float] = {}
    bow_c1: dict[int, float] = {}
    alpha_c1: dict[int, float] = {}

    for c1, succs in grouped_cb.items():
        tot = cb_total[c1]
        if tot <= 0:
            bow_c1[c1] = 0.0
            alpha_c1[c1] = 1.0
            continue
        gamma = (d2_1 * cb_n1[c1] + d2_2 * cb_n2[c1] + d2_3 * cb_n3p[c1]) / tot
        raw_probs: list[tuple[int, float]] = []
        sum_p = 0.0
        sum_lo = 0.0
        for c2, cnt in succs:
            d = d2_1 if cnt == 1 else (d2_2 if cnt == 2 else d2_3)
            p_kn = (cnt - d) / tot + gamma * p_char_uni[c2]
            p_kn = max(1e-11, min(0.999999, p_kn))
            raw_probs.append((c2, p_kn))
            sum_p += p_kn
            sum_lo += p_char_uni[c2]

        if sum_p >= 0.999999:
            scale = 0.99999 / sum_p
            raw_probs = [(c2, p * scale) for c2, p in raw_probs]
            sum_p = 0.99999

        rem_hi = max(1e-12, 1.0 - sum_p)
        rem_lo = max(1e-12, 1.0 - sum_lo)
        a1 = rem_hi / rem_lo
        alpha_c1[c1] = a1
        bow_c1[c1] = clamp_log10(a1)
        for c2, p_kn in raw_probs:
            cb_prob_map[(c1 << 16) | c2] = p_kn

    def eff_p_cb(c2: int, c3: int) -> float:
        p = cb_prob_map.get((c2 << 16) | c3)
        if p is not None:
            return p
        return alpha_c1.get(c2, 1.0) * p_char_uni[c3]

    # 3. 字元三元組統計、過濾並立即釋放原始 Counter
    d3_1, d3_2, d3_3 = compute_mkn_discounts(acc.char_tri.values())
    ct_total: dict[int, int] = defaultdict(int)
    ct_n1: dict[int, int] = defaultdict(int)
    ct_n2: dict[int, int] = defaultdict(int)
    ct_n3p: dict[int, int] = defaultdict(int)
    ct_cands: list[tuple[int, int]] = []

    for key, cnt in acc.char_tri.items():
        prefix = key >> 16
        if prefix not in retained_cb_set:
            continue
        c3 = key & 0xFFFF
        if c3 == 0 or c3 not in kept_char_ids:
            continue
        ct_total[prefix] += cnt
        if cnt == 1:
            ct_n1[prefix] += 1
        elif cnt == 2:
            ct_n2[prefix] += 1
        else:
            ct_n3p[prefix] += 1
        if cnt >= 4:
            ct_cands.append((cnt, key))

    acc.char_tri.clear()
    gc.collect()

    top_ct = heapq.nlargest(max_char_tri, ((cnt, -k, k) for cnt, k in ct_cands))
    del ct_cands
    gc.collect()

    grouped_ct: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for cnt, _nk, key in top_ct:
        grouped_ct[key >> 16].append((key & 0xFFFF, cnt))
    del top_ct

    bow_cb: dict[int, float] = {}
    ct_buffer: list[str] = []
    ct_count = 0

    for prefix, succs in grouped_ct.items():
        tot = ct_total.get(prefix, 0)
        if tot <= 0:
            bow_cb[prefix] = 0.0
            continue
        gamma = (d3_1 * ct_n1[prefix] + d3_2 * ct_n2[prefix] + d3_3 * ct_n3p[prefix]) / tot
        c1 = prefix >> 16
        c2 = prefix & 0xFFFF
        raw_probs = []
        sum_p = 0.0
        sum_lo = 0.0
        for c3, cnt in succs:
            d = d3_1 if cnt == 1 else (d3_2 if cnt == 2 else d3_3)
            p_lo = eff_p_cb(c2, c3)
            p_kn = (cnt - d) / tot + gamma * p_lo
            p_kn = max(1e-11, min(0.999999, p_kn))
            raw_probs.append((c3, p_kn))
            sum_p += p_kn
            sum_lo += p_lo

        if sum_p >= 0.999999:
            scale = 0.99999 / sum_p
            raw_probs = [(c3, p * scale) for c3, p in raw_probs]
            sum_p = 0.99999

        rem_hi = max(1e-12, 1.0 - sum_p)
        rem_lo = max(1e-12, 1.0 - sum_lo)
        bow_cb[prefix] = clamp_log10(rem_hi / rem_lo)

        s1 = id_to_char[c1]
        s2 = id_to_char[c2]
        for c3, p_kn in raw_probs:
            ct_buffer.append(f"CT\t{s1}\t{s2}\t{id_to_char[c3]}\t{clamp_log10(p_kn):.6f}\n")
            ct_count += 1

    # 輸出 CU, CB, CT
    cu_count = 0
    out_file.write(f"CU\t<s>\t{LOG10_MIN:.6f}\t{bow_c1.get(0, 0.0):.6f}\n")
    cu_count += 1
    out_file.write(f"CU\t{UNK_WORD}\t{clamp_log10(p_char_unk):.6f}\t0.000000\n")
    cu_count += 1
    for cid in sorted(kept_char_ids):
        if cid == 0:
            continue
        out_file.write(
            f"CU\t{id_to_char[cid]}\t{clamp_log10(p_char_uni[cid]):.6f}\t{bow_c1.get(cid, 0.0):.6f}\n"
        )
        cu_count += 1

    cb_count = 0
    for c1 in sorted(grouped_cb.keys()):
        s1 = id_to_char[c1]
        for c2, _cnt in grouped_cb[c1]:
            key = (c1 << 16) | c2
            lp = clamp_log10(cb_prob_map[key])
            lb = bow_cb.get(key, 0.0)
            out_file.write(f"CB\t{s1}\t{id_to_char[c2]}\t{lp:.6f}\t{lb:.6f}\n")
            cb_count += 1

    out_file.writelines(ct_buffer)
    del ct_buffer, grouped_ct, grouped_cb, bow_cb, cb_prob_map
    gc.collect()

    return {
        "char_bi_discounts": [round(d2_1, 4), round(d2_2, 4), round(d2_3, 4)],
        "char_tri_discounts": [round(d3_1, 4), round(d3_2, 4), round(d3_3, 4)],
        "char_vocab_emitted": cu_count,
        "char_bigrams_emitted": cb_count,
        "char_trigrams_emitted": ct_count,
        "elapsed_sec": round(time.monotonic() - t0, 1),
    }


def run_self_test_seg(lexicon_path: str | Path) -> int:
    seg = LexiconSegmenter(lexicon_path)
    cases = [
        ("美是主觀", ["美", "是", "主觀"]),
        ("美式主觀", ["美式", "主觀"]),
        ("我想要去台北吃牛肉麵", ["我", "想要", "去", "台北", "吃", "牛肉麵"]),
        ("台灣軟體工程師建立檔案", ["台灣", "軟體", "工程師", "建立", "檔案"]),
        ("中華民國憲法規定", ["中華民國", "憲法", "規定"]),
        ("他在開發初期", ["他", "在", "開發初期"]),
        ("它在開發初期", ["它", "在", "開發初期"]),
        ("他的新作品", ["他", "的", "新作品"]),
    ]
    failed = 0
    for text, expected in cases:
        got = seg.segment_words(text)
        ok = got == expected
        mark = "PASS" if ok else "FAIL"
        print(f"[self-test-seg] {mark}: {text} -> {got} (expected {expected})", flush=True)
        if not ok:
            failed += 1
    return 1 if failed else 0


def load_dev_hashes(path: str | Path) -> set[str]:
    hashes: set[str] = set()
    with open_maybe_gz_text(path, "rt") as f:
        for line in f:
            s = line.strip().lower()
            if s:
                hashes.add(s)
    return hashes


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description="如喜（RuhI）商業授權安全之台灣正體 N-gram 模型訓練工具")
    ap.add_argument("--lexicon", required=True, help="核心詞庫路徑 (.tsv 或 .tsv.gz)")
    ap.add_argument("--dev-hashes", help="開發評測句子 SHA-256 雜湊名單 (.txt)")
    ap.add_argument("--out-arpa", help="輸出壓縮 ARPA TSV 路徑 (.tsv 或 .tsv.gz)")
    ap.add_argument("--out-manifest", help="輸出訓練報告 JSON 路徑")
    ap.add_argument("--target-chars", type=int, default=80_000_000, help="目標乾淨總字數上限")
    ap.add_argument("--max-word-bi", type=int, default=2_200_000, help="詞彙雙元組保留上限")
    ap.add_argument("--max-char-vocab", type=int, default=25_000, help="字元字表保留上限")
    ap.add_argument("--max-char-bi", type=int, default=750_000, help="字元雙元組保留上限")
    ap.add_argument("--max-char-tri", type=int, default=1_600_000, help="字元三元組保留上限")
    ap.add_argument("--fineweb-timeout-sec", type=int, default=600, help="FineWeb-2 串流逾時上限（秒）")
    ap.add_argument("--self-test-seg", action="store_true", help="僅執行本機 S5 斷詞回歸驗證並結束")
    return ap.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test_seg:
        return run_self_test_seg(args.lexicon)

    if not args.dev_hashes or not args.out_arpa or not args.out_manifest:
        print("[錯誤] 完整訓練模式必須提供 --dev-hashes、--out-arpa 與 --out-manifest", file=sys.stderr)
        return 2

    t_start = time.monotonic()
    dev_hashes = load_dev_hashes(args.dev_hashes)
    purity = TaiwanPurityFilter(dev_hashes)
    print(f"[lexicon] 載入詞庫：{args.lexicon}", flush=True)
    segmenter = LexiconSegmenter(args.lexicon)
    acc = NgramAccumulator(segmenter)
    ingestor = CorpusIngestor(purity, acc, args.target_chars, args.fineweb_timeout_sec)

    # 依四大領域授權安全清單依序載入
    ingestor.run_fineweb_tw(cap=40_000_000)
    ingestor.run_ivod(cap=14_000_000)
    ingestor.run_ly_interpellation(cap=4_000_000)
    ingestor.run_ly_bill(cap=2_000_000)
    ingestor.run_parquet_source(
        "tw_reasoning_instruct_50k",
        "twinkle-ai/tw-reasoning-instruct-50k",
        "data/datasets.parquet",
        "MIT",
        cap=10_000_000,
        preferred_cols=("input", "output", "think"),
    )
    ingestor.run_parquet_source(
        "tw_function_call_reasoning_10k",
        "twinkle-ai/tw-function-call-reasoning-10k",
        "data/train-00000-of-00001.parquet",
        "CC-BY-4.0",
        cap=2_500_000,
        preferred_cols=("query_zhtw", "think"),
    )
    ingestor.run_parquet_source(
        "tw_math_reasoning_2k",
        "twinkle-ai/tw-math-reasoning-2k",
        "data/train-00000-of-00001.parquet",
        "MIT",
        cap=1_500_000,
    )
    ingestor.run_jsonl_source(
        "tw_processed_law_article",
        "lianghsun/tw-processed-law-article",
        "datasets.jsonl",
        "CC-BY-4.0",
        cap=3_500_000,
    )
    ingestor.run_jsonl_source(
        "tw_processed_law_ctx",
        "lianghsun/tw-processed-law-ctx",
        "train.jsonl",
        "Apache-2.0",
        cap=2_500_000,
    )

    fineweb_chars = ingestor.source_stats["fineweb_tw"]["clean_chars"]
    total_chars = ingestor.total_clean_chars

    manifest: dict = {
        "schema_version": 1,
        "segmentation_policy": "ruhi_core_viterbi_production_s5",
        "target_chars": args.target_chars,
        "total_clean_chars": total_chars,
        "fineweb_tw_clean_chars": fineweb_chars,
        "attributions": ATTRIBUTION_NOTICES,
        "purity_filter_stats": purity.stats,
        "sources": ingestor.source_stats,
        "model_stats": {},
        "status": "ok",
    }

    # 釋放不再使用的去重與雜湊結構
    purity.clause_freq.clear()
    purity.dev_hashes.clear()
    segmenter.segment_to_ids.cache_clear()
    gc.collect()

    if fineweb_chars < 5_000_000 or total_chars < 20_000_000:
        manifest["status"] = "ABORTED_INSUFFICIENT_CORPUS"
        manifest["elapsed_total_sec"] = round(time.monotonic() - t_start, 1)
        Path(args.out_manifest).parent.mkdir(parents=True, exist_ok=True)
        with open(args.out_manifest, "w", encoding="utf-8") as mf:
            json.dump(manifest, mf, ensure_ascii=False, indent=2)
        print(
            f"[中止] 語料字數未達最低門檻: fineweb_tw={fineweb_chars:,}, total={total_chars:,}",
            file=sys.stderr,
        )
        return 2

    Path(args.out_arpa).parent.mkdir(parents=True, exist_ok=True)
    with open_maybe_gz_text(args.out_arpa, "wt") as out_f:
        print("[MKN] 計算並輸出詞彙級 N-gram (WU/WB)...", flush=True)
        word_stats = emit_word_level_and_free(out_f, segmenter, acc, args.max_word_bi)
        print(f"[MKN] 詞彙級完成: {word_stats}", flush=True)

        print("[MKN] 計算並輸出字元級 N-gram (CU/CB/CT)...", flush=True)
        char_stats = emit_char_level(
            out_f, acc, args.max_char_vocab, args.max_char_bi, args.max_char_tri
        )
        print(f"[MKN] 字元級完成: {char_stats}", flush=True)

    manifest["model_stats"] = {**word_stats, **char_stats}
    manifest["elapsed_total_sec"] = round(time.monotonic() - t_start, 1)
    Path(args.out_manifest).parent.mkdir(parents=True, exist_ok=True)
    with open(args.out_manifest, "w", encoding="utf-8") as mf:
        json.dump(manifest, mf, ensure_ascii=False, indent=2)
    print(f"[完成] 總耗時 {manifest['elapsed_total_sec']}s -> {args.out_arpa}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
