# Citation agent probe

- note: 五胡归华录87：北魏与刘宋的淮北争夺战 (yn-2fff289291e330a999cc7002)
- status: success
- summary: 共挂 6 条出处，全部为候选（链接）状态、原文待补：3 条指向《宋書》（十五万大军北上受降；刘彧语“我正要趁此机会北讨，你觉得如何？”），2 条指向《魏書》（薛安都降魏求援、尉元南下；慕容白曜入青州），2 条指向《資治通鑑》宋紀（冻死过半与堕指、张永冻落脚趾；死者枕藉六十里、张永沈攸之仅以身免逃淮阴）——因同书重复使用同一书目候选，实际 6 条绑定覆盖上述 5 处史事与 1 处原话。已入库目录中只有《明史》一条且与此无关，本主题无任何可读原文段落，故未做 attach_citation；引文细节（兵力十五万、冻死比例、六十里等）须待《宋書》《魏書》《資治通鑑》原文补入后逐条核校。主观评论段落（“弦外之音”等）未作出处。
- citations: 6
- duration: 29.0s
- usage: 67036 in / 4781 out / ¥0.0294

## Citations

### [candidate] 但刘彧的做法是，让张永、沈攸之两名将领领十五万大军向北出发，去接受薛安都的投降。
- 出处：宋書
- 链接：https://ctext.org/wb4404360
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；须在《宋書》相应纪传中核对张永、沈攸之北上受降事。

### [candidate] 在彭城的薛安都看到刘彧摆出这个架势，自然怕了，果然向北魏投诚，要求北魏派出援军。
- 出处：魏書
- 链接：https://ctext.org/wb403341
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；须在《魏書》相關傳中核对薛安都降魏求援及尉元南下事。

### [candidate] 不久北魏又派慕容白曜等将领率大军进入青州。
- 出处：魏書
- 链接：https://ctext.org/wb403341
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；须在《魏書》慕容白曜相關傳中核对入青州事。

### [candidate] 史书记载，宋军冻死超过一半，十之八九的人被冻掉手脚，连主帅张永的脚趾头都被冻掉了。
- 出处：資治通鑑
- 链接：https://ctext.org/wb548761
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；“冻死过半”“十之八九堕指”“张永脚趾冻落”等语须核对《資治通鑑》宋紀原文（并可参《宋書》）。

### [candidate] 宋军大败，数万人战死，尸体枕藉六十里。两名主将张永和沈攸之仅以身免，逃到淮阴整顿残军。
- 出处：資治通鑑
- 链接：https://ctext.org/wb548761
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；“死者数万、枕藉六十里”“张永、沈攸之仅以身免”须核对《資治通鑑》宋紀原文。

### [candidate] 我正要趁此机会北讨，你觉得如何？
- 出处：宋書
- 链接：https://ctext.org/wb4404360
- 候选摘要：候选为书目链接（全文需订阅/API Key），原文待补；刘彧对萧道成此语须核对《宋書》相关纪传（另见《資治通鑑》宋紀）。

## Trajectory

0. model: 我会先查已入库目录与候选，找出能覆盖这些人物、事件的史料。
1. list_sources({"query":"宋书"})
   → {"sources":[]}
2. list_sources({"query":"魏书"})
   → {"sources":[]}
3. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
4. find_candidates({"query":"薛安都"})
   → {"candidates":[]}
6. list_sources({"limit":12,"query":"宋"})
   → {"sources":[]}
7. list_sources({"limit":12,"query":"魏"})
   → {"sources":[]}
8. list_sources({"limit":12,"query":"通鉴"})
   → {"sources":[]}
9. find_candidates({"limit":12,"query":"彭城"})
   → {"candidates":[]}
11. find_candidates({"query":"尉元"})
   → {"candidates":[]}
12. find_candidates({"query":"张永 沈攸之"})
   → {"candidates":[]}
13. find_candidates({"query":"北魏 刘宋 淮北"})
   → {"candidates":[]}
14. find_candidates({"query":"义嘉之乱"})
   → {"candidates":[]}
