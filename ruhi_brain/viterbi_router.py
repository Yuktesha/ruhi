"""
Ruhi (如喜) Input Method Framework
ruhi_brain.viterbi_router - 雙向 Viterbi 最優路徑動態規劃路由 (長詞優先與語意加權)
"""

import math
from typing import List, Dict, Tuple, Optional
from .lattice_dag import LatticeNode

class ViterbiRouter:
    """Viterbi 動態規劃最優路徑解算器"""
    def __init__(self, syllable_base_score: float = 10.0, multi_char_bonus: float = 8.0):
        self.syllable_base_score = syllable_base_score
        self.multi_char_bonus = multi_char_bonus

    def solve_best_path(self, n_syllables: int, dag: Dict[int, List[LatticeNode]]) -> Tuple[List[LatticeNode], float]:
        """
        在詞網格圖中尋找整體權重與長詞得分最高的全域最佳路徑
        返回: (最優節點清單, 總評分)
        """
        if n_syllables == 0:
            return [], 0.0

        # dp[i] = (最高總分, 抵達該位置的最佳節點, 前驅位置)
        dp: Dict[int, Tuple[float, Optional[LatticeNode], int]] = {
            i: (-float('inf'), None, -1) for i in range(n_syllables + 1)
        }
        dp[0] = (0.0, None, 0)

        for i in range(n_syllables):
            current_score, _, _ = dp[i]
            if current_score == -float('inf'):
                continue

            nodes = dag.get(i, [])
            for node in nodes:
                j = node.end_idx
                word_len = node.length
                base_weight = max(1, node.weight)
                
                # 評分公式：
                # 1. 每個音節基礎得分（保證音節覆蓋連貫）
                # 2. 詞頻 log 得分
                # 3. 多字詞/成語長詞獎勵 (長詞優先，大幅抑制單字破碎化)
                syllable_score = word_len * self.syllable_base_score
                freq_score = math.log(base_weight)
                length_bonus = (word_len - 1) * self.multi_char_bonus if word_len > 1 else 0.0

                node_score = syllable_score + freq_score + length_bonus

                new_total_score = current_score + node_score
                if new_total_score > dp[j][0]:
                    dp[j] = (new_total_score, node, i)

        # 回溯最優路徑
        best_nodes: List[LatticeNode] = []
        curr = n_syllables
        
        while curr > 0:
            score, node, prev = dp[curr]
            if node is None:
                break
            best_nodes.append(node)
            curr = prev

        best_nodes.reverse()
        total_score = dp[n_syllables][0]
        return best_nodes, total_score

    def get_candidate_alternatives(self, dag: Dict[int, List[LatticeNode]], start_idx: int) -> List[str]:
        """獲取特定位置的所有候選詞清單"""
        nodes = dag.get(start_idx, [])
        sorted_nodes = sorted(nodes, key=lambda x: (x.length, x.weight), reverse=True)
        seen = set()
        candidates = []
        for n in sorted_nodes:
            if n.word not in seen:
                seen.add(n.word)
                candidates.append(n.word)
        return candidates
