# Trace agent probe

- note: 胡塞一发导弹打中沙特 PIF 总部：大楼没塌，被打穿的是沙特转型的账本 (yn-bc6ca5b7f3c63365c9cee6c8)
- status: success / finish
- summary: 共核查8条主张：3条外部可印证（PIF大楼外立面/窗户受损、楼高385米、线性城市2025年9月16日暂停施工），3条存在争议（「10月19日」应为2026年11月19日；导弹被联军拦截、碎片造成损坏而非直接「击中」；「140多家企业/75家跨国区域总部」实指整个阿卜杜拉国王金融区而非楼内），1条仅非权威来源（6000亿国防开支，媒体口径为约830亿美元、全球第八，SIPRI原文未能校验），1条未找到外部出处（「7到8个楼层」的具体层数）。受预算限制未核查：2700名驻沙美军、穆哈港9天失守、也门70%山地、1万无人机对2000万拦截弹、旅游1.4万亿与奇迪亚560亿、80万亿线性城市预算等。
- claims: 8
- searches: 7, fetches: 6
- duration: 114.6s
- usage: 307222 in / 14296 out / ¥0.0893

## Claims

- **存在争议** 胡塞为报复沙特连日轰炸，向利雅得发射导弹，击中沙特公共投资基金（PIF）总部大楼
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
  - “Coalition spokesman Turki al-Malki said an initial assessment indicated that debris from the interception had caused material damage, adding that authorities were examining the impact.”
- **存在争议** 这次对利雅得PIF大楼的打击发生在10月19日
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
  - “Air Strike Hits Saudi Arabia’s Sovereign Wealth Fund, Second-Tallest Skyscraper November 19, 2026”
- **外部可印证** 袭击造成PIF总部大楼多个楼层窗户碎裂、部分金属外立面受损
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
  - “The 80-storey skyscraper, which houses the headquarters of the kingdom’s sovereign wealth fund, sustained damage to its facade, with several windows reportedly blown out.”
- **未找到外部出处** 这次打击造成PIF大楼7到8个楼层的窗户碎裂
- **外部可印证** 沙特公共投资基金（PIF）总部大楼总高385米
  - https://executivecentre.sa/blog/pif-tower-in-riyadh-kafd
  - “Standing 385 metres above Riyadh’s surrounding cityscape, the PIF Tower, formerly known as the CMA Tower, is like the centerpiece of the King Abdullah Financial District.”
- **存在争议** PIF大楼里面聚集140多家入驻企业和超过75家跨国公司的区域总部
  - https://globalbusinessoutlook.com/banking-and-finance/saudi-vision-riyadh-transforms-into-global-command-centre/
  - “Hosting more than 140 companies, from startups to MNCs, the district offers world-class flexible workspace through facilities like The Executive Centre, essential for firms in rapid growth or market-testing phases.”
- **外部可印证** 未来之城核心的线性城市（THE LINE）于2025年9月16日暂停施工
  - https://www.archdaily.com/zh/1040858/wei-yu-shi-zi-lu-kou-de-the-line-xiang-mu-zhong-shen-neom-wu-tuo-bang-shi-cheng-shi-yuan-jing
  - “2025年9月16日， 沙特阿拉伯 公共投资基金宣布无限期暂停“THE LINE”的施工，在规划的170公里中，目前仅完成了2.4公里的基础工程。”
- **仅非权威来源** 沙特2025年国防开支6000亿、全球第八
  - https://k.sina.com.cn/article_6362632222_m17b3e101e03301f2ts.html
  - “英国媒体曾经报道，2025年沙特军费开支超过830亿美元，位列全球第八。”

## Trajectory

1. find_facts({"claim":"胡塞武装10月19日向利雅得发射导弹击中沙特公共投资基金（PIF）总部大楼"})
   → {"candidates":[]}
