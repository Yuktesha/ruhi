#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
字戀 (WordChain) ⇄ 如喜 (RuhI) - Gemma 語意除魅與自動分析管線
模組職責：
1. 詞目品質與合法性審查 (Legitimacy & Anti-Junk Audit)
2. 同音詞消歧、權重估算與轉移矩陣推薦 (Homophone & Transitions Optimization)
3. 字戀故事卡 (典故/出處/釋義/例句) 與聲韻標籤自動生成
4. 支援本機 Ollama (gemma2/gemma3)、Google GenAI API 與離線啟發式推理三種模式
"""

import argparse
import json
import math
import os
import re
import sys
import urllib.request
import urllib.error
import pypinyin

sys.stdout.reconfigure(encoding='utf-8')

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
RUHI_ROOT = os.path.abspath(os.path.join(BASE_DIR, ".."))
WORDCHAIN_DIR = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain")
SUPPLEMENTARY_PATH = os.path.join(WORDCHAIN_DIR, "supplementary_lexicon.json")
REJECTED_PATH = os.path.join(WORDCHAIN_DIR, "rejected_words.json")

# ==============================================================================
# 1. Gemma 專家系統 Prompt 設計
# ==============================================================================

GEMMA_AUDIT_SYSTEM_PROMPT = """你是一位深諳中華古典詩詞、典籍文史、臺灣在地國語與現代語意學的「教育部國語文學院士暨 AI 語意分析師」。
你的職責是同時為兩套系統審核語料：
1. 《字戀 (WordChain)》：臺灣正體漢字聲韻接龍博弈遊戲（重視成語、詩詞、熟語出度與故事卡）。
2. 《如喜 (RuhI)》：許氏雙拼極速輸入法（重視日常高頻口語、現代熟語、同音字詞消歧與語意流暢）。

請逐一審查傳入的詞彙或短語，並輸出標準 JSON 物件：
【審查標準】：
- "idiom": 四字古典成語或文典短語（如「海闊天空」、「得來全不費工夫」）。
- "saying": 民間諺語、俗語、歇後語、行話熟語（如「起手無回」、「吃葡萄不吐葡萄皮」）。
- "modern": 現代高頻活語言、定型口語、流行手勢短語（如「先這樣」、「比倒讚」、「騎車兜風」）。
- "reject": 機械碎片、小說雜物碎詞（如「打前劉海」）、斷句殘缺或無意義組合。

