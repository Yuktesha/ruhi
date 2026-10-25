# -*- coding: utf-8 -*-
"""
產生「如喜（RuhI）— 智慧型四角號碼 4×5 小鍵盤輸入法」分鏡教學動畫 (GIF)
"""
import os
from PIL import Image, ImageDraw, ImageFont

OUT_ARTIFACT_GIF = r"C:\Users\Yuktesha\.gemini\antigravity\brain\d0ae9437-e0ca-4efc-872e-e3b5973b294f\four_corner_tutorial.gif"
OUT_DOCS_GIF = r"C:\ATGprjs\ruhi\docs\four_corner_tutorial.gif"

W, H = 960, 560

# 色彩配置 (深色沉浸主題)
BG_MAIN = (18, 22, 31)
BG_CARD = (28, 34, 48)
BG_KEY = (40, 48, 66)
BG_KEY_ACTIVE = (14, 165, 233)
BG_KEY_SPECIAL = (52, 62, 84)
BG_CAND = (24, 30, 42)
BORDER = (62, 74, 98)
TEXT_BRIGHT = (241, 245, 249)
TEXT_MAIN = (203, 213, 225)
TEXT_MUTED = (148, 163, 184)
ACCENT_CYAN = (56, 189, 248)
ACCENT_GOLD = (250, 204, 21)
ACCENT_GREEN = (52, 211, 153)
ACCENT_PINK = (244, 114, 182)
ACCENT_PURPLE = (167, 139, 250)

CORNER_COLORS = [
    (56, 189, 248),   # 左上 (1) - Cyan
    (250, 204, 21),   # 右上 (2) - Gold
    (52, 211, 153),   # 左下 (3) - Green
    (244, 114, 182),  # 右下 (4) - Pink
]


def load_font(size: int, bold: bool = False):
    font_paths = [
        r"C:\Windows\Fonts\msjhbd.ttc" if bold else r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\mingliu.ttc",
    ]
    for fp in font_paths:
        if os.path.exists(fp):
            return ImageFont.truetype(fp, size)
    return ImageFont.load_default()


FONT_H1 = load_font(26, bold=True)
FONT_H2 = load_font(20, bold=True)
FONT_BODY = load_font(16, bold=False)
FONT_BOLD = load_font(16, bold=True)
FONT_SMALL = load_font(13, bold=False)
FONT_SMALL_B = load_font(13, bold=True)
FONT_CHAR_HUGE = load_font(150, bold=True)
FONT_CHAR_MID = load_font(96, bold=True)
FONT_KEY = load_font(20, bold=True)
FONT_KEY_SUB = load_font(11, bold=False)


def draw_rounded_rect(draw, box, radius, fill, outline=None, width=1):
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def draw_header(draw, scene_idx, total_scenes, title, subtitle):
    # Top bar
    draw_rounded_rect(draw, (20, 16, W - 20, 82), 12, BG_CARD, BORDER, 1)
    badge_text = f"教學單元 {scene_idx}/{total_scenes}"
    draw_rounded_rect(draw, (34, 28, 155, 68), 8, (14, 116, 144))
    draw.text((46, 36), badge_text, font=FONT_BOLD, fill=TEXT_BRIGHT)
    draw.text((172, 24), title, font=FONT_H1, fill=TEXT_BRIGHT)
    draw.text((172, 54), subtitle, font=FONT_SMALL, fill=ACCENT_CYAN)


