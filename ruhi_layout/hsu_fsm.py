"""
Ruhi (如喜) Input Method Framework
ruhi_layout.hsu_fsm - 許氏鍵盤精密有限狀態機 (Precision Hsu FSM)
"""

from typing import List, Optional
from .hsu_table import (
    HSU_INITIALS, HSU_MEDIALS, HSU_FINALS,
    HSU_TONES, HSU_TONE_SYMBOLS, TONE_TO_BOPOMOFO
)

class HsuSyllable:
    """注音音節模型"""
    def __init__(self, initial: str = "", medial: str = "", final: str = "", tone: str = "1"):
        self.initial = initial  # 聲母
        self.medial = medial    # 介音
        self.final = final      # 韻母
        self.tone = tone        # 聲調 '1','2','3','4','5'

    @property
    def bopomofo_display(self) -> str:
        """標準繁體注音顯示（例如：ㄓㄨㄥ、ㄏㄨㄚˊ、ㄨㄛˇ）"""
        tone_mark = TONE_TO_BOPOMOFO.get(self.tone, "")
        if self.tone == '5':
            return f"˙{self.initial}{self.medial}{self.final}"
        return f"{self.initial}{self.medial}{self.final}{tone_mark}"

    def is_empty(self) -> bool:
        return not (self.initial or self.medial or self.final)

    def __repr__(self) -> str:
        return f"<HsuSyllable {self.bopomofo_display}>"


class HsuFSM:
    """許氏鍵盤雙拼狀態機"""
    
    def __init__(self):
        self.reset()

    def reset(self):
        self.raw_keys: List[str] = []
        self.syllables: List[HsuSyllable] = []
        self.cur_initial: str = ""
        self.cur_medial: str = ""
        self.cur_final: str = ""
        self.cur_tone: str = "1"

    def has_pending(self) -> bool:
        return bool(self.cur_initial or self.cur_medial or self.cur_final)

    def parse_hsu_key(self, key: str) -> bool:
        k = key.lower()

        # 1. 聲調結算 (當已具備聲母/介音/韻母任一項時)
        if self.has_pending():
            # 空白鍵一律作為一聲結算
            if k == ' ':
                self._commit_current('1')
                self.raw_keys.append(k)
                return True
            
            # 若為調號輔助符號 (/, v, \, .)
            if k in HSU_TONE_SYMBOLS:
                self._commit_current(HSU_TONE_SYMBOLS[k])
                self.raw_keys.append(k)
                return True

            # 若為字母調號鍵 d, f, a, s：
            if (self.cur_final) or (self.cur_medial and k in ['d', 'f', 'a', 's']) or (self.cur_initial and k in ['d', 'f', 'a', 's'] and k not in HSU_FINALS):
                self._commit_current(HSU_TONES[k])
                self.raw_keys.append(k)
                return True

        # 空白鍵在無暫存時不處理
        if k == ' ':
            return False

        # 2. 狀態 A：空音節開始 (尚未有任何聲母/介音/韻母)
        if not self.has_pending():
            if k in HSU_INITIALS:
                self.cur_initial = HSU_INITIALS[k]
                self.raw_keys.append(k)
                return True
            if k in HSU_MEDIALS:
                self.cur_medial = HSU_MEDIALS[k]
                self.raw_keys.append(k)
                return True
            if k in HSU_FINALS:
                self.cur_final = HSU_FINALS[k]
                self.raw_keys.append(k)
                return True
            return False

        # 3. 狀態 B：已有聲母，尚未有介音與韻母
        if self.cur_initial and not self.cur_medial and not self.cur_final:
            if k in HSU_MEDIALS:
                self.cur_medial = HSU_MEDIALS[k]
                self.raw_keys.append(k)
                return True
            if k in HSU_FINALS:
                self.cur_final = HSU_FINALS[k]
                self.raw_keys.append(k)
                return True
            if k in HSU_TONES:
                self._commit_current(HSU_TONES[k])
                self.raw_keys.append(k)
                return True
            if k in HSU_INITIALS:
                self._commit_current('1')
                self.cur_initial = HSU_INITIALS[k]
                self.raw_keys.append(k)
                return True

        # 4. 狀態 C：已有介音，尚未有韻母
        if self.cur_medial and not self.cur_final:
            if k in HSU_FINALS:
                self.cur_final = HSU_FINALS[k]
                self.raw_keys.append(k)
                return True
            if k in HSU_TONES:
                self._commit_current(HSU_TONES[k])
                self.raw_keys.append(k)
                return True
            if k in HSU_INITIALS:
                self._commit_current('1')
                self.cur_initial = HSU_INITIALS[k]
                self.raw_keys.append(k)
                return True

        # 5. 狀態 D：已有韻母 (音節拼裝已滿)
        if self.cur_final:
            if k in HSU_TONES:
                self._commit_current(HSU_TONES[k])
                self.raw_keys.append(k)
                return True
            self._commit_current('1')
            if k in HSU_INITIALS:
                self.cur_initial = HSU_INITIALS[k]
            elif k in HSU_MEDIALS:
                self.cur_medial = HSU_MEDIALS[k]
            elif k in HSU_FINALS:
                self.cur_final = HSU_FINALS[k]
            self.raw_keys.append(k)
            return True

        return False

    def _commit_current(self, tone: str):
        init = self.cur_initial
        med = self.cur_medial
        fin = self.cur_final

        # 許氏自動轉換：ㄓ/ㄔ/ㄕ 遇 ㄧ/ㄩ 轉 ㄐ/ㄑ/ㄒ
        if med in ['ㄧ', 'ㄩ']:
            if init == 'ㄓ':
                init = 'ㄐ'
            elif init == 'ㄔ':
                init = 'ㄑ'
            elif init == 'ㄕ':
                init = 'ㄒ'

        syl = HsuSyllable(initial=init, medial=med, final=fin, tone=tone)
        self.syllables.append(syl)
        
        self.cur_initial = ""
        self.cur_medial = ""
        self.cur_final = ""
        self.cur_tone = "1"

    def get_display_preedit(self) -> str:
        parts = [s.bopomofo_display for s in self.syllables]
        cur = f"{self.cur_initial}{self.cur_medial}{self.cur_final}"
        if cur:
            parts.append(cur)
        return " ".join(parts)

    def get_all_syllables(self) -> List[HsuSyllable]:
        res = list(self.syllables)
        if self.has_pending():
            init = self.cur_initial
            med = self.cur_medial
            fin = self.cur_final
            if med in ['ㄧ', 'ㄩ']:
                if init == 'ㄓ': init = 'ㄐ'
                elif init == 'ㄔ': init = 'ㄑ'
                elif init == 'ㄕ': init = 'ㄒ'
            res.append(HsuSyllable(init, med, fin, "1"))
        return res