【JSON 輸出格式】：
{
  "word": "詞目",
  "verdict": "APPROVE" | "REJECT",
  "category": "idiom" | "saying" | "modern" | "reject",
  "origin": "典故出處或語源 (如：古典文獻、民間諺語、現代口語、相聲繞口令)",
  "def": "優雅精確的釋義與文化意涵說明",
  "usage": "典雅或生活化的標準例句",
  "ruhi_weight": 950,
  "transitions": [
    ["前驅詞", "當前詞", 30.0]
  ],
  "reason": "審核判定之核心依據"
}
"""

# ==============================================================================
# 2. 多重推理後端介面 (Ollama / Gemini API / 離線啟發式引擎)
# ==============================================================================

def get_pypinyin_bp_and_py(word):
    """取得標準注音與漢語拼音"""
    bps = pypinyin.pinyin(word, style=pypinyin.BOPOMOFO)
    bp_str = " ".join([b[0] for b in bps])
    pys = pypinyin.pinyin(word, style=pypinyin.TONE)
    py_str = " ".join([p[0] for p in pys])
    return bp_str, py_str

def call_ollama(prompt, model='gemma2:9b', host='http://127.0.0.1:11434'):
    """調用本機 Ollama 上的 Gemma 模型"""
    url = f"{host}/api/generate"
    payload = {
        "model": model,
        "system": GEMMA_AUDIT_SYSTEM_PROMPT,
        "prompt": prompt,
        "stream": False,
        "format": "json",
        "options": {"temperature": 0.2}
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode('utf-8'),
        headers={'Content-Type': 'application/json'}
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        result = json.loads(resp.read().decode('utf-8'))
        return result.get('response', '')

def call_gemini_api(prompt, api_key=None, model='gemini-2.0-flash'):
    """調用 Google Gemini API"""
    key = api_key or os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
    if not key:
        raise ValueError("未設置 GEMINI_API_KEY 或 GOOGLE_API_KEY")
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={key}"
    payload = {
        "contents": [
            {
                "role": "user",
                "parts": [{"text": GEMMA_AUDIT_SYSTEM_PROMPT + "\n\n" + prompt}]
            }
        ],
        "generationConfig": {
            "temperature": 0.2,
            "responseMimeType": "application/json"
        }
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode('utf-8'),
        headers={'Content-Type': 'application/json'}
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        result = json.loads(resp.read().decode('utf-8'))
        cands = result.get('candidates', [])
        if cands:
            return cands[0].get('content', {}).get('parts', [{}])[0].get('text', '')
    return ''

def heuristic_semantic_analysis(word, def_text="", quote=""):
    """
    離線啟發式 AI 語意分析引擎：
    當無本機 Ollama 或無 API Key 時，依據教育部辭典特徵、句法結構與長度進行智慧除魅分析
    """
    clean_w = word.strip()
    bp_str, py_str = get_pypinyin_bp_and_py(clean_w)
    
    # 1. 判定剔除黑名單特徵 (Junk & Fragmentation)
    bad_fragments = ['的地下', '的頭上', '的身上', '打前劉海', '的裡頭']
    if any(b in clean_w for b in bad_fragments):
        return {
            "word": clean_w,
            "verdict": "REJECT",
            "category": "reject",
            "reason": "小說瑣碎碎詞或語意殘缺片段"
        }

    # 2. 成語判定
    if len(clean_w) == 4 and any(kw in def_text for kw in ['比喻', '形容', '語本', '典出', '成語']):
        cat = "idiom"
        origin = "古典文獻成語"
        weight = 980
    # 3. 俗語 / 繞口令 / 諺語判定
    elif any(kw in def_text for kw in ['（諺語）', '（俗語）', '（歇後語）']) or '繞口令' in def_text or len(clean_w) >= 6:
        cat = "saying"
        origin = "民間俗語或熟語"
        weight = 980
    # 4. 現代定型口語
    else:
        cat = "modern"
        origin = "現代通俗口語"
        weight = 950

    meaning = def_text.strip() if def_text else f"指「{clean_w}」，為文脈中結構定型之熟語短語。"
    usage_ex = quote.strip() if quote else f"「{clean_w}」"

    # 生成轉移關係
    transitions = []
    if len(clean_w) >= 3:
        mid = len(clean_w) // 2
        transitions.append([clean_w[:mid], clean_w[mid:], 30.0])

    return {
        "word": clean_w,
        "bopomofo": bp_str,
        "pinyin": py_str,
        "verdict": "APPROVE",
        "category": cat,
        "origin": origin,
        "def": meaning[:140],
        "usage": usage_ex[:90],
        "is_idiom": True,
        "ruhi_weight": weight,
        "transitions": transitions,
        "reason": "啟發式語意分析審查合格"
    }

# ==============================================================================
# 3. 主分析流程
# ==============================================================================

class GemmaSemanticPipeline:
    def __init__(self, backend='auto'):
        self.backend = backend
        self.ollama_available = False
        self.gemini_available = bool(os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY"))
        self._check_ollama()

    def _check_ollama(self):
        try:
            req = urllib.request.Request("http://127.0.0.1:11434/api/tags", headers={'User-Agent': 'RuhI-Gemma'})
            with urllib.request.urlopen(req, timeout=1) as resp:
                if resp.status == 200:
                    self.ollama_available = True
        except Exception:
            self.ollama_available = False

    def audit_single_word(self, word, def_text="", quote=""):
        """對單一詞目執行語意除魅分析"""
        prompt = f"請審核候選詞目：【{word}】\n教育部釋義：{def_text}\n典籍引文：{quote}\n請給出 JSON 評定。"
        
        # 優先嘗試 Ollama
        if (self.backend in ['auto', 'ollama']) and self.ollama_available:
            try:
                resp = call_ollama(prompt)
                parsed = json.loads(resp)
                if isinstance(parsed, list) and len(parsed) > 0:
                    parsed = parsed[0]
                bp_str, py_str = get_pypinyin_bp_and_py(word)
                parsed['bopomofo'] = bp_str
                parsed['pinyin'] = py_str
                parsed['is_idiom'] = True
                return parsed
            except Exception as e:
                print(f"    [!] Ollama 推理異常: {e}，切換備用管線...")

        # 次選 Gemini API
        if (self.backend in ['auto', 'gemini']) and self.gemini_available:
            try:
                resp = call_gemini_api(prompt)
                parsed = json.loads(resp)
                if isinstance(parsed, list) and len(parsed) > 0:
                    parsed = parsed[0]
                bp_str, py_str = get_pypinyin_bp_and_py(word)
                parsed['bopomofo'] = bp_str
                parsed['pinyin'] = py_str
                parsed['is_idiom'] = True
                return parsed
            except Exception as e:
                print(f"    [!] Gemini API 推理異常: {e}，切換備用管線...")

        # 降級至智慧啟發式除魅引擎
        return heuristic_semantic_analysis(word, def_text, quote)

    def load_supplementary_lexicon(self):
        if os.path.exists(SUPPLEMENTARY_PATH):
            with open(SUPPLEMENTARY_PATH, "r", encoding="utf-8") as f:
                return json.load(f)
        return []

    def save_supplementary_lexicon(self, items):
        with open(SUPPLEMENTARY_PATH, "w", encoding="utf-8") as f:
            json.dump(items, f, ensure_ascii=False, indent=2)
        print(f">>> [OK] 已儲存字戀擴充詞庫: {SUPPLEMENTARY_PATH} (共 {len(items):,} 條)")

    def add_word_to_supplementary(self, word, category="modern", origin="現代通俗口語", def_text="", usage="", weight=950, transitions=None):
        """將審核通過的新詞加入字戀擴充庫"""
        items = self.load_supplementary_lexicon()
        bp_str, py_str = get_pypinyin_bp_and_py(word)
        
        # 檢查是否已存在
        for it in items:
            if it.get("word") == word:
                it["bopomofo"] = bp_str
                it["pinyin"] = py_str
                it["category"] = category
                it["origin"] = origin
                it["def"] = def_text or it.get("def", "")
                it["usage"] = usage or it.get("usage", f"「{word}」")
                it["ruhi_weight"] = weight
                it["is_idiom"] = True
                if transitions:
                    it["transitions"] = transitions
                self.save_supplementary_lexicon(items)
                return it

        new_entry = {
            "word": word,
            "bopomofo": bp_str,
            "pinyin": py_str,
            "category": category,
            "origin": origin,
            "def": def_text or f"指「{word}」，現代生活或文學定型化熟語。",
            "usage": usage or f"「{word}」",
            "is_idiom": True,
            "ruhi_weight": weight,
            "transitions": transitions or []
        }
        items.append(new_entry)
        self.save_supplementary_lexicon(items)
        return new_entry

# ==============================================================================
# 4. CLI 介面
# ==============================================================================

def main():
    parser = argparse.ArgumentParser(description="Gemma 語意除魅與字戀-如喜雙向同步管線")
    parser.add_argument("--word", type=str, help="欲審核或加入的新詞")
    parser.add_argument("--def-text", type=str, default="", help="詞彙釋義")
    parser.add_argument("--origin", type=str, default="民間熟語", help="詞彙出處或語源")
    parser.add_argument("--weight", type=int, default=950, help="如喜基底權重")
    parser.add_argument("--backend", type=str, default="auto", choices=["auto", "ollama", "gemini", "heuristic"])
    args = parser.parse_args()

    pipeline = GemmaSemanticPipeline(backend=args.backend)
    
    if args.word:
        print(f"\n>>> 正在執行 Gemma 語意審核: 【{args.word}】...")
        res = pipeline.audit_single_word(args.word, def_text=args.def_text)
        print(json.dumps(res, ensure_ascii=False, indent=2))
        
        if res.get("verdict") == "APPROVE":
            print(f">>> 審核合格，正在自動寫入字戀擴充庫...")
            pipeline.add_word_to_supplementary(
                word=args.word,
                category=res.get("category", "modern"),
                origin=res.get("origin", args.origin),
                def_text=res.get("def", ""),
                usage=res.get("usage", ""),
                weight=res.get("ruhi_weight", args.weight),
                transitions=res.get("transitions", [])
            )
    else:
        print("請指定 --word 參數進行審核，或調用 pipeline API。")

if __name__ == '__main__':
    main()