def draw_numpad(draw, x0, y0, active_key=None, trail_keys=None):
    """繪製 4x5 實體數字小鍵盤與即時高亮動畫"""
    if trail_keys is None:
        trail_keys = []
    draw_rounded_rect(draw, (x0, y0, x0 + 310, y0 + 365), 14, BG_CARD, BORDER, 1)
    draw.text((x0 + 18, y0 + 12), "4×5 實體小鍵盤 (單手盲打區)", font=FONT_BOLD, fill=ACCENT_GOLD)

    kw, kh = 64, 54
    gap = 8
    sx, sy = x0 + 18, y0 + 44

    # Keys definition: (label, sublabel, col, row, colspan, rowspan, is_special)
    keys = [
        ("Num", "鎖定", 0, 0, 1, 1, True),
        ("/", "符號/？", 1, 0, 1, 1, True),
        ("*", "萬用/、", 2, 0, 1, 1, True),
        ("-", "上選/—", 3, 0, 1, 1, True),

        ("7", "角 ┏━", 0, 1, 1, 1, False),
        ("8", "八 八", 1, 1, 1, 1, False),
        ("9", "小 小", 2, 1, 1, 1, False),
        ("+", "下選/，", 3, 1, 1, 2, True),

        ("4", "叉 十", 0, 2, 1, 1, False),
        ("5", "插 丰", 1, 2, 1, 1, False),
        ("6", "方 口", 2, 2, 1, 1, False),

        ("1", "橫 一", 0, 3, 1, 1, False),
        ("2", "垂 ｜", 1, 3, 1, 1, False),
        ("3", "點 丶", 2, 3, 1, 1, False),
        ("Enter", "上字", 3, 3, 1, 2, True),

        ("0", "頭 亠", 0, 4, 2, 1, False),
        (".", "附角/。", 2, 4, 1, 1, True),
    ]

    for label, sub, c, r, cs, rs, is_spec in keys:
        kx1 = sx + c * (kw + gap)
        ky1 = sy + r * (kh + gap)
        kx2 = kx1 + cs * kw + (cs - 1) * gap
        ky2 = ky1 + rs * kh + (rs - 1) * gap

        if label == active_key:
            fill = BG_KEY_ACTIVE
            outline = ACCENT_GOLD
            bw = 3
            fg = (15, 23, 42)
            sub_fg = (15, 23, 42)
        elif label in trail_keys:
            fill = (30, 88, 128)
            outline = ACCENT_CYAN
            bw = 2
            fg = TEXT_BRIGHT
            sub_fg = ACCENT_CYAN
        else:
            fill = BG_KEY_SPECIAL if is_spec else BG_KEY
            outline = BORDER
            bw = 1
            fg = TEXT_BRIGHT
            sub_fg = TEXT_MUTED

        draw_rounded_rect(draw, (kx1, ky1, kx2, ky2), 8, fill, outline, bw)
        draw.text((kx1 + 10, ky1 + 6), label, font=FONT_KEY if len(label) <= 2 else FONT_BOLD, fill=fg)
        draw.text((kx1 + 10, ky1 + 32), sub, font=FONT_KEY_SUB, fill=sub_fg)


def draw_candidate_bar(draw, x0, y0, w, code_str, mode_label, candidates, selected_idx=0, committed_text=""):
    """繪製模擬如喜 TSF 懸浮組字與候選視窗"""
    draw_rounded_rect(draw, (x0, y0, x0 + w, y0 + 102), 12, BG_CAND, ACCENT_CYAN, 2)
    # 狀態徽章
    draw_rounded_rect(draw, (x0 + 14, y0 + 12, x0 + 46, y0 + 42), 6, ACCENT_CYAN)
    draw.text((x0 + 21, y0 + 15), "四", font=FONT_H2, fill=(15, 23, 42))

    # 組碼區
    code_disp = code_str if code_str else "(等待輸入...)"
    draw.text((x0 + 58, y0 + 14), f"四角碼：{code_disp}", font=FONT_H2, fill=ACCENT_GOLD if code_str else TEXT_MUTED)
    draw.text((x0 + 280, y0 + 17), mode_label, font=FONT_SMALL_B, fill=ACCENT_GREEN)

    if committed_text:
        draw_rounded_rect(draw, (x0 + w - 210, y0 + 10, x0 + w - 14, y0 + 44), 6, (20, 83, 45), ACCENT_GREEN, 1)
        draw.text((x0 + w - 198, y0 + 16), f"已上字：{committed_text}", font=FONT_BOLD, fill=TEXT_BRIGHT)

    # 候選詞列
    cx = x0 + 16
    cy = y0 + 54
    for idx, (word, tag) in enumerate(candidates):
        is_sel = (idx == selected_idx)
        item_txt = f"{idx + 1}. {word}"
        tw = len(word) * 22 + 54 + (len(tag) * 11 if tag else 0)
        if is_sel:
            draw_rounded_rect(draw, (cx, cy, cx + tw, cy + 36), 6, (14, 116, 144), ACCENT_GOLD, 2)
            draw.text((cx + 10, cy + 6), item_txt, font=FONT_BOLD, fill=TEXT_BRIGHT)
            if tag:
                draw.text((cx + 14 + len(item_txt) * 11, cy + 9), f"[{tag}]", font=FONT_SMALL, fill=ACCENT_GOLD)
        else:
            draw_rounded_rect(draw, (cx, cy, cx + tw, cy + 36), 6, BG_KEY, BORDER, 1)
            draw.text((cx + 10, cy + 6), item_txt, font=FONT_BODY, fill=TEXT_MAIN)
            if tag:
                draw.text((cx + 14 + len(item_txt) * 11, cy + 9), f"[{tag}]", font=FONT_SMALL, fill=TEXT_MUTED)
        cx += tw + 10


