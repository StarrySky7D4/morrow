# 插件选择与启用注册表

这是不可变 `.mplugin` 安装目录之上的原生持久状态底座，提供 `morrow_core::plugin_package::registry::Registry`。它还没有接入工作台插件管理页面、运行实例停止与撤权协调。当前包执行适配器仍可由可信代码直接调用，因此不能把本模块描述为全局执行隔离或完整插件管理器。

## 状态与可信接口

`Registry::open(registry_root, catalog)` 获取宿主管理目录的协作文件独占锁，加载 `selection.morrow`。同一目录只能由一个参与该协议的管理器持有；锁文件持续存在，通过操作系统文件句柄而非存在性判断所有权。Windows 活跃锁文件不共享删除权限。调用者不得将包内字段作为目录路径。

| 操作 | 行为 |
| --- | --- |
| `select(digest, expected_revision)` | 从 Catalog 重新校验不可变归档。首次选择默认禁用、零批准；同摘要为无操作。替换同 ID 必须具有更高 SemVer 优先级，构建元数据不算升级。升级再次禁用，批准能力仅保留旧批准与新声明交集。 |
| `approve(id, expected_digest, set, expected_revision)` | 单独的可信批准决定；确认必须绑定当前不可变包摘要，仅接受当前包声明的子集。没有来自 guest 的同名接口。 |
| `set_enabled(id, expected_digest, bool, expected_revision)` | 保存未来启动选择，要求确认摘要仍是当前选择，启用前重新校验包。不发放对象授权。 |
| `resolve_enabled(id)` | 仅在启用时返回重新校验的包与批准上限快照；不是执行许可，不能缓存后假定永远有效。 |
| `remove(id, expected_revision)` | 删除选择和批准，不删除安装包、附件、卡片或其他用户内容。再次选择从禁用、零批准开始。 |
| `selection / selections / revision` | 读取当前状态；非变更操作及相同状态不增加修订。 |

所有变更都要求预期全局修订与当前修订相同，不匹配在写入前返回 `RevisionConflict`。批准与启用还要求预期摘要一致。旧确认不能复活已撤销的同摘要批准，旧移除不能删除后续的新选择；发生冲突必须重新读取并取得新的可信决定，不应盲目重试。

批准集合仅为宿主后续授权的上限，不是可转让 grant。可信集成者必须在创建连接、发放具体对象授权时取此集合与其他政策的交集，并在禁用、升级、批准收窄时停止或撤销既有实例。当前模块不持有实例句柄，无法自动完成这些操作。直接使用低层 Catalog、PreparedPackage 或 HostRuntime 的可信代码尚未受本注册表强制门控。

## 固定持久契约与失败语义

`core/schemas/plugin_registry.proto` 固定 schema v1，容器 magic `MORROWG1`，沿用核心 SHA-256 校验的 Protobuf＋LZ4 封装。最多 1024 项选择、解压原文 512 KiB、每项至多 7 种批准能力，文件读入也受压缩容器上限限制。摘要检测损坏，不证明作者或防篡改。

v1 状态采用规范 Protobuf 编码，包含不认识的字段、重复标量编码、不认识的版本／能力时拒绝加载并保留原文件，避免静默丢弃未来字段。选择按 ID 排序，重复 ID／重复批准均拒绝。打开时重新验证所有被选中的安装包；损坏或丢失会拒绝整个注册表加载，不自动重置状态或恢复旧版本。

变更先完成校验、编码，在同目录写入临时文件并同步，随后以原子替换发布；只有替换成功才更新内存修订和选择。发布成功后没有可能失败的 I/O，因此普通错误返回发生在发布前，旧状态保持。未发布的临时文件由正常清理处理；进程被杀可能留下无效临时文件，它们不是权威状态。不宣称已完成文件系统断电、目录项持久性或网络文件系统资格验证。一个拥有宿主目录任意写权限的外部进程、旧宿主及跨设备状态协调也不属于该协作锁保证。

注册表全体状态损坏或选中包缺失时保留证据并失败，不允许自动清空。恢复／修复工具、历史选择和撤销记录、依赖锁定、签名及发布者信任、升级中的运行任务收束、用户可见安装管理均为后续工作。

## 当前验证

在 Windows 使用合成临时目录完成：10 项集成测试（持久重开、升级默认禁用、批准交集、非法升级／越界批准、禁用／移除保留文件、重复管理器锁、包损坏、发布前失败、状态超限、SemVer 构建元数据、旧摘要批准／启用确认拒绝、旧修订不能复活撤权或删除新选择）；另有 2 项模块测试覆盖未知／重复字段、未来版本、非法修订、重复选择和未知能力。测试无需真实用户内容库。

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --target-dir build/plugin-lifecycle --test plugin_registry
cargo test --offline --locked --manifest-path core/Cargo.toml --target-dir build/plugin-lifecycle --lib plugin_package::registry
cargo clippy --offline --locked --manifest-path core/Cargo.toml --target-dir build/plugin-lifecycle --all-targets -- -D warnings
```

这些测试只证明原生注册表的上述行为，不证明实际插件 UI、运行实例撤销、移动端／浏览器支持或断电恢复。
