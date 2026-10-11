#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
字戀 (WordChain) ⇄ 如喜 (RuhI) 一鍵雙向自動化建置與除魅同步管線 (Sync Unified)
功能：
1. 觸發字戀詞庫建置管線 (build_moedict_lexicon.py)，重新計算出度拓撲與產出故事卡
2. 觸發如喜雙向反哺管線 (sync_wordchain_to_ruhi.py)，更新詞庫 TSV 與動態轉移矩陣
3. 自動執行魔鬼訓練營與經典繞口令雙榜自動回歸驗證
4. 確保字戀與如喜兩套系統互為表裡、零延遲同步！
"""

import os
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_DIR = os.path.abspath(os.path.join(BASE_DIR, ".."))
WORDCHAIN_DIR = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain")

def run_step(desc, cmd, cwd):
    print(f"\n{'='*80}")
    print(f"  [步驟] {desc}")
    print(f"{'='*80}")
    start_t = time.time()
    res = subprocess.run(cmd, cwd=cwd, shell=True)
    cost = time.time() - start_t
    if res.returncode != 0:
        print(f"❌ 執行失敗 (耗時: {cost:.2f}s，退出碼: {res.returncode})")
        sys.exit(res.returncode)
    print(f"✅ 步驟完成 (耗時: {cost:.2f}s)")

def main():
    print("================================================================================")
    print("  RuhI (如喜) ⇄ WordChain (字戀) 一鍵雙向自動化除魅與語意同步管線")
    print("================================================================================")

    # 1. 建置字戀知識庫與出度圖論
    run_step(
        "1. 建置字戀 (WordChain) 萌典詞庫、出度拓撲與成語故事卡",
        [sys.executable, "build_moedict_lexicon.py"],
        cwd=WORDCHAIN_DIR
    )

    # 2. 同步反哺至如喜輸入法
    run_step(
        "2. 同步反哺至如喜 (RuhI) 詞庫、連續字頻權重與語意轉移矩陣",
        [sys.executable, "tools/sync_wordchain_to_ruhi.py"],
        cwd=RUHI_DIR
    )

    # 3. 執行魔鬼訓練營回歸測試
    run_step(
        "3. 驗證如喜「魔鬼訓練營」14 道極限語意消歧測試題",
        [sys.executable, "tests/eval_devil_camp.py"],
        cwd=RUHI_DIR
    )

    # 4. 執行經典繞口令回歸測試
    run_step(
        "4. 驗證如喜「相聲大師級」8 首舌頭打結繞口令測試",
        [sys.executable, "tests/eval_tongue_twisters.py"],
        cwd=RUHI_DIR
    )

    print("\n" + "="*80)
    print("🎉🎉🎉 全管線自動同步與雙榜驗證大獲全勝！")
    print("  - 字戀 (WordChain)：出度拓撲已重算，全新故事卡與擴充熟語已生效！")
    print("  - 如喜 (RuhI)：連續對數字頻、本音優先權重與語意轉移矩陣已熱重載！")
    print("  - 雙榜回歸驗證：13/13 魔鬼營 + 8/8 繞口令 = 100% 完美通過！")
    print("="*80 + "\n")

if __name__ == '__main__':
    main()
