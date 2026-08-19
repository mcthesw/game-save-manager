# E2E 测试说明

`pnpm web:e2e`

## 云端同步端到端测试

覆盖旧版云配置/存档升级到新版，以及设备各自与云交互。

### V1 to V2 cutover interrupts, resumes, and stays idempotent

云端已有一份旧版配置和两份存档。升级迁完第一份存档后被打断。

- 云端那份旧配置和两份存档字节不变
- 重启后能接着升完
- 再重启不会多出配置或存档

### two V1 devices cut over, join, and keep V2 device boundaries

1. A、B 本地都是旧版配置，共用云端同一份旧配置和两份存档。A 往云上传存档，B 从云下载同一份
2. A 的本地配置升到新版，B 的本地配置仍是旧版，B 被提示加入
3. B 在加入里选 Keep the cloud version；A、B 本地配置都变成新版
4. 这款游戏在 A、B 上都是 Cloud Backup。A 在存档列表点 Upload to cloud；B 点 Download to this device 后，B 本机多出这份文件，游戏目录还没变；B 再点 Apply，游戏文件才变成这份
5. A 把这款游戏改成 Manual，B 仍是 Cloud Backup
6. A、B 都改成 Multi-device Sync。两边先停在云上同一份，再各自新建并上传，两边都还没吃到对方那份。A 点 Progress diverged, compare，再点 Use this progress：A 的游戏文件变成 B 那份，只有 A 的当前指针移动，B 的指针不变
7. A 删掉当前这份存档：A 的当前指针回到上一份，游戏文件不变；云上这份被标记删除，B 不能再把它拉回本地
8. A、B 重启后，两边的同步模式、当前指针、本机和云上的存档文件，都与重启前一致

### empty cloud creates library then first snapshot uploads

云端文件夹是空的。A 本地已有一款游戏和一份本机存档。

- 点 Create library，再点 Create and switch
- A 的本地配置变成新版，云端出现新版配置，还没有存档
- A 在存档列表点 Upload to cloud：云上多出这一份，字节与本机一致
- B 本地是旧版配置、没有这款游戏。B 点 Join library 后，能在存档列表点 Download to this device 拿到这一份

### per-game upload download disable re-enable

A、B 都已在新版资料库里管理同一款游戏，两边都是 Cloud Backup，云上已有一份双方都有的存档。

- A 新建一份本机存档，在存档列表点 Upload to cloud：云上多出这一份，B 本地还没有
- B 点 Download to this device：B 本地出现相同字节，游戏文件不变，云上这份还在
- A 关掉这款游戏的 Cloud sync 开关：A 的本地配置里这款游戏不再上传，模式仍是 Cloud Backup；B 仍是 Cloud Backup 且开着；云上已有存档还在
- A 此时再新建本机存档：云上不会多出这一份
- A 重新打开 Cloud sync：之后新建的本机存档可以再上传

### sync modes and enable catch-up

A、B 都已在新版资料库里管理同一款游戏，云上已有若干存档。

- A 在概览 Sync mode 里改成 Manual：新本机存档不会自动出现在云上，要手点 Upload to cloud；不会自动 Apply
- A 改成 Cloud Backup，打开对话框里选 Keep in cloud，再点 Enable：之后新本机存档会自动出现在云上；打开时不会把云上已有存档拉到本机；不会自动 Apply
- B 改成 Cloud Backup，打开对话框里选 Download to this device，再点 Enable：云上已有存档出现在 B 本机，游戏文件不变
- B 再改成 Multi-device Sync：当云上只有一份可前进的存档时，B 会自动下载并 Apply 到游戏文件

### evict local or cloud copy without deleting snapshot

云上和 A 本地都有同一份存档。

- A 在存档列表点 Remove local copy：本机文件没了，云上这份还在，存档记录还在
- A 再点 Download to this device：本机文件回来，字节与云上一致
- A 点 Remove cloud copy：云上文件没了，A 本机这份还在，存档记录还在
- B 不能再 Download to this device
- A 再点 Upload to cloud：云上文件回来

### keep local instead of taking the other device save

A、B 都已在新版资料库，这款游戏两边都是 Multi-device Sync。两边先停在云上同一份，再各自新建并上传。

- 概览上出现 Progress diverged, compare，不能自动套用任何一边
- A 在比较里选 Keep this device：云上仍有 A、B 各一份；A 本地游戏文件仍是 A 自己的；B 当前指向的仍是 B 那份

### download all missing cloud copies

A 本机缺若干云上已有的存档。

- A 在共享存档历史点 Download all to this device：缺的那些出现在 A 本机，已有的不重复，游戏文件不变
- 中途打断后再点 Resume download：能从停下的地方拉完

### permanently delete shared game

A、B 都已在新版资料库里管理同一款游戏，云上有这款游戏的存档。

- A 在概览这款游戏旁点 Permanently delete shared game：云上这款游戏的配置和存档都没了，A 本地这款游戏也不再被管理
- B 下次检查云端后，不能再把这款游戏或那些存档拉回来

### remove other device from library

A、B 都在同一份新版资料库里。设置页的 Library devices 列出两台设备。

