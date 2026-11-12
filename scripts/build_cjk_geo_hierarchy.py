#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
build_cjk_geo_hierarchy.py
==========================
建構漢字圈（台灣、日本、韓國、中國大陸與港澳）條件式階層郵政／地政／戶政知識樹（cjk_geo_hierarchy.tsv）。

欄位規格（Tab 分隔 TSV）：
    country<TAB>level<TAB>parent<TAB>entity<TAB>postal_code<TAB>category<TAB>children_csv<TAB>l1_root

遵循 ATG 專案規範：
    1. 純 Python CLI 介面（含 argparse 與繁體中文說明）。
    2. 資料與全域詞庫（lexicon_domain.tsv）完全分離，僅於上層行政區錨點（Anchor）在 Viterbi
       路徑中被觸發時，才由 geo_hierarchy.rs 條件式喚醒下層鄰里、路街、地段、町丁與洞名。
"""

import argparse
import os
import sys
from typing import List, Tuple

# (country, level, parent, entity, postal_code, category, children_csv, l1_root)
GeoRow = Tuple[str, str, str, str, str, str, str, str]


def ensure_open_data_sources(repo_root: str):
    """確保 open_data 目錄下的 tw_zipcode_3plus3.tsv 與 tw_villages.tsv 存在且完整。"""
    open_data_dir = os.path.join(repo_root, "ruhi_lexicon", "data", "open_data")
    os.makedirs(open_data_dir, exist_ok=True)
    out_zip = os.path.join(open_data_dir, "tw_zipcode_3plus3.tsv")
    out_vil = os.path.join(open_data_dir, "tw_villages.tsv")

    scratch_dirs = [
        r"C:\Users\Yuktesha\.gemini\antigravity\brain\efe74b09-b76e-469d-a21a-7bbb8ec40671\scratch",
        os.path.join(repo_root, "scratch"),
        os.path.join(os.path.dirname(repo_root), "scratch"),
    ]
    zipcode_txt = None
    counties_csv = None
    towns_csv = None
    villages_csv = None
    for sd in scratch_dirs:
        if os.path.exists(os.path.join(sd, "zipcode.txt")):
            zipcode_txt = os.path.join(sd, "zipcode.txt")
            counties_csv = os.path.join(sd, "counties.csv")
            towns_csv = os.path.join(sd, "towns.csv")
            villages_csv = os.path.join(sd, "villages.csv")
            break

    if zipcode_txt and os.path.exists(zipcode_txt):
        with open(zipcode_txt, encoding="utf-8") as fin, open(out_zip, "w", encoding="utf-8", newline="\n") as fout:
            fout.write("# 中華郵政 3+3 郵遞區號公開資料 V2603\n")
            fout.write("# 格式: #縣市 / @鄉鎮市區\t3碼前綴 / 路街名\t3碼尾綴:規則[|大宗戶]\t...\n")
            for line in fin:
                line = line.strip("\r\n")
                if not line or line.startswith("# ") or line.startswith("##"):
                    continue
                parts = line.split("\t")
                if parts[0].startswith("#"):
                    c_clean = parts[0][1:].strip()
                    if c_clean:
                        fout.write(f"#{c_clean}\n")
                elif parts[0].startswith("@"):
                    pfx = parts[2] if len(parts) > 2 else ""
                    fout.write(f"{parts[0]}\t{pfx}\n")
                else:
                    rules = parts[2:]
                    fout.write(f"{parts[0]}\t" + "\t".join(rules) + "\n")

    if villages_csv and os.path.exists(villages_csv):
        import csv
        from collections import defaultdict
        counties_map = dict(csv.reader(open(counties_csv, encoding="utf-8")))
        towns_map = dict(csv.reader(open(towns_csv, encoding="utf-8")))
        vil_list = list(csv.reader(open(villages_csv, encoding="utf-8")))[1:]
        town_villages = defaultdict(list)
        for vid, vname in vil_list:
            vname = vname.strip()
            if vname:
                town_villages[vid[:8]].append(vname)
        with open(out_vil, "w", encoding="utf-8", newline="\n") as fout:
            fout.write("# 政府資料開放平臺內政部村里清單 (8,044 村里 / 368 鄉鎮市區)\n")
            fout.write("# 格式: 縣市\t鄉鎮市區\t村里1,村里2,...\n")
            for tid, tname in list(towns_map.items())[1:]:
                cname = counties_map[tid[:5]]
                vils = town_villages.get(tid, [])
                fout.write(f"{cname}\t{tname}\t" + ",".join(vils) + "\n")


def build_taiwan_hierarchy() -> List[GeoRow]:
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ensure_open_data_sources(repo_root)

    open_data_dir = os.path.join(repo_root, "ruhi_lexicon", "data", "open_data")
    out_zip = os.path.join(open_data_dir, "tw_zipcode_3plus3.tsv")
    out_vil = os.path.join(open_data_dir, "tw_villages.tsv")

    # 1. 讀取內政部村里開放資料
    town_villages = {}
    with open(out_vil, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#"):
                continue
            p = line.strip("\r\n").split("\t")
            if len(p) >= 3:
                c, t, v = p[0], p[1], p[2]
                vils = [x for x in v.split(",") if x]
                town_villages[(c, t)] = vils

    # 2. 讀取中華郵政 3+3 郵遞區號公開資料
    county_towns = {}
    town_prefixes = {}
    town_roads = {}
    cur_c = ""
    cur_t = ""
    cur_pfx = ""
    with open(out_zip, encoding="utf-8") as f:
        for line in f:
            line = line.strip("\r\n")
            if not line or line.startswith("# ") or line.startswith("##"):
                continue
            p = line.split("\t")
            if p[0].startswith("#"):
                cur_c = p[0][1:].strip()
                if not cur_c:
                    continue
                if cur_c not in county_towns:
                    county_towns[cur_c] = []
            elif p[0].startswith("@"):
                cur_t = p[0][1:].strip()
                cur_pfx = p[1].strip() if len(p) > 1 else ""
                if cur_c and cur_t not in county_towns.setdefault(cur_c, []):
                    county_towns[cur_c].append(cur_t)
                town_prefixes[(cur_c, cur_t)] = cur_pfx
            else:
                road_name = p[0]
                rules = p[1:]
                town_roads.setdefault((cur_c, cur_t), []).append((road_name, cur_pfx, rules))

    # 地政主要地段清單
    cadastral_sections = [
        ("七堵區", "瑪陵坑段", "206000", "基隆市"),
        ("中正區", "正濱段", "202000", "基隆市"),
        ("中正區", "社寮段", "202000", "基隆市"),
        ("中正區", "城中段", "100005", "臺北市"),
        ("信義區", "興雅段", "110058", "臺北市"),
        ("板橋區", "新板段", "220063", "新北市"),
    ]
    town_cadastral = {}
    for town, sec, code, l1 in cadastral_sections:
        town_cadastral.setdefault((l1, town), []).append(sec)

    explicit_village_postal = {
        ("基隆市", "中正區", "社寮里"): "202009",
        ("基隆市", "中正區", "平寮里"): "202009",
        ("基隆市", "中正區", "和憲里"): "202008",
        ("基隆市", "中正區", "正濱里"): "202008",
    }

    rows: List[GeoRow] = []

    # 1. 台灣 22 縣市 L1 (含 臺 / 台 雙寫法)
    for cname, towns in county_towns.items():
        if not cname:
            continue
        pfx3 = town_prefixes.get((cname, towns[0]), "") if towns else ""
        cat = "city" if cname.endswith("市") else "county"
        rows.append(("TW", "L1", "ROOT", cname, f"{pfx3}000", cat, ",".join(towns), cname))
        if "臺" in cname:
            alt_c = cname.replace("臺", "台")
            rows.append(("TW", "L1", "ROOT", alt_c, f"{pfx3}000", cat, ",".join(towns), alt_c))

    # 2. 台灣 368 鄉鎮市區 L2
    for cname, towns in county_towns.items():
        for tname in towns:
            pfx3 = town_prefixes.get((cname, tname), "")
            vils = town_villages.get((cname, tname), [])
            if not vils and "臺" in cname:
                vils = town_villages.get((cname.replace("臺", "台"), tname), [])
            elif not vils and "台" in cname:
                vils = town_villages.get((cname.replace("台", "臺"), tname), [])
            cad_secs = town_cadastral.get((cname, tname), [])
            children_items = vils + cad_secs
            cat = "district" if tname.endswith("區") else ("township" if tname.endswith("鄉") else "city")
            rows.append(("TW", "L2", cname, tname, f"{pfx3}000", cat, ",".join(children_items), cname))
            if "臺" in cname:
                alt_c = cname.replace("臺", "台")
                rows.append(("TW", "L2", alt_c, tname, f"{pfx3}000", cat, ",".join(children_items), cname))

    # 3. 台灣 8,044 村里 L3 (category = "village")
    for (cname, tname), vils in town_villages.items():
        pfx3 = town_prefixes.get((cname, tname), "")
        if not pfx3 and "臺" in cname:
            pfx3 = town_prefixes.get((cname.replace("臺", "台"), tname), "")
        elif not pfx3 and "台" in cname:
            pfx3 = town_prefixes.get((cname.replace("台", "臺"), tname), "")

        ordered_vils = list(vils)
        if cname == "基隆市" and tname == "中正區":
            if "社寮里" in ordered_vils and "平寮里" in ordered_vils:
                idx_she = ordered_vils.index("社寮里")
                idx_ping = ordered_vils.index("平寮里")
                if idx_she > idx_ping:
                    ordered_vils.remove("社寮里")
                    ordered_vils.insert(idx_ping, "社寮里")

        for vname in ordered_vils:
            code = explicit_village_postal.get((cname, tname, vname), f"{pfx3}000")
            rows.append(("TW", "L3", tname, vname, code, "village", "", cname))

    # 4. 地政地段 L3 (category = "cadastral_section")
    for town, sec, code, l1 in cadastral_sections:
        rows.append(("TW", "L3", town, sec, code, "cadastral_section", "", l1))

    # 5. 中華郵政全臺 45,079 條路街段 L3 (category = "road")
    for (cname, tname), roads in town_roads.items():
        for (road_name, pfx3, rules) in roads:
            combined_rules = []
            for r in rules:
                if ":" in r:
                    code_part, cond_part = r.split(":", 1)
                    full_code = f"{pfx3}{code_part}" if len(code_part) == 3 else code_part
                    combined_rules.append(f"{full_code}:{cond_part}")
                else:
                    combined_rules.append(r)
            code_spec = ";".join(combined_rules)
            rows.append(("TW", "L3", tname, road_name, code_spec, "road", "", cname))

    return rows


def build_japan_hierarchy() -> List[GeoRow]:
    rows: List[GeoRow] = []

    # 日本 47 都道府縣 -> 主要市區町村
    jp_l1 = [
        (
            "JP",
            "L1",
            "ROOT",
            "東京都",
            "〒100",
            "prefecture",
            "千代田區,中央區,港區,新宿區,文京區,台東區,臺東區,墨田區,江東區,品川區,目黑區,大田區,世田谷區,澀谷區,中野區,杉並區,豐島區,北區,荒川區,板橋區,練馬區,足立區,葛飾區,江戶川區,八王子市,立川市,武藏野市,三鷹市,町田市",
        ),
        (
            "JP",
            "L1",
            "ROOT",
            "北海道",
            "〒060",
            "prefecture",
            "札幌市,函館市,小樽市,旭川市,室蘭市,釧路市,帶廣市,北見市,夕張市,岩見澤市,網走市,留萌市,苫小牧市,稚內市,美唄市,蘆別市,江別市,赤平市,紋別市,士別市,名寄市,三笠市,根室市,千歲市,瀧川市,砂川市,歌志內市,深川市,富良野市,登別市,惠庭市,伊達市,北廣島市,石狩市,北斗市",
        ),
        (
            "JP",
            "L1",
            "ROOT",
            "京都府",
            "〒600",
            "prefecture",
            "京都市,福知山市,舞鶴市,綾部市,宇治市,宮津市,龜岡市,城陽市,向日市,長岡京市,八幡市,京田邊市,京丹後市,南丹市,木津川市",
        ),
        (
            "JP",
            "L1",
            "ROOT",
            "大阪府",
            "〒530",
            "prefecture",
            "大阪市,堺市,岸和田市,豐中市,池田市,吹田市,泉大津市,高槻市,貝塚市,守口市,枚方市,茨木市,八尾市,泉佐野市,富田林市,寢屋川市,河內長野市,松原市,大東市,和泉市,箕面市,柏原市,羽曳野市,門真市,攝津市,高石市,藤井寺市,東大阪市,泉南市,四條畷市,交野市,大阪狹山市,阪南市",
        ),
        (
            "JP",
            "L1",
            "ROOT",
            "神奈川縣",
            "〒220",
            "prefecture",
            "橫濱市,川崎市,相模原市,橫須賀市,平塚市,鎌倉市,藤澤市,小田原市,茅崎市,逗子市,三浦市,秦野市,厚木市,大和市,伊勢原市,海老名市,座間市,南足柄市,綾瀨市",
        ),
        ("JP", "L1", "ROOT", "愛知縣", "〒450", "prefecture", "名古屋市,豐橋市,岡崎市,一宮市,瀨戶市,半田市,春日井市,豐川市,津島市,碧南市,刈谷市,豐田市,安城市,西尾市,蒲郡市,犬山市,常滑市,江南市,小牧市"),
        ("JP", "L1", "ROOT", "福岡縣", "〒810", "prefecture", "北九州市,福岡市,大牟田市,久留米市,直方市,飯塚市,田川市,柳川市,八女市,筑後市,大川市,行橋市,豐前市,中間市,小郡市,筑紫野市,春日市,大野城市,宗像市,太宰府市"),
        ("JP", "L1", "ROOT", "沖繩縣", "〒900", "prefecture", "那霸市,宜野灣市,石垣市,浦添市,名護市,糸滿市,沖繩市,豐見城市,宇流麻市,宮古島市,南城市"),
        ("JP", "L1", "ROOT", "兵庫縣", "〒650", "prefecture", "神戶市,姬路市,尼崎市,明石市,西宮市,洲本市,蘆屋市,伊丹市,相生市,豐岡市,加古川市,赤穗市,寶塚市,三木市"),
        ("JP", "L1", "ROOT", "千葉縣", "〒260", "prefecture", "千葉市,銚子市,市川市,船橋市,館山市,木更津市,松戶市,野田市,茂原市,成田市,佐倉市,浦安市,柏市"),
        ("JP", "L1", "ROOT", "埼玉縣", "〒330", "prefecture", "埼玉市,川越市,熊谷市,川口市,行田市,秩父市,所澤市,飯能市,加須市,本庄市,春日部市,狹山市,越谷市"),
        ("JP", "L1", "ROOT", "宮城縣", "〒980", "prefecture", "仙台市,石卷市,鹽竈市,氣仙沼市,白石市,名取市,角田市,多賀城市"),
        ("JP", "L1", "ROOT", "廣島縣", "〒730", "prefecture", "廣島市,吳市,竹原市,三原市,尾道市,福山市,府中市,三次市,廿日市市,東廣島市"),
        ("JP", "L1", "ROOT", "奈良縣", "〒630", "prefecture", "奈良市,大和高田市,大和郡山市,天理市,橿原市,櫻井市,五條市,御所市,生駒市"),
        ("JP", "L1", "ROOT", "青森縣", "〒030", "prefecture", "青森市,弘前市,八戶市,黑石市,五所川原市,十和田市,三澤市,陸奧市,津輕市,平川市,中津輕郡"),
        ("JP", "L1", "ROOT", "靜岡縣", "〒420", "prefecture", "靜岡市,濱松市,沼津市,熱海市,三島市,富士宮市,伊東市,島田市,富士市"),
    ]
    rows.extend(jp_l1)

    # 東京都、京都市、大阪市、札幌市、橫濱市、福岡市、那霸市下轄區與町丁
    jp_l2 = [
        ("JP", "L2", "青森縣", "中津輕郡", "〒036", "district", "西目屋村,大字田代,田代,神田"),
        ("JP", "L3", "中津輕郡", "西目屋村", "〒036", "village", "田代,大字田代,神田"),
        ("JP", "L2", "東京都", "千代田區", "〒100", "ward", "丸之內,大手町,有樂町,霞關,永田町,秋葉原,神田,九段北,九段南,飯田橋,麴町"),
        ("JP", "L2", "東京都", "中央區", "〒103", "ward", "日本橋,八重洲,京橋,銀座,築地,月島,人形町,茅場町,晴海"),
        ("JP", "L2", "東京都", "港區", "〒105", "ward", "新橋,汐留,虎之門,六本木,赤坂,青山,麻布十番,芝公園,白金台,台場,品川"),
        ("JP", "L2", "東京都", "新宿區", "〒160", "ward", "歌舞伎町,西新宿,新宿,四谷,神樂坂,高田馬場,早稻田,大久保,百人町"),
        ("JP", "L2", "東京都", "文京區", "〒112", "ward", "本鄉,後樂,小石川,湯島,根津,千駄木,茗荷谷,音羽"),
        ("JP", "L2", "東京都", "台東區", "〒110", "ward", "上野,淺草,秋葉原,淺草橋,藏前,谷中,雷門,御徒町,入谷"),
        ("JP", "L2", "東京都", "臺東區", "〒110", "ward", "上野,淺草,秋葉原,淺草橋,藏前,谷中,雷門,御徒町,入谷"),
        ("JP", "L2", "東京都", "墨田區", "〒130", "ward", "押上,錦糸町,兩國,向島,吾妻橋"),
        ("JP", "L2", "東京都", "江東區", "〒135", "ward", "豐洲,有明,青海,門前仲町,清澄白河,龜戶"),
        ("JP", "L2", "東京都", "品川區", "〒140", "ward", "大崎,五反田,目黑,天王洲,大井町,戶越"),
        ("JP", "L2", "東京都", "目黑區", "〒152", "ward", "中目黑,自由之丘,代官山,學藝大學,駒場"),
        ("JP", "L2", "東京都", "大田區", "〒144", "ward", "蒲田,羽田,田園調布,大森,平和島"),
        ("JP", "L2", "東京都", "世田谷區", "〒154", "ward", "下北澤,三軒茶屋,二子玉川,成城,經堂,用賀,駒澤"),
        ("JP", "L2", "東京都", "澀谷區", "〒150", "ward", "道玄坂,宇田川町,原宿,表參道,惠比壽,代官山,代代木,神宮前,幡谷"),
        ("JP", "L2", "東京都", "豐島區", "〒170", "ward", "池袋,東池袋,西池袋,巢鴨,大塚,目白,駒込"),
        ("JP", "L2", "東京都", "武藏野市", "〒180", "city", "吉祥寺,境南町,中町,御殿山"),
        ("JP", "L2", "京都府", "京都市", "〒600", "city", "北區,上京區,左京區,中京區,東山區,下京區,南區,右京區,伏見區,山科區,西京區"),
        ("JP", "L3", "京都市", "東山區", "〒605", "ward", "祇園,祇園町,清水,三條,四條,五條,東山"),
        ("JP", "L3", "京都市", "左京區", "〒606", "ward", "銀閣寺,吉田,下鴨,岡崎,鞍馬,貴船,百萬遍"),
        ("JP", "L3", "京都市", "右京區", "〒616", "ward", "嵐山,嵯峨野,太秦,西院,花園"),
        ("JP", "L3", "京都市", "伏見區", "〒612", "ward", "深草,桃山,伏見稻荷,醍醐"),
        ("JP", "L2", "大阪府", "大阪市", "〒530", "city", "北區,都島區,福島區,此花區,中央區,西區,港區,大正區,天王寺區,浪速區,西淀川區,淀川區,東成區,生野區,旭區,城東區,鶴見區,阿倍野區,住之江區,住吉區,東住吉區,平野區,西成區"),
        ("JP", "L3", "大阪市", "中央區", "〒540", "ward", "心齋橋,道頓堀,難波,日本橋,本町,北濱,淀屋橋,大阪城"),
        ("JP", "L3", "大阪市", "北區", "〒530", "ward", "梅田,中之島,天神橋,堂島,茶屋町"),
        ("JP", "L2", "北海道", "札幌市", "〒060", "city", "中央區,北區,東區,白石區,豐平區,南區,西區,厚別區,手稻區,清田區"),
        ("JP", "L3", "札幌市", "中央區", "〒060", "ward", "大通,薄野,圓山,狸小路,宮之森"),
        ("JP", "L2", "神奈川縣", "橫濱市", "〒220", "city", "鶴見區,神奈川區,西區,中區,南區,保土谷區,磯子區,金澤區,港北區,戶塚區,港南區,旭區,綠區,瀨谷區,榮區,泉區,青葉區,都筑區,港未來,中華街,元町"),
        ("JP", "L2", "福岡縣", "福岡市", "〒810", "city", "東區,博多區,中央區,南區,西區,城南區,早良區,天神,中洲"),
        ("JP", "L3", "福岡市", "博多區", "〒812", "ward", "博多站前,天神,中洲,住吉,祇園町,冷泉町"),
        ("JP", "L2", "沖繩縣", "那霸市", "〒900", "city", "國際通,牧志,久茂地,首里,新都心,小祿,若狹"),
    ]
    rows.extend(jp_l2)

    return rows


def build_korea_hierarchy() -> List[GeoRow]:
    rows: List[GeoRow] = []

    # 韓國 17 廣域市道 -> 市郡區
    kr_l1 = [
        (
            "KR",
            "L1",
            "ROOT",
            "首爾特別市",
            "04524",
            "metropolitan",
            "鍾路區,中區,龍山區,城東區,廣津區,東大門區,中浪區,城北區,江北區,道峰區,蘆原區,恩平區,西大門區,麻浦區,陽川區,江西區,九老區,衿川區,永登浦區,銅雀區,冠岳區,瑞草區,江南區,松坡區,江東區",
        ),
        (
            "KR",
            "L1",
            "ROOT",
            "首爾",
            "04524",
            "metropolitan",
            "鍾路區,中區,龍山區,城東區,廣津區,東大門區,中浪區,城北區,江北區,道峰區,蘆原區,恩平區,西大門區,麻浦區,陽川區,江西區,九老區,衿川區,永登浦區,銅雀區,冠岳區,瑞草區,江南區,松坡區,江東區",
        ),
        (
            "KR",
            "L1",
            "ROOT",
            "釜山廣域市",
            "47545",
            "metropolitan",
            "中區,西區,東區,影島區,釜山鎮區,東萊區,南區,北區,海雲臺區,海雲台區,沙下區,金井區,江西區,蓮堤區,水營區,沙上區,機張郡",
        ),
        ("KR", "L1", "ROOT", "大邱廣域市", "41911", "metropolitan", "中區,東區,西區,南區,北區,壽城區,達西區,達城郡,軍威郡"),
        ("KR", "L1", "ROOT", "仁川廣域市", "21554", "metropolitan", "中區,東區,彌鄒忽區,延壽區,南洞區,富平區,桂陽區,西區,江華郡,甕津郡"),
        ("KR", "L1", "ROOT", "光州廣域市", "61945", "metropolitan", "東區,西區,南區,北區,光山區"),
        ("KR", "L1", "ROOT", "大田廣域市", "35242", "metropolitan", "東區,中區,西區,儒城區,大德區"),
        ("KR", "L1", "ROOT", "蔚山廣域市", "44675", "metropolitan", "中區,南區,東區,北區,蔚州郡"),
        ("KR", "L1", "ROOT", "世宗特別自治市", "30151", "metropolitan", "鳥致院邑,多情洞,어진洞,博覽洞"),
        (
            "KR",
            "L1",
            "ROOT",
            "京畿道",
            "16508",
            "province",
            "水原市,城南市,議政府市,安養市,富川市,光明市,平澤市,東豆川市,安山市,高陽市,果川市,九里市,南楊州市,烏山市,始興市,軍浦市,義王市,河南市,龍仁市,坡州市,利川市,安城市,金浦市,華城市,廣州市,楊州市,抱川市,驪州市",
        ),
        ("KR", "L1", "ROOT", "江原特別自治道", "24266", "province", "春川市,原州市,江陵市,東海市,太白市,束草市,三陟市,平昌郡"),
        ("KR", "L1", "ROOT", "江原道", "24266", "province", "春川市,原州市,江陵市,東海市,太白市,束草市,三陟市,平昌郡"),
        ("KR", "L1", "ROOT", "忠清北道", "28515", "province", "清州市,忠州市,堤川市"),
        ("KR", "L1", "ROOT", "忠清南道", "32255", "province", "天安市,公州市,保寧市,牙山市,瑞山市,論山市,雞龍市,唐津市"),
        ("KR", "L1", "ROOT", "全北特別自治道", "54968", "province", "全州市,群山市,益山市,井邑市,南原市,金堤市"),
        ("KR", "L1", "ROOT", "全羅南道", "58564", "province", "木浦市,麗水市,順天市,羅州市,光陽市"),
        ("KR", "L1", "ROOT", "慶尚北道", "36759", "province", "浦項市,慶州市,金泉市,安東市,龜尾市,榮州市,永川市,尚州市,聞慶市,慶山市"),
        ("KR", "L1", "ROOT", "慶尚南道", "51154", "province", "昌原市,晉州市,統營市,泗川市,金海市,密陽市,巨濟市,梁山市"),
        ("KR", "L1", "ROOT", "濟州特別自治道", "63122", "province", "濟州市,西歸浦市"),
        ("KR", "L1", "ROOT", "濟州道", "63122", "province", "濟州市,西歸浦市"),
    ]
    rows.extend(kr_l1)

    # 首爾、釜山、仁川、京畿道、濟州下轄區與法政洞／路
    kr_l2 = [
        (
            "KR",
            "L2",
            "首爾特別市",
            "江南區",
            "06000",
            "district",
            "驛三洞,論峴洞,狎鷗亭洞,清潭洞,三成洞,大峙洞,新沙洞,道谷洞,開浦洞,水西洞,德黑蘭路,江南大路,島山大路,奉恩寺路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "瑞草區",
            "06500",
            "district",
            "瑞草洞,盤浦洞,方背洞,良才洞,蠶院洞,內谷洞,江南大路,盤浦大路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "松坡區",
            "05500",
            "district",
            "蠶室洞,文井洞,可樂洞,芳荑洞,風納洞,石村洞,松坡洞,奧林匹克路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "鍾路區",
            "03000",
            "district",
            "光化門,三清洞,仁寺洞,惠化洞,北村,西村,平倉洞,世宗大路,鍾路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "中區",
            "04500",
            "district",
            "明洞,乙支路,忠武路,南大門,東大門,小公洞,會賢洞,獎忠洞,新堂洞",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "麻浦區",
            "03900",
            "district",
            "弘大,合井洞,延南洞,西橋洞,上岩洞,望遠洞,孔德洞,麻浦大路,楊花路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "龍山區",
            "04300",
            "district",
            "梨泰院洞,漢南洞,二村洞,厚岩洞,青坡洞,龍山洞,漢江大路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "永登浦區",
            "07200",
            "district",
            "汝矣島洞,永登浦洞,堂山洞,文來洞,楊平洞,大林洞,新吉洞,國際金融路",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "城東區",
            "04700",
            "district",
            "聖水洞,往十里,玉水洞,金湖洞,杏堂洞,馬場洞",
        ),
        (
            "KR",
            "L2",
            "首爾特別市",
            "西大門區",
            "03600",
            "district",
            "新村洞,延禧洞,大賢洞,忠正路,北阿峴洞,弘濟洞",
        ),
        ("KR", "L2", "釜山廣域市", "海雲臺區", "48000", "district", "佑洞,中洞,佐洞,松亭洞,栽松洞,盤여洞,海雲臺路,Centum"),
        ("KR", "L2", "釜山廣域市", "海雲台區", "48000", "district", "佑洞,中洞,佐洞,松亭洞,栽松洞,海雲台路"),
        ("KR", "L2", "釜山廣域市", "釜山鎮區", "47200", "district", "西面,釜田洞,田浦洞,凡川洞,伽倻洞"),
        ("KR", "L2", "仁川廣域市", "延壽區", "21900", "district", "松島洞,延壽洞,東春洞,玉蓮洞,青鶴洞"),
        ("KR", "L2", "京畿道", "城南市", "13500", "city", "盆唐區,壽井區,中院區,板橋洞,亭子洞,書峴洞,野塔洞"),
        ("KR", "L3", "城南市", "盆唐區", "13500", "district", "板橋洞,三坪洞,亭子洞,書峴洞,野塔洞,藪內洞"),
        ("KR", "L2", "京畿道", "水原市", "16200", "city", "長安區,勸善區,八達區,靈通區,光教洞,梅灘洞,行宮洞,水原華城"),
        ("KR", "L3", "水原市", "長安區", "16200", "district", "水原華城,迎華洞,鍊武洞,松竹洞,亭子洞"),
        ("KR", "L3", "水原市", "八達區", "16200", "district", "水原華城,行宮洞,八達路,梅山路,仁溪洞"),
        ("KR", "L2", "濟州特別自治道", "濟州市", "63000", "city", "連洞,老衡洞,二徒洞,涯月邑,朝天邑,舊左邑,牛島面"),
        ("KR", "L2", "濟州特別自治道", "西歸浦市", "63500", "city", "中文洞,西歸洞,城山邑,安德面,大靜邑,表善面,南元邑"),
    ]
    rows.extend(kr_l2)

    return rows


def build_cn_hk_mo_hierarchy() -> List[GeoRow]:
    rows: List[GeoRow] = []

    # 香港、澳門、中國大陸省級與地級／縣區級
    cn_l1 = [
        (
            "HK",
            "L1",
            "ROOT",
            "香港特別行政區",
            "999077",
            "sar",
            "中西區,灣仔區,東區,南區,油尖旺區,深水埗區,九龍城區,黃大仙區,觀塘區,葵青區,荃灣區,屯門區,元朗區,北區,大埔區,沙田區,西貢區,離島區",
        ),
        (
            "HK",
            "L1",
            "ROOT",
            "香港",
            "999077",
            "sar",
            "中西區,灣仔區,東區,南區,油尖旺區,深水埗區,九龍城區,黃大仙區,觀塘區,葵青區,荃灣區,屯門區,元朗區,北區,大埔區,沙田區,西貢區,離島區",
        ),
        (
            "HK",
            "L1",
            "ROOT",
            "九龍",
            "999077",
            "region",
            "油尖旺區,深水埗區,九龍城區,黃大仙區,觀塘區,深水埗,九龍城,紅磡,油麻地,尖沙咀,旺角",
        ),
        (
            "MO",
            "L1",
            "ROOT",
            "澳門特別行政區",
            "999078",
            "sar",
            "花地瑪堂區,聖安多尼堂區,大堂區,望德堂區,風順堂區,嘉模堂區,路氹城,聖方濟各堂區",
        ),
        (
            "MO",
            "L1",
            "ROOT",
            "澳門",
            "999078",
            "sar",
            "花地瑪堂區,聖安多尼堂區,大堂區,望德堂區,風順堂區,嘉模堂區,路氹城,聖方濟各堂區",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "北京市",
            "100000",
            "municipality",
            "東城區,西城區,朝陽區,豐台區,石景山區,海淀區,門頭溝區,房山區,通州區,順義區,昌平區,大興區,懷柔區,平谷區,密雲區,延慶區",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "上海市",
            "200000",
            "municipality",
            "黃浦區,徐匯區,長寧區,靜安區,普陀區,虹口區,楊浦區,閔行區,寶山區,嘉定區,浦東新區,金山區,松江區,青浦區,奉賢區,崇明區,黃浦江,外灘,陸家嘴",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "廣東省",
            "510000",
            "province",
            "廣州市,韶關市,深圳市,珠海市,汕頭市,佛山市,江門市,湛江市,茂名市,肇慶市,惠州市,梅州市,汕尾市,河源市,陽江市,清遠市,東莞市,中山市,潮州市,揭陽市,雲浮市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "江蘇省",
            "210000",
            "province",
            "南京市,無錫市,徐州市,常州市,蘇州市,南通市,連雲港市,淮安市,鹽城市,揚州市,鎮江市,泰州市,宿遷市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "浙江省",
            "310000",
            "province",
            "杭州市,寧波市,溫州市,嘉興市,湖州市,紹興市,金華市,衢州市,舟山市,台州市,麗水市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "福建省",
            "350000",
            "province",
            "福州市,廈門市,莆田市,三明市,泉州市,漳州市,南平市,龍巖市,寧德市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "四川省",
            "610000",
            "province",
            "成都市,自貢市,攀枝花市,瀘州市,德陽市,綿陽市,廣元市,遂寧市,內江市,樂山市,南充市,眉山市,宜賓市,廣安市,達州市,雅安市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "湖北省",
            "430000",
            "province",
            "武漢市,黃石市,十堰市,宜昌市,襄陽市,鄂州市,荊門市,孝感市,荊州市,黃岡市,咸寧市,隨州市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "山東省",
            "250000",
            "province",
            "濟南市,青島市,淄博市,棗莊市,東營市,煙台市,濰坊市,濟寧市,泰安市,威海市,日照市,臨沂市",
        ),
        (
            "CN",
            "L1",
            "ROOT",
            "陝西省",
            "710000",
            "province",
            "西安市,銅川市,寶雞市,咸陽市,渭南市,延安市,漢中市,榆林市,安康市,商洛市",
        ),
    ]
    rows.extend(cn_l1)

    cn_l2 = [
        ("HK", "L2", "香港特別行政區", "中西區", "999077", "district", "中環,上環,西環,金鐘,半山區,半山,半山手扶梯,山頂,堅尼地城,西營盤,德輔道中,皇后大道中"),
        ("HK", "L2", "香港特別行政區", "灣仔區", "999077", "district", "灣仔,銅鑼灣,跑馬地,大坑,天后,渣甸山,軒尼詩道,告士打道"),
        ("HK", "L2", "香港特別行政區", "東區", "999077", "district", "北角,鰂魚涌,太古城,西灣河,筲箕灣,柴灣,小西灣,英皇道"),
        ("HK", "L2", "香港特別行政區", "油尖旺區", "999077", "district", "尖沙咀,油麻地,旺角,佐敦,太子,大角咀,西九龍,彌敦道,廣東道"),
        ("HK", "L2", "香港特別行政區", "深水埗區", "999077", "district", "深水埗,欽州街,長沙灣,荔枝角,美孚,石硤尾,又一村,南昌"),
        ("HK", "L2", "香港", "深水埗區", "999077", "district", "深水埗,欽州街,長沙灣,荔枝角,美孚,石硤尾,又一村,南昌"),
        ("HK", "L2", "九龍", "深水埗區", "999077", "district", "深水埗,欽州街,長沙灣,荔枝角,美孚,石硤尾,又一村,南昌"),
        ("HK", "L2", "香港特別行政區", "九龍城區", "999077", "district", "九龍城,紅磡,黃埔,黃埔花園,土瓜灣,何文田,啟德,馬頭圍"),
        ("HK", "L2", "香港", "九龍城區", "999077", "district", "九龍城,紅磡,黃埔,黃埔花園,土瓜灣,何文田,啟德,馬頭圍"),
        ("HK", "L2", "九龍", "九龍城區", "999077", "district", "九龍城,紅磡,黃埔,黃埔花園,土瓜灣,何文田,啟德,馬頭圍"),
        ("HK", "L2", "香港特別行政區", "沙田區", "999077", "district", "沙田,大圍,火炭,馬料水,馬鞍山,小瀝源"),
        ("HK", "L2", "香港特別行政區", "離島區", "999077", "district", "東涌,赤鱲角,大嶼山,長洲,南丫島,坪洲,愉景灣"),
        ("MO", "L2", "澳門特別行政區", "花地瑪堂區", "999078", "parish", "青洲,台山,黑沙環,祐漢,慕拉士,關閘"),
        ("MO", "L2", "澳門特別行政區", "大堂區", "999078", "parish", "新馬路,南灣,新口岸,皇朝,外港"),
        ("MO", "L2", "澳門特別行政區", "嘉模堂區", "999078", "parish", "氹仔,官也街,龍環葡韻,海洋花園"),
        ("CN", "L2", "廣東省", "廣州市", "510000", "city", "越秀區,海珠區,荔灣區,天河區,白雲區,黃埔區,番禺區,花都區,南沙區,從化區,增城區"),
        ("CN", "L2", "廣東省", "深圳市", "518000", "city", "羅湖區,福田區,南山區,寶安區,龍崗區,鹽田區,龍華區,坪山區,光明區,大鵬新區"),
        ("CN", "L3", "深圳市", "南山區", "518052", "district", "粵海街道,南頭街道,南山街道,沙河街道,蛇口街道,招商街道,西麗街道,桃源街道,深南大道,科技園"),
        ("CN", "L3", "深圳市", "福田區", "518000", "district", "華強北街道,福田街道,南園街道,園嶺街道,沙頭街道,香蜜湖街道,蓮花街道"),
        ("CN", "L2", "江蘇省", "蘇州市", "215000", "city", "姑蘇區,虎丘區,吳中區,相城區,吳江區,常熟市,張家港市,崑山市,太倉市,蘇州工業園區"),
        ("CN", "L3", "蘇州市", "姑蘇區", "215000", "district", "平江街道,金閶街道,滄浪街道,雙塔街道,虎丘街道,蘇錦街道,吳門橋街道,白洋灣街道,觀前街,山塘街"),
        ("CN", "L2", "江蘇省", "南京市", "210000", "city", "玄武區,秦淮區,建鄴區,鼓樓區,浦口區,棲霞區,雨花台區,江寧區,六合區,溧水區,高淳區"),
        ("CN", "L2", "浙江省", "杭州市", "310000", "city", "上城區,拱墅區,西湖區,濱江區,蕭山區,餘杭區,富陽區,臨安區,臨平區,錢塘區"),
        ("CN", "L3", "杭州市", "西湖區", "310012", "district", "北山街道,西溪街道,翠苑街道,古蕩街道,靈隱街道,文新街道,三墩鎮,轉塘街道"),
        ("CN", "L2", "福建省", "廈門市", "361000", "city", "思明區,海滄區,湖里區,集美區,同安區,翔安區,鼓浪嶼"),
        ("CN", "L2", "四川省", "成都市", "610000", "city", "錦江區,青羊區,金牛區,武侯區,成華區,龍泉驛區,青白江區,新都區,溫江區,雙流區,郫都區"),
        ("CN", "L3", "成都市", "武侯區", "610041", "district", "漿洗街街道,望江路街道,玉林街道,火車南站街道,簇錦街道,華興街道,桂溪街道"),
    ]
    rows.extend(cn_l2)

    return rows






def generate_tsv(output_path: str) -> int:
    raw_rows: List[GeoRow] = []
    raw_rows.extend(build_taiwan_hierarchy())

    # Helper to upgrade 7-tuple rows to 8-tuple GeoRow
    l1_map = {}
    for r in build_japan_hierarchy() + build_korea_hierarchy() + build_cn_hk_mo_hierarchy():
        if r[1] == "L1":
            l1_map[r[3]] = r[3]
        elif r[1] == "L2":
            l1_map[r[3]] = r[2]

    for r in build_japan_hierarchy() + build_korea_hierarchy() + build_cn_hk_mo_hierarchy():
        if r[1] == "L1":
            l1 = r[3]
        elif r[1] == "L2":
            l1 = r[2]
        else:
            l1 = l1_map.get(r[2], r[2])
        raw_rows.append((r[0], r[1], r[2], r[3], r[4], r[5], r[6], l1))

    rows: List[GeoRow] = []
    seen_keys = set()
    for r in raw_rows:
        key = (r[0], r[1], r[2], r[3], r[5], r[7])
        if key not in seen_keys:
            seen_keys.add(key)
            rows.append(r)

    os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)
    with open(output_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("# country\tlevel\tparent\tentity\tpostal_code\tcategory\tchildren_csv\tl1_root\n")
        for row in rows:
            f.write("\t".join(row) + "\n")

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    data_dir_file = os.path.join(repo_root, "data", "cjk_geo_hierarchy.tsv")
    if os.path.exists(os.path.join(repo_root, "data")):
        with open(data_dir_file, "w", encoding="utf-8", newline="\n") as f:
            f.write("# country\tlevel\tparent\tentity\tpostal_code\tcategory\tchildren_csv\tl1_root\n")
            for row in rows:
                f.write("\t".join(row) + "\n")

    return len(rows)


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(
        description="建構漢字圈（台／日／韓／陸港澳）條件式階層郵政、地政與戶政知識樹 TSV 檔案。"
    )
    default_out = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "ruhi_lexicon",
        "data",
        "cjk_geo_hierarchy.tsv",
    )
    parser.add_argument(
        "-o",
        "--output",
        default=default_out,
        help="輸出之 TSV 檔案路徑（預設為 ruhi_lexicon/data/cjk_geo_hierarchy.tsv）",
    )
    args = parser.parse_args()

    count = generate_tsv(args.output)
    total_children = 0
    with open(args.output, "r", encoding="utf-8") as f:
        for line in f:
            if not line.startswith("#") and line.strip():
                parts = line.rstrip("\n").split("\t")
                if len(parts) >= 7:
                    total_children += len([c for c in parts[6].split(",") if c.strip()])

    print(
        f"[CJK-Geo-Hierarchy] 建構完成：{args.output} "
        f"(節點群數={count}, 條件喚醒子地名／路街／鄰里／地段／町洞總數={total_children})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