16. list_sources({"limit":12,"query":"史"})
   → {"sources":[{"author":"张廷玉等","id":"src_b66505a8fdcb4d788819fcb475f2626d","passages":1,"source_type":"primary","url":"","volume":"卷三百八","work_title":"明史"}]}
17. list_sources({"limit":12,"query":"a"})
   → {"sources":[]}
18. find_candidates({"limit":12,"query":"宋明帝"})
   → {"candidates":[]}
19. find_candidates({"limit":12,"query":"慕容白曜"})
   → {"candidates":[]}
21. find_candidates({"limit":12,"query":"宋"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_e4dd0d454d084dc7afcd26bc11b7f8db","snippet":"ctext 书目；全文 API 需要订阅/API Key",
22. find_candidates({"limit":12,"query":"传"})
   → {"candidates":[{"authority":"link_only","id":"cand_9d80d5c216e84d9cacdc32a81fb92878","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：江表传","url":"https://fhac.com.cn/","work_title":"《江表传》（裴松之注引）"},{"authority":"link_only","id":"cand_a4e3ec67d02f4161b7f50782c5877200","snippet":"馆藏目录/影像；需要按书名检索，当前只作候
23. find_candidates({"limit":12,"query":"北魏"})
   → {"candidates":[]}
24. find_candidates({"limit":12,"query":"徐州"})
   → {"candidates":[]}
26. find_candidates({"limit":12,"query":"魏書"})
   → {"candidates":[{"authority":"link_only","id":"cand_1dcd59085bd54f3e8bc3fe152ce8cf41","snippet":"ctext 书目；全文 API 需要订阅/API Key","title":"西魏書","url":"https://ctext.org/wb290037","work_title":"西魏書"},{"authority":"link_only","id":"cand_f6df20aa6d024fb0b75e56048d5f261d","snippet":"ctext 书目；全文 API 需要订阅/API
27. find_candidates({"limit":12,"query":"資治通鑑 宋紀"})
   → {"candidates":[{"authority":"link_only","id":"cand_c797a3ba3c094bcdb195178646e3ef27","snippet":"ctext 书目；全文 API 需要订阅/API Key","title":"續宋編年資治通鑑","url":"https://ctext.org/wb965291","work_title":"續宋編年資治通鑑"},{"authority":"link_only","id":"cand_dfaf04170ea94df2af9bedee7e05de27","snippet":"ctext 书目；全文 AP
28. find_candidates({"limit":12,"query":"薛安都傳"})
   → {"candidates":[]}
29. find_candidates({"limit":12,"query":"尉元傳"})
   → {"candidates":[]}
31. list_sources({"limit":12,"query":"元龟"})
   → {"sources":[]}
32. list_sources({"limit":12,"query":"建康"})
   → {"sources":[]}
33. list_sources({"limit":12,"query":"通典"})
   → {"sources":[]}
34. find_candidates({"limit":12,"query":"宋書 薛安都 沈文秀 崔道固"})
   → {"candidates":[{"authority":"link_only","id":"cand_e4dd0d454d084dc7afcd26bc11b7f8db","snippet":"ctext 书目；全文 API 需要订阅/API Key","title":"補宋書刑法志、食貨志","url":"https://ctext.org/wb609170","work_title":"補宋書刑法志、食貨志"},{"authority":"link_only","id":"cand_9c2f03e6ab4145ffa25c8d04d522bc52","snippet":"ctext 书目；全
36. find_candidates({"limit":12,"query":"北史"})
   → {"candidates":[]}
37. find_candidates({"limit":12,"query":"沈攸之"})
   → {"candidates":[]}
38. find_candidates({"limit":12,"query":"宋明帝 劉彧"})
   → {"candidates":[]}
39. find_candidates({"limit":12,"query":"青州 慕容白曜"})
   → {"candidates":[]}
40. model: 已确认：目录里只有《明史》一条无关入库史料，本主题的《宋书》《魏书》《资治通鉴》都只有书目候选、无原文段落，因此按“候选＋待补原文”处理。
41. attach_candidate({"candidate_id":"cand_7b1af944081c4ff9ab2eb61756750c26","note_quote":"但刘彧的做法是，让张永、沈攸之两名将领领十五万大军向北出发，去接受薛安都的投降。","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；须在《宋書》相应纪传中核对张永、沈攸之北上受降事。","url":"https://ctext.org/wb4404360","work_t)
   → {"id":"nc_793725dbcb2d46648eb7b38e15d41964","ok":true,"status":"candidate","total":1,"work_title":"宋書"}
42. attach_candidate({"candidate_id":"cand_c5b4e45aa6114e2295ac2b0dc875bfe0","note_quote":"在彭城的薛安都看到刘彧摆出这个架势，自然怕了，果然向北魏投诚，要求北魏派出援军。","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；须在《魏書》相關傳中核对薛安都降魏求援及尉元南下事。","url":"https://ctext.org/wb403341","work_t)
   → {"id":"nc_00f4f867a5c648168dd0d831f625b345","ok":true,"status":"candidate","total":2,"work_title":"魏書"}
43. attach_candidate({"candidate_id":"cand_c5b4e45aa6114e2295ac2b0dc875bfe0","note_quote":"不久北魏又派慕容白曜等将领率大军进入青州。","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；须在《魏書》慕容白曜相關傳中核对入青州事。","url":"https://ctext.org/wb403341","work_title":"魏書"})
   → {"id":"nc_f78a6f1f7ac640358052e469a70ff6a4","ok":true,"status":"candidate","total":3,"work_title":"魏書"}
44. attach_candidate({"candidate_id":"cand_dfaf04170ea94df2af9bedee7e05de27","note_quote":"史书记载，宋军冻死超过一半，十之八九的人被冻掉手脚，连主帅张永的脚趾头都被冻掉了。","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；“冻死过半”“十之八九堕指”“张永脚趾冻落”等语须核对《資治通鑑》宋紀原文（并可参《宋書》）。","url":"https://ctext)
   → {"id":"nc_f11a5eb58e3e40f99711dffe191d772e","ok":true,"status":"candidate","total":4,"work_title":"資治通鑑"}
45. attach_candidate({"candidate_id":"cand_dfaf04170ea94df2af9bedee7e05de27","note_quote":"宋军大败，数万人战死，尸体枕藉六十里。两名主将张永和沈攸之仅以身免，逃到淮阴整顿残军。","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；“死者数万、枕藉六十里”“张永、沈攸之仅以身免”须核对《資治通鑑》宋紀原文。","url":"https://ctext.org/wb)
   → {"id":"nc_3ba5b6a26e124c509d5de833dce76d58","ok":true,"status":"candidate","total":5,"work_title":"資治通鑑"}
46. attach_candidate({"candidate_id":"cand_7b1af944081c4ff9ab2eb61756750c26","note_quote":"我正要趁此机会北讨，你觉得如何？","provider":"ctext","snippet":"候选为书目链接（全文需订阅/API Key），原文待补；刘彧对萧道成此语须核对《宋書》相关纪传（另见《資治通鑑》宋紀）。","url":"https://ctext.org/wb4404360","work_title":"宋書"})
   → {"id":"nc_78d911c6105d4f5a8f7826ab4cde63a1","ok":true,"status":"candidate","total":6,"work_title":"宋書"}
48. finish({"summary":"共挂 6 条出处，全部为候选（链接）状态、原文待补：3 条指向《宋書》（十五万大军北上受降；刘彧语“我正要趁此机会北讨，你觉得如何？”），2 条指向《魏書》（薛安都降魏求援、尉元南下；慕容白曜入青州），2 条指向《資治通鑑》宋紀（冻死过半与堕指、张永冻落脚趾；死者枕藉六十里、张永沈攸之仅以身免逃淮阴）——因同书重复使用同一书目候选，实际 6 条绑定覆盖上述 5 处史事与 1 处原话。已入库目录中只有《明史》一条且与此无关，本主题无任何可读原文段落，故未)
   → {"ok":true}