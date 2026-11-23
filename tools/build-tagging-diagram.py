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
GOLD = "#8a5d0c"
GOLD_SOFT = "#fbf3e2"
RED = "#b4232f"
RED_SOFT = "#fbecee"

SANS = "C:/Windows/Fonts/msyh.ttc"
SANS_B = "C:/Windows/Fonts/msyhbd.ttc"
SERIF = "C:/Windows/Fonts/NotoSerifSC-VF.ttf"


def font(path, size):
    return ImageFont.truetype(path, size)


F_TITLE = font(SERIF, 58)
F_SUB = font(SANS, 23)
F_COL = font(SANS_B, 23)
F_TITLE2 = font(SANS_B, 22)
F_BODY = font(SANS, 17)
F_SMALL = font(SANS, 15)
F_TAG = font(SANS_B, 16)
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


def dashed(x1, y1, x2, y2, color, width=2, dash=12):
    length = ((x2 - x1) ** 2 + (y2 - y1) ** 2) ** 0.5
    if length == 0:
        return
    dx = (x2 - x1) / length
    dy = (y2 - y1) / length
    pos = 0
    while pos < length:
        end = min(pos + dash, length)
        d.line((x1 + dx * pos, y1 + dy * pos, x1 + dx * end, y1 + dy * end), fill=color, width=width)
        pos += dash * 2


def chip(text, x, y, fill, outline, color, width=None):
    fnt = F_TAG
    bbox = d.textbbox((0, 0), text, font=fnt)
    w = width or (bbox[2] - bbox[0] + 18)
    h = 28
    rounded((x, y, x + w, y + h), 6, fill, outline, 1)
    d.text((x + 9, y + 5), text, font=fnt, fill=color)
    return w


# Header
text_at("现在的打标流程", 80, 48, F_TITLE, INK)
text_at("读代码得到的实际链路：字段 → 解析 → 索引 → 标签台 / 图谱 / 检索", 84, 125, F_SUB, INK2)
d.line((80, 176, W - 80, 176), fill=LINE2, width=2)

# Column labels
text_at("写入", 80, 200, F_COL, BLUE)
text_at("解析 / 索引", 680, 200, F_COL, BLUE)
text_at("消费", 1360, 200, F_COL, BLUE)

# A: front matter
rounded((80, 245, 620, 840), 12, SURFACE, LINE2, 2)
text_at("Markdown front matter", 102, 264, F_TITLE2, INK)
text_at("手工 / 旧笔记", 102, 302, F_SMALL, INK3)
y = 332
for line in [
    "`tags:` 列表（自由命名）",
    "`类型:` / `领域:` 标量",
    "`作者 / 来源 / 原始链接`",
    "`人物 / 事件 / 时期` 列表",
]:
    text_at(line.replace("`", ""), 104, y, F_BODY, INK2)
    y += 31
d.line((102, 492, 598, 492), fill=LINE, width=1)
text_at("AI 生产入库（outbox）", 102, 510, F_TITLE2, INK)
y = 548
for line in [
    "artifact.tags ← options.tags",
    "订阅任务 options.tags = []",
    "类型 / 来源 / 作者 / 链接 → scalar",
    "领域 → 当前为空",
    "人物 / 事件 ← extract_entities",
]:
    text_at(line, 104, y, F_BODY, INK2)
    y += 31

# C: parse
rounded((680, 245, 1280, 840), 12, SURFACE, LINE2, 2)
text_at("parse_file_at → notes / FTS", 702, 264, F_TITLE2, INK)
y = 306
for line in [
    "读 `tags`，删除 `自动制作`",
    "B站 `作者` → 派生 `UP主/<作者>`",
    "`领域/…` tag → `domain` 列",
    "读 `人物 / 事件 / 时期`",
]:
    text_at(line.replace("`", ""), 704, y, F_BODY, INK2)
    y += 31
