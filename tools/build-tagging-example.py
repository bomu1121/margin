from PIL import Image, ImageDraw, ImageFont
from pathlib import Path

W, H = 2000, 1400
BG = "#f4f1e8"
SURFACE = "#fffdf8"
INK = "#1b1813"
INK2 = "#514a3f"
INK3 = "#686055"
LINE = "#e2dccf"
LINE2 = "#c9c0ae"
BLUE = "#1f4fd8"
BLUE_SOFT = "#eef2ff"
BLUE_LINE = "#c8d5f7"
GREEN = "#187a4d"
GREEN_SOFT = "#eaf6ef"
GREEN_LINE = "#bfe3cf"
GOLD = "#8a5d0c"
GOLD_SOFT = "#fbf3e2"
GOLD_LINE = "#e4c98f"
RED = "#b4232f"
RED_SOFT = "#fbecee"
RED_LINE = "#e7b6bb"

SANS = "C:/Windows/Fonts/msyh.ttc"
SANS_B = "C:/Windows/Fonts/msyhbd.ttc"
SERIF = "C:/Windows/Fonts/NotoSerifSC-VF.ttf"


def font(path, size):
    return ImageFont.truetype(path, size)


F_TITLE = font(SERIF, 56)
F_SUB = font(SANS, 23)
F_COL = font(SANS_B, 22)
F_CARD = font(SANS_B, 22)
F_BODY = font(SANS, 17)
F_MONO = font(SANS, 16)
F_SMALL = font(SANS, 15)
F_FOOT = font(SANS, 17)

img = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(img)


def text_at(text, x, y, fnt, fill):
    d.multiline_text((x, y), text, font=fnt, fill=fill, spacing=6)


def rounded(box, radius, fill, outline, width=2):
    d.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def arrow(x1, y1, x2, y2, color=INK3, width=3):
    d.line((x1, y1, x2, y2), fill=color, width=width)
    if x2 > x1:
        d.polygon([(x2, y2), (x2 - 12, y2 - 7), (x2 - 12, y2 + 7)], fill=color)
    elif x2 < x1:
        d.polygon([(x2, y2), (x2 + 12, y2 - 7), (x2 + 12, y2 + 7)], fill=color)
    else:
        d.polygon([(x2, y2), (x2 - 7, y2 - 12), (x2 + 7, y2 - 12)], fill=color)


def check(text, x, y, color=GREEN):
    text_at("会   " + text, x, y, F_BODY, color)


def cross(text, x, y, color=RED):
    text_at("不会 " + text, x, y, F_BODY, color)


# Header
text_at("打标流程：以《九个字里的官渡》为例", 80, 48, F_TITLE, INK)
text_at("同一份 front matter，经过 parse_file_at 后，哪些能出现在标签台 / 图谱 / 检索", 84, 125, F_SUB, INK2)
d.line((80, 176, W - 80, 176), fill=LINE2, width=2)

# Column labels
text_at("1. 原始字段", 80, 200, F_COL, BLUE)
text_at("2. 解析 / 索引", 700, 200, F_COL, BLUE)
text_at("3. 实际消费", 1300, 200, F_COL, BLUE)

# Left example card
rounded((80, 245, 620, 900), 12, SURFACE, LINE2, 2)
text_at("《九个字里的官渡》", 102, 264, F_CARD, INK)
text_at("手工 / 旧笔记的 front matter", 102, 300, F_SMALL, INK3)
raw = """类型: 学习笔记
来源: 文本
tags:
  - 领域/历史
  - 主题/官渡之战
  - 来源/三国志
人物:
  - 曹操
  - 袁绍
事件:
  - 官渡之战
时期:
  - 东汉末年"""
text_at(raw, 104, 338, F_MONO, INK2)
rounded((100, 760, 600, 876), 8, GOLD_SOFT, GOLD_LINE, 1)
text_at("对照：AI 成稿通常没有 tags。\n只写 类型 / 来源 / 作者 / 链接；\n订阅 options.tags 为空，领域也为空。", 116, 776, F_BODY, GOLD)

