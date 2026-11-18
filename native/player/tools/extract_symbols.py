#!/usr/bin/env python3
"""从官方 UZPlayer.exe 里提取 C++ 符号，还原播放器的实现结构。

Qt 的 moc 元信息会把类名和方法名以 `ClassName::methodName` 的形式留在
二进制里（槽函数、信号、Q_INVOKABLE 都算）。这些符号比 QSS 更有价值：
QSS 只告诉你「长什么样」，符号表告诉你「有哪些类、每类负责什么」。

用法：
    python3 native/player/tools/extract_symbols.py <UZPlayer.exe> [输出.md]

没有输出路径时只打印统计；给了路径就生成一份清单文档。
"""

import collections
import re
import sys
from pathlib import Path

# 只关心播放器自己的类，Qt/std 的一大堆（QWidget::paintEvent 之类）全部丢掉
IGNORE_PREFIX = ("std", "Q", "boost", "VLC", "libvlc", "__", "_")

SYM_RE = re.compile(rb"([A-Z][A-Za-z0-9_]{2,32})::([A-Za-z_~][A-Za-z0-9_]{1,48})")


def collect(data: bytes) -> dict[str, set[str]]:
    out: dict[str, set[str]] = collections.defaultdict(set)
    for m in SYM_RE.finditer(data):
        cls = m.group(1).decode("ascii", "ignore")
        fn = m.group(2).decode("ascii", "ignore")
        if cls.startswith(IGNORE_PREFIX) or len(fn) < 2:
            continue
        out[cls].add(fn)
    # 只保留有两个以上方法的类，避免把偶然匹配当成类
    return {c: v for c, v in out.items() if len(v) >= 2}


def classify(fn: str) -> str:
    if fn.startswith("BTool"):
        return "工具栏/快捷键槽"
    if fn.startswith("TcpRecv"):
        return "协议回包分支"
    if fn.startswith("KeyPress") or fn.startswith("KeySeek"):
        return "快捷键"
    if fn.startswith("Timer"):
        return "定时器"
    if "Popup" in fn or "popup" in fn:
        return "弹层"
    if fn.startswith(("on_btn", "on_comb", "on_")):
        return "控件槽"
    if fn.startswith(("Show", "Hide", "Set", "Get", "Update", "Init", "Change", "reset")):
        return "状态/显示"
    return "其它"


def render(classes: dict[str, set[str]], src_name: str) -> str:
    lines = [
        "# 官方播放器实现结构（从符号表还原）",
        "",
        f"> 来源：`{src_name}` 里残留的 C++ 符号（Qt moc 元信息）。",
        "> 生成命令：`python3 native/player/tools/extract_symbols.py <exe> <本文件>`",
        ">",
        "> 这份文档回答的是「官方有哪些类、每个类管什么」，",
        "> 样式部分见 `PLAYER-FEATURE-PARITY.md`，素材见 `native/player/assets/README.md`。",
        "",
        f"识别到 **{len(classes)}** 个自有类、"
        f"**{sum(len(v) for v in classes.values())}** 个方法。",
        "",
    ]
    # MainWindow 方法最多，放最前面
    order = sorted(classes, key=lambda c: (-len(classes[c]), c))
    for cls in order:
        fns = classes[cls]
        lines.append(f"## {cls}（{len(fns)} 个方法）")
        lines.append("")
        groups: dict[str, list[str]] = collections.defaultdict(list)
        for fn in sorted(fns):
            groups[classify(fn)].append(fn)
        # 分类按固定顺序输出，没内容的不写
        for g in ("弹层", "协议回包分支", "快捷键", "定时器", "工具栏/快捷键槽", "控件槽", "状态/显示", "其它"):
            if g not in groups:
                continue
            lines.append(f"**{g}**")
            lines.append("")
            for fn in groups[g]:
                lines.append(f"- `{fn}`")
            lines.append("")
    return "\n".join(lines)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    exe = Path(sys.argv[1])
    data = exe.read_bytes()
    classes = collect(data)
    if len(sys.argv) >= 3:
        out = Path(sys.argv[2])
        out.write_text(render(classes, exe.name), encoding="utf-8")
        print(f"写出 {out}（{len(classes)} 个类）")
    else:
        for cls, fns in sorted(classes.items(), key=lambda kv: -len(kv[1])):
            print(f"{cls:28s} {len(fns):4d}")


if __name__ == "__main__":
    main()
