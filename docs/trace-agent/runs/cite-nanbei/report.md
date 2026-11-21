# Citation agent probe

- note: 五胡归华录87：北魏与刘宋的淮北争夺战 (yn-2fff289291e330a999cc7002)
- status: success
- summary: 共挂 6 条出处：薛安都等遣使归顺（《宋书》候选）、466年11月张永沈攸之十五万北上（《资治通鉴》卷131候选）、薛安都投魏与尉元慕容白曜南下青徐（《魏书·尉元传》候选）、泗水冰合弃船步走与冻死过半（《资治通鉴》候选）、尸体枕藉六十里与仅以身免（《资治通鉴》候选）、刘彧「战无不胜，无一遗军」原话（《魏书》相关候选）；史料库内暂无入库原文，6 条均标「待补原文」，需按书名补读卷次后再升级为原文引用。
- citations: 6
- duration: 28.4s
- usage: 37265 in / 5179 out / ¥0.0289

## Citations

### [candidate] 义嘉之乱平定后，彭城的薛安都、青州的沈文秀与崔道固遣使向刘彧归顺
- 出处：《宋书》
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E8%96%9B%E5%AE%89%E9%83%BD+%E9%81%A3%E4%BD%BF+%E5%BB%BA%E5%BA%B7+%E5%BD%92%E9%A1%BA+%E5%88%98%E5%BD%A7;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 466年11月派张永、沈攸之率十五万大军北上受降
- 出处：资治通鉴
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E5%8D%B7%E4%B8%80%E7%99%BE%E4%B8%89%E5%8D%81%E4%B8%80+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 薛安都果然被逼反，向北魏求援；北魏尉元领兵南下彭城，慕容白曜等率大军进入青州
- 出处：《魏书》
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 泗水结冰，宋军弃船步行，史载冻死超过一半，十之八九的人被冻掉手脚。
- 出处：资治通鉴
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E6%B3%97%E6%B0%B4%E5%86%B0%E5%90%88+%E5%BC%83%E8%88%B9%E6%AD%A5%E8%B5%B0;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 宋军数万人战死、尸体枕藉六十里，张永与沈攸之仅以身免，逃到淮阴。
- 出处：资治通鉴
- 链接：https://read.nlc.cn/
- 候选摘要：馆藏目录/影像；需要按书名检索，当前只作候选

### [candidate] 我们的大军必然战无不胜，无一遗军。
- 出处：魏书
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E8%96%9B%E5%AE%89%E9%83%BD+%E5%BC%A0%E6%B0%B8+%E6%B2%88%E6%94%B8%E4%B9%8B;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

## Trajectory

0. model: 我先查一查可用的史料。
1. find_candidates({"query":"尉元 薛安都 彭城 张永"})
   → {"candidates":[]}
2. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
4. find_candidates({"query":"宋书"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
5. list_sources({"query":"魏书"})
   → {"sources":[]}
6. list_sources({"query":"宋"})
   → {"sources":[]}
8. find_candidates({"query":"资治通鉴 宋纪 明帝"})
   → {"candidates":[]}
9. find_candidates({"query":"魏书 尉元传"})
   → {"candidates":[]}
10. find_candidates({"query":"南史 薛安都"})
   → {"candidates":[]}
12. find_candidates({"query":"通鉴"})
   → {"candidates":[{"authority":"link_only","id":"cand_2eddf39327c74093b5d2e2a5b848ca60","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：资治通鉴","url":"https://fhac.com.cn/","work_title":"资治通鉴"},{"authority":"link_only","id":"cand_2da34159ceba43118ad8f14a540f2009","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","tit
13. find_candidates({"query":"北魏 青州 慕容白曜"})
   → {"candidates":[]}
14. list_sources({"query":"宋书"})
   → {"sources":[]}
16. find_candidates({"query":"魏书"})
   → {"candidates":[{"authority":"link_only","id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","snippet":"外文/公版全文；Full view 才可引用，当前只作候选","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93
17. find_candidates({"query":"沈文秀 崔道固 青州"})
   → {"candidates":[]}
18. find_candidates({"query":"张永 沈攸之 彭城 败退"})
   → {"candidates":[]}
20. attach_candidate({"candidate_id":"cand_9c6ebae72efd4ef3ac6e716ea3574787","note_quote":"义嘉之乱平定后，彭城的薛安都、青州的沈文秀与崔道固遣使向刘彧归顺","provider":"HathiTrust","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（原文待补）","title":"HathiTrust：宋书","url":"https://babel.hathitrust.org/cgi/)
   → {"id":"nc_8d22d32aaaaa452ea4be6d2004576fef","ok":true,"status":"candidate","total":1,"work_title":"《宋书》"}
21. attach_candidate({"candidate_id":"cand_ff7f51a75d8e4a7d8ed731ee26631980","note_quote":"466年11月派张永、沈攸之率十五万大军北上受降","provider":"HathiTrust","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（原文待补）","title":"HathiTrust：资治通鉴","url":"https://babel.hathitrust.org/cgi/ls?q1=)
   → {"id":"nc_a768d86a072a4ccba0baee726bb33005","ok":true,"status":"candidate","total":2,"work_title":"资治通鉴"}
22. attach_candidate({"candidate_id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","note_quote":"薛安都果然被逼反，向北魏求援；北魏尉元领兵南下彭城，慕容白曜等率大军进入青州","provider":"HathiTrust","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（原文待补）","title":"HathiTrust：魏书","url":"https://babel.hathitrust.or)
   → {"id":"nc_1ee4e87236b947e89a7074c66da48d3b","ok":true,"status":"candidate","total":3,"work_title":"《魏书》"}
23. attach_candidate({"candidate_id":"cand_bb21ae6016bd41d5abbc87333409d699","note_quote":"泗水结冰，宋军弃船步行，史载冻死超过一半，十之八九的人被冻掉手脚。","provider":"HathiTrust","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（原文待补）","title":"HathiTrust：资治通鉴","url":"https://babel.hathitrust.org/c)
   → {"id":"nc_4835d1df2e4a4bbdb0dcfed290dd1706","ok":true,"status":"candidate","total":4,"work_title":"资治通鉴"}
24. attach_candidate({"candidate_id":"cand_2da34159ceba43118ad8f14a540f2009","note_quote":"宋军数万人战死、尸体枕藉六十里，张永与沈攸之仅以身免，逃到淮阴。","provider":"中华古籍资源库","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选（原文待补）","title":"中华古籍资源库：资治通鉴","url":"https://read.nlc.cn/","work_title":"资治通鉴"})
   → {"id":"nc_e4f9b9dfe0714d639b51d147abdc2128","ok":true,"status":"candidate","total":5,"work_title":"资治通鉴"}
25. attach_candidate({"candidate_id":"cand_92afa64b0e714c0494fdb74d328134e0","note_quote":"我们的大军必然战无不胜，无一遗军。","provider":"HathiTrust","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（原文待补）","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F)
   → {"id":"nc_ef4984a7690c4b18bafefc894e41ca2c","ok":true,"status":"candidate","total":6,"work_title":"魏书"}
27. finish({"summary":"共挂 6 条出处：薛安都等遣使归顺（《宋书》候选）、466年11月张永沈攸之十五万北上（《资治通鉴》卷131候选）、薛安都投魏与尉元慕容白曜南下青徐（《魏书·尉元传》候选）、泗水冰合弃船步走与冻死过半（《资治通鉴》候选）、尸体枕藉六十里与仅以身免（《资治通鉴》候选）、刘彧「战无不胜，无一遗军」原话（《魏书》相关候选）；史料库内暂无入库原文，6 条均标「待补原文」，需按书名补读卷次后再升级为原文引用。"})
   → {"ok":true}