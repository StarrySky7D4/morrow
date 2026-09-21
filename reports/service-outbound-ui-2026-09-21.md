# 服务出站端点选择面板与启动协议

日期：2026-09-21。基于 `56332b5` 的原生服务准入，版本仍为 `0.1.9-test.52+56`。

## 实现

私有宿主 `ServiceRunStart` 增加最多 8 个端点引用与预期修订，Rust 解码后交给上一轮的原拥有者准入。原省略字段调用保持空选择，不能因已保存端点或 Registry 批准而获得隐式出站权限。新增字段只属于应用私有协议；冻结的 guest IO / C/C++/Rust SDK 文件未修改。

Dart 启动模型复制并冻结引用、修订和选择列表，校验重复、零值、数量与 UInt64 边界；编码前拒绝不合法输入，完整保存大于有符号 64 位范围的合法修订。已经提交的 request 属于 ServiceRunSession，卸载页面、切换语言或发生 Unknown 不会更换端点、批准新资源或重发请求。

Flutter 服务面板新增同风格的端点复选项，按当前插件 ID/摘要、声明/批准的 HttpRequest 与必要 CredentialUse，以及端点启用/有效期过滤。加载完整有界快照后才提供选择，不默认选中任何端点。刷新遇到旧修订、撤销/到期或读取失败时保留原选择并阻止使用；用户可明确重新选择当前版本或清空。空选择仍允许普通服务启动。正在使用的启动尝试另行显示其固定引用和修订。

端点分页检查完整 snapshot、单页/总量限制、引用排序和游标前进；所有保留的字节与政策字段均复制。后端替换后的旧结果不能更新新页面。新增中英文文案通过权威 ARB 构建器生成 typed messages、PB/LZ4 资源及 pins。

## 验证

- 五文件 Dart 组合 **71 项通过，0 跳过**：service_run_control、service_run_session、service_endpoint_catalog、service_run_manager、service_run_transport_native。管道用例设置 `MORROW_CLOSE_TEST_PYTHON` 后实际执行，但其中 Python 是受控协议替身，不代表 Rust 网络效果。
- 原生工作台服务 **14 项通过**：私有启动协议携带所选端点后，由实际 Rust guest 执行真实 HTTP；重复/超量/零值/错误修订拒绝后，同一未使用 submission 仍可明确启动。原有直接准入、重放与回收回归保持通过。
- 语言资源 **4 项通过**，包含实际 bundled PB/LZ4 验证；i18n 产物与 Dart Cap'n Proto 再生成检查通过。
- 八文件 Flutter 分析无问题，workbench lib 严格 Clippy 通过；修改文件格式与 diff 检查通过。原生 lib test 保留既有 `prepare_write` 未使用警告。

新增界面断言覆盖 360 像素窄屏下最多 8 项、缺失批准/外来包过滤、启动中的选择冻结与双语重挂、端点修订变化后的明确重选、刷新失败后的明确清空，以及旧后端迟到结果丢弃。分页测试包含 512 页上限、重复游标、顺序和 snapshot 漂移。

修正了 Windows 原生测试服务器的一个竞态：nonblocking listener 接受的 socket 必须显式转回 blocking 模式，才能按预期读取完整请求。先前偶现的空请求属于测试夹具，不通过增加生产超时或削弱回包校验解决。

日志：忽略目录 `build/service-endpoint-ui-regression.log`、`build/service-outbound-protocol-tests.log`。原生测试 guest 的构建/环境见[说明](../workbench_host/tests/fixtures/service-outbound/README.md)。UI 测试使用模拟后端；Rust 协议测试使用真实网络，两个验证范围分别成立，尚未新增完整 Windows 窗口出站验收。

## 辅助编码与下一步

SubagentBridge 的 GLM/max 提供模型与分页测试草稿，主代理修正实际 API、错误类型、伪造构造器、游标和断言后应用。DeepSeek/max审阅请求冻结；其提出的“检查与复制之间被异步刷新打断”不适用于当前同一 Dart event-loop 的同步区间，且原生仍独立核对记录修订，因此未额外引入无依据的锁或授权 token。执行成功不替代主代理审核与测试。

下一项：插件获取明确允许资源的引用契约、运行中端点/凭据撤销与完成/停止竞争，以及完整 Windows 窗口验收。大帧分段、应用服务 TLS、Unknown 核对、文件系统和三语言 IO SDK 仍继续推进。

本轮未重建 Windows 完整应用、未推送、未发布。私有协议摘要已变化；后续运行预览必须一起重建 Flutter 与原生宿主，不能把新 Dart 源码与旧宿主拼接使用。
