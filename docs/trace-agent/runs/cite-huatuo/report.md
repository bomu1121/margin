# Citation agent probe

- note: 假如华佗真的给曹操“开瓢”了，曹操的头痛能治好吗？ (yn-3306fa8a6a82ae9abc406f72)
- status: max_rounds
- summary: 找到 5 条出处，其中 5 条待补原文
- citations: 5
- duration: 31.8s
- usage: 94446 in / 5636 out / ¥0.0361

## Citations

### [candidate] 《三国志》记载，华佗曾让患者饮下麻沸散之后，进行剖腹断肠、缝腹膏摩的外科手术，病人一月之间即平复矣。
- 出处：《三国志》
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E4%B8%89%E5%9B%BD%E5%BF%97+%E5%8D%8E%E4%BD%97%E4%BC%A0+%E9%BA%BB%E6%B2%B8%E6%95%A3+%E5%89%96%E8%85%B9%E6%96%AD%E8%82%A0;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 但正史中确有军医给关羽做了手臂的外科手术，关二爷没留下什么后遗症。
- 出处：《三国志》
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E4%B8%89%E5%9B%BD%E5%BF%97+%E5%85%B3%E7%BE%BD%E4%BC%A0+%E5%88%AE%E9%AA%A8+%E7%9F%A2%E9%95%9E+%E7%A0%B4%E8%87%82%E4%BD%9C%E5%88%9B;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 曹操高陵被发现之后，根据头骨上的蛀牙判断，困扰曹丞相多年的头痛原因很有可能是牙疼。
- 出处：曹操高陵考古发掘报告
- 链接：https://fhac.com.cn/
- 候选摘要：明清档案目录；公开全文有限，当前只作候选

### [candidate] 曹操确有过多次疫区接触史，比如熹平二年持续六年的瘟疫、建安二十二年持续八年的大瘟疫，惨烈到“数十年间生民殆尽”
- 出处：后汉书
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E5%90%8E%E6%B1%89%E4%B9%A6+%E4%BA%94%E8%A1%8C%E5%BF%97+%E7%96%AB+%E7%86%B9%E5%B9%B3%E4%BA%8C%E5%B9%B4;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

### [candidate] 如果真的只是单纯蛀牙，神医华佗不会说这个病无法根治
- 出处：三国志
- 链接：https://babel.hathitrust.org/cgi/ls?q1=%E4%B8%89%E5%9B%BD%E5%BF%97+%E9%AD%8F%E4%B9%A6+%E6%96%B9%E6%8A%80%E4%BC%A0+%E5%8D%8E%E4%BD%97+%E9%BA%BB%E6%B2%B8%E6%95%A3;a=srchls;anyall1=all;lmt=ft
- 候选摘要：外文/公版全文；Full view 才可引用，当前只作候选

## Trajectory

0. model: 我先查库里的正史与考古材料。
1. list_sources({"query":"三国志"})
   → {"sources":[]}
2. list_sources({"query":"后汉书"})
   → {"sources":[]}
3. find_candidates({"query":"后屯 西汉墓 开颅"})
   → {"candidates":[]}
4. find_candidates({"query":"曹操高陵 头骨 蛀牙"})
   → {"candidates":[]}
5. model: 目录里没有直接命中，我换关键词再找。
6. list_sources({"query":"华佗"})
   → {"sources":[]}
7. list_sources({"query":"魏书"})
   → {"sources":[]}
8. find_candidates({"query":"华佗 麻沸散 剖腹"})
   → {"candidates":[]}
9. find_candidates({"query":"考古 颅骨 穿孔"})
   → {"candidates":[]}
11. list_sources({"limit":12,"query":"传"})
   → {"sources":[{"author":"张廷玉等","id":"src_b66505a8fdcb4d788819fcb475f2626d","passages":1,"source_type":"primary","url":"","volume":"卷三百八","work_title":"明史"}]}
