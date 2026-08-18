# GUI Cloud Fs E2E

`pnpm web:e2e`

单 worker、真实网页 + 双 Host + OpenDAL Fs。失败即产品缺陷，不改业务语义。

## Tests

**host-isolation** — A/B Host 隔离  
Device ID / token / port / app-data / browser 请求不交叉。

**cutover** — V1→V2 中断恢复

1. 迁移 1 份 archive 后注入中断：progress 落盘、无 `v2/namespace.json`、V1 字节不变
2. 同 app-data 重启：inspect = `cutover_required` + `resumable`，UI 完成 cutover
3. 再重启：`active`，对象不重复；legacy upload 返回 `V2CloudLibraryActive`

**two-devices** — 两台 V1 → A cutover → B join → V2 操作

1. 双 V1 基线，A→B 经 V1 交换 archive
2. A cutover，B 仍 LegacyV1，inspect = `join_required`
3. B join keep-cloud，两端 V2，legacy path 被挡住
4. A 前进，B 下载并 apply 唯一 forward
5. sync mode 只改本机 profile
6. 同 ancestor 分叉，A 必须显式选 B candidate
7. 删当前 head 回退 parent + tombstone，对端 reconcile 不复活
8. 两端重启状态一致

## Unmet

本地 `pnpm web:e2e`（2026-08-18）：

| 项             | 卡点                  | 期望                             | 实际                                                                        |
| -------------- | --------------------- | -------------------------------- | --------------------------------------------------------------------------- |
| cutover #2     | 中断后重启 inspect    | `cutover_required` + `resumable` | `V2 namespace descriptor is absent but V2 objects remain: ["v2/archives/"]` |
| two-devices #6 | 分叉后上传新 snapshot | upload 通过                      | `Archive integrity mismatch`，差 1–2 字节                                   |

因此尚未跑到：cutover 完成/幂等/legacy 阻断；two-devices 分叉选择、删 head 回退、对端 reconcile、重启恢复。

host-isolation 已过。two-devices #1–5 在卡住前已通过。
