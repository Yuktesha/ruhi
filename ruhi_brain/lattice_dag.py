"""
Ruhi (如喜) Input Method Framework
ruhi_brain.lattice_dag - 詞網格圖 (Word Lattice DAG) 建構器
"""

from typing import List, Dict, Tuple
from ..ruhi_layout.hsu_fsm import HsuSyllable
from ..ruhi_lexicon.lexicon_manager import LexiconManager, LexiconEntry

class LatticeNode:
    """詞網格圖節點"""
    def __init__(self, start_idx: int, end_idx: int, entry: LexiconEntry):
        self.start_idx = start_idx  # 在音節陣列中的起始位置
        self.end_idx = end_idx      # 在音節陣列中的結束位置 (exclusive)
        self.entry = entry          # 對應的詞條

    @property
    def length(self) -> int:
        return self.end_idx - self.start_idx

    @property
    def word(self) -> str:
        return self.entry.word

    @property
    def weight(self) -> int:
        return self.entry.weight

    def __repr__(self) -> str:
        return f"[{self.start_idx}:{self.end_idx} {self.word} (w={self.weight})]"


class LatticeDAG:
    """以音節序列為骨架的長詞優先網格圖"""
    def __init__(self, lexicon: LexiconManager):
        self.lexicon = lexicon

    def build_dag(self, syllables: List[HsuSyllable], exact_tone: bool = True) -> Dict[int, List[LatticeNode]]:
        """
        根據輸入音節清單，為每個起始位置構建所有可能的詞條跳轉分支
        返回: {start_idx: [LatticeNode, ...]}
        """
        n = len(syllables)
        dag: Dict[int, List[LatticeNode]] = {i: [] for i in range(n)}
        
        # 遍歷所有可能的切片區間 [i:j]
        # 長度由長至短 (最大詞長度限制為 6)
        max_word_len = min(6, n)
        
        for i in range(n):
            for length in range(1, min(max_word_len + 1, n - i + 1)):
                j = i + length
                sub_syls = syllables[i:j]
                
                # 組合注音字串
                bpm_str = " ".join([s.bopomofo_display for s in sub_syls])
                matched_entries = self.lexicon.lookup_bopomofo(bpm_str, exact_tone=exact_tone)
                
                # 若精確調號未找到且允許模糊調號，退回無調號重查
                if not matched_entries and not exact_tone:
                    matched_entries = self.lexicon.lookup_bopomofo(bpm_str, exact_tone=False)

                for entry in matched_entries:
                    node = LatticeNode(start_idx=i, end_idx=j, entry=entry)
                    dag[i].append(node)

            # 若此位置沒有找到任何詞，插入一個單字備用節點
            if not dag[i]:
                single_syl = syllables[i]
                fallback_entry = LexiconEntry(
                    word=single_syl.bopomofo_display or "?", 
                    bopomofo=single_syl.bopomofo_display, 
                    weight=1, 
                    category="unknown"
                )
                dag[i].append(LatticeNode(start_idx=i, end_idx=i+1, entry=fallback_entry))

        return dag