# Middle parse card
rounded((700, 245, 1240, 670), 12, SURFACE, LINE2, 2)
text_at("parse_file_at", 722, 264, F_CARD, INK)
y = 306
for line in [
    "tags → [领域/历史, 主题/官渡之战, 来源/三国志]",
    "domain = 历史（从 领域/ 取）",
    "people = [曹操, 袁绍]",
    "events = [官渡之战]",
    "periods = [东汉末年]",
]:
    text_at(line, 724, y, F_BODY, INK2)
    y += 31
text_at("类型: 学习笔记 是 scalar，不转成 类型/ 标签", 724, 470, F_BODY, RED)
text_at("来源: 文本 是 scalar，不转成 来源/ 标签", 724, 505, F_BODY, RED)
text_at("（手写 tags 里的 来源/三国志 会保留）", 724, 540, F_SMALL, INK3)

rounded((700, 700, 1240, 900), 12, GOLD_SOFT, GOLD_LINE, 2)
text_at("notes 表 + FTS", 722, 718, F_CARD, INK)
text_at("tags / people / events / periods → JSON", 724, 758, F_BODY, INK2)
text_at("FTS = title + tags + people + events + periods + body", 724, 792, F_BODY, INK2)
text_at("事件能搜；类型 scalar 不进 FTS", 724, 826, F_SMALL, INK3)

# Right consumer cards
rounded((1300, 245, 1920, 560), 12, SURFACE, LINE2, 2)
text_at("标签台", 1322, 264, F_CARD, INK)
check("领域：历史", 1324, 304)
check("主题：官渡之战", 1324, 338)
check("来源：三国志", 1324, 372)
check("人物：曹操 / 袁绍", 1324, 406)
check("时期：东汉末年", 1324, 440)
cross("类型：学习笔记（scalar 没转 tag）", 1324, 480)
cross("事件：官渡之战（attrList 不取 events）", 1324, 514)

rounded((1300, 595, 1920, 840), 12, SURFACE, LINE2, 2)
text_at("关系图谱", 1322, 614, F_CARD, INK)
check("note → 历史 / 官渡之战 / 三国志", 1324, 654)
check("note → 曹操 / 袁绍 / 东汉末年", 1324, 688)
check("note–note 共享这些属性时连边", 1324, 722)
cross("事件节点和边（官渡之战不出现）", 1324, 762)

rounded((1300, 875, 1920, 1120), 12, SURFACE, LINE2, 2)
text_at("检索", 1322, 894, F_CARD, INK)
check("搜「官渡之战」：events + body", 1324, 934)
check("搜「历史」：tags", 1324, 968)
cross("搜「学习笔记」：类型 scalar 不进 FTS", 1324, 1008)
text_at("除非正文本身出现这四个字。", 1324, 1048, F_SMALL, INK3)

# Arrows
arrow(620, 440, 700, 440)
arrow(620, 790, 700, 790)
arrow(1240, 500, 1300, 500)
d.line((1240, 800, 1270, 800), fill=INK3, width=3)
d.line((1270, 500, 1270, 800), fill=INK3, width=3)
arrow(1270, 500, 1300, 500)
arrow(1270, 800, 1300, 800)

# Bottom current AI note strip
rounded((80, 1160, 1920, 1340), 10, RED_SOFT, RED_LINE, 2)
text_at("AI 成稿的实际结果（当前可见 18 篇都是这种）", 102, 1176, F_CARD, RED)
text_at("outbox 写：类型 / 来源 / 作者 / 链接；tags 为空；领域为空。", 104, 1216, F_BODY, INK2)
text_at("parse 从 B站作者派生 UP主/<作者>；人物/事件/时期只有抽取成功时才有。", 104, 1248, F_BODY, INK2)
text_at("所以标签台只剩 UP主 / 人物 / 时期；事件仍不可见；类型/领域/主题/来源组是空的。", 104, 1280, F_BODY, INK2)
text_at("检索仍可搜到正文，但标签面没有东西可点。", 104, 1312, F_BODY, INK3)

out = Path("docs/diagrams/tagging-example-2026-11-23.png")
out.parent.mkdir(parents=True, exist_ok=True)
img.save(out)
print(out, img.size)
