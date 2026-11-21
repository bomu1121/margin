# Citation agent probe

- note: 五胡归华录87：北魏与刘宋的淮北争夺战 (yn-2fff289291e330a999cc7002)
- status: no_sources
- summary: 仓库与候选池均无相关史料：list_sources 仅检索到《明史》卷三百八一种（与北魏刘宋无关），find_candidates 对「薛安都」「张永」「尉元」「义嘉之乱」「泗水」「资治通鉴」「宋书」「魏书」等关键词均返回空，故本次未能给出任何出处（0 条）。待补原文的关键句有：①「彭城的薛安都、青州的沈文秀与崔道固遣使向刘彧归顺」②「466年11月派张永、沈攸之率十五万大军北上受降」（应出《宋书·薛安都传》/《资治通鉴》宋纪）③「北魏尉元领兵南下彭城，慕容白曜等率大军进入青州」（应出《魏书·尉元传》）④「尉元用正面相持加骑兵断粮道……467年正月张永被迫南撤」⑤「泗水结冰，宋军弃船步行，史载冻死超过一半，十之八九的人被冻掉手脚」⑥「尸体枕藉六十里，张永与沈攸之仅以身免，逃到淮阴」。另笔记所引刘彧原话「我正要趁此机会北讨，你觉得如何？」「我们的大军必然战无不胜，无一遗军」也无原文可校。建议
- citations: 0
- duration: 13.8s
- usage: 31742 in / 1875 out / ¥0.0105

## Citations

## Trajectory

0. model: 我先查有哪些可用史料。
1. list_sources({"query":"宋书"})
   → {"sources":[]}
2. list_sources({"query":"魏书"})
   → {"sources":[]}
3. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
4. list_sources({"query":"南史"})
   → {"sources":[]}
6. find_candidates({"query":"宋书 薛安都"})
   → {"candidates":[]}
7. find_candidates({"query":"魏书 尉元"})
   → {"candidates":[]}
8. find_candidates({"query":"资治通鉴 宋纪 薛安都降魏"})
   → {"candidates":[]}
9. find_candidates({"query":"张永 沈攸之 彭城"})
   → {"candidates":[]}
11. find_candidates({"query":"北魏"})
   → {"candidates":[]}
12. find_candidates({"query":"刘宋"})
   → {"candidates":[]}
13. list_sources({"query":"史料"})
   → {"sources":[]}
14. find_candidates({"query":"断粮道 骑兵"})
   → {"candidates":[]}
16. find_candidates({"query":"义嘉之乱"})
   → {"candidates":[]}
17. find_candidates({"query":"宋明帝"})
   → {"candidates":[]}
18. list_sources({"query":"通鉴"})
   → {"sources":[]}
19. list_sources({"query":"沈文秀"})
   → {"sources":[]}
21. find_candidates({"limit":12,"query":"薛安都"})
   → {"candidates":[]}
22. find_candidates({"limit":12,"query":"张永"})
   → {"candidates":[]}
23. list_sources({"limit":12,"query":"中国"})
   → {"sources":[]}
24. find_candidates({"limit":12,"query":"泗水"})
   → {"candidates":[]}
26. list_sources({"limit":12,"query":"卷"})
   → {"sources":[{"author":"张廷玉等","id":"src_b66505a8fdcb4d788819fcb475f2626d","passages":1,"source_type":"primary","url":"","volume":"卷三百八","work_title":"明史"}]}
27. find_candidates({"limit":12,"query":"北魏 南宋 战争"})
   → {"candidates":[]}
29. list_sources({"limit":12,"query":"宋"})
   → {"sources":[]}
30. list_sources({"limit":12,"query":"魏"})
   → {"sources":[]}
31. list_sources({"limit":12,"query":"书"})
   → {"sources":[]}
33. finish({"summary":"仓库与候选池均无相关史料：list_sources 仅检索到《明史》卷三百八一种（与北魏刘宋无关），find_candidates 对「薛安都」「张永」「尉元」「义嘉之乱」「泗水」「资治通鉴」「宋书」「魏书」等关键词均返回空，故本次未能给出任何出处（0 条）。待补原文的关键句有：①「彭城的薛安都、青州的沈文秀与崔道固遣使向刘彧归顺」②「466年11月派张永、沈攸之率十五万大军北上受降」（应出《宋书·薛安都传》/《资治通鉴》宋纪）③「北魏尉元领兵南下彭城)
   → {"ok":true}