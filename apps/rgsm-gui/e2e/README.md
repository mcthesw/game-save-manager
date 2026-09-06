# E2E 测试说明

`pnpm web:e2e`

## 本地调试循环

修改功能时先跑对应单元测试，再从仓库根目录选一个相关 E2E 场景：

```powershell
pnpm web:test
pnpm web:e2e local-main-path.spec.ts
```

运行前请停止占用 5173 端口的开发服务器。端口被占用时，测试会报错，不会接管或关闭原有进程。E2E 使用临时配置和本地 Fs 云目录，不需要个人存档或云账号。默认的 `pnpm web:e2e` 只运行 browser 项目，通过真实 Rust HTTP API 验证业务，不打开桌面窗口，也不注册托盘或全局快捷键。配置迁移本身不发送系统通知，升级提示由正常桌面启动负责，HTTP-only 宿主保持静默。

实际窗口生命周期测试单独使用 `pnpm web:e2e:desktop`，会启动和关闭隔离配置的桌面窗口，请在允许弹窗时运行。Windows CI 使用 Playwright 的完整项目集合，仍包含此桌面覆盖。

每次 E2E 的 global setup 都让 Cargo 检查代码是否需要重编译，构建一次宿主和校验工具，所有 worker 复用本轮产物，不复用未经检查的旧二进制。本地编译默认最多两个 Cargo jobs，Playwright 保持单 worker；显式设置的 `CARGO_BUILD_JOBS` 和 CI runner 默认并发不变。

Vite 由整轮测试持有，worker 重启或重复运行场景时不重新启动，结束时统一关闭。各场景负责关闭自己的宿主和浏览器上下文，部分启动失败也会清理已经启动的资源。HTTP 启动探测和操作请求有超时，失败场景保留临时目录及宿主日志。

Linux 还需要 `xvfb-run` 和 `dbus-run-session`，CI 已安装相应依赖。每个宿主使用独立的临时 D-Bus 会话，退出时清理该宿主的进程组，不共享桌面会话。

需要定位耗时时，启用分阶段日志，区分构建、Vite、HTTP host 和页面加载：

```powershell
$env:RGSM_E2E_TIMINGS = '1'
pnpm web:e2e local-main-path.spec.ts
Remove-Item Env:RGSM_E2E_TIMINGS
```

CI 自动输出这些阶段耗时。Linux 全量套件分到两个独立 runner，各自仍只有一个 worker，避免共享端口和宿主状态；`GUI Cloud Fs E2E` 汇总检查只有在两个分片都成功时才通过。失败报告按分片分别保存。Windows 平台场景保留原有覆盖。

需要复现某个 CI 分片时可用 `pnpm web:e2e --shard=1/2`。调整超时、增加重试或在本机同时运行多套 E2E 不是默认加速手段；推送前仍须运行改动涉及的场景，发布前运行完整套件。

## 功能与迁移覆盖

这些场景使用真实 Rust HTTP API、SSE、临时存档文件和 Fs 云目录，不替换业务接口为 mock。具体断言以同目录的测试文件为准，UI 不应自动下载或应用远端进度

