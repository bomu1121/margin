# Trace agent probe

- note: 把主机游戏原生搬到 PC 和手机的开源工具 (yn-806731c8e380e10da6119eee)
- status: success / finish
- summary: 主语无法定位：笔记正文与 frontmatter 均未给出项目名称、仓库地址或来源链接，只有「一个开源项目」的泛指；共核查 7 条原子主张（4.1k stars / 两周、原生移植非模拟器、Windows+macOS+Linux+安卓四平台、Windows/Mac 已编译中文汉化版、手柄自动适配、2K 稳定 60 帧、画质帧率优于模拟器），因无法确认检索到的任何来源在讲同一主语，7 条全部记为「未找到外部出处」（检索到的 sf2recomp、ZeldaWWHDRecomp、OldBits Hub、AnyPS5 等均为其他具体项目，不能挪用为其证据）；有 3 条仅与事实库中「这类原生移植项目」的类别性旧事实相近，已按新事实单独登记。
- claims: 7
- searches: 3, fetches: 0
- duration: 20.2s
- usage: 44733 in / 3091 out / ¥0.0236

## Claims

- **未找到外部出处** 这个开源项目上线两周就拿到 4.1k stars
- **未找到外部出处** 这个开源项目把各类主机、掌机游戏原生地搬到电脑上和手机上玩，不是模拟器
- **未找到外部出处** 这个项目能装在 Windows、macOS、Linux、安卓手机全部四个平台上
- **未找到外部出处** 本期提供的这个项目的 Windows、Mac 版本是已经编译好的中文汉化版
- **未找到外部出处** 这个项目的手柄会自动适配，不用手动配
- **未找到外部出处** 用这个项目玩，很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧
- **未找到外部出处** 这个项目在画质、帧率上都比模拟器更好

## Trajectory

1. find_facts({"claim":"某开源项目上线两周就拿到 4.1k stars"})
   → {"candidates":[{"confidence":0.8571428571428571,"id":"fact_2721492af756e635a9ff542a","relation":"same_as","status":"unsupported","text":"素材里说该开源项目上线两周就拿到 4.1k stars"}]}
2. find_facts({"claim":"把主机、掌机游戏原生地搬到电脑和手机上玩的开源项目"})
   → {"candidates":[{"confidence":0.3448275862068966,"id":"fact_e3df25933908d33990547864","relation":"related","status":"verified","text":"近期集中冒出一批把主机游戏原生移植到其他平台的开源项目，并在短期内聚起关注"}]}
3. find_facts({"claim":"该开源项目支持 Windows、macOS、Linux、安卓手机四个平台"})
   → {"candidates":[{"confidence":0.6176470588235294,"id":"fact_61bd467f433d40659cbb4ed9","relation":"related","status":"verified","text":"这类原生移植项目可以同时覆盖 Windows、macOS、Linux 和安卓手机多个平台"}]}
