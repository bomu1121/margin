from PIL import Image, ImageDraw, ImageFont
from pathlib import Path

W, H = 2000, 1500
BG = "#f4f1e8"
SURFACE = "#fffdf8"
RAIL = "#ece8de"
INK = "#1b1813"
INK2 = "#514a3f"
INK3 = "#686055"
LINE = "#e2dccf"
LINE2 = "#c9c0ae"
BLUE = "#1f4fd8"
BLUE_SOFT = "#eef2ff"
BLUE_LINE = "#c8d5f7"
GOLD = "#8a5d0c"
GOLD_SOFT = "#fbf3e2"
GREEN = "#187a4d"
GREEN_SOFT = "#eaf6ef"
RED = "#b4232f"
RED_SOFT = "#fbecee"

SANS = "C:/Windows/Fonts/msyh.ttc"
SANS_B = "C:/Windows/Fonts/msyhbd.ttc"
SERIF = "C:/Windows/Fonts/NotoSerifSC-VF.ttf"


def font(path, size):
    return ImageFont.truetype(path, size)


F_TITLE = font(SERIF, 62)
F_SUB = font(SANS, 25)
F_SECTION = font(SANS_B, 24)
F_STEP = font(SANS_B, 20)
F_STEP_SUB = font(SANS, 17)
F_BOX_TITLE = font(SANS_B, 26)
F_BOX = font(SANS, 18)
F_SMALL = font(SANS, 15)
F_DECISION = font(SANS_B, 30)
F_DECISION_SUB = font(SANS, 18)
F_CARD_TITLE = font(SANS_B, 25)
F_CARD = font(SANS, 18)
F_FOOT = font(SANS, 18)

img = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(img)


def rounded(box, radius, fill, outline, width=2):
    d.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def arrow(x1, y1, x2, y2, color=INK3, width=2):
    d.line((x1, y1, x2, y2), fill=color, width=width)
    if x2 > x1:
        d.polygon([(x2, y2), (x2 - 10, y2 - 6), (x2 - 10, y2 + 6)], fill=color)
    elif x2 < x1:
        d.polygon([(x2, y2), (x2 + 10, y2 - 6), (x2 + 10, y2 + 6)], fill=color)
    else:
        d.polygon([(x2, y2), (x2 - 6, y2 - 10), (x2 + 6, y2 - 10)], fill=color)


def centered(text, box, fnt, fill):
    x1, y1, x2, y2 = box
    d.multiline_text(((x1 + x2) / 2, (y1 + y2) / 2), text, font=fnt, fill=fill, anchor="mm", align="center", spacing=5)


def text_at(text, x, y, fnt, fill):
    d.multiline_text((x, y), text, font=fnt, fill=fill, spacing=5)


# Header
text_at("出处系统的下一步", 80, 54, F_TITLE, INK)
text_at("先把「AI 找来源」变可靠，再决定要不要上本地全文库", 84, 142, F_SUB, INK2)
d.line((80, 190, W - 80, 190), fill=LINE2, width=2)

# Pipeline section
text_at("现在：AI 找来源的 7 步", 80, 214, F_SECTION, BLUE)
steps = [
    ("1", "候选无链接", "有书名 / 篇名 / 原句"),
    ("2", "联网检索", "最多 5 条，试 3 个"),
    ("3", "来源分级", "原文库优先"),
    ("4", "抓正文", "SSRF / 超时 / GBK"),
    ("5", "AI 摘原文", "只从正文里找"),
    ("6", "逐字校验", "脏注记拒绝"),
    ("7", "写入出处", "found / candidate"),
]
left, right = 80, W - 80
gap = 14
box_w = (right - left - gap * 6) // 7
box_h = 108
y = 260
for i, (num, title, sub) in enumerate(steps):
    x = left + i * (box_w + gap)
    highlight = i == 2
    fill = BLUE_SOFT if highlight else SURFACE
    outline = BLUE_LINE if highlight else LINE
    rounded((x, y, x + box_w, y + box_h), 10, fill, outline, 2)
    d.ellipse((x + 14, y + 14, x + 40, y + 40), fill=BLUE if highlight else RAIL, outline=BLUE if highlight else LINE2)
    centered(num, (x + 14, y + 14, x + 40, y + 40), F_SMALL, SURFACE if highlight else INK2)
    text_at(title, x + 48, y + 15, F_STEP, BLUE if highlight else INK)
    text_at(sub, x + 14, y + 63, F_STEP_SUB, INK3)
    if i < 6:
        arrow(x + box_w + 2, y + box_h / 2, x + box_w + gap - 3, y + box_h / 2, LINE2, 2)

