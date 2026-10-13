# Windows runner 诊断后继三角色构建记录（613）

2026-10-13 的限定结果：harness、runner、setup 三个 Windows AMD64 角色均已实际编译完成，三个原 Cargo 进程及各原外层工具均退出0，捕获双流 EOF，无超时。构建入口已完成每个真实前缀的 strict HOME 后验；618 独立实际读回仍在等待，本记录不替代该独立验收。

源码图为2394叶：2393份原冻结基线叶加603唯一诊断叶 `runner_client.rs`，20份原 Cargo.lock 保持原字节。603只增加原 runner PID和库内终止前的退出状态观测日志，未改变DACL、协议、权限、重试、Claim或Start业务语义。577三份程序仅代表旧基线，不能代替613诊断后继产物。

| 角色 | 字节数 | SHA256 |
| --- | ---: | --- |
| harness | 95575040 | `b4dc88f1c0db0b55fd9d0419d18fd519dca1c4ee82bc462b834f351286dbe288` |
| runner | 24376320 | `67a91832ad5e34e7f1b25829f923f633764473451549f6c2fbdc316944c60754` |
| setup | 37720064 | `54b56c7776aa4d59bedbca76acb9bdeebc2551c79cf8a2d20415229738c9ac1f` |

三个命令均使用原 `companions/morrow-codex/qualification/c28-basic-harness/Cargo.toml` 构建根，选择原 package/bin；harness package为 `morrow-protected-checked-vm-harness`，runner/setup package为 `codex-windows-sandbox`。共同参数为 `build --locked --offline --target x86_64-pc-windows-msvc --message-format json-render-diagnostics --jobs 1`，不启用额外feature或tests。目标为独立目录，但依赖缓存明确从已验证旧TARGET独立复制，不能称全fresh构建；三个选定bin均有 `fresh=false` 的实际Cargo产物事件。

新薄HOME种子5589文件；实际编译后5592文件、873目录，原构建入口核对完整声明namespace/Git内容和只在内存读取的SQLite bookkeeping，并绑定分别的真实编译时间窗、命令/环境、原始子进程身份、双流和产物。供应商范围仍是固定vendor元数据门，不宣称全部第三方payload重新冻结。源码图清单SHA256为 `f97c1bf27d2fb01c69a703f0215a719bf20eb58790360919053f8c4d8826628b`。

真实590安全链仍为39个已完成命令后的一次Claim/Start Unknown，35秒启动等待未取得实际启动确认，最后快照位于PipeConnectIn。599管理员无参数入口探针取得预期exit1和双EOF，只说明原程序可进入参数错误分支。已保存原590 Standard现场并恢复指定基线；这不是原owner正常cleanup，原Unknown不升级、不重放。新619目前仅SOURCE，尚未transfer/execute。

完整47步、basic文件允许/拒绝、实际SID/网络见证、join/ACK、owner finish、factory release与正常owned cleanup仍待真实验收。SDK26/G04、通用provisioning-home接口、namespace/持续祖先reparse、全OS后代树、生产及跨平台资格均OPEN，release_eligible=false。此前限定正常路径、565故障隔离、35次Rust合成执行与23项Dart回归保持历史范围，未因本次编译扩大。此报告不上传原始操作日志、机器身份或二进制；没有新的native验收、提交、推送、CI、标签或Release。