- A 对 B 点 Remove device：B 从 Library devices 消失；双方已经上传的存档还在

### protect automatic snapshot and set retention

A 已在新版资料库，这款游戏有若干自动存档。

- A 在某一份自动存档上点 Keep：这份带上保护，不会被自动清理算进去
- A 在自动存档设置里打开 Shared retention limit，填一个较小的数并保存：超出的未保护自动存档从云和本机消失，被 Keep 的那份和当前指针还在

### repeated upload download round trips stay consistent

A、B 都已在新版资料库，这款游戏两边都是 Multi-device Sync。连续三轮：

- A 改存档、新建并上传；B 下载这份并 Apply，B 的游戏文件变成 A 的内容
- B 改存档、新建并上传；A 下载这份并 Apply，A 的游戏文件变成 B 的内容
- 每轮结束后：云上和两台本机都留有每一轮的存档文件，两边游戏文件一致，两边的当前指针各自指向自己最后上传的那份

### broken library reset and recreate

A 已在新版资料库，本地还有这款游戏。云端新版配置被删掉，只剩残缺对象。

- A 检查后不能当正常资料库用
- A 点 Reset and recreate 并输入 yes：云端按 A 当前游戏列表重建一份新资料库，残缺对象没了

## 杂项测试

### A/B Hosts isolate device id, token, port, and browser traffic

同时开两套本机：

- 设备 ID 不同
- 本机配置目录不同
- A 的本机配置里没有 B 的设备配置
- 用 B 的 API token 访问 A 的本机接口会被拒绝
- A 的界面只读写 A 这份本机配置，B 同理

## 本地操作端到端测试

云端关闭的单机环境，覆盖不依赖云的玩家路径。

### main path: create, apply latest, apply old snapshot, confirmations, delete

- 新建两份存档（第二份带描述），游戏文件与当前指针跟随
- 取消 Apply latest 的确认框：什么都不变；确认后游戏文件变成最新存档
- 应用一份旧存档：游戏文件回到旧内容，当前指针指向旧存档
- 删除一份存档：记录和压缩包一起消失，当前指针按规则回退

### snapshot tree: branch on apply-then-create, set head, delete head fallbacks

- 应用旧存档后再新建：新存档挂在旧存档下，形成分支，分支视图可见
- 删除分支头：当前指针回退到最近的存活祖先；子节点重新挂到存活祖先

### save units: disable skips backup and restore; delete-before-overwrite clears extras

- 停用某个存档单元后：新建存档不包含它，应用也不碰它的文件
- 单元目录里多出的无关文件：不开"覆盖前删除"时保留，打开后被清掉

### edit save path: apply follows the current path, invalid path fails then fix works

- 在"View managed files"里把存档路径改到新位置：Apply 落到新路径，不回到旧绝对路径
- 路径无效（父级是个文件）：报 Recovery failed，当前指针不动；改好后同一份存档能正常应用

### apply preserves file and nested directory mtimes

- 应用存档后：文件和嵌套目录的修改时间与备份时一致（秒级）

### restore permissions matrix (POSIX chmod, Windows hidden/read-only)

- POSIX（CI Linux）：恢复目录/文件的权限位按预期落地
- Windows：覆盖隐藏文件两种模式都成功；覆盖只读文件在不开"覆盖前删除"时失败且内容不动，打开后成功

### extra backups: created on apply, undo restores content and head, retention trims

- 带着未保存进度应用旧存档：先自动留一份额外备份
- 点 Undo last apply：被覆盖的进度恢复，当前指针回到应用前
- 在 Extra backups 抽屉里 Apply 一份额外备份：只改游戏文件，不动当前指针、不新建存档
- max_extra_backup_count=2 时连续应用：只留最新两份额外备份
- 关掉 Extra backup before apply：应用不再留额外备份，Undo 按钮禁用并带提示

### extra backups: deleting one from the drawer（当前红，等产品缺陷修复）

- 抽屉里点 Delete 弹出的确认框被抽屉遮罩压住点不到（与共享 retention 抽屉确认框同一层级缺陷）。修复后：确认删除，额外备份精确减一

### batch delete removes selected snapshots and rewires the tree

- 勾选链中间两份存档批删（输入 yes 确认）：幸存者重挂到最近存活祖先，压缩包删除，当前指针不动
- 勾选当前指针所在存档及其父级批删：指针回退到最近存活祖先

### compression presets all capture and restore correctly

- Store/Fast/Standard/Best 四档逐一：建出来是 7z，应用后内容与备份时一致，文件修改时间保留

### legacy zip backup from an old version restores and joins the tree

- 旧版应用留下的扁平 zip 存档（无注释、根目录平铺、legacy head 字段）：列表里可见可应用，内容恢复，当前指针指向它
- 在它之后新建存档：新存档挂到这份旧存档下

## 未覆盖

- 两台设备同时改云上同一份清单
- S3、WebDAV
- 自动存档定时器自己触发清理（本地自动 retention 只有桌面定时器入口，web 宿主无此玩家路径）
- 换云端位置时带走未完成的升级进度
- Stop managing here、隐藏游戏（界面没有入口）
- Windows 注册表存档单元的备份/恢复（平台限定且有副作用）
