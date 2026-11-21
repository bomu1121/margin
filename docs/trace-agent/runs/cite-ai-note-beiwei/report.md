# Citation agent probe

- note: 十五万兵去受降：北魏与刘宋的淮北争夺战（466—467） (yn-10851feb1e331547c3846f50)
- status: success
- summary: 找到 6 条出处，其中 6 条待补原文
- citations: 6
- duration: 30.6s
- usage: 78597 in / 6470 out / ¥0.0421

## Citations

### [candidate] 江北的薛安都（彭城）、沈文秀与崔道固（青州）遣使到建康表示归顺。
- 出处：《宋书》（薛安都等遣使归顺条，待补原文）
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E8%96%9B%E5%AE%89%E9%83%BD+%E9%81%A3%E4%BD%BF+%E5%BB%BA%E5%BA%B7+%E5%BD%92%E9%A1%BA+%E5%88%98%E5%BD%A7;a=srchls;anyall1=all;lmt=ft
- 候选摘要：HathiTrust 公版全文检索入口；对应《宋书》薛安都遣使建康归顺、刘彧部分，需 Full view 核对卷页

### [candidate] 「我正要趁此机会北讨，你觉得如何？」
- 出处：《宋书·明帝纪》（泰始年间北讨议，待补原文）
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E6%98%8E%E5%B8%9D%E7%BA%AA+%E4%B9%89%E5%98%89+%E5%88%98%E5%AD%90%E5%8B%8B;a=srchls;anyall1=all;lmt=ft
- 候选摘要：HathiTrust《宋书》明帝纪、义嘉之乱相关检索入口；此为素材转写的刘彧语，文言原文待补核对

### [candidate] 「我们的大军必然战无不胜，无一遗军。」
- 出处：《资治通鉴》卷一三一 泰始二年（彭城之役，待补原文）
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E5%8D%B7%E4%B8%80%E7%99%BE%E4%B8%89%E5%8D%81%E4%B8%80+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E;a=srchls;anyall1=all;lmt=ft
- 候选摘要：HathiTrust《资治通鉴》卷一百三十一 泰始二年 张永 彭城 检索入口；素材自称“原文如此”的转写句，原文字样待补

### [candidate] 张永南撤。大雪降温、泗水结冰，弃船步行；尉元令薛安都背后追击、自率骑兵前拦。宋军大败，张永、沈攸之仅以身免，逃至淮阴收拾残军。
- 出处：《资治通鉴》泰始二年（泗水冰合、弃船步走条，待补原文）
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9%89%B4+%E6%B3%B0%E5%A7%8B%E4%BA%8C%E5%B9%B4+%E5%BC%A0%E6%B0%B8+%E6%B3%97%E6%B0%B4%E5%86%B0%E5%90%88+%E5%BC%83%E8%88%B9%E6%AD%A5%E8%B5%B0;a=srchls;anyall1=all;lmt=ft
- 候选摘要：HathiTrust《资治通鉴》泰始二年 张永“泗水冰合、弃船步走”检索入口；素材中唯一标“史载”的冻损数字（过半、十之八九）亦应在此段核对

### [candidate] 主力正面相持，另遣机动骑兵截粮道，拖垮补给。
- 出处：《魏书·尉元传》（张永、彭城、粮道条，待补原文）
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93;a=srchls;anyall1=all;lmt=ft
- 候选摘要：HathiTrust《魏书》尉元传 张永 彭城 粮道 检索入口；魏军截粮道与兵力数字（素材全篇未给魏军数）需在此核对

### [candidate] 拒谏发了十五万兵
- 出处：《宋书》（张永、沈攸之北伐兵力条，待补原文）
- 链接：https://read.nlc.cn/
- 候选摘要：国家图书馆中华古籍资源库《宋书》馆藏目录/影像入口；“十五万”兵力数需按《宋书》明帝纪、张永传、沈攸之传核对卷页

## Trajectory

