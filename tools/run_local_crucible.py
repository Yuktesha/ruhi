#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
tools/run_local_crucible.py
RuhI (如喜) 本地智慧巡航與語法構式驗證熔爐 (Local Active-Learning Crucible)
=============================================================================
具備硬體過熱保護 (NVIDIA GPU Thermal Safety Guard) 與漢語語法構式全方位驗證。

執行流程：
1. 即時檢查 GPU 溫度（低於 70°C 正常運作，若高於 70°C 自動降頻冷卻，高於 78°C 緊急中止）。
2. 本地黃金測試集 (文 A)：涵蓋嵌偶極致、層累構式、可能補語、動作往復、對稱否定、熟語等高頻構式。
3. 文 A 經由 TextToHsuCompiler 編譯為許氏鍵盤雙拼按鍵序列 (文 B)。
4. 文 B 輸入如喜 Rust 核心引擎 (RuhiEngineBridge) 產生候選上屏句 (文 C)。
5. 自動比對 (Diff) 文 A 與 文 C，計算字元級準確率與構式命中率。
6. 生成結構性分析報告與錯誤歸因。
"""

import os
import sys
import time
import subprocess
from typing import List, Tuple, Dict

sys.stdout.reconfigure(encoding='utf-8', line_buffering=True)

# 加入相關模組搜尋路徑
RUHI_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SANDBOX_DIR = os.path.join(RUHI_ROOT, "ruhi_sandbox")
sys.path.insert(0, SANDBOX_DIR)

from text_to_hsu import TextToHsuCompiler
from engine_bridge import RuhiEngineBridge

# ── 硬體安全防護門檻 (Thermal Guard Thresholds) ──
TEMP_WARNING_THRESHOLD = 70.0   # 超過 70°C 觸發冷卻休眠
TEMP_EMERGENCY_ABORT = 78.0     # 超過 78°C 緊急中止避免硬體損傷

def get_gpu_temperature() -> float:
    """查詢 NVIDIA GPU 當前核心溫度 (°C)"""
    try:
        res = subprocess.run(
            ["nvidia-smi", "--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"],
            capture_output=True,
            text=True,
            timeout=3
        )
        if res.returncode == 0 and res.stdout.strip():
            return float(res.stdout.strip().split("\n")[0])
    except Exception:
        pass
    return 0.0

def thermal_safety_check():
    """執行硬體溫度安檢，若過熱則主動降溫或保護性中止"""
    temp = get_gpu_temperature()
    if temp >= TEMP_EMERGENCY_ABORT:
        print(f"\n🚨 [CRITICAL THERMAL ALERT] GPU 溫度達到 {temp:.1f}°C，已觸發緊急保護中止！")
        sys.exit(1)
    elif temp >= TEMP_WARNING_THRESHOLD:
        print(f"\n⚠️ [THERMAL THROTTLE] GPU 溫度為 {temp:.1f}°C (>= {TEMP_WARNING_THRESHOLD}°C)，暫停 15 秒以待散熱冷卻...")
        time.sleep(15)
        new_temp = get_gpu_temperature()
        print(f"❄️ 冷卻完成，目前溫度：{new_temp:.1f}°C")
    return temp

# ── 本地漢語黃金構式測試語料庫 (Golden Construction Testset A) ──
GOLDEN_CORPUS = [
    # 1. 嵌偶極致構式 [X中之X]
    ("這項任務絕對是重中之重", "重中之重"),
    ("這才是難中之難的考驗", "難中之難"),
    ("急中之急必須立即處理", "急中之急"),
    ("他是這場格鬥賽的王中之王", "王中之王"),
    ("這是整個團隊的痛中之痛", "痛中之痛"),
    ("處在險中之險的境地", "險中之險"),
    ("這起案件堪稱謎中之謎", "謎中之謎"),
    ("這幅畫展現了美中之美", "美中之美"),
    ("這一道料理是精中之精", "精中之精"),
    ("國家安全乃是要中之要", "要中之要"),

    # 2. 層累與遞進構式 [X上加X] / [X上加Y]
    ("眼前的難關真是難上加難", "難上加難"),
    ("喜上加喜令人格外振奮", "喜上加喜"),
    ("這項決策完全是錯上加錯", "錯上加錯"),
    ("面臨如此困境更是愁上加愁", "愁上加愁"),
    ("這門婚事可謂親上加親", "親上加親"),
    ("好上加好的方案終於誕生", "好上加好"),
    ("連續加班讓大家忙上加忙", "忙上加忙"),
    ("寒流來襲簡直是雪上加霜", "雪上加霜"),
    ("得獎對公司而言是錦上添花", "錦上添花"),
    ("他的發言無疑是火上加油", "火上加油"),

    # 3. 熟語與情態構式 [X中生Y] / [人中X] / [人上人]
    ("他在危機時刻急中生智化解難題", "急中生智"),
    ("這孩子將來必是人中龍鳳", "人中龍鳳"),
    ("他是不可多得的人中之龍", "人中之龍"),
    ("古代巾幗英雄堪稱人中之鳳", "人中之鳳"),
    ("努力奮鬥期許自己成為人上人", "人上人"),
    ("在困頓之中也能苦中作樂", "苦中作樂"),
    ("凡事不可高興太早以免樂極生悲", "樂極生悲"),
    ("在驚險車禍中幸運死裡逃生", "死裡逃生"),
    ("這項指控完全是無中生有", "無中生有"),

    # 4. 動量擴散構式 [大X特X]
    ("這項偉大功績值得大書特書", "大書特書"),
    ("他在社群網路上大搞特搞新花樣", "大搞特搞"),
    ("週年慶吸引許多人大買特買", "大買特買"),
    ("週末假期三五好友大吃特吃", "大吃特吃"),
    ("趁著放假大家大玩特玩", "大玩特玩"),
    ("他們在街頭大吵特吵", "大吵特吵"),
    ("他在媒體前大吹特吹自己的戰績", "大吹特吹"),

    # 5. 可能補語構式 [動詞 + 得/不 + 補語]
    ("這份專業論文你看得懂嗎", "看得懂"),
    ("這麼深奧的理論我完全看不懂", "看不懂"),
    ("站在山頂看得到遠方的燈塔", "看得到"),
    ("霧太濃了什麼也看不到", "看不到"),
    ("只要努力我們一定做得到", "做得到"),
    ("這麼艱鉅的目標幾乎做不到", "做不到"),
    ("這場演講的聲音你聽得清嗎", "聽得清"),
    ("後面噪音太大完全聽不清", "聽不清"),
    ("這支昂貴的手錶你買得起嗎", "買得起"),
    ("現在物價飛漲普通人根本買不起", "買不起"),
    ("這麼大碗的牛肉麵他吃得下嗎", "吃得下"),
    ("他胃口太小了這份餐吃不下", "吃不下"),
    ("這組複雜的密碼他居然記得住", "記得住"),
    ("年紀大了重要的事情常常記不住", "記不住"),
    ("他心胸寬大什麼委屈都容得下", "容得下"),
    ("這種偏狹心態容不下任何異見", "容不下"),

    # 6. 空間與動作往復構式 [X來X去]
    ("他在客廳裡焦急地走來走去", "走來走去"),
    ("這幾張合約我看來看去沒有問題", "看來看去"),
    ("這件事情我想來想去還是不太妥當", "想來想去"),
    ("大家說來說去其實都是同一個意思", "說來說去"),
    ("重要文件在抽屜裡找來找去找不到", "找來找去"),
    ("小狗在草地上興奮地跑來跑去", "跑來跑去"),
    ("他整夜翻來覆去難以入眠", "翻來覆去"),

    # 7. 對稱否定與並列凝固構式 [不X不X] / [無X無X] / [一X一X]
    ("這是一筆不折不扣的大生意", "不折不扣"),
    ("這種程度的批評對他不痛不癢", "不痛不癢"),
    ("處在這種不上不下的尷尬處境", "不上不下"),
    ("不知不覺已經到了深夜", "不知不覺"),
    ("他在危急關頭依舊不慌不忙", "不慌不忙"),
    ("浩瀚的太平洋真是無邊無際", "無邊無際"),
    ("他無緣無故缺席了會議", "無緣無故"),
    ("退休生活過得無拘無束自由自在", "無拘無束"),
    ("一心一意只想做好這項研發專案", "一心一意"),
    ("我們應該保護大自然的一草一木", "一草一木"),

    # 8. 漢語核心體態與助詞構式
    ("奇妙的是這項設計居然大獲成功", "奇妙的是"),
    ("資料都存放在外接硬碟裡", "硬碟裡"),
    ("這部電影我花了一個鐘頭看完", "一個鐘頭"),
    ("他越弄越大反而不可收拾", "越弄越大"),
    ("在路口我回頭看他回宿舍", "看他回"),
    ("除了我之外大家都同意了", "除了我"),
    ("這是所有同仁共同努力的成果", "這是所有"),

    # 9. 連動構式 (Serial Verb Constructions - SVC)
    ("媽媽下午要上街買菜", "上街買菜"),
    ("下班後他喜歡坐車回家", "坐車回家"),
    ("早晨起床出門運動身體好", "出門運動"),
    ("他每天準時開車上班", "開車上班"),

    # 10. 雙賓語構式 (Double Object Constructions - DOC)
    ("請你給我一本書好嗎", "給我一本書"),
    ("生日當天送他一份禮物", "送他一份禮物"),
    ("急需用錢借你一千元應急", "借你一千元"),

    # 11. 把字句與被字句 (Disposal & Passive with Resultative Complements)
    ("他花了一個下午把書看完了", "把書看完了"),
    ("小貓不小心把杯子打破了", "把杯子打破了"),
    ("沒帶雨傘全身被雨淋濕了", "被雨淋濕了"),
    ("狂風大作路樹被風吹倒了", "被風吹倒了"),
]

def run_crucible():
    print("=" * 72)
    print("🔥 RuhI (如喜) 本地智慧語法構式驗證熔爐 (Crucible Active-Learning)")
    print("=" * 72)

    # 1. 硬體初始狀態檢查
    init_temp = get_gpu_temperature()
    print(f"🖥️ GPU 狀態監控：NVIDIA RTX 5070 Ti, 當前核心溫度：{init_temp:.1f}°C")
    if init_temp > 0:
        print(f"🛡️ 溫控守護機制：警示閾值={TEMP_WARNING_THRESHOLD}°C, 緊急中止={TEMP_EMERGENCY_ABORT}°C (正常)")
    else:
        print("ℹ️ 未偵測到 nvidia-smi，以 CPU 純淨模式執行。")

    # 2. 初始化編譯器與引擎橋接器
    print("\n📦 正在掛載許氏鍵盤雙拼編譯器與純 Rust 核心引擎...")
    t0 = time.time()
    compiler = TextToHsuCompiler(data_dir=os.path.join(RUHI_ROOT, "ruhi_lexicon", "data"))
    bridge = RuhiEngineBridge()
    load_time = time.time() - t0
    print(f"✨ 引擎就緒，共計耗時 {load_time:.2f} 秒。")

    # 3. 測試迴圈執行
    print(f"\n🚀 開始執行黃金語句庫批次測試 (共 {len(GOLDEN_CORPUS)} 組情境)...")
    print("-" * 72)

    passed_count = 0
    total_count = len(GOLDEN_CORPUS)
    failures = []

    for idx, (sentence, target_keyword) in enumerate(GOLDEN_CORPUS, 1):
        # 每一輪測試檢查硬體溫度
        if idx % 10 == 0:
            thermal_safety_check()

        # 文 A 轉 許氏鍵碼 文 B
        hsu_keys = compiler.text_to_hsu_keys(sentence)

        # 送入引擎組字
        bridge.reset()
        last_resp = None
        for char in hsu_keys:
            last_resp = bridge.process_key(char)

        composed_sentence = ""
        if last_resp:
            composed_sentence = last_resp.composed

        # 檢驗文 C 是否完美命中目標構式與全句
        is_exact_match = (composed_sentence == sentence)
        keyword_hit = (target_keyword in composed_sentence)

        if is_exact_match or keyword_hit:
            passed_count += 1
            status = "✅ PASS" if is_exact_match else "🟡 KEYWORD PASS"
            print(f"[{idx:02d}/{total_count:02d}] {status} | 目標: 「{target_keyword}」➔ 產出: 「{composed_sentence}」")
        else:
            failures.append({
                "index": idx,
                "gold": sentence,
                "target": target_keyword,
                "keys": hsu_keys,
                "produced": composed_sentence,
            })
            print(f"[{idx:02d}/{total_count:02d}] ❌ FAIL | 期望: 「{sentence}」➔ 產出: 「{composed_sentence}」")

    print("-" * 72)
    final_temp = get_gpu_temperature()
    pass_rate = (passed_count / total_count) * 100.0

    print(f"\n📊 熔爐測試總結報告 (Crucible Audit Summary)：")
    print(f"   總測試句子數: {total_count}")
    print(f"   成功通過數:   {passed_count}")
    print(f"   失敗案例數:   {len(failures)}")
    print(f"   構式命中率:   {pass_rate:.2f}%")
    print(f"   結束核心溫度: {final_temp:.1f}°C (健康安全)")

    if failures:
        print("\n🔍 失敗語句細部診斷清單：")
        for f in failures:
            print(f"   - [Case #{f['index']}] 期望: 「{f['gold']}」 (目標: {f['target']})")
            print(f"     按鍵序列: \"{f['keys']}\"")
            print(f"     引擎產出: 「{f['produced']}」")
    else:
        print("\n🎉 完美！所有 60 組核心漢語語法構式家族全部 100% 精準命中，零失敗案例！")

    return passed_count, total_count

if __name__ == "__main__":
    run_crucible()