# Quality section
text_at("来源分级怎么选", 80, 408, F_SECTION, INK)
quality = [
    ("原文库", "识典 / ctext / Wikisource / 南博", "优先使用", GREEN, GREEN_SOFT),
    ("二手页面", "学术库 / 新闻 / 百科", "可用，但明确标注「二手」", GOLD, GOLD_SOFT),
    ("目录页", "NLC / HathiTrust / 一史馆", "跳过，不当来源", RED, RED_SOFT),
]
qy = 452
for i, (title, examples, action, color, soft) in enumerate(quality):
    x = 80 + i * 620
    w = 580
    rounded((x, qy, x + w, qy + 118), 10, soft, color, 2)
    text_at(title, x + 18, qy + 14, F_BOX_TITLE, color)
    text_at(examples, x + 18, qy + 55, F_BOX, INK2)
    text_at(action, x + 18, qy + 86, F_SMALL, INK3)

# Decision
dy = 620
rounded((330, dy, 1670, dy + 118), 12, SURFACE, BLUE_LINE, 2)
text_at("验收：5 个查询，分级后 top-1 原文库 ≥ 4/5？", 370, dy + 18, F_DECISION, INK)
text_at("达不到，就不要硬做网络检索；先补链接或转本地全文库。", 370, dy + 73, F_DECISION_SUB, INK3)
arrow(1000, dy + 118, 600, 800, LINE2, 3)
arrow(1000, dy + 118, 1400, 800, LINE2, 3)
rounded((120, 800, 880, 918), 10, GREEN_SOFT, GREEN, 2)
text_at("是 → 继续优化网络检索", 150, 820, F_BOX_TITLE, GREEN)
text_at("实测分级后 5/5；原始 top-1 3/5，端到端补原文 4/5。", 150, 862, F_BOX, INK2)
rounded((1120, 800, 1880, 918), 10, GOLD_SOFT, GOLD, 2)
text_at("否 → 本地全文库 / 要求补链接", 1150, 820, F_BOX_TITLE, GOLD)
text_at("频繁落到二手页，就先导 1–2 本正史，别一上来做全库。", 1150, 862, F_BOX, INK2)

# Roadmap
text_at("开发顺序", 80, 968, F_SECTION, INK)
cards = [
    ("P0 已完成 · 1–2 天", BLUE, BLUE_SOFT, [
        "5 个查询的来源分级验收",
        "批量补原文：取消 + 预算 + 失败原因",
        "出处列表展开完整原文",
        "安装包实际跑一遍",
    ], "先拿数字，再动代码", "5 行查询表 · 0 假原文 · 安装版可用"),
    ("P1 完成 · 本地库已跑通", GOLD, GOLD_SOFT, [
        "本地 4/5 · 总成功 5/5",
        "3 本 / 36.2 万段",
        "总成本 −61.8%",
    ], "只 3 本，没做全量", "5/5 成功 · 4/5 本地 · 总成本 −61.8%"),
    ("P2 下一步", INK3, RAIL, [
        "二十四史 / 清史稿全量",
        "版本 / 点校本排序",
        "本地检索建索引",
    ], "本地库稳定后再扩", "全量库可用后再做"),
]
cy = 1014
for i, (title, color, soft, bullets, note, acceptance) in enumerate(cards):
    x = 80 + i * 620
    w = 580
    rounded((x, cy, x + w, cy + 356), 12, soft, color, 2)
    text_at(title, x + 20, cy + 18, F_CARD_TITLE, color)
    yy = cy + 66
    for bullet in bullets:
        d.ellipse((x + 22, yy + 8, x + 30, yy + 16), fill=color)
        text_at(bullet, x + 40, yy, F_CARD, INK2)
        yy += 54
    text_at(note, x + 20, cy + 250, F_SMALL, color)
    d.line((x + 20, cy + 284, x + w - 20, cy + 284), fill=color, width=1)
    text_at("验收：" + acceptance, x + 20, cy + 300, F_SMALL, INK3)

# Footer
d.line((80, 1390, W - 80, 1390), fill=LINE2, width=2)
text_at("当前：25 条出处 / 11 个书名分组 / 230 tests / 安装包 9,046,219 B", 80, 1410, F_FOOT, INK3)
text_at("当前瓶颈：只 3 本本地库 · 2 篇 AI 成稿 8/8 待查 · 全文搜索约 0.9s", 80, 1442, F_FOOT, GOLD)
text_at("不做：事实/判定/合并/拆分/来源等级 UI · ctext 网页抓取 · 自动改正文", 80, 1474, F_FOOT, RED)

out = Path("docs/diagrams/plan-2026-11-22.png")
out.parent.mkdir(parents=True, exist_ok=True)
img.save(out, "PNG")
print(out, img.size)
