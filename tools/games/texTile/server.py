#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
texTile (詞布達意) 本地 AI 鑑賞與雙向賦能伺服器
Port: 8088
職責：
1. 提供 texTile 靜態網頁服務
2. 題目讀取 (/api/challenges)
3. 本地 Gemma AI 鑑賞評分 (/api/evaluate)，含 80 分品質防火牆
4. 雙向賦能提交通道 (/api/commit)：
   - 寫入 RuhI 使用者轉移矩陣 (transitions_user.tsv)
   - 寫入 WordChain 金句典藏庫 (golden_quotes.json)
"""

import http.server
import json
import os
import re
import socketserver
import sys
import urllib.request
import urllib.parse
from datetime import datetime

sys.stdout.reconfigure(encoding='utf-8')

PORT = 8088
TEXTILE_DIR = os.path.dirname(os.path.abspath(__file__))
CHALLENGES_PATH = os.path.join(TEXTILE_DIR, "challenges.json")
RUHI_DIR = os.path.abspath(os.path.join(TEXTILE_DIR, "..", ".."))
RUHI_TRANSITIONS_PATH = os.path.join(RUHI_DIR, "ruhi_lexicon", "data", "transitions_user.tsv")
WORDCHAIN_GOLDEN_PATH = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain/golden_quotes.json")
WORDCHAIN_SUPP_PATH = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain/supplementary_lexicon.json")

# ==============================================================================
# Gemma 鑑賞 Prompt 設計
# ==============================================================================
GEMMA_TEXTILE_PROMPT = """你是一位精通中文語意學、修辭學與當代文學的「文心語意鑑賞大師」。
玩家正在進行一場名為《texTile 詞布達意》的文字織布遊戲。
玩家的挑戰是：將題目給定的一組容易混淆的「同音/近音詞塊（Tiles）」同時、自然且巧妙地編織入同一個句子或小段落中，達成「化詞不達意為詞布達意」的奇趣與消歧效果。

【評分三要素】：
1. 語法通順度 (Fluency, 0-40分)：文脈是否自然通達，有無生硬拼湊之感。
2. 意境與奇趣 (Wit & Semantics, 0-35分)：是否展現出語意反差、文人雅趣、幽默感或深厚情境。
3. 消歧上下文合理性 (Disambiguation, 0-25分)：上下文是否給出足夠強烈的語境線索，使讀者或輸入法能明確分辨各個詞塊的真正意義。

