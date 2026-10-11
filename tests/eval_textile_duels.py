#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
texTile (詞布達意) 織句消歧自動化驗證腳本
自動從 wordchain/golden_quotes.json 讀取大十織出的消歧金句，
轉成注音與許氏雙拼後餵入如喜 Rust 引擎，驗證如喜打字是否 100% 正確命中金句！
"""

import ctypes
import json
import os
import re
import sys
import pypinyin

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.abspath(os.path.join(BASE_DIR, ".."))
DLL_PATH = os.path.join(RUHI_ROOT, "ruhi-core", "target", "release", "ruhi.dll")
GOLDEN_PATH = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain/golden_quotes.json")
TSV_PATH = os.path.join(RUHI_ROOT, "ruhi_lexicon", "data", "lexicon_moe.tsv")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from eval_devil_camp import get_hsu_keys_for_syllables

def run_textile_eval():
    print("=" * 80)
    print("  texTile (詞布達意) 織句消歧閉環評測")
    print("=" * 80)

    if not os.path.exists(GOLDEN_PATH):
        print("尚未有典藏金句。")
        return

    with open(GOLDEN_PATH, "r", encoding="utf-8") as f:
        golden_list = json.load(f)

    if not golden_list:
        print("金句庫為空。")
        return

    lib = ctypes.CDLL(DLL_PATH)
    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_transitions_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p
    lib.ruhi_engine_reset.argtypes = [ctypes.c_void_p]

    engine = lib.ruhi_engine_create()
    lib.ruhi_engine_load_tsv(engine, TSV_PATH.encode("utf-8"))
    trans_tsv = os.path.join(RUHI_ROOT, "ruhi_lexicon", "data", "transitions_user.tsv").encode("utf-8")
    lib.ruhi_engine_load_transitions_tsv(engine, trans_tsv)

    passed = 0
    total = len(golden_list)

    for i, item in enumerate(golden_list, 1):
        clean_target = re.sub(r'[，。！？；：「」『』、\s\?]+', '', item['full_sentence'])
        
        # 轉注音
        bps = pypinyin.pinyin(clean_target, style=pypinyin.BOPOMOFO)
        syls = [b[0] for b in bps]
        # 修正輕聲在 pypinyin 的位置 (如 ˙ㄇㄜ)
        fixed_syls = []
        for s in syls:
            if s.endswith('˙'):
                s = '˙' + s[:-1]
            fixed_syls.append(s)

        keys = get_hsu_keys_for_syllables(fixed_syls)

        lib.ruhi_engine_reset(engine)
        output = ""
        for k in keys:
            ptr = lib.ruhi_engine_process_key(engine, k.encode('ascii'))
            if ptr:
                res = json.loads(ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8'))
                if res.get("action", "").startswith("commit"):
                    output += res.get("committed_chunk", "")
                elif res.get("composed"):
                    output = res.get("composed")

        clean_out = re.sub(r'[，。！？；：「」『』、\s\?]+', '', output)
        is_pass = (clean_target == clean_out)
        if is_pass:
            passed += 1
            print(f"\n[PASS] 第 {i:2d} 句: 【{'】vs【'.join(item['tiles'])}】")
            print(f"  目標: {clean_target}")
            print(f"  如喜: {clean_out}")
        else:
            print(f"\n[FAIL] 第 {i:2d} 句: 【{'】vs【'.join(item['tiles'])}】")
            print(f"  目標: {clean_target}")
            print(f"  如喜: {clean_out}")

    print("\n" + "-" * 80)
    print(f"總結: {passed} / {total} 通過 (正確率: {passed/total*100:.1f}%)")
    print("=" * 80)

if __name__ == "__main__":
    run_textile_eval()
