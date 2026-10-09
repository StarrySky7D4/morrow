# v30 原音乐导入缓存的中断清理恢复

2026-10-09：修复实际 `MusicFiles.ets` 在部分删除后恢复丢失原 entry 的问题。公开 `PreparedMusicFile` 字段与落盘 metadata/request schema 保持不变；没有修改 Library、Controller、Index、Native、SDK、Git 远端或设备。

原场景：调用方已核对完整 Ready 导入，`release(original)` 删除 data 成功，但 metadata/request 删除失败。随后 `recover()` 因 data 缺失而删掉原 registry；调用方仍保留 qualified Ready 计划和原对象，却无法再次 `release(original)`，清理永久报身份变化。

现在 `MusicSpool.releaseStarted` 仅在原私有目录、FD 状态、允许的普通子文件检查全部通过且将要 unlink 时置位。原释放失败的 process-local entry 保留同一个 PreparedMusicFile 对象与签名；`recover()` 不把它列为完整可导入缓存，不重新构造对象，不删除其 registry。`importPrepared()` 拒绝此清理状态；原调用方的明确 `release(original)` 可以继续清理剩余允许文件及空目录，成功后才撤销 entry。release 前置检查因未关 FD 或未知内容失败，不授予 cleanup 状态。

**22/22 PASS**，0 fail/cancelled/skipped/todo，**1070.2741ms**；四项实际产品/工具输入和三项 Node/SDK TypeScript 输入前后哈希完全一致。见 [结果](music-files-cleanup-result.json)、[精确输入](music-files-cleanup-inputs.json)、[完整 TAP 日志](music-files-cleanup-tests.log)。当前 `MusicFiles.ets`：**29,363 bytes**，SHA-256 `93613ABDEDBBFB811BC48A0366643825A48E9FAC622FF52A275D5915BE6EC34D`；日志 SHA-256 `466014C661C47DB7386594272FB054D4D5458FB3FA97F8FC068B41428BE38DDD`。

七项新增检查覆盖 metadata/request unlink 中断、首个 data unlink 拒绝、empty-directory 删除失败、retry_io 后 recover 不列为完整缓存、原对象 identity 保留及显式 release 重试、禁止再发 FD import、未知子文件保留、fresh adapter 不从缺 data 的重启残留伪造 cleanup 身份。原 FD close、完整请求 fsync、150 MiB capture、UTF-8 完整歌词、对象身份、文件命名空间与错误保留检查继续通过。测试执行实际 ETS；fs/picker/native prepare/export/import 为可控主机 provider，不能视为实际 Core Ready 或设备成功。

Controller 负责在释放前核对本次原请求的实际 Ready、完整 retained bytes 和回执。本文件层专项不授予该提交资格；它仅在先前持有 exact object 且开始过明确 release 的本进程范围内保留清理所有权。对应 Controller 的实际源码组合恢复另见 `music-workbench-source-integration-a2-*`，其 provider 限定由相应报告说明。

**OPEN：完整进程重启后的部分清理残留协议。** 本次状态不写入磁盘；新实例遇到缺 data 或 metadata 的目录继续保留并报告诊断，不返回完整 spool，也不能用旧普通 JSON mint 新 cleanup entry 或盲删残留。新增 durable cleanup intent、实际 Ready 重核对与重启专用清理入口尚未实现。SDK、HAP、设备声音、codec、后台行为、完整音乐 UI 和 Windows parity 不属于此专项资格。

复现工具：以本机 DevEco Node 执行 `hmos/tool/music-files-cleanup-check.cjs`；已有标签证据拒绝覆盖，复查需传新 ASCII 标签。该工具只执行限定 host suite 和记录输入身份，不构建、安装或发布。