text_at("B站派生 UP 标签是当前唯一自动标签", 704, 438, F_SMALL, GREEN)
# gaps
rounded((700, 486, 1260, 570), 8, RED_SOFT, "#e7b6bb", 1)
text_at("断点 1：类型 / 领域 是 scalar，不转成 类型/ / 领域/ tag", 714, 498, F_BODY, RED)
rounded((700, 584, 1260, 700), 8, RED_SOFT, "#e7b6bb", 1)
text_at("断点 2：来源: scalar 不转 tag；\n来源/ tag 又在 outbox 被过滤，来源组为空", 714, 596, F_BODY, RED)
rounded((700, 716, 1260, 806), 8, BLUE_SOFT, BLUE_LINE, 1)
text_at("FTS 会收：title + tags + people + events\n+ periods + body", 714, 730, F_BODY, BLUE)

# D: index
rounded((680, 880, 1280, 1160), 12, GOLD_SOFT, "#e4c98f", 2)
text_at("notes 表 + 全文索引", 702, 898, F_TITLE2, INK)
y = 940
for line in [
    "tags / people / events / periods → JSON",
    "domain → 单独列",
    "标签筛选在 UI 精确匹配，不是后端 facet",
]:
    text_at(line, 704, y, F_BODY, INK2)
    y += 32
text_at("事件有列、能进 FTS，但不在 attrList", 704, 1078, F_SMALL, RED)

# E/F/G
rounded((1360, 245, 1920, 505), 12, SURFACE, LINE2, 2)
text_at("标签台 TagsView", 1382, 264, F_TITLE2, INK)
text_at("attrList = tags + people + periods", 1382, 304, F_BODY, INK2)
text_at("分组：领域 / 类型 / UP / 主题 / 来源 / 其他 / 人物 / 时期", 1382, 338, F_BODY, INK2)
text_at("事件不在 attrList → 标签台看不到", 1382, 378, F_SMALL, RED)
text_at("当前 18 篇可见笔记：tags / people / events / periods 全为 0", 1382, 420, F_SMALL, INK3)

rounded((1360, 545, 1920, 805), 12, SURFACE, LINE2, 2)
text_at("关系图谱", 1382, 564, F_TITLE2, INK)
text_at("同一套 attrList；note → attr 连边", 1382, 604, F_BODY, INK2)
text_at("note–note 共享属性连边", 1382, 638, F_BODY, INK2)
text_at("事件不进入 → 图谱少一层关系", 1382, 678, F_SMALL, RED)
text_at("UP 主派生标签可连，但当前笔记没有作者", 1382, 720, F_SMALL, INK3)

rounded((1360, 845, 1920, 1105), 12, SURFACE, LINE2, 2)
text_at("检索", 1382, 864, F_TITLE2, INK)
text_at("FTS 命中 tags / people / events / periods / body", 1382, 904, F_BODY, INK2)
text_at("标签筛选 = 前端精确匹配", 1382, 938, F_BODY, INK2)
text_at("事件可搜，但标签台/图谱看不到", 1382, 978, F_SMALL, GOLD)

# Arrows
arrow(620, 542, 680, 542)
arrow(980, 840, 980, 880)
# D to consumers bus
d.line((1280, 1020, 1320, 1020), fill=INK3, width=3)
d.line((1320, 375, 1320, 1020), fill=INK3, width=3)
for y in (375, 675, 975):
    arrow(1320, y, 1360, y)

# Footer
d.line((80, 1210, W - 80, 1210), fill=LINE2, width=2)
text_at("现状：打标字段没有形成可用标签面", 80, 1232, F_COL, RED)
text_at("当前可见 18 篇：tags 0 / people 0 / events 0 / periods 0。", 80, 1272, F_FOOT, INK2)
text_at("根因不是索引坏了：生成笔记只写 类型/来源/作者 scalar；类型/、领域/、来源/ tag 没有生成；订阅 tags 为空；events 不进 attrList。", 80, 1306, F_FOOT, INK2)
text_at("当前可见集连 UP 派生标签也没有；有 B 站作者时才会多一个 UP 标签。检索仍能搜到正文。", 80, 1340, F_FOOT, INK3)

out = Path("docs/diagrams/tagging-flow-2026-11-23.png")
out.parent.mkdir(parents=True, exist_ok=True)
img.save(out)
print(out, img.size)