def draw_char_decomposition_box(draw, x0, y0, char_str, corners_info, step_progress):
    """
    繪製漢字四角拆解視覺化框：
    corners_info: list of 4 tuples: (corner_idx, digit, stroke_desc, highlight_box_rel)
    step_progress: 0..4 (亮起第幾個角)
    """
    bw, bh = 250, 250
    draw_rounded_rect(draw, (x0, y0, x0 + bw, y0 + bh), 14, (22, 28, 40), BORDER, 2)
    # 米字格輔助線
    mx, my = x0 + bw // 2, y0 + bh // 2
    draw.line([(x0 + 15, my), (x0 + bw - 15, my)], fill=(45, 55, 72), width=1)
    draw.line([(mx, y0 + 15), (mx, y0 + bh - 15)], fill=(45, 55, 72), width=1)

    # 繪製中央大字
    draw.text((x0 + 50, y0 + 32), char_str, font=FONT_CHAR_HUGE, fill=TEXT_BRIGHT)

    # 四角座標位置 (左上, 右上, 左下, 右下)
    corner_Badges = [
        (x0 + 12, y0 + 12, "①左上"),
        (x0 + bw - 82, y0 + 12, "②右上"),
        (x0 + 12, y0 + bh - 44, "③左下"),
        (x0 + bw - 82, y0 + bh - 44, "④右下"),
    ]

    # Z 字形箭頭軌跡
    z_centers = [
        (x0 + 48, y0 + 48),
        (x0 + bw - 48, y0 + 48),
        (x0 + 48, y0 + bh - 48),
        (x0 + bw - 48, y0 + bh - 48),
    ]
    for i in range(min(step_progress - 1, 3)):
        if i >= 0:
            p1 = z_centers[i]
            p2 = z_centers[i + 1]
            draw.line([p1, p2], fill=CORNER_COLORS[i + 1], width=3)

    for idx in range(4):
        bx, by, label = corner_Badges[idx]
        digit, desc, hbox = corners_info[idx]
        col = CORNER_COLORS[idx]
        is_active = (idx < step_progress)
        is_current = (idx == step_progress - 1)

        if is_active:
            # 畫出該角的筆形高亮框
            rx1, ry1, rx2, ry2 = hbox
            draw_rounded_rect(
                draw,
                (x0 + rx1, y0 + ry1, x0 + rx2, y0 + ry2),
                8,
                fill=None,
                outline=col,
                width=4 if is_current else 2,
            )
            # 角標徽章
            draw_rounded_rect(draw, (bx, by, bx + 70, by + 32), 6, col, TEXT_BRIGHT if is_current else None, 2 if is_current else 0)
            draw.text((bx + 6, by + 4), f"{label}:{digit}", font=FONT_SMALL_B, fill=(15, 23, 42))
        else:
            draw_rounded_rect(draw, (bx, by, bx + 70, by + 32), 6, (35, 43, 58), BORDER, 1)
            draw.text((bx + 6, by + 4), label, font=FONT_SMALL, fill=TEXT_MUTED)


