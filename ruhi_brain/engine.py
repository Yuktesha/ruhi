"""
Ruhi (如喜) Input Method Framework
ruhi_brain.engine - Ruhi 智慧輸入法協同引擎中樞 (支援 16 萬詞庫與本機 Gemma AI)
"""

import os
from typing import List, Dict, Tuple, Optional
from ..ruhi_layout.hsu_fsm import HsuFSM, HsuSyllable
from ..ruhi_layout.hsu_table import HSU_PUNCTUATIONS
from ..ruhi_lexicon.lexicon_manager import LexiconManager, LexiconEntry
from ..ruhi_lexicon.templates_manager import TemplatesManager, TemplateItem
from .lattice_dag import LatticeDAG, LatticeNode
from .viterbi_router import ViterbiRouter
from .ai_provider import RuhiAIProvider, OllamaGemmaProvider

class RuhiEngineMode:
    NORMAL = "normal"               # 正常注音輸入模式
    TEMPLATE_SELECT = "templates"   # 文字範本快選模式 (` 按一下)
    QUICK_NUM = "quick_num"         # 數字快打模式 (` 按兩下)
    LEXICON_SWITCH = "lexicon"      # 專業詞庫切換模式 (~)
    CANDIDATE_POPUP = "candidates"  # 完整候選字清單展開 (Q 連按 N 下)


