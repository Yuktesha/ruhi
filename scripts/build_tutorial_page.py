# -*- coding: utf-8 -*-
"""
建置「如喜（RuhI）— 智慧型四角號碼互動教學動畫與整串文字查碼器」單一獨立 HTML 頁面
內嵌完整單字拆碼字典、高頻詞彙前兩角簡碼字典與候選序位反查表。
"""
import json
import sys
from collections import defaultdict
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

ROOT = Path(r"C:\ATGprjs\ruhi")
TSV_PATH = ROOT / "ruhi_lexicon" / "data" / "four_corner.tsv"
OUT_HTML = ROOT / "docs" / "four_corner_tutorial_anim.html"


def build_dictionary_payload():
    # char -> {"full": [...], "fuzzy": [...], "variant": [...], "ranks": {code: rank_1based}}
    char_info = defaultdict(lambda: {"full": [], "fuzzy": [], "variant": [], "ranks": {}})
    # word (len>=2) -> [{"code": c, "kind": k, "rank": r}]
    word_info = defaultdict(list)
    # code -> [(weight, text, kind)]
    code_buckets = defaultdict(list)

    with open(TSV_PATH, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "\t" not in line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            code, text, w_str, kind = parts[0], parts[1], parts[2], parts[3]
            try:
                w = int(w_str)
            except ValueError:
                w = 100
            code_buckets[code].append((w, text, kind))

    # 計算每個 code 下的候選排序 (1-based) — 單字 (XXXX) 與詞彙 (-XXXX / -XXXXX / -XXXXXX) 徹底分流！
    code_top_cands = {}
    for code, items in code_buckets.items():
        items.sort(key=lambda x: (-x[0], x[1]))
        seen = set()
        single_dedup = []
        word_dedup = []
        for w, t, k in items:
            if t not in seen:
                seen.add(t)
                if len(t) == 1:
                    single_dedup.append((t, k, w))
                elif 2 <= len(t) <= 4:
                    word_dedup.append((t, k, w))

        # 無 `-` 前導：單字優先（若該碼無單字則保底多字詞）
        if single_dedup:
            code_top_cands[code] = [[t, k] for t, k, _ in single_dedup[:30]]
        elif word_dedup:
            code_top_cands[code] = [[t, k] for t, k, _ in word_dedup[:30]]

        # 帶 `-` 詞前導：100% 純多字詞
        if word_dedup:
            code_top_cands["-" + code] = [[t, k] for t, k, _ in word_dedup[:30]]

        for idx, (t, k, w) in enumerate(single_dedup[:30]):
            rank = idx + 1
            info = char_info[t]
            if k == "full":
                if code not in info["full"]:
                    info["full"].append(code)
            elif k == "fuzzy":
                if code not in info["fuzzy"]:
                    info["fuzzy"].append(code)
            elif k == "variant":
                if code not in info["variant"]:
                    info["variant"].append(code)
            else:
                if code not in info["full"]:
                    info["full"].append(code)
            if code not in info["ranks"] or rank < info["ranks"][code]:
                info["ranks"][code] = rank

        for idx, (t, k, w) in enumerate(word_dedup[:30]):
            rank = idx + 1
            word_info[t].append({"c": "-" + code, "k": k, "r": rank, "w": w})

    # 精簡 char_db: { char: [full_list, fuzzy_list, variant_list, {code: rank}] }
    compact_chars = {}
    for ch, inf in char_info.items():
        # 優先讓 4 碼排在 5 碼前面，但保留 5 碼以顯示附角
        full_sorted = sorted(inf["full"], key=lambda x: (len(x), x))
        fuzzy_sorted = sorted(inf["fuzzy"], key=lambda x: (len(x), x))
        var_sorted = sorted(inf["variant"], key=lambda x: (len(x), x))
        compact_chars[ch] = [full_sorted, fuzzy_sorted, var_sorted, inf["ranks"]]

    # 精簡 word_db: { word: [[code, kind, rank], ...] }
    compact_words = {}
    for wd, lst in word_info.items():
        lst.sort(key=lambda x: (x["r"], len(x["c"]), -x["w"]))
        seen_c = set()
        clean_lst = []
        for item in lst:
            if item["c"] not in seen_c:
                seen_c.add(item["c"])
                clean_lst.append([item["c"], item["k"], item["r"]])
        if clean_lst:
            compact_words[wd] = clean_lst[:4]

    return compact_chars, compact_words, code_top_cands


HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="zh-TW">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>如喜（RuhI）— 智慧型四角號碼（4×5 小鍵盤）互動教學動畫與整串文字查碼器</title>
<style>
  :root {
    --bg-main: #0f141e;
    --bg-card: #192130;
    --bg-surface: #222c40;
    --bg-key: #28344b;
    --bg-key-spec: #34435e;
    --border: #3d4e6d;
    --text-bright: #f8fafc;
    --text-main: #cbd5e1;
    --text-muted: #94a3b8;
    --cyan: #38bdf8;
    --gold: #facc15;
    --green: #34d399;
    --pink: #f472b6;
    --purple: #a78bfa;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    background: var(--bg-main);
    color: var(--text-bright);
    font-family: "Microsoft JhengHei", "PingFang TC", "Noto Sans TC", sans-serif;
    line-height: 1.5;
    padding: 16px 22px;
  }
  .container {
    max-width: min(97vw, 2520px);
    margin: 0 auto;
  }

  /* 4K / 寬螢幕自適應雙主欄工作台 (R-UI/UX Split Workbench) */
  .workbench-layout {
    display: grid;
    grid-template-columns: 1fr;
    gap: 20px;
    align-items: start;
  }
  @media (min-width: 1520px) {
    body:not(.force-stack) .workbench-layout {
      grid-template-columns: minmax(520px, 1fr) minmax(880px, 1.15fr);
    }
    body:not(.force-stack) .stage-col {
      position: sticky;
      top: 14px;
    }
  }
  @media (min-width: 2100px) {
    body:not(.force-stack) .workbench-layout {
      grid-template-columns: minmax(680px, 1.05fr) minmax(1020px, 1.2fr);
      gap: 26px;
    }
    .char-cards-grid {
      grid-template-columns: repeat(auto-fill, minmax(255px, 1fr)) !important;
    }
  }
  body.force-split .workbench-layout {
    grid-template-columns: minmax(480px, 1fr) minmax(820px, 1.15fr);
  }
  body.force-split .stage-col {
    position: sticky;
    top: 14px;
  }

  /* 舞台聚光燈脈衝動畫（點擊字卡／詞塊時醒目提示） */
  @keyframes stagePulseGlow {
    0% { box-shadow: 0 0 0 0 rgba(250, 204, 21, 0.85), 0 0 32px rgba(56, 189, 248, 0.75); border-color: var(--gold); }
    60% { box-shadow: 0 0 0 10px rgba(250, 204, 21, 0), 0 0 24px rgba(56, 189, 248, 0.35); border-color: var(--cyan); }
    100% { box-shadow: none; border-color: var(--border); }
  }
  .stage-card.stage-flash {
    animation: stagePulseGlow 1.15s ease-out;
  }
  .combo-token.active-token {
    background: #065f46 !important;
    border: 2px solid var(--gold) !important;
    box-shadow: 0 0 16px rgba(250, 204, 21, 0.45);
    transform: translateY(-2px);
  }
  .char-detail-card.active-card {
    border: 2px solid var(--gold) !important;
    background: #1e3a5f !important;
    box-shadow: 0 0 18px rgba(250, 204, 21, 0.4);
  }
  .now-playing-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: rgba(250, 204, 21, 0.18);
    color: var(--gold);
    border: 1px solid var(--gold);
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 0.82rem;
    font-weight: 700;
  }
  .toolbar-box {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    background: #111827;
    padding: 6px 12px;
    border-radius: 10px;
    border: 1px solid var(--border);
  }
  .tb-btn {
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 3px 9px;
    font-size: 0.78rem;
    font-weight: 600;
    cursor: pointer;
  }
  .tb-btn:hover, .tb-btn.active {
    background: var(--cyan);
    color: #0f172a;
    border-color: var(--cyan);
  }
  header {
    background: linear-gradient(135deg, #17253b 0%, #1e293b 100%);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px 22px;
    margin-bottom: 16px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
  }
  header h1 {
    font-size: 1.42rem;
    color: var(--text-bright);
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .badge-four {
    background: var(--cyan);
    color: #0f172a;
    font-weight: 900;
    padding: 3px 10px;
    border-radius: 8px;
    font-size: 1.2rem;
  }
  header p { color: var(--cyan); font-size: 0.92rem; margin-top: 4px; }

  /* 🔍 整串文字／句子四角碼即時查詢器 */
  .lookup-panel {
    background: linear-gradient(180deg, #1b263b 0%, #151e2e 100%);
    border: 2px solid var(--cyan);
    border-radius: 14px;
    padding: 18px 20px;
    margin-bottom: 18px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35);
  }
  .lookup-top {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .lookup-title {
    font-size: 1.12rem;
    font-weight: 800;
    color: var(--gold);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .lookup-input-row {
    display: flex;
    gap: 10px;
    width: 100%;
    flex-wrap: wrap;
  }
  .lookup-input {
    flex: 1;
    min-width: 260px;
    background: #0f172a;
    color: var(--text-bright);
    border: 2px solid var(--border);
    border-radius: 10px;
    padding: 10px 16px;
    font-size: 1.15rem;
    font-weight: 700;
    outline: none;
    transition: border-color 0.2s;
  }
  .lookup-input:focus { border-color: var(--gold); }
  .preset-chips {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 8px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }
  .chip-btn {
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 11px;
    font-size: 0.82rem;
    cursor: pointer;
    transition: all 0.15s;
  }
  .chip-btn:hover { border-color: var(--cyan); color: #fff; background: #1e3a5f; }

  /* 整串最速連打攻略條 */
  .combo-banner {
    background: #111827;
    border: 1px solid var(--border);
    border-left: 4px solid var(--green);
    border-radius: 10px;
    padding: 12px 16px;
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .combo-tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }
  .combo-token {
    background: #1e293b;
    border: 1px solid var(--green);
    border-radius: 9px;
    padding: 6px 12px;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    transition: all 0.15s;
  }
  .combo-token:hover {
    background: #064e3b;
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(52, 211, 153, 0.25);
  }
  .ct-word { font-size: 1.15rem; font-weight: 800; color: var(--text-bright); }
  .ct-code {
    font-family: "Consolas", monospace;
    font-size: 1.05rem;
    font-weight: 800;
    color: var(--gold);
    background: #0f172a;
    padding: 2px 8px;
    border-radius: 6px;
  }
  .ct-keyhint { font-size: 0.8rem; color: var(--green); font-weight: 600; }

  /* 逐字四角拆解卡片網格 */
  .char-cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(235px, 1fr));
    gap: 12px;
    margin-top: 12px;
  }
  .char-detail-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 12px 14px;
    cursor: pointer;
    transition: all 0.18s;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .char-detail-card:hover, .char-detail-card.active-card {
    border-color: var(--gold);
    background: #1e2d44;
    transform: translateY(-2px);
    box-shadow: 0 4px 14px rgba(250, 204, 21, 0.2);
  }
  .cdc-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid rgba(148, 163, 184, 0.18);
    padding-bottom: 6px;
  }
  .cdc-char {
    font-size: 2rem;
    font-weight: 900;
    color: var(--text-bright);
    line-height: 1.1;
  }
  .cdc-maincode {
    font-family: "Consolas", monospace;
    font-size: 1.35rem;
    font-weight: 900;
    color: var(--gold);
    background: #0f172a;
    padding: 2px 10px;
    border-radius: 7px;
    border: 1px solid var(--border);
  }
  .cdc-corners {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 8px;
    font-size: 0.82rem;
  }
  .cdc-c-item {
    background: #131a27;
    padding: 3px 7px;
    border-radius: 5px;
    display: flex;
    justify-content: space-between;
  }
  .cdc-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    font-size: 0.78rem;
  }
  .cdc-badge {
    padding: 2px 7px;
    border-radius: 5px;
    font-weight: 600;
  }
  .b-fuzzy { background: rgba(52, 211, 153, 0.18); color: var(--green); border: 1px solid rgba(52, 211, 153, 0.4); }
  .b-var { background: rgba(244, 114, 182, 0.18); color: var(--pink); border: 1px solid rgba(244, 114, 182, 0.4); }
  .b-how { background: rgba(56, 189, 248, 0.18); color: var(--cyan); border: 1px solid rgba(56, 189, 248, 0.4); width: 100%; margin-top: 2px; }

  .lesson-tabs {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .tab-btn {
    background: var(--bg-card);
    color: var(--text-main);
    border: 1px solid var(--border);
    padding: 8px 14px;
    border-radius: 10px;
    cursor: pointer;
    font-size: 0.9rem;
    font-weight: 600;
    transition: all 0.2s;
  }
  .tab-btn:hover { border-color: var(--cyan); color: var(--text-bright); }
  .tab-btn.active {
    background: #0284c7;
    color: #fff;
    border-color: var(--gold);
    box-shadow: 0 0 12px rgba(56, 189, 248, 0.35);
  }
  .main-grid {
    display: grid;
    grid-template-columns: 1fr 388px;
    gap: 18px;
  }
  @media (max-width: 980px) { .main-grid { grid-template-columns: 1fr; } }
  .stage-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .stage-top {
    display: grid;
    grid-template-columns: 265px 1fr;
    gap: 16px;
    align-items: stretch;
  }
  @media (max-width: 720px) { .stage-top { grid-template-columns: 1fr; } }
  .char-canvas {
    position: relative;
    width: 256px;
    height: 256px;
    background: #131a27;
    border: 2px solid var(--border);
    border-radius: 14px;
    margin: 0 auto;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .char-canvas::before, .char-canvas::after {
    content: "";
    position: absolute;
    background: rgba(148, 163, 184, 0.14);
  }
  .char-canvas::before { width: 100%; height: 1px; top: 50%; left: 0; }
  .char-canvas::after { height: 100%; width: 1px; left: 50%; top: 0; }
  .big-char {
    font-size: 144px;
    font-weight: 800;
    color: var(--text-bright);
    user-select: none;
    z-index: 2;
    line-height: 1;
  }
  .phrase-pair { display: flex; gap: 8px; z-index: 2; }
  .phrase-pair span { font-size: 92px; font-weight: 800; padding: 4px; }
  .corner-spot {
    position: absolute;
    border-radius: 9px;
    padding: 4px 8px;
    font-size: 0.8rem;
    font-weight: 700;
    z-index: 5;
    transition: all 0.28s;
    opacity: 0.35;
    background: #1e293b;
    border: 1px solid var(--border);
  }
  .corner-spot.active {
    opacity: 1;
    transform: scale(1.06);
    color: #0f172a;
    box-shadow: 0 0 14px currentColor;
  }
  .spot-tl { top: 10px; left: 10px; }
  .spot-tr { top: 10px; right: 10px; }
  .spot-bl { bottom: 10px; left: 10px; }
  .spot-br { bottom: 10px; right: 10px; }
  .spot-tl.active { background: var(--cyan); border-color: #fff; }
  .spot-tr.active { background: var(--gold); border-color: #fff; }
  .spot-bl.active { background: var(--green); border-color: #fff; }
  .spot-br.active { background: var(--pink); border-color: #fff; }
  .highlight-zone {
    position: absolute;
    border: 3px dashed var(--gold);
    border-radius: 10px;
    background: rgba(56, 189, 248, 0.14);
    z-index: 3;
    transition: all 0.3s ease;
    pointer-events: none;
    opacity: 0;
  }
  .highlight-zone.show { opacity: 1; }
  .steps-panel {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    background: #141c2b;
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 14px 16px;
  }
  .step-list { display: flex; flex-direction: column; gap: 7px; }
  .step-item {
    padding: 8px 11px;
    border-radius: 8px;
    background: var(--bg-surface);
    border-left: 4px solid var(--border);
    font-size: 0.9rem;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.2s;
  }
  .step-item.done { color: var(--text-main); border-left-color: var(--cyan); }
  .step-item.current {
    background: #1e3a5f;
    color: var(--text-bright);
    border-left-color: var(--gold);
    font-weight: 700;
  }
  .playback-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .ctrl-btn {
    background: var(--bg-key);
    color: var(--text-bright);
    border: 1px solid var(--border);
    padding: 6px 12px;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
    font-size: 0.85rem;
  }
  .ctrl-btn:hover { border-color: var(--cyan); background: #334155; }
  .ctrl-btn.primary { background: var(--cyan); color: #0f172a; border-color: var(--cyan); }

  /* 模擬 TSF 候選字視窗 */
  .tsf-window {
    background: #131b29;
    border: 2px solid var(--cyan);
    border-radius: 12px;
    padding: 12px 16px;
  }
  .tsf-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    flex-wrap: wrap;
    gap: 8px;
  }
  .tsf-code-box { display: flex; align-items: center; gap: 10px; }
  .tsf-code {
    font-family: "Consolas", monospace;
    font-size: 1.3rem;
    font-weight: 800;
    color: var(--gold);
    letter-spacing: 2px;
    background: #1e293b;
    padding: 2px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
  }
  .tsf-status { color: var(--green); font-size: 0.88rem; font-weight: 600; }
  .tsf-candidates { display: flex; gap: 8px; flex-wrap: wrap; }
  .cand-item {
    background: var(--bg-key);
    border: 1px solid var(--border);
    padding: 5px 12px;
    border-radius: 8px;
    font-size: 1rem;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .cand-item.selected {
    background: #0369a1;
    border: 2px solid var(--gold);
    font-weight: 700;
  }
  .cand-tag {
    font-size: 0.74rem;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(15, 23, 42, 0.6);
    color: var(--gold);
  }

  /* 4x5 實體小鍵盤 */
  .numpad-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
  }
  .numpad-title {
    font-size: 1rem;
    font-weight: 700;
    color: var(--gold);
    margin-bottom: 10px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .numpad-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    grid-auto-rows: minmax(68px, auto);
    gap: 9px;
  }
  .np-key {
    background: var(--bg-key);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 4px;
    cursor: pointer;
    user-select: none;
    transition: all 0.12s;
    overflow: hidden;
  }
  .np-key:hover { border-color: var(--cyan); background: #334155; }
  .np-key:active, .np-key.pressing {
    background: var(--cyan) !important;
    color: #0f172a !important;
    border-color: var(--gold) !important;
    transform: scale(0.95);
    box-shadow: 0 0 18px rgba(56, 189, 248, 0.75);
  }
  .np-key.pressing .k-main, .np-key.pressing .k-sub { color: #0f172a !important; }
  .np-key.trail { border-color: var(--cyan); background: #1e3a5f; }
  .np-key.spec { background: var(--bg-key-spec); }
  .np-key.tall { grid-row: span 2; }
  .np-key.wide { grid-column: span 2; }
  .k-main {
    font-size: 1.25rem;
    font-weight: 800;
    line-height: 1.15;
    color: var(--text-bright);
  }
  .k-sub {
    font-size: 0.78rem;
    line-height: 1.25;
    color: var(--text-muted);
    white-space: nowrap;
  }

  /* 0~9 筆形口訣互動卡片 */
  .rhyme-section {
    margin-top: 16px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px 18px;
  }
  .rhyme-grid {
    display: grid;
    grid-template-columns: repeat(10, 1fr);
    gap: 8px;
    margin-top: 10px;
  }
  @media (max-width: 900px) { .rhyme-grid { grid-template-columns: repeat(5, 1fr); } }
  .rhyme-box {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 7px;
    text-align: center;
    cursor: pointer;
  }
  .rhyme-box:hover { border-color: var(--gold); background: #1e3a5f; }
  .r-num { font-size: 1.25rem; font-weight: 900; color: var(--cyan); }
  .r-name { font-size: 0.88rem; font-weight: 700; color: var(--gold); }
  .r-shape { font-size: 0.8rem; color: var(--text-bright); }
  .r-ex { font-size: 0.72rem; color: var(--text-muted); }
</style>
</head>
<body>
<div class="container">
  <header>
    <div>
      <h1><span class="badge-four">四</span> 如喜（RuhI）— 智慧型四角號碼（4×5 小鍵盤）互動教學動畫與整串查碼器</h1>
      <p>內建完整 76,299 筆混合構詞碼表（單字 XXXX｜詞彙 -XXXX/-XXXXX/-XXXXXX）｜<strong>九宮格小鍵盤專責四角編碼與選單（/ 選單 · * 萬用 · . 標點 · - 詞前導 · + 分隔），主鍵盤區免切換直出一般英數字與符號！</strong></p>
    </div>
    <div style="display:flex; flex-direction:column; align-items:flex-end; gap:6px;">
      <div class="toolbar-box">
        <span style="font-size:0.78rem; color:var(--gold); font-weight:700;">🖥️ 4K 螢幕排版：</span>
        <button class="tb-btn active" id="layoutAutoBtn" onclick="setWorkbenchLayout('auto')" title="寬螢幕(≥1520px)自動左右並排，窄視窗上下展開">自動自適應</button>
        <button class="tb-btn" id="layoutSplitBtn" onclick="setWorkbenchLayout('split')" title="強制左右雙欄並排（左查字卡、右釘選動畫舞台）">左右雙欄並排</button>
        <button class="tb-btn" id="layoutStackBtn" onclick="setWorkbenchLayout('stack')" title="傳統上下垂直堆疊">上下垂直展開</button>
        <span style="color:var(--border);">|</span>
        <span style="font-size:0.78rem; color:var(--cyan); font-weight:700;">🔍 視窗縮放：</span>
        <button class="tb-btn active" id="zoom100Btn" onclick="setPageZoom(1)">100%</button>
        <button class="tb-btn" id="zoom115Btn" onclick="setPageZoom(1.15)">115%</button>
        <button class="tb-btn" id="zoom130Btn" onclick="setPageZoom(1.3)">130% (4K舒適)</button>
      </div>
      <div style="font-size:0.82rem; color:var(--text-muted);">
        胡適口訣：橫一垂二三點捺，叉四插五方塊六，七角八八九是小，點下有橫變零頭
      </div>
    </div>
  </header>

  <div class="workbench-layout">
    <!-- 左主欄：🔍 整串文字／句子四角碼即時查詢器與逐字字卡 -->
    <div class="lookup-col">
      <div class="lookup-panel" style="margin-bottom:0;">
        <div class="lookup-top" style="justify-content:space-between;">
          <div class="lookup-title">🔍 整串文字／單字四角碼即時查詢器（輸入任何字詞或整句話，點擊詞塊或字卡立即播放拆角動畫）</div>
          <span class="now-playing-pill" id="nowPlayingBadge">🎬 舞台播放中：單元 4</span>
        </div>
        <div class="lookup-input-row">
          <input type="text" id="lookupInput" class="lookup-input" value="智慧型四角號碼輸入法" placeholder="在此輸入或貼上想查詢的單字、詞彙或整串句子（例如：台灣、四角號碼、輸入法）..." oninput="analyzeFullText()" />
          <button class="ctrl-btn primary" onclick="playLookupSequenceAnim()" style="font-size:0.95rem; padding:10px 18px;">▶ 依序連播整句打字動畫</button>
        </div>
        <div class="preset-chips">
          <span>快速範例點擊：</span>
          <button class="chip-btn" onclick="setLookupText('智慧型四角號碼輸入法')">智慧型四角號碼輸入法 (含 4字6碼/3字5碼)</button>
          <button class="chip-btn" onclick="setLookupText('台灣的國家主權')">台灣的國家主權 (含「主權 0044」)</button>
          <button class="chip-btn" onclick="setLookupText('台灣')">台灣 (2字4碼 2332 + 單字灣拆解)</button>
          <button class="chip-btn" onclick="setLookupText('一舉兩得')">一舉兩得 (4字6碼 107164)</button>
          <button class="chip-btn" onclick="setLookupText('為什麼台灣人')">為什麼台灣人 (3字5碼 30203 + 26300)</button>
          <button class="chip-btn" onclick="setLookupText('台臺裡裏線綫為爲')">常見正異體字對照</button>
        </div>

        <!-- A. 整串最速連打推薦序列 -->
        <div class="combo-banner" id="comboBanner"></div>

        <!-- B. 逐字四角拆解詳情卡片 -->
        <div class="char-cards-grid" id="charCardsGrid"></div>
      </div>
    </div>

    <!-- 右主欄（寬螢幕 Sticky 常駐釘選）：單元頁籤 ＋ 動畫舞台 ＋ 4×5 小鍵盤 ＋ 0~9 筆形速查表 -->
    <div class="stage-col" id="stageAnchor">
      <!-- 單元切換頁籤 -->
      <div class="lesson-tabs" id="lessonTabs">
        <button class="tab-btn active" onclick="loadLesson(0)">單元 1：單字正統取碼（台 2360）</button>
        <button class="tab-btn" onclick="loadLesson(1)">單元 2：「灣」字怎麼拆？（灣 3212 / 直覺 3232）</button>
        <button class="tab-btn" onclick="loadLesson(2)">單元 3：「- 詞前導」二字詞 4 碼（台灣 -2332 / 天才 -1040）</button>
        <button class="tab-btn" onclick="loadLesson(3)">單元 4：三／四字詞「-首1,3[中1×n]尾2,4」（輸入法 -50843 / 四角號碼 -622612）</button>
        <button class="tab-btn" onclick="loadLesson(4)">單元 5：「- 詞前導 / + 字詞分隔 / . 標點選單 / * 萬用字元」</button>
        <button class="tab-btn" onclick="loadLesson(5)">單元 6：「/ 字詞選單：1~0 快選、+ 翻下一頁、- 翻上一頁」</button>
      </div>

      <div class="main-grid">
        <!-- 左側動畫舞台與模擬候選窗 -->
        <div class="stage-card" id="stageCard">
      <div class="stage-top">
        <div class="char-canvas" id="charCanvas">
          <div class="spot-tl corner-spot" id="spot0">①左上</div>
          <div class="spot-tr corner-spot" id="spot1">②右上</div>
          <div class="spot-bl corner-spot" id="spot2">③左下</div>
          <div class="spot-br corner-spot" id="spot3">④右下</div>
          <div class="highlight-zone" id="hlZone"></div>
          <div class="big-char" id="stageChar">台</div>
        </div>

        <div class="steps-panel">
          <div>
            <h3 id="lessonTitle" style="color:var(--gold); margin-bottom:8px; font-size:1.08rem;"></h3>
            <div class="step-list" id="stepList"></div>
          </div>
          <div class="playback-controls">
            <button class="ctrl-btn primary" id="playPauseBtn" onclick="togglePlay()">⏸ 暫停動畫</button>
            <button class="ctrl-btn" onclick="prevStep()">⏮ 上一步</button>
            <button class="ctrl-btn" onclick="nextStep()">⏭ 下一步</button>
            <button class="ctrl-btn" onclick="clearSandbox()">🧹 清空試打</button>
            <span style="font-size:0.8rem; color:var(--cyan); margin-left:auto;">
              💡 點擊上方任一字卡或右側小鍵盤即可互動！
            </span>
          </div>
        </div>
      </div>

      <!-- 模擬如喜 TSF 懸浮候選窗 -->
      <div class="tsf-window">
        <div class="tsf-header">
          <div class="tsf-code-box">
            <span class="badge-four" style="font-size:0.95rem; padding:2px 8px;">四</span>
            <span>四角碼：</span>
            <span class="tsf-code" id="tsfCode">2360</span>
          </div>
          <div class="tsf-status" id="tsfStatus">正統四角碼模式</div>
          <div style="display:flex; align-items:center; gap:6px;">
            <span id="tsfPageInfo" style="font-size:0.8rem; color:var(--cyan);">第 1/1 頁</span>
            <button class="ctrl-btn" style="padding:3px 8px; font-size:0.78rem;" onclick="pressVirtualKey('PageUp')" title="選單模式下按 - 或 PgUp 翻上一頁">◀ (-) 上頁</button>
            <button class="ctrl-btn" style="padding:3px 8px; font-size:0.78rem;" onclick="pressVirtualKey('PageDown')" title="選單模式下按 + 或 PgDn 翻下一頁">(+) 下頁 ▶</button>
          </div>
        </div>
        <div style="display:grid; grid-template-columns:1fr 1fr; gap:10px; margin-bottom:10px; background:#0f172a; padding:8px 12px; border-radius:8px; border:1px solid var(--border);">
          <div style="font-size:0.88rem; color:var(--text-main);">
            📝 如喜輸入座組句區（按 <kbd style="color:var(--cyan);">+</kbd> 分隔收攏 ／ <kbd style="color:var(--green);">-</kbd> 詞前導）：<strong id="composeDeskOut" style="color:var(--cyan); font-size:1.08rem;">（空白）</strong>
          </div>
          <div style="font-size:0.88rem; color:var(--text-main);">
            📤 目標視窗已送出（按 <kbd style="color:var(--gold);">Enter</kbd> 完稿上屏）：<strong id="committedOut" style="color:var(--gold); font-size:1.08rem;">（尚無）</strong>
          </div>
        </div>
        <div class="tsf-candidates" id="tsfCandidates"></div>
      </div>
    </div>

    <!-- 右側 4x5 實體數字小鍵盤（對應 18/19 鍵外接獨立九宮格與 104 鍵盤） -->
    <div class="numpad-card">
      <div class="numpad-title">
        <span>⌨️ 18/19 鍵外接九宮格 (點擊可試打)</span>
        <span style="font-size:0.75rem; color:var(--cyan);">/ 字詞選單 · * 萬用 · . 標點 · - 詞前導 · + 分隔</span>
      </div>
      <div class="numpad-grid">
        <div class="np-key spec" data-key="NumLock" title="保留原生 NumLock（104 鍵盤與外接 18/19 鍵非同步九宮格皆適用）">
          <span class="k-main">NumLk</span><span class="k-sub">原生數字鎖</span>
        </div>
        <div class="np-key spec" data-key="/" onclick="pressVirtualKey('/')" title="進入字詞選單：數字鍵 1~0 快選，+ 翻下一頁，- 翻上一頁">
          <span class="k-main">/</span><span class="k-sub">字詞選單</span>
        </div>
        <div class="np-key spec" data-key="*" onclick="pressVirtualKey('*')" title="萬用字元：不確定某角號碼時以 * 代替（如 23*0）">
          <span class="k-main">*</span><span class="k-sub">萬用字元</span>
        </div>
        <div class="np-key spec" data-key="-" onclick="pressVirtualKey('-')" title="一般狀態：詞前導符號 (-XXXX)；選單狀態：翻上一頁">
          <span class="k-main">-</span><span class="k-sub">詞前導 / 上頁</span>
        </div>

        <div class="np-key" data-key="7" onclick="pressVirtualKey('7')">
          <span class="k-main">7</span><span class="k-sub">角 ┏┐厂</span>
        </div>
        <div class="np-key" data-key="8" onclick="pressVirtualKey('8')">
          <span class="k-main">8</span><span class="k-sub">八 人入丷</span>
        </div>
        <div class="np-key" data-key="9" onclick="pressVirtualKey('9')">
          <span class="k-main">9</span><span class="k-sub">小 忄㣺业</span>
        </div>
        <div class="np-key spec" data-key="+" onclick="pressVirtualKey('+')" title="一般狀態：字詞分隔收攏；選單狀態：翻下一頁">
          <span class="k-main">+</span><span class="k-sub">字詞分隔 / 下頁</span>
        </div>

        <div class="np-key" data-key="4" onclick="pressVirtualKey('4')">
          <span class="k-main">4</span><span class="k-sub">叉 十艹乂</span>
        </div>
        <div class="np-key" data-key="5" onclick="pressVirtualKey('5')">
          <span class="k-main">5</span><span class="k-sub">插 丰扌戈</span>
        </div>
        <div class="np-key" data-key="6" onclick="pressVirtualKey('6')">
          <span class="k-main">6</span><span class="k-sub">方 口囗四</span>
        </div>
        <div class="np-key spec" data-key="Backspace" onclick="pressVirtualKey('Backspace')" title="外接 18/19 鍵九宮格專屬 BackSpace 退格鍵">
          <span class="k-main">⌫ Back</span><span class="k-sub">退格清除</span>
        </div>

        <div class="np-key" data-key="1" onclick="pressVirtualKey('1')">
          <span class="k-main">1</span><span class="k-sub">橫 一乚提</span>
        </div>
        <div class="np-key" data-key="2" onclick="pressVirtualKey('2')">
          <span class="k-main">2</span><span class="k-sub">垂 ｜丿亅</span>
        </div>
        <div class="np-key" data-key="3" onclick="pressVirtualKey('3')">
          <span class="k-main">3</span><span class="k-sub">點 丶捺氵</span>
        </div>
        <div class="np-key spec tall" data-key="Enter" onclick="pressVirtualKey('Enter')">
          <span class="k-main">Enter</span><span class="k-sub" style="white-space:normal;">整句送出<br>(上屏至視窗)</span>
        </div>

        <div class="np-key wide" data-key="0" onclick="pressVirtualKey('0')">
          <span class="k-main">0</span><span class="k-sub">頭 亠(言主广) ／ 複用歸零</span>
        </div>
        <div class="np-key spec" data-key="." onclick="pressVirtualKey('.')" title="標點符號選單前導：按 . 後以 1~0 選標點，+ 下頁 / - 上頁，連按 .. 直出逗號">
          <span class="k-main">.</span><span class="k-sub">標點選單</span>
        </div>
      </div>
    </div>
  </div>

  <!-- 下方 0~9 筆形口訣速查互動卡 -->
  <div class="rhyme-section">
    <div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap;">
      <strong style="color:var(--gold);">📐 0~9 筆形代碼速查表（點擊任一數字可立即輸入試打）</strong>
      <span style="font-size:0.84rem; color:var(--text-muted);">選字口訣：首選按【+ 分隔 / Enter 送出】；非首選按【/ 字詞選單】再按【1~0 快選（+ 下頁 / - 上頁）】；標點按【.】</span>
    </div>
    <div class="rhyme-grid">
      <div class="rhyme-box" onclick="pressVirtualKey('0')"><div class="r-num">0</div><div class="r-name">頭</div><div class="r-shape">亠 獨立點橫</div><div class="r-ex">言 主 广 方</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('1')"><div class="r-num">1</div><div class="r-name">橫</div><div class="r-shape">一 乚 平橫提</div><div class="r-ex">天 土 江 元</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('2')"><div class="r-num">2</div><div class="r-name">垂</div><div class="r-shape">｜ 丿 亅 豎撇</div><div class="r-ex">山 月 千 則</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('3')"><div class="r-num">3</div><div class="r-name">點</div><div class="r-shape">丶 捺 單點捺</div><div class="r-ex">水 汁 寶 之</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('4')"><div class="r-num">4</div><div class="r-name">叉</div><div class="r-shape">十 乂 艹 交叉</div><div class="r-ex">草 杏 皮 刈</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('5')"><div class="r-num">5</div><div class="r-name">插</div><div class="r-shape">丰 扌 穿兩筆</div><div class="r-ex">打 戈 申 史</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('6')"><div class="r-num">6</div><div class="r-name">方</div><div class="r-shape">口 囗 四方框</div><div class="r-ex">國 鳴 目 甲</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('7')"><div class="r-num">7</div><div class="r-name">角</div><div class="r-shape">┏ ┐ ┗ 折角</div><div class="r-ex">羽 門 陰 雪</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('8')"><div class="r-num">8</div><div class="r-name">八</div><div class="r-shape">八 人 入 丷</div><div class="r-ex">分 公 坐 羊</div></div>
      <div class="rhyme-box" onclick="pressVirtualKey('9')"><div class="r-num">9</div><div class="r-name">小</div><div class="r-shape">小 忄 㣺 尖小</div><div class="r-ex">尖 惟 慕 榮</div></div>
    </div>
  </div>
    </div><!-- /.stage-col -->
  </div><!-- /.workbench-layout -->
</div>

<script>
// 內嵌完整單字拆碼庫、詞彙簡碼庫與即時反查表
const CHAR_DB = __CHAR_DB_JSON__;
const WORD_DB = __WORD_DB_JSON__;
const CODE_CANDS = __CODE_CANDS_JSON__;

const STROKE_NAMES = {
  "0": "0頭(亠/複用)",
  "1": "1橫(一/提㇀)",
  "2": "2垂(｜/丿/亅)",
  "3": "3點(丶/捺)",
  "4": "4叉(十/乂/艹)",
  "5": "5插(丰/扌/戈)",
  "6": "6方(口/囗)",
  "7": "7角(┏/┐/折)",
  "8": "8八(八/人/丷)",
  "9": "9小(小/忄/业)"
};

let sequenceTokens = [];
let sequenceIdx = -1;

// 4K 螢幕排版與縮放控制
function setWorkbenchLayout(mode) {
  document.body.classList.remove("force-split", "force-stack");
  if (mode === "split") document.body.classList.add("force-split");
  if (mode === "stack") document.body.classList.add("force-stack");
  document.getElementById("layoutAutoBtn").classList.toggle("active", mode === "auto");
  document.getElementById("layoutSplitBtn").classList.toggle("active", mode === "split");
  document.getElementById("layoutStackBtn").classList.toggle("active", mode === "stack");
}

function setPageZoom(scale) {
  document.body.style.zoom = String(scale);
  document.getElementById("zoom100Btn").classList.toggle("active", scale === 1);
  document.getElementById("zoom115Btn").classList.toggle("active", scale === 1.15);
  document.getElementById("zoom130Btn").classList.toggle("active", scale === 1.3);
}

// 點擊字卡或詞塊時：智慧定位捲動至動畫舞台 ＋ 觸發聚光燈閃爍提示 ＋ 高亮來源卡片
function focusAnimationStage(label, opts = {}) {
  const badge = document.getElementById("nowPlayingBadge");
  if (badge) {
    badge.textContent = `🎬 舞台播放中：${label}`;
  }
  document.querySelectorAll(".char-detail-card").forEach((el, idx) => {
    const isHit = (opts.cardIdx !== undefined && opts.cardIdx === idx) ||
                  (opts.cardIdx === undefined && opts.charVal && el.getAttribute("data-char") === opts.charVal);
    el.classList.toggle("active-card", Boolean(isHit));
  });
  document.querySelectorAll(".combo-token").forEach((el, idx) => {
    const isHit = (opts.tokenIdx !== undefined && opts.tokenIdx === idx) ||
                  (opts.tokenIdx === undefined && opts.tokenText && el.getAttribute("data-token") === opts.tokenText);
    el.classList.toggle("active-token", Boolean(isHit));
  });

  const stageCard = document.getElementById("stageCard");
  if (!stageCard) return;

  // 若動畫舞台不在當前可視範圍（例如上下展開或長句查詢時），自動平滑捲動至舞台中央
  const rect = stageCard.getBoundingClientRect();
  const vh = window.innerHeight || document.documentElement.clientHeight;
  if (rect.top < 10 || rect.bottom > vh + 40) {
    stageCard.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  // 觸發聚光燈邊框閃爍動畫
  stageCard.classList.remove("stage-flash");
  void stageCard.offsetWidth;
  stageCard.classList.add("stage-flash");
}

function formatHowToSelect(rank) {
  if (!rank || rank === 1) {
    return "首選 #1：按 [+] 收攏至輸入座，或按 [Enter] 直接送出";
  } else if (rank <= 9) {
    return `第 #${rank} 順位：按 [/] 進入字詞選單 ➔ 按 [${rank}] 快選`;
  } else if (rank === 10) {
    return "第 #10 順位：按 [/] 進入字詞選單 ➔ 按 [0] 快選";
  } else {
    const pgFlips = Math.floor((rank - 1) / 10);
    const keyNum = rank % 10;
    return `第 #${rank} 順位（第 ${pgFlips + 1} 頁）：按 [/] 進入字詞選單 ➔ 按 [+] 翻下一頁 ${pgFlips} 次 ➔ 按 [${keyNum}] 快選`;
  }
}

// 依據「二字詞 -首1,2+尾1,2 (4碼) ／ 三字詞以上 -首1,3[中1×n]尾2,4 (N+2碼)」動態計算任意多字詞構詞碼
function computeDynamicPhraseCode(wordStr) {
  const chs = Array.from(wordStr).filter(c => !/\s/.test(c));
  if (chs.length < 2) return null;
  const c4List = [];
  for (const ch of chs) {
    const inf = CHAR_DB[ch];
    if (!inf) return null;
    const fulls = inf[0] || [];
    const fuzzys = inf[1] || [];
    const vars = inf[2] || [];
    const c4 = fulls.find(c => c.length >= 4) || fuzzys[0] || vars[0];
    if (!c4 || c4.length < 4) return null;
    c4List.push(c4.slice(0, 4));
  }
  if (chs.length === 2) {
    return {
      code: "-" + c4List[0].slice(0, 2) + c4List[1].slice(0, 2),
      formula: "二字詞（-前導 + 首1,2 + 尾1,2）"
    };
  }
  let mid = "";
  for (let k = 1; k < chs.length - 1; k++) {
    mid += c4List[k][0];
  }
  const first = c4List[0][0] + c4List[0][2];
  const last = c4List[chs.length - 1][1] + c4List[chs.length - 1][3];
  return {
    code: "-" + first + mid + last,
    formula: `${chs.length}字詞（-前導 + 首1,3 + 中1×${chs.length - 2} + 尾2,4）`
  };
}

// 貪婪最長詞優先切分（找出整串文字最省鍵的混合構詞連打方案）
function segmentBestCombo(text) {
  const chars = Array.from(text);
  const n = chars.length;
  const tokens = [];
  let i = 0;
  while (i < n) {
    const ch = chars[i];
    if (/\s/.test(ch)) { i++; continue; }
    let matched = null;
    for (let len = Math.min(4, n - i); len >= 2; len--) {
      const sub = chars.slice(i, i + len).join("");
      if (WORD_DB[sub] && WORD_DB[sub].length > 0) {
        const best = WORD_DB[sub][0]; // [code, kind, rank]
        matched = { text: sub, code: best[0], kind: best[1], rank: best[2], isWord: true };
        i += len;
        break;
      }
    }
    if (matched) {
      tokens.push(matched);
    } else {
      const inf = CHAR_DB[ch];
      if (inf) {
        const fulls = inf[0];
        const fuzzys = inf[1];
        const vars = inf[2];
        const ranks = inf[3] || {};
        // 挑選排名最靠前且最短的碼
        const allCodes = [...fulls, ...fuzzys, ...vars];
        let bestCode = fulls[0] || allCodes[0] || "????";
        let bestRank = ranks[bestCode] || 99;
        for (const c of allCodes) {
          const r = ranks[c] || 99;
          if (r < bestRank || (r === bestRank && c.length < bestCode.length)) {
            bestCode = c;
            bestRank = r;
          }
        }
        tokens.push({ text: ch, code: bestCode, kind: "single", rank: bestRank, isWord: false });
      } else {
        tokens.push({ text: ch, code: "（標點/符號）", kind: "punct", rank: 1, isWord: false });
      }
      i++;
    }
  }
  return tokens;
}

function setLookupText(str) {
  document.getElementById("lookupInput").value = str;
  analyzeFullText();
}

function analyzeFullText() {
  const raw = document.getElementById("lookupInput").value.trim();
  const banner = document.getElementById("comboBanner");
  const grid = document.getElementById("charCardsGrid");

  if (!raw) {
    banner.innerHTML = "<span style='color:var(--text-muted);'>請在上方輸入框輸入任意中文字詞或句子...</span>";
    grid.innerHTML = "";
    return;
  }

  // 1. 渲染整串最速連打推薦序列
  const tokens = segmentBestCombo(raw);
  const dynWhole = computeDynamicPhraseCode(raw);
  let dynHintHtml = "";
  if (dynWhole) {
    dynHintHtml = `<span style="font-size:0.82rem; color:var(--gold); background:#1e293b; padding:2px 10px; border-radius:6px; border:1px solid var(--border);">
      📐 整詞公式（${dynWhole.formula}）：<strong>${dynWhole.code}</strong>
    </span>`;
  }
  let bannerHtml = `<div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:8px;">
    <strong style="color:var(--green);">🚀 整串最速連打攻略（二字詞 1,2+1,2 四碼 ／ 三字詞以上 1,3[1×n]2,4 五～六碼，點擊詞塊看動畫）：</strong>
    <div style="display:flex; align-items:center; gap:10px; flex-wrap:wrap;">
      ${dynHintHtml}
      <span style="font-size:0.8rem; color:var(--text-muted);">共 ${tokens.length} 個輸入單元（詞間按 [+] 收攏、句尾按 [Enter] 送出）</span>
    </div>
  </div><div class="combo-tokens">`;

  tokens.forEach((tk, idx) => {
    const isLast = idx === tokens.length - 1;
    let rkHint;
    if (tk.rank === 1) {
      rkHint = isLast ? "[Enter]送出(或[+]分隔)" : "[+]字詞分隔";
    } else if (tk.rank <= 10) {
      rkHint = `[*]選單➔[${tk.rank === 10 ? 0 : tk.rank}]快選`;
    } else {
      const pgFlips = Math.floor((tk.rank - 1) / 10);
      const keyNum = tk.rank % 10;
      rkHint = `[*]選單➔[+下頁${pgFlips > 1 ? '×' + pgFlips : ''}]➔[${keyNum}]`;
    }
    let tagLbl = "單字碼(無-)";
    if (tk.isWord) {
      if (tk.text.length === 2) tagLbl = "二字詞(-首1,2+尾1,2)";
      else if (tk.text.length === 3) tagLbl = "三字詞(-首1,3+中1+尾2,4)";
      else if (tk.text.length >= 4) tagLbl = "四字詞(-首1,3+中1×2+尾2,4)";
      else tagLbl = "詞彙簡碼(-)";
    }
    const safeText = tk.text.replace(/'/g, "");
    bannerHtml += `<div class="combo-token" data-token="${safeText}" onclick="sequenceIdx=-1; animateCustomToken('${safeText}', '${tk.code}', ${tk.isWord}, ${tk.rank}, ${idx})">
      <span class="ct-word">${tk.text}</span>
      <span class="ct-code">${tk.code}</span>
      <span class="ct-keyhint">${tagLbl} · ${rkHint}</span>
    </div>`;
    // 若二字詞因同碼桶較擠（順位 > 3），但拆成兩個單字皆為 #1 首選時，並列提示「免選字盲打捷徑」
    if (tk.isWord && tk.text.length === 2 && tk.rank > 3) {
      const cA = tk.text[0], cB = tk.text[1];
      const infA = CHAR_DB[cA], infB = CHAR_DB[cB];
      if (infA && infB) {
        const codeA = (infA[0] || []).find(c => c.length === 4) || (infA[0] || [])[0];
        const codeB = (infB[0] || []).find(c => c.length === 4) || (infB[0] || [])[0];
        const rkA = codeA ? (infA[3] || {})[codeA] : 99;
        const rkB = codeB ? (infB[3] || {})[codeB] : 99;
        if (rkA === 1 && rkB === 1) {
          bannerHtml += `<div class="combo-token" style="border-color:var(--cyan); background:#172554;" onclick="sequenceIdx=-1; animateSingleCharCard('${cA}', 0)">
            <span class="ct-word" style="font-size:0.95rem; color:var(--cyan);">⚡ 免選字盲打：${cA} + ${cB}</span>
            <span class="ct-code">${codeA} ➔ [+] ➔ ${codeB}</span>
            <span class="ct-keyhint" style="color:var(--gold);">兩字皆 #1 首選（零選字免翻頁！）</span>
          </div>`;
        }
      }
    }
    if (idx < tokens.length - 1) {
      bannerHtml += `<span style="color:var(--text-muted); font-weight:bold;">➔</span>`;
    }
  });
  bannerHtml += `</div>`;
  banner.innerHTML = bannerHtml;

  // 2. 渲染逐字四角拆解詳情卡片
  const chars = Array.from(raw).filter(c => !/\s/.test(c));
  let cardsHtml = "";
  chars.forEach((ch, idx) => {
    const inf = CHAR_DB[ch];
    if (!inf) {
      cardsHtml += `<div class="char-detail-card">
        <div class="cdc-header"><span class="cdc-char">${ch}</span><span class="cdc-maincode">符號/其他</span></div>
        <div style="font-size:0.82rem; color:var(--text-muted);">可透過小鍵盤 [/] 標點選單輸入</div>
      </div>`;
      return;
    }
    const fulls = inf[0] || [];
    const fuzzys = inf[1] || [];
    const vars = inf[2] || [];
    const ranks = inf[3] || {};

    const primary4 = fulls.find(c => c.length === 4) || fulls[0] || fuzzys[0] || vars[0] || "0000";
    const primary5 = fulls.find(c => c.length === 5);
    const dispCode = primary5 ? `${primary5.slice(0,4)}.${primary5[4]}` : primary4;

    const c0 = STROKE_NAMES[primary4[0]] || primary4[0];
    const c1 = STROKE_NAMES[primary4[1]] || primary4[1];
    const c2 = STROKE_NAMES[primary4[2]] || primary4[2];
    const c3 = STROKE_NAMES[primary4[3]] || primary4[3];

    let badgesHtml = "";
    if (fuzzys.length > 0) {
      badgesHtml += `<span class="cdc-badge b-fuzzy">直覺寬鬆碼：${fuzzys.join(" / ")}</span>`;
    }
    if (vars.length > 0) {
      badgesHtml += `<span class="cdc-badge b-var">正異共享碼：${vars.join(" / ")}</span>`;
    }
    const rank4 = ranks[primary4];
    const rank5 = primary5 ? ranks[primary5] : null;
    let howStr = `打 <strong>${primary4}</strong>（${formatHowToSelect(rank4)}）`;
    if (primary5 && rank4 && rank4 > 3 && rank5 && rank5 <= 2) {
      howStr = `打 5 碼 <strong>${primary5}</strong> 直取 #${rank5}（或打 ${primary4} 在 #${rank4}）`;
    }
    badgesHtml += `<span class="cdc-badge b-how">⌨️ 單字打法：${howStr}</span>`;

    cardsHtml += `<div class="char-detail-card" data-char="${ch}" onclick="sequenceIdx=-1; animateSingleCharCard('${ch}', ${idx})">
      <div class="cdc-header">
        <div>
          <span class="cdc-char">${ch}</span>
          <span style="font-size:0.75rem; color:var(--cyan); margin-left:4px;">點擊看拆角動畫 ▶</span>
        </div>
        <span class="cdc-maincode" title="正統四角碼 (含第5碼附角)">${dispCode}</span>
      </div>
      <div class="cdc-corners">
        <div class="cdc-c-item"><span style="color:var(--cyan);">①左上</span><strong>${c0}</strong></div>
        <div class="cdc-c-item"><span style="color:var(--gold);">②右上</span><strong>${c1}</strong></div>
        <div class="cdc-c-item"><span style="color:var(--green);">③左下</span><strong>${c2}</strong></div>
        <div class="cdc-c-item"><span style="color:var(--pink);">④右下</span><strong>${c3}</strong></div>
      </div>
      <div class="cdc-badges">${badgesHtml}</div>
    </div>`;
  });
  grid.innerHTML = cardsHtml;
}

// 點擊單字卡：動態合成該單字的 4 角分解教學步驟並在舞台播放！
function animateSingleCharCard(ch, cardIdx) {
  const inf = CHAR_DB[ch];
  if (!inf) return;
  const fulls = inf[0] || [];
  const fuzzys = inf[1] || [];
  const vars = inf[2] || [];
  const ranks = inf[3] || {};
  const code4 = fulls.find(c => c.length === 4) || fulls[0] || fuzzys[0] || "0000";
  const code5 = fulls.find(c => c.length === 5);

  const cornerZones = [
    [35, 30, 95, 90],
    [130, 30, 95, 90],
    [35, 135, 95, 90],
    [130, 135, 95, 90]
  ];
  const cornerNames = ["① 左上角", "② 右上角", "③ 左下角", "④ 右下角"];
  const steps = [];

  for (let i = 0; i < 4; i++) {
    const partial = code4.slice(0, i + 1);
    const d = code4[i];
    const cands = CODE_CANDS[partial] || [[ch, "四角碼"]];
    steps.push({
      key: d,
      code: partial,
      activeSpots: Array.from({length: i + 1}, (_, k) => k),
      zone: cornerZones[i],
      desc: `${cornerNames[i]}：取【${STROKE_NAMES[d] || d}】➔ 輸入數字 ${d}`,
      status: `第 ${i + 1} 碼（${partial}）`,
      cands: cands,
      sel: 0
    });
  }

  const r4 = ranks[code4] || 1;
  if (r4 > 1 && code5 && (ranks[code5] || 99) < r4) {
    const d5 = code5[4];
    steps.push({
      key: d5,
      code: code5,
      activeSpots: [0,1,2,3],
      zone: [90, 145, 80, 75],
      desc: `⑤ 加打第 5 碼附角【${STROKE_NAMES[d5] || d5}】（${code5}）➔ 直接精準鎖定「${ch}」為第 #${ranks[code5]} 順位！`,
      status: `第 5 碼附角精準定位 (${code5})`,
      cands: CODE_CANDS[code5] || [[ch, "5碼附角"]],
      sel: 0,
      compose: ch
    });
  } else if (r4 > 1 && r4 <= 10) {
    const selKey = r4 === 10 ? "0" : String(r4);
    steps.push({
      key: "*",
      code: code4,
      activeSpots: [0,1,2,3],
      zone: null,
      desc: `選字步驟：「${ch}」位於第 #${r4} 順位 ➔ 先按小鍵盤【*】進入選字模式，再按【${selKey}】（或連按 * 跳選）即可收攏至輸入座！`,
      status: `按 [*] ➔ [${selKey}] 選取第 #${r4} 候選字`,
      cands: CODE_CANDS[code4] || [[ch, "候選"]],
      sel: r4 - 1,
      compose: ch
    });
  } else {
    steps.push({
      key: "+",
      code: code4,
      activeSpots: [0,1,2,3],
      zone: null,
      desc: `收攏與送出：「${ch}」已是第 #1 首選字 ➔ 按【+】收攏至輸入座繼續組句，或按【Enter】直接送出至目標視窗！`,
      status: `首選 #1 按 [+] 收攏組句 / 按 [Enter] 完稿送出`,
      cands: CODE_CANDS[code4] || [[ch, "首選"]],
      sel: 0,
      compose: ch,
      commit: ch
    });
  }

  if (fuzzys.length > 0) {
    const fz = fuzzys[0];
    steps.push({
      key: fz[3],
      code: fz,
      activeSpots: [0,1,2,3],
      zone: null,
      desc: `💡 初學者直覺容錯：這字也可以打寬鬆碼【${fz}】，免背筆畫複用／三點水提筆例外！`,
      status: `直覺寬鬆碼 ${fz} 同步支援`,
      cands: CODE_CANDS[fz] || [[ch, "寬鬆碼"]],
      sel: 0,
      compose: ch,
      commit: ch
    });
  }

  LESSONS[6] = {
    title: `🎯 自訂查詢單字拆解動畫：「${ch}」（正統碼 ${code5 ? code5.slice(0,4)+'.'+code5[4] : code4}）`,
    charHtml: ch,
    spots: [
      `①左上: ${code4[0]}`,
      `②右上: ${code4[1]}`,
      `③左下: ${code4[2]}`,
      `④右下: ${code4[3]}`
    ],
    steps: steps
  };
  curLesson = 6;
  curStep = 0;
  playing = true;
  document.getElementById("playPauseBtn").textContent = "⏸ 暫停動畫";
  renderLessonStep();
  focusAnimationStage(`單字「${ch}」(${code4})`, { cardIdx: cardIdx, charVal: ch });
}

function animateCustomToken(text, code, isWord, rank, tokenIdx) {
  if (!isWord && text.length === 1) {
    animateSingleCharCard(text);
    if (tokenIdx !== undefined) {
      focusAnimationStage(`單字「${text}」(${code})`, { tokenIdx: tokenIdx, charVal: text });
    }
    return;
  }
  const chars = Array.from(text);
  const steps = [];
  const isTwoChar = chars.length === 2;
  const rawDigits = code.startsWith("-") ? code.slice(1) : code;

  steps.push({
    key: "-",
    code: "-",
    activeSpots: [],
    zone: null,
    desc: `前導步驟：先按小鍵盤【-】進入「詞彙模式（-）」，徹底與單字 4 碼分流！`,
    status: `➖ 詞彙前導模式 (-)` ,
    cands: [[text, "詞彙模式"]],
    sel: 0
  });

  for (let i = 0; i < rawDigits.length; i++) {
    const partial = "-" + rawDigits.slice(0, i + 1);
    let stepDesc = "";
    if (isTwoChar) {
      const charIdx = Math.min(1, Math.floor(i / 2));
      const cornerNo = (i % 2) + 1;
      stepDesc = `第 ${i + 1} 碼（二字詞 1,2+1,2）：取「${chars[charIdx]}」第 ${cornerNo} 角（${cornerNo === 1 ? '左上①' : '右上②'}）【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
    } else {
      // 三字詞以上：首字(1,3) + [中間字(1)×n] + 尾字(2,4)
      if (i === 0) {
        stepDesc = `第 1 碼（首字 1,3 之第 1 角）：取首字「${chars[0]}」第 ① 左上角【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
      } else if (i === 1) {
        stepDesc = `第 2 碼（首字 1,3 之第 3 角）：取首字「${chars[0]}」第 ③ 左下角【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
      } else if (i < rawDigits.length - 2) {
        const midChar = chars[i - 1] || chars[1];
        stepDesc = `第 ${i + 1} 碼（中間字取第 1 角）：取中字「${midChar}」第 ① 左上角【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
      } else if (i === rawDigits.length - 2) {
        const tailChar = chars[chars.length - 1];
        stepDesc = `第 ${i + 1} 碼（尾字 2,4 之第 2 角）：取尾字「${tailChar}」第 ② 右上角【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
      } else {
        const tailChar = chars[chars.length - 1];
        stepDesc = `第 ${i + 1} 碼（尾字 2,4 之第 4 角）：取尾字「${tailChar}」第 ④ 右下角【${STROKE_NAMES[rawDigits[i]] || rawDigits[i]}】`;
      }
    }
    steps.push({
      key: rawDigits[i],
      code: partial,
      activeSpots: Array.from({length: Math.min(4, i + 1)}, (_, k) => k),
      zone: null,
      desc: stepDesc,
      status: isTwoChar ? `二字詞連打 (${partial})` : `${chars.length}字詞連打 (${partial})`,
      cands: CODE_CANDS[partial] || [[text, "詞彙簡碼"]],
      sel: 0
    });
  }
  let selDesc;
  if (rank <= 1) {
    selDesc = `✅ 滿碼後「${text}」即為第 #1 首選詞！按【+】分隔收攏至輸入座，或按【Enter】整句送出至視窗！`;
  } else if (rank <= 10) {
    selDesc = `✅ 滿碼後「${text}」位於第 #${rank} 順位：按【/】進入字詞選單 ➔ 按【${rank === 10 ? 0 : rank}】快選收攏至輸入座！`;
  } else {
    const pgFlips = Math.floor((rank - 1) / 10);
    const keyNum = rank % 10;
    selDesc = `✅ 滿碼後「${text}」位於第 ${pgFlips + 1} 頁（第 #${rank} 順位）：按【/】進入字詞選單 ➔ 按【+】翻下一頁 ➔ 按【${keyNum}】快選！`;
  }
  steps.push({
    key: rank <= 1 ? "+" : "/",
    code: code,
    activeSpots: [0,1,2,3],
    zone: null,
    desc: selDesc,
    status: `詞彙連打完成：${text}`,
    cands: CODE_CANDS[code] || [[text, "詞彙簡碼"]],
    sel: Math.max(0, rank - 1),
    compose: text,
    commit: text
  });

  const spotsArr = isTwoChar
    ? [
        `${chars[0]}①左上:${rawDigits[0] || ''}`,
        `${chars[0]}②右上:${rawDigits[1] || ''}`,
        `${chars[1] || ''}①左上:${rawDigits[2] || ''}`,
        `${chars[1] || ''}②右上:${rawDigits[3] || ''}`
      ]
    : [
        `首(${chars[0]})①,③:${rawDigits.slice(0,2)}`,
        `中(${chars.slice(1, -1).join("")})①:${rawDigits.slice(2, -2)}`,
        `尾(${chars[chars.length - 1]})②:${rawDigits[rawDigits.length - 2] || ''}`,
        `尾(${chars[chars.length - 1]})④:${rawDigits[rawDigits.length - 1] || ''}`
      ];

  const fontSizePx = chars.length >= 4 ? 54 : (chars.length === 3 ? 68 : 92);
  LESSONS[6] = {
    title: isTwoChar
      ? `🚀 二字詞 4 碼（首1,2+尾1,2）動畫：「${text}」（${code}）`
      : `🚀 ${chars.length}字詞 ${code.length} 碼（首1,3+中1×${chars.length-2}+尾2,4）動畫：「${text}」（${code}）`,
    charHtml: `<div class='phrase-pair'>${chars.map(c => `<span style="font-size:${fontSizePx}px;">${c}</span>`).join("")}</div>`,
    spots: spotsArr,
    steps: steps
  };
  curLesson = 6;
  curStep = 0;
  playing = true;
  document.getElementById("playPauseBtn").textContent = "⏸ 暫停動畫";
  renderLessonStep();
  focusAnimationStage(`詞彙「${text}」(${code})`, { tokenIdx: tokenIdx, tokenText: text });
}

function playLookupSequenceAnim() {
  const raw = document.getElementById("lookupInput").value.trim();
  if (!raw) return;
  sequenceTokens = segmentBestCombo(raw);
  if (sequenceTokens.length > 0) {
    sequenceIdx = 0;
    const tk = sequenceTokens[0];
    animateCustomToken(tk.text, tk.code, tk.isWord, tk.rank, 0);
  }
}

const LESSONS = [
  {
    title: "單元 1：單字正統取碼 — 以「台 (2360)」為例",
    charHtml: "台",
    spots: ["①左上: 2", "②右上: 3", "③左下: 6", "④右下: 0"],
    steps: [
      { key: null, code: "", activeSpots: [], zone: null, desc: "準備拆解「台」字：依 Z 字形順序（左上① ➔ 右上② ➔ 左下③ ➔ 右下④）", status: "等待輸入", cands: [["台","2360"],["臺","4010"]], sel: 0 },
      { key: "2", code: "2", activeSpots: [0], zone: [45,35,90,85], desc: "① 左上角：上方『厶』起筆撇折，歸為【2 垂】", status: "第 1 碼（左上角）", cands: [["的","2762"],["我","2355"],["後","2224"]], sel: 0 },
      { key: "3", code: "23", activeSpots: [0,1], zone: [130,55,80,75], desc: "② 右上角：上方『厶』右端之點，歸為【3 點】", status: "第 2 碼（右上角）", cands: [["我","2355"],["台","2360"],["台灣","2332"]], sel: 0 },
      { key: "6", code: "236", activeSpots: [0,1,2], zone: [55,130,150,90], desc: "③ 左下角：下方『口』為四方框，歸為【6 方】", status: "第 3 碼（左下角）", cands: [["台","2360"],["臺","正異共享"]], sel: 0 },
      { key: "0", code: "2360", activeSpots: [0,1,2,3], zone: [130,155,80,68], desc: "④ 右下角：『口』已被左下角取用過，正統規則「筆形複用歸 0」（若直覺打 2366 也支援容錯）！", status: "第 4 碼完成（正統碼）", cands: [["台","正統碼"],["臺","正異共享"]], sel: 0 },
      { key: "+", code: "2360", activeSpots: [0,1,2,3], zone: null, desc: "✅ 按小鍵盤 [+] 將「台」收攏至輸入座（或按 [Enter] 直接送出至目標視窗）！", status: "已收攏至輸入座：台", cands: [["台","正統碼"],["臺","正異共享"]], sel: 0, compose: "台" }
    ]
  },
  {
    title: "單元 2：「台灣的灣」單字怎麼拆？ — 正統 3212.7 ／ 直覺寬鬆 3232",
    charHtml: "灣",
    spots: ["①左上: 3", "②右上: 2", "③左下: 1(或3)", "④右下: 2(或7)"],
    steps: [
      { key: "3", code: "3", activeSpots: [0], zone: [25,30,65,85], desc: "① 左上角：左偏旁三點水『氵』最上方的點，取【3 點】", status: "第 1 碼（左上 3）", cands: [["之","3030"],["家","3023"]], sel: 0 },
      { key: "2", code: "32", activeSpots: [0,1], zone: [145,25,85,85], desc: "② 右上角：右上方『彎』頂部右側為『糸』，起筆撇折取【2 垂】", status: "第 2 碼（前兩角 32）", cands: [["業","3290"],["澄","3211"],["灣","3212"]], sel: 0 },
      { key: "1", code: "321", activeSpots: [0,1,2], zone: [25,135,65,90], desc: "③ 左下角：『氵』最下筆為「挑／提筆（㇀）」，正統歸【1 橫】！（若直覺打 3 點也支援容錯！）", status: "第 3 碼（321 或 323）", cands: [["澄","3211"],["灣","3212"],["潘","3216"]], sel: 0 },
      { key: "2", code: "3212", activeSpots: [0,1,2,3], zone: [120,140,105,85], desc: "④ 右下角：下方『弓』最右下為豎左鉤，取【2 垂】➔ 3212 第 1 順位即為「灣」！", status: "3212 首選 #1 即為「灣」！", cands: [["灣","正統3212"],["漸","3212"],["澎","3212"]], sel: 0, compose: "灣" },
      { key: "2", code: "3232", activeSpots: [0,1,2,3], zone: null, desc: "💡 初學者免卡關：若把三點水左下當成 3 打【3232】、或把弓當成 7 打【3217 / 3237】，全部自動容錯出「灣」！", status: "直覺寬鬆碼 3232 / 3217 同步命中！", cands: [["灣","直覺容錯碼"],["溪","3233"]], sel: 0, compose: "灣", commit: "灣" }
    ]
  },
  {
    title: "單元 3：「- 詞前導」二字詞 4 碼（-首1,2+尾1,2）— 打「台灣 (-2332)」、「主權 (-0044)」、「天才 (-1040)」",
    charHtml: "<div class='phrase-pair'><span>台</span><span>灣</span></div>",
    spots: ["台①左上: 2", "台②右上: 3", "灣①左上: 3", "灣②右上: 2"],
    steps: [
      { key: "-", code: "-", activeSpots: [], zone: null, desc: "步驟 0（詞前導）：先按小鍵盤【-】進入「詞彙模式（-）」，徹底將二字詞 4 碼與單字 4 碼分流（不再互相搶位）！", status: "➖ 詞彙前導模式 (-)", cands: [["台灣","-2332"],["主權","-0044"],["天才","-1040"]], sel: 0 },
      { key: "2", code: "-2", activeSpots: [0], zone: [18,65,110,70], desc: "步驟 1：取首字『台』第 ① 左上角【2】", status: "首字第 1 角 (-2)", cands: [["台灣","-2332"],["我們","-2327"]], sel: 0 },
      { key: "3", code: "-23", activeSpots: [0,1], zone: [18,65,110,70], desc: "步驟 2：取首字『台』第 ② 右上角【3】（『台』下半部直接跳過免拆！）", status: "首字前兩角 [-23] 完成", cands: [["台灣","-2332"],["我們","-2327"]], sel: 0 },
      { key: "3", code: "-233", activeSpots: [0,1,2], zone: [132,65,110,70], desc: "步驟 3：取尾字『灣』第 ① 左上角氵點【3】", status: "尾字第 1 角 (-233)", cands: [["台灣","二字4碼"],["臺灣","正異共享"]], sel: 0 },
      { key: "2", code: "-2332", activeSpots: [0,1,2,3], zone: [132,65,110,70], desc: "步驟 4：取尾字『灣』第 ② 右上角糸撇【2】➔ -2332 直接命中「台灣」（同理「主權」=-0044、「天才」=-1040 第1頁#9）！", status: "二字詞 -2332 連打完成！", cands: [["台灣","二字4碼"],["臺灣","正異共享"]], sel: 0, compose: "台灣", commit: "台灣" }
    ]
  },
  {
    title: "單元 4：三字詞 5 碼／四字詞 6 碼 — 「-首(1,3) + 中(1)×n + 尾(2,4)」黃金結構",
    charHtml: "<div class='phrase-pair'><span style='font-size:66px;'>輸</span><span style='font-size:66px;'>入</span><span style='font-size:66px;'>法</span></div>",
    spots: ["首(輸)①,③: 50", "中(入)①: 8", "尾(法)②: 4", "尾(法)④: 3"],
    steps: [
      { key: "-", code: "-", activeSpots: [], zone: null, desc: "步驟 0（詞前導）：按小鍵盤【-】進入詞彙模式", status: "➖ 詞彙前導模式 (-)", cands: [["輸入法","-50843"],["四角號碼","-622612"]], sel: 0 },
      { key: "5", code: "-50", activeSpots: [0], zone: [12,65,80,75], desc: "步驟 1（首字取 1, 3 角）：首字『輸(5802)』取第 ① 左上角【5】與第 ③ 左下角【0】➔ 輸入 -50", status: "首字 (1,3) 角 = -50", cands: [["輸入法","-50843"]], sel: 0 },
      { key: "8", code: "-508", activeSpots: [0,1], zone: [92,65,75,75], desc: "步驟 2（中間各字皆取第 1 角）：中字『入(8000)』取第 ① 左上角【8】➔ 輸入 8", status: "中字 (1) 角 = 8", cands: [["輸入法","三字5碼"]], sel: 0 },
      { key: "4", code: "-5084", activeSpots: [0,1,2], zone: [168,65,80,75], desc: "步驟 3（尾字取 2, 4 角之第 2 角）：尾字『法(3413)』取第 ② 右上角『土』交叉【4】", status: "尾字第 2 角 = 4", cands: [["輸入法","三字5碼"]], sel: 0 },
      { key: "3", code: "-50843", activeSpots: [0,1,2,3], zone: [168,65,80,75], desc: "步驟 4（尾字取 2, 4 角之第 4 角）：尾字『法(3413)』取第 ④ 右下角『厶』點筆【3】➔ -50843 唯一精準命中「輸入法」！", status: "三字詞 -50843 零重碼命中！", cands: [["輸入法","三字詞5碼"]], sel: 0, compose: "輸入法" },
      { key: "2", code: "-622612", activeSpots: [0,1,2,3], zone: null, desc: "💡 四字成語／詞彙同理為 -6 碼（-首1,3 + 中一1 + 中二1 + 尾2,4）：例如「四角號碼」= -四(62) + 角(2) + 號(6) + 碼(12) =【-622612】！", status: "四字詞 -622612 直出「四角號碼」！", cands: [["四角號碼","四字詞6碼"]], sel: 0, compose: "四角號碼輸入法", commit: "四角號碼輸入法" }
    ]
  },
  {
    title: "單元 5：「- 詞前導 ／ + 字詞分隔 ／ . 標點選單 ／ * 萬用字元」",
    charHtml: "<div class='phrase-pair'><span>台</span><span>灣</span></div>",
    spots: ["- 鍵：詞前導符號", "+ 鍵：字詞分隔收攏", ". 鍵：標點符號選單", "* 鍵：萬用字元查詢"],
    steps: [
      { key: "-", code: "-2332", activeSpots: [0], zone: null, desc: "步驟 1：先按【-】再輸入二字詞「台灣」4 碼【-2332】，候選第 #1 為「台灣」（若某角不確定可按【*】萬用字元，如 23*0）！", status: "輸入 -2332 ➔ 1.台灣", cands: [["台灣","首選#1"],["臺灣","正異共享"]], sel: 0 },
      { key: "+", code: "", activeSpots: [0,1], zone: null, desc: "步驟 2：按小鍵盤【+】作為字、詞分隔符號 ➔ 「台灣」收攏進入『如喜輸入座組句區』，可繼續打下一字詞！", status: "按 [+] 分隔收攏至輸入座：「台灣」", cands: [["(等待輸入下一字詞)","提示"]], sel: 0, compose: "台灣" },
      { key: ".", code: ".", activeSpots: [0,1,2], zone: null, desc: "步驟 3：想打標點符號時，按小鍵盤【.】開啟標點符號選單（按 1~0 快選，+ 翻下頁 / - 翻上頁，或連按 .. 直出逗號）！", status: "按 [.] 開啟標點選單（1， 2。 3、 4？ 5！）", cands: [["，",".1"],["。",".2"],["、",".3"],["？",".4"],["！",".5"],["：",".6"],["；",".7"],["「」",".8"],["（）",".9"],["……",".0"]], sel: 0, compose: "台灣" },
      { key: "Enter", code: "", activeSpots: [0,1,2,3], zone: null, desc: "步驟 4：整句打完後，按小鍵盤右下角【Enter】鍵 ➔ 將整句一次完稿送出至目標視窗！", status: "按 [Enter] 整句送出至目標視窗：「台灣，」", cands: [["(已完稿送出至目標視窗)","完成"]], sel: 0, compose: "（空白）", commit: "台灣，" }
    ]
  },
  {
    title: "單元 6：「/ 字詞選單，數字鍵 1~0 快選，+ 翻下一頁，- 翻上一頁」",
    charHtml: "<div class='phrase-pair'><span>天</span><span>才</span></div>",
    spots: ["/ 鍵：字詞選單", "1~0：數字快選", "+ 鍵：翻下一頁", "- 鍵：翻上一頁"],
    steps: [
      { key: "0", code: "-1040", activeSpots: [0], zone: null, desc: "步驟 1：例如輸入「天才」的詞碼【-1040】（因單字 1040 已分流，「天才」直接排在第 1 頁第 #9 順位）！", status: "輸入 -1040 ➔ 天才位於第 1 頁 #9", cands: [["干預","#1"],["平時","#2"],["平衡","#3"],["平常","#4"],["平均","#5"],["平坦","#6"],["干擾","#7"],["平靜","#8"],["天才","#9"],["天職","#0"]], sel: 0 },
      { key: "/", code: "-1040", activeSpots: [0,1], zone: null, desc: "步驟 2：按一下小鍵盤【/（除號）】進入字詞選單模式！此時按【+】可翻下一頁、按【-】可翻上一頁！", status: "按 [/] 進入字詞選單模式（+翻下頁 / -翻上頁）", cands: [["干預","#1"],["平時","選單中"],["平衡","#3"],["平常","#4"],["平均","#5"],["平坦","#6"],["干擾","#7"],["平靜","#8"],["天才","#9"],["天職","#0"]], sel: 1 },
      { key: "9", code: "-1040", activeSpots: [0,1,2,3], zone: null, desc: "步驟 3：直接按數字鍵【9】快選 ➔ 立即選出「9.天才」收攏至輸入座！（或直接打單字：天 1080 #1 + 才 4020 /2）", status: "按 [9] 快選 ➔ 直接選出「天才」", cands: [["干預","#1"],["平時","#2"],["平衡","#3"],["平常","#4"],["平均","#5"],["平坦","#6"],["干擾","#7"],["平靜","#8"],["天才","已選中#9"],["天職","#0"]], sel: 8, compose: "天才", commit: "天才" }
    ]
  }
];

let curLesson = 3; // 預設直接展示單元 4（三／四字詞 1,3[1×n]2,4 黃金結構）
let curStep = 0;
let playing = true;
let sandboxCode = "";
let sandboxCompose = "";
let sandboxTargetOut = "";
let sandboxSelecting = false;
let sandboxSel = 0;

function renderLessonStep() {
  const L = LESSONS[curLesson];
  if (!L) return;
  const S = L.steps[curStep];
  document.getElementById("lessonTitle").textContent = L.title;
  document.getElementById("stageChar").innerHTML = L.charHtml;

  for (let i = 0; i < 4; i++) {
    const sp = document.getElementById("spot" + i);
    sp.textContent = L.spots[i] || "";
    sp.classList.toggle("active", S.activeSpots.includes(i));
  }

  const hl = document.getElementById("hlZone");
  if (S.zone) {
    hl.style.left = S.zone[0] + "px";
    hl.style.top = S.zone[1] + "px";
    hl.style.width = S.zone[2] + "px";
    hl.style.height = S.zone[3] + "px";
    hl.classList.add("show");
  } else {
    hl.classList.remove("show");
  }

  const listEl = document.getElementById("stepList");
  listEl.innerHTML = "";
  L.steps.forEach((st, idx) => {
    const div = document.createElement("div");
    div.className = "step-item" + (idx < curStep ? " done" : "") + (idx === curStep ? " current" : "");
    div.textContent = st.desc;
    div.onclick = () => { curStep = idx; pauseAnim(); renderLessonStep(); };
    listEl.appendChild(div);
  });

  document.querySelectorAll(".np-key").forEach(k => {
    const dk = k.getAttribute("data-key");
    k.classList.toggle("pressing", dk === S.key);
    k.classList.toggle("trail", S.code.includes(dk) && dk !== S.key);
  });

  document.getElementById("tsfCode").textContent = S.code || "(等待輸入)";
  document.getElementById("tsfStatus").textContent = S.status;
  if (S.compose !== undefined) {
    document.getElementById("composeDeskOut").textContent = S.compose;
  }
  if (S.commit) {
    document.getElementById("committedOut").textContent = S.commit;
  }
  renderCandidates(S.cands, S.sel);
}

function renderCandidates(cands, selIdx) {
  const box = document.getElementById("tsfCandidates");
  box.innerHTML = "";
  const safeSel = Math.max(0, Math.min(selIdx, Math.max(0, cands.length - 1)));
  const pageIdx = Math.floor(safeSel / 10);
  const totalPages = Math.max(1, Math.ceil(cands.length / 10));
  const pageStart = pageIdx * 10;
  const pageItems = cands.slice(pageStart, pageStart + 10);

  pageItems.forEach((c, idx) => {
    const actualIdx = pageStart + idx;
    const div = document.createElement("div");
    div.className = "cand-item" + (actualIdx === safeSel ? " selected" : "");
    const numLbl = idx === 9 ? "0" : String(idx + 1);
    div.innerHTML = `<span>${numLbl}.${c[0]}</span><span class="cand-tag">${c[1]}</span>`;
    div.onclick = () => {
      sandboxCompose += c[0];
      document.getElementById("composeDeskOut").textContent = sandboxCompose;
    };
    box.appendChild(div);
  });

  const pgInfo = document.getElementById("tsfPageInfo");
  if (pgInfo) {
    pgInfo.textContent = `第 ${pageIdx + 1}/${totalPages} 頁（共 ${cands.length} 項）`;
  }
}

function loadLesson(idx) {
  sequenceIdx = -1;
  curLesson = idx;
  curStep = 0;
  document.querySelectorAll(".tab-btn").forEach((b, i) => b.classList.toggle("active", i === idx));
  document.querySelectorAll(".char-detail-card").forEach(el => el.classList.remove("active-card"));
  document.querySelectorAll(".combo-token").forEach(el => el.classList.remove("active-token"));
  const badge = document.getElementById("nowPlayingBadge");
  if (badge) badge.textContent = `🎬 舞台播放中：單元 ${idx + 1}`;
  renderLessonStep();
}

function nextStep() {
  const L = LESSONS[curLesson];
  if (!L) return;
  if (sequenceIdx >= 0 && sequenceTokens.length > 1 && curStep === L.steps.length - 1) {
    sequenceIdx = (sequenceIdx + 1) % sequenceTokens.length;
    const tk = sequenceTokens[sequenceIdx];
    animateCustomToken(tk.text, tk.code, tk.isWord, tk.rank, sequenceIdx);
    return;
  }
  curStep = (curStep + 1) % L.steps.length;
  renderLessonStep();
}

function prevStep() {
  const L = LESSONS[curLesson];
  if (!L) return;
  curStep = (curStep - 1 + L.steps.length) % L.steps.length;
  renderLessonStep();
}

function pauseAnim() {
  playing = false;
  document.getElementById("playPauseBtn").textContent = "▶ 繼續播放動畫";
}

function togglePlay() {
  playing = !playing;
  document.getElementById("playPauseBtn").textContent = playing ? "⏸ 暫停動畫" : "▶ 繼續播放動畫";
}

function lookupSandbox(code) {
  if (!code) return [["台","2360"],["台灣","-2332"],["輸入法","-50843"],["四角號碼","-622612"],["天才","-1040"]];
  if (code === ".") {
    return [
      ["，","標點1"],["。","標點2"],["、","標點3"],["？","標點4"],["！","標點5"],
      ["：","標點6"],["；","標點7"],["「」","標點8"],["（）","標點9"],["……","標點0"],
      ["「","第2頁1"],["」","第2頁2"],["『』","第2頁3"],["《》","第2頁4"],["【】","第2頁5"],
      ["——","第2頁6"],["～","第2頁7"],["·","第2頁8"],["（","第2頁9"],["）","第2頁0"]
    ];
  }
  if (code === "-") {
    return [
      ["台灣","-2332"],["輸入法","-50843"],["四角號碼","-622612"],["主權","-0044"],["電腦","-1072"],
      ["天才","-1040"],["為什麼","-30203"],["如喜","-4640"],["我們","-2327"],["自己","-2677"],
      ["國家","-6030"],["知道","-8638"],["智慧","-8655"],["時間","-6077"],["可以","-1028"],
      ["現在","-1040"],["這個","-3026"],["因為","-6034"],["所以","-7228"],["一舉兩得","-107164"]
    ];
  }
  if (CODE_CANDS[code]) return CODE_CANDS[code];
  // 支援 `*` 萬用字元查詢
  if (code.includes("*")) {
    const pat = new RegExp("^" + code.replace(/^\-/, "\\-").replace(/\*/g, "[0-9]") + "$");
    const hits = [];
    for (const [k, arr] of Object.entries(CODE_CANDS)) {
      if (pat.test(k)) {
        for (const item of arr) {
          if (!hits.some(x => x[0] === item[0])) hits.push([item[0], k]);
          if (hits.length >= 20) break;
        }
      }
      if (hits.length >= 20) break;
    }
    if (hits.length) return hits;
  }
  // 前綴匹配
  const res = [];
  for (const [k, arr] of Object.entries(CODE_CANDS)) {
    if (k.startsWith(code)) {
      for (const item of arr) {
        if (!res.some(x => x[0] === item[0])) res.push([item[0], k]);
        if (res.length >= 10) break;
      }
    }
    if (res.length >= 10) break;
  }
  return res.length ? res : [["(無匹配候選)","提示"]];
}

function pressVirtualKey(k) {
  pauseAnim();
  document.querySelectorAll(".np-key").forEach(el => {
    el.classList.toggle("pressing", el.getAttribute("data-key") === k);
  });
  setTimeout(() => {
    document.querySelectorAll(".np-key").forEach(el => el.classList.remove("pressing"));
  }, 200);

  const cands = lookupSandbox(sandboxCode);

  if (k === "Backspace") {
    if (sandboxSelecting) {
      sandboxSelecting = false;
      sandboxSel = 0;
    } else if (sandboxCode) {
      sandboxCode = sandboxCode.slice(0, -1);
      sandboxSel = 0;
    } else if (sandboxCompose) {
      sandboxCompose = Array.from(sandboxCompose).slice(0, -1).join("");
      document.getElementById("composeDeskOut").textContent = sandboxCompose || "（空白）";
    }
  } else if (k === "-") {
    // 選單模式或標點選單下：`-` 翻上一頁；一般狀態下：`-` 為「詞」前導符號！
    if ((sandboxSelecting || sandboxCode === ".") && cands.length > 0) {
      const curPage = Math.floor(sandboxSel / 10);
      const totalPages = Math.max(1, Math.ceil(cands.length / 10));
      const prevPage = curPage > 0 ? curPage - 1 : totalPages - 1;
      sandboxSel = Math.min(prevPage * 10, cands.length - 1);
    } else if (!sandboxCode) {
      sandboxCode = "-";
      sandboxSel = 0;
    } else if (sandboxCode === "-") {
      sandboxCode = "";
      sandboxSel = 0;
    } else if (sandboxCode.replace(/^-/, "").length < 4) {
      sandboxCode = sandboxCode.startsWith("-") ? sandboxCode.slice(1) : ("-" + sandboxCode);
      sandboxSel = 0;
    } else {
      if (cands.length > 0) {
        const pick = cands[sandboxSel % cands.length][0];
        if (!pick.startsWith("(")) {
          sandboxCompose += pick;
          document.getElementById("composeDeskOut").textContent = sandboxCompose;
        }
      }
      sandboxCode = "-";
      sandboxSelecting = false;
      sandboxSel = 0;
    }
  } else if (k === "/") {
    // `/` 字詞選單：進入選單模式（1~0 快選，+ 翻下一頁，- 翻上一頁；包含剛按 `-` 時亦可直接按 `/` 選常用詞）
    if (sandboxCode && cands.length > 0) {
      if (sandboxCode === ".") {
        sandboxSelecting = true;
      } else if (!sandboxSelecting) {
        sandboxSelecting = true;
        sandboxSel = cands.length > 1 ? 1 : 0;
      } else {
        sandboxSel = (sandboxSel + 1) % cands.length;
      }
    }
  } else if (k === "*") {
    // `*` 萬用字元：代表任意 0~9 筆形角碼
    if (sandboxCode === ".") sandboxCode = "";
    sandboxSelecting = false;
    if (sandboxCode.replace(/^-/, "").length < 6) {
      sandboxCode += "*";
      sandboxSel = 0;
    }
  } else if (k === ".") {
    // `.` 標點符號選單前導：連按兩次 `..` 直出逗號，否則展開標點選單
    if (sandboxCode === ".") {
      const pick = cands[sandboxSel % cands.length][0];
      sandboxCompose += pick;
      document.getElementById("composeDeskOut").textContent = sandboxCompose;
      sandboxCode = "";
      sandboxSelecting = false;
      sandboxSel = 0;
    } else {
      if (sandboxCode && sandboxCode !== "-" && cands.length > 0) {
        const pick = cands[sandboxSel % cands.length][0];
        if (!pick.startsWith("(")) {
          sandboxCompose += pick;
          document.getElementById("composeDeskOut").textContent = sandboxCompose;
        }
      }
      sandboxCode = ".";
      sandboxSelecting = false;
      sandboxSel = 0;
    }
  } else if (k === "PageDown") {
    if (cands.length > 0) {
      if (!sandboxCode) sandboxCode = document.getElementById("tsfCode").textContent.split(" ")[0];
      sandboxSelecting = true;
      const curPage = Math.floor(sandboxSel / 10);
      const totalPages = Math.max(1, Math.ceil(cands.length / 10));
      const nextPage = (curPage + 1) % totalPages;
      sandboxSel = Math.min(nextPage * 10, cands.length - 1);
    }
  } else if (k === "PageUp") {
    if (cands.length > 0) {
      if (!sandboxCode) sandboxCode = document.getElementById("tsfCode").textContent.split(" ")[0];
      sandboxSelecting = true;
      const curPage = Math.floor(sandboxSel / 10);
      const totalPages = Math.max(1, Math.ceil(cands.length / 10));
      const prevPage = curPage > 0 ? curPage - 1 : totalPages - 1;
      sandboxSel = Math.min(prevPage * 10, cands.length - 1);
    }
  } else if (k === "ArrowDown") {
    if (cands.length > 0) {
      if (!sandboxCode) sandboxCode = document.getElementById("tsfCode").textContent.split(" ")[0];
      sandboxSelecting = true;
      sandboxSel = (sandboxSel + 1) % cands.length;
    }
  } else if (k === "ArrowUp") {
    if (cands.length > 0) {
      if (!sandboxCode) sandboxCode = document.getElementById("tsfCode").textContent.split(" ")[0];
      sandboxSelecting = true;
      sandboxSel = (sandboxSel - 1 + cands.length) % cands.length;
    }
  } else if (k === "+") {
    // 選單模式或標點選單下：`+` 翻下一頁；一般狀態下：`+` 為字／詞分隔符號（收攏至輸入座）
    if ((sandboxSelecting || sandboxCode === ".") && cands.length > 0) {
      const curPage = Math.floor(sandboxSel / 10);
      const totalPages = Math.max(1, Math.ceil(cands.length / 10));
      const nextPage = (curPage + 1) % totalPages;
      sandboxSel = Math.min(nextPage * 10, cands.length - 1);
    } else {
      if (sandboxCode && sandboxCode !== "-" && cands.length > 0) {
        const pick = cands[sandboxSel % cands.length][0];
        if (!pick.startsWith("(")) {
          sandboxCompose += pick;
          document.getElementById("composeDeskOut").textContent = sandboxCompose;
        }
      }
      sandboxCode = "";
      sandboxSelecting = false;
      sandboxSel = 0;
    }
  } else if (k === "Enter") {
    // Enter 鍵：確認當前字碼並將「如喜輸入座組句區」整句完稿送出至目標視窗！
    if (sandboxCode && cands.length > 0) {
      const pick = cands[sandboxSel % cands.length][0];
      if (!pick.startsWith("(")) {
        sandboxCompose += pick;
      }
    }
    if (sandboxCompose) {
      sandboxTargetOut = sandboxCompose;
      document.getElementById("committedOut").textContent = sandboxTargetOut;
      sandboxCompose = "";
      document.getElementById("composeDeskOut").textContent = "（已送出至目標視窗）";
    }
    sandboxCode = "";
    sandboxSelecting = false;
    sandboxSel = 0;
  } else if (/^[0-9]$/.test(k)) {
    if ((sandboxSelecting || sandboxCode === ".") && sandboxCode) {
      const pageBase = Math.floor(sandboxSel / 10) * 10;
      const offset = k === "0" ? 9 : (parseInt(k, 10) - 1);
      const idx = pageBase + offset;
      if (idx < cands.length) {
        const pick = cands[idx][0];
        if (!pick.startsWith("(")) {
          sandboxCompose += pick;
          document.getElementById("composeDeskOut").textContent = sandboxCompose;
        }
        sandboxCode = "";
        sandboxSelecting = false;
        sandboxSel = 0;
      }
    } else {
      if (sandboxCode.replace(/^-/, "").length < 6) sandboxCode += k;
      sandboxSel = 0;
    }
  }

  const newCands = lookupSandbox(sandboxCode);
  const curPg = Math.floor(sandboxSel / 10) + 1;
  const totPg = Math.max(1, Math.ceil(newCands.length / 10));
  document.getElementById("tsfCode").textContent = sandboxSelecting
    ? `${sandboxCode} [第${curPg}/${totPg}頁 · 選字#${(sandboxSel % 10) + 1}：按 1~0 直選或 PgDn 翻頁]`
    : (sandboxCode || "(互動試打中)");
  document.getElementById("tsfStatus").textContent = sandboxSelecting
    ? "🎯 選字鎖定模式：按數字 1~0 收攏至輸入座，或按 Enter 直接整句送出！"
    : "⚡ 即時試打模式（按 + 收攏至輸入座組句 ／ 按 Enter 整句完稿送出至視窗）";
  renderCandidates(newCands, sandboxSel % Math.max(1, newCands.length));
}

function clearSandbox() {
  sandboxCode = "";
  sandboxCompose = "";
  sandboxTargetOut = "";
  sandboxSelecting = false;
  sandboxSel = 0;
  document.getElementById("composeDeskOut").textContent = "（空白）";
  document.getElementById("committedOut").textContent = "（已清空）";
  renderLessonStep();
}

window.addEventListener("keydown", (e) => {
  // 若焦點在上方文字查詢輸入框內，不攔截鍵盤，讓使用者正常打字查碼
  if (document.activeElement && document.activeElement.id === "lookupInput") {
    return;
  }
  if (
    /^[0-9*+\-./]$/.test(e.key) ||
    e.key === "Backspace" ||
    e.key === "Enter" ||
    e.key === "PageDown" ||
    e.key === "PageUp" ||
    e.key === "ArrowDown" ||
    e.key === "ArrowUp"
  ) {
    e.preventDefault();
    pressVirtualKey(e.key);
  }
});

setInterval(() => {
  if (playing) {
    nextStep();
  }
}, 2400);

// 初始化執行「智慧型四角號碼輸入法」整串查碼與單元 4（三／四字詞 1,3[1×n]2,4 黃金結構）
analyzeFullText();
loadLesson(3);
</script>
</body>
</html>
"""


def main():
    print("[1/2] 正在從 four_corner.tsv 抽取單字拆碼與詞彙前兩角簡碼索引...")
    compact_chars, compact_words, code_top_cands = build_dictionary_payload()
    html = (
        HTML_TEMPLATE.replace("__CHAR_DB_JSON__", json.dumps(compact_chars, ensure_ascii=False, separators=(",", ":")))
        .replace("__WORD_DB_JSON__", json.dumps(compact_words, ensure_ascii=False, separators=(",", ":")))
        .replace("__CODE_CANDS_JSON__", json.dumps(code_top_cands, ensure_ascii=False, separators=(",", ":")))
    )
    OUT_HTML.write_text(html, encoding="utf-8")
    print(f"[2/2] 成功更新互動教學與整串查碼頁面：{OUT_HTML} ({OUT_HTML.stat().st_size / 1024:.1f} KB)")
    print(f"      -> 內建單字查碼數：{len(compact_chars):,} 字｜詞彙簡碼數：{len(compact_words):,} 詞｜即時碼表：{len(code_top_cands):,} 組")


if __name__ == "__main__":
    main()
