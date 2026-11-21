# Citation agent probe

- note: 五胡归华录87：北魏与刘宋的淮北争夺战 (yn-2fff289291e330a999cc7002)
- status: success
- summary: 共绑定 6 条出处，全部为「候选（书名+链接）、原文待补」：《宋书》3 条（十五万大军北上受降、刘彧「趁此机会北讨」原话、「战无不胜，无一遗军」原话）、《魏书》1 条（尉元正面相持＋骑兵断粮道）、《资治通鉴》2 条（泗水结冰弃船步行、数万人战死尸体枕藉六十里）；库中无已入库原文段落（list_sources 多次检索为空），故未能用 attach_citation，均需补原文核对卷次与字句。未及出处的部分：薛安都等遣使归顺、慕容白曜入青州（找不到相关候选）、冻死过半与十之八九冻掉手脚、张永沈攸之仅以身免逃淮阴等，已超 6 条上限，留待下一轮补。
- citations: 6
- duration: 25.7s
- usage: 67186 in / 4569 out / ¥0.0277

## Citations

### [candidate] 刘彧的做法是，让张永、沈攸之两名将领领十五万大军向北出发，去接受薛安都的投降
- 出处：宋书
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E8%96%9B%E5%AE%89%E9%83%BD%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E6%B2%88%E6%94%B8%E4%B9%8B+%E5%8D%81%E4%BA%94%E4%B8%87;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 「我正要趁此机会北讨，你觉得如何？」
- 出处：宋书
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E6%98%8E%E5%B8%9D%E7%BA%AA+%E4%B9%89%E5%98%89+%E5%88%98%E5%AD%90%E5%8B%8B;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 「我们的大军必然战无不胜，无一遗军。」
- 出处：宋书
- 链接：https://read.nlc.cn/
- 候选摘要：馆藏目录/影像；需要按书名检索，当前只作候选

### [candidate] 本人在正面与张永相持，另派机动骑兵截断张永的粮道、打击其补给
- 出处：魏书
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E8%96%9B%E5%AE%89%E9%83%BD+%E5%BC%A0%E6%B0%B8+%E6%B2%88%E6%94%B8%E4%B9%8B;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 但这场降温导致泗水结冰，张永只能率宋军弃船步行
- 出处：资治通鉴
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E6%B3%97%E6%B0%B4%E5%86%B0%E5%90%88+%E5%BC%83%E8%88%B9%E6%AD%A5%E8%B5%B0;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 宋军大败，数万人战死，尸体枕藉六十里
- 出处：资治通鉴
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E5%8D%B7%E4%B8%80%E7%99%BE%E4%B8%89%E5%8D%81%E4%B8%80+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

## Trajectory

0. model: 我先检索已入库史料目录和候选。
1. list_sources({"query":"宋书"})
   → {"sources":[]}
2. list_sources({"query":"魏书"})
   → {"sources":[]}
3. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
4. find_candidates({"query":"薛安都 彭城 北魏"})
   → {"candidates":[]}
5. model: 目录检索无结果，换关键词再试。
6. list_sources({"limit":12,"query":"宋"})
   → {"sources":[]}
7. find_candidates({"limit":12,"query":"宋明帝 刘彧"})
   → {"candidates":[]}
8. find_candidates({"limit":12,"query":"尉元"})
   → {"candidates":[]}
9. find_candidates({"limit":12,"query":"张永 沈攸之"})
   → {"candidates":[]}
11. list_sources({"query":"通鉴"})
   → {"sources":[]}
12. list_sources({"query":"北魏"})
   → {"sources":[]}