class RuhiEngine:
    """Ruhi 輸入法核心中樞"""
    def __init__(self, data_dir: Optional[str] = None, enable_ai: bool = True):
        if not data_dir:
            data_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), "ruhi_lexicon", "data")
        self.data_dir = data_dir

        # 核心元件初始化
        self.fsm = HsuFSM()
        self.lexicon = LexiconManager()
        self.templates = TemplatesManager()
        self.dag_builder = LatticeDAG(self.lexicon)
        self.router = ViterbiRouter()
        
        # 本地 AI 大腦 (可熱插拔)
        self.ai_provider: Optional[RuhiAIProvider] = OllamaGemmaProvider() if enable_ai else None

        # 狀態控制與設定
        self.mode: str = RuhiEngineMode.NORMAL
        self.q_press_count: int = 0
        self.q_popup_threshold: int = 5     # Q 連續按 N 下彈出選字窗 (預設 5)
        self.backtick_press_count: int = 0  # 反引號連續按鍵計數

        # 當前輸出與緩衝區
        self.committed_text: str = ""       # 已送出的文字
        self.composed_sentence: str = ""    # 當前音節推導出的最佳整句
        self.current_candidates: List[str] = [] # 當前候選字/詞清單
        self.candidate_cycle_idx: int = 0   # Q 鍵輪播當前索引
        
        # 載入預設資料
        self._load_defaults()

    def _load_defaults(self):
        """優先載入 16 萬詞教育部萌典詞庫，若無則退回核心詞庫"""
        moe_lex = os.path.join(self.data_dir, "lexicon_moe.tsv")
        core_lex = os.path.join(self.data_dir, "lexicon_core.tsv")

        if os.path.exists(moe_lex):
            self.lexicon.load_tsv(moe_lex, category="moe")
        elif os.path.exists(core_lex):
            self.lexicon.load_tsv(core_lex, category="core")

        tpl_file = os.path.join(self.data_dir, "templates.tsv")
        if os.path.exists(tpl_file):
            self.templates.load_from_tsv(tpl_file)

    def reset_buffer(self):
        """清空輸入緩衝區與狀態"""
        self.fsm.reset()
        self.mode = RuhiEngineMode.NORMAL
        self.q_press_count = 0
        self.backtick_press_count = 0
        self.composed_sentence = ""
        self.current_candidates = []
        self.candidate_cycle_idx = 0

    def process_key(self, key: str) -> Dict:
        """
        處理外部傳入的按鍵事件
        返回當前引擎狀態字典
        """
        # 1. 處理波浪號 `~` (Shift + `) ➔ 專業詞庫切換
        if key == '~':
            self.mode = RuhiEngineMode.LEXICON_SWITCH
            return self._build_state_response(action="show_lexicon_switcher")

        # 2. 處理反引號 `` ` ``
        if key == '`':
            self.backtick_press_count += 1
            if self.backtick_press_count == 1:
                self.mode = RuhiEngineMode.TEMPLATE_SELECT
                return self._build_state_response(action="show_templates")
            elif self.backtick_press_count >= 2:
                self.mode = RuhiEngineMode.QUICK_NUM
                return self._build_state_response(action="show_quick_num")

        # 若按下非反引號鍵，重置反引號計數
        if key != '`':
            self.backtick_press_count = 0

        # 3. 處理數字鍵選字 (當候選字清單非空時，按 1~9 直接選取)
        if self.current_candidates and key in "123456789":
            digit_idx = int(key) - 1
            if digit_idx < len(self.current_candidates):
                chosen = self.current_candidates[digit_idx]
                self.committed_text += chosen
                self.reset_buffer()
                return self._build_state_response(action="commit_candidate", committed=chosen)

        # 4. 處理 Q 鍵 (許氏快速候選輪播與清單彈出)
        if key.lower() == 'q' and self.fsm.get_all_syllables():
            self.q_press_count += 1
            if self.q_press_count >= self.q_popup_threshold:
                self.mode = RuhiEngineMode.CANDIDATE_POPUP
                return self._build_state_response(action="show_candidate_popup")
            else:
                self._cycle_next_candidate()
                return self._build_state_response(action="cycle_candidate")

        # 非 Q 鍵則重置 Q 計數
        if key.lower() != 'q':
            self.q_press_count = 0

        # 5. 處理標點符號 (如 ，。！？)
        if key in HSU_PUNCTUATIONS:
            self._commit_current()
            punct = HSU_PUNCTUATIONS[key]
            self.committed_text += punct
            self.reset_buffer()
            return self._build_state_response(action="commit_punctuation", committed=punct)

        # 6. 處理 Enter (直接確認送出當前文字)
        if key in ['\r', '\n', 'Return', 'Enter']:
            committed = self.composed_sentence or self.fsm.get_display_preedit()
            self.committed_text += committed
            self.reset_buffer()
            return self._build_state_response(action="commit_enter", committed=committed)

        # 7. 處理 Backspace (退格)
        if key in ['\b', 'BackSpace', 'Backspace']:
            if self.fsm.raw_keys:
                self.fsm.raw_keys.pop()
                all_keys = list(self.fsm.raw_keys)
                self.fsm.reset()
                for k in all_keys:
                    self.fsm.parse_hsu_key(k)
                self._update_sentence_composition()
            else:
                self.reset_buffer()
            return self._build_state_response(action="backspace")

        # 8. 餵入許氏鍵盤 FSM
        success = self.fsm.parse_hsu_key(key)
        if success:
            self._update_sentence_composition()
            return self._build_state_response(action="update_preedit")

        # 9. 首碼或文字範本直接匹配
        tpl_match = self.templates.match(key)
        if tpl_match:
            expanded, _ = tpl_match.expand()
            self.committed_text += expanded
            self.reset_buffer()
            return self._build_state_response(action="commit_template", committed=expanded)

        return self._build_state_response(action="none")

    def _update_sentence_composition(self):
        """根據當前音節清單，透過 Lattice DAG + Viterbi 解算最佳整句與候選詞"""
        syls = self.fsm.get_all_syllables()
        if not syls:
            self.composed_sentence = ""
            self.current_candidates = []
            return

        dag = self.dag_builder.build_dag(syls, exact_tone=True)
        nodes, score = self.router.solve_best_path(len(syls), dag)
        
        self.composed_sentence = "".join([n.word for n in nodes])
        self.current_candidates = self.router.get_candidate_alternatives(dag, 0)
        self.candidate_cycle_idx = 0

    def _cycle_next_candidate(self):
        """Q 鍵輪換下一個候選詞"""
        if not self.current_candidates:
            return
        self.candidate_cycle_idx = (self.candidate_cycle_idx + 1) % len(self.current_candidates)
        chosen = self.current_candidates[self.candidate_cycle_idx]
        self.composed_sentence = chosen

    def _commit_current(self):
        """將當前合成的文字存入 committed_text"""
        if self.composed_sentence:
            self.committed_text += self.composed_sentence

    def _build_state_response(self, action: str = "", committed: str = "") -> Dict:
        """打包引擎即時狀態"""
        return {
            "action": action,
            "mode": self.mode,
            "preedit": self.fsm.get_display_preedit(),
            "composed": self.composed_sentence,
            "candidates": self.current_candidates,
            "active_candidate": self.current_candidates[self.candidate_cycle_idx] if self.current_candidates else "",
            "candidate_idx": self.candidate_cycle_idx,
            "q_press_count": self.q_press_count,
            "committed_chunk": committed,
            "full_committed_text": self.committed_text
        }
