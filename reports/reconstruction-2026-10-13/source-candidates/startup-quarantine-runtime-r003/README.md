# Rust 启动隔离候选 r003 增量

只修正 startup_quarantine.rs 的三处过时注释，明确资源观测由 supervisor 提供，模块只校验持久账本；逻辑与 allow 属性保持。其他四个 Rust 源复用 r002 原字节，三条 Dart 源与隔离测试复用 Dart r003，新增 JSON 测试另存 r001。

[十路径组合映射](composition.json)列出当前公开基线的前后哈希与仓库相对源码位置，并要求保留现有 audit/src/keys.rs 加固和 20 份锁。组合投影为 2394 个已声明文件，不代表整个 Git 树，也尚未应用或完成新的联合构建。

旧 r002 档案与测试记录保持。此目录不是可直接执行的部署入口；SDK26／G04 仍 OPEN，release_eligible=false。