13. find_candidates({"query":"宋书 沈攸之"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
14. find_candidates({"query":"义嘉之乱"})
   → {"candidates":[]}
16. find_candidates({"limit":12,"query":"魏书 尉元 慕容白曜 彭城"})
   → {"candidates":[{"authority":"link_only","id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","snippet":"外文/公版全文；Full view 才可引用，当前只作候选","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93
17. find_candidates({"limit":12,"query":"资治通鉴 宋纪 泰始"})
   → {"candidates":[{"authority":"link_only","id":"cand_2eddf39327c74093b5d2e2a5b848ca60","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：资治通鉴","url":"https://fhac.com.cn/","work_title":"资治通鉴"},{"authority":"link_only","id":"cand_2da34159ceba43118ad8f14a540f2009","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","tit
18. find_candidates({"limit":12,"query":"南史 张永"})
   → {"candidates":[]}
19. find_candidates({"limit":12,"query":"宋书 张永 泗水 冻"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
21. list_sources({"query":"沈约"})
   → {"sources":[]}
22. list_sources({"query":"司马光"})
   → {"sources":[]}
23. find_candidates({"limit":6,"query":"慕容白曜 青州"})
   → {"candidates":[]}
24. model: 库中没有已入库原文段落（list_sources 各词均空），只能用候选并标注「待补原文」。
25. attach_candidate({"candidate_id":"cand_9a5037f67f4a4cd6bd8a8337a54e22c8","note_quote":"刘彧的做法是，让张永、沈攸之两名将领领十五万大军向北出发，去接受薛安都的投降","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（检索词：薛安都传 张永 沈攸之 十五万）","title":"HathiTrust：宋书","url":"https://babel.hathitrust.org/cgi/ls?)
   → {"id":"nc_9ec2d5d3d76f475d9c15f0bd578f9622","ok":true,"status":"candidate","total":1,"work_title":"宋书"}
26. attach_candidate({"candidate_id":"cand_2faf563df47b444d9c11a92578bade0d","note_quote":"「我正要趁此机会北讨，你觉得如何？」","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（检索词：明帝纪 义嘉 刘子勋）","title":"HathiTrust：宋书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E6)
   → {"id":"nc_6ae08cbe42ba48789b9c353ea3fb790d","ok":true,"status":"candidate","total":2,"work_title":"宋书"}
28. attach_candidate({"candidate_id":"cand_b507b87a34d84c029f8741c68705c155","note_quote":"「我们的大军必然战无不胜，无一遗军。」","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选（宋书检索：明帝纪/薛安都相关记事）","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"})
   → {"id":"nc_6cf9cb00dbc24acea7696fdce26bb475","ok":true,"status":"candidate","total":3,"work_title":"宋书"}
29. attach_candidate({"candidate_id":"cand_92afa64b0e714c0494fdb74d328134e0","note_quote":"本人在正面与张永相持，另派机动骑兵截断张永的粮道、打击其补给","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（检索词：尉元传 薛安都 张永 沈攸之）","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD)
   → {"id":"nc_f9d5e468d793492eb0eeb4718c3c61bf","ok":true,"status":"candidate","total":4,"work_title":"魏书"}
31. attach_candidate({"candidate_id":"cand_bb21ae6016bd41d5abbc87333409d699","note_quote":"但这场降温导致泗水结冰，张永只能率宋军弃船步行","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（检索词：泰始二年 张永 泗水冰合 弃船步走）","title":"HathiTrust：资治通鉴","url":"https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%8)
   → {"id":"nc_1dd4e0cf92004060ac130bcbb739c3ec","ok":true,"status":"candidate","total":5,"work_title":"资治通鉴"}
32. attach_candidate({"candidate_id":"cand_ff7f51a75d8e4a7d8ed731ee26631980","note_quote":"宋军大败，数万人战死，尸体枕藉六十里","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（检索词：卷一三一 泰始二年 张永 彭城）","title":"HathiTrust：资治通鉴","url":"https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2)
   → {"id":"nc_abf025abb0d74b39a1fb24caf5255e8f","ok":true,"status":"candidate","total":6,"work_title":"资治通鉴"}
34. finish({"summary":"共绑定 6 条出处，全部为「候选（书名+链接）、原文待补」：《宋书》3 条（十五万大军北上受降、刘彧「趁此机会北讨」原话、「战无不胜，无一遗军」原话）、《魏书》1 条（尉元正面相持＋骑兵断粮道）、《资治通鉴》2 条（泗水结冰弃船步行、数万人战死尸体枕藉六十里）；库中无已入库原文段落（list_sources 多次检索为空），故未能用 attach_citation，均需补原文核对卷次与字句。未及出处的部分：薛安都等遣使归顺、慕容白曜入青州（找不到相关候)
   → {"ok":true}