0. model: 我先查库里有哪些相关史料（《宋书》《资治通鉴》等）。
1. list_sources({"query":"宋书"})
   → {"sources":[]}
2. list_sources({"query":"资治通鉴"})
   → {"sources":[]}
3. find_candidates({"query":"薛安都 彭城 北魏"})
   → {"candidates":[]}
4. find_candidates({"query":"慕容白曜 青州"})
   → {"candidates":[]}
5. model: 目录和候选都为空，我换几个关键词再查。
6. list_sources({"query":"魏"})
   → {"sources":[]}
7. list_sources({"query":"沈攸之"})
   → {"sources":[]}
8. find_candidates({"query":"宋书 张永 沈攸之 泰始"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
9. find_candidates({"query":"刘宋 淮北 争夺"})
   → {"candidates":[]}
11. find_candidates({"query":"资治通鉴 泰始二年 泰始三年"})
   → {"candidates":[{"authority":"link_only","id":"cand_2eddf39327c74093b5d2e2a5b848ca60","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：资治通鉴","url":"https://fhac.com.cn/","work_title":"资治通鉴"},{"authority":"link_only","id":"cand_2da34159ceba43118ad8f14a540f2009","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","tit
12. find_candidates({"query":"魏书 尉元 慕容白曜"})
   → {"candidates":[{"authority":"link_only","id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","snippet":"外文/公版全文；Full view 才可引用，当前只作候选","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93
13. find_candidates({"query":"宋书 明帝纪"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
14. list_sources({"query":"通鉴"})
   → {"sources":[]}
16. list_sources({"query":"六朝"})
   → {"sources":[]}
17. list_sources({"limit":12,"query":"史"})
   → {"sources":[{"author":"张廷玉等","id":"src_b66505a8fdcb4d788819fcb475f2626d","passages":1,"source_type":"primary","url":"","volume":"卷三百八","work_title":"明史"}]}
18. find_candidates({"query":"冻死 十之八九 泗水 结冰 弃船"})
   → {"candidates":[]}
19. find_candidates({"query":"义嘉之乱 刘子勋 建康"})
   → {"candidates":[]}
21. list_sources({"query":"宋"})
   → {"sources":[]}
22. list_sources({"query":"书"})
   → {"sources":[]}
23. list_sources({"query":"记"})
   → {"sources":[]}
24. list_sources({"query":"南"})
   → {"sources":[]}
26. find_candidates({"query":"慕容白曜 青州 沈文秀 崔道固"})
   → {"candidates":[]}
27. find_candidates({"query":"宋书 张永传 薛安都 尉元 彭城 粮道"})
   → {"candidates":[{"authority":"link_only","id":"cand_b507b87a34d84c029f8741c68705c155","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title":"中华古籍资源库：宋书","url":"https://read.nlc.cn/","work_title":"宋书"},{"authority":"link_only","id":"cand_2faf563df47b444d9c11a92578bade0d","snippet":"外文/公版全文；Full view 才可引用，当前只作候选"
28. model: 库里入库史料只有《明史》（与本篇无关），《宋书》《魏书》《资治通鉴》都只有候选目录（链接级，原文待补）。我按优先级把六处关键内容绑到候选上。
29. attach_candidate({"candidate_id":"cand_9c6ebae72efd4ef3ac6e716ea3574787","note_quote":"江北的薛安都（彭城）、沈文秀与崔道固（青州）遣使到建康表示归顺。","snippet":"HathiTrust 公版全文检索入口；对应《宋书》薛安都遣使建康归顺、刘彧部分，需 Full view 核对卷页","url":"https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%)
   → {"id":"nc_1f4a6212e03a4d618174a27a351ed1dc","ok":true,"status":"candidate","total":1,"work_title":"《宋书》（薛安都等遣使归顺条，待补原文）"}
30. attach_candidate({"candidate_id":"cand_b507b87a34d84c029f8741c68705c155","note_quote":"刘彧命张永、沈攸之领十五万大军北上“受降”。群臣与萧道成劝阻无效。","snippet":"国家图书馆中华古籍资源库《宋书》馆藏目录/影像入口；“十五万”兵力数需按《宋书》明帝纪、张永传、沈攸之传核对卷页","url":"https://read.nlc.cn/","work_title":"《宋书》（张永、沈攸之北伐兵力条，待补原文）")
   → {"error":"note_quote 必须逐字来自这篇笔记；不要改写"}
31. attach_candidate({"candidate_id":"cand_2faf563df47b444d9c11a92578bade0d","note_quote":"「我正要趁此机会北讨，你觉得如何？」","snippet":"HathiTrust《宋书》明帝纪、义嘉之乱相关检索入口；此为素材转写的刘彧语，文言原文待补核对","url":"https://babel.hathitrust.org/cgi/ls?q1=%E5%AE%8B%E4%B9%A6+%E6%98%8E%E5%B8%9D%E7%BA)
   → {"id":"nc_e1fc60f5599543918e7dc3826bfea3b4","ok":true,"status":"candidate","total":2,"work_title":"《宋书·明帝纪》（泰始年间北讨议，待补原文）"}
32. attach_candidate({"candidate_id":"cand_ff7f51a75d8e4a7d8ed731ee26631980","note_quote":"「我们的大军必然战无不胜，无一遗军。」","snippet":"HathiTrust《资治通鉴》卷一百三十一 泰始二年 张永 彭城 检索入口；素材自称“原文如此”的转写句，原文字样待补","url":"https://babel.hathitrust.org/cgi/ls?q1=%E8%B5%84%E6%B2%BB%E9%80%9A%E9)
   → {"id":"nc_f880262df0904451ba54b53e2bcdcc65","ok":true,"status":"candidate","total":3,"work_title":"《资治通鉴》卷一三一 泰始二年（彭城之役，待补原文）"}
33. attach_candidate({"candidate_id":"cand_bb21ae6016bd41d5abbc87333409d699","note_quote":"张永南撤。大雪降温、泗水结冰，弃船步行；尉元令薛安都背后追击、自率骑兵前拦。宋军大败，张永、沈攸之仅以身免，逃至淮阴收拾残军。","snippet":"HathiTrust《资治通鉴》泰始二年 张永“泗水冰合、弃船步走”检索入口；素材中唯一标“史载”的冻损数字（过半、十之八九）亦应在此段核对","url":"https://babel.h)
   → {"id":"nc_d9872bcf99a344d4a8078fffc4bc9681","ok":true,"status":"candidate","total":4,"work_title":"《资治通鉴》泰始二年（泗水冰合、弃船步走条，待补原文）"}
34. attach_candidate({"candidate_id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","note_quote":"主力正面相持，另遣机动骑兵截粮道，拖垮补给。","snippet":"HathiTrust《魏书》尉元传 张永 彭城 粮道 检索入口；魏军截粮道与兵力数字（素材全篇未给魏军数）需在此核对","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%)
   → {"id":"nc_c155b4efd45247a097934ad6af520e82","ok":true,"status":"candidate","total":5,"work_title":"《魏书·尉元传》（张永、彭城、粮道条，待补原文）"}
35. model: 第 2 条因引号/加粗标记未通过逐字校验，我换一句避开标记的原句重试（这样正好 6 条）。
36. attach_candidate({"candidate_id":"cand_b507b87a34d84c029f8741c68705c155","note_quote":"拒谏发了十五万兵","snippet":"国家图书馆中华古籍资源库《宋书》馆藏目录/影像入口；“十五万”兵力数需按《宋书》明帝纪、张永传、沈攸之传核对卷页","url":"https://read.nlc.cn/","work_title":"《宋书》（张永、沈攸之北伐兵力条，待补原文）"})
   → {"id":"nc_ad9cb8c70bf04ac9b468e24ea092c54b","ok":true,"status":"candidate","total":6,"work_title":"《宋书》（张永、沈攸之北伐兵力条，待补原文）"}