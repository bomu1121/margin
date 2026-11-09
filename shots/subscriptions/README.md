# UP 订阅空间请求恢复（2026-11-09）

事件：`寒清关下说书人`（mid 27199932）一次 `B站空间请求失败：error sending request`；其他 6 个 UP 正常，手动立即检查成功。

修复：space/WBI/buvid/followings/login 最多 3 次重试（250/500ms），只重试 timeout/connect/request；错误分类显示；瞬时网络失败首次退避 2 分钟。详见 `docs/PRODUCTION.md` §3.7 和 `docs/experiment-log.txt` P66。

| 截图 | 内容 |
| --- | --- |
| [01-recovered.png](01-recovered.png) | 最终构建复检：卡片无错误、显示“没有新视频”、上次检查 2 分钟前、31 分钟后检查 |

复检 DB：`last_error=''`、`consecutive_failures=0`。
