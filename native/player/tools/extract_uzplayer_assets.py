#!/usr/bin/env python3
"""从官方 UZPlayer.exe 里提取样式资源（QSS + 图标）。

官方播放器是 Qt6 Widgets + QSS，界面素材以 Qt 资源的形式编进 exe。
这个 exe 的 Qt 资源没有 `qres` 头（被裁掉了），但两块数据都在 .rdata 里：

  1. 名字表：`[u16 长度][u32 qHash][UTF-16BE 名称]` 连续排列
  2. 数据区：`[u32 长度][文件字节]` 连续排列，顺序与名字表中的文件节点一一对应

两个数组的长度都验过：名字表 249 项（15 个目录 + 234 个文件），
数据区正好 234 块。顺序对应关系用 `playWhite2x.png`（56x56）等
QSS 里能查到尺寸的图标交叉验证过。

QSS 不是独立文件，而是以字符串常量逐条内联在 .text 里
（`QPushButton#btnPlay{...}`），这里按 `选择器{...}` 扫描汇总。

用法：
    python3 native/player/tools/extract_uzplayer_assets.py <UZPlayer.exe> [输出目录]
"""

import re
import struct
import sys
import zlib
from pathlib import Path

PNG_SIG = b"\x89PNG\r\n\x1a\n"
JPG_SIG = b"\xff\xd8\xff"


def find_name_table(data: bytes) -> tuple[int, list[str]]:
    """定位并解析 Qt 资源名字表，返回 (起点, 名称列表)。

    每项是 `[u16 字符数][u32 qHash][UTF-16BE 名称]`。表里有个别 1 字节的
    对齐空洞，逐项累加会错位，所以先在整段窗口里扫出所有「单看合法」的
    候选项，再按 offset 精确拼接成最长的一条链。
    """
    # 名字表里一定有 png 名称，拿一个当锚点圈定窗口
    anchor = data.find(b"p\x00l\x00a\x00y\x00W\x00h\x00i\x00t\x00e\x002\x00x\x00.\x00p\x00n\x00g")
    if anchor < 0:
        raise SystemExit("找不到资源名表锚点，exe 可能换了资源格式")
    lo = max(0, anchor - 0x4000)
    hi = min(len(data), anchor + 0x8000)

    cands = {}
    for pos in range(lo, hi):
        if pos + 6 > hi:
            break
        n = struct.unpack(">H", data[pos : pos + 2])[0]
        if n == 0 or n > 300 or pos + 6 + n * 2 > hi:
            continue
        try:
            s = data[pos + 6 : pos + 6 + n * 2].decode("utf-16-be")
        except UnicodeDecodeError:
            continue
        if not s or not all(0x20 <= ord(c) < 0x3000 for c in s):
            continue
        if not (s[0].isalnum() or s[0] in "._-"):
            continue
        cands[pos] = (n, s)

    best_start, best_seq = None, []
    for start in cands:
        pos = start
        seq = []
        while pos in cands:
            n, s = cands[pos]
            seq.append(s)
            pos += 6 + n * 2
        if len(seq) > len(best_seq):
            best_start, best_seq = start, seq
    if not best_seq:
        raise SystemExit("资源名表定位失败")

    return best_start, best_seq


def find_data_start(data: bytes, names_end_guess: int) -> int:
    """数据区起点：从第一个 PNG 反着走，一路 [u32 长度][数据] 直到走不动。"""
    first_png = data.find(PNG_SIG)
    p = first_png
    while True:
        found = None
        for ln in range(16, 2_000_000):
            cand = p - 4 - ln - 4
            if cand < 0:
                break
            if struct.unpack(">I", data[cand : cand + 4])[0] == ln:
                found = cand
                break
        if found is None:
            break
        p = found + 4
    return p - 4  # 指向第一块的长度前缀


def walk_blocks(data: bytes, start: int, stop: int) -> list[tuple[int, int]]:
    """按 [u32 长度][数据] 遍历，容忍块间 0~3 字节对齐填充。"""
    blocks = []
    pos = start
    while pos < stop:
        ln = struct.unpack(">I", data[pos : pos + 4])[0]
        body = pos + 4
        if ln < 16 or body + ln > len(data):
            break
        # 选一个能接上下一个合理块的填充量
        chosen = None
        for pad in range(4):
            nxt = body + ln + pad
            if nxt >= stop:
                chosen = pad
                break
            nln = struct.unpack(">I", data[nxt : nxt + 4])[0]
            if 16 <= nln <= 8_000_000 and nxt + 4 + nln <= len(data):
                chosen = pad
                break
        if chosen is None:
            chosen = 0
        blocks.append((body, ln))
        pos = body + ln + chosen
    return blocks


def build_paths(names: list[str], url_map: dict[str, str]) -> list[str]:
    """文件节点 → 相对路径。

    名字表只有深度优先顺序、没有层级信息，所以目录不能靠顺序推。
    权威来源是 QSS 里的 `url(:/resource/<目录>/<文件>)`，
    没被 QSS 引用到的资源（图标、背景图）落到根目录。
    """
    paths = []
    for name in names:
        if "." not in name:
            continue
        sub = url_map.get(name, "")
        paths.append(f"{sub}/{name}" if sub else name)
    return paths


def kind_of(blob: bytes) -> str:
    if blob.startswith(PNG_SIG):
        return "png"
    if blob.startswith(JPG_SIG):
        return "jpg"
    if blob[:1] == b"<":
        return "svg"
    return "other"


