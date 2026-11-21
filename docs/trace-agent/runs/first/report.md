# Trace agent probe

- note: 把主机游戏原生搬到 PC 和手机的开源工具 (yn-806731c8e380e10da6119eee)
- status: success / finish
- summary: 共核查 8 条主张：4 条有外部证据（原生重编译≠模拟器、画质/帧率优于模拟器、可覆盖 Windows/macOS/Linux/安卓四平台、此类项目近期集中爆发），4 条未找到外部出处（"两周 4.1k stars"、"很多游戏 2K/稳定 60 帧"、"手柄自动适配无需手动配置"、"提供 Windows/Mac 已编译中文汉化版"）；笔记未给项目名，故无法把具体数字归到唯一仓库。
- claims: 8
- searches: 8, fetches: 2
- duration: 63.0s
- usage: 227366 in / 8890 out / ¥0.0669

## Claims

- **未找到外部出处** 素材里说该开源项目上线两周就拿到 4.1k stars
- **外部可印证** 这类项目走的是原生重编译/原生移植路线，不依赖模拟器，而是把游戏代码编译成目标平台可直接执行的程序
  - https://blog.ocam.live/archives/6018
  - “專案不是單純的模擬器，而是將遊戲的 PowerPC 程式碼預先轉換成 C，再重新編譯成目標平台可以直接執行的程式。”
- **外部可印证** 原生重编译方式在画质和帧率上优于模拟器，能解决模拟器锁帧、掉帧的问题
  - https://3g.ali213.net/news/html/1051551.html
  - “项目实测最高支持4K分辨率、60帧稳定运行，解决了长期困扰模拟器玩家的过场 动画 锁30帧痛点”
- **外部可印证** 这类原生移植项目可以同时覆盖 Windows、macOS、Linux 和安卓手机多个平台
  - https://blog.ocam.live/archives/6018
  - “目前支援 Windows、Linux、Apple Silicon macOS，以及 Android ARM64 平台”
- **未找到外部出处** 很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧
- **未找到外部出处** 手柄会自动适配，不用手动配置
- **未找到外部出处** 本期提供 Windows、Mac 版本是已经编译好的中文汉化版
- **外部可印证** 近期集中冒出一批把主机游戏原生移植到其他平台的开源项目，并在短期内聚起关注
  - https://gitcode.com/hot-concept/game-porting#1
  - “玩家对原生体验有需求，重编译工具链成熟后，单个游戏的移植能在短期内聚起关注并带动同类实现。”

## Trajectory

0. model: I'll start by checking the fact bank for existing facts on the main verifiable claims.
1. find_facts({"claim":"一个开源项目上线两周就拿到 4.1k stars"})
   → {"candidates":[]}
2. find_facts({"claim":"开源工具把主机游戏以原生移植方式运行在 PC 和手机上，不是模拟器"})
   → {"candidates":[]}
