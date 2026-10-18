# Windows bootstrap 管道方向修复与验收范围

P 在 bootstrap 服务端写入，C 使用只读客户端。旧服务端采用 INBOUND，
与这条传输方向不兼容。本次改为 OUTBOUND，保留 OVERLAPPED、
FIRST_PIPE_INSTANCE、消息模式、单实例、ACL、继承规则、只读客户端、
协议及 owner/清理逻辑，并为两个访问模式命名。
参见 [Microsoft 管道访问模式](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-open-modes)。

## 已验证

- 原开发虚拟机中的独立 Win32 探针对旧模式实际观察到只读客户端打开失败、
  error 5；新模式完成异步 Connect、32 字节写入/读取和逐字节比对。
  六个自有有效句柄 checked-close，探针实际退出码 0，新会话关闭。
- Source048 全量 3284 个输入经独立哈希核对，只有 asuser_parent_native.rs
  与 Source047 不同。其余 3283 个文件、83 个 Cargo.toml、
  20 个 Cargo.lock、6 个 pubspec.yaml、5 个 pubspec.lock 和
  1211 个 upstream 输入保持原字节。
- 当前全新构建环境下，P、C、R、S 离线锁定构建均 Cargo 0。
  C 编译输入绑定实际 P 哈希；S 只读 .c28a1 段的 80 字节表与实际 C/R
  哈希相符。原始编译依赖文件核对了绑定顺序。
- 新源码的原生测试程序在原虚拟机执行五个既有 parent_native 状态用例：
  5 passed、0 failed、510 filtered out。单元测试使用此前批准的独立测试锁，
  未替换生产锁。
- 更新 Source048 角色绑定及一个既有长度期望值后的控制器，通过全部
  73 个既有纯测试。上述两项新测试程序均取得原进程实际退出码 0、双 EOF、
  自有捕获句柄 checked-close、新会话关闭；managed reader join 不证明 native TLS。

Source048 输入树哈希（排序相对路径、字节数、文件 SHA 的 UTF-8 记录）：
cbcab508304e241f6f530081b6cf8c85d22712cc01f576edcebbdea0f8cab696。

## 仍待验收

独立探针只证明访问方向契约；状态用例及控制器纯测试不证明实际 P/C
bootstrap 的 ACL/继承/握手、完整 47 步或同次九项门禁。

旧单次运行未取得原 P 首个失败返回值或完整原生关闭证明，继续记录为
Unknown/不可重放。新增账户权限已恢复；这不撤销已签发 token，也不使旧运行合格。
新源码完整运行仍待原准备条件满足，并取得绑定后的新单次授权。

source_accepted=false，release_eligible=false；
SDK26、G04、SDK_FREEZE 继续 OPEN。没有据此创建测试预览标签或 Release。