def build_frames():
    frames = []
    durations = []

    def add_frame(img, ms):
        frames.append(img)
        durations.append(ms)

    total_scenes = 4

    # =========================================================================
    # 單元 1：四角取碼順序（Z 字軌跡）與「台 (2360)」正統拆碼動畫
    # =========================================================================
    tai_corners = [
        ("2", "左上『厶』起筆撇折＝2(垂)", (58, 45, 130, 128)),
        ("3", "右上『厶』右端點＝3(點)", (132, 72, 198, 132)),
        ("6", "左下『口』方形＝6(方)", (66, 136, 188, 210)),
        ("0", "右下同『口』已用歸零＝0", (135, 160, 194, 214)),
    ]
    tai_steps = [
        (0, None, [], "", "準備拆解「台」字四角...", [("台", "正統2360"), ("臺", "異體共享")], ""),
        (1, "2", ["2"], "2", "① 左上角：『厶』撇折歸 2（垂）", [("仁", "21"), ("台", "2360"), ("行", "21")], ""),
        (2, "3", ["2", "3"], "23", "② 右上角：『厶』右點歸 3（點）", [("台", "2360"), ("參", "2320"), ("編", "2322")], ""),
        (3, "6", ["2", "3", "6"], "236", "③ 左下角：下方『口』歸 6（方）", [("台", "2360"), ("", "2360")], ""),
        (4, "0", ["2", "3", "6", "0"], "2360", "④ 右下角：『口』已被左下角用過，正統歸 0！", [("台", "正統碼"), ("臺", "正異共享"), ("", "罕用")], ""),
        (4, "Enter", ["2", "3", "6", "0"], "2360", "按 Enter 上字！同時支援選出「台」與「臺」", [("台", "正統碼"), ("臺", "正異共享")], "台"),
    ]

    for step_idx, active_k, trail_k, code_s, desc_s, cands, committed in tai_steps:
        img = Image.new("RGB", (W, H), BG_MAIN)
        draw = ImageDraw.Draw(img)
        draw_header(
            draw, 1, total_scenes,
            "四角取碼順序（左上① ➔ 右上② ➔ 左下③ ➔ 右下④）",
            "口訣：橫一垂二三點捺，叉四插五方塊六，七角八八九是小，點下有橫變零頭"
        )
        # 左側拆字框
        draw_char_decomposition_box(draw, 24, 100, "台", tai_corners, step_idx)

        # 中間步驟解說卡
        draw_rounded_rect(draw, (290, 100, 616, 350), 14, BG_CARD, BORDER, 1)
        draw.text((308, 114), "即時拆碼導引（Z 字形軌跡）", font=FONT_H2, fill=ACCENT_CYAN)
        for idx, (dig, text_d, _) in enumerate(tai_corners):
            y_row = 154 + idx * 45
            is_done = idx < step_idx
            is_cur = idx == step_idx - 1
            col = CORNER_COLORS[idx] if is_done else TEXT_MUTED
            bg_r = (36, 48, 68) if is_cur else (24, 30, 42)
            draw_rounded_rect(draw, (304, y_row, 602, y_row + 38), 8, bg_r, col if is_cur else None, 2 if is_cur else 0)
            draw.text((314, y_row + 8), f"第{idx+1}碼 [{dig}]：{text_d}", font=FONT_BOLD if is_cur else FONT_BODY, fill=col if is_done else TEXT_MUTED)

        # 下方候選窗
        draw_candidate_bar(draw, 24, 368, 592, code_s, desc_s, cands, 0, committed)
        # 下方歌訣速查條
        draw_rounded_rect(draw, (24, 482, 592, 542), 10, BG_CARD, BORDER, 1)
        draw.text((38, 490), "0 亠頭   1 一橫   2 ｜垂/丿   3 丶點   4 十叉   5 丰插   6 口方   7 ┏角   8 八   9 小", font=FONT_BOLD, fill=ACCENT_GOLD)
        draw.text((38, 515), "★ 傳統規則：同一筆畫或『口/門/囗』已被他角取用時，再取用改作 0", font=FONT_SMALL, fill=TEXT_MAIN)

        # 右側小鍵盤
        draw_numpad(draw, 630, 100, active_key=active_k, trail_keys=trail_k)
        add_frame(img, 1300 if step_idx in (0, 4) else 1050)

    # =========================================================================
    # 單元 2：三軌容錯解碼（初學者直覺 2366 & 正異體雙向 4010）
    # =========================================================================
    fuzzy_corners = [
        ("2", "左上『厶』撇折＝2", (58, 45, 130, 128)),
        ("3", "右上『厶』點＝3", (132, 72, 198, 132)),
        ("6", "左下『口』方塊＝6", (66, 136, 188, 210)),
        ("6", "右下直覺看也是『口』＝6 (免歸零!)", (135, 160, 194, 214)),
    ]
    scene2_steps = [
        ("2", ["2", "3", "6"], "236", "初學者痛點：忘記『口複用歸0』怎麼辦？", [("台", "2360"), ("台", "容錯2366")], ""),
        ("6", ["2", "3", "6", "6"], "2366", "軌道二【直覺寬鬆碼】：打 2366 照樣命中「台」！", [("台", "直覺寬鬆碼"), ("", "罕用")], ""),
        ("Enter", ["2", "3", "6", "6"], "2366", "免背複雜例外！打 2360 或 2366 都能出「台」", [("台", "直覺寬鬆碼")], "台"),
        ("0", ["4", "0", "1", "0"], "4010", "軌道三【正異體共享】：打『臺(4010)』也能選『台』！", [("臺", "正統4010"), ("台", "正異共享"), ("查", "4010")], "臺 / 台"),
    ]
    for active_k, trail_k, code_s, desc_s, cands, committed in scene2_steps:
        img = Image.new("RGB", (W, H), BG_MAIN)
        draw = ImageDraw.Draw(img)
        draw_header(
            draw, 2, total_scenes,
            "三軌容錯解碼：初學者免背「歸零例外」、正異體字雙向互通",
            "不論打正統碼 (2360)、直覺寬鬆碼 (2366) 或異體碼 (4010)，全部自動容錯命中！"
        )
        draw_char_decomposition_box(draw, 24, 100, "台" if code_s != "4010" else "臺", fuzzy_corners, 4)

        draw_rounded_rect(draw, (290, 100, 616, 350), 14, BG_CARD, BORDER, 1)
        draw.text((308, 114), "如喜三軌容錯架構對照", font=FONT_H2, fill=ACCENT_GOLD)

        tracks = [
            ("軌道一：Unihan 正統碼", "台 = 2360 (右下口歸 0)", code_s == "2360", ACCENT_CYAN),
            ("軌道二：初學者直覺寬鬆碼", "台 = 2366 / 如 = 4646 (右下口直取 6)", code_s in ("236", "2366"), ACCENT_GREEN),
            ("軌道三：台灣正異體雙向共享", "臺(4010) ⇄ 台(2360) 雙向互通", code_s == "4010", ACCENT_PINK),
        ]
        for idx, (t_title, t_sub, is_hi, col) in enumerate(tracks):
            yy = 154 + idx * 60
            draw_rounded_rect(draw, (304, yy, 602, yy + 52), 8, (36, 48, 68) if is_hi else (24, 30, 42), col if is_hi else BORDER, 2 if is_hi else 1)
            draw.text((316, yy + 6), t_title, font=FONT_BOLD, fill=col)
            draw.text((316, yy + 28), t_sub, font=FONT_SMALL, fill=TEXT_BRIGHT if is_hi else TEXT_MUTED)

        draw_candidate_bar(draw, 24, 368, 592, code_s, desc_s, cands, 0, committed)
        draw_rounded_rect(draw, (24, 482, 592, 542), 10, BG_CARD, BORDER, 1)
        draw.text((38, 492), "💡 更多直覺寬鬆範例：『如』正統 4640 / 直覺 4646；『因』正統 6080 / 直覺 6043", font=FONT_SMALL_B, fill=ACCENT_GREEN)
        draw.text((38, 515), "💡 正異體互通範例：裡(3621)⇄裏(0073)、線(2693)⇄綫(2395)、為(3402)⇄爲(2022)", font=FONT_SMALL, fill=TEXT_MAIN)

        draw_numpad(draw, 630, 100, active_key=active_k, trail_keys=trail_k)
        add_frame(img, 1450)

    # =========================================================================
    # 單元 3：「前兩角簡碼」雙字詞連打（2332 = 台灣、1072 = 電腦）
    # =========================================================================
    phrase_steps = [
        # (char1, char2, c1_p2, c2_p2, step_len, active_k, trail_k, code_s, desc_s, cands, committed)
        ("台", "灣", "23", "32", 1, "2", ["2"], "2", "打「台灣」：先取首字『台』左上角 [2]", [("台", "23"), ("台灣", "2332")], ""),
        ("台", "灣", "23", "32", 2, "3", ["2", "3"], "23", "取首字『台』右上角 [3] ➔ 首字前兩角 [23] 完成！", [("台", "2360"), ("台灣", "2332")], ""),
        ("台", "灣", "23", "32", 3, "3", ["2", "3", "3"], "233", "接著取次字『灣』左上角氵點 [3]（跳過『台』下半部！）", [("台灣", "前兩角2332"), ("峻", "2334")], ""),
        ("台", "灣", "23", "32", 4, "2", ["2", "3", "3", "2"], "2332", "取次字『灣』右上角彎折 [2] ➔ 2332 直出「台灣」！", [("台灣", "雙字簡碼"), ("臺灣", "異體共享")], "台灣"),
        ("電", "腦", "10", "72", 2, "0", ["1", "0"], "10", "再試「電腦」：首字『電』上雨頭前兩角 [10]", [("電", "1071"), ("電腦", "1072")], ""),
        ("電", "腦", "10", "72", 4, "2", ["1", "0", "7", "2"], "1072", "次字『腦』左上月[7]＋右上巛[2] ➔ 1072 直出「電腦」！", [("電腦", "雙字簡碼"), ("電纜", "1021")], "電腦"),
    ]

    for c1, c2, p1, p2, slen, active_k, trail_k, code_s, desc_s, cands, committed in phrase_steps:
        img = Image.new("RGB", (W, H), BG_MAIN)
        draw = ImageDraw.Draw(img)
        draw_header(
            draw, 3, total_scenes,
            "「前兩角簡碼」詞彙連打：每字只看上半部（左上①＋右上②）",
            "雙字詞 = 第一字前2碼 + 第二字前2碼（共 4 碼），速度翻倍、重碼率暴降 90%！"
        )

        # 左側雙字拆解卡
        draw_rounded_rect(draw, (24, 100, 616, 350), 14, BG_CARD, BORDER, 1)
        draw.text((42, 114), f"雙字詞前兩角公式：【{c1}】前兩角 ({p1}) ＋ 【{c2}】前兩角 ({p2}) ＝ {p1}{p2}", font=FONT_H2, fill=ACCENT_GOLD)

        # 畫出第一個字與第二個字的上半部高亮
        for idx_c, (ch, p_code, x_box) in enumerate([(c1, p1, 48), (c2, p2, 336)]):
            draw_rounded_rect(draw, (x_box, 152, x_box + 240, 332), 12, (20, 26, 38), BORDER, 1)
            # 上半部高亮區 (只取前兩角!)
            is_c1_active = (idx_c == 0 and slen >= 1) or (idx_c == 1 and slen >= 3)
            hi_col = ACCENT_CYAN if idx_c == 0 else ACCENT_GREEN
            draw_rounded_rect(draw, (x_box + 12, 160, x_box + 228, 242), 8, (28, 52, 74) if is_c1_active else (26, 32, 44), hi_col if is_c1_active else BORDER, 2)
            draw.text((x_box + 72, 168), ch, font=FONT_CHAR_MID, fill=TEXT_BRIGHT)
            # 左上、右上角標
            draw_rounded_rect(draw, (x_box + 18, 166, x_box + 74, 194), 5, CORNER_COLORS[0] if is_c1_active else BG_KEY)
            draw.text((x_box + 23, 170), f"①左上:{p_code[0]}", font=FONT_SMALL_B, fill=(15, 23, 42) if is_c1_active else TEXT_MUTED)
            draw_rounded_rect(draw, (x_box + 166, 166, x_box + 222, 194), 5, CORNER_COLORS[1] if is_c1_active else BG_KEY)
            draw.text((x_box + 171, 170), f"②右上:{p_code[1]}", font=FONT_SMALL_B, fill=(15, 23, 42) if is_c1_active else TEXT_MUTED)
            # 下半部標示「免拆！」
            draw.text((x_box + 42, 298), "下半部 (③④角) 免拆跳過！", font=FONT_SMALL_B, fill=ACCENT_PINK)

        draw_candidate_bar(draw, 24, 368, 592, code_s, desc_s, cands, 0, committed)
        draw_rounded_rect(draw, (24, 482, 592, 542), 10, BG_CARD, BORDER, 1)
        draw.text((38, 490), "🔥 常用詞前兩角範例：2332=台灣  |  4032=臺灣/台灣  |  1072=電腦  |  6261=四角號碼", font=FONT_BOLD, fill=ACCENT_CYAN)
        draw.text((38, 515), "🔥 三字詞範例：588034 (或 5834) = 輸入法（每字取前兩角，連打一氣呵成！）", font=FONT_SMALL, fill=TEXT_MAIN)

        draw_numpad(draw, 630, 100, active_key=active_k, trail_keys=trail_k)
        add_frame(img, 1300)

    # =========================================================================
    # 單元 4：4×5 小鍵盤單手盲打閉環（+ / - 選字、* 萬用字元查詢）
    # =========================================================================
    ergo_steps = [
        ("*", ["2", "3", "*", "0"], "23*0", "忘記左下角是幾號？按【*】鍵當萬用字元查詢！", [("台", "2360"), ("參", "2320"), ("牟", "2350")], 0, ""),
        ("+", ["2", "3", "*", "0"], "23*0", "按小鍵盤【+】鍵：單手往下移動選字游標 (免移手按方向鍵)", [("台", "2360"), ("參", "2320"), ("牟", "2350")], 1, ""),
        ("-", ["2", "3", "*", "0"], "23*0", "按小鍵盤【-】鍵：單手往上移回第 1 候選字「台」", [("台", "2360"), ("參", "2320"), ("牟", "2350")], 0, ""),
        (".", ["2", "3", "6", "0", "."], "2360.", "按小鍵盤【.】鍵：進入第 5 碼『附角精確篩選』模式", [("台", "2360.0"), ("邰", "2362.7")], 0, "台"),
    ]

    for active_k, trail_k, code_s, desc_s, cands, sel_i, committed in ergo_steps:
        img = Image.new("RGB", (W, H), BG_MAIN)
        draw = ImageDraw.Draw(img)
        draw_header(
            draw, 4, total_scenes,
            "4×5 小鍵盤人體工學：右手單手盲打閉環 & 萬用字元「*」",
            "右手全程不必離開小鍵盤！『+ / -』上下選字、『*』萬用查碼、『.』附角與標點"
        )

        draw_rounded_rect(draw, (24, 100, 616, 350), 14, BG_CARD, BORDER, 1)
        draw.text((42, 114), "4×5 小鍵盤特殊鍵雙模式（可於偏好設定一鍵切換）", font=FONT_H2, fill=ACCENT_CYAN)

        key_guides = [
            ("【*】乘號鍵", "組字中：萬用字元查詢（如 23*0 查『台』）｜ 無組字：輸出頓號『、』", active_k == "*", ACCENT_GOLD),
            ("【+】加號鍵", "組字中：候選詞游標往下移 / 下一頁     ｜ 無組字：輸出全形『，』", active_k == "+", ACCENT_GREEN),
            ("【-】減號鍵", "組字中：候選詞游標往上移 / 上一頁     ｜ 無組字：輸出全形『。』", active_k == "-", ACCENT_CYAN),
            ("【.】小數點", "組字中：進入第 5 碼附角 / 聯想字模式  ｜ 無組字：輸出全形『。』", active_k == ".", ACCENT_PINK),
        ]
        for idx, (k_name, k_desc, is_hi, col) in enumerate(key_guides):
            yy = 152 + idx * 46
            draw_rounded_rect(draw, (38, yy, 600, yy + 40), 8, (36, 48, 68) if is_hi else (24, 30, 42), col if is_hi else BORDER, 2 if is_hi else 1)
            draw.text((50, yy + 9), k_name, font=FONT_BOLD, fill=col)
            draw.text((168, yy + 10), k_desc, font=FONT_SMALL, fill=TEXT_BRIGHT if is_hi else TEXT_MAIN)

        draw_candidate_bar(draw, 24, 368, 592, code_s, desc_s, cands, sel_i, committed)
        draw_rounded_rect(draw, (24, 482, 592, 542), 10, BG_CARD, BORDER, 1)
        draw.text((38, 490), "🎯 偏好設定亦提供「直出全形標點模式」：按 + * / - . 立即上字並輸出 ， 、 ？ — 。", font=FONT_BOLD, fill=ACCENT_GOLD)
        draw.text((38, 515), "🎯 互動網頁教學版支援直接在螢幕上點擊小鍵盤或按實體小鍵盤即時試打！", font=FONT_SMALL, fill=TEXT_MAIN)

        draw_numpad(draw, 630, 100, active_key=active_k, trail_keys=trail_k)
        add_frame(img, 1500)

    os.makedirs(os.path.dirname(OUT_ARTIFACT_GIF), exist_ok=True)
    os.makedirs(os.path.dirname(OUT_DOCS_GIF), exist_ok=True)

    frames[0].save(
        OUT_ARTIFACT_GIF,
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        optimize=False,
    )
    frames[0].save(
        OUT_DOCS_GIF,
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        optimize=False,
    )
    print(f"成功產出教學動畫 GIF：{OUT_ARTIFACT_GIF} ({os.path.getsize(OUT_ARTIFACT_GIF)/1024:.1f} KB, 共 {len(frames)} 格分鏡)")


if __name__ == "__main__":
    build_frames()