4. search_web({"limit":6,"query":"开源项目 两周 4.1k stars 主机游戏 原生移植 不是模拟器 PC 手机"})
   → {"query":"开源项目 两周 4.1k stars 主机游戏 原生移植 不是模拟器 PC 手机","results":[{"snippet":"# AnyPS5 是什么？2.1 万星开源项目如何把 PS5 ELF 直接变成 PC 原生程序\n\n## 引言\n\nAnyPS5 是 2026 年 10 月登上 GitHub Trending 的一个开源项目。它做的事情很直接：把 PS5 的 ELF 可执行文件重新链接成 Linux、Windows、macOS 上的原生二进制程序，单周进账 21365 颗星，把第二名甩开约 3 倍。\n\n这个数字把开源游戏圈上一次出现同等量级的热度，要追到 PS3 模拟器 RPCS3 跑出可玩游
5. search_web({"limit":6,"query":"开源项目 把主机游戏 原生移植 到 PC 和手机 不是模拟器 github stars"})
   → {"query":"开源项目 把主机游戏 原生移植 到 PC 和手机 不是模拟器 github stars","results":[{"snippet":"# 游戏移植\n引爆\n把主机游戏重编译或搬上其它平台的项目\n...\n## 方向介绍\n...\n把主机游戏搬到别的平台运行的项目集合：有的把原版代码翻译成 C 再编译成本机程序，有的做成桌面原生程序，有的做成手机、平板与 Mac 上的原生应用，也有在成熟模拟器分支上继续修兼容的路线；多数需要用户自备游戏副本。\n\n为什么现在火\n\n不依赖模拟器、直接重编译成原生程序是这批项目共同的路线；玩家对原生体验有需求，重编译工具链成熟后，单个游戏的移植能在短期内聚起关注并带动同类
7. search_web({"limit":6,"query":"开源项目 支持 Windows macOS Linux 安卓 原生运行 Switch 游戏 4.1k stars 两周"})
   → {"query":"开源项目 支持 Windows macOS Linux 安卓 原生运行 Switch 游戏 4.1k stars 两周","results":[{"snippet":"shirai91\nnx2android\n...\nA compatibility layer for porting Switch games to Android, one game at a time\n\n1.7k\n...\n874/wk +124 yesterday \nactive\n\nAdded Nov 6, 2026\n...\nLASRecomp\n...\nLAS Recomp is a static recompilat
8. search_web({"limit":6,"query":"\"原生移植\" 开源 项目 主机 掌机 游戏 手机 电脑 编译 中文汉化版 手柄自动适配 2K 60帧"})
   → {"query":"\"原生移植\" 开源 项目 主机 掌机 游戏 手机 电脑 编译 中文汉化版 手柄自动适配 2K 60帧","results":[{"snippet":"# OutRunners 原生多平台移植项目\n\nOutRunners 是基于已验证 C 反编译结果的原生移植项目，支持桌面系统、Nintendo Switch、Android 与 Anbernic H700 掌机，并同时提供街机版和家用主机版体验。\n\n数据来源 GitHub，内容可能由 AI 辅助生成，欢迎纠错。\n\n项目定位\n\n如果目标是合法地在多种桌面或便携设备上体验 OutRunners，并且能够自行准备兼容的 ROM 集，这个项目值得优先评
10. search_web({"limit":6,"query":"\"4.1k stars\" 开源 游戏 移植 项目 GitHub 两周"})
   → {"query":"\"4.1k stars\" 开源 游戏 移植 项目 GitHub 两周","results":[{"snippet":"# 在电脑和手机上玩 Switch 游戏？这 5 个GitHub 项目值得看看。\n\n01\n\n在苹果电脑上继续海拉鲁的冒险\n\n相信很多人都听过一句话：塞尔达是天。\n\n就是这个被称为天的游戏，最近很多开源项目都把它们移植到其他设备上了。\n\nMacNativeGames 就是把《塞尔达传说：王国之泪》带到了搭载苹果芯片的 Mac 上，目前已经有 4.8K 多个 Star。\n\n准备好自己的游戏文件后，你可以用它构建一个 Mac 应用，在电脑上继续探索海拉鲁。\n\n《旷野之息》
11. search_web({"limit":6,"query":"静态重编译 原生运行 支持 Windows macOS Linux Android 手势 手柄 分辨率 60帧 模拟器 画质 对比"})
   → {"query":"静态重编译 原生运行 支持 Windows macOS Linux Android 手势 手柄 分辨率 60帧 模拟器 画质 对比","results":[{"snippet":"# WiiU《薩爾達傳說 風之律動 HD》原生 PC 移植版開放下載！支援 Windows、Linux、macOS 與 Android\n\nWiiU《薩爾達傳說 風之律動 HD》原生 PC 移植版 Zelda Wind Waker HD Recomp 是《The Legend of Zelda: The Wind Waker HD》的原生重編譯（Static Recompilation）專案，將 Wii U 版本的 PowerPC 遊
13. search_web({"limit":6,"query":"游戏原生移植 开源项目 \"两周\" stars 增长 报道 2026年11月 GitHub 趋势"})
   → {"query":"游戏原生移植 开源项目 \"两周\" stars 增长 报道 2026年11月 GitHub 趋势","results":[{"snippet":"# Craft Anything:纯Rust逆向引擎带320个工具,AI代理一句话复刻网站和游戏\n\n2026年11月17日01:09:28评论7 views字数 2837阅读9分27秒阅读模式\n\nCraftAnything是近期GitHub增长最快的项目之一，建仓两周即获近3万星。它用纯Rust重写逆向引擎，内置320个工具，覆盖反汇编、反编译、调用图等功能，支持原生程序、.NET、Java、Android、Electron、游戏文件等多种目标。通过AI代理驱
14. search_web({"limit":6,"query":"原生移植 项目 已编译 中文汉化版 Windows Mac 打包下载 Switch 游戏"})
   → {"query":"原生移植 项目 已编译 中文汉化版 Windows Mac 打包下载 Switch 游戏","results":[{"snippet":"# 宝可梦：晶灿钻石/Pokémon Brilliant Diamond\n\n游戏来源：网友分享 更新时间：2026-11-15\n\n### 版本介绍\n\nv0.1.6|容量5.53GB|官方简体中文|支持键盘.鼠标.手柄\n\n### 游戏介绍\n\n《宝可梦：晶灿钻石》是由 ILCA 开发、任天堂与宝可梦公司发行的角色扮演游戏，于2021年11月19日登陆 Nintendo Switch 平台。本作是经典游戏《宝可梦：钻石》的高清重制版，以神奥地区为舞台，在保留原作经
16. fetch_source({"url":"https://blog.ocam.live/archives/6018"})
   → {"text":"跳至主要內容 找不到符合條件的結果 首頁 IT 日常 科技新聞 電玩模擬器 程式設言 生活雜記 搜尋 首頁 IT 日常 科技新聞 電玩模擬器 程式設言 生活雜記 聯絡我們 搜尋 選單 WiiU《薩爾達傳說 風之律動 HD》原生 PC 移植版開放下載！支援 Windows、Linux、macOS 與 Android blog.ocam.live 2026-10-08 電玩模擬器 首頁 電玩模擬器 WiiU《薩爾達傳說 風之律動 HD》原生 PC 移植版開放下載！支援 Windows、Linux、macOS 與 Android WiiU《薩爾達傳說 風之律動 HD》原生 PC 移植版 Zelda Wind Waker 
17. fetch_source({"url":"https://3g.ali213.net/news/html/1051551.html"})
   → {"text":"游侠网 攻略资讯一手掌握 首页 资讯 专题 攻略 视频 下载 手游 商城 简介 攻略 评分 下载 新闻 最高4K60帧！《猎天使魔女3》重编译实现PC原生运行 2026-11-03 20:47:17 来源：互联网 0 近期海外开发者Zexyen推出《猎天使魔女3》静态重编译项目Witch-Time，在玩家社群引发热议。该方案和传统 Switch 模拟 器完全不同，并非虚拟主机环境，而是将游戏ARM架构机器码转译为C代码，再编译为Windows、Linux可执行程序，依托Vulkan图形API直接运行。 项目实测最高支持4K分辨率、60帧稳定运行，解决了长期困扰模拟器玩家的过场 动画 锁30帧痛点，还开放90/120
19. report_claim({"claim":"素材里说该开源项目上线两周就拿到 4.1k stars","queries":["开源项目 两周 4.1k stars 主机游戏 原生移植 不是模拟器 PC 手机","\"4.1k stars\" 开源 游戏 移植 项目 GitHub 两周","游戏原生移植 开源项目 \"两周\" stars 增长 报道 2026年11月 GitHub 趋势"],"verdict":"未找到外部出处"})
   → {"factId":"fact_2721492af756e635a9ff542a","ok":true,"reported":1,"status":"unsupported"}
20. report_claim({"claim":"这类项目走的是原生重编译/原生移植路线，不依赖模拟器，而是把游戏代码编译成目标平台可直接执行的程序","queries":["开源项目 把主机游戏 原生移植 到 PC 和手机 不是模拟器 github stars","静态重编译 原生运行 支持 Windows macOS Linux Android 手柄 分辨率 60帧"],"quote":"專案不是單純的模擬器，而是將遊戲的 PowerPC 程式碼預先轉換成 C，再重新編譯成目標平台可以直接執行的程式。)
   → {"factId":"fact_2de8dbf6dcf3094c359da713","ok":true,"reported":2,"status":"verified"}
22. report_claim({"claim":"原生重编译方式在画质和帧率上优于模拟器，能解决模拟器锁帧、掉帧的问题","queries":["静态重编译 原生运行 画质 60帧 模拟器 对比"],"quote":"项目实测最高支持4K分辨率、60帧稳定运行，解决了长期困扰模拟器玩家的过场 动画 锁30帧痛点","relation":"supports","source_url":"https://3g.ali213.net/news/html/1051551.html","verdict":"外部可印)
   → {"factId":"fact_1c71a0a8876e88f9d26f4e38","ok":true,"reported":3,"status":"verified"}
23. report_claim({"claim":"这类原生移植项目可以同时覆盖 Windows、macOS、Linux 和安卓手机多个平台","queries":["静态重编译 原生运行 支持 Windows macOS Linux Android 手柄 分辨率 60帧"],"quote":"目前支援 Windows、Linux、Apple Silicon macOS，以及 Android ARM64 平台","relation":"context","source_url":"https://blog.)
   → {"factId":"fact_61bd467f433d40659cbb4ed9","ok":true,"reported":4,"status":"verified"}
25. report_claim({"claim":"很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧","queries":["静态重编译 原生运行 支持 Windows macOS Linux Android 手柄 分辨率 60帧","静态重编译 原生运行 画质 60帧 模拟器 对比"],"verdict":"未找到外部出处"})
   → {"factId":"fact_a5184c26e1d7b312413b524b","ok":true,"reported":5,"status":"unsupported"}
26. report_claim({"claim":"手柄会自动适配，不用手动配置","queries":["静态重编译 原生运行 支持 Windows macOS Linux Android 手柄 分辨率 60帧"],"verdict":"未找到外部出处"})
   → {"factId":"fact_21d7498ed91a384b204f1a67","ok":true,"reported":6,"status":"unsupported"}
27. report_claim({"claim":"本期提供 Windows、Mac 版本是已经编译好的中文汉化版","queries":["原生移植 项目 已编译 中文汉化版 Windows Mac 打包下载 Switch 游戏"],"verdict":"未找到外部出处"})
   → {"factId":"fact_81108cdbe2d13dd1a6e142b3","ok":true,"reported":7,"status":"unsupported"}
29. report_claim({"claim":"近期集中冒出一批把主机游戏原生移植到其他平台的开源项目，并在短期内聚起关注","queries":["开源项目 把主机游戏 原生移植 到 PC 和手机 不是模拟器 github stars"],"quote":"玩家对原生体验有需求，重编译工具链成熟后，单个游戏的移植能在短期内聚起关注并带动同类实现。","relation":"context","source_url":"https://gitcode.com/hot-concept/game-porti)
   → {"error":"source_url 不是本轮 search_web / fetch_source 见过的 URL"}
31. report_claim({"claim":"近期集中冒出一批把主机游戏原生移植到其他平台的开源项目，并在短期内聚起关注","queries":["开源项目 把主机游戏 原生移植 到 PC 和手机 不是模拟器 github stars"],"quote":"玩家对原生体验有需求，重编译工具链成熟后，单个游戏的移植能在短期内聚起关注并带动同类实现。","relation":"context","source_url":"https://gitcode.com/hot-concept/game-porti)
   → {"factId":"fact_e3df25933908d33990547864","ok":true,"reported":8,"status":"verified"}
33. finish({"summary":"共核查 8 条主张：4 条有外部证据（原生重编译≠模拟器、画质/帧率优于模拟器、可覆盖 Windows/macOS/Linux/安卓四平台、此类项目近期集中爆发），4 条未找到外部出处（\"两周 4.1k stars\"、\"很多游戏 2K/稳定 60 帧\"、\"手柄自动适配无需手动配置\"、\"提供 Windows/Mac 已编译中文汉化版\"）；笔记未给项目名，故无法把具体数字归到唯一仓库。"})
   → {"ok":true}

## Facts for this note

- [verified] 近期集中冒出一批把主机游戏原生移植到其他平台的开源项目，并在短期内聚起关注
  - https://gitcode.com/hot-concept/game-porting#1
    “玩家对原生体验有需求，重编译工具链成熟后，单个游戏的移植能在短期内聚起关注并带动同类实现。”
- [unsupported] 本期提供 Windows、Mac 版本是已经编译好的中文汉化版
- [unsupported] 手柄会自动适配，不用手动配置
- [unsupported] 很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧
- [verified] 这类原生移植项目可以同时覆盖 Windows、macOS、Linux 和安卓手机多个平台
  - https://blog.ocam.live/archives/6018
    “目前支援 Windows、Linux、Apple Silicon macOS，以及 Android ARM64 平台”
- [verified] 原生重编译方式在画质和帧率上优于模拟器，能解决模拟器锁帧、掉帧的问题
  - https://3g.ali213.net/news/html/1051551.html
    “项目实测最高支持4K分辨率、60帧稳定运行，解决了长期困扰模拟器玩家的过场 动画 锁30帧痛点”
- [verified] 这类项目走的是原生重编译/原生移植路线，不依赖模拟器，而是把游戏代码编译成目标平台可直接执行的程序
  - https://blog.ocam.live/archives/6018
    “專案不是單純的模擬器，而是將遊戲的 PowerPC 程式碼預先轉換成 C，再重新編譯成目標平台可以直接執行的程式。”
- [unsupported] 素材里说该开源项目上线两周就拿到 4.1k stars