2. search_web({"query":"胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日"})
   → {"query":"胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日","results":[{"snippet":"## 沙特首都第一高楼遭袭\n...\n沙特阿拉伯民防总局19日发表声明说，沙特首都利雅得最高建筑当天遭到也门胡塞武装袭击，但未造成人员伤亡。\n\n声明所附图片显示，遭袭建筑为位于利雅得阿卜杜拉国王金融区的公共投资基金大厦，是利雅得目前最高的建筑。\n\n声明说，袭击造成该建筑多个楼层和周边建筑受损。这起针对民用设施的袭击公然违反国际人道法。\n\n>>>目击者称沙特利雅得金融中心一大楼遭袭受损\n\n当地时间19日，总台记者获悉，有居住在沙特利雅得金融中心的目击者称，沙特主权财富基金旗下的沙特
4. search_web({"query":"PIF Tower 385米 利雅得 阿卜杜拉国王金融区 最高建筑 高度"})
   → {"query":"PIF Tower 385米 利雅得 阿卜杜拉国王金融区 最高建筑 高度","results":[{"snippet":"#### 沙特利雅得 KAFD 区域 PIF 大厦遇袭事件\n\n和平日报，2026年11月19日，（星期四），沙特阿拉伯首都利雅得阿卜杜拉国王金融区（KAFD）发生袭击事件，导致公共投资基金大厦（PIF Tower，高约385米）及周边两栋建筑受损，相关消息于11月20日（星期五）广泛报道。\n...\n- 主要受损建筑： PIF 大厦（PIF Tower），系 KAFD 金融区及利雅得市的最高摩天大楼（高约385米），同时也是沙特公共投资基金总部所在地。\n...\n部分报道中所称的“最高
5. search_web({"query":"沙特 2025年 国防开支 全球第八 军费 排名"})
   → {"query":"沙特 2025年 国防开支 全球第八 军费 排名","results":[{"snippet":"# 军费全球前十，沙特为什么挡不住胡塞武装？\n...\n2026-10-22 16:34:20来源：央广军事\n...\n李悦：据报道，沙特是阿拉伯国家中军费开支最高的国家。根据瑞典斯德哥尔摩国际和平研究所的测算，沙特2025年国防开支约830亿美元，位列世界第八，沙特军队列装了F-15和“台风”战斗机，部署了“爱国者”“萨德”等多层防空系统，还有M1A2主战坦克等一批国际先进装备。","title":"军费全球前十，沙特为什么挡不住胡塞武装？金一南解析 - 军费全球前十，沙特为什么挡不住胡塞武装？金一南解析","
7. fetch_source({"url":"https://malaysiagazette.com/2026/11/20/houthi-attack-damages-riyadhs-tallest-building/"})
   → {"error":"来源返回 HTTP 403 Forbidden"}
8. search_web({"query":"PIF Tower 入驻企业 140家 跨国公司区域总部 75家"})
   → {"query":"PIF Tower 入驻企业 140家 跨国公司区域总部 75家","results":[{"snippet":"Hosting more than 140 companies, from startups to MNCs, the district offers world-class flexible workspace through facilities like The Executive Centre, essential for firms in rapid growth or market-testing phases.\n\nConversations atop the PIF tower fo
10. fetch_source({"url":"https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper"})
   → {"text":"Skip to main content Social Media Links Search Header Navigation News Opinion More topics Politics Sports Entertainment Education Lifestyle #NigeriaDecides Main navigation Videos Photos Documents Air Strike Hits Saudi Arabia’s Sovereign Wealth Fund, Second-Tallest Skyscraper November 19, 2026 News The 80-store
11. fetch_source({"url":"https://executivecentre.sa/blog/pif-tower-in-riyadh-kafd"})
   → {"text":"About us Community Blog EN +966 11 500 6600 Contact Us Contact Us Locations Serviced Offices Virtual Office Coworking Meeting Rooms Event Spaces Managed Offices About us Community Blog EN +966 11 500 6600 Contact Us Contact Us Public Investment Fund (PIF) Tower: Tallest Landmark in KAFD Published Date: Publish
13. fetch_source({"url":"https://military.cnr.cn/jsrp/20261022/t20261022_527849931.shtml"})
   → {"text":"��ҳ > ������ > �������� ����ȫ��ǰʮ��ɳ��Ϊʲô����ס������װ����һ�Ͻ��� 2026-10-22 16:34:20 ��Դ�������� ���գ�Ҳ�ź�����װ��ɳ�ذ�����֮��ĳ�ͻ�ٶ�������ɳ����Ϊ���ѿ�֧��������ǰʮ�Ĺ��ң����������װ�Ĳ�����ʼ������ռ���Ϸ硣�ر��Ǻ�����װ��ǰϮ��ɳ���׶����ŵõĹ����¹������ʻ�������ʩ������ش���Ա�������ù�������ٴ�����ɳ�صľ�����������ô���ۺ����֧�ŵ�ɳ�أ�Ϊ�ε��
14. search_web({"query":"NEOM 未来之城 线性城市 The Line 2025年9月16日 暂停施工"})
   → {"query":"NEOM 未来之城 线性城市 The Line 2025年9月16日 暂停施工","results":[{"snippet":"# 站在十字路口的 The Line：重新审视 NEOM 的乌托邦城市愿景\n...\n自展览结束以来，作为庞大的新未来城（NEOM）巨型项目核心的“THE LINE”在经历多年的过度扩张和外界不断积聚的质疑后，已在物理定律、政治结构以及严酷的资金体系面前处处碰壁，项目规模也被大幅缩减。这一在2021年雄心勃勃公布的、计划容纳900万人口、长达170公里的镜面线性城市，到了2026年初，却已沦为停工不断、主权财富基金减记80亿美元，以及重新审视2030年前还能保住哪些成果的“战略评估”对
16. fetch_source({"url":"https://globalbusinessoutlook.com/banking-and-finance/saudi-vision-riyadh-transforms-into-global-command-centre/"})
   → {"text":"Facebook Twitter Instagram Linkedin Email Top Stories CATEGORIES Banking and Finance Real Estate Economy Technology Banking and Finance UBS to concede in Swiss capital rules battle,… November 19, 2026 Banking and Finance JPMorgan appoints insider Bregje de Best as CFO,… November 18, 2026 Banking and Finance Mo
17. fetch_source({"url":"https://www.archdaily.com/zh/1040858/wei-yu-shi-zi-lu-kou-de-the-line-xiang-mu-zhong-shen-neom-wu-tuo-bang-shi-cheng-shi-yuan-jing"})
   → {"text":"ArchDaily 用品 站在十字路口的 The Line：重新审视 NEOM 的乌托邦城市愿景 订阅者专属 站在十字路口的 The Line：重新审视 NEOM 的乌托邦城市愿景 THE LINE 一期战略合作伙伴。图片 © NEOM 作者 Olivia Poston 阅读 英文 原文 点击这里 . 于2026年四月, 28 2023年，在利雅得举办的“THE LINE”展览闭幕之际， ArchDaily 总编辑与沙特新未来城（NEOM）“THE LINE”设计执行总监 Tarek Qaddumi 进行了一场深度对话 。Qaddumi 描绘了 一个以“五分钟步行生活圈”为核心概念的分层、立体的三维城市 ：垂直叠加
19. search_web({"query":"美国在沙特驻军 2700名美军 基地"})
   → {"query":"美国在沙特驻军 2700名美军 基地","results":[{"snippet":"此前3天，伊朗军队袭击了位于沙地境内的一个美军空军基地。\n...\n美国和沙地有著长达80多年的伙伴关系。根据Militarybases.com网站报导，美国在这个石油资源丰富的国家设有5个军事基地；NewsNation网站报导，驻扎在那里的美军人数约为2700人。\n...\n上周五，伊朗导弹袭击了位于沙地哈尔吉的苏丹王子空军基地，造成12名美军士兵受伤。\n\n美国《华尔街日报》报导，上周五，伊朗导弹袭击了位于沙地哈尔吉的苏丹王子空军基地，造成12名美军士兵受伤。","title":"尊重沙地视其为兄弟国家 伊朗外长呼吁“
20. search_web({"query":"也门 地形 70% 土地 海拔1500米以上 山脉"})
   → {"query":"也门 地形 70% 土地 海拔1500米以上 山脉","results":[{"snippet":"The area is characterized by an extensive thick sediment layer with slight almost uniform ( \\(1^{\\circ}\\) dip) at the surface. It includes the Hadramout- Mahrah uplands and areas from Mareb, Aljouf, and Shabwah.\n...\nRelative Humidity ranges from \\(40 - 6
22. report_claim({"claim":"胡塞为报复沙特连日轰炸，向利雅得发射导弹，击中沙特公共投资基金（PIF）总部大楼","queries":["胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日"],"quote":"Coalition spokesman Turki al-Malki said an initial assessment indicated that debris from the interception had caused material dama)
   → {"factId":"fact_e52620b678f594546d659d9d","ok":true,"reported":1,"status":"candidate"}
23. report_claim({"claim":"这次对利雅得PIF大楼的打击发生在10月19日","queries":["胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日"],"quote":"Air Strike Hits Saudi Arabia’s Sovereign Wealth Fund, Second-Tallest Skyscraper November 19, 2026","relation":"contradicts","source_url":"https://s)
   → {"factId":"fact_1c862cf3c8589fef2e242c77","ok":true,"reported":2,"status":"disputed"}
24. report_claim({"claim":"袭击造成PIF总部大楼多个楼层窗户碎裂、部分金属外立面受损","queries":["胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日"],"quote":"The 80-storey skyscraper, which houses the headquarters of the kingdom’s sovereign wealth fund, sustained damage to its facade, with several )
   → {"factId":"fact_c5da5d1d41bdcbd95c26c371","ok":true,"reported":3,"status":"verified"}
25. report_claim({"claim":"这次打击造成PIF大楼7到8个楼层的窗户碎裂","queries":["胡塞 导弹 利雅得 沙特公共投资基金 PIF 总部大楼 10月19日"],"verdict":"未找到外部出处"})
   → {"factId":"fact_16a7986ea99d099792bbdef7","ok":true,"reported":4,"status":"unsupported"}
27. report_claim({"claim":"沙特公共投资基金（PIF）总部大楼总高385米","queries":["PIF Tower 385米 利雅得 阿卜杜拉国王金融区 最高建筑 高度"],"quote":"Standing 385 metres above Riyadh’s surrounding cityscape, the PIF Tower, formerly known as the CMA Tower, is like the centerpiece of the King Abd)
   → {"factId":"fact_ad53ac92634b054f9b99c43a","ok":true,"reported":5,"status":"verified"}
28. report_claim({"claim":"PIF大楼里面聚集140多家入驻企业和超过75家跨国公司的区域总部","queries":["PIF Tower 入驻企业 140家 跨国公司区域总部 75家"],"quote":"Hosting more than 140 companies, from startups to MNCs, the district offers world-class flexible workspace through facilities like The Exec)
   → {"factId":"fact_252ed3ee5ac463916f5b4c84","ok":true,"reported":6,"status":"disputed"}
29. report_claim({"claim":"未来之城核心的线性城市（THE LINE）于2025年9月16日暂停施工","queries":["NEOM 未来之城 线性城市 The Line 2025年9月16日 暂停施工"],"quote":"2025年9月16日， 沙特阿拉伯 公共投资基金宣布无限期暂停“THE LINE”的施工，在规划的170公里中，目前仅完成了2.4公里的基础工程。","relation":"supports","source_url":"https://www.archda)
   → {"factId":"fact_a99f2b08aeec7c13e928bcb9","ok":true,"reported":7,"status":"verified"}
30. report_claim({"claim":"沙特2025年国防开支6000亿、全球第八","queries":["沙特 2025年 国防开支 全球第八 军费 排名"],"quote":"根据瑞典斯德哥尔摩国际和平研究所的测算，沙特2025年国防开支约830亿美元，位列世界第八","relation":"context","source_url":"https://military.cnr.cn/jsrp/20261022/t20261022_527849931.shtml","verdict":"外)
   → {"error":"quote 不是该来源的连续原文；请从摘要或正文里逐字复制，不要改写"}
32. report_claim({"claim":"沙特2025年国防开支6000亿、全球第八","queries":["沙特 2025年 国防开支 全球第八 军费 排名","沙特军费 830亿美元 全球第八 SIPRI"],"quote":"英国媒体曾经报道，2025年沙特军费开支超过830亿美元，位列全球第八。","relation":"context","source_url":"https://k.sina.com.cn/article_6362632222_m17b3e101e03301f2ts.)
   → {"error":"报「外部可印证 / 存在争议」前必须先 fetch_source 读该来源正文，不能只凭搜索摘要"}
34. fetch_source({"url":"https://k.sina.com.cn/article_6362632222_m17b3e101e03301f2ts.html"})
   → {"error":"fetch_source 预算已用完（6 次）"}
36. find_facts({"claim":"沙特2025年国防开支约830亿美元位列全球第八"})
   → {"candidates":[]}
38. report_claim({"claim":"沙特2025年国防开支6000亿、全球第八","queries":["沙特 2025年 国防开支 全球第八 军费 排名"],"quote":"英国媒体曾经报道，2025年沙特军费开支超过830亿美元，位列全球第八。","relation":"context","source_url":"https://k.sina.com.cn/article_6362632222_m17b3e101e03301f2ts.html","verdict":"仅非权威来源"})
   → {"factId":"fact_e217812c737c569bb65fc922","ok":true,"reported":8,"status":"candidate"}
40. finish({"summary":"共核查8条主张：3条外部可印证（PIF大楼外立面/窗户受损、楼高385米、线性城市2025年9月16日暂停施工），3条存在争议（「10月19日」应为2026年11月19日；导弹被联军拦截、碎片造成损坏而非直接「击中」；「140多家企业/75家跨国区域总部」实指整个阿卜杜拉国王金融区而非楼内），1条仅非权威来源（6000亿国防开支，媒体口径为约830亿美元、全球第八，SIPRI原文未能校验），1条未找到外部出处（「7到8个楼层」的具体层数）。受预算限制未)
   → {"ok":true}

## Facts for this note

- [candidate] 沙特2025年国防开支6000亿、全球第八
  - https://k.sina.com.cn/article_6362632222_m17b3e101e03301f2ts.html
    “英国媒体曾经报道，2025年沙特军费开支超过830亿美元，位列全球第八。”
- [verified] 未来之城核心的线性城市（THE LINE）于2025年9月16日暂停施工
  - https://www.archdaily.com/zh/1040858/wei-yu-shi-zi-lu-kou-de-the-line-xiang-mu-zhong-shen-neom-wu-tuo-bang-shi-cheng-shi-yuan-jing
    “2025年9月16日， 沙特阿拉伯 公共投资基金宣布无限期暂停“THE LINE”的施工，在规划的170公里中，目前仅完成了2.4公里的基础工程。”
- [disputed] PIF大楼里面聚集140多家入驻企业和超过75家跨国公司的区域总部
  - https://globalbusinessoutlook.com/banking-and-finance/saudi-vision-riyadh-transforms-into-global-command-centre/
    “Hosting more than 140 companies, from startups to MNCs, the district offers world-class flexible workspace through facilities like The Executive Centre, essential for firms in rapid growth or market-testing phases.”
- [verified] 沙特公共投资基金（PIF）总部大楼总高385米
  - https://executivecentre.sa/blog/pif-tower-in-riyadh-kafd
    “Standing 385 metres above Riyadh’s surrounding cityscape, the PIF Tower, formerly known as the CMA Tower, is like the centerpiece of the King Abdullah Financial District.”
- [unsupported] 这次打击造成PIF大楼7到8个楼层的窗户碎裂
- [verified] 袭击造成PIF总部大楼多个楼层窗户碎裂、部分金属外立面受损
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
    “The 80-storey skyscraper, which houses the headquarters of the kingdom’s sovereign wealth fund, sustained damage to its facade, with several windows reportedly blown out.”
- [disputed] 这次对利雅得PIF大楼的打击发生在10月19日
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
    “Air Strike Hits Saudi Arabia’s Sovereign Wealth Fund, Second-Tallest Skyscraper November 19, 2026”
- [candidate] 胡塞为报复沙特连日轰炸，向利雅得发射导弹，击中沙特公共投资基金（PIF）总部大楼
  - https://saharareporters.com/2026/11/19/air-strike-hits-saudi-arabias-sovereign-wealth-fund-second-tallest-skyscraper
    “Coalition spokesman Turki al-Malki said an initial assessment indicated that debris from the interception had caused material damage, adding that authorities were examining the impact.”