#!/usr/bin/env python3
"""从 app 图标（src-tauri/icons/icon.png）抠出 macOS 菜单栏用的单色模板图。

为什么需要单独一张图：
    macOS 菜单栏图标应当是「模板图」——系统拿它的 alpha 当形状，自己填黑或白，
    从而跟随菜单栏明暗反色（对应 Rust 侧的 TrayIconBuilder::icon_as_template(true)）。
    直接拿彩色 app 图标开 template 会出事：macOS 会把整个不透明的橙色方块当成遮罩，
    菜单栏上只剩一团实心色块。而彩色图标的 alpha 通道里也没有可用轮廓（内部全不透明），
    所以标记形状必须从像素的明度里抠，产物是一张新的 PNG。

怎么抠：
    标记（奶油色 V 形 + 下方圆点）的 min(R,G,B) 明显高于橙色底（实测底 14~88、标记 130+），
    于是用 min(R,G,B) 做阈值二值化，再按面积平均降采样到 36×36（18pt @2x）——
    二值化保证轮廓利落，降采样自然产生抗锯齿边缘。
    裁剪范围取「二值化后最大连通域」的包围盒再加 3% 余量：最大连通域就是 V 形，
    它的包围盒也覆盖了下方圆点，不需要额外处理。

用法：
    python3 scripts/make-tray-icon.py            # 重新生成 src-tauri/icons/tray-template.png
    换 logo 后必须重跑，并把结果一起提交。

依赖：只用标准库（zlib / struct），不装 Pillow。
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "src-tauri" / "icons" / "icon.png"
TARGET = ROOT / "src-tauri" / "icons" / "tray-template.png"

# 二值化阈值：min(R,G,B) 高于它才算标记。底图橙色在 130 以下，标记在 130 以上。
CUTOFF = 130
# 输出边长：36px = 18pt @2x，符合 macOS 菜单栏图标的常见尺寸
SIZE = 36
# 裁剪时在包围盒外留的余量比例
MARGIN = 0.03


def decode_png(path: Path) -> tuple[int, int, list[bytearray]]:
    """解码 8bit RGBA 非隔行 PNG，返回 (宽, 高, 每行 RGBA 字节)。只支持本项目图标的格式。"""
    raw = path.read_bytes()
    if raw[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} 不是 PNG")
    pos, idat = 8, bytearray()
    width = height = 0
    while pos < len(raw):
        (length,) = struct.unpack(">I", raw[pos : pos + 4])
        kind = raw[pos + 4 : pos + 8]
        data = raw[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color = struct.unpack(">IIBB", data[:10])
            if (depth, color) != (8, 6):
                raise SystemExit(f"只支持 8bit RGBA，当前 depth={depth} color={color}")
        elif kind == b"IDAT":
            idat += data
        elif kind == b"IEND":
            break
        pos += 12 + length

    pixels = zlib.decompress(bytes(idat))
    stride = width * 4
    rows: list[bytearray] = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        filter_type = pixels[offset]
        offset += 1
        line = bytearray(pixels[offset : offset + stride])
        offset += stride
        for x in range(stride):
            left = line[x - 4] if x >= 4 else 0
            up = previous[x]
            up_left = previous[x - 4] if x >= 4 else 0
            if filter_type == 1:
                line[x] = (line[x] + left) & 0xFF
            elif filter_type == 2:
                line[x] = (line[x] + up) & 0xFF
            elif filter_type == 3:
                line[x] = (line[x] + (left + up) // 2) & 0xFF
            elif filter_type == 4:
                estimate = left + up - up_left
                dl, du, dul = abs(estimate - left), abs(estimate - up), abs(estimate - up_left)
                predictor = left if (dl <= du and dl <= dul) else (up if du <= dul else up_left)
                line[x] = (line[x] + predictor) & 0xFF
        rows.append(line)
        previous = line
    return width, height, rows


def encode_png(path: Path, width: int, height: int, rgba: bytearray) -> None:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    scanlines = b"".join(
        b"\x00" + bytes(rgba[y * width * 4 : (y + 1) * width * 4]) for y in range(height)
    )
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(scanlines, 9))
        + chunk(b"IEND", b"")
    )


def mark_mask(width: int, height: int, rows: list[bytearray]) -> list[bytearray]:
    """二值化：不透明且 min(R,G,B) >= CUTOFF 的像素算标记。"""
    mask = [bytearray(width) for _ in range(height)]
    for y in range(height):
        row = rows[y]
        for x in range(width):
            r, g, b, a = row[x * 4 : x * 4 + 4]
            if a > 0 and min(r, g, b) >= CUTOFF:
                mask[y][x] = 1
    return mask


def largest_bounds(mask: list[bytearray]) -> tuple[int, int, int, int]:
    """最大连通域（4 邻域）的包围盒。它就是 V 形标记，包围盒顺带覆盖下方圆点。"""
    height, width = len(mask), len(mask[0])
    seen = [bytearray(width) for _ in range(height)]
    best: list[tuple[int, int]] = []
    for start_y in range(height):
        for start_x in range(width):
            if seen[start_y][start_x] or not mask[start_y][start_x]:
                continue
            stack = [(start_x, start_y)]
            seen[start_y][start_x] = 1
            points: list[tuple[int, int]] = []
            while stack:
                x, y = stack.pop()
                points.append((x, y))
                for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                    if 0 <= nx < width and 0 <= ny < height and not seen[ny][nx] and mask[ny][nx]:
                        seen[ny][nx] = 1
                        stack.append((nx, ny))
            if len(points) > len(best):
                best = points
    if not best:
        raise SystemExit("没找到标记：阈值或底图可能变了")
    xs = [p[0] for p in best]
    ys = [p[1] for p in best]
    return min(xs), max(xs), min(ys), max(ys)


def main() -> None:
    width, height, rows = decode_png(SOURCE)
    mask = mark_mask(width, height, rows)
    x0, x1, y0, y1 = largest_bounds(mask)

    # 留一点余量，避免标记贴边
    mx = int((x1 - x0 + 1) * MARGIN)
    my = int((y1 - y0 + 1) * MARGIN)
    x0, x1 = max(0, x0 - mx), min(width - 1, x1 + mx)
    y0, y1 = max(0, y0 - my), min(height - 1, y1 + my)
    box_w, box_h = x1 - x0 + 1, y1 - y0 + 1

    # 等比缩放到 SIZE-2，居中放进 SIZE×SIZE（四周留 1px，避免贴边）
    inner = SIZE - 2
    scale = min(inner / box_w, inner / box_h)
    draw_w, draw_h = max(1, round(box_w * scale)), max(1, round(box_h * scale))
    offset_x, offset_y = (SIZE - draw_w) // 2, (SIZE - draw_h) // 2

    rgba = bytearray(SIZE * SIZE * 4)
    for ty in range(draw_h):
        src_y0 = y0 + ty * box_h / draw_h
        src_y1 = y0 + (ty + 1) * box_h / draw_h
        for tx in range(draw_w):
            src_x0 = x0 + tx * box_w / draw_w
            src_x1 = x0 + (tx + 1) * box_w / draw_w
            total = 0
            count = 0
            for sy in range(int(src_y0), min(int(src_y1) + 1, y1 + 1)):
                for sx in range(int(src_x0), min(int(src_x1) + 1, x1 + 1)):
                    total += mask[sy][sx]
                    count += 1
            # 面积平均 → 二值轮廓的边缘自然获得抗锯齿；RGB 保持黑色，模板图只看 alpha
            alpha = round(total / count * 255) if count else 0
            index = ((offset_y + ty) * SIZE + (offset_x + tx)) * 4
            rgba[index + 3] = alpha

    encode_png(TARGET, SIZE, SIZE, rgba)
    covered = sum(1 for i in range(0, len(rgba), 4) if rgba[i + 3])
    print(f"底图 {width}×{height}，标记包围盒 {box_w}×{box_h}")
    print(f"已写出 {TARGET.relative_to(ROOT)}：{SIZE}×{SIZE}，覆盖 {covered}/{SIZE * SIZE} 格")


if __name__ == "__main__":
    sys.exit(main())