4. find_facts({"claim":"原生移植方式在画质和帧率上优于模拟器，很多游戏能开 2K 稳定 60 帧"})
   → {"candidates":[{"confidence":0.4745762711864407,"id":"fact_1c71a0a8876e88f9d26f4e38","relation":"related","status":"verified","text":"原生重编译方式在画质和帧率上优于模拟器，能解决模拟器锁帧、掉帧的问题"},{"confidence":0.34615384615384615,"id":"fact_a5184c26e1d7b312413b524b","relation":"related","status":"unsupported","text":"很多游戏能直接开到 2K 分辨率，甚至还能稳定 60
6. search_web({"limit":6,"query":"开源项目 原生移植 主机游戏 电脑 手机 不是模拟器 4.1k stars"})
   → {"query":"开源项目 原生移植 主机游戏 电脑 手机 不是模拟器 4.1k stars","results":[{"snippet":"## 不靠模拟器！三款《战争机器》原生移植PC\n\n爱好者项目COGBox用静态代码重编译，把Xbox 360时代的三款《战争机器》原生带到Windows，还支持5K、超分、高帧率和超宽屏。老游戏获得现代PC体验，但民间移植的合规与稳定性仍需观望。\n...\n## 从Xbox 360到Windows，不是模拟器\n\n海外消息显示，爱好者开发项目 COGBox 已成功将《战争机器2》《战争机器3》和《战争机器：审判》进行静态代码重编译，让三款Xbox 360作品能够在Windows系统下
7. search_web({"limit":6,"query":"\"原生移植\" 开源 两周 4.1k stars 掌机游戏 PC 安卓"})
   → {"query":"\"原生移植\" 开源 两周 4.1k stars 掌机游戏 PC 安卓","results":[{"snippet":"# 49元掌机不靠模拟器玩忍龙黑？原生移植是咋回事\n\n源自196位全网作者\n\n15:27\n\n这两天，B站冒出不少视频，晒的是\"49元掌机流畅运行Xbox《忍者龙剑传黑之章》，3326等Linux机型玩PS2游戏也有新进展\"。画面里的机器都是R36S、H700、迷你龙、X35S这类百元级Linux掌机，那台135块的X35S把《战神1+2》和《生化危机4》同时跑了起来，而且全程没用模拟器。＋2\n\n让它们跑起来的是一条叫\"原生移植\"的路线：绕开模拟器，把游戏本身做成掌机可以
9. search_web({"limit":6,"query":"开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧"})
   → {"query":"开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧","results":[{"snippet":"# GitHub - bipinkrish/sf2recomp: Static recompilation of Shadow Fight 2 (Switch) to native Linux, Windows, macOS and Andorid · GitHub\n\n# SF2Recomp\n\nShadow Fight 2 for Nintendo Switch, running as a native game on your device
11. report_claim({"claim":"这个开源项目上线两周就拿到 4.1k stars","fact_id":"fact_2721492af756e635a9ff542a","queries":["开源项目 原生移植 主机游戏 电脑 手机 不是模拟器 4.1k stars","\"原生移植\" 开源 两周 4.1k stars 掌机游戏 PC 安卓"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_2721492af756e635a9ff542a","ok":true,"reported":1,"status":"unsupported"}
12. report_claim({"claim":"这个开源项目把各类主机、掌机游戏原生地搬到电脑上和手机上玩，不是模拟器","queries":["开源项目 原生移植 主机游戏 电脑 手机 不是模拟器 4.1k stars"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_90d2cce09c4a27dcbbc2b7e5","ok":true,"reported":2,"status":"unsupported"}
13. report_claim({"claim":"这个项目能装在 Windows、macOS、Linux、安卓手机全部四个平台上","queries":["开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_681de23a13b9c2d69cc09b9f","ok":true,"reported":3,"status":"unsupported"}
14. report_claim({"claim":"本期提供的这个项目的 Windows、Mac 版本是已经编译好的中文汉化版","queries":["开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_34bf3b3cf41f07d55bfdb28a","ok":true,"reported":4,"status":"unsupported"}
16. report_claim({"claim":"这个项目的手柄会自动适配，不用手动配","queries":["开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_8d3e0a3a08cf77edee27995f","ok":true,"reported":5,"status":"unsupported"}
17. report_claim({"claim":"用这个项目玩，很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧","queries":["\"原生移植\" 开源 两周 4.1k stars 掌机游戏 PC 安卓","开源 静态重编译 项目 Windows macOS Linux 安卓 手机 原生 汉化版 手柄自动适配 2K 60帧"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_1d4f33eefd4c5718f63ca13e","ok":true,"reported":6,"status":"unsupported"}
18. report_claim({"claim":"这个项目在画质、帧率上都比模拟器更好","queries":["开源项目 原生移植 主机游戏 电脑 手机 不是模拟器 4.1k stars"],"relation":"context","verdict":"未找到外部出处"})
   → {"factId":"fact_da6e48713457d9681b598ab0","ok":true,"reported":7,"status":"unsupported"}
20. finish({"summary":"主语无法定位：笔记正文与 frontmatter 均未给出项目名称、仓库地址或来源链接，只有「一个开源项目」的泛指；共核查 7 条原子主张（4.1k stars / 两周、原生移植非模拟器、Windows+macOS+Linux+安卓四平台、Windows/Mac 已编译中文汉化版、手柄自动适配、2K 稳定 60 帧、画质帧率优于模拟器），因无法确认检索到的任何来源在讲同一主语，7 条全部记为「未找到外部出处」（检索到的 sf2recomp、Zelda)
   → {"ok":true}

## Facts for this note

- [unsupported] 这个项目在画质、帧率上都比模拟器更好
- [unsupported] 用这个项目玩，很多游戏能直接开到 2K 分辨率，甚至还能稳定 60 帧
- [unsupported] 这个项目的手柄会自动适配，不用手动配
- [unsupported] 本期提供的这个项目的 Windows、Mac 版本是已经编译好的中文汉化版
- [unsupported] 这个项目能装在 Windows、macOS、Linux、安卓手机全部四个平台上
- [unsupported] 这个开源项目把各类主机、掌机游戏原生地搬到电脑上和手机上玩，不是模拟器
- [unsupported] 素材里说该开源项目上线两周就拿到 4.1k stars
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