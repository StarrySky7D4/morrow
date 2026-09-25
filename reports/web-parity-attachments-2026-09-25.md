# 浏览器附件链路增量

本阶段将正式 Web 工作区的附件导入、内容读取、预览和下载接到同一个 Rust 宿主。云端仍只提供静态程序资源；文件通过设备内 Worker 消息传递，不上传服务器。完整 Windows 功能对齐仍未完成。

## 实现

- `core-web/src/device_files.rs`：使用 Worker 内的 FileReaderSync 分块读取浏览器 Blob，交给现有 `Workbench::import`，由相同的元数据校验、暂存和提交规则接纳附件。导出经现有 `Workbench::export` 验证数据库内容，按块生成不可变 Blob 并计算 SHA256。单文件上限与 UI 的 200 MiB 保持一致；没有将整个文件塞入 128 KiB 协议帧。
- `WorkbenchDeviceFiles`：设备文件适配接口。共享控制器将文件任务排入既有业务/通信队列；关闭或服务归属不明确时拒绝执行。超时不自动重放，也不把超时解释成事务回滚，而是请求原 Worker 完成处理后关闭。
- `versioned_workspace_native.dart` 与旧内容读取：浏览器不再调用 `dart:io File` 或原生预览目录。确认附件长度、修订和摘要后使用当前宿主拥有的预览；重开必须重新从内容库导出。关闭清理当前会话预览。
- `texture_storage_web.dart`：新素材键改用随机 UUID，加上只允许新增的 IndexedDB 写入，避免同毫秒并发选择互相覆盖；旧键继续可读。宿主预览采用单独的会话注册表，不写入旧媒体数据库。预览不可变，过期时明确报错。
- `file_access_web.dart`：下载直接引用已验证 Blob，不再先复制到完整 Dart 字节数组再复制回 Blob。

## 证据

- `build/web-attachment-channel-verification.log`：Chrome 154 实际 Worker、生产 Wasm 与原工作台插件验证通过。导入 5 MiB + 7 字节附件后删除选取副本，确认内容库读回摘要相同；V1/V2 迁移及编辑保留附件；独立 Worker 重开后原字节相同；不存在的附件拒绝导出且宿主继续可用；关闭后旧预览失效。查询、设置、任务、中文长草稿及审批原用例仍通过。
- `build/device-preview-web-tests.log`：Chrome 两项测试通过，覆盖 12 个并发同名选择的独立内容，以及不可变预览、关闭失效和旧素材互不影响。
- `build/web-attachment-native-regression.log`：Windows 7 项真实宿主、混合内容与设置/保护回归通过；该轮缺少关闭夹具而跳过的 3 项没有计为通过。
- `build/web-attachment-close-regression.log`：补齐 Python 关闭夹具后，3 项关闭回归全部通过，包括原请求隔离、等待超过五秒及非零退出。
- `build/web-attachment-final-analyze.log`：8 个修改相关 Dart 文件分析通过。Rust/Wasm 检查、正式 release 构建与 JavaScript 语法检查通过。
- `build/web-attachment-production-verified.log`：正式页面使用真实文件选择器导入附件、保存、整页刷新、下载原件逐字节比较通过；同时重跑旧数据单独打开、新旧库并存选择和缺失身份保护场景。测试使用隔离浏览器 profile 和生成的测试文件。

## 仍需完成

本次确认了基本内容附件链路，未覆盖 200 MiB 边界、20 个大附件、空间不足、事务中断及全部媒体预览。当前查询仍复制整个数据库到最多 128 MiB 的内存快照，大附件会扩大数据库，因此大库查询是明确差距，不能把 5 MiB 验证外推到完整上限。

持久草稿附件、导入决定和恢复接口仍有原生路径依赖，尚未由此设备桥接覆盖；字体、背景/音乐设置中的本地素材定位、剪贴板平台差异、旧库迁移、备份恢复与云端发布/升级也未完成。旧选择缓存的完整生命周期和配额治理仍需继续验收。