12. find_candidates({"limit":12,"query":"曹"})
   → {"candidates":[{"authority":"link_only","id":"cand_e840a669408c44ae976a719151e56df5","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：曹操高陵考古发掘报告","url":"https://fhac.com.cn/","work_title":"曹操高陵考古发掘报告"},{"authority":"link_only","id":"cand_3a1eb9f7c995459c89ce0dfcb2eb26a1","snippet":"馆藏目录/影像；需要按书名检索，
13. find_candidates({"limit":12,"query":"汉"})
   → {"candidates":[{"authority":"link_only","id":"cand_74fa14a511564fa686c3b66f6bdbf8b4","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：后汉书","url":"https://fhac.com.cn/","work_title":"后汉书"},{"authority":"link_only","id":"cand_f63ba78fb64148ed875528e73b62f4be","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","title
14. find_candidates({"limit":12,"query":"医"})
   → {"candidates":[]}
16. find_candidates({"limit":12,"query":"三国志 华佗"})
   → {"candidates":[]}
17. find_candidates({"limit":12,"query":"关羽 刮骨"})
   → {"candidates":[]}
18. find_candidates({"limit":12,"query":"后屯"})
   → {"candidates":[]}
19. find_candidates({"limit":12,"query":"瘟疫 建安二十二年"})
   → {"candidates":[]}
21. find_candidates({"limit":12,"query":"三国"})
   → {"candidates":[{"authority":"link_only","id":"cand_943be285415d450491aba19628d44d83","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：三国志","url":"https://fhac.com.cn/","work_title":"《三国志》"},{"authority":"link_only","id":"cand_65865311dc2048ecba1a9891452c4425","snippet":"馆藏目录/影像；需要按书名检索，当前只作候选","tit
22. find_candidates({"limit":12,"query":"魏"})
   → {"candidates":[{"authority":"link_only","id":"cand_b6eacb6c21cb45d49cd77e279e0958cc","snippet":"外文/公版全文；Full view 才可引用，当前只作候选","title":"HathiTrust：魏书","url":"https://babel.hathitrust.org/cgi/ls?q1=%E9%AD%8F%E4%B9%A6+%E5%B0%89%E5%85%83%E4%BC%A0+%E5%BC%A0%E6%B0%B8+%E5%BD%AD%E5%9F%8E+%E7%B2%AE%E9%81%93
23. find_candidates({"limit":12,"query":"考古报告"})
   → {"candidates":[]}
24. find_candidates({"limit":12,"query":"墓"})
   → {"candidates":[]}
26. list_sources({"limit":12,"query":"史"})
   → {"sources":[{"author":"张廷玉等","id":"src_b66505a8fdcb4d788819fcb475f2626d","passages":1,"source_type":"primary","url":"","volume":"卷三百八","work_title":"明史"}]}
27. read_source({"source_id":"src_b66505a8fdcb4d788819fcb475f2626d"})
   → {"author":"张廷玉等","passages":[{"chapter":"严嵩传","locator":"卷三百八《严嵩传》","original_text":"嵩无他才略，惟一意媚上，窃权罔利。","page":"","passage_id":"passage_ce572f877e4f4ab0a46499aa31416fb6","volume":"卷三百八"}],"url":"","volume":"卷三百八","work_title":"明史"}
29. find_candidates({"limit":12,"query":"颅骨"})
   → {"candidates":[]}
30. find_candidates({"limit":12,"query":"汉墓"})
   → {"candidates":[]}
31. find_candidates({"limit":12,"query":"文物"})
   → {"candidates":[]}
32. find_candidates({"limit":12,"query":"麻沸散"})
   → {"candidates":[]}
34. find_candidates({"limit":12,"query":"发掘"})
   → {"candidates":[{"authority":"link_only","id":"cand_e840a669408c44ae976a719151e56df5","snippet":"明清档案目录；公开全文有限，当前只作候选","title":"第一历史档案馆：曹操高陵考古发掘报告","url":"https://fhac.com.cn/","work_title":"曹操高陵考古发掘报告"},{"authority":"link_only","id":"cand_3a1eb9f7c995459c89ce0dfcb2eb26a1","snippet":"馆藏目录/影像；需要按书名检索，
35. find_candidates({"limit":12,"query":"西汉"})
   → {"candidates":[]}
36. find_candidates({"limit":12,"query":"后汉书 五行志"})
   → {"candidates":[]}
37. find_candidates({"limit":12,"query":"关羽"})
   → {"candidates":[]}
39. find_candidates({"limit":12,"query":"开颅"})
   → {"candidates":[]}
40. find_candidates({"limit":12,"query":"人骨"})
   → {"candidates":[]}
41. find_candidates({"limit":12,"query":"穿孔"})
   → {"candidates":[]}
42. find_candidates({"limit":12,"query":"东汉"})
   → {"candidates":[]}
43. model: 候选库只有《三国志》《后汉书》与曹操高陵报告的链接型候选；我先绑定能对应的四句。
44. attach_candidate({"candidate_id":"cand_57121497435043f7891ae78f38c24cc4","note_quote":"《三国志》记载，华佗曾让患者饮下麻沸散之后，进行剖腹断肠、缝腹膏摩的外科手术，病人一月之间即平复矣。","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（《华佗传》麻沸散、剖腹断肠，原文待补）","title":"HathiTrust：三国志","url":"https://babel.hathitrust)
   → {"id":"nc_e06aeaea7a6d4a2a9ab378aac9d8c8a1","ok":true,"status":"candidate","total":1,"work_title":"《三国志》"}
45. attach_candidate({"candidate_id":"cand_d39ade32375b4a19ae14c45fce53d691","note_quote":"但正史中确有军医给关羽做了手臂的外科手术，关二爷没留下什么后遗症。","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（《关羽传》矢镞破臂作创，原文待补）","title":"HathiTrust：三国志","url":"https://babel.hathitrust.org/cgi/ls?q1=%E4%)
   → {"id":"nc_c3f1fe776905430daf713102fbf12bf8","ok":true,"status":"candidate","total":2,"work_title":"《三国志》"}
46. attach_candidate({"candidate_id":"cand_e840a669408c44ae976a719151e56df5","note_quote":"曹操高陵被发现之后，根据头骨上的蛀牙判断，困扰曹丞相多年的头痛原因很有可能是牙疼。","snippet":"明清档案目录；公开全文有限，当前只作候选（头骨蛀牙，原文待补）","title":"第一历史档案馆：曹操高陵考古发掘报告","url":"https://fhac.com.cn/","work_title":"曹操高陵考古发掘报告")
   → {"id":"nc_0d2c0a70f058460fa42f8fb0f8944aa8","ok":true,"status":"candidate","total":3,"work_title":"曹操高陵考古发掘报告"}
47. attach_candidate({"candidate_id":"cand_a2651da0c5eb4938af0e2c17733fcde4","note_quote":"曹操确有过多次疫区接触史，比如熹平二年持续六年的瘟疫、建安二十二年持续八年的大瘟疫，惨烈到“数十年间生民殆尽”","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（《五行志》疫、熹平二年，原文待补）","title":"HathiTrust：后汉书","url":"https://babel.hathitr)
   → {"id":"nc_4ebf5dde164f48548ac1a858d5e8582b","ok":true,"status":"candidate","total":4,"work_title":"后汉书"}
49. find_candidates({"limit":12,"query":"济南"})
   → {"candidates":[]}
50. find_candidates({"limit":12,"query":"解剖"})
   → {"candidates":[]}
51. find_candidates({"limit":12,"query":"考古学报"})
   → {"candidates":[]}
52. find_candidates({"limit":12,"query":"墓群"})
   → {"candidates":[]}
53. model: 再补一条《三国志·华佗传》的候选（对应“无法根治”那句），然后收尾。
54. attach_candidate({"candidate_id":"cand_019289084c8f43bc86ce7d12165a3d15","note_quote":"如果真的只是单纯蛀牙，神医华佗不会说这个病无法根治","snippet":"外文/公版全文；Full view 才可引用，当前只作候选（《魏书·方技传》华佗，原文待补）","title":"HathiTrust：三国志","url":"https://babel.hathitrust.org/cgi/ls?q1=%E4%B8%89%E5%)
   → {"id":"nc_e0e9e4ced3304e5899d70dbf8284da54","ok":true,"status":"candidate","total":5,"work_title":"三国志"}