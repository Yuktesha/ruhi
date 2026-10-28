# -*- coding: utf-8 -*-
"""
建置「如喜（RuhI）— 智慧型四角號碼互動教學動畫與整串文字查碼器」單一獨立 HTML 頁面
內嵌完整單字拆碼字典、高頻詞彙前兩角簡碼字典與候選序位反查表。
"""
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

ROOT = Path(r"C:\ATGprjs\ruhi")
TSV_PATH = ROOT / "ruhi_lexicon" / "data" / "four_corner.tsv"
OUT_HTML = ROOT / "docs" / "ruhi_interactive_workbench.html"
LEGACY_HTML = ROOT / "docs" / "four_corner_tutorial_anim.html"


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


def build_cross_ime_and_longkan_payloads(compact_chars):
    data_dir = ROOT / "ruhi_lexicon" / "data"
    ar30_primary = {}
    ar30_extra = defaultdict(list)
    ar30_path = data_dir / "array30.tsv"
    if ar30_path.exists():
        for line in ar30_path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "\t" not in line:
                continue
            parts = line.split("\t")
            code, ch = parts[0].strip().lower(), parts[1].strip()
            kind = parts[3].strip() if len(parts) >= 4 else "full"
            if len(ch) != 1:
                continue
            if kind == "full" and ch not in ar30_primary:
                ar30_primary[ch] = code
            elif kind == "special":
                ar30_extra[ch].append(f"特:{code.upper()}")
            elif kind == "short1":
                ar30_extra[ch].append(f"簡1:{code.upper()}")
            elif kind == "short2" and not any(x.startswith("簡1:") for x in ar30_extra[ch]):
                ar30_extra[ch].append(f"簡2:{code.upper()}")

    cj_primary = {}
    cj_path = data_dir / "cangjie.tsv"
    if cj_path.exists():
        for line in cj_path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "\t" not in line:
                continue
            parts = line.split("\t")
            code, ch = parts[0].strip().lower(), parts[1].strip()
            if len(ch) == 1 and ch not in cj_primary:
                cj_primary[ch] = code

    bpm_primary = {}
    for fn in ["lexicon_mcbopomofo.tsv", "lexicon_moe.tsv"]:
        p = data_dir / fn
        if not p.exists():
            continue
        best_w = {}
        for line in p.read_text(encoding="utf-8").splitlines():
            parts = line.strip().split("\t")
            if len(parts) >= 3 and len(parts[0]) == 1:
                ch, bpm = parts[0], parts[1].strip()
                try:
                    w = int(parts[2])
                except ValueError:
                    w = 100
                if ch not in bpm_primary and (ch not in best_w or w > best_w[ch][0]):
                    best_w[ch] = (w, bpm)
        for ch, (_, bpm) in best_w.items():
            if ch not in bpm_primary:
                bpm_primary[ch] = bpm

    cross_db = {}
    for ch in compact_chars:
        a_code = ar30_primary.get(ch, "")
        a_ext = "·".join(ar30_extra.get(ch, [])[:2])
        c_code = cj_primary.get(ch, "")
        b_code = bpm_primary.get(ch, "")
        if a_code or c_code or b_code:
            cross_db[ch] = [a_code, a_ext, c_code, b_code]

    longkan_db = {}
    lk_candidates = [
        data_dir / "longkan.tsv",
    ]
    lk_path = next((p for p in lk_candidates if p.exists()), None)
    if lk_path is not None:
        char_all = defaultdict(list)
        for line in lk_path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#") or line.startswith("CODE\t") or "\t" not in line:
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            code, text = parts[0].strip().upper(), parts[1].strip()
            if len(text) != 1:
                continue
            try:
                w = int(parts[2])
            except ValueError:
                w = 500
            kind = parts[3].strip() if len(parts) >= 4 else "rad_phon"
            bdesc = parts[4].strip() if len(parts) >= 5 else ""
            if kind == "prefix_hint":
                continue
            score = w + (
                1000 if kind == "curated"
                else (280 if kind in ("hsu_full_comp", "pinyin_full_comp")
                      else (250 if len(code) == 4 and (code.isalpha() or kind == "rad2_phon1_tone")
                            else (220 if kind == "visual_4" else 0)))
            )
            char_all[text].append((score, code, bdesc, kind))

        def alt_category(code_s: str, bdesc_s: str, kind_s: str) -> str:
            if "許氏全拼" in bdesc_s or "許氏三部件" in bdesc_s or "許氏首尾" in bdesc_s or kind_s == "hsu_full_comp":
                return "hsu_full"
            if "拼音全拼" in bdesc_s or "拼音三部件" in bdesc_s or "拼音首尾" in bdesc_s or kind_s == "pinyin_full_comp":
                return "pinyin_full"
            if "視覺拆字" in bdesc_s or "視覺簡碼" in bdesc_s or kind_s == "visual_4":
                return "visual_4"
            if "許氏" in bdesc_s:
                return "hsu"
            if "通用拼音" in bdesc_s:
                return "tongyong"
            if "注音部件" in bdesc_s or kind_s == "bpm_comp":
                return "bpm_comp" if len(code_s) == 4 else "bpm_comp_3"
            if "字母部件" in bdesc_s or kind_s == "latin_comp":
                return "latin_comp" if len(code_s) == 4 else "latin_comp_3"
            if "首尾部件" in bdesc_s:
                return "first_last"
            if "2+1+調" in bdesc_s or kind_s == "rad2_phon1_tone":
                return "tone_21"
            if "本音直覺" in bdesc_s or "本音" in bdesc_s:
                return "phon_self"
            if "最簡部件" in bdesc_s or "右旁" in bdesc_s or "左旁" in bdesc_s or "×" in bdesc_s:
                return "comp_direct"
            if "象形" in bdesc_s or kind_s in ("pict_3", "pict_4") or code_s.startswith(("AO", "OT", "XE", "XU", "CP")):
                return "pict"
            if "雙拼混搭" in bdesc_s or "筆順相容" in bdesc_s:
                return "hybrid"
            if "雙軌別稱" in bdesc_s:
                return "alias"
            if len(code_s) == 4 and code_s.isalpha():
                return "rad_22"
            return "other"

        def code_stem(code_s: str, bdesc_s: str) -> str:
            if len(code_s) >= 3 and re.search(r"\+[1-5dfjsDFJS]?[聲調]", bdesc_s):
                if code_s[-1] in "12345DFJS":
                    return code_s[:-1]
            return code_s

        # 載入 CHISE IDS 漢字結構描述表以精確判定每個漢字的空間幾何拓撲結構
        ids_map = {}
        ids_candidates = [
            ROOT / "_archive" / "_external" / "ids.txt",
            data_dir / "ids.txt",
        ]
        ids_path = next((p for p in ids_candidates if p.exists()), None)
        if ids_path is not None:
            for line in ids_path.read_text(encoding="utf-8", errors="ignore").splitlines():
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) >= 3 and len(parts[1]) == 1:
                    ids_map[parts[1]] = parts[2].split("[")[0].strip()

        def classify_char_structure(ch_s: str, bdesc_s: str) -> str:
            if ch_s in ("品", "森", "晶", "淼", "矗", "磊", "眾", "歮", "姦", "聶", "轟", "鑫", "焱"):
                return "tri3"
            if ch_s == "區" or (ch_s in ("樞", "歐", "毆", "嘔", "謳") and "口×3" in bdesc_s):
                return "left3_tri3"
            if ch_s == "協":
                return "left_tri3"
            if ch_s in ("蕊", "惢"):
                return "top_tri3"
            if ch_s in ("凶", "凹", "凸", "函", "畫", "幽"):
                return "bot3"
            if ch_s in ("龍", "入", "一", "二", "三", "十", "大", "小", "米", "女", "子", "水", "火", "山", "石", "田", "目", "日", "月", "木", "本", "末", "未", "車", "馬", "鳥", "魚", "門", "雨", "耳", "手", "心", "戈", "寸", "工", "土", "士", "夕", "弓", "己", "已", "巳", "巾", "干", "幺", "尸", "尺", "毛", "片", "牙", "牛", "犬", "王", "玉", "瓜", "瓦", "甘", "生", "用", "白", "皮", "皿", "矛", "矢", "禾", "穴", "立", "竹", "糸", "羊", "羽", "老", "而", "耒", "聿", "肉", "臣", "自", "至", "臼", "舌", "舟", "艮", "色", "虫", "血", "行", "衣", "見", "角", "言", "谷", "豆", "豕", "豸", "貝", "赤", "走", "足", "身", "辛", "辰", "酉", "釆", "里", "金", "長", "阜", "隶", "隹", "青", "非", "面", "革", "韋", "韭", "音", "頁", "風", "飛", "食", "首", "香", "骨", "高", "鬼", "鹿", "麥", "麻", "黃", "黍", "黑", "鼎", "鼓", "鼠", "鼻", "齊", "齒", "龜"):
                if ch_s == "音":
                    return "tb"
                if ch_s == "鼻":
                    return "tmb"
                return "single"
            if ch_s in ("謝", "樹", "街", "術", "衍", "辨", "辯", "辮", "瓣", "班"):
                return "lmr"
            if ch_s in ("喜", "尋", "翼", "鼻", "算", "答", "茶", "荷", "落", "意", "愛", "憂", "憲", "塞", "寒", "寨", "賣", "聽"):
                return "tmb"
            if ch_s == "肴":
                return "tb"

            raw_ids = ids_map.get(ch_s, "")
            if not raw_ids or len(raw_ids) == 1:
                return "single"

            p1 = bdesc_s.split("+")[0] if "+" in bdesc_s else ""
            p1_char = ""
            for c_char in p1:
                if "\u4e00" <= c_char <= "\u9fff":
                    p1_char = c_char
                    break

            op = raw_ids[0]
            if op == "⿰":
                if "×3" in bdesc_s:
                    return "left_tri3"
                # 若首部明確為左旁部件（如「形」= 井+3畫），歸為左右結構 lr
                if p1_char in ("井", "開", "刑"):
                    return "lr"
                # 檢查 IDS 左半與右半何者對應首部
                rest = raw_ids[1:]
                if p1_char and len(rest) >= 2:
                    if rest[0] == p1_char:
                        return "lr"
                    if rest[-1] == p1_char:
                        return "rl"
                if p1_char in ("刀", "刂", "力", "頁", "欠", "隹", "戈", "斤", "邑", "阝", "攴", "攵", "彡", "鳥", "見", "斗", "文", "寸", "殳"):
                    if ch_s not in ("到", "動", "加", "切", "初", "刻", "劇", "劍", "劑", "劉", "剛", "創", "判", "別", "利", "刷", "刺", "前", "則", "削", "剖", "剝", "副", "割", "劃"):
                        pass
                    return "rl"
                return "lr"
            if op == "⿱":
                if "×3" in bdesc_s:
                    if raw_ids.count("⿱") >= 1 and len(raw_ids) >= 5 and len(set(raw_ids[2:])) == 1:
                        return "tri3"
                    return "top_tri3"
                if raw_ids.count("⿱") >= 2 and ch_s in ("尋", "翼", "鼻", "算", "答", "茶", "荷", "落", "意", "愛", "憂", "憲", "塞", "寒", "寨"):
                    return "tmb"
                rest = raw_ids[1:]
                if p1_char and len(rest) >= 2:
                    if rest[0] == p1_char:
                        return "tb"
                    if rest[-1] == p1_char:
                        return "bt"
                if p1_char in ("心", "灬", "皿", "土", "衣", "木", "貝", "目", "石", "馬", "魚", "鳥", "虫", "金", "食", "女", "子", "手", "足", "糸") and p1_char not in ("艹", "竹", "雨", "穴", "宀", "亠", "立", "合", "日", "爪", "爫", "山", "尸", "广", "疒", "虍", "門", "鬥"):
                    return "bt"
                return "tb"
            if op == "⿲":
                return "lmr"
            if op == "⿳":
                return "tmb"
            if op == "⿴":
                return "full_sur"
            if op == "⿵":
                return "top3"
            if op == "⿶":
                return "bot3"
            if op == "⿷":
                return "left3_tri3" if "×3" in bdesc_s else "left3"
            if op == "⿸":
                return "tl_sur"
            if op == "⿹":
                return "tr_sur"
            if op == "⿺":
                return "bl_sur"
            return "single"

        # 完整收錄全部龍龕單字（不設人為截斷上限，確保任何常用字與罕用字皆可於互動網頁即時查碼）
        ranked_chars = sorted(char_all.items(), key=lambda kv: -max(x[0] for x in kv[1]))
        for ch, entries in ranked_chars:
            entries_sorted = sorted(entries, key=lambda x: (-x[0], x[1]))
            _, best_code, best_bdesc, best_kind = entries_sorted[0]
            best_cat = alt_category(best_code, best_bdesc, best_kind)
            seen_codes = {best_code}
            seen_stems = {code_stem(best_code, best_bdesc)}
            dedup_alts = []
            for sc, cd, bd, kd in entries_sorted[1:]:
                st = code_stem(cd, bd)
                if cd not in seen_codes and st not in seen_stems:
                    seen_codes.add(cd)
                    seen_stems.add(st)
                    dedup_alts.append((sc, cd, bd, kd, alt_category(cd, bd, kd)))
            # 龍龕 2.0 四軌並陳：優先確保「軌道 H (許氏全拼)」、「軌道 P (拼音全拼)」、「軌道 V (徐冰純視覺字母)」、「軌道 A/C (雙拼簡碼/部件直拆/本音)」各具代表
            chosen_alts = []
            used_cats = {best_cat}
            priority_cats = ["hsu_full", "pinyin_full", "visual_4", "rad_22", "comp_direct", "phon_self", "hsu", "first_last", "pict"]
            for p_cat in priority_cats:
                if len(chosen_alts) >= 4:
                    break
                if p_cat in used_cats:
                    continue
                for item in dedup_alts:
                    if item[4] == p_cat and item not in chosen_alts:
                        used_cats.add(p_cat)
                        chosen_alts.append(item)
                        break
            for item in dedup_alts:
                if len(chosen_alts) >= 4:
                    break
                if item not in chosen_alts and item[4] not in used_cats:
                    used_cats.add(item[4])
                    chosen_alts.append(item)
            for item in dedup_alts:
                if len(chosen_alts) >= 4:
                    break
                if item not in chosen_alts:
                    chosen_alts.append(item)
            alts_str = "｜".join(f"{cd}={bd}" for _, cd, bd, _, _ in chosen_alts)
            struct_code = classify_char_structure(ch, best_bdesc)
            longkan_db[ch] = [best_code, best_bdesc, alts_str, struct_code]
            for s_part in (best_bdesc, alts_str):
                for c_char in s_part:
                    if "\u4e00" <= c_char <= "\u9fff" and c_char not in cross_db and c_char in bpm_primary:
                        cross_db[c_char] = [
                            ar30_primary.get(c_char, ""),
                            "·".join(ar30_extra.get(c_char, [])[:2]),
                            cj_primary.get(c_char, ""),
                            bpm_primary.get(c_char, ""),
                        ]

    return cross_db, longkan_db


HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="zh-TW">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>如喜（RuhI）— 跨輸入法通用反查教練 ＆ 互動拆碼教學工作台</title>
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

  .ime-mode-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 10px;
    margin-bottom: 10px;
    padding: 8px 12px;
    background: #131b29;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .ime-mode-btn {
    background: #1e293b;
    color: var(--text-main);
    border: 1px solid var(--border);
    padding: 5px 11px;
    border-radius: 8px;
    font-size: 0.82rem;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.18s;
  }
  .ime-mode-btn:hover { border-color: var(--cyan); color: #fff; }
  .ime-mode-btn.active {
    background: #0284c7;
    color: #fff;
    border-color: var(--gold);
    box-shadow: 0 0 10px rgba(56, 189, 248, 0.35);
  }
  .cross-ime-matrix {
    background: #111927;
    border: 1px solid rgba(56, 189, 248, 0.4);
    border-radius: 10px;
    padding: 12px 14px;
    margin-bottom: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cim-row {
    display: grid;
    grid-template-columns: 185px 1fr;
    gap: 10px;
    align-items: start;
    padding: 8px 10px;
    border-radius: 8px;
    background: rgba(30, 41, 59, 0.55);
    font-size: 0.85rem;
  }
  .cim-row.highlight-ime {
    background: rgba(2, 132, 199, 0.18);
    border: 1px solid var(--gold);
  }
  .cim-name { font-weight: 800; color: var(--gold); padding-top: 4px; }
  .cim-val { color: var(--text-bright); font-family: "Consolas", "Microsoft JhengHei", monospace; word-break: break-word; display: flex; flex-direction: column; gap: 6px; }
  /* 逐字獨立成行 + 相容碼思考推論引導列 */
  .cim-char-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
    width: 100%;
  }
  .cim-char-line {
    display: grid;
    grid-template-columns: 44px minmax(195px, 240px) 1fr;
    gap: 10px;
    align-items: start;
    background: #0f172a;
    border: 1px solid rgba(61, 78, 109, 0.85);
    border-left: 4px solid var(--cyan);
    border-radius: 8px;
    padding: 8px 12px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .cim-char-line:hover {
    border-color: var(--gold);
    background: #17253b;
    transform: translateX(2px);
  }
  @media (max-width: 860px) {
    .cim-char-line { grid-template-columns: 42px 1fr; }
    .cim-char-reasoning { grid-column: 1 / -1; }
  }
  .cim-char-glyph {
    font-size: 1.45rem;
    font-weight: 900;
    color: var(--text-bright);
    background: #1e293b;
    border: 1px solid var(--border);
    border-radius: 7px;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .cim-char-primary {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .cim-main-code {
    font-family: "Consolas", monospace;
    font-size: 1.08rem;
    font-weight: 900;
    color: var(--gold);
    background: #1e293b;
    padding: 1px 8px;
    border-radius: 5px;
    border: 1px solid rgba(250, 204, 21, 0.45);
    display: inline-block;
  }
  .cim-main-desc {
    font-size: 0.82rem;
    color: var(--cyan);
    font-weight: 700;
  }
  .cim-char-reasoning {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.8rem;
    border-left: 1px dashed rgba(148, 163, 184, 0.3);
    padding-left: 10px;
  }
  .reason-item {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px;
    line-height: 1.4;
    background: rgba(30, 41, 59, 0.55);
    padding: 3px 8px;
    border-radius: 6px;
  }
  .reason-tag {
    font-size: 0.74rem;
    font-weight: 800;
    padding: 1px 6px;
    border-radius: 4px;
    white-space: nowrap;
  }
  .rt-sound { background: rgba(52, 211, 153, 0.2); color: var(--green); border: 1px solid rgba(52, 211, 153, 0.45); }
  .rt-shape { background: rgba(56, 189, 248, 0.2); color: var(--cyan); border: 1px solid rgba(56, 189, 248, 0.45); }
  .rt-dup   { background: rgba(250, 204, 21, 0.2); color: var(--gold); border: 1px solid rgba(250, 204, 21, 0.45); }
  .rt-hsu   { background: rgba(167, 139, 250, 0.2); color: var(--purple); border: 1px solid rgba(167, 139, 250, 0.45); }
  .rt-vis   { background: rgba(244, 114, 182, 0.2); color: var(--pink); border: 1px solid rgba(244, 114, 182, 0.45); }
  .reason-code {
    font-family: "Consolas", monospace;
    font-weight: 900;
    color: var(--gold);
    font-size: 0.9rem;
  }
  .reason-explain {
    color: var(--text-main);
    font-family: "Microsoft JhengHei", sans-serif;
  }
  .guide-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .guide-rule-box {
    background: #131c2b;
    border: 1px solid var(--border);
    border-left: 4px solid var(--cyan);
    border-radius: 8px;
    padding: 9px 12px;
    font-size: 0.83rem;
  }
  .guide-rule-box strong { color: var(--gold); }
  .cdc-cross-ime {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px dashed rgba(148, 163, 184, 0.28);
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.77rem;
  }
  .cdc-ime-row {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    color: var(--text-muted);
  }
  .cdc-ime-row strong { color: var(--text-bright); font-family: "Consolas", "Microsoft JhengHei", monospace; }

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
  .canvas-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .struct-type-pill {
    width: 256px;
    background: linear-gradient(90deg, #17253b 0%, #1e293b 100%);
    border: 1px solid rgba(56, 189, 248, 0.45);
    border-radius: 9px;
    padding: 4px 10px;
    font-size: 0.78rem;
    font-weight: 800;
    color: var(--cyan);
    text-align: center;
    letter-spacing: 0.3px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  }
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
  .char-canvas[data-struct="four_corner"]::before,
  .char-canvas[data-struct="four_corner"]::after {
    content: "";
    position: absolute;
    background: rgba(148, 163, 184, 0.14);
  }
  .char-canvas[data-struct="four_corner"]::before { width: 100%; height: 1px; top: 50%; left: 0; }
  .char-canvas[data-struct="four_corner"]::after { height: 100%; width: 1px; left: 50%; top: 0; }
  .hl-svg-overlay {
    position: absolute;
    inset: 0;
    width: 256px;
    height: 256px;
    z-index: 4;
    pointer-events: none;
    overflow: visible;
  }
  @keyframes svgZonePulse {
    0%, 100% { fill-opacity: 0.22; stroke-width: 3.2px; }
    50% { fill-opacity: 0.34; stroke-width: 4.2px; }
  }
  .svg-hl-shape {
    animation: svgZonePulse 1.8s ease-in-out infinite;
    transition: all 0.28s ease;
  }
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
      <h1><span class="badge-four" id="headerBadge">龍2.0</span> <span id="headerTitleText">如喜（RuhI）— 跨輸入法通用反查教練 ＆ 互動拆碼教學工作台</span></h1>
      <p id="headerSubText">內建完整 545,778 筆<strong>龍龕形音 2.0 四軌詞庫</strong> ＋ 76,299 筆四角混合構詞碼表 ＋ <strong>全系統 7 大輸入法雙向反查與相容碼思考推論引擎</strong>！</p>
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
      <div id="headerMotto" style="font-size:0.82rem; color:var(--text-muted);">
        切換下方「跨輸入法反查視角」，全頁即刻純淨化為該輸入法專屬拆碼教學（絕不混雜其他方案干擾資訊）
      </div>
    </div>
  </header>

  <div class="workbench-layout">
    <!-- 左主欄：🔍 整串文字／句子即時查詢器與逐字字卡 -->
    <div class="lookup-col">
      <div class="lookup-panel" style="margin-bottom:0;">
        <div class="lookup-top" style="justify-content:space-between;">
          <div class="lookup-title" id="lookupTitleText">🔍 跨輸入法整句即時反查 ＆ 相容碼思考推論教練（點擊任一字列或字卡立即播放拆解動畫）</div>
          <span class="now-playing-pill" id="nowPlayingBadge">🎬 舞台播放中：單元 1</span>
        </div>
        <div class="lookup-input-row">
          <input type="text" id="lookupInput" class="lookup-input" value="龍龕形音輸入法" placeholder="在此輸入或貼上想查詢的單字、詞彙或整串句子（例如：龍龕形音輸入法、台灣）..." oninput="analyzeFullText()" />
          <button class="ctrl-btn primary" onclick="playLookupSequenceAnim()" style="font-size:0.95rem; padding:10px 18px;">▶ 依序連播整句打字動畫</button>
        </div>
        <div class="preset-chips" id="presetChips"></div>

        <!-- 通用可擴展跨輸入法反查教練模式切換列 -->
        <div class="ime-mode-bar" id="imeModeBar">
          <span style="font-size:0.82rem; font-weight:800; color:var(--gold);">🎯 選擇單一輸入法教學視角（純淨無干擾）：</span>
          <button class="ime-mode-btn" data-ime="longkan" onclick="setCrossImeMode('longkan')">🐉 龍龕形音 2.0 (四軌+許氏協同)</button>
          <button class="ime-mode-btn" data-ime="four_corner" onclick="setCrossImeMode('four_corner')">🔢 智慧四角號碼 (4×5)</button>
          <button class="ime-mode-btn" data-ime="array30" onclick="setCrossImeMode('array30')">📐 行列 30 (筆形座標)</button>
          <button class="ime-mode-btn" data-ime="cangjie" onclick="setCrossImeMode('cangjie')">🔤 倉頡 (哲理字根)</button>
          <button class="ime-mode-btn" data-ime="hsu" onclick="setCrossImeMode('hsu')">🎵 許氏雙拼注音</button>
          <button class="ime-mode-btn" data-ime="standard" onclick="setCrossImeMode('standard')">🎹 標準大千 / 倚天26鍵</button>
          <button class="ime-mode-btn active" data-ime="all" onclick="setCrossImeMode('all')">🔄 全輸入法同屏對照 (7合1)</button>
        </div>

        <!-- 逐字獨立成行 + 相容碼思考推論引導面板 -->
        <div class="cross-ime-matrix" id="crossImeMatrix"></div>

        <!-- A. 整串最速連打推薦序列 -->
        <div class="combo-banner" id="comboBanner"></div>

        <!-- B. 逐字拆解詳情卡片 -->
        <div class="char-cards-grid" id="charCardsGrid"></div>
      </div>
    </div>

    <!-- 右主欄（寬螢幕 Sticky 常駐釘選）：單元頁籤 ＋ 動畫舞台 ＋ 專屬互動卡 ＋ 底部速查表 -->
    <div class="stage-col" id="stageAnchor">
      <!-- 單元切換頁籤（隨當前選取的輸入法動態切換） -->
      <div class="lesson-tabs" id="lessonTabs"></div>

      <div class="main-grid">
        <!-- 左側動畫舞台與模擬候選窗 -->
        <div class="stage-card" id="stageCard">
      <div class="stage-top">
        <div class="canvas-col">
          <div class="struct-type-pill" id="structTypePill">💠 結構拓撲分析</div>
          <div class="char-canvas" id="charCanvas" data-struct="single">
            <svg class="hl-svg-overlay" id="hlSvg" viewBox="0 0 256 256" aria-hidden="true"></svg>
            <div class="spot-tl corner-spot" id="spot0">①首部</div>
            <div class="spot-tr corner-spot" id="spot1">②次部</div>
            <div class="spot-bl corner-spot" id="spot2">③相容軌</div>
            <div class="spot-br corner-spot" id="spot3">④推論法</div>
            <div class="highlight-zone" id="hlZone"></div>
            <div class="big-char" id="stageChar">龍</div>
          </div>
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
            <button class="ctrl-btn" onclick="clearSandbox()">🧹 重置舞台</button>
            <span style="font-size:0.8rem; color:var(--cyan); margin-left:auto;">
              💡 點擊左側任一字列或字卡即可播放該字拆解！
            </span>
          </div>
        </div>
      </div>

      <!-- 模擬如喜 TSF 懸浮候選窗 -->
      <div class="tsf-window">
        <div class="tsf-header">
          <div class="tsf-code-box">
            <span class="badge-four" id="tsfBadgeIcon" style="font-size:0.95rem; padding:2px 8px;">龍</span>
            <span id="tsfCodeLabel">輸入碼：</span>
            <span class="tsf-code" id="tsfCode">lolo</span>
          </div>
          <div class="tsf-status" id="tsfStatus">龍龕形音 4 碼模式</div>
          <div style="display:flex; align-items:center; gap:6px;" id="tsfPagerBox">
            <span id="tsfPageInfo" style="font-size:0.8rem; color:var(--cyan);">第 1/1 頁</span>
            <button class="ctrl-btn" style="padding:3px 8px; font-size:0.78rem;" onclick="pressVirtualKey('PageUp')" title="翻上一頁">◀ 上頁</button>
            <button class="ctrl-btn" style="padding:3px 8px; font-size:0.78rem;" onclick="pressVirtualKey('PageDown')" title="翻下一頁">下頁 ▶</button>
          </div>
        </div>
        <div style="display:grid; grid-template-columns:1fr 1fr; gap:10px; margin-bottom:10px; background:#0f172a; padding:8px 12px; border-radius:8px; border:1px solid var(--border);">
          <div style="font-size:0.88rem; color:var(--text-main);">
            <span id="composeDeskHint">📝 如喜輸入座組句區：</span><strong id="composeDeskOut" style="color:var(--cyan); font-size:1.08rem;">（空白）</strong>
          </div>
          <div style="font-size:0.88rem; color:var(--text-main);">
            📤 目標視窗已送出：<strong id="committedOut" style="color:var(--gold); font-size:1.08rem;">（尚無）</strong>
          </div>
        </div>
        <div class="tsf-candidates" id="tsfCandidates"></div>
      </div>
    </div>

    <!-- 右側卡片 A：龍龕形音 2.0「四軌合一 ＆ 許氏雙向協同」思考推論法則卡（選擇龍龕形音時顯示） -->
    <div class="guide-card" id="longkanGuideCard" style="display:none;">
      <div class="numpad-title" style="margin-bottom:4px;">
        <span>🐉 龍龕形音 2.0「四軌合一 ＆ 許氏雙向協同」核心架構</span>
      </div>
      <div style="font-size:0.79rem; color:var(--text-muted); margin-bottom:4px;">
        <strong>免死背縮碼、可變碼長（2~12鍵）</strong>：前中部件免打聲調，字尾按<strong>【空白鍵 (一聲/通配)】</strong>或<strong>【聲調鍵 (1~5 或 許氏 d/f/j/s)】</strong>定界上屏：
      </div>
      <div class="guide-rule-box" style="border-left-color:var(--cyan);">
        <strong>🎵 軌道 H ＆ 🌐 軌道 P：許氏／拼音「部件全注音免縮碼」直打</strong><br>
        直接把部件完整注音（許氏或漢語/通用拼音）連打即可！例：<strong>鋪</strong>＝金(<code>jen</code>)+甫(<code>fx</code>)➔<code>jenfx</code>（拼音 <code>jinfu</code>）；<strong>龕</strong>＝合(<code>hg</code>)+龍(<code>lxl</code>)➔<code>hglxl</code>（拼音 <code>helong</code>）；三部件字如<strong>謝</strong>可全打 <code>emcnam</code>（<code>yanshencun</code>）或首尾 <code>emam</code>（<code>yancun</code>）！
      </div>
      <div class="guide-rule-box" style="border-left-color:var(--green);">
        <strong>👁️ 軌道 V：徐冰《英文方塊字》概念之「純視覺英文字母軌」（外國人零讀音友善）</strong><br>
        完全不需懂漢字讀音！依書寫筆順（先上後下、先左後右、先外後內）看形狀打英文字母（<code>O</code>=口、<code>B</code>=日/目/田/貝、<code>D</code>=月、<code>T</code>=十/土/木/艹、<code>A</code>=人/大/金、<code>X</code>=乂/女、<code>I</code>=丨）：例 <strong>中</strong>=<code>oi</code>、<strong>品</strong>=<code>ooo</code>、<strong>古</strong>=<code>tio</code>、<strong>叶</strong>=<code>oti</code>、<strong>明</strong>=<code>bode</code>、<strong>好</strong>=<code>xoje</code>、<strong>休</strong>=<code>atx</code>、<strong>森</strong>=<code>ttt</code>！
      </div>
      <div class="guide-rule-box" style="border-left-color:var(--gold);">
        <strong>🔄 許氏注音 ⇄ 龍龕 2.0 雙向協同 ＆ 事後合體熔煉（Shift+← + Tab）</strong><br>
        ① 許氏注音中單按 <strong>【<code>`</code>】</strong> 臨時插打 1 個龍龕字即自動返回許氏；單按 <strong>【<code>~</code>】</strong> 切換許氏 ⇄ 龍龕 2.0 模式。<br>
        ② <strong>事後合體熔煉</strong>：在許氏注音已打出部件音（如 <code>ㄏㄜ</code>+<code>ㄌㄨㄥ</code> 或 <code>ㄐㄧˊ</code>+<code>ㄐㄧˊ</code>），按 <strong>【Shift + ←】</strong> 反白選取後按 <strong>【Tab】</strong>，即刻合體熔煉為「<strong>龕</strong>」、「<strong>喆</strong>」、「<strong>謝</strong>」！
      </div>
      <div class="guide-rule-box" style="border-left-color:var(--purple);">
        <strong>⚡ 軌道 A/C 傳統 4 碼相容 · <code>``</code> 財務大寫快打 · <code>~~</code> 專業詞庫切換</strong><br>
        完整向下相容 2+2 雙拼簡碼（如 <code>jnin</code>/<code>holn</code>）與重複部件 <code>C×N</code>（<code>ko3</code>/<code>mu3</code>）；連按兩下 <strong>【<code>``</code>】</strong> 輸入數字（如 <code>``12345</code>）按 <code>Q</code> 切換中文數字／財務大寫；連按兩下 <strong>【<code>~~</code>】</strong> 切換 5 大專業領域詞庫！
      </div>
    </div>

    <!-- 右側卡片 B：其他單一輸入法（行列30 / 倉頡 / 許氏 / 大千）思考推論卡片 -->
    <div class="guide-card" id="otherImeGuideCard" style="display:none;"></div>

    <!-- 右側卡片 C：4x5 實體數字小鍵盤（僅在智慧四角號碼或全方案對照時顯示） -->
    <div class="numpad-card" id="numpadCard">
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

  <!-- 下方速查卡 A：龍龕形音 2.0 四軌合一 ＆ 許氏雙向協同速查表（選擇龍龕形音時顯示） -->
  <div class="rhyme-section" id="lkRhymeSection" style="display:none;">
    <div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap;">
      <strong style="color:var(--gold);">📐 龍龕形音 2.0 四軌合一 ＆ 許氏協同速查表（點擊任一範例自動帶入查詢並播放拆解）</strong>
      <span style="font-size:0.84rem; color:var(--text-muted);">心法：軌道 H 許氏部件全拼 ｜ 軌道 P 拼音部件全拼 ｜ 軌道 V 徐冰式純視覺字母 ｜ 軌道 A/C 雙拼簡碼與 C×N ｜ 空白鍵或聲調鍵定界</span>
    </div>
    <div class="rhyme-grid" style="grid-template-columns: repeat(5, 1fr);">
      <div class="rhyme-box" onclick="setLookupText('鋪銀銅鐵鎮')"><div class="r-num">jen / jin</div><div class="r-name">釒(金) 軌道H/P</div><div class="r-shape">許氏 jen · 拼音 jin · 雙拼 jn/km</div><div class="r-ex">鋪 jenfx / jinfu / jnfx</div></div>
      <div class="rhyme-box" onclick="setLookupText('龕合龍喆喜謝')"><div class="r-num">H / P 全拼</div><div class="r-name">免縮碼部件直拼</div><div class="r-shape">前中免調＋尾按調/空白</div><div class="r-ex">龕 hglxl · 謝 emcnam</div></div>
      <div class="rhyme-box" onclick="setLookupText('中品古叶杏呆回困')"><div class="r-num">O = 口/囗</div><div class="r-name">軌道V 視覺方框</div><div class="r-shape">O=口 · I=丨 · T=十/木</div><div class="r-ex">中 oi · 品 ooo · 古 tio</div></div>
      <div class="rhyme-box" onclick="setLookupText('明早旦唱看休好林森')"><div class="r-num">B·D·T·A·X</div><div class="r-name">軌道V 視覺字根</div><div class="r-shape">B=日/目 D=月 T=木 A=人 X=女</div><div class="r-ex">明 bode · 好 xoje · 休 atx</div></div>
      <div class="rhyme-box" onclick="setLookupText('灣海河法清鴻')"><div class="r-num">cui / shui</div><div class="r-name">氵(水) 軌道H/P</div><div class="r-shape">許氏 cui · 拼音 shui · 雙拼 su/se</div><div class="r-ex">法 cuifa / shuifa / seci</div></div>
      <div class="rhyme-box" onclick="setLookupText('語話說講論謝謹')"><div class="r-num">em / yan</div><div class="r-name">言(訁) 軌道H/P</div><div class="r-shape">許氏 em · 拼音 yan · 雙拼 yn</div><div class="r-ex">謝 emcnam / yanshencun</div></div>
      <div class="rhyme-box" onclick="setLookupText('林森機權橋杏呆')"><div class="r-num">mx / mu / T</div><div class="r-name">木部 四軌互通</div><div class="r-shape">許氏 mx · 拼音 mu · 視覺 T</div><div class="r-ex">森 mumu/mu3/ttt · 杏 mxko/txo</div></div>
      <div class="rhyme-box" onclick="setLookupText('凶區凹凸亞网')"><div class="r-num">X / C / H</div><div class="r-name">包圍與外框象形</div><div class="r-shape">外框字母＋內部部件</div><div class="r-ex">區 cko3/cooo · 凶 xu1/ux</div></div>
      <div class="rhyme-box" onclick="setLookupText('品森晶淼矗協磊蕊')"><div class="r-num">C × N / V</div><div class="r-name">重複部件三疊字</div><div class="r-shape">部件×3 或 視覺字母連打</div><div class="r-ex">品 ko3/ooo · 森 mu3/ttt</div></div>
      <div class="rhyme-box" onclick="setLookupText('龕喆謝喜鋪明看')"><div class="r-num">` / ~ / Tab</div><div class="r-name">許氏 ⇄ 龍龕協同</div><div class="r-shape">單按 ` 插字 · Shift+←+Tab 熔煉</div><div class="r-ex">ㄏㄜ+ㄌㄨㄥ ➔ Tab 合體「龕」</div></div>
    </div>
  </div>

  <!-- 下方速查卡 B：0~9 筆形口訣速查互動卡（僅在智慧四角號碼或全方案對照時顯示） -->
  <div class="rhyme-section" id="fcRhymeSection">
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

const CROSS_IME_DB = __CROSS_IME_DB_JSON__;
const LONGKAN_DB = __LONGKAN_DB_JSON__;

const ARRAY30_COORDS = {
  "q":"1^","a":"1-","z":"1v","w":"2^","s":"2-","x":"2v","e":"3^","d":"3-","c":"3v","r":"4^","f":"4-","v":"4v",
  "t":"5^","g":"5-","b":"5v","y":"6^","h":"6-","n":"6v","u":"7^","j":"7-","m":"7v","i":"8^","k":"8-",",":"8v",
  "o":"9^","l":"9-",".":"9v","p":"0^",";":"0-","/":"0v"
};

const CANGJIE_RADS = {
  "a":"日","b":"月","c":"金","d":"木","e":"水","f":"火","g":"土","h":"竹","i":"戈","j":"十","k":"大","l":"中","m":"一",
  "n":"弓","o":"人","p":"心","q":"手","r":"口","s":"尸","t":"廿","u":"山","v":"女","w":"田","x":"難","y":"卜","z":"重"
};

function bopomofoToHsu(bpm) {
  if (!bpm) return "";
  let s = bpm.trim(), tone = "␣";
  if (s.includes("˙")) { tone = "S"; s = s.replace(/˙/g, ""); }
  else if (s.endsWith("ˊ")) { tone = "D"; s = s.slice(0, -1); }
  else if (s.endsWith("ˇ")) { tone = "F"; s = s.slice(0, -1); }
  else if (s.endsWith("ˋ")) { tone = "J"; s = s.slice(0, -1); }
  const map = {"ㄅ":"B","ㄆ":"P","ㄇ":"M","ㄈ":"F","ㄉ":"D","ㄊ":"T","ㄋ":"N","ㄌ":"L","ㄍ":"G","ㄎ":"K","ㄏ":"H","ㄐ":"J","ㄑ":"V","ㄒ":"C","ㄓ":"J","ㄔ":"V","ㄕ":"C","ㄖ":"R","ㄗ":"Z","ㄘ":"A","ㄙ":"S","ㄧ":"E","ㄨ":"X","ㄩ":"U","ㄚ":"Y","ㄛ":"H","ㄜ":"G","ㄝ":"E","ㄞ":"I","ㄟ":"A","ㄠ":"W","ㄡ":"O","ㄢ":"M","ㄣ":"N","ㄤ":"K","ㄥ":"L","ㄦ":"L"};
  return Array.from(s).map(c => map[c] || "").join("") + tone;
}

function bopomofoToStandard(bpm) {
  if (!bpm) return "";
  let s = bpm.trim(), tone = "␣";
  if (s.includes("˙")) { tone = "7"; s = s.replace(/˙/g, ""); }
  else if (s.endsWith("ˊ")) { tone = "6"; s = s.slice(0, -1); }
  else if (s.endsWith("ˇ")) { tone = "3"; s = s.slice(0, -1); }
  else if (s.endsWith("ˋ")) { tone = "4"; s = s.slice(0, -1); }
  const map = {"ㄅ":"1","ㄆ":"Q","ㄇ":"A","ㄈ":"Z","ㄉ":"2","ㄊ":"W","ㄋ":"S","ㄌ":"X","ㄍ":"E","ㄎ":"D","ㄏ":"C","ㄐ":"R","ㄑ":"F","ㄒ":"V","ㄓ":"5","ㄔ":"T","ㄕ":"G","ㄖ":"B","ㄗ":"Y","ㄘ":"H","ㄙ":"N","ㄧ":"U","ㄨ":"J","ㄩ":"M","ㄚ":"8","ㄛ":"I","ㄜ":"K","ㄝ":",","ㄞ":"9","ㄟ":"O","ㄠ":"L","ㄡ":".","ㄢ":"0","ㄣ":"P","ㄤ":";","ㄥ":"/","ㄦ":"-"};
  return Array.from(s).map(c => map[c] || "").join("") + tone;
}

function bopomofoToEt26(bpm) {
  if (!bpm) return "";
  let s = bpm.trim(), tone = "␣";
  if (s.includes("˙")) { tone = "Q"; s = s.replace(/˙/g, ""); }
  else if (s.endsWith("ˊ")) { tone = "F"; s = s.slice(0, -1); }
  else if (s.endsWith("ˇ")) { tone = "J"; s = s.slice(0, -1); }
  else if (s.endsWith("ˋ")) { tone = "K"; s = s.slice(0, -1); }
  const map = {"ㄅ":"B","ㄆ":"P","ㄇ":"M","ㄈ":"F","ㄉ":"D","ㄊ":"T","ㄋ":"N","ㄌ":"L","ㄍ":"V","ㄎ":"K","ㄏ":"H","ㄐ":"G","ㄑ":"C","ㄒ":"X","ㄓ":"J","ㄔ":"Q","ㄕ":"W","ㄖ":"R","ㄗ":"Z","ㄘ":"A","ㄙ":"S","ㄧ":"E","ㄨ":"U","ㄩ":"I","ㄚ":"A","ㄛ":"O","ㄜ":"R","ㄝ":"W","ㄞ":"I","ㄟ":"S","ㄠ":"Z","ㄡ":"X","ㄢ":"M","ㄣ":"N","ㄤ":"T","ㄥ":"L","ㄦ":"H"};
  return Array.from(s).map(c => map[c] || "").join("") + tone;
}

const COMP_BPM_FALLBACK = {
  "彡":"ㄕㄢ","艹":"ㄘㄠˇ","艸":"ㄘㄠˇ","氵":"ㄕㄨㄟˇ","扌":"ㄕㄡˇ","忄":"ㄒㄧㄣ",
  "凵":"ㄎㄢˇ","匚":"ㄈㄤ","冂":"ㄐㄩㄥ","乂":"ㄧˋ","艮":"ㄍㄣˋ","辵":"ㄗㄡˇ","辶":"ㄗㄡˇ",
  "阜":"ㄈㄨˋ","邑":"ㄧˋ","宀":"ㄇㄧㄢˊ","广":"ㄍㄨㄤˇ","彳":"ㄔˋ","夂":"ㄓˇ",
  "夊":"ㄙㄨㄟ","廾":"ㄍㄨㄥˇ","弋":"ㄧˋ","彐":"ㄐㄧˋ","攴":"ㄆㄨ","攵":"ㄆㄨ",
  "无":"ㄨˊ","毋":"ㄨˊ","歹":"ㄉㄞˇ","殳":"ㄕㄨ","氏":"ㄕˋ","气":"ㄑㄧˋ",
  "爻":"ㄧㄠˊ","爿":"ㄅㄢˋ","牜":"ㄋㄧㄡˊ","犭":"ㄑㄩㄢˇ","礻":"ㄕˋ","禸":"ㄖㄡˊ",
  "糹":"ㄙ","糸":"ㄙ","罒":"ㄙˋ","耂":"ㄌㄠˇ","聿":"ㄩˋ","虍":"ㄏㄨˇ",
  "衤":"ㄧ","覀":"ㄒㄧ","訁":"ㄧㄢˊ","豸":"ㄓˋ","釒":"ㄐㄧㄣ","隹":"ㄓㄨㄟ",
  "頁":"ㄧㄝˋ","飠":"ㄕˊ","髟":"ㄅㄧㄠ","鬯":"ㄔㄤˋ","鬲":"ㄍㄜˊ","鹵":"ㄌㄨˇ",
  "黹":"ㄓˇ","黽":"ㄇㄧㄣˇ","龠":"ㄩㄝˋ","幺":"ㄧㄠ","么":"ㄧㄠ","虫":"ㄔㄨㄥˊ",
  "肉":"ㄖㄡˋ","行":"ㄒㄧㄥˊ","長":"ㄔㄤˊ","镸":"ㄔㄤˊ","子":"ㄗˇ","重":"ㄓㄨㄥˋ",
  "便":"ㄅㄧㄢˋ","乘":"ㄔㄥˊ","曾":"ㄘㄥˊ","和":"ㄏㄜˊ","怎":"ㄗㄣˇ","蘋":"ㄆㄧㄥˊ",
  "怔":"ㄓㄥ","括":"ㄎㄨㄛˋ","賜":"ㄘˋ","勹":"ㄅㄛ","𠂊":"ㄅㄛ","𠫓":"ㄊㄜ",
  "丂":"ㄎㄜ","厂":"ㄏㄜ","丩":"ㄐㄧ","卩":"ㄗ","㔾":"ㄗ","厶":"ㄙ","凵":"ㄩ",
  "乚":"ㄣ","𠃌":"ㄣ","尢":"ㄤ","儿":"ㄦ"
};

const COMP_CODE_BPM_MAP = {
  "幺:yo":"ㄧㄠ","幺:ya":"ㄧㄠ","幺:ew":"ㄧㄠ","幺:ao":"ㄠ","幺:a1":"ㄠ","幺:aa":"ㄠ","幺:a":"ㄠ","幺:ww":"ㄠ","幺:me":"ㄇㄜ˙","幺:ma":"ㄇㄚˊ",
  "么:yo":"ㄧㄠ","么:ya":"ㄧㄠ","么:ew":"ㄧㄠ","么:ao":"ㄠ","么:a1":"ㄠ","么:aa":"ㄠ","么:a":"ㄠ","么:ww":"ㄠ","么:me":"ㄇㄜ˙","么:mg":"ㄇㄜ˙","么:ma":"ㄇㄚˊ",
  "勹:bo":"ㄅㄛ","勹:b1":"ㄅ","勹:bb":"ㄅ","勹:b":"ㄅ",
  "𠫓:te":"ㄊㄜ","𠫓:t1":"ㄊ","𠫓:tt":"ㄊ","𠫓:t":"ㄊ",
  "丂:ke":"ㄎㄜ","丂:ko":"ㄎㄠˇ","丂:k1":"ㄎ","丂:kk":"ㄎ","丂:k":"ㄎ",
  "厂:he":"ㄏㄜ","厂:hn":"ㄏㄢˇ","厂:h1":"ㄏ","厂:hh":"ㄏ","厂:h":"ㄏ",
  "丩:ji":"ㄐㄧ","丩:j1":"ㄐ","丩:jj":"ㄐ","丩:j":"ㄐ",
  "卩:zi":"ㄗ","卩:z1":"ㄗ","卩:zz":"ㄗ","卩:z":"ㄗ","卩:je":"ㄐㄧㄝˊ",
  "厶:si":"ㄙ","厶:s1":"ㄙ","厶:ss":"ㄙ","厶:s":"ㄙ",
  "凵:yu":"ㄩ","凵:v1":"ㄩ","凵:vv":"ㄩ","凵:v":"ㄩ","凵:kn":"ㄎㄢˇ",
  "乚:en":"ㄣ","乚:n1":"ㄣ","乚:nn":"ㄣ","乚:n":"ㄣ","乚:yi":"ㄧˇ",
  "尢:an":"ㄤ","尢:wn":"ㄨㄤ","尢:yo":"ㄧㄡˊ","尢:a1":"ㄤ","尢:aa":"ㄤ","尢:a":"ㄤ",
  "儿:er":"ㄦ","儿:r1":"ㄦ","儿:rr":"ㄦ","儿:r":"ㄦ","儿:rn":"ㄖㄣˊ",
  "糸:mi":"ㄇㄧˋ","糸:me":"ㄇㄧˋ","糸:si":"ㄙ","糸:ss":"ㄙ",
  "重:zn":"ㄓㄨㄥˋ","重:jn":"ㄓㄨㄥˋ","重:zo":"ㄓㄨㄥˋ","重:jo":"ㄓㄨㄥˋ","重:jl":"ㄓㄨㄥˋ","重:cn":"ㄔㄨㄥˊ","重:co":"ㄔㄨㄥˊ","重:vl":"ㄔㄨㄥˊ",
  "長:cn":"ㄔㄤˊ","長:vk":"ㄔㄤˊ","長:zn":"ㄓㄤˇ","長:jn":"ㄓㄤˇ","長:jk":"ㄓㄤˇ",
  "行:xn":"ㄒㄧㄥˊ","行:sn":"ㄒㄧㄥˊ","行:cl":"ㄒㄧㄥˊ","行:hn":"ㄏㄤˊ","行:hl":"ㄏㄤˊ",
  "樂:le":"ㄌㄜˋ","樂:lg":"ㄌㄜˋ","樂:ye":"ㄩㄝˋ",
  "乾:gn":"ㄍㄢ","乾:gm":"ㄍㄢ","乾:qn":"ㄑㄧㄢˊ","乾:cn":"ㄑㄧㄢˊ","乾:vm":"ㄑㄧㄢˊ",
  "便:bn":"ㄅㄧㄢˋ","便:bm":"ㄅㄧㄢˋ","便:pn":"ㄆㄧㄢˊ","便:pm":"ㄆㄧㄢˊ",
  "乘:cn":"ㄔㄥˊ","乘:vl":"ㄔㄥˊ","乘:sn":"ㄕㄥˋ","乘:cl":"ㄕㄥˋ",
  "曾:cn":"ㄘㄥˊ","曾:al":"ㄘㄥˊ","曾:zn":"ㄗㄥ","曾:zl":"ㄗㄥ",
  "朝:co":"ㄔㄠˊ","朝:ca":"ㄔㄠˊ","朝:vw":"ㄔㄠˊ","朝:zo":"ㄓㄠ","朝:za":"ㄓㄠ","朝:jw":"ㄓㄠ",
  "攴:pu":"ㄆㄨ","攴:px":"ㄆㄨ","攴:wn":"ㄨㄣˊ","攴:xn":"ㄨㄣˊ"
};

function getComponentBpm(ch, codeLo) {
  if (codeLo && COMP_CODE_BPM_MAP[`${ch}:${codeLo}`]) return COMP_CODE_BPM_MAP[`${ch}:${codeLo}`];
  if (COMP_BPM_FALLBACK[ch]) return COMP_BPM_FALLBACK[ch];
  if (CROSS_IME_DB[ch] && CROSS_IME_DB[ch][3]) return CROSS_IME_DB[ch][3];
  return "";
}

function bopomofoToPinyin(bpm) {
  if (!bpm) return "";
  let s = bpm.trim(), tone = 1;
  if (s.includes("˙")) { tone = 5; s = s.replace(/˙/g, ""); }
  else if (s.endsWith("ˊ")) { tone = 2; s = s.slice(0, -1); }
  else if (s.endsWith("ˇ")) { tone = 3; s = s.slice(0, -1); }
  else if (s.endsWith("ˋ")) { tone = 4; s = s.slice(0, -1); }
  const initMap = {"ㄅ":"b","ㄆ":"p","ㄇ":"m","ㄈ":"f","ㄉ":"d","ㄊ":"t","ㄋ":"n","ㄌ":"l","ㄍ":"g","ㄎ":"k","ㄏ":"h","ㄐ":"j","ㄑ":"q","ㄒ":"x","ㄓ":"zh","ㄔ":"ch","ㄕ":"sh","ㄖ":"r","ㄗ":"z","ㄘ":"c","ㄙ":"s"};
  const finMap = {
    "ㄚ":"a","ㄛ":"o","ㄜ":"e","ㄝ":"e","ㄞ":"ai","ㄟ":"ei","ㄠ":"ao","ㄡ":"ou","ㄢ":"an","ㄣ":"en","ㄤ":"ang","ㄥ":"eng","ㄦ":"er",
    "ㄧ":"i","ㄧㄚ":"ia","ㄧㄛ":"io","ㄧㄝ":"ie","ㄧㄞ":"iai","ㄧㄠ":"iao","ㄧㄡ":"iu","ㄧㄢ":"ian","ㄧㄣ":"in","ㄧㄤ":"iang","ㄧㄥ":"ing",
    "ㄨ":"u","ㄨㄚ":"ua","ㄨㄛ":"uo","ㄨㄞ":"uai","ㄨㄟ":"ui","ㄨㄢ":"uan","ㄨㄣ":"un","ㄨㄤ":"uang","ㄨㄥ":"ong",
    "ㄩ":"ü","ㄩㄝ":"üe","ㄩㄢ":"üan","ㄩㄣ":"ün","ㄩㄥ":"iong"
  };
  let init = "", rest = s;
  if (s.length > 0 && initMap[s[0]]) { init = initMap[s[0]]; rest = s.slice(1); }
  let fin = finMap[rest] || "";
  if (!fin && init) fin = "i";
  let base = init + fin;
  if (!init) {
    if (rest === "ㄧ") base = "yi";
    else if (rest === "ㄧㄣ") base = "yin";
    else if (rest === "ㄧㄥ") base = "ying";
    else if (rest.startsWith("ㄧ")) base = "y" + fin.slice(1);
    else if (rest === "ㄨ") base = "wu";
    else if (rest === "ㄨㄥ") base = "weng";
    else if (rest === "ㄨㄣ") base = "wen";
    else if (rest === "ㄨㄟ") base = "wei";
    else if (rest.startsWith("ㄨ")) base = "w" + fin.slice(1);
    else if (rest === "ㄩ") base = "yu";
    else if (rest === "ㄩㄝ") base = "yue";
    else if (rest === "ㄩㄢ") base = "yuan";
    else if (rest === "ㄩㄣ") base = "yun";
    else if (rest === "ㄩㄥ") base = "yong";
  } else if ((init === "j" || init === "q" || init === "x") && fin.startsWith("ü")) {
    base = init + "u" + fin.slice(1);
  }
  const markMap = {
    "a":["ā","á","ǎ","à","a"],"o":["ō","ó","ǒ","ò","o"],"e":["ē","é","ě","è","e"],
    "i":["ī","í","ǐ","ì","i"],"u":["ū","ú","ǔ","ù","u"],"ü":["ǖ","ǘ","ǚ","ǜ","ü"]
  };
  const tIdx = Math.max(0, Math.min(4, tone - 1));
  if (base.includes("a")) return base.replace("a", markMap["a"][tIdx]);
  if (base.includes("e")) return base.replace("e", markMap["e"][tIdx]);
  if (base.includes("ou")) return base.replace("o", markMap["o"][tIdx]);
  for (let i = base.length - 1; i >= 0; i--) {
    const chV = base[i];
    if (markMap[chV]) return base.slice(0, i) + markMap[chV][tIdx] + base.slice(i + 1);
  }
  return base;
}

function bopomofoToTongyong(bpm) {
  if (!bpm) return "";
  let s = bpm.trim(), tone = 1;
  if (s.includes("˙")) { tone = 5; s = s.replace(/˙/g, ""); }
  else if (s.endsWith("ˊ")) { tone = 2; s = s.slice(0, -1); }
  else if (s.endsWith("ˇ")) { tone = 3; s = s.slice(0, -1); }
  else if (s.endsWith("ˋ")) { tone = 4; s = s.slice(0, -1); }
  const initMap = {"ㄅ":"b","ㄆ":"p","ㄇ":"m","ㄈ":"f","ㄉ":"d","ㄊ":"t","ㄋ":"n","ㄌ":"l","ㄍ":"g","ㄎ":"k","ㄏ":"h","ㄐ":"j","ㄑ":"c","ㄒ":"s","ㄓ":"jh","ㄔ":"ch","ㄕ":"sh","ㄖ":"r","ㄗ":"z","ㄘ":"c","ㄙ":"s"};
  const finMap = {
    "ㄚ":"a","ㄛ":"o","ㄜ":"e","ㄝ":"e","ㄞ":"ai","ㄟ":"ei","ㄠ":"ao","ㄡ":"ou","ㄢ":"an","ㄣ":"en","ㄤ":"ang","ㄥ":"eng","ㄦ":"er",
    "ㄧ":"i","ㄧㄚ":"ia","ㄧㄛ":"io","ㄧㄝ":"ie","ㄧㄞ":"iai","ㄧㄠ":"iao","ㄧㄡ":"iou","ㄧㄢ":"ian","ㄧㄣ":"in","ㄧㄤ":"iang","ㄧㄥ":"ing",
    "ㄨ":"u","ㄨㄚ":"ua","ㄨㄛ":"uo","ㄨㄞ":"uai","ㄨㄟ":"uei","ㄨㄢ":"uan","ㄨㄣ":"un","ㄨㄤ":"uang","ㄨㄥ":"ong",
    "ㄩ":"yu","ㄩㄝ":"yue","ㄩㄢ":"yuan","ㄩㄣ":"yun","ㄩㄥ":"yong"
  };
  let init = "", rest = s;
  if (s.length > 0 && initMap[s[0]]) { init = initMap[s[0]]; rest = s.slice(1); }
  let fin = finMap[rest] || "";
  if (!fin && init) fin = "ih";
  if (init === "f" && rest === "ㄥ") fin = "ong";
  let base = init + fin;
  if (!init) {
    if (rest === "ㄧ") base = "yi";
    else if (rest === "ㄧㄣ") base = "yin";
    else if (rest === "ㄧㄥ") base = "ying";
    else if (rest.startsWith("ㄧ")) base = "y" + fin.slice(1);
    else if (rest === "ㄨ") base = "wu";
    else if (rest === "ㄨㄥ") base = "wong";
    else if (rest === "ㄨㄣ") base = "wun";
    else if (rest === "ㄨㄟ") base = "wei";
    else if (rest.startsWith("ㄨ")) base = "w" + fin.slice(1);
    else if (rest.startsWith("ㄩ")) base = fin;
  }
  const markMap = {
    "a":["ā","á","ǎ","à","a"],"o":["ō","ó","ǒ","ò","o"],"e":["ē","é","ě","è","e"],
    "i":["ī","í","ǐ","ì","i"],"u":["ū","ú","ǔ","ù","u"]
  };
  const tIdx = Math.max(0, Math.min(4, tone - 1));
  if (base.includes("a")) return base.replace("a", markMap["a"][tIdx]);
  if (base.includes("e")) return base.replace("e", markMap["e"][tIdx]);
  if (base.includes("ou")) return base.replace("o", markMap["o"][tIdx]);
  for (let i = base.length - 1; i >= 0; i--) {
    const chV = base[i];
    if (markMap[chV]) return base.slice(0, i) + markMap[chV][tIdx] + base.slice(i + 1);
  }
  return base;
}

const HSU_KEY_MAP = {"ㄅ":"b","ㄆ":"p","ㄇ":"m","ㄈ":"f","ㄉ":"d","ㄊ":"t","ㄋ":"n","ㄌ":"l","ㄍ":"g","ㄎ":"k","ㄏ":"h","ㄐ":"j","ㄑ":"v","ㄒ":"c","ㄓ":"j","ㄔ":"v","ㄕ":"c","ㄖ":"r","ㄗ":"z","ㄘ":"a","ㄙ":"s","ㄧ":"e","ㄨ":"x","ㄩ":"u","ㄚ":"y","ㄛ":"h","ㄜ":"g","ㄝ":"e","ㄞ":"i","ㄟ":"a","ㄠ":"w","ㄡ":"o","ㄢ":"m","ㄣ":"n","ㄤ":"k","ㄥ":"l","ㄦ":"l"};

const HERITAGE_KOR_LABEL = {
  "金:km": "韓김·금 / 台古音 kim ➔ KM",
  "心:sm": "韓심 / 台古音 sim ➔ SM",
  "木:mk": "韓목 / 台古音 bo̍k ➔ MK",
  "竹:tk": "韓죽 / 台古音 tek ➔ TK",
  "勹:b": "注音 ㄅ ➔ B",
  "勹:b1": "注音 ㄅ＋一聲 ➔ B1",
  "勹:bb": "注音 ㄅ雙疊 ➔ BB",
  "勹:bo": "注音 ㄅ / bō ➔ BO",
  "勹:n": "字母象形 n ➔ N",
  "勹:nn": "字母象形 n雙疊 ➔ NN",
  "冂:m": "注音 ㄇ ➔ M",
  "冂:m1": "注音 ㄇ＋一聲 ➔ M1",
  "冂:mm": "注音 ㄇ雙疊 ➔ MM",
  "冂:mo": "注音 ㄇ / mō ➔ MO",
  "冂:n": "字母象形 n ➔ N",
  "冂:nn": "字母象形 n雙疊 ➔ NN",
  "冖:m": "注音 ㄇ ➔ M",
  "冖:m1": "注音 ㄇ＋一聲 ➔ M1",
  "冖:mm": "注音 ㄇ雙疊 ➔ MM",
  "冖:mo": "注音 ㄇ / mō ➔ MO",
  "冖:n": "字母象形 n ➔ N",
  "冖:nn": "字母象形 n雙疊 ➔ NN",
  "匚:f": "注音 ㄈ ➔ F",
  "匚:f1": "注音 ㄈ＋一聲 ➔ F1",
  "匚:ff": "注音 ㄈ雙疊 ➔ FF",
  "匚:fo": "注音 ㄈ / fō ➔ FO",
  "匚:c": "字母象形 c ➔ C",
  "匚:cc": "字母象形 c雙疊 ➔ CC",
  "凵:v": "注音 ㄩ ➔ V",
  "凵:v1": "注音 ㄩ＋一聲 ➔ V1",
  "凵:vv": "注音 ㄩ雙疊 ➔ VV",
  "凵:yu": "注音 ㄩ / yū ➔ YU",
  "凵:u": "字母象形 u ➔ U",
  "凵:uu": "字母象形 u雙疊 ➔ UU",
  "幺:ao": "注音 ㄠ / āo ➔ AO",
  "幺:a": "注音 ㄠ ➔ A",
  "幺:a1": "注音 ㄠ＋一聲 ➔ A1",
  "幺:aa": "注音 ㄠ雙疊 ➔ AA",
  "么:ao": "注音 ㄠ / āo ➔ AO",
  "么:a": "注音 ㄠ ➔ A",
  "么:a1": "注音 ㄠ＋一聲 ➔ A1",
  "么:aa": "注音 ㄠ雙疊 ➔ AA",
  "𠫓:t": "注音 ㄊ ➔ T",
  "𠫓:t1": "注音 ㄊ＋一聲 ➔ T1",
  "𠫓:tt": "注音 ㄊ雙疊 ➔ TT",
  "𠫓:te": "注音 ㄊ / tē ➔ TE",
  "丂:k": "注音 ㄎ ➔ K",
  "丂:k1": "注音 ㄎ＋一聲 ➔ K1",
  "丂:kk": "注音 ㄎ雙疊 ➔ KK",
  "丂:ke": "注音 ㄎ / kē ➔ KE",
  "厂:h": "注音 ㄏ ➔ H",
  "厂:h1": "注音 ㄏ＋一聲 ➔ H1",
  "厂:hh": "注音 ㄏ雙疊 ➔ HH",
  "厂:he": "注音 ㄏ / hē ➔ HE",
  "厂:f": "字母象形 f ➔ F",
  "厂:ff": "字母象形 f雙疊 ➔ FF",
  "丩:j": "注音 ㄐ ➔ J",
  "丩:j1": "注音 ㄐ＋一聲 ➔ J1",
  "丩:jj": "注音 ㄐ雙疊 ➔ JJ",
  "丩:ji": "注音 ㄐ / jī ➔ JI",
  "卩:z": "注音 ㄗ ➔ Z",
  "卩:z1": "注音 ㄗ＋一聲 ➔ Z1",
  "卩:zz": "注音 ㄗ雙疊 ➔ ZZ",
  "卩:zi": "注音 ㄗ / zī ➔ ZI",
  "卩:p": "字母象形 p ➔ P",
  "卩:pp": "字母象形 p雙疊 ➔ PP",
  "厶:s": "注音 ㄙ ➔ S",
  "厶:s1": "注音 ㄙ＋一聲 ➔ S1",
  "厶:ss": "注音 ㄙ雙疊 ➔ SS",
  "厶:si": "注音 ㄙ / sī ➔ SI",
  "儿:er": "注音 ㄦ / ér ➔ ER",
  "儿:r": "注音 ㄦ ➔ R",
  "儿:r1": "注音 ㄦ＋一聲 ➔ R1",
  "儿:rr": "注音 ㄦ雙疊 ➔ RR"
};

// 將部件拆解字串（如 許氏全拼:金(jen)+甫(fx) 或 視覺拆字:日(bo)+月(de)）自動標註完整注音／拼音或視覺字母推導
function formatLongkanDescWithPhonetics(rawDesc) {
  if (!rawDesc) return "";
  const isVisualCtx = rawDesc.includes("視覺拆字") || rawDesc.includes("視覺簡碼");
  const isHsuCtx = rawDesc.includes("許氏");
  const clean = rawDesc.replace(/^[^:]+:/, "");
  if (isVisualCtx) {
    return clean.replace(/([\u4e00-\u9fff])\(([a-z0-9]+)\)/gi, (match, compCh, compCode) => {
      return `${compCh}〔視覺字母 ➔ ${compCode.toUpperCase()}〕`;
    });
  }
  return clean.replace(/([許氏右旁左旁本音古聲符部件首尾尾部\/]*?)([\u4e00-\u9fff])\(([a-z0-9]+)\)/gi, (match, prefix, compCh, compCode) => {
    const codeLo = compCode.toLowerCase();
    const codeUp = compCode.toUpperCase();
    const korKey = `${compCh}:${codeLo}`;
    if (HERITAGE_KOR_LABEL[korKey]) {
      return `${prefix}${compCh}〔${HERITAGE_KOR_LABEL[korKey]}〕`;
    }
    const bpm = getComponentBpm(compCh, codeLo);
    if (!bpm) return `${prefix}${compCh}(${codeUp})`;
    const py = bopomofoToPinyin(bpm);
    const ty = bopomofoToTongyong(bpm);
    const cleanBpm = bpm.replace(/[ˊˇˋ˙]/g, "");
    const useHsu = isHsuCtx || (prefix && prefix.includes("許氏"));
    if (useHsu && cleanBpm.length >= 1) {
      const hsuFull = Array.from(cleanBpm).map(s => (HSU_KEY_MAP[s] || "").toUpperCase()).join("");
      if (codeLo.length >= 2 && hsuFull === codeUp) {
        const pairs = Array.from(cleanBpm).map((s, idx) => `${s}=${codeUp[idx] || ""}`).join("、");
        return `${prefix}${compCh}〔${bpm} / ${py}：許氏 ${pairs} ➔ ${codeUp}〕`;
      }
      const firstSym = cleanBpm[0];
      const lastSym = cleanBpm[cleanBpm.length - 1];
      const k1 = (HSU_KEY_MAP[firstSym] || codeLo[0] || "").toUpperCase();
      if (codeLo.length === 1) {
        return `${prefix}${compCh}〔${bpm} / ${py}：許氏${firstSym}=${codeUp}〕`;
      }
      const k2 = (HSU_KEY_MAP[lastSym] || codeLo[1] || "").toUpperCase();
      if (firstSym !== lastSym && (k1 + k2) === codeUp) {
        return `${prefix}${compCh}〔${bpm} / ${py}：許氏 ${firstSym}=${k1}、${lastSym}=${k2} ➔ ${codeUp}〕`;
      }
      return `${prefix}${compCh}〔${bpm} / ${py} ➔ 許氏 ${codeUp}〕`;
    }
    const pyLabel = (ty && ty !== py && (rawDesc.includes("通用拼音") || (codeLo[0] === ty[0] && codeLo[0] !== py[0])))
      ? `通拼 ${ty} (${py})`
      : (ty && ty !== py ? `${py} · 通 ${ty}` : py);
    if (codeLo.length === 1) {
      return `${prefix}${compCh}〔${bpm} / ${pyLabel} ➔ 首音 ${codeUp}〕`;
    }
    return `${prefix}${compCh}〔${bpm} / ${pyLabel} ➔ ${codeUp}〕`;
  });
}

// 龍龕形音 2.0 相容碼「思考推論引導」解析器：將代碼轉化為四軌合一認知心理推論路徑說明
function parseLongkanAltReasonings(ch, primaryCode, primaryDesc, altRaw) {
  const items = [];
  if (altRaw) {
    const pairs = altRaw.split("｜").filter(Boolean);
    for (const p of pairs) {
      const eqIdx = p.indexOf("=");
      const code = eqIdx >= 0 ? p.slice(0, eqIdx).trim() : p.trim();
      const desc = eqIdx >= 0 ? p.slice(eqIdx + 1).trim() : "";
      if (!code) continue;
      let tagClass = "rt-shape";
      let tagLabel = "📐 軌道 A：2+2 雙拼簡碼";
      let cleanDesc = formatLongkanDescWithPhonetics(desc);
      let thought = `取首尾部件 2+2 雙拼簡碼：${cleanDesc} ➔ 合成【${code}】`;

      if (desc.startsWith("許氏全拼:") || desc.startsWith("許氏三部件:") || desc.startsWith("許氏首尾:")) {
        tagClass = "rt-hsu";
        tagLabel = "🎵 軌道 H：許氏部件全注音免縮碼";
        thought = `免背縮碼！直接連打各部件的許氏完整注音（前中部件免打聲調，字尾按空白鍵或聲調鍵 d/f/j/s 定界）：${cleanDesc} ➔ 直出【${code}】`;
      } else if (desc.startsWith("拼音全拼:") || desc.startsWith("拼音三部件:") || desc.startsWith("拼音首尾:")) {
        tagClass = "rt-sound";
        tagLabel = "🌐 軌道 P：拼音部件全拼免縮碼";
        thought = `免背縮碼！直接連打各部件的漢語／通用拼音全碼（字尾按空白鍵或聲調數字 1~5 定界）：${cleanDesc} ➔ 直出【${code}】`;
      } else if (desc.startsWith("視覺拆字:") || desc.startsWith("視覺簡碼:")) {
        tagClass = "rt-vis";
        tagLabel = "👁️ 軌道 V：徐冰式純視覺字母（外國人零讀音軌）";
        thought = `零讀音門檻！依書寫筆順（先上後下、先左後右、先外後內）看部件幾何形狀打英文字母：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("首尾部件")) {
        tagClass = "rt-shape";
        const mChain = desc.match(/^首尾部件\(([^)]+)\):/);
        const chainHint = mChain ? `（筆順拆解：${mChain[1]}）` : "";
        tagLabel = "🧭 筆順多部件取首尾";
        thought = `依筆順拆為 3 個以上可讀部件${chainHint}，取第 1 個與最後 1 個部件讀音：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("通用拼音:")) {
        tagClass = "rt-sound";
        tagLabel = "🌐 通用拼音／歐美直覺";
        thought = `免背漢語拼音特殊字母（ㄒ=s、ㄑ=c、ㄓ=jh、ㄩ=yu），直接依通用拼音與英語自然拼讀取碼：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("注音部件:")) {
        tagClass = "rt-sound";
        tagLabel = "🔤 注音符號長相直讀";
        thought = `部件外觀同注音符號（如 勹/𠂊=ㄅ、幺=ㄠ、儿=ㄦ、匚=ㄈ、厂=ㄏ、卩=ㄗ、厶=ㄙ），直接以注音讀音取碼：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("字母部件:")) {
        tagClass = "rt-vis";
        tagLabel = "🅰️ 英文字母長相直讀";
        thought = `部件外觀形似英文字母（如 勹/𠂊/冂=n、匚=c、凵=u、乂=x），直接以字母取碼：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("本音直覺:") || desc.startsWith("本音相容:")) {
        tagClass = "rt-sound";
        tagLabel = "🧠 本音直覺（部首＋本字音）";
        thought = `不認得冷僻古聲符？首部不變，後半直接讀「${ch}」自身讀音：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("許氏雙拼:") || desc.startsWith("許氏本音:") || desc.startsWith("許氏友善:")) {
        tagClass = "rt-hsu";
        tagLabel = "🎵 許氏 2+2 雙拼簡碼";
        thought = `習慣許氏鍵盤者，依部件注音首尾鍵取 2+2 簡碼：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("2+1+調:")) {
        tagClass = "rt-sound";
        tagLabel = "🎯 2+1+調（部首＋首音＋聲調）";
        thought = `首部2碼＋後部首音1碼＋聲調數字：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("右旁部件:") || desc.startsWith("許氏部件:")) {
        tagClass = "rt-shape";
        tagLabel = "🧩 右旁部件（熟悉部件者）";
        thought = `由部件讀音推導簡碼：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("最簡部件:") || desc.startsWith("雙軌別稱:")) {
        tagClass = "rt-shape";
        tagLabel = "🧩 最簡可識部件直拆";
        thought = `取字中最好認的最簡部件讀音：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("獨體疊音:")) {
        tagClass = "rt-dup";
        tagLabel = "🔁 獨體字疊音推論";
        thought = `「${ch}」為獨體部首字，將字音重複疊打兩次：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("許氏疊音:")) {
        tagClass = "rt-hsu";
        tagLabel = "🔁 許氏獨體疊音";
        thought = `獨體字以許氏雙拼重複疊打兩次：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("象形:")) {
        tagClass = "rt-vis";
        tagLabel = "👁️ 字母象形聯想";
        thought = `觀察外框幾何形狀聯想英文字母＋字音：${cleanDesc} ➔ 推得【${code}】`;
      } else if (desc.startsWith("雙拼混搭:")) {
        tagClass = "rt-hsu";
        tagLabel = "🔀 羅馬＋許氏自由混搭";
        thought = `一側打羅馬音、另一側打許氏碼：${cleanDesc} ➔ 推得【${code}】`;
      }
      items.push({ code, rawDesc: desc, cleanDesc, tagClass, tagLabel, thought });
    }
  }
  if (items.length === 0) {
    if (primaryDesc && primaryDesc.includes("×")) {
      const fPrimary = formatLongkanDescWithPhonetics(primaryDesc);
      items.push({
        code: primaryCode,
        rawDesc: primaryDesc,
        cleanDesc: fPrimary,
        tagClass: "rt-dup",
        tagLabel: "🔁 重複部件公式 (C×N)",
        thought: `「${ch}」由相同部件重複疊成，打「部件雙拼(2碼)＋重複數量」：${fPrimary}，3 碼即完稿！`
      });
    } else if (primaryDesc && primaryDesc.startsWith("象形:")) {
      const fPrimary = formatLongkanDescWithPhonetics(primaryDesc);
      items.push({
        code: primaryCode,
        rawDesc: primaryDesc,
        cleanDesc: fPrimary,
        tagClass: "rt-vis",
        tagLabel: "👁️ 26字母視覺象形軌",
        thought: `觀察「${ch}」外框輪廓近似英文字母，打「象形字母＋字音(2碼)＋補碼」：${fPrimary}！`
      });
    }
  }
  return items;
}

// =========================================================================
// 漢字空間幾何拓撲結構與多邊形 SVG 聚光燈引擎 (Structural Topology & SVG Polygon Engine)
// =========================================================================
const STRUCT_META = {
  lr:         { badge: "⿰ 左右結構", pill: "⿰ 左右結構（左部首 ➔ 右聲符/部件）", p1Dir: "①左部", p2Dir: "②右部" },
  rl:         { badge: "⿰ 右部左聲", pill: "⿰ 左右結構（右部首 ➔ 左聲符/本音）", p1Dir: "①右部首", p2Dir: "②左聲符" },
  tb:         { badge: "⿱ 上下結構", pill: "⿱ 上下結構（上部首 ➔ 下聲符/部件）", p1Dir: "①上部", p2Dir: "②下部" },
  bt:         { badge: "⿱ 下部上聲", pill: "⿱ 上下結構（下部首 ➔ 上聲符/本音）", p1Dir: "①下部首", p2Dir: "②上部" },
  tmb:        { badge: "⿳ 上中下結構", pill: "⿳ 上中下三層結構（上層 ➔ 中下層）", p1Dir: "①上層", p2Dir: "②中下層" },
  lmr:        { badge: "⿲ 左中右結構", pill: "⿲ 左中右三欄結構（左欄 ➔ 中右欄）", p1Dir: "①左欄", p2Dir: "②中右欄" },
  tri3:       { badge: "品 三疊結構", pill: "品 三疊結構（一上二下重複單元 C×3）", p1Dir: "①單一部件", p2Dir: "②三疊×3" },
  top_tri3:   { badge: "⿱ 上部＋三疊", pill: "⿱ 上部首 ＋ 下方三疊結構 (C×3)", p1Dir: "①上部首", p2Dir: "②下三疊" },
  left_tri3:  { badge: "⿰ 左部＋三疊", pill: "⿰ 左部首 ＋ 右方三疊結構 (C×3)", p1Dir: "①左部首", p2Dir: "②右三疊" },
  left3_tri3: { badge: "⿷ 左包圍＋三疊", pill: "⿷ 左三包圍外框 (匚) ＋ 內部三疊 (×3)", p1Dir: "①外框匚", p2Dir: "②內三疊" },
  left3:      { badge: "⿷ 左三包圍", pill: "⿷ 左三包圍結構（外框 匚 ➔ 內部）", p1Dir: "①外框匚", p2Dir: "②框內" },
  tl_sur:     { badge: "⿸ 左上包圍", pill: "⿸ 左上包圍結構（外框 ┏ ➔ 右下內部）", p1Dir: "①左上外框", p2Dir: "②右下內部" },
  bl_sur:     { badge: "⿺ 左下包圍", pill: "⿺ 左下包圍結構（走之/外框 ┗ ➔ 右上內部）", p1Dir: "①左下外框", p2Dir: "②右上內部" },
  tr_sur:     { badge: "⿹ 右上包圍", pill: "⿹ 右上包圍結構（外框 ┓ ➔ 左下內部）", p1Dir: "①右上外框", p2Dir: "②左下內部" },
  top3:       { badge: "⿵ 上三包圍", pill: "⿵ 上三包圍結構（外框 冂/門 ➔ 框內下部）", p1Dir: "①上包外框", p2Dir: "②框內下部" },
  bot3:       { badge: "⿶ 下三包圍", pill: "⿶ 下三包圍／嵌套結構（中心 ➔ 外框 凵）", p1Dir: "①內部核心", p2Dir: "②下包框凵" },
  full_sur:   { badge: "⿴ 全包圍結構", pill: "⿴ 全包圍結構（外框 囗 ➔ 中心內部）", p1Dir: "①全包外框", p2Dir: "②中心內部" },
  single:     { badge: "💠 獨體／交錯", pill: "💠 獨體／整體交錯結構（整體疊音或局部特徵）", p1Dir: "①首段取碼", p2Dir: "②次段取碼" },
  four_corner:{ badge: "┼ 四角座標", pill: "┼ 四角號碼十字座標（①左上 ➔ ②右上 ➔ ③左下 ➔ ④右下）", p1Dir: "①左上", p2Dir: "②右上" },
  phrase:     { badge: "🚀 多字詞構詞", pill: "🚀 多字詞跨字取碼構詞模式", p1Dir: "①詞首", p2Dir: "②詞尾" }
};

function getCharStructCode(ch) {
  const lk = LONGKAN_DB[ch];
  if (lk && lk[3]) return lk[3];
  return "single";
}

function getStructMeta(structCode) {
  return STRUCT_META[structCode] || STRUCT_META.single;
}

// 依據單字拓撲結構與拆解描述，推算第 1 步、第 2 步及各相容推論步的精確幾何高亮區域
function computeLongkanStepShapes(ch, structCode, part1, part2, bestCode, bestDesc) {
  let s1 = "left", s2 = "right";
  const meta = getStructMeta(structCode);
  let lbl1 = `${meta.p1Dir}：${part1}`;
  let lbl2 = `${meta.p2Dir}：${part2}`;

  if (ch === "龍") {
    s1 = "whole";
    s2 = "whole_inner";
    lbl1 = `① 獨體首疊：${part1}`;
    lbl2 = `② 獨體次疊：${part2}`;
  } else if (ch === "凶") {
    s1 = "x_cross";
    s2 = "bottom3_surround";
    lbl1 = `① 內部交叉象形：X (乂)`;
    lbl2 = `② 下三包圍框 凵 ＋ 音：${part2}`;
  } else if (ch === "肴") {
    if ((bestDesc || "").startsWith("肉") || (bestDesc || "").startsWith("月")) {
      s1 = "bottom";
      s2 = "top";
      lbl1 = `① 下部首（月/肉）：${part1}`;
      lbl2 = `② 上部（爻/肴音）：${part2}`;
    } else {
      s1 = "top";
      s2 = "bottom";
      lbl1 = `① 上部象形 X(爻)：${part1}`;
      lbl2 = `② 下部月＋肴音：${part2}`;
    }
  } else {
    switch (structCode) {
      case "lr":        s1 = "left"; s2 = "right"; break;
      case "rl":        s1 = "right"; s2 = "left"; break;
      case "tb":        s1 = "top"; s2 = "bottom"; break;
      case "bt":        s1 = "bottom"; s2 = "top"; break;
      case "tmb":       s1 = "top_band"; s2 = "mid_bot_band"; break;
      case "lmr":       s1 = "left_col"; s2 = "mid_right_col"; break;
      case "tri3":      s1 = "tri3_top"; s2 = "tri3_all"; break;
      case "top_tri3":  s1 = "top"; s2 = "bottom_tri3"; break;
      case "left_tri3": s1 = "left"; s2 = "right_tri3"; break;
      case "left3_tri3":s1 = "left3_surround"; s2 = "right_inner_tri3"; break;
      case "left3":     s1 = "left3_surround"; s2 = "right_inner"; break;
      case "tl_sur":    s1 = "tl_surround"; s2 = "br_inner"; break;
      case "bl_sur":    s1 = "bl_surround"; s2 = "tr_inner"; break;
      case "tr_sur":    s1 = "tr_surround"; s2 = "bl_inner"; break;
      case "top3":      s1 = "top3_surround"; s2 = "center_bottom_inner"; break;
      case "bot3":      s1 = "center_top_inner"; s2 = "bottom3_surround"; break;
      case "full_sur":  s1 = "outer_ring"; s2 = "inner_core"; break;
      default:          s1 = "whole"; s2 = "whole_inner"; break;
    }
  }
  return { s1, s2, lbl1, lbl2 };
}

function computeAltStepShape(ch, structCode, s1, s2, r) {
  const rd = r.rawDesc || "";
  if (ch === "龍") {
    if (rd.includes("立")) return { shapeZone: "tl_quad", shapeLabel: `🧩 左上部件：立 [li] ＋ 龍 [ln]` };
    if (rd.includes("月")) return { shapeZone: "bl_quad", shapeLabel: `🧩 左下部件：月 [ye] ＋ 龍 [ln]` };
    return { shapeZone: "whole", shapeLabel: `${r.code}：${r.cleanDesc}` };
  }
  if (ch === "音") {
    if (rd.includes("立") && rd.includes("日")) {
      return { shapeZone: "dual:top+bottom", shapeLabel: `🧩 上部立 [li] ＋ 下部日 [r] ＋ 4聲` };
    }
    if (rd.includes("疊音")) {
      return { shapeZone: "whole", shapeLabel: `🔁 整體疊音：${r.cleanDesc}` };
    }
  }
  if (ch === "肴" && (rd.includes("X") || rd.includes("象形"))) {
    return { shapeZone: "x_cross_top", shapeLabel: `👁️ 上部交叉象形 X ＋ 肴音：${r.code}` };
  }
  if (rd.includes("右旁部件")) {
    return { shapeZone: s2, shapeLabel: `🧩 次部部件直讀：${r.cleanDesc}` };
  }
  if (rd.includes("獨體疊音") || rd.includes("許氏疊音")) {
    return { shapeZone: "whole", shapeLabel: `🔁 獨體疊音：${r.cleanDesc}` };
  }
  if (s1 !== s2 && s1 !== "whole") {
    return { shapeZone: `dual:${s1}+${s2}`, shapeLabel: `✨ 雙部合璧 [${r.code}]：${r.cleanDesc}` };
  }
  return { shapeZone: "whole", shapeLabel: `✨ 相容推論 [${r.code}]：${r.cleanDesc}` };
}

// 繪製結構輔助骨架線（依漢字真實拓撲結構繪製，取代僵硬的單一十字線）
function buildStructGuideSvg(structCode) {
  const gColor = "rgba(148, 163, 184, 0.28)";
  const gDash = "6,5";
  switch (structCode) {
    case "lr":
    case "rl":
      return `<line x1="124" y1="18" x2="124" y2="238" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "tb":
    case "bt":
      return `<line x1="18" y1="126" x2="238" y2="126" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "tmb":
      return `<line x1="18" y1="90" x2="238" y2="90" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />
              <line x1="18" y1="164" x2="238" y2="164" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />`;
    case "lmr":
      return `<line x1="90" y1="18" x2="90" y2="238" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />
              <line x1="166" y1="18" x2="166" y2="238" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />`;
    case "tri3":
      return `<line x1="18" y1="116" x2="238" y2="116" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />
              <line x1="128" y1="116" x2="128" y2="238" stroke="${gColor}" stroke-width="1.8" stroke-dasharray="${gDash}" />
              <polygon points="128,58 72,175 184,175" fill="none" stroke="rgba(250,204,21,0.22)" stroke-width="1.6" stroke-dasharray="4,4" />`;
    case "top_tri3":
      return `<line x1="18" y1="108" x2="238" y2="108" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />
              <line x1="92" y1="108" x2="92" y2="238" stroke="${gColor}" stroke-width="1.5" stroke-dasharray="${gDash}" />
              <line x1="164" y1="108" x2="164" y2="238" stroke="${gColor}" stroke-width="1.5" stroke-dasharray="${gDash}" />`;
    case "left_tri3":
      return `<line x1="98" y1="18" x2="98" y2="238" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />
              <line x1="98" y1="118" x2="238" y2="118" stroke="${gColor}" stroke-width="1.6" stroke-dasharray="${gDash}" />
              <line x1="168" y1="118" x2="168" y2="238" stroke="${gColor}" stroke-width="1.6" stroke-dasharray="${gDash}" />`;
    case "left3_tri3":
    case "left3":
      return `<polyline points="228,66 70,66 70,190 228,190" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "tl_sur":
      return `<polyline points="232,78 80,78 80,232" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "bl_sur":
      return `<polyline points="74,24 74,180 234,180" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "tr_sur":
      return `<polyline points="24,78 176,78 176,232" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "top3":
      return `<polyline points="70,230 70,78 186,78 186,230" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "bot3":
      return `<polyline points="68,30 68,186 188,186 188,30" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    case "full_sur":
      return `<rect x="68" y="68" width="120" height="120" rx="8" fill="none" stroke="${gColor}" stroke-width="2" stroke-dasharray="${gDash}" />`;
    default:
      return `<circle cx="128" cy="128" r="96" fill="none" stroke="rgba(148, 163, 184, 0.16)" stroke-width="1.5" stroke-dasharray="5,5" />`;
  }
}

// 單一幾何形狀生成（支援上下、左右、上中下、三疊品字、C型/U型/┏型/┗型/冂型/囗型包圍框、X交叉與象限）
function buildSingleShapeSvg(zoneName, strokeCol, fillCol) {
  const attr = `class="svg-hl-shape" fill="${fillCol}" stroke="${strokeCol}" stroke-dasharray="7,4" stroke-linejoin="round"`;
  switch (zoneName) {
    case "left":
      return {
        svg: `<rect x="20" y="28" width="100" height="200" rx="14" ${attr} />
              <path d="M 102,128 L 116,128 M 111,122 L 118,128 L 111,134" stroke="${strokeCol}" stroke-width="2.5" fill="none" />`,
        cx: 70, cy: 214
      };
    case "right":
      return {
        svg: `<rect x="124" y="28" width="112" height="200" rx="14" ${attr} />`,
        cx: 180, cy: 214
      };
    case "top":
      return {
        svg: `<rect x="22" y="24" width="212" height="98" rx="14" ${attr} />
              <path d="M 128,106 L 128,120 M 122,115 L 128,122 L 134,115" stroke="${strokeCol}" stroke-width="2.5" fill="none" />`,
        cx: 128, cy: 40
      };
    case "bottom":
      return {
        svg: `<rect x="22" y="128" width="212" height="104" rx="14" ${attr} />`,
        cx: 128, cy: 218
      };
    case "top_band":
      return {
        svg: `<rect x="22" y="22" width="212" height="66" rx="12" ${attr} />`,
        cx: 128, cy: 38
      };
    case "mid_bot_band":
      return {
        svg: `<rect x="22" y="92" width="212" height="140" rx="14" ${attr} />`,
        cx: 128, cy: 218
      };
    case "left_col":
      return {
        svg: `<rect x="18" y="26" width="70" height="204" rx="12" ${attr} />`,
        cx: 53, cy: 216
      };
    case "mid_right_col":
      return {
        svg: `<rect x="92" y="26" width="146" height="204" rx="14" ${attr} />`,
        cx: 165, cy: 216
      };
    case "tri3_top":
      return {
        svg: `<rect x="64" y="20" width="128" height="92" rx="16" ${attr} />`,
        cx: 128, cy: 38
      };
    case "tri3_all":
      return {
        svg: `<polygon points="128,64 70,174 186,174" fill="none" stroke="${strokeCol}" stroke-width="2.2" stroke-dasharray="4,4" />
              <rect x="68" y="20" width="120" height="86" rx="14" ${attr} />
              <rect x="18" y="120" width="104" height="104" rx="14" ${attr} />
              <rect x="134" y="120" width="104" height="104" rx="14" ${attr} />
              <circle cx="128" cy="63" r="11" fill="#0f172a" stroke="${strokeCol}" stroke-width="2"/>
              <text x="128" y="67" text-anchor="middle" fill="${strokeCol}" font-size="12" font-weight="900">1</text>
              <circle cx="70" cy="172" r="11" fill="#0f172a" stroke="${strokeCol}" stroke-width="2"/>
              <text x="70" y="176" text-anchor="middle" fill="${strokeCol}" font-size="12" font-weight="900">2</text>
              <circle cx="186" cy="172" r="11" fill="#0f172a" stroke="${strokeCol}" stroke-width="2"/>
              <text x="186" y="176" text-anchor="middle" fill="${strokeCol}" font-size="12" font-weight="900">3</text>`,
        cx: 128, cy: 218
      };
    case "bottom_tri3":
      return {
        svg: `<rect x="18" y="116" width="68" height="106" rx="12" ${attr} />
              <rect x="94" y="122" width="68" height="106" rx="12" ${attr} />
              <rect x="170" y="116" width="68" height="106" rx="12" ${attr} />`,
        cx: 128, cy: 218
      };
    case "right_tri3":
      return {
        svg: `<rect x="126" y="24" width="106" height="86" rx="13" ${attr} />
              <rect x="102" y="122" width="66" height="102" rx="12" ${attr} />
              <rect x="174" y="122" width="66" height="102" rx="12" ${attr} />`,
        cx: 172, cy: 218
      };
    case "left3_surround":
      return {
        svg: `<path d="M 22,24 H 232 V 64 H 68 V 192 H 232 V 232 H 22 Z" ${attr} />`,
        cx: 128, cy: 44
      };
    case "right_inner_tri3":
      return {
        svg: `<rect x="106" y="68" width="96" height="54" rx="10" ${attr} />
              <rect x="76" y="130" width="74" height="56" rx="10" ${attr} />
              <rect x="158" y="130" width="74" height="56" rx="10" ${attr} />`,
        cx: 154, cy: 176
      };
    case "right_inner":
      return {
        svg: `<rect x="74" y="68" width="156" height="120" rx="12" ${attr} />`,
        cx: 152, cy: 174
      };
    case "tl_surround":
      return {
        svg: `<path d="M 20,22 H 234 V 74 H 78 V 234 H 20 Z" ${attr} />`,
        cx: 128, cy: 46
      };
    case "br_inner":
      return {
        svg: `<rect x="82" y="78" width="152" height="154" rx="14" ${attr} />`,
        cx: 158, cy: 216
      };
    case "bl_surround":
      return {
        svg: `<path d="M 18,28 H 72 V 182 H 238 V 234 H 18 Z" ${attr} />`,
        cx: 128, cy: 212
      };
    case "tr_inner":
      return {
        svg: `<rect x="76" y="22" width="158" height="154" rx="14" ${attr} />`,
        cx: 155, cy: 42
      };
    case "tr_surround":
      return {
        svg: `<path d="M 22,22 H 234 V 234 H 178 V 76 H 22 Z" ${attr} />`,
        cx: 128, cy: 46
      };
    case "bl_inner":
      return {
        svg: `<rect x="22" y="80" width="150" height="150" rx="14" ${attr} />`,
        cx: 97, cy: 216
      };
    case "top3_surround":
      return {
        svg: `<path d="M 20,22 H 236 V 232 H 186 V 74 H 70 V 232 H 20 Z" ${attr} />`,
        cx: 128, cy: 46
      };
    case "center_bottom_inner":
      return {
        svg: `<rect x="74" y="80" width="108" height="148" rx="12" ${attr} />`,
        cx: 128, cy: 214
      };
    case "bottom3_surround":
      return {
        svg: `<path d="M 22,82 H 68 V 186 H 188 V 82 H 234 V 232 H 22 Z" ${attr} />`,
        cx: 128, cy: 212
      };
    case "center_top_inner":
      return {
        svg: `<rect x="72" y="26" width="112" height="152" rx="14" ${attr} />`,
        cx: 128, cy: 44
      };
    case "x_cross":
      return {
        svg: `<polygon points="82,46 106,46 128,82 150,46 174,46 142,102 176,162 152,162 128,122 104,162 80,162 114,102" ${attr} />`,
        cx: 128, cy: 42
      };
    case "x_cross_top":
      return {
        svg: `<polygon points="76,24 102,24 128,58 154,24 180,24 144,72 182,118 156,118 128,84 100,118 74,118 112,72" ${attr} />`,
        cx: 128, cy: 38
      };
    case "outer_ring":
      return {
        svg: `<path d="M 20,20 H 236 V 236 H 20 Z M 68,68 V 188 H 188 V 68 Z" fill-rule="evenodd" ${attr} />`,
        cx: 128, cy: 44
      };
    case "inner_core":
      return {
        svg: `<rect x="70" y="70" width="116" height="116" rx="12" ${attr} />`,
        cx: 128, cy: 174
      };
    case "tl_quad":
      return {
        svg: `<rect x="24" y="24" width="102" height="100" rx="14" ${attr} />`,
        cx: 75, cy: 42
      };
    case "tr_quad":
      return {
        svg: `<rect x="130" y="24" width="102" height="100" rx="14" ${attr} />`,
        cx: 181, cy: 42
      };
    case "bl_quad":
      return {
        svg: `<rect x="24" y="128" width="102" height="102" rx="14" ${attr} />`,
        cx: 75, cy: 214
      };
    case "br_quad":
      return {
        svg: `<rect x="130" y="128" width="102" height="102" rx="14" ${attr} />`,
        cx: 181, cy: 214
      };
    case "whole_inner":
      return {
        svg: `<polygon points="62,28 194,28 228,62 228,194 194,228 62,228 28,194 28,62" ${attr} />`,
        cx: 128, cy: 216
      };
    case "whole":
    default:
      return {
        svg: `<polygon points="54,20 202,20 236,54 236,202 202,236 54,236 20,202 20,54" ${attr} />`,
        cx: 128, cy: 216
      };
  }
}

function renderSvgHighlight(L, S) {
  const svgEl = document.getElementById("hlSvg");
  const canvasEl = document.getElementById("charCanvas");
  const pillEl = document.getElementById("structTypePill");
  if (!svgEl || !canvasEl) return;

  const structCode = L.structType || (currentCrossImeMode === "four_corner" ? "four_corner" : "single");
  canvasEl.setAttribute("data-struct", structCode);
  if (pillEl) {
    pillEl.textContent = L.structLabel || getStructMeta(structCode).pill;
  }

  let html = buildStructGuideSvg(structCode);
  if (S && S.shapeZone) {
    if (S.shapeZone.startsWith("dual:")) {
      const pair = S.shapeZone.slice(5).split("+");
      const g1 = buildSingleShapeSvg(pair[0], "#38bdf8", "rgba(56, 189, 248, 0.16)");
      const g2 = buildSingleShapeSvg(pair[1], "#facc15", "rgba(250, 204, 21, 0.18)");
      html += g1.svg + g2.svg;
    } else {
      const isStep1 = S.activeSpots && S.activeSpots.length === 1 && S.activeSpots[0] === 0;
      const strokeCol = isStep1 ? "#38bdf8" : "#facc15";
      const fillCol = isStep1 ? "rgba(56, 189, 248, 0.20)" : "rgba(250, 204, 21, 0.20)";
      const g = buildSingleShapeSvg(S.shapeZone, strokeCol, fillCol);
      html += g.svg;
    }
    if (S.shapeLabel) {
      const safeText = String(S.shapeLabel).slice(0, 24);
      const boxW = Math.min(244, Math.max(120, safeText.length * 11 + 18));
      const boxX = Math.round((256 - boxW) / 2);
      const boxY = 198;
      html += `
        <g class="svg-callout">
          <rect x="${boxX}" y="${boxY}" width="${boxW}" height="24" rx="7" fill="rgba(15, 23, 42, 0.92)" stroke="#facc15" stroke-width="1.5" />
          <text x="128" y="${boxY + 16}" text-anchor="middle" fill="#f8fafc" font-size="11.5" font-weight="800" font-family="Microsoft JhengHei, sans-serif">${safeText}</text>
        </g>
      `;
    }
  }
  svgEl.innerHTML = html;
}

// 拆解龍龕主說明字串為 [首部說明, 次部說明]（自動附上各部件完整注音／拼音➔簡碼推導）
function splitLongkanPrimaryParts(primaryCode, primaryDesc) {
  const clean = formatLongkanDescWithPhonetics(primaryDesc || "");
  if (clean.includes("+")) {
    const parts = clean.split("+");
    return [parts[0].trim(), parts.slice(1).join("+").trim()];
  }
  if (clean.includes("×")) {
    const [comp, cnt] = clean.split("×");
    return [`部件：${comp}`, `重複次數：×${cnt}`];
  }
  return [clean || primaryCode.slice(0, 2), primaryCode.slice(2)];
}

// 逐字獨立成行渲染器（針對任一輸入法，每個字獨立成一行並附上思考推論引導）
function renderPerCharLinesHtml(provId, raw) {
  const chars = Array.from(raw).filter(c => !/\s/.test(c));
  if (chars.length === 0) return "";
  let html = `<div class="cim-char-list">`;

  chars.forEach((ch, idx) => {
    if (provId === "longkan") {
      const lk = LONGKAN_DB[ch];
      if (!lk) {
        html += `<div class="cim-char-line"><div class="cim-char-glyph">${ch}</div><div class="cim-char-primary"><span class="cim-main-code">—</span><span class="cim-main-desc">標點或特殊字元</span></div><div class="cim-char-reasoning">直接輸入標點符號</div></div>`;
        return;
      }
      const [bestCode, bestDesc, altRaw, structCode] = lk;
      const sMeta = getStructMeta(structCode || "single");
      const cleanPrimary = formatLongkanDescWithPhonetics(bestDesc || "");
      const reasons = parseLongkanAltReasonings(ch, bestCode, bestDesc, altRaw);
      let reasonHtml = `<div style="font-size:0.75rem; color:var(--gold); font-weight:800; margin-bottom:2px;">💡 相容碼思考推論引導（免死背，任選一種思維皆可打出「${ch}」）：</div>`;
      reasons.forEach(r => {
        reasonHtml += `<div class="reason-item">
          <span class="reason-tag ${r.tagClass}">${r.tagLabel}</span>
          <span class="reason-code">${r.code}</span>
          <span class="reason-explain">${r.thought}</span>
        </div>`;
      });
      html += `<div class="cim-char-line" onclick="sequenceIdx=-1; triggerCharStageAnim('${ch}', ${idx})" title="點擊播放「${ch}」的龍龕形音空間拓撲拆碼與相容推論動畫">
        <div class="cim-char-glyph">${ch}</div>
        <div class="cim-char-primary">
          <div style="display:flex; align-items:center; gap:5px; flex-wrap:wrap;">
            <span style="font-size:0.75rem; color:var(--text-muted);">正軌主碼：</span><span class="cim-main-code">${bestCode}</span>
            <span style="font-size:0.72rem; background:#1e293b; color:var(--cyan); border:1px solid rgba(56,189,248,0.4); border-radius:4px; padding:1px 5px; font-weight:800;">${sMeta.badge}</span>
          </div>
          <div class="cim-main-desc">📐 ${cleanPrimary}</div>
          <div style="font-size:0.73rem; color:var(--text-muted);">🎬 點擊播放【${sMeta.badge}】拆碼動畫 ▶</div>
        </div>
        <div class="cim-char-reasoning">${reasonHtml}</div>
      </div>`;
    } else if (provId === "four_corner") {
      const inf = CHAR_DB[ch];
      if (!inf) {
        html += `<div class="cim-char-line"><div class="cim-char-glyph">${ch}</div><div class="cim-char-primary"><span class="cim-main-code">.</span><span class="cim-main-desc">標點符號</span></div><div class="cim-char-reasoning">按小鍵盤 [.] 標點選單</div></div>`;
        return;
      }
      const fulls = inf[0] || [], fuzzys = inf[1] || [], vars = inf[2] || [];
      const c4 = fulls.find(c => c.length === 4) || fulls[0] || fuzzys[0] || "0000";
      const c5 = fulls.find(c => c.length === 5);
      const disp = c5 ? `${c5.slice(0,4)}.${c5[4]}` : c4;
      let reasonHtml = `<div class="reason-item"><span class="reason-tag rt-shape">📐 四角Z字取碼</span><span class="reason-explain">①左上【${STROKE_NAMES[c4[0]]||c4[0]}】➔ ②右上【${STROKE_NAMES[c4[1]]||c4[1]}】➔ ③左下【${STROKE_NAMES[c4[2]]||c4[2]}】➔ ④右下【${STROKE_NAMES[c4[3]]||c4[3]}】</span></div>`;
      if (fuzzys.length > 0) {
        reasonHtml += `<div class="reason-item"><span class="reason-tag rt-sound">🧠 直覺寬鬆相容碼</span><span class="reason-code">${fuzzys.join(" / ")}</span><span class="reason-explain">免死背「筆形複用歸 0」或「氵挑筆歸 1」例外，按直覺看到的筆形打也能命中！</span></div>`;
      }
      if (vars.length > 0) {
        reasonHtml += `<div class="reason-item"><span class="reason-tag rt-vis">🔄 正異體共享碼</span><span class="reason-code">${vars.join(" / ")}</span><span class="reason-explain">依習慣拆正體或異體字形皆可互通出字</span></div>`;
      }
      html += `<div class="cim-char-line" onclick="sequenceIdx=-1; triggerCharStageAnim('${ch}', ${idx})">
        <div class="cim-char-glyph">${ch}</div>
        <div class="cim-char-primary">
          <div><span style="font-size:0.75rem; color:var(--text-muted);">正統四角：</span><span class="cim-main-code">${disp}</span></div>
          <div class="cim-main-desc">🔢 左上${c4[0]} · 右上${c4[1]} · 左下${c4[2]} · 右下${c4[3]}</div>
        </div>
        <div class="cim-char-reasoning">${reasonHtml}</div>
      </div>`;
    } else if (provId === "array30") {
      const rec = CROSS_IME_DB[ch];
      const code = rec && rec[0] ? rec[0] : "";
      const extra = rec && rec[1] ? rec[1] : "";
      const coords = Array.from(code).map(k => `${k.toUpperCase()}(${ARRAY30_COORDS[k]||k})`).join(" ➔ ");
      html += `<div class="cim-char-line" onclick="sequenceIdx=-1; triggerCharStageAnim('${ch}', ${idx})">
        <div class="cim-char-glyph">${ch}</div>
        <div class="cim-char-primary">
          <div><span style="font-size:0.75rem; color:var(--text-muted);">行列鍵碼：</span><span class="cim-main-code">${code.toUpperCase() || "—"}</span></div>
          <div class="cim-main-desc">📐 ${coords || "—"}</div>
        </div>
        <div class="cim-char-reasoning">
          <div class="reason-item"><span class="reason-tag rt-shape">📐 筆形座標推論</span><span class="reason-explain">按書寫筆順取字根：首筆決定行號(0~9)、尾筆決定列位(上^/中-/下v)${extra ? `；💡 捷徑相容：<strong>${extra}</strong>` : ""}</span></div>
        </div>
      </div>`;
    } else if (provId === "cangjie") {
      const rec = CROSS_IME_DB[ch];
      const code = rec && rec[2] ? rec[2] : "";
      const rads = Array.from(code).map(k => `${CANGJIE_RADS[k]||k}(${k.toUpperCase()})`).join(" ＋ ");
      const radStr = Array.from(code).map(k => CANGJIE_RADS[k]||k).join("");
      html += `<div class="cim-char-line" onclick="sequenceIdx=-1; triggerCharStageAnim('${ch}', ${idx})">
        <div class="cim-char-glyph">${ch}</div>
        <div class="cim-char-primary">
          <div><span style="font-size:0.75rem; color:var(--text-muted);">倉頡字根：</span><span class="cim-main-code">${radStr || "—"} (${code.toUpperCase()})</span></div>
          <div class="cim-main-desc">🔤 共 ${code.length} 碼</div>
        </div>
        <div class="cim-char-reasoning">
          <div class="reason-item"><span class="reason-tag rt-shape">🔤 字根取碼推論</span><span class="reason-explain">依「由左而右、由上而下、由外而內」拆解為：<strong>${rads}</strong>（連體取首二三尾，分體字首取首尾＋字身取首二尾）</span></div>
        </div>
      </div>`;
    } else if (provId === "hsu" || provId === "standard" || provId === "et26") {
      const rec = CROSS_IME_DB[ch];
      const bpm = rec && rec[3] ? rec[3] : "";
      const hsuCode = bopomofoToHsu(bpm);
      const stdCode = bopomofoToStandard(bpm);
      const etCode = bopomofoToEt26(bpm);
      const mainKey = provId === "hsu" ? hsuCode : (provId === "et26" ? etCode : stdCode);
      const explain = provId === "hsu"
        ? `注音【${bpm}】➔ 許氏聲韻雙拼鍵【${hsuCode}】（聲韻母合併於 25 字母鍵，空白=一聲、D=二聲、F=三聲、J=四聲、S=輕聲）`
        : `注音【${bpm}】➔ 標準大千鍵【${stdCode}】｜ 倚天26鍵【${etCode}】`;
      html += `<div class="cim-char-line" onclick="sequenceIdx=-1; triggerCharStageAnim('${ch}', ${idx})">
        <div class="cim-char-glyph">${ch}</div>
        <div class="cim-char-primary">
          <div><span style="font-size:0.75rem; color:var(--text-muted);">按鍵序列：</span><span class="cim-main-code">${mainKey || "—"}</span></div>
          <div class="cim-main-desc">🎵 注音：${bpm || "—"}</div>
        </div>
        <div class="cim-char-reasoning">
          <div class="reason-item"><span class="reason-tag rt-sound">🎵 音韻鍵位推論</span><span class="reason-explain">${explain}</span></div>
        </div>
      </div>`;
    }
  });

  html += `</div>`;
  return html;
}

// 通用可擴展跨輸入法反查提供者註冊表 (Open-Closed Universal Provider Registry)
const IME_PROVIDERS_REGISTRY = [
  {
    id: "longkan",
    name: "🐉 龍龕形音 2.0 (四軌)",
    inspectChar(ch) {
      const lk = LONGKAN_DB[ch];
      if (!lk) return null;
      return lk[2] ? `${lk[0]} [${lk[1]}] (四軌:${lk[2]})` : `${lk[0]} [${lk[1]}]`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("longkan", raw);
    }
  },
  {
    id: "four_corner",
    name: "🔢 智慧四角 (4×5)",
    inspectChar(ch) {
      const inf = CHAR_DB[ch];
      if (!inf) return null;
      const fulls = inf[0] || [], fuzzys = inf[1] || [];
      const c4 = fulls.find(c => c.length === 4) || fulls[0] || fuzzys[0] || "";
      const c5 = fulls.find(c => c.length === 5);
      return c5 ? `${c5.slice(0,4)}.${c5[4]}` : c4;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("four_corner", raw);
    }
  },
  {
    id: "array30",
    name: "📐 行列 30 (筆形座標)",
    inspectChar(ch) {
      const rec = CROSS_IME_DB[ch];
      if (!rec || !rec[0]) return null;
      const code = rec[0], extra = rec[1];
      const coords = Array.from(code).map(k => ARRAY30_COORDS[k] || k).join(" ");
      return `${code.toUpperCase()} [${coords}]${extra ? " · " + extra : ""}`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("array30", raw);
    }
  },
  {
    id: "cangjie",
    name: "🔤 倉頡 (哲理字根)",
    inspectChar(ch) {
      const rec = CROSS_IME_DB[ch];
      if (!rec || !rec[2]) return null;
      const code = rec[2];
      const rads = Array.from(code).map(k => CANGJIE_RADS[k] || k).join("");
      return `${rads} (${code.toUpperCase()})`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("cangjie", raw);
    }
  },
  {
    id: "hsu",
    name: "🎵 許氏雙拼注音",
    inspectChar(ch) {
      const rec = CROSS_IME_DB[ch];
      if (!rec || !rec[3]) return null;
      return `${bopomofoToHsu(rec[3])} [${rec[3]}]`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("hsu", raw);
    }
  },
  {
    id: "standard",
    name: "🎹 標準大千注音",
    inspectChar(ch) {
      const rec = CROSS_IME_DB[ch];
      if (!rec || !rec[3]) return null;
      return `${bopomofoToStandard(rec[3])} [${rec[3]}]`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("standard", raw);
    }
  },
  {
    id: "et26",
    name: "⌨️ 倚天 26 鍵注音",
    inspectChar(ch) {
      const rec = CROSS_IME_DB[ch];
      if (!rec || !rec[3]) return null;
      return `${bopomofoToEt26(rec[3])} [${rec[3]}]`;
    },
    inspectSentence(raw) {
      return renderPerCharLinesHtml("et26", raw);
    }
  }
];

let currentCrossImeMode = "longkan";

// 龍龕形音 2.0 專屬 6 大互動單元課程
const LONGKAN_LESSONS = [
  {
    tabName: "單元 1：軌道 H ＆ P 部件全拼免縮碼（鋪 jenfx/jinfu · 龕 hglxl/helong）",
    title: "🐉 龍龕 2.0 單元 1：軌道 H/P「部件全注音／全拼免縮碼」＋ 空白鍵/聲調鍵定界 — 以「鋪」與「龕」為例",
    charHtml: "鋪",
    structType: "lr",
    structLabel: "⿰ 左右結構（軌道H 許氏全拼 jenfx ｜ 軌道P 拼音全拼 jinfu ｜ 軌道A 雙拼 jnfx）",
    spots: ["①軌道H(左): 金[ㄐㄧㄣ➔JEN]", "②軌道H(右): 甫[ㄈㄨˇ➔FX]", "③軌道P(全拼): JINFU [金+甫]", "④定界: 空白鍵 / 聲調鍵 1/4/j"],
    steps: [
      { key: "j", code: "jen", activeSpots: [0], shapeZone: "left", shapeLabel: "① 軌道 H 左部：金〔ㄐㄧㄣ ➔ 許氏 JEN〕", desc: "① 軌道 H（許氏部件全注音免縮碼）：左偏旁為『金〔ㄐㄧㄣ〕』，不用背縮碼！直接按許氏注音全碼【jen】（前段部件免打聲調）！", status: "軌道 H 首部：金〔ㄐㄧㄣ ➔ JEN〕", cands: [["金","jen"],["鋪","jenfx / jinfu"]], sel: 0 },
      { key: "f", code: "jenfx", activeSpots: [0,1], shapeZone: "right", shapeLabel: "② 軌道 H 右部：甫〔ㄈㄨˇ ➔ 許氏 FX〕", desc: "② 接續右旁部件：右旁是『甫〔ㄈㄨˇ〕』，直接接著打許氏全碼【fx】➔ 合成【jenfx】，按空白鍵（或聲調鍵 1/4/j）即刻定界直出「鋪」！", status: "🎵 軌道 H 完成：JENFX（金jen＋甫fx）", cands: [["鋪","jenfx (許氏全拼)"]], sel: 0, compose: "鋪" },
      { key: "i", code: "jinfu", activeSpots: [0,1,2], shapeZone: "dual:left+right", shapeLabel: "🌐 軌道 P 拼音全拼：金[JIN]＋甫[FU]", desc: "🌐 軌道 P（拼音部件全拼免縮碼）：習慣漢語／通用拼音者，直接打『金(jin)＋甫(fu)』➔【jinfu】（加空白鍵或聲調 1/4）100% 同步直出「鋪」！", status: "🌐 軌道 P 完成：JINFU（金jin＋甫fu）", cands: [["鋪","jinfu (拼音全拼)"]], sel: 0, compose: "鋪" },
      { key: "h", code: "hglxl", activeSpots: [0,1,2,3], shapeZone: "whole", shapeLabel: "💡 同理「龕」= 合(hg/he)+龍(lxl/long)", desc: "💡 上下結構同理：「龕」＝上部『合(許氏 hg / 拼音 he)』＋下部『龍(許氏 lxl / 拼音 long)』➔ 打【hglxl】或【helong】（傳統 4 碼 holn/hgln 亦完全相容）！", status: "四軌並容：jenfx / jinfu / hglxl / helong", cands: [["鋪","jenfx / jinfu / jnfx"],["龕","hglxl / helong / holn"]], sel: 0, compose: "鋪龕" }
    ]
  },
  {
    tabName: "單元 2：軌道 V 徐冰式純視覺字母 — 外國人零讀音（明 bode · 古 tio · 品 ooo）",
    title: "🐉 龍龕 2.0 單元 2：軌道 V「徐冰《英文方塊字》純視覺字母軌」— 完全不需懂讀音，看形狀依筆順打字！",
    charHtml: "明",
    structType: "lr",
    structLabel: "⿰ 左右結構（軌道V 純視覺字母：日=B/BO ＋ 月=D/DE ➔ BODE / BD）",
    spots: ["①左部視覺: 日 ➔ B (BO)", "②右部視覺: 月 ➔ D (DE)", "③上下對比: 古 TIO / 叶 OTI", "④三疊視覺: 品 OOO / 森 TTT"],
    steps: [
      { key: "b", code: "bo", activeSpots: [0], shapeZone: "left", shapeLabel: "① 視覺左部：日 形似雙格窗【B / BO】", desc: "① 軌道 V（外國人零讀音友善軌）：完全不必會唸漢字！左邊『日』外框有中間橫隔，形似英文字母【B】（雙碼展開【bo】）", status: "👁️ 軌道 V 左部：日 ➔ BO", cands: [["日","bo"],["明","bode / bd"]], sel: 0 },
      { key: "d", code: "bode", activeSpots: [0,1], shapeZone: "right", shapeLabel: "② 視覺右部：月 形似彎月【D / DE】", desc: "② 視覺右部：右邊『月』左側直豎、右側弧彎，形似英文字母【D】（雙碼【de】）➔ 連打【bode】（或 2 碼簡碼【bd】）直出「明」（同理軌道 H 打 rue / 軌道 P 打 riyue）！", status: "👁️ 軌道 V 完成：BODE / BD ➔ 明", cands: [["明","bode (視覺:日bo+月de)"],["明","rue / riyue (音韻軌)"]], sel: 0, compose: "明" },
      { key: "t", code: "tio", activeSpots: [0,1,2], shapeZone: "dual:top+bottom", shapeLabel: "🧭 筆順定序：古(TIO) vs 叶(OTI)", desc: "🧭 依宏觀書寫筆順（先上後下、先左後右）自然區分同部件字：『古』先上十(T/ti)後下口(O) ➔ 打【tio/to】；『叶』先左口(O)後右十(T/ti) ➔ 打【oti/ot】！", status: "筆順視覺定序：古 tio/to · 叶 oti/ot · 杏 txo · 呆 otx", cands: [["古","tio / to (上十+下口)"],["叶","oti / ot (左口+右十)"],["杏","txo (上木+下口)"],["呆","otx (上口+下木)"]], sel: 0, compose: "古叶杏呆" },
      { key: "o", code: "ooo", activeSpots: [0,1,2,3], shapeZone: "whole", shapeLabel: "👁️ 純視覺直覺：中 OI · 品 OOO · 森 TTT", desc: "👁️ 更多零讀音視覺範例：『中』=口(O)+丨(I)➔【oi】；『品』=三個口➔【ooo】；『休』=人(A)+木(TX)➔【atx】；『好』=女(XO)+子(JE)➔【xoje】；『森』=三個木➔【ttt】！", status: "外國人零讀音視覺軌：oi / ooo / atx / xoje / ttt", cands: [["中","oi"],["品","ooo"],["好","xoje"],["森","ttt"]], sel: 0, compose: "中品好森" }
    ]
  },
  {
    tabName: "單元 3：三部件全拼 ＆ 首尾取碼（謝 emcnam/yanshencun · 喜 cdoko/shidoukou）",
    title: "🐉 龍龕 2.0 單元 3：三部件字「全拼直打」與「首尾部件快取」雙雙支援 — 以「謝」與「喜」為例",
    charHtml: "謝",
    structType: "lmr",
    structLabel: "⿲ 左中右結構（左言 em/yan ＋ 中身 cn/shen ＋ 右寸 am/cun）",
    spots: ["①左部: 言[許em/拼yan]", "②中部: 身[許cn/拼shen]", "③右部: 寸[許am/拼cun]", "④首尾快取: emam / yancun"],
    steps: [
      { key: "e", code: "em", activeSpots: [0], shapeZone: "left_col", shapeLabel: "① 左部：言〔ㄧㄢˊ ➔ 許氏 EM / 拼音 YAN〕", desc: "① 第 1 部件（左欄）：「謝」為左中右三部件結構（⿲），左部『言』打許氏全碼【em】（或拼音【yan】）", status: "① 左部：言 (em / yan)", cands: [["言","em"],["謝","emcnam / emam"]], sel: 0 },
      { key: "c", code: "emcnam", activeSpots: [0,1,2], shapeZone: "mid_right_col", shapeLabel: "②+③ 中身(CN)＋右寸(AM) ➔ EMCNAM", desc: "②+③ 三部件全拼直打：接續中間『身(許氏 cn / 拼音 shen)』＋右邊『寸(許氏 am / 拼音 cun)』➔ 打【emcnam】或拼音【yanshencun】（加四聲 j 或 4）零重碼唯一命中「謝」！", status: "三部件全拼：EMCNAM / YANSHENCUN", cands: [["謝","emcnam (言+身+寸)"],["謝","yanshencun (拼音全拼)"]], sel: 0, compose: "謝" },
      { key: "a", code: "emam", activeSpots: [0,2,3], shapeZone: "dual:left_col+mid_right_col", shapeLabel: "⚡ 首尾部件快取：首(言)＋尾(寸) ➔ EMAM", desc: "⚡ 首尾部件快取：超過 2 個部件時，亦可只取「第 1 部件＋最後 1 部件」➔ 打【emam】或拼音【yancun】（同理上中下結構「喜」＝士+豆+口，可打全拼【cdoko/shidoukou】或首尾【cko/shikou】）！", status: "首尾快取：謝 emam/yancun · 喜 cdoko/cko", cands: [["謝","emam / yancun (言+寸)"],["喜","cdoko / cko (士+豆+口)"]], sel: 0, compose: "謝喜" }
    ]
  },
  {
    tabName: "單元 4：許氏注音 ⇄ 龍龕 2.0 雙向協同（單按 ` 插字 · ~ 切換 · Shift+←+Tab 合體熔煉）",
    title: "🐉 龍龕 2.0 單元 4：許氏注音無縫協同三大神器 — 單按 ` 臨時插字 · 單按 ~ 切換 · Shift+← + Tab 事後合體熔煉",
    charHtml: "喆",
    structType: "lr",
    structLabel: "🔄 許氏注音 ⇄ 龍龕 2.0 雙向協同（以「喆 = 吉+吉」與「龕 = 合+龍」為例）",
    spots: ["①神器1: 單按 ` 臨時插打龍龕", "②神器2: 單按 ~ 切換許氏⇄龍龕", "③神器3: Shift+← 選部件音", "④按 Tab: 即刻合體熔煉出字！"],
    steps: [
      { key: "`", code: "`jeje2", activeSpots: [0], shapeZone: "dual:left+right", shapeLabel: "① 單按 [`]：許氏注音中臨時插打 1 個龍龕字", desc: "① 單按反引號【`】（臨時單字龍龕模式）：在許氏注音打字時遇到同音字極多的「喆」或忘記讀音的字，單按一下【`】顯示「🐉龍龕單字」，輸入吉+吉【jeje2】（或視覺【toto】）上屏後自動返回許氏注音！", status: "🐉 單按 [`] 臨時插打龍龕單字，上屏自動回許氏", cands: [["喆","`jeje2 (吉+吉)"],["龕","`hglxl (合+龍)"]], sel: 0, compose: "喆" },
      { key: "~", code: "~", activeSpots: [0,1], shapeZone: "whole", shapeLabel: "② 單按 [~]：一鍵切換「許氏注音 ⇄ 龍龕 2.0」", desc: "② 單按波浪號【~】（Shift + `）：在「許氏注音」與「龍龕形音 2.0」兩種主模式間一鍵秒切，狀態列即時顯示當前模式！", status: "🔄 單按 [~] 切換：許氏注音 ⇄ 龍龕形音 2.0", cands: [["許氏 ⇄ 龍龕 2.0","單按 ~ 秒切"]], sel: 0, compose: "喆" },
      { key: "Tab", code: "吉吉 ➔ 喆", activeSpots: [0,1,2,3], shapeZone: "dual:left+right", shapeLabel: "③ 事後合體熔煉：Shift+← 選「吉吉」按 Tab ➔「喆」", desc: "③ 事後部件合體熔煉（Post-hoc Fusion）：在許氏注音已經打出部件字（如打出『吉吉』或『合龍』或『言身寸』），按【Shift + ←】反白選取這 2~4 個部件字，再按【Tab】，引擎即刻將它們合體熔煉為「喆」、「龕」、「謝」！", status: "✨ Shift+← 選取部件音 ＋ 按 [Tab] ➔ 合體熔煉完成！", cands: [["喆","吉+吉 ➔ Tab 熔煉"],["龕","合+龍 ➔ Tab 熔煉"],["謝","言+身+寸 ➔ Tab 熔煉"]], sel: 0, compose: "喆龕謝" }
    ]
  },
  {
    tabName: "單元 5：軌道 B/C 包圍象形 ＆ 重複部件 C×N（區 cko3/cooo · 凶 xu1 · 森 mu3/ttt）",
    title: "🐉 龍龕 2.0 單元 5：包圍結構與重複部件 — 傳統 C×N 與 2.0 純視覺字母雙軌並容",
    charHtml: "區",
    structType: "left3",
    structLabel: "⿷ 左三包圍結構（外框 匚=C ＋ 內部三個口=KO3 或 OOO ➔ CKO3 / COOO）",
    spots: ["①外框象形: 匚 ➔ C", "②內部重複: 口×3 ➔ KO3 / OOO", "③下包圍(凶): XU1 / UX", "④品字三疊(森): MU3 / TTT"],
    steps: [
      { key: "c", code: "c", activeSpots: [0], shapeZone: "left3_surround", shapeLabel: "① 左三包圍外框：匚 形似字母【C】", desc: "① 外框視覺定碼：「區」為左三包圍結構（⿷），外框『匚』開口向右，直接對應英文字母【C】", status: "① 外框象形：匚 ➔ C", cands: [["區","cko3 / cooo"],["匹","cpi3"]], sel: 0 },
      { key: "o", code: "cooo", activeSpots: [0,1,2,3], shapeZone: "right_inner_3", shapeLabel: "② 內部三口：視覺 COOO 或 雙拼 CKO3", desc: "② 內部三口雙軌並容：內部有 3 個『口』，打純視覺字母【cooo】（C＋三個O）或打雙拼重複公式【cko3】（C＋口ko＋3）皆可直出「區」（同理「森」可打 mu3 / mumu / ttt）！", status: "包圍與三疊雙軌命中：cooo / cko3 · ttt / mu3", cands: [["區","cooo (視覺) / cko3 (C+口×3)"],["凶","xu1 / ux"],["森","ttt (視覺) / mu3 (木×3)"]], sel: 0, compose: "區凶森" }
    ]
  },
  {
    tabName: "單元 6：連按 `` 數字轉中文財務大寫 ＆ ~~ 專業詞庫切換 ＆ Tab 多字詞簡碼",
    title: "🐉 龍龕 2.0 單元 6：連按兩下 `` 智慧數字／財務大寫轉換 · 連按兩下 ~~ 五大專業詞庫切換 · Tab 多字詞簡碼",
    charHtml: "<div class='phrase-pair'><span style='font-size:56px;'>壹</span><span style='font-size:56px;'>萬</span><span style='font-size:56px;'>貳</span></div>",
    structType: "phrase",
    structLabel: "⚡ 連按 `` 數字轉中文大寫（按 Q 切換）｜ 連按 ~~ 專業領域詞庫 ｜ Tab 多字詞 N+2",
    spots: ["①連按 ``: 數字/財務大寫", "②按 Q 鍵: 切換小寫/大寫金額", "③連按 ~~: 5大專業詞庫切換", "④按 Tab: 多字詞簡碼 N+2"],
    steps: [
      { key: "`", code: "``12345", activeSpots: [0,1], zone: [12,65,232,125], desc: "① 連按兩下反引號【``】（智慧中文數字與範本模式）：輸入【``12345】即自動轉為『一萬二千三百四十五』，再按【Q】鍵立即循環切換為財務大寫『壹萬貳仟參佰肆拾伍圓整』！", status: "🔢 ``12345 ＋ 按 [Q] ➔ 壹萬貳仟參佰肆拾伍圓整", cands: [["一萬二千三百四十五","一般中文數字"],["壹萬貳仟參佰肆拾伍圓整","按 Q 切換財務大寫"]], sel: 1, compose: "壹萬貳仟參佰肆拾伍圓整" },
      { key: "~", code: "~~", activeSpots: [2], zone: [12,65,232,125], desc: "② 連按兩下波浪號【~~】（專業領域詞庫熱切換）：在「通用核心、資訊工程、人文歷史、醫學生命、財經法律」五大領域詞庫間一鍵切換，自動調優領域專有名詞排序！", status: "📚 連按 [~~] 切換 5 大專業領域詞庫", cands: [["1. 通用核心詞庫","預設"],["2. 資訊工程詞庫","軟硬體/AI"],["3. 人文歷史 / 4. 醫學 / 5. 財經法律","領域優先"]], sel: 0, compose: "壹萬貳仟參佰肆拾伍圓整" },
      { key: "Tab", code: "-cerci", activeSpots: [0,1,2,3], zone: null, desc: "③ Tab 多字詞簡碼 ＆ 輸入座安全確認：打三字詞以上按【Tab】取「首字前2碼＋中間字第1碼＋尾字後2碼」（如輸入法 -cerci）；所有組字皆先暫存於「如喜輸入座」，確認無誤按【Enter】才送出至目標視窗！", status: "按 [Enter] 確認無誤送出至目標視窗", cands: [["輸入法","-cerci / -cerfa"],["(已完稿送出至目標視窗)","完成"]], sel: 0, compose: "（空白）", commit: "壹萬貳仟參佰肆拾伍圓整" }
    ]
  }
];

function updateModeSpecificChrome() {
  const mode = currentCrossImeMode;
  const hBadge = document.getElementById("headerBadge");
  const hTitle = document.getElementById("headerTitleText");
  const hSub = document.getElementById("headerSubText");
  const hMotto = document.getElementById("headerMotto");
  const lTitle = document.getElementById("lookupTitleText");
  const chipsBox = document.getElementById("presetChips");
  const tabsBox = document.getElementById("lessonTabs");
  const numpadCard = document.getElementById("numpadCard");
  const lkGuideCard = document.getElementById("longkanGuideCard");
  const otherGuideCard = document.getElementById("otherImeGuideCard");
  const fcRhyme = document.getElementById("fcRhymeSection");
  const lkRhyme = document.getElementById("lkRhymeSection");
  const tsfBadge = document.getElementById("tsfBadgeIcon");
  const tsfLabel = document.getElementById("tsfCodeLabel");
  const tsfPager = document.getElementById("tsfPagerBox");
  const composeHint = document.getElementById("composeDeskHint");

  if (mode === "longkan") {
    hBadge.textContent = "龍2.0";
    hTitle.textContent = "如喜（RuhI）— 龍龕形音 2.0（四軌合一 ＆ 許氏雙向協同）互動拆碼教學工作台";
    hSub.innerHTML = "內建完整 545,778 筆<strong>龍龕形音 2.0 四軌詞庫</strong>（<strong>軌道 H 許氏部件全拼 · 軌道 P 拼音部件全拼 · 軌道 V 徐冰式純視覺字母 · 軌道 A/C 雙拼與 C×N</strong>）＋ <strong>許氏注音雙向協同（<code>`</code> / <code>~</code> / Shift+←+Tab）</strong>！";
    hMotto.textContent = "🐉 龍龕 2.0 心法：免背縮碼直接打「部件許氏/拼音全碼」＋空白/聲調定界 ｜ 外國人零讀音看形狀打「軌道 V 視覺字母」｜ 許氏按 ` 臨時插字、Shift+←+Tab 事後合體熔煉";
    lTitle.textContent = "🔍 龍龕形音 2.0 逐字四軌反查 ＆ 思考推論教練（每字獨立成行，同時呈現軌道 H / P / V / A，點擊任一字列或字卡立即播放動畫）";
    chipsBox.innerHTML = `
      <span>🐉 龍龕 2.0 快速範例：</span>
      <button class="chip-btn" onclick="setLookupText('鋪龕謝喜喆明看')">軌道 H/P 部件全拼免縮碼（鋪jenfx/jinfu·龕hglxl/helong·謝emcnam）</button>
      <button class="chip-btn" onclick="setLookupText('中品古叶杏呆明好休林森區')">軌道 V 徐冰式純視覺字母（外國人零讀音：中oi·品ooo·古tio·明bode）</button>
      <button class="chip-btn" onclick="setLookupText('龍龕形音輸入法')">龍龕形音輸入法</button>
      <button class="chip-btn" onclick="setLookupText('銀鋪龕肴凶區品森協蕊')">四軌全方位對照（銀·鋪·龕·肴·凶·區·品·森·協·蕊）</button>
      <button class="chip-btn" onclick="setLookupText('台灣的國家主權')">台灣的國家主權</button>
    `;
    tabsBox.innerHTML = LONGKAN_LESSONS.map((ls, i) =>
      `<button class="tab-btn ${i === 0 ? 'active' : ''}" onclick="loadLongkanLesson(${i})">${ls.tabName}</button>`
    ).join("");
    numpadCard.style.display = "none";
    lkGuideCard.style.display = "flex";
    otherGuideCard.style.display = "none";
    fcRhyme.style.display = "none";
    lkRhyme.style.display = "block";
    tsfBadge.textContent = "龍";
    tsfLabel.textContent = "龍龕 2.0 碼：";
    tsfPager.style.display = "none";
    composeHint.innerHTML = "📝 如喜龍龕 2.0 組字區（空白鍵/聲調鍵定界 ｜ 許氏按 <kbd style='color:var(--cyan);'>`</kbd> 臨時插字 ／ <kbd style='color:var(--gold);'>Shift+←+Tab</kbd> 合體熔煉）：";
  } else if (mode === "four_corner" || mode === "all") {
    hBadge.textContent = mode === "four_corner" ? "四角" : "7合1";
    hTitle.textContent = mode === "four_corner"
      ? "如喜（RuhI）— 智慧型四角號碼（4×5 小鍵盤）專屬拆碼教學工作台"
      : "如喜（RuhI）— 跨輸入法通用反查教練 ＆ 互動拆碼教學工作台";
    hSub.innerHTML = mode === "four_corner"
      ? "內建 76,299 筆四角混合構詞碼表（<strong>單字正統碼 · 直覺寬鬆容錯碼 · 二/三/四字詞 -前導連打</strong>）！"
      : "內建完整 545,778 筆龍龕形音 2.0 四軌詞庫 ＋ 76,299 筆四角混合構詞碼表 ＋ <strong>全系統 7 大輸入法雙向反查引擎</strong>！";
    hMotto.textContent = "🔢 胡適口訣：橫一垂二三點捺，叉四插五方塊六，七角八八九是小，點下有橫變零頭";
    lTitle.textContent = mode === "four_corner"
      ? "🔍 智慧型四角號碼逐字拆解 ＆ 連打查詢器（每字獨立成行，點擊詞塊或字卡立即播放拆角動畫）"
      : "🔍 跨輸入法整句即時反查 ＆ 相容碼思考推論教練（點選下方任一輸入法視角可切換為純淨專屬教學）";
    chipsBox.innerHTML = `
      <span>快速範例點擊：</span>
      <button class="chip-btn" onclick="setLookupText('龍龕形音輸入法')">龍龕形音輸入法</button>
      <button class="chip-btn" onclick="setLookupText('智慧型四角號碼輸入法')">智慧型四角號碼輸入法</button>
      <button class="chip-btn" onclick="setLookupText('台灣的國家主權')">台灣的國家主權</button>
      <button class="chip-btn" onclick="setLookupText('鋪龕肴凶區品森協蕊')">鋪龕肴凶區品森協蕊</button>
      <button class="chip-btn" onclick="setLookupText('台臺裡裏線綫為爲')">常見正異體字對照</button>
    `;
    tabsBox.innerHTML = `
      <button class="tab-btn" onclick="loadLesson(0)">單元 1：單字正統取碼（台 2360）</button>
      <button class="tab-btn" onclick="loadLesson(1)">單元 2：「灣」字怎麼拆？（灣 3212 / 直覺 3232）</button>
      <button class="tab-btn" onclick="loadLesson(2)">單元 3：「- 詞前導」二字詞 4 碼（台灣 -2332）</button>
      <button class="tab-btn active" onclick="loadLesson(3)">單元 4：三／四字詞「-首1,3[中1×n]尾2,4」</button>
      <button class="tab-btn" onclick="loadLesson(4)">單元 5：「- 詞前導 / + 分隔 / . 標點 / * 萬用」</button>
      <button class="tab-btn" onclick="loadLesson(5)">單元 6：「/ 字詞選單：1~0 快選、+/- 翻頁」</button>
    `;
    numpadCard.style.display = "block";
    lkGuideCard.style.display = "none";
    otherGuideCard.style.display = "none";
    fcRhyme.style.display = "block";
    lkRhyme.style.display = "none";
    tsfBadge.textContent = "四";
    tsfLabel.textContent = "四角碼：";
    tsfPager.style.display = "flex";
    composeHint.innerHTML = "📝 如喜輸入座組句區（按 <kbd style='color:var(--cyan);'>+</kbd> 分隔收攏 ／ <kbd style='color:var(--green);'>-</kbd> 詞前導）：";
  } else {
    const modeNames = {
      array30: ["行列", "📐 行列 30（筆形座標）專屬反查 ＆ 拆碼推論工作台", "首筆定十位(0~9行)、尾筆定個位(上^/中-/下v)，結合一二級簡碼與特別碼極速盲打"],
      cangjie: ["倉頡", "🔤 倉頡輸入法（哲理字根）專屬反查 ＆ 取碼推論工作台", "由左而右、由上而下、由外而內：連體字取「首二三尾」，分體字取「字首(首尾)＋字身(首二尾)」"],
      hsu: ["許氏", "🎵 許氏雙拼注音專屬反查 ＆ 音韻鍵位推論工作台", "依據音韻相近與字形聯想將 37 個注音符號映射於 25 個英文字母鍵，免跨數字列極速輸入"],
      standard: ["注音", "🎹 標準大千 ／ 倚天 26 鍵注音專屬反查工作台", "完整對照傳統大千直列鍵盤與倚天 26 鍵純英文字母鍵位映射"]
    };
    const info = modeNames[mode] || modeNames.array30;
    hBadge.textContent = info[0];
    hTitle.textContent = `如喜（RuhI）— ${info[1]}`;
    hSub.textContent = info[2];
    hMotto.textContent = info[2];
    lTitle.textContent = `🔍 ${info[1]}（每字獨立成行，點擊任一字列或字卡立即播放按鍵步驟動畫）`;
    chipsBox.innerHTML = `
      <span>快速範例點擊：</span>
      <button class="chip-btn" onclick="setLookupText('龍龕形音輸入法')">龍龕形音輸入法</button>
      <button class="chip-btn" onclick="setLookupText('台灣的國家主權')">台灣的國家主權</button>
      <button class="chip-btn" onclick="setLookupText('智慧型四角號碼')">智慧型四角號碼</button>
      <button class="chip-btn" onclick="setLookupText('台臺裡裏線綫為爲')">常見正異體字對照</button>
    `;
    tabsBox.innerHTML = `<span style="font-size:0.85rem; color:var(--gold); font-weight:700;">💡 點擊左側任一單字列或字卡，下方舞台立即分解演示該字的 ${info[0]} 拆碼步驟！</span>`;
    numpadCard.style.display = "none";
    lkGuideCard.style.display = "none";
    otherGuideCard.style.display = "flex";
    fcRhyme.style.display = "none";
    lkRhyme.style.display = "none";
    tsfBadge.textContent = info[0][0];
    tsfLabel.textContent = `${info[0]}碼：`;
    tsfPager.style.display = "none";
    composeHint.innerHTML = `📝 如喜 ${info[0]} 組字預覽區：`;
    otherGuideCard.innerHTML = `
      <div class="numpad-title" style="margin-bottom:4px;"><span>${info[1]} — 核心思考推論指南</span></div>
      <div class="guide-rule-box" style="border-left-color:var(--cyan);">
        <strong>📐 結構與鍵位推論原則</strong><br>${info[2]}
      </div>
      <div class="guide-rule-box" style="border-left-color:var(--gold);">
        <strong>🎬 互動式拆碼步進演示</strong><br>點擊左側任一字列或下方字卡，左側舞台即刻逐步亮顯該字的每一碼取碼位置與對應鍵位！
      </div>
    `;
  }
}

function setCrossImeMode(mode) {
  currentCrossImeMode = mode;
  document.querySelectorAll(".ime-mode-btn").forEach(btn => {
    btn.classList.toggle("active", btn.getAttribute("data-ime") === mode);
  });
  updateModeSpecificChrome();
  analyzeFullText();
  const raw = document.getElementById("lookupInput").value.trim();
  const firstChar = Array.from(raw).find(c => !/\s/.test(c));
  if (mode === "longkan") {
    if (firstChar && LONGKAN_DB[firstChar]) {
      animateLongkanChar(firstChar, 0, false);
    } else {
      loadLongkanLesson(0);
    }
  } else if (mode === "four_corner" || mode === "all") {
    loadLesson(3);
  } else if (firstChar) {
    animateOtherImeChar(firstChar, 0, false);
  }
}

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

function triggerCharStageAnim(ch, idx) {
  if (currentCrossImeMode === "longkan") {
    animateLongkanChar(ch, idx, true);
  } else if (currentCrossImeMode === "four_corner" || currentCrossImeMode === "all") {
    animateSingleCharCard(ch, idx);
  } else {
    animateOtherImeChar(ch, idx, true);
  }
}

function loadLongkanLesson(idx) {
  sequenceIdx = -1;
  const ls = LONGKAN_LESSONS[idx] || LONGKAN_LESSONS[0];
  LESSONS[6] = ls;
  curLesson = 6;
  curStep = 0;
  playing = true;
  document.getElementById("playPauseBtn").textContent = "⏸ 暫停動畫";
  document.querySelectorAll("#lessonTabs .tab-btn").forEach((b, i) => b.classList.toggle("active", i === idx));
  document.querySelectorAll(".char-detail-card").forEach(el => el.classList.remove("active-card"));
  document.querySelectorAll(".combo-token").forEach(el => el.classList.remove("active-token"));
  const badge = document.getElementById("nowPlayingBadge");
  if (badge) badge.textContent = `🎬 舞台播放中：龍龕單元 ${idx + 1}`;
  renderLessonStep();
}

function animateLongkanChar(ch, cardIdx, doFocus = true) {
  const lk = LONGKAN_DB[ch];
  if (!lk) return;
  const [bestCode, bestDesc, altRaw, structRaw] = lk;
  const structCode = structRaw || getCharStructCode(ch);
  const sMeta = getStructMeta(structCode);
  const cleanPrimary = formatLongkanDescWithPhonetics(bestDesc || "");
  const [part1, part2] = splitLongkanPrimaryParts(bestCode, bestDesc);
  const reasons = parseLongkanAltReasonings(ch, bestCode, bestDesc, altRaw);
  const { s1, s2, lbl1, lbl2 } = computeLongkanStepShapes(ch, structCode, part1, part2, bestCode, bestDesc);

  const steps = [];
  const mFirstComp = (bestDesc || "").match(/\(([a-z0-9]+)\)/i);
  const p1Code = (mFirstComp && mFirstComp[1] && bestCode.toLowerCase().startsWith(mFirstComp[1].toLowerCase()) && mFirstComp[1].length < bestCode.length)
    ? bestCode.slice(0, mFirstComp[1].length)
    : bestCode.slice(0, Math.max(1, Math.floor(bestCode.length / 2)));
  const p2Key = bestCode[p1Code.length] || bestCode[bestCode.length - 1] || "";
  steps.push({
    key: bestCode[0] || "",
    code: p1Code,
    activeSpots: [0],
    shapeZone: s1,
    shapeLabel: lbl1,
    desc: `${sMeta.p1Dir}取碼（首部碼）：依【${sMeta.badge}】鎖定首部【${part1}】➔ 輸入【${p1Code}】`,
    status: `${sMeta.p1Dir}（首部碼）：${p1Code}`,
    cands: [[ch, bestCode]],
    sel: 0
  });
  steps.push({
    key: p2Key,
    code: bestCode,
    activeSpots: [0, 1],
    shapeZone: s2,
    shapeLabel: lbl2,
    desc: `${sMeta.p2Dir}取碼（次部碼）：接續次部【${part2}】➔ 合成【${bestCode}】（${cleanPrimary}），按空白鍵或聲調鍵定界收攏至如喜輸入座！`,
    status: `主碼完成：${bestCode}（按空白鍵/聲調鍵定界，Enter 送出）`,
    cands: [[ch, `主碼 ${bestCode}`]],
    sel: 0,
    compose: ch
  });

  reasons.forEach((r, rIdx) => {
    if (r.code === bestCode && reasons.length > 1) return;
    const altShape = computeAltStepShape(ch, structCode, s1, s2, r);
    steps.push({
      key: r.code[0] || "",
      code: r.code,
      activeSpots: [0, 1, Math.min(3, 2 + (rIdx % 2))],
      shapeZone: altShape.shapeZone,
      shapeLabel: altShape.shapeLabel,
      desc: `${r.tagLabel}：${r.thought}`,
      status: `💡 相容推論碼：${r.code}（暫存於輸入座）`,
      cands: [[ch, r.tagLabel]],
      sel: 0,
      compose: ch
    });
  });

  const spot2 = reasons[0] ? `③相容: ${reasons[0].code}` : `③相容: 同步支援`;
  const spot3 = reasons[1] ? `④推論: ${reasons[1].code}` : `④雙字2+2: ${p1Code}+..`;

  LESSONS[6] = {
    title: `🐉 龍龕形音【${sMeta.badge}】拆碼動畫：「${ch}」（正軌碼 ${bestCode}：${cleanPrimary}）`,
    charHtml: ch,
    structType: structCode,
    structLabel: `${sMeta.pill} — 「${ch}」`,
    spots: [
      `${sMeta.p1Dir}: ${part1}`,
      `${sMeta.p2Dir}: ${part2}`,
      spot2,
      spot3
    ],
    steps: steps
  };
  curLesson = 6;
  curStep = 0;
  playing = true;
  document.getElementById("playPauseBtn").textContent = "⏸ 暫停動畫";
  renderLessonStep();
  if (doFocus) {
    focusAnimationStage(`龍龕「${ch}」(${bestCode} · ${sMeta.badge})`, { cardIdx: cardIdx, charVal: ch });
  }
}

function animateOtherImeChar(ch, cardIdx, doFocus = true) {
  const mode = currentCrossImeMode;
  const rec = CROSS_IME_DB[ch] || ["", "", "", ""];
  const structCode = getCharStructCode(ch);
  const sMeta = getStructMeta(structCode);
  const { s1, s2 } = computeLongkanStepShapes(ch, structCode, "", "", "", "");
  let mainCode = "";
  let spots = ["①起筆", "②中段", "③尾碼", "④完成"];
  const steps = [];

  if (mode === "array30") {
    mainCode = (rec[0] || "").toUpperCase();
    const extra = rec[1] || "";
    const keys = Array.from(rec[0] || "");
    spots = keys.slice(0, 4).map((k, i) => `第${i+1}碼: ${k.toUpperCase()}[${ARRAY30_COORDS[k]||k}]`);
    while (spots.length < 4) spots.push("空白鍵上屏");
    keys.forEach((k, i) => {
      const partial = keys.slice(0, i + 1).join("").toUpperCase();
      const z = i === 0 ? s1 : (i === keys.length - 1 ? s2 : `dual:${s1}+${s2}`);
      steps.push({
        key: k,
        code: partial,
        activeSpots: Array.from({length: Math.min(4, i + 1)}, (_, x) => x),
        shapeZone: z,
        shapeLabel: `第 ${i + 1} 碼：${k.toUpperCase()}〔${ARRAY30_COORDS[k] || k}〕`,
        desc: `第 ${i + 1} 碼：取筆形座標【${ARRAY30_COORDS[k] || k}】➔ 按鍵盤【${k.toUpperCase()}】`,
        status: `行列 30 第 ${i + 1} 碼 (${partial})`,
        cands: [[ch, partial]],
        sel: 0,
        compose: i === keys.length - 1 ? ch : undefined,
        commit: i === keys.length - 1 ? ch : undefined
      });
    });
    if (extra) {
      steps.push({
        key: " ",
        code: extra,
        activeSpots: [0,1,2,3],
        shapeZone: "whole",
        shapeLabel: `⚡ 簡碼／特別碼：${extra}`,
        desc: `💡 行列簡碼／特別碼推論：此字亦可直接打【${extra}】極速出字！`,
        status: `簡碼／特別碼：${extra}`,
        cands: [[ch, extra]],
        sel: 0,
        compose: ch,
        commit: ch
      });
    }
  } else if (mode === "cangjie") {
    const keys = Array.from(rec[2] || "");
    mainCode = keys.map(k => CANGJIE_RADS[k] || k).join("") + ` (${(rec[2]||"").toUpperCase()})`;
    spots = keys.slice(0, 4).map((k, i) => `第${i+1}碼: ${CANGJIE_RADS[k]||k}(${k.toUpperCase()})`);
    while (spots.length < 4) spots.push("空白鍵組字");
    keys.forEach((k, i) => {
      const partialRads = keys.slice(0, i + 1).map(x => CANGJIE_RADS[x] || x).join("");
      const partialKeys = keys.slice(0, i + 1).join("").toUpperCase();
      const z = i === 0 ? s1 : (i >= Math.ceil(keys.length / 2) ? s2 : s1);
      steps.push({
        key: k,
        code: `${partialRads} (${partialKeys})`,
        activeSpots: Array.from({length: Math.min(4, i + 1)}, (_, x) => x),
        shapeZone: z,
        shapeLabel: `第 ${i + 1} 碼：${CANGJIE_RADS[k] || k} (${k.toUpperCase()})`,
        desc: `第 ${i + 1} 碼：取倉頡字根【${CANGJIE_RADS[k] || k}】➔ 按鍵盤【${k.toUpperCase()}】`,
        status: `倉頡第 ${i + 1} 碼 (${partialRads})`,
        cands: [[ch, partialKeys]],
        sel: 0,
        compose: i === keys.length - 1 ? ch : undefined,
        commit: i === keys.length - 1 ? ch : undefined
      });
    });
  } else {
    const bpm = rec[3] || "";
    const hsu = bopomofoToHsu(bpm);
    const std = bopomofoToStandard(bpm);
    const et = bopomofoToEt26(bpm);
    mainCode = mode === "hsu" ? `${hsu} [${bpm}]` : `${std} / 倚:${et} [${bpm}]`;
    spots = [`注音: ${bpm}`, `許氏: ${hsu}`, `大千: ${std}`, `倚天: ${et}`];
    steps.push({
      key: "",
      code: mainCode,
      activeSpots: [0,1,2,3],
      shapeZone: "whole",
      shapeLabel: `🎵 讀音：${bpm} ➔ ${mode === "hsu" ? hsu : std}`,
      desc: mode === "hsu"
        ? `🎵 許氏雙拼推論：「${ch}」讀音為【${bpm}】➔ 依聲韻雙拼打【${hsu}】`
        : `🎹 注音鍵位對照：「${ch}」讀音為【${bpm}】➔ 大千打【${std}】｜ 倚天26鍵打【${et}】`,
      status: `注音拼讀完成：${ch}`,
      cands: [[ch, bpm]],
      sel: 0,
      compose: ch,
      commit: ch
    });
  }

  if (steps.length === 0) {
    steps.push({ key: "", code: "—", activeSpots: [], zone: null, desc: `「${ch}」查無此方案編碼`, status: "無編碼", cands: [[ch, "—"]], sel: 0 });
  }

  LESSONS[6] = {
    title: `🎯 「${ch}」【${sMeta.badge}】拆碼步驟動畫（${mainCode}）`,
    charHtml: ch,
    structType: structCode,
    structLabel: `${sMeta.pill} — 「${ch}」`,
    spots: spots,
    steps: steps
  };
  curLesson = 6;
  curStep = 0;
  playing = true;
  document.getElementById("playPauseBtn").textContent = "⏸ 暫停動畫";
  renderLessonStep();
  if (doFocus) {
    focusAnimationStage(`「${ch}」(${mainCode})`, { cardIdx: cardIdx, charVal: ch });
  }
}

function analyzeFullText() {
  const raw = document.getElementById("lookupInput").value.trim();
  const banner = document.getElementById("comboBanner");
  const grid = document.getElementById("charCardsGrid");
  const matrixEl = document.getElementById("crossImeMatrix");

  if (!raw) {
    banner.innerHTML = "<span style='color:var(--text-muted);'>請在上方輸入框輸入任意中文字詞或句子...</span>";
    grid.innerHTML = "";
    if (matrixEl) matrixEl.innerHTML = "";
    return;
  }

  const chars = Array.from(raw).filter(c => !/\s/.test(c));
  const mode = currentCrossImeMode;

  // =========================================================================
  // 1. 渲染「逐字獨立成行 ＋ 相容碼思考推論引導」面板 (#crossImeMatrix)
  // =========================================================================
  if (matrixEl) {
    matrixEl.style.display = "flex";
    if (mode === "longkan") {
      matrixEl.innerHTML = `
        <div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:6px;">
          <strong style="color:var(--gold); font-size:0.92rem;">🐉 龍龕形音 2.0 — 逐字四軌拆解與「思考推論引導」（每字獨立一行，點擊任一列立即播放動畫）：</strong>
          <span style="font-size:0.78rem; color:var(--cyan);">✨ 四軌合一（軌道 H 許氏部件全拼 · 軌道 P 拼音全拼 · 軌道 V 徐冰式純視覺字母 · 軌道 A/C 雙拼與 C×N）＋ 空白/聲調定界</span>
        </div>
        ${renderPerCharLinesHtml("longkan", raw)}
      `;
    } else if (mode !== "all") {
      const prov = IME_PROVIDERS_REGISTRY.find(p => p.id === mode) || IME_PROVIDERS_REGISTRY[0];
      matrixEl.innerHTML = `
        <div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:6px;">
          <strong style="color:var(--gold); font-size:0.92rem;">${prov.name} — 逐字拆解與思考推論引導（每字獨立一行，點擊任一列立即播放動畫）：</strong>
          <span style="font-size:0.78rem; color:var(--cyan);">點擊任一字列即可在右側舞台觀看分解動畫</span>
        </div>
        ${renderPerCharLinesHtml(mode, raw)}
      `;
    } else {
      let mHtml = `<div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:6px;">
        <strong style="color:var(--cyan); font-size:0.88rem;">🌐 全輸入法整句逐字反查對照表（每字獨立成行，含相容碼思考推論引導）：</strong>
        <span style="font-size:0.76rem; color:var(--gold);">💡 建議點選上方單一輸入法按鈕（如「🐉 龍龕形音」），可切換為無干擾的純淨專屬教學視角！</span>
      </div>`;
      IME_PROVIDERS_REGISTRY.forEach(prov => {
        mHtml += `<div class="cim-row">
          <span class="cim-name">${prov.name}</span>
          <div class="cim-val">${prov.inspectSentence(raw)}</div>
        </div>`;
      });
      matrixEl.innerHTML = mHtml;
    }
  }

  // =========================================================================
  // 2. 渲染「整串連打攻略條」(#comboBanner) —— 嚴格依當前選取的輸入法純淨化渲染！
  // =========================================================================
  if (mode === "longkan") {
    let phraseHint = "";
    if (chars.length >= 2) {
      const lk0 = LONGKAN_DB[chars[0]];
      const lk1 = LONGKAN_DB[chars[1]];
      if (lk0 && lk1) {
        const twoWordCode = lk0[0].slice(0, 2) + lk1[0].slice(0, 2);
        phraseHint = `<span style="font-size:0.82rem; color:var(--gold); background:#1e293b; padding:2px 10px; border-radius:6px; border:1px solid var(--border);">
          📐 雙字詞 2+2 連打範例（「${chars[0]}${chars[1]}」＝ ${chars[0]}前2碼 <code>${lk0[0].slice(0,2)}</code> ＋ ${chars[1]}前2碼 <code>${lk1[0].slice(0,2)}</code>）：<strong>${twoWordCode}</strong>
        </span>`;
      }
    }
    let bHtml = `<div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:8px;">
      <strong style="color:var(--green);">🚀 龍龕形音 2.0 整句連打序列（可變碼長＋空白鍵/聲調鍵定界 ｜ 許氏按 \` 臨時插字 · Shift+←+Tab 合體熔煉，點擊字塊看動畫）：</strong>
      <div style="display:flex; align-items:center; gap:10px; flex-wrap:wrap;">${phraseHint}</div>
    </div><div class="combo-tokens">`;
    chars.forEach((ch, idx) => {
      const lk = LONGKAN_DB[ch];
      const code = lk ? lk[0] : "—";
      const cleanDesc = lk ? (lk[1] || "").replace(/^[^:]+:/, "") : "標點";
      bHtml += `<div class="combo-token" data-token="${ch}" onclick="sequenceIdx=-1; animateLongkanChar('${ch}', ${idx}, true)">
        <span class="ct-word">${ch}</span>
        <span class="ct-code">${code}</span>
        <span class="ct-keyhint">${cleanDesc}</span>
      </div>`;
      if (idx < chars.length - 1) {
        bHtml += `<span style="color:var(--text-muted); font-weight:bold;">➔</span>`;
      }
    });
    bHtml += `</div>`;
    banner.innerHTML = bHtml;
  } else if (mode === "four_corner" || mode === "all") {
    const tokens = segmentBestCombo(raw);
    const dynWhole = computeDynamicPhraseCode(raw);
    let dynHintHtml = "";
    if (dynWhole) {
      dynHintHtml = `<span style="font-size:0.82rem; color:var(--gold); background:#1e293b; padding:2px 10px; border-radius:6px; border:1px solid var(--border);">
        📐 整詞公式（${dynWhole.formula}）：<strong>${dynWhole.code}</strong>
      </span>`;
    }
    let bannerHtml = `<div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:8px;">
      <strong style="color:var(--green);">🚀 四角號碼整串最速連打攻略（二字詞 1,2+1,2 四碼 ／ 三字詞以上 1,3[1×n]2,4 五～六碼，點擊詞塊看動畫）：</strong>
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
        rkHint = `[/]選單➔[${tk.rank === 10 ? 0 : tk.rank}]快選`;
      } else {
        const pgFlips = Math.floor((tk.rank - 1) / 10);
        const keyNum = tk.rank % 10;
        rkHint = `[/]選單➔[+下頁${pgFlips > 1 ? '×' + pgFlips : ''}]➔[${keyNum}]`;
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
      if (idx < tokens.length - 1) {
        bannerHtml += `<span style="color:var(--text-muted); font-weight:bold;">➔</span>`;
      }
    });
    bannerHtml += `</div>`;
    banner.innerHTML = bannerHtml;
  } else {
    const prov = IME_PROVIDERS_REGISTRY.find(p => p.id === mode) || IME_PROVIDERS_REGISTRY[0];
    let bHtml = `<div style="display:flex; justify-content:space-between; align-items:center; flex-wrap:wrap; gap:8px;">
      <strong style="color:var(--green);">🚀 ${prov.name} 整句連打按鍵序列（點擊任一字塊觀看按鍵分解動畫）：</strong>
    </div><div class="combo-tokens">`;
    chars.forEach((ch, idx) => {
      const infoStr = prov.inspectChar(ch) || "—";
      const mainPart = infoStr.split(" ")[0] || infoStr;
      bHtml += `<div class="combo-token" data-token="${ch}" onclick="sequenceIdx=-1; animateOtherImeChar('${ch}', ${idx}, true)">
        <span class="ct-word">${ch}</span>
        <span class="ct-code">${mainPart}</span>
        <span class="ct-keyhint">${infoStr}</span>
      </div>`;
      if (idx < chars.length - 1) {
        bHtml += `<span style="color:var(--text-muted); font-weight:bold;">➔</span>`;
      }
    });
    bHtml += `</div>`;
    banner.innerHTML = bHtml;
  }

  // =========================================================================
  // 3. 渲染「逐字拆解詳情卡片」(#charCardsGrid) —— 單一輸入法模式下絕不混雜其他方案資訊！
  // =========================================================================
  let cardsHtml = "";
  chars.forEach((ch, idx) => {
    if (mode === "longkan") {
      const lk = LONGKAN_DB[ch];
      if (!lk) {
        cardsHtml += `<div class="char-detail-card"><div class="cdc-header"><span class="cdc-char">${ch}</span><span class="cdc-maincode">標點/其他</span></div></div>`;
        return;
      }
      const [bestCode, bestDesc, altRaw, structRaw] = lk;
      const sMeta = getStructMeta(structRaw || "single");
      const [part1, part2] = splitLongkanPrimaryParts(bestCode, bestDesc);
      const reasons = parseLongkanAltReasonings(ch, bestCode, bestDesc, altRaw);
      let badgesHtml = "";
      reasons.forEach(r => {
        badgesHtml += `<div class="reason-item" style="width:100%;">
          <span class="reason-tag ${r.tagClass}">${r.tagLabel}</span>
          <span class="reason-code">${r.code}</span>
          <span class="reason-explain" style="font-size:0.75rem;">${r.cleanDesc}</span>
        </div>`;
      });
      badgesHtml += `<span class="cdc-badge b-how">⌨️ 龍龕 2.0 輸入方式：打主碼 <strong>${bestCode}</strong> 或上列任一軌道碼（加空白鍵/聲調鍵）直取</span>`;

      cardsHtml += `<div class="char-detail-card" data-char="${ch}" onclick="sequenceIdx=-1; animateLongkanChar('${ch}', ${idx}, true)">
        <div class="cdc-header">
          <div>
            <span class="cdc-char">${ch}</span>
            <span style="font-size:0.74rem; background:#1e293b; color:var(--cyan); border:1px solid rgba(56,189,248,0.4); border-radius:4px; padding:1px 6px; margin-left:4px; font-weight:800;">${sMeta.badge} ▶</span>
          </div>
          <span class="cdc-maincode" title="龍龕形音 2.0 代表主碼">${bestCode}</span>
        </div>
        <div class="cdc-corners">
          <div class="cdc-c-item"><span style="color:var(--cyan);">${sMeta.p1Dir}</span><strong>${part1}</strong></div>
          <div class="cdc-c-item"><span style="color:var(--gold);">${sMeta.p2Dir}</span><strong>${part2}</strong></div>
        </div>
        <div class="cdc-badges">${badgesHtml}</div>
      </div>`;
      return;
    }

    if (mode !== "four_corner" && mode !== "all") {
      const prov = IME_PROVIDERS_REGISTRY.find(p => p.id === mode) || IME_PROVIDERS_REGISTRY[0];
      const val = prov.inspectChar(ch) || "—";
      const shortCode = val.split(" ")[0] || val;
      cardsHtml += `<div class="char-detail-card" data-char="${ch}" onclick="sequenceIdx=-1; animateOtherImeChar('${ch}', ${idx}, true)">
        <div class="cdc-header">
          <div>
            <span class="cdc-char">${ch}</span>
            <span style="font-size:0.75rem; color:var(--cyan); margin-left:4px;">點擊看拆碼動畫 ▶</span>
          </div>
          <span class="cdc-maincode">${shortCode}</span>
        </div>
        <div class="cdc-badges">
          <span class="cdc-badge b-how">${prov.name}：<strong>${val}</strong></span>
        </div>
      </div>`;
      return;
    }

    // four_corner 或 all 模式
    const inf = CHAR_DB[ch];
    if (!inf) {
      cardsHtml += `<div class="char-detail-card">
        <div class="cdc-header"><span class="cdc-char">${ch}</span><span class="cdc-maincode">符號/其他</span></div>
        <div style="font-size:0.82rem; color:var(--text-muted);">可透過小鍵盤 [.] 標點選單輸入</div>
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
    badgesHtml += `<span class="cdc-badge b-how">⌨️ 單字輸入方式：${howStr}</span>`;

    const crossBlockHtml = mode === "all" ? (() => {
      const rec = CROSS_IME_DB[ch];
      const lkStr = IME_PROVIDERS_REGISTRY[0].inspectChar(ch) || "—";
      const ar30Str = IME_PROVIDERS_REGISTRY[2].inspectChar(ch) || "—";
      const cjStr = IME_PROVIDERS_REGISTRY[3].inspectChar(ch) || "—";
      const bpm = rec ? (rec[3] || "") : "";
      const phonStr = bpm ? `${bpm} ｜ 許:${bopomofoToHsu(bpm)} · 大千:${bopomofoToStandard(bpm)} · 倚:${bopomofoToEt26(bpm)}` : "—";
      return `<div class="cdc-cross-ime">
        <div class="cdc-ime-row"><span style="color:var(--gold); font-weight:700;">🐉 龍龕2.0</span><strong style="color:var(--gold);">${lkStr}</strong></div>
        <div class="cdc-ime-row"><span>📐 行列30</span><strong>${ar30Str}</strong></div>
        <div class="cdc-ime-row"><span>🔤 倉頡碼</span><strong>${cjStr}</strong></div>
        <div class="cdc-ime-row"><span>🎵 注音碼</span><strong>${phonStr}</strong></div>
      </div>`;
    })() : "";

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
      ${crossBlockHtml}
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
      key: "/",
      code: code4,
      activeSpots: [0,1,2,3],
      zone: null,
      desc: `選字步驟：「${ch}」位於第 #${r4} 順位 ➔ 先按小鍵盤【/】進入選字模式，再按【${selKey}】（或連按 / 跳選）即可收攏至輸入座！`,
      status: `按 [/] ➔ [${selKey}] 選取第 #${r4} 候選字`,
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
  if (currentCrossImeMode === "longkan") {
    const chs = Array.from(raw).filter(c => !/\s/.test(c));
    if (chs.length > 0) {
      sequenceTokens = chs.map(c => ({ text: c, code: (LONGKAN_DB[c] || [""])[0], isWord: false, rank: 1 }));
      sequenceIdx = 0;
      animateLongkanChar(chs[0], 0, true);
    }
    return;
  }
  if (currentCrossImeMode !== "four_corner" && currentCrossImeMode !== "all") {
    const chs = Array.from(raw).filter(c => !/\s/.test(c));
    if (chs.length > 0) {
      sequenceTokens = chs.map(c => ({ text: c, code: "", isWord: false, rank: 1 }));
      sequenceIdx = 0;
      animateOtherImeChar(chs[0], 0, true);
    }
    return;
  }
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

  renderSvgHighlight(L, S);

  const hl = document.getElementById("hlZone");
  if (S.zone && !S.shapeZone) {
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
    if (currentCrossImeMode === "longkan") {
      animateLongkanChar(tk.text, sequenceIdx, true);
    } else if (currentCrossImeMode !== "four_corner" && currentCrossImeMode !== "all") {
      animateOtherImeChar(tk.text, sequenceIdx, true);
    } else {
      animateCustomToken(tk.text, tk.code, tk.isWord, tk.rank, sequenceIdx);
    }
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

// 動態建立龍龕形音反向檢索表，供下方互動試打沙盒支援 a~z 龍龕打字與 Enter 送出漢字
const LONGKAN_CODE_CANDS = (() => {
  const map = {};
  const add = (cd, ch, label) => {
    if (!cd) return;
    const k = cd.toLowerCase();
    if (!map[k]) map[k] = [];
    if (!map[k].some(x => x[0] === ch)) map[k].push([ch, label]);
  };
  for (const [ch, lk] of Object.entries(LONGKAN_DB)) {
    const [bestCode, bestDesc, altRaw] = lk;
    add(bestCode, ch, bestCode.toUpperCase());
    if (altRaw) {
      for (const pair of altRaw.split("｜")) {
        const cd = pair.split("=")[0];
        if (cd) add(cd, ch, cd.toUpperCase());
      }
    }
  }
  return map;
})();

function lookupSandbox(code) {
  if (!code) return [["台","2360"],["台灣","-2332"],["輸入法","-50843"],["四角號碼","-622612"],["天才","-1040"]];
  const low = code.toLowerCase();
  if (/^[a-z]/.test(low.replace(/^-/, ""))) {
    const cleanLow = low.replace(/^-/, "");
    if (LONGKAN_CODE_CANDS[cleanLow]) return LONGKAN_CODE_CANDS[cleanLow];
    const lkRes = [];
    for (const [k, arr] of Object.entries(LONGKAN_CODE_CANDS)) {
      if (k.startsWith(cleanLow)) {
        for (const item of arr) {
          if (!lkRes.some(x => x[0] === item[0])) lkRes.push([item[0], k.toUpperCase()]);
          if (lkRes.length >= 10) break;
        }
      }
      if (lkRes.length >= 10) break;
    }
    return lkRes.length ? lkRes : [["(無匹配候選)","提示"]];
  }
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
  } else if (/^[a-zA-Z]$/.test(k)) {
    if (sandboxCode === ".") sandboxCode = "";
    sandboxSelecting = false;
    const rawLen = sandboxCode.replace(/^-/, "").length;
    if (rawLen >= 4 && !sandboxCode.startsWith("-") && cands.length > 0 && !cands[0][0].startsWith("(")) {
      sandboxCompose += cands[sandboxSel % cands.length][0];
      document.getElementById("composeDeskOut").textContent = sandboxCompose;
      sandboxCode = k.toLowerCase();
    } else if (rawLen < 6) {
      sandboxCode += k.toLowerCase();
    }
    sandboxSel = 0;
  } else if (k === " ") {
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
    /^[0-9a-zA-Z*+\-./ ]$/.test(e.key) ||
    e.key === "Backspace" ||
    e.key === "Enter" ||
    e.key === "PageDown" ||
    e.key === "PageUp" ||
    e.key === "ArrowDown" ||
    e.key === "ArrowUp"
  ) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    e.preventDefault();
    pressVirtualKey(e.key);
  }
});

setInterval(() => {
  if (playing) {
    nextStep();
  }
}, 2400);

// 初始化：支援網址參數 ?ime=longkan|four_corner|array30|cangjie|hsu|standard|all，預設進入純淨「龍龕形音 (longkan)」教學視角
(function initWorkbench() {
  const params = new URLSearchParams(window.location.search);
  const initIme = params.get("ime") || "longkan";
  const initQuery = params.get("q");
  if (initQuery) {
    document.getElementById("lookupInput").value = initQuery;
  }
  setCrossImeMode(initIme);
})();
</script>
</body>
</html>
"""


def main():
    print("[1/2] 正在從 four_corner.tsv 與跨輸入法字典抽取單字拆碼、詞彙簡碼與 7 大方案反查索引...")
    compact_chars, compact_words, code_top_cands = build_dictionary_payload()
    cross_db, longkan_db = build_cross_ime_and_longkan_payloads(compact_chars)
    html = (
        HTML_TEMPLATE.replace("__CHAR_DB_JSON__", json.dumps(compact_chars, ensure_ascii=False, separators=(",", ":")))
        .replace("__WORD_DB_JSON__", json.dumps(compact_words, ensure_ascii=False, separators=(",", ":")))
        .replace("__CODE_CANDS_JSON__", json.dumps(code_top_cands, ensure_ascii=False, separators=(",", ":")))
        .replace("__CROSS_IME_DB_JSON__", json.dumps(cross_db, ensure_ascii=False, separators=(",", ":")))
        .replace("__LONGKAN_DB_JSON__", json.dumps(longkan_db, ensure_ascii=False, separators=(",", ":")))
    )
    OUT_HTML.write_text(html, encoding="utf-8")
    if LEGACY_HTML.exists() and LEGACY_HTML != OUT_HTML:
        LEGACY_HTML.unlink()
        print(f"      -> 已移除舊檔名頁面：{LEGACY_HTML}")
    dist_docs = ROOT / "dist" / "four_corner_schema" / "docs"
    if dist_docs.exists():
        (dist_docs / "ruhi_interactive_workbench.html").write_text(html, encoding="utf-8")
        legacy_dist = dist_docs / "four_corner_tutorial_anim.html"
        if legacy_dist.exists():
            legacy_dist.unlink()
    print(f"[2/2] 成功更新互動教學與跨輸入法查碼頁面：{OUT_HTML} ({OUT_HTML.stat().st_size / 1024:.1f} KB)")
    print(f"      -> 內建四角單字：{len(compact_chars):,} 字｜龍龕形音：{len(longkan_db):,} 字｜跨方案對照：{len(cross_db):,} 字｜詞彙簡碼：{len(compact_words):,} 詞")


if __name__ == "__main__":
    main()
