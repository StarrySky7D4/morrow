# 失败启动隔离：Dart 接线候选 r002

状态：**SOURCE_NOT_RUN**。本目录是惰性源码候选归档，未激活生产接线。16个内存测试尚未运行；旧r001保持为历史版本，不被此包覆盖。

本次针对r001的一个关闭门缺口作最小修正：启动失败会标记本地业务关闭，但该标记不能证明调用方请求过正常关闭。私有接收路径现在只在`_normalCloseRequested`为真时接受正常Released；最终退出0也必须同时满足该条件。未发送正常kind3时，即使收到同身份Released、实际退出0并出现双EOF，也不能作为正常回收成功。

新增第16个内存反例覆盖上述场景，要求保留初始受验状态、返回Unconfirmed，不生成隔离凭据，也不发送kind3或kind4。其余15个测试正文及另两个生产文件保持原字节。三个生产文件和完整测试文件均在`overlay/`中；`diffs/`保留从r001到本候选的两个精确最小差异。`manifest.json`绑定相对路径、基线与候选SHA-256，`exact-tests.json`列出全部16个精确测试名。

此前候选的失败启动标记、真实接收路径验证、私有深不可变扩展副本、单次空kind4、原supervisor实际退出2及双EOF门继续保留。失败启动隔离凭据仍独立于正常Released：业务Unknown、ownerRetained、normal=false、accepted=false。原始主异常与堆栈及次级关闭记录保留；坏frame或传输失败不能获得成功凭据，不重试或kill。

本地已完成两个独立合成Dart测试树的数据准备：一条使用旧channel，仅计划运行第16项作为预期失败负对照；另一条使用本候选，计划运行全部16项。**这只是数据准备通过，A/B测试均NOT_RUN，不能记为测试PASS。** 未执行Dart、Flutter、pub或VM。

本候选的静态分析、16项实际测试、与完整354 Rust运行时的联合编译及验收均NOT_RUN。源码中的内存FakeProcess不代表真实native或持久owner证明。SDK26/G04与namespace保持OPEN，release=false。本归档不包含本机路径、操作记录、工具缓存、凭据、原日志或恢复授权，也不授予恢复、执行或清理能力。
