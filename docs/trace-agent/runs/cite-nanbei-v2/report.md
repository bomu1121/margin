# Citation agent probe

- note: 五胡归华录87：北魏与刘宋的淮北争夺战 (yn-2fff289291e330a999cc7002)
- status: no_sources
- summary: 已用宋书、魏书、资治通鉴、宋明帝、南北朝、薛安都、尉元／彭城、沈文秀／青州、张永／沈攸之、萧道成、慕容白曜、淮北等词检索 list_sources 与 find_candidates，库内史料目录与候选池均为空，故本次未能绑定任何一条出处：刘彧命张永、沈攸之率十五万北上受降（466年11月）、薛安都遣使归顺后降魏求援、《宋书》所载「我正要趁此机会北讨」与「大军必然战无不胜，无一遗军」两处原话、尉元断粮道与前后夹击致宋军「尸体枕藉六十里」、泗水结冰宋军冻死过半且十之八九堕指、张永沈攸之仅以身免逃淮阴这 6 句均需出处但一律待补原文，建议后续补入《宋书·薛安都传》《宋书·沈文秀传》《宋书·张永传》《魏书·尉元传》及《资治通鉴》宋明帝泰始二至三年条后再行 attach_citation。
- citations: 0
- duration: 11.7s
- usage: 22160 in / 1534 out / ¥0.0083

## Citations

## Trajectory

0. model: 我先查一下库里有哪些相关史料。
1. list_sources({"query":"宋书"})
   → {"sources":[]}
2. list_sources({"query":"魏书"})
   → {"sources":[]}
3. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
4. find_candidates({"query":"薛安都"})
   → {"candidates":[]}
5. model: 库里没有直接命中，扩大检索范围。
6. list_sources({"query":"宋明帝"})
   → {"sources":[]}
7. list_sources({"query":"南北朝"})
   → {"sources":[]}
8. find_candidates({"query":"尉元 彭城"})
   → {"candidates":[]}
9. find_candidates({"query":"沈文秀 青州"})
   → {"candidates":[]}
10. model: 再用更宽的检索词试一次，确认库内是否有任何条目。
11. list_sources({"limit":12,"query":"宋"})
   → {"sources":[]}
12. list_sources({"limit":12,"query":"魏"})
   → {"sources":[]}
13. find_candidates({"limit":12,"query":"彭城"})
   → {"candidates":[]}
14. find_candidates({"limit":12,"query":"义嘉之乱"})
   → {"candidates":[]}
16. find_candidates({"limit":12,"query":"张永 沈攸之"})
   → {"candidates":[]}
17. find_candidates({"limit":12,"query":"萧道成"})
   → {"candidates":[]}
18. list_sources({"limit":12,"query":"通鉴"})
   → {"sources":[]}
19. list_sources({"limit":12,"query":"慕容白曜"})
   → {"sources":[]}
21. list_sources({"limit":12,"query":"书"})
   → {"sources":[]}
22. find_candidates({"limit":12,"query":"淮北"})
   → {"candidates":[]}
24. finish({"summary":"已用宋书、魏书、资治通鉴、宋明帝、南北朝、薛安都、尉元／彭城、沈文秀／青州、张永／沈攸之、萧道成、慕容白曜、淮北等词检索 list_sources 与 find_candidates，库内史料目录与候选池均为空，故本次未能绑定任何一条出处：刘彧命张永、沈攸之率十五万北上受降（466年11月）、薛安都遣使归顺后降魏求援、《宋书》所载「我正要趁此机会北讨」与「大军必然战无不胜，无一遗军」两处原话、尉元断粮道与前后夹击致宋军「尸体枕藉六十里」、泗水结冰宋军冻)
   → {"ok":true}