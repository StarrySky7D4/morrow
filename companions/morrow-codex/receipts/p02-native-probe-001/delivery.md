# P-02 新批次：固定源码补齐与真实网络调用点验证

2026-09-28。当前结论：固定 Codex 与 CC Switch 完整源码已核验；Windows 本机资格探针已编译并运行，两个拒绝案例、15 项断言全部通过。**P-02 尚未完成，G0 仍未通过，84 项产品验收未运行。**

## 已完成的真实验证

- Codex 固定提交 `44fe510ce3ee61c8ef623adcbf89b901c73ddd61`：8,697 个文件，985 棵 Git tree，86,130,187 字节；完整树为 `3b868fad63be6ac5db91402b579fab37f587d7d5`。
- CC Switch 固定提交 `846de29c13ac4d65f164db8c15dd5fd58e29f972`：1,322 个文件，133 棵 Git tree，48,415,580 字节；完整树为 `e373c27485681c074ddab7d4f085ee9a6667b891`。本批没有编译 CC Switch/Tauri。
- 单独的原生资格 crate 调用未修改的 `codex_api::ResponsesClient::stream_request`，经原 `EndpointSession::stream_encoded_json_with` 到达具体 `HttpTransport::stream` 实现；没有复制 endpoint 或另造会话循环。
- 第一个案例实际截获 POST、SSE/JSON headers 和 241 字节的准备后请求。宿主 `Stream.Open` 帧绑定相同摘要、session、epoch、attempt；显式断开 fixture 返回可识别错误。原入口仅调用一次，无重试，无 CommitRequest。
- 第二个案例通过同一真实入口发送未批准的 `.invalid` 目的地，收到目的地拒绝错误；宿主交换次数为 0，无 CommitRequest。
- Rust/Cargo 1.95.0，Windows `x86_64-pc-windows-msvc`，隔离 Cargo、用户目录、临时目录与目标目录，正式 build/run 均使用 `--locked --offline`，退出码均为 0。
- 编译器报告的可执行文件与实际运行文件摘要一致：`e0f5d6da9587cc3722b98701c52bd23474e935e302bd77b41f4796521aa5c9ca`。
- 编译后重新检查全部 Codex 文件和树，modified/missing/extra 均为 0。原先 17 项冻结输入及 `receipts/handoff.json` 保持不变。

## 来源与依赖边界

两次整包下载均中途断开，原失败归档和回执完整保留。没有把截断压缩包标成成功：只回收长度完整且固定 Git blob 校验通过的成员，再从固定提交补取缺项，最终从全部文件重新构造 Git tree 并与独立 commit tree 比较。

692 个公开 registry 包按上游锁或 host-kit-003 锁校验并展开，作为保守的离线依赖库存。实际解析图为 598 个包：21 个本地包（17 个 Codex 包、宿主 kit、探针和两个固定 fork），577 个 registry 包。与上游锁相比，registry 新增的三个包均来自 003 kit 锁；没有替换成 crates.io 版 WebSocket fork。

两个固定 fork 的原始完整源码保留不动。资格构建副本仅在 tokio-tungstenite 的 Cargo.toml 中，将固定 tungstenite Git 依赖声明改为指向相邻固定副本的路径；所有 Rust 源码字节保持一致。精确补丁及逐文件对照位于 `../p02-dependency-cache-001/build-forks-001/`。独立锁因此把这两个 fork 记录为本地来源，不能将锁中的来源字段宣传成未经修改的 Git 来源。runfiles 的固定完整包子集和许可证也已准备，但没有进入实际解析图。

早期两次离线锁解析失败、一次 VS 环境脚本失败均保留在 `runs/`。实际构建使用已安装工具链的正常 MSVC 自动发现成功；先前失败没有被删除或重写为成功。旧 preparation 与旧依赖路径映射回执属于历史尝试，当前交付索引以本目录新 handoff 为准。

## 本批未证明的事项

这是进程内断开 fixture 的局部调用点验证。没有真实宿主 IPC、成功的 HTTP/SSE 响应、账号登录、模型调用或 OS 网络记录器。不能由这两个案例推断整个程序没有直连、进程或第二存储旁路。

上游 ModelClient/core loop、执行接口接管、LiveThread/ThreadStore 接管均尚未实现。本机资格程序不是 A 或 B 产品程序；native/Wasm 两套产品构建仍为 0/2。没有提交、推送、发布或真实账号验收。

首批 `sources.lock.json` 与 `tools/build_plan.py` 均保持冻结，没有把它们的旧 partial snapshot 偷换成新源码。本批使用独立 source inventory、qualification Cargo.lock 与 runner；原产品构建入口的未实现阶段仍不能据此算通过。

源码获取阻碍已解决。剩余内容是明确的实现与验证工作，不能继续笼统归因为下载失败：

1. 在真实 ModelClient 路径注入 transport，处理默认 Reqwest、WebSocket、认证、模型目录、compact 与后台请求；通过实际记录器验证无旁路。
2. 执行路径需要显式的 Environment 注入和本地 spawn 拒绝，并运行真实 core dispatch 的负面探针。
3. 存储路径先处理 `LiveThread::create` 在 store 调用前收集 Git 信息的旁路，再覆盖 resume/append/persist 与断开拒绝；不能只替换 append。
4. 完整 HTTP 元数据、会话管理/持久确认、进程输入/PTY/控制/输出需要宿主后续合同支持。宿主已单独记录这些需求，003 schema 不由插件复制改写。

详细真实接缝和宿主边界见 `../p02-probe-review-001/decision.md`。所有未覆盖项继续保留为未实现或未运行，不计入通过数。