| 发布边界           | 主要场景                                                                                                   | 验证内容                                                                                                                          |
| ------------------ | ---------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 1.7/1.8 → 1.9      | `local-released-upgrade`、`local-v1-8-upgrade`                                                             | 启动后不手工修配置；保留 Save Unit 类型、ID、旧分支位置和收藏；旧 ZIP 恢复、新建、重启后再恢复；旧归档字节不变                    |
| 覆盖方式与恢复失败 | `local-save-units`、`local-path-relocation`、`local-failure-surface`、`local-restore-permissions`          | 直接覆盖保留额外文件；删除后覆盖移除额外文件；禁用单元不参与；缺失、损坏或不可写目标报错                                          |
| 本地备份与自动化   | `local-main-path`、`local-extra-backup`、`local-auto-backup`、`local-compression`                          | 新建、恢复、额外备份、撤销、清理和压缩；同名游戏定时器独立运行；后台备份不重置设置草稿                                            |
| 游戏与快照身份     | `local-game-identity`、`local-favorites`、`local-ludusavi-import`、`cloud-snapshot-choice`                 | 同名游戏不会串路径、收藏或快捷操作；迟到的路径检查不覆盖导入选择；快照 ID 与时间、创建来源分离                                    |
| 旧云端迁移         | `cloud-library-cutover`、`cloud-library-two-v1-devices`                                                    | 旧对象字节保留；中断后继续；重复连接不重复迁移；设备设置与进度各自独立                                                            |
| 发现与定义选择     | `cloud-library-late-game`、`cloud-local-games`、`cloud-definition-choices`                                 | 自动显示云端游戏；本地独有游戏离线仍可用；同 ID 定义不同时由玩家选择，刷新不代替选择                                              |
| 元数据刷新         | `cloud-catalog-read`、`cloud-refresh`                                                                      | 读取不迁移或传输归档；并发刷新共用请求；旧连接响应不写入新状态；前后台刷新不覆盖编辑草稿                                          |
| 玩家确认同步       | `cloud-sync-modes`、`cloud-per-game-sync`、`cloud-progress-prompts`、`cloud-keep-local`、`cloud-ping-pong` | Manual 只发布记录，Cloud Backup 增加上传，Multi-device Sync 增加提示；Later 去重；下载不改游戏存档，Apply 必须由玩家选择          |
| 副本与全端删除     | `cloud-evict-copies`、`cloud-library-two-v1-devices`、`cloud-deleted-ancestor`、`local-delete-recovery`    | 删除本地/云端副本保留历史；最后副本明确警告；后续自动上传不恢复主动删除的云端副本；全端删除收敛且不动游戏存档；已删祖先不阻断后代 |
| 设备移除与重连     | `cloud-remove-device`、`cloud-reset-library`                                                               | 普通后台写入不能重新注册被移除的设备；用户确认后重连；其他移除标记、已有归档和游戏存档不变                                        |
| 保留策略与批量传输 | `cloud-protect-retention`、`cloud-download-all`、`local-batch-delete`                                      | 手动、当前进度和受保护快照不误清理；下载全部是明确操作；批删位置回退                                                              |
| 宿主边界           | `host-isolation`、`harness-lifecycle`、`desktop-window-lifecycle`                                          | HTTP token 与设备数据隔离；只清理测试拥有的进程；桌面专用场景验证销毁/重建、单实例唤醒、有效 API runtime 和退出设置               |

Rust 侧的 `config_upgrade_compatibility` 和 `cloud_sync_v2_fs` 集成测试补充历史格式与文件传输验证；核心单元测试覆盖解析器、缺失/多目标恢复边界、元数据短暂不完整读取以及设备重连校验

## 发布前检查与证据边界

先完成相关场景，再在同一个最终提交上跑完整 browser 套件及工作区检查。GitHub 四类门禁为 Backend Tests、Style & Lint、GUI Cloud Fs E2E（两分片汇总）和 Windows Release E2E。绿灯只证明实际执行的场景，不代表以下边界已验证

- 兼容目标为正式 1.7/1.8 升级到 1.9；1.7 目前依据历史源码结构制作 fixture，没有已确认的发布包。V2 ZIP 已用于 1.8，不能当作仅有 1.9 测试版需要兼容
- 不默认覆盖所有历史 1.9 中间配置或直接降级回 1.8；也不承诺跨文件和云端的原子事务或自动回滚
- Fs 覆盖不能替代真实 S3/WebDAV 账号和网络条件验证；短暂 JSON 覆写回归不等于任意多写者一致性保证
- HTTP-only 测试不验证系统通知实际送达、托盘、全局快捷键或 WebView 行为；桌面专用场景应单独执行，不能靠 browser 绿灯替代
- Windows 注册表存档单元的真实备份/恢复仍未在 E2E 中执行，以免修改测试机注册表
- Stop managing、隐藏游戏以及更换云端位置时迁移未完成记录的完整交互，尚无专门 E2E
