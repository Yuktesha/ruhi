#!/usr/bin/env python3
"""從向量 SVG 產生如喜輸入法「關於」對話框專用之零外部相依 RLE Alpha 圖像素材。

產出格式 (`RLGO`):
  Header (10 bytes):
    - magic[4] : b"RLGO"
    - width    : u16 LE
    - height   : u16 LE
    - layers   : u8 (1 或 3)
    - reserved : u8 (0)
  Payload:
    - 依序儲存 `layers` 個圖層，每個圖層為 `(run_len: u8 in 1..=255, alpha: u8)` 之 RLE 位元組串，
      解壓縮後總長度必須恰等於 `width * height`。
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import tempfile
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
ASSETS_DIR = ROOT / "assets"
ATG_LOGO_EXE = pathlib.Path(r"C:\ATGprjs\_lib_rs\target\release\atg_logo.exe")
SCRATCH_PREVIEW = pathlib.Path(
    r"C:\Users\Yuktesha\.gemini\antigravity\brain\8103940b-b03f-423c-aa00-e1b36ad711a5\scratch\about_logo_preview.png"
)


def encode_rle_layer(alphas: bytes) -> bytes:
    """將單通道 Alpha 位元組序列編碼為 `(run_len: u8, alpha: u8)` RLE 格式。"""
    out = bytearray()
    n = len(alphas)
    i = 0
    while i < n:
        val = alphas[i]
        run = 1
        while i + run < n and alphas[i + run] == val and run < 255:
            run += 1
        out.append(run)
        out.append(val)
        i += run
    return bytes(out)


def encode_rlgo(width: int, height: int, layers: list[bytes]) -> bytes:
    """封裝多圖層 RLE Alpha 為 `RLGO` 二進位檔。"""
    expected = width * height
    header = (
        b"RLGO"
        + int(width).to_bytes(2, "little")
        + int(height).to_bytes(2, "little")
        + bytes([len(layers), 0])
    )
    payload = bytearray()
    for layer in layers:
        if len(layer) != expected:
            raise ValueError(f"圖層像素長度不符: expected {expected}, got {len(layer)}")
        payload.extend(encode_rle_layer(layer))
    return header + bytes(payload)


def rasterize_with_atg_logo(svg_path: pathlib.Path, size: int, padding: float) -> Image.Image:
    """呼叫 `atg_logo.exe` 將 SVG 等比點陣化為 RGBA PNG 並載入為 PIL Image。"""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = pathlib.Path(tmp)
        cmd = [
            str(ATG_LOGO_EXE),
            str(svg_path),
            "-o",
            str(tmp_dir),
            "-n",
            "item",
            "-p",
            f"{padding}",
            "--ico-sizes",
            "16",
            "--png-sizes",
            str(size),
            "--no-rc",
            "--no-copy-svg",
        ]
        res = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if res.returncode != 0:
            raise RuntimeError(f"atg_logo 執行失敗 ({res.returncode}): {res.stdout}\n{res.stderr}")
        png_path = tmp_dir / f"item_{size}.png"
        if not png_path.is_file():
            raise FileNotFoundError(f"找不到輸出 PNG: {png_path}")
        with Image.open(png_path) as im:
            return im.convert("RGBA")


def split_studio_layer_svg(svg_text: str, keep_class: str) -> str:
    """保留指定圖層類別（`fil0`/`fil1`/`fil2`），隱藏其餘圖層並保持相同 `viewBox`。"""
    out_lines: list[str] = []
    cls_pattern = re.compile(r'class="(fil[0-9]+)"')
    for line in svg_text.splitlines(keepends=True):
        m = cls_pattern.search(line)
        if m and m.group(1) != keep_class:
            continue
        out_lines.append(line)
    return "".join(out_lines)


def composite_layer_over(
    dst: Image.Image, alpha_bytes: bytes, rgb: tuple[int, int, int]
) -> Image.Image:
    """以純色 `(r, g, b)` 搭配 8-bit Alpha 圖層覆疊至 `dst`（RGBA）。"""
    layer_img = Image.new("RGBA", dst.size, (*rgb, 0))
    alpha_img = Image.frombytes("L", dst.size, alpha_bytes)
    layer_img.putalpha(alpha_img)
    return Image.alpha_composite(dst, layer_img)


def build_assets() -> None:
    if not ATG_LOGO_EXE.is_file():
        raise FileNotFoundError(f"找不到 atg_logo.exe: {ATG_LOGO_EXE}")

    studio_svg = ASSETS_DIR / "studio_logo.svg"
    ruhi_svg = ASSETS_DIR / "ICON.svg"
    studio_src = studio_svg.read_text(encoding="utf-8")

    # 1. 點陣化 Studio Logo 三色獨立 Alpha 圖層 (640x100)
    studio_layers: list[bytes] = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = pathlib.Path(tmp)
        for cls_name in ("fil0", "fil1", "fil2"):
            split_svg = split_studio_layer_svg(studio_src, cls_name)
            split_path = tmp_dir / f"studio_{cls_name}.svg"
            split_path.write_text(split_svg, encoding="utf-8")
            img_sq = rasterize_with_atg_logo(split_path, size=640, padding=0.0)
            banner = img_sq.crop((0, 270, 640, 370))
            alpha_bytes = banner.getchannel("A").tobytes()
            if max(alpha_bytes) == 0:
                raise RuntimeError(f"圖層 {cls_name} 裁切結果完全透明，請檢查 viewBox 與裁切範圍")
            studio_layers.append(alpha_bytes)

    studio_out = ASSETS_DIR / "studio_logo_640x100.rle3"
    studio_blob = encode_rlgo(640, 100, studio_layers)
    studio_out.write_bytes(studio_blob)

    # 2. 點陣化 RuhI 品牌圖示 Alpha 圖層 (128x128)
    ruhi_img = rasterize_with_atg_logo(ruhi_svg, size=128, padding=0.04)
    ruhi_alpha = ruhi_img.getchannel("A").tobytes()
    ruhi_out = ASSETS_DIR / "ruhi_icon_128x128.rle1"
    ruhi_blob = encode_rlgo(128, 128, [ruhi_alpha])
    ruhi_out.write_bytes(ruhi_blob)

    # 3. 產生深淺色對照預覽圖供視覺核對
    preview = Image.new("RGBA", (720, 360), (200, 200, 200, 255))
    themes = [
        ("light", (0xF7, 0xF5, 0xF0), (0x2F, 0x31, 0x8B), 0),
        ("dark", (0x1E, 0x1E, 0x22), (0xE8, 0xEE, 0xF8), 180),
    ]
    for _, bg_rgb, ink_rgb, y_off in themes:
        panel = Image.new("RGBA", (720, 180), (*bg_rgb, 255))

        icon_canvas = Image.new("RGBA", (128, 128), (0, 0, 0, 0))
        icon_canvas = composite_layer_over(icon_canvas, ruhi_alpha, (0xEA, 0x5B, 0x24))
        icon_sm = icon_canvas.resize((56, 56), Image.Resampling.BILINEAR)
        panel.alpha_composite(icon_sm, (24, 24))

        logo_canvas = Image.new("RGBA", (640, 100), (0, 0, 0, 0))
        logo_canvas = composite_layer_over(logo_canvas, studio_layers[0], (0x00, 0xA2, 0xE9))
        logo_canvas = composite_layer_over(logo_canvas, studio_layers[1], (0xE4, 0x00, 0x82))
        logo_canvas = composite_layer_over(logo_canvas, studio_layers[2], ink_rgb)
        logo_sm = logo_canvas.resize((320, 50), Image.Resampling.BILINEAR)
        panel.alpha_composite(logo_sm, (24, 104))

        preview.paste(panel, (0, y_off))

    SCRATCH_PREVIEW.parent.mkdir(parents=True, exist_ok=True)
    preview.save(SCRATCH_PREVIEW)

    print(f"[OK] {studio_out} ({len(studio_blob)} bytes)")
    print(f"[OK] {ruhi_out} ({len(ruhi_blob)} bytes)")
    print(f"[OK] preview: {SCRATCH_PREVIEW}")


if __name__ == "__main__":
    build_assets()