def ext_kind(name: str) -> str:
    ext = name.rsplit(".", 1)[-1].lower()
    if ext == "jpg":
        return "jpg"
    if ext in ("png", "svg"):
        return ext
    return "other"


def align(blocks: list[tuple[int, int]], files: list[str], data: bytes) -> dict[int, str]:
    """把数据块和文件名配对。

    两者大致同序，但资源里既有「有名字没数据」的项（同一批 qm 的多个语言
    变体只存了一份），也有「有数据没名字」的孤儿块（多出来的一帧动画）。
    直接 zip 会从第一个错位点开始全部串位。

    这里用动态规划做保序对齐：类型匹配的配对免费，跳过任意一方记 1 分代价，
    求总代价最小的路径。
    """
    M, N = len(blocks), len(files)
    INF = float("inf")
    bkind = [kind_of(data[off : off + min(8, ln)]) for off, ln in blocks]
    nkind = [ext_kind(f) for f in files]
    cost = [[0] * (N + 1) for _ in range(M + 1)]
    move = [[None] * (N + 1) for _ in range(M + 1)]
    for i in range(1, M + 1):
        cost[i][0] = cost[i - 1][0] + 1
        move[i][0] = "skip_block"
    for j in range(1, N + 1):
        cost[0][j] = cost[0][j - 1] + 1
        move[0][j] = "skip_name"
    for i in range(1, M + 1):
        for j in range(1, N + 1):
            pair = cost[i - 1][j - 1] + (0 if bkind[i - 1] == nkind[j - 1] else 4)
            sb = cost[i - 1][j] + 1
            sn = cost[i][j - 1] + 1
            best = min(pair, sb, sn)
            cost[i][j] = best
            move[i][j] = "pair" if best == pair else ("skip_block" if best == sb else "skip_name")

    mapping = {}
    i, j = M, N
    while i > 0 and j > 0:
        m = move[i][j]
        if m == "pair":
            mapping[i - 1] = files[j - 1]
            i, j = i - 1, j - 1
        elif m == "skip_block":
            i -= 1
        else:
            j -= 1
    skipped_names = [files[jj] for jj in range(j)]
    skipped_blocks = [ii for ii in range(i)]
    if skipped_names:
        print("未取到数据的名字:", skipped_names)
    if skipped_blocks:
        print("无名字的孤儿块:", skipped_blocks)
    return mapping


def collect_qss(data: bytes) -> tuple[list[tuple[str, str]], dict[str, str]]:
    """扫描 .text 里的内联样式表字符串，按 选择器{...} 归集。"""
    rules = {}
    url_map = {}
    i = 0
    n = len(data)
    while True:
        brace = data.find(b"{", i)
        if brace < 0 or brace >= n:
            break
        # 选择器：向前回溯到字符串边界（\x00 或前一个 }）
        s = brace
        while s > 0 and s > brace - 200:
            c = data[s - 1]
            if c == 0 or c == 0x7D or c == 0x0A and data[s - 3 : s] == b"}\n\x00":
                break
            s -= 1
        selector = data[s:brace].decode("latin-1").strip()
        # 向后找匹配的右括号
        e = brace + 1
        while e < n and data[e] != 0x7D:
            e += 1
        body = data[brace + 1 : e].decode("latin-1")
        if e + 1 > n:
            break
        if selector and ("#" in selector or "Q" in selector or ":" in selector) and "\n\n" not in selector:
            key = selector.replace("\t", " ").strip()
            body_clean = body.strip()
            ascii_ok = all(0x20 <= ord(c) < 0x7F for c in key + body_clean)
            if len(key) < 200 and ascii_ok and ":" in body_clean:
                rules.setdefault(key, body_clean)
        # 顺带收集 url(:/resource/<目录>/<文件>)，这是资源目录的权威来源
        for m in re.finditer(r"url\(:/resource/([^)]*?)/([^/)]+)\)", body):
            url_map[m.group(2)] = m.group(1)
        i = e + 1
    return sorted(rules.items()), url_map


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    exe = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else exe.parent / "assets"
    data = exe.read_bytes()

    names_start, names = find_name_table(data)
    data_start = find_data_start(data, names_start)
    blocks = walk_blocks(data, data_start, names_start)
    qss, url_map = collect_qss(data)
    files = [n for n in names if "." in n]
    mapping = align(blocks, files, data)

    print(f"名字表 : {len(names)} 项（文件 {len(files)}）@ {names_start:#x}")
    print(f"数据区 : {len(blocks)} 块 @ {data_start:#x}")
    print(f"配对   : {len(mapping)} 组")

    icons_dir = out / "icons"
    written = 0
    total_bytes = 0
    for bi, fname in sorted(mapping.items()):
        off, ln = blocks[bi]
        blob = data[off : off + ln]
        sub = url_map.get(fname, "")
        dest = icons_dir / sub / fname if sub else icons_dir / fname
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(blob)
        written += 1
        total_bytes += len(blob)

    qss_text = (
        "/* 从官方 UZPlayer.exe 提取的 QSS 原文（逐条内联样式表汇总） */\n\n"
        + "\n\n".join(f"{sel} {{{body}}}" for sel, body in qss)
    )
    (out / "official.qss").write_text(qss_text, encoding="utf-8")

    print(f"写出图标: {written} 个 / {total_bytes} 字节 → {icons_dir}")
    print(f"写出 QSS : {len(qss)} 条规则 → {out / 'official.qss'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