【輸出標準 JSON 格式】：
{
  "score": 88,
  "passed": true,
  "verdict": "錦心繡口",
  "critique": "以古典文墨與現代鍵盤俠並置，既消解了筆舌墨的生澀，又諷喻了現代競逐，上下文消歧脈絡極其清晰。",
  "pithy_quote": "古人以筆舌墨開拓天下，現代鍵盤俠又在比什麼鍵程？"
}
請嚴格輸出 JSON 格式，勿附帶 Markdown 語法或多餘文字。"""

def evaluate_with_llm(sentence, tiles, challenge_title=""):
    """調用 Ollama Gemma 或 Google API，若無則降級為啟發式鑑賞"""
    prompt = f"【題目】：{challenge_title}\n【同音詞塊】：{', '.join(tiles)}\n【玩家織成的文句】：\n「{sentence}」\n請依三要素進行鑑賞評分。"
    
    # 1. 嘗試 Ollama (gemma2/gemma3)
    try:
        url = "http://127.0.0.1:11434/api/generate"
        payload = {
            "model": "gemma2:9b",
            "system": GEMMA_TEXTILE_PROMPT,
            "prompt": prompt,
            "stream": False,
            "format": "json",
            "options": {"temperature": 0.3}
        }
        req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(req, timeout=8) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            res_text = data.get('response', '')
            res_json = json.loads(res_text)
            res_json['passed'] = res_json.get('score', 0) >= 80
            return res_json
    except Exception:
        pass

    # 2. 嘗試 Google Gemini API (若有金鑰)
    api_key = os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
    if api_key:
        try:
            url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={api_key}"
            payload = {
                "contents": [{"role": "user", "parts": [{"text": GEMMA_TEXTILE_PROMPT + "\n\n" + prompt}]}],
                "generationConfig": {"temperature": 0.3, "responseMimeType": "application/json"}
            }
            req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers={'Content-Type': 'application/json'})
            with urllib.request.urlopen(req, timeout=12) as resp:
                data = json.loads(resp.read().decode('utf-8'))
                res_text = data['candidates'][0]['content']['parts'][0]['text']
                res_json = json.loads(res_text)
                res_json['passed'] = res_json.get('score', 0) >= 80
                return res_json
        except Exception:
            pass

    # 3. 離線啟發式文心鑑賞引擎 (Heuristic Fallback)
    clean_s = sentence.strip()
    has_all_tiles = all(t in clean_s for t in tiles)
    length = len(clean_s)
    
    score = 70
    if has_all_tiles:
        score += 15
    if 15 <= length <= 60:
        score += 8
    elif length > 60:
        score += 4
    
    # 檢查是否含有典雅或鮮明的修辭銜接詞
    wit_markers = ['以', '而', '不', '又', '豈', '縱', '若', '天下', '古人', '今日', '鍵盤', '凡', '何須', '何必', '？', '！']
    found_markers = [m for m in wit_markers if m in clean_s]
    score += min(7, len(found_markers) * 2)

    passed = score >= 80
    verdict = "錦心繡口" if score >= 85 else ("通達暢達" if passed else "尚待推敲")
    critique = f"文句成功將【{'】與【'.join(tiles)}】融於一爐，文氣流暢，語境脈絡足供消歧。" if passed else "詞塊雖皆已入列，但文脈銜接略顯生硬，建議再多推敲前後語境。"

    return {
        "score": min(98, score),
        "passed": passed,
        "verdict": verdict,
        "critique": critique,
        "pithy_quote": clean_s
    }

def extract_disambiguation_transitions(sentence, tiles):
    """
    從合成的句子中萃取每個詞塊的前後 Bigram/Trigram，作為如喜輸入法的高權重消歧突觸
    """
    clean_s = re.sub(r'[，。！？；：「」『』、\s]+', ' ', sentence.strip())
    segments = clean_s.split(' ')
    
    transitions = []
    today_str = datetime.now().strftime("%Y-%m-%d")

    for seg in segments:
        for tile in tiles:
            # 1. 內部切分轉移 (長度 >= 3 的詞塊，如 筆舌 -> 墨)
            if len(tile) >= 3:
                mid = len(tile) // 2
                transitions.append({
                    "prev": tile[:mid],
                    "curr": tile[mid:],
                    "weight": 35.0,
                    "type": "textile_internal",
                    "date": today_str
                })
            
            if tile in seg:
                idx = seg.find(tile)
                # 前驅詞 (Preceding word)
                if idx > 0:
                    prefix = seg[max(0, idx - 2):idx]
                    if len(prefix) >= 1:
                        transitions.append({
                            "prev": prefix,
                            "curr": tile,
                            "weight": 25.0,
                            "type": "textile_pre_context",
                            "date": today_str
                        })
                # 後續詞 (Succeeding word)
                after_idx = idx + len(tile)
                if after_idx < len(seg):
                    suffix = seg[after_idx:min(len(seg), after_idx + 2)]
                    if len(suffix) >= 1:
                        transitions.append({
                            "prev": tile,
                            "curr": suffix,
                            "weight": 25.0,
                            "type": "textile_post_context",
                            "date": today_str
                        })
    return transitions

# ==============================================================================
# HTTP Request Handler
# ==============================================================================
class TexTileHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=TEXTILE_DIR, **kwargs)

    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        super().end_headers()

    def do_OPTIONS(self):
        self.send_response(200)
        self.end_headers()

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/challenges":
            self.send_response(200)
            self.send_header('Content-Type', 'application/json; charset=utf-8')
            self.end_headers()
            if os.path.exists(CHALLENGES_PATH):
                with open(CHALLENGES_PATH, "r", encoding="utf-8") as f:
                    self.wfile.write(f.read().encode("utf-8"))
            else:
                self.wfile.write(b"[]")
            return
        
        # 預設交給靜態檔案伺服器
        return super().do_GET()

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        content_len = int(self.headers.get('Content-Length', 0))
        post_data = self.rfile.read(content_len).decode('utf-8')
        payload = json.loads(post_data) if post_data else {}

        if parsed.path == "/api/evaluate":
            sentence = payload.get("sentence", "")
            tiles = payload.get("tiles", [])
            title = payload.get("title", "")
            
            result = evaluate_with_llm(sentence, tiles, title)
            transitions = extract_disambiguation_transitions(sentence, tiles)
            result["transitions"] = transitions
            
            self.send_response(200)
            self.send_header('Content-Type', 'application/json; charset=utf-8')
            self.end_headers()
            self.wfile.write(json.dumps(result, ensure_ascii=False).encode('utf-8'))
            return

        elif parsed.path == "/api/commit":
            # 提交審核通過的句子並賦能 RuhI 與 WordChain
            score = payload.get("score", 0)
            sentence = payload.get("sentence", "")
            tiles = payload.get("tiles", [])
            critique = payload.get("critique", "")
            pithy_quote = payload.get("pithy_quote", sentence)
            transitions = payload.get("transitions", [])

            if score < 80:
                self.send_response(400)
                self.send_header('Content-Type', 'application/json; charset=utf-8')
                self.end_headers()
                self.wfile.write(json.dumps({
                    "error": "未達 80 分門檻，啟動防污染保護機制，暫不寫入輸入法神經矩陣。"
                }, ensure_ascii=False).encode('utf-8'))
                return

            # 1. 寫入 RuhI transitions_user.tsv
            added_transitions_count = 0
            if os.path.exists(RUHI_TRANSITIONS_PATH) and transitions:
                existing_lines = []
                with open(RUHI_TRANSITIONS_PATH, "r", encoding="utf-8") as f:
                    existing_lines = [line.strip() for line in f if line.strip()]
                
                existing_pairs = set()
                for line in existing_lines:
                    if not line.startswith("#"):
                        parts = line.split("\t")
                        if len(parts) >= 2:
                            existing_pairs.add((parts[0], parts[1]))
                
                new_tsv_rows = []
                for tr in transitions:
                    pair = (tr['prev'], tr['curr'])
                    if pair not in existing_pairs:
                        new_tsv_rows.append(f"{tr['prev']}\t{tr['curr']}\t{tr['weight']}\t{tr['type']}\t{tr['date']}")
                        existing_pairs.add(pair)
                        added_transitions_count += 1
                
                if new_tsv_rows:
                    with open(RUHI_TRANSITIONS_PATH, "a", encoding="utf-8") as f:
                        f.write("\n" + "\n".join(new_tsv_rows) + "\n")

            # 2. 寫入 WordChain golden_quotes.json
            golden_list = []
            if os.path.exists(WORDCHAIN_GOLDEN_PATH):
                try:
                    with open(WORDCHAIN_GOLDEN_PATH, "r", encoding="utf-8") as f:
                        golden_list = json.load(f)
                except Exception:
                    golden_list = []
            
            golden_entry = {
                "quote": pithy_quote,
                "full_sentence": sentence,
                "tiles": tiles,
                "score": score,
                "critique": critique,
                "author": "大十 (texTile 織句大師)",
                "created_at": datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            }
            # 避免重複
            if not any(g.get("quote") == pithy_quote for g in golden_list):
                golden_list.append(golden_entry)
                with open(WORDCHAIN_GOLDEN_PATH, "w", encoding="utf-8") as f:
                    json.dump(golden_list, f, ensure_ascii=False, indent=2)

            self.send_response(200)
            self.send_header('Content-Type', 'application/json; charset=utf-8')
            self.end_headers()
            self.wfile.write(json.dumps({
                "status": "success",
                "message": f"成功織入金句！RuhI 新增 {added_transitions_count} 組消歧轉移，WordChain 已典藏金句卡。",
                "added_transitions": added_transitions_count
            }, ensure_ascii=False).encode('utf-8'))
            return

        self.send_response(404)
        self.end_headers()

def run_server():
    with socketserver.TCPServer(("", PORT), TexTileHandler) as httpd:
        print(f"================================================================")
        print(f" 🧶 texTile (詞布達意) 本地伺服器已啟動！")
        print(f" 🌐 瀏覽器開啟位址: http://localhost:{PORT}")
        print(f" 🛡️  品質防火牆: 評分 >= 80 分自動雙向賦能 RuhI 與 WordChain")
        print(f"================================================================")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\n伺服器安全關閉。")

if __name__ == "__main__":
    run_server()
