"""
Ruhi (如喜) Input Method Framework
ruhi_lexicon.lexicon_manager - 模組化試算表 (TSV) 詞庫管理中樞
"""

import os
from typing import Dict, List, Optional, Tuple, Set

class LexiconEntry:
    def __init__(self, word: str, bopomofo: str, weight: int = 100, category: str = "core", note: str = ""):
        self.word = word.strip()
        self.bopomofo = bopomofo.strip() # 例: "ㄓㄨㄥ ㄏㄨㄚˊ ㄇㄧㄣˊ ㄍㄨㄛˊ"
        self.weight = int(weight)
        self.category = category.strip()
        self.note = note.strip()

        # 生成純注音（無調號）鍵值
        self.bopomofo_notone = self._strip_tones(self.bopomofo)
        # 生成首碼鍵值 (聲母列表，如 jhmg)
        self.headcode = self._extract_headcode(self.bopomofo)

    @staticmethod
    def _strip_tones(bpm: str) -> str:
        for t in ['ˊ', 'ˇ', 'ˋ', '˙']:
            bpm = bpm.replace(t, '')
        return bpm

    @staticmethod
    def _extract_headcode(bpm: str) -> str:
        """從注音字串提取首碼代號"""
        # 首碼簡化映射 (ㄓ->j, ㄏ->h, ㄇ->m, ㄍ->g 等)
        initial_map = {
            'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
            'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
            'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
            'ㄐ': 'j', 'ㄑ': 'c', 'ㄒ': 'v',
            'ㄓ': 'j', 'ㄔ': 'c', 'ㄕ': 's', 'ㄖ': 'r',
            'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's',
            'ㄧ': 'e', 'ㄨ': 'x', 'ㄩ': 'u'
        }
        syllables = bpm.split()
        res = []
        for s in syllables:
            s_clean = s.replace('˙', '')
            if s_clean:
                first_char = s_clean[0]
                res.append(initial_map.get(first_char, ''))
        return "".join(res)


class LexiconManager:
    """詞庫管理器"""
    def __init__(self):
        self.entries: List[LexiconEntry] = []
        # 注音索引: "ㄓㄨㄥ ㄏㄨㄚˊ ㄇㄧㄣˊ ㄍㄨㄛˊ" -> [LexiconEntry, ...]
        self.bopomofo_index: Dict[str, List[LexiconEntry]] = {}
        # 無調號索引: "ㄓㄨㄥ ㄏㄨㄚ ㄇㄧㄣ ㄍㄨㄛ" -> [LexiconEntry, ...]
        self.notone_index: Dict[str, List[LexiconEntry]] = {}
        # 首碼索引: "jhmg" -> [LexiconEntry, ...]
        self.headcode_index: Dict[str, List[LexiconEntry]] = {}
        # 已載入之詞庫模組
        self.loaded_zones: Set[str] = set()

    def load_tsv(self, filepath: str, category: str = "core"):
        """載入單一 TSV 詞庫檔案"""
        if not os.path.exists(filepath):
            return
        
        with open(filepath, "r", encoding="utf-8") as f:
            for line_idx, line in enumerate(f):
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                # 跳過表頭
                if line_idx == 0 and ("WORD" in line or "詞彙" in line):
                    continue

                parts = line.split("\t")
                if len(parts) >= 2:
                    word = parts[0].strip()
                    bpm = parts[1].strip()
                    weight = int(parts[2]) if len(parts) > 2 and parts[2].isdigit() else 100
                    cat = parts[3].strip() if len(parts) > 3 else category
                    note = parts[4].strip() if len(parts) > 4 else ""

                    entry = LexiconEntry(word, bpm, weight, cat, note)
                    self._add_entry(entry)

        self.loaded_zones.add(category)
        self._sort_indices()

    @staticmethod
    def is_archaic_polyphone(word: str, bpm: str) -> bool:
        clean_bpm = bpm.replace('˙', '')
        if (word, clean_bpm) in [
            ("尾", "ㄧˇ"),
            ("會", "ㄏㄨㄟˇ"),
            ("篼", "ㄉㄡ"),
            ("荊婦", "ㄐㄧㄥ ㄈㄨˋ"),
            ("時後", "ㄕˊ ㄏㄡˋ"),
            ("字擠", "ㄗˋ ㄐㄧˇ"),
        ] or word == "𣇫":
            return True
        return False

    def _add_entry(self, entry: LexiconEntry):
        if self.is_archaic_polyphone(entry.word, entry.bopomofo):
            return
        self.entries.append(entry)

        # 索引完整注音
        self.bopomofo_index.setdefault(entry.bopomofo, []).append(entry)
        # 索引無調號
        self.notone_index.setdefault(entry.bopomofo_notone, []).append(entry)
        # 索引首碼
        if entry.headcode:
            self.headcode_index.setdefault(entry.headcode, []).append(entry)

    def _sort_indices(self):
        """依權重降冪排序索引"""
        for k in self.bopomofo_index:
            self.bopomofo_index[k].sort(key=lambda x: x.weight, reverse=True)
        for k in self.notone_index:
            self.notone_index[k].sort(key=lambda x: x.weight, reverse=True)
        for k in self.headcode_index:
            self.headcode_index[k].sort(key=lambda x: x.weight, reverse=True)

    def lookup_bopomofo(self, bpm_str: str, exact_tone: bool = True) -> List[LexiconEntry]:
        """依注音查詢候選詞"""
        if exact_tone:
            return self.bopomofo_index.get(bpm_str, [])
        else:
            notone = LexiconEntry._strip_tones(bpm_str)
            return self.notone_index.get(notone, [])

    def lookup_headcode(self, code: str) -> List[LexiconEntry]:
        """依首碼查詢候選詞 (例如 jhmg -> 中華民國)"""
        return self.headcode_index.get(code.lower(), [])
