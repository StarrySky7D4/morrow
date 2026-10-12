# Windows 原生构建与产物恢复进度（v2）

日期：2026-10-13。此记录仅覆盖349基线的Windows AMD64 host与supervisor编译和产物恢复，不代表完整354 relocation已经构建或验收。

原Cargo进程17932以退出码0结束，stdout与stderr均已到达EOF，两个产品二进制编译成功。随后包装器以退出码1失败：原单链接读取规则拒绝了Cargo生成的两个已知硬链接别名。原包装器失败结果保留，未改写为通过。

第一次产物恢复读取器在读取载荷前以退出码1失败，原因是路径查询与持有句柄API的mode/ctime表示不同（不将ctime等同为同一种创建时间语义）。修正后的读取器以退出码0完成独立恢复：对精确两个别名核验跨API公共身份、各API内部前后元数据稳定性、完整SHA-256及AMD64头，并保存新副本。没有重新运行Cargo，也没有执行这些二进制。

| 产物 | 字节数 | SHA-256 |
| --- | ---: | --- |
| host | 62,653,952 | `7d30a4ece32daa58be7bc8bd1f3f000da6387072eb8507af28e5f4e9dbbf9bc1` |
| supervisor | 27,291,648 | `152b4c38117b0ee7e716769e57bb6d2892a4844284e454d882ebbc74ff4b5221` |

两个原编译时间窗的HOME复核现已闭合，状态为**PASS_RECOVERED_PRODUCT_COMPILE_HOME_POSTREVIEW_ONLY**，`input_drift=0`、`findings=[]`。原353 lib-test编译和355产品编译的UTC/单调时钟边界分别核验；记账时间戳必须落在其中一个独立UTC窗口，两个窗口之间的空档不予接受。

有限HOME核验覆盖5,592个文件和873个目录，包含前后完整声明命名空间、声明文件内容、三个薄Git仓库的原冻结压缩字节/解压对象ID/选定树与工作树，以及固定本地alternate。记账SQLite仅在内存中检查严格schema与完整性，不连接磁盘数据库。另核对1,668条锁图推导的vendor清单/checksum元数据；这不等于全vendor载荷验证。2392份源码和20个锁保持绑定，target为显式复用缓存，不宣称全新。

复核报告：21,950字节，SHA-256 `cc7378396ce5ad3a1196edf3e310853b6c387841e8e37a1d1c9d7f4e00d2411f`。此结果只接受独立恢复产物和限定HOME后置条件，原包装器退出码1及其缺失输出继续保留。

357仍为**SOURCE_NOT_RUN**；Windows341已物理闭合，但业务结果为**CLOSED_NOT_ACCEPTED**。SDK26、G04及namespace继续**OPEN**，`release_eligible=false`。

闭合事实来自原本地ROOT观测。本公开摘要不包含原始日志或二进制，不构成产品、VM、SDK或namespace验收结论。v1保持不变；本版记录已闭合的限定HOME复核，后续验收结果另建版本。
