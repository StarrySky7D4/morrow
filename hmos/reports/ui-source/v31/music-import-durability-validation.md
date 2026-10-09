# v31 音乐导入原请求持久化与文件选择生命周期

基线：`4c04f97e6beb580d33f14f9540db98129fc50180`。本 worker 只修改实际 `MusicFiles.ets`、`MusicLibrary.ets`、`MusicWorkbench.ets`、其源执行测试和本目录证据；未修改 Playback、UI、Index、Rust、C++、SDK、版本配置、Git 或设备。

`music-import-durability-a2-result.json`：**114/114 PASS**，0 fail/cancel/skip/todo，9,807.4809 ms。21 个仓库输入在运行前后完全一致，其中 20 个由实际测试进程读取；Node 与 TypeScript 运行时输入也保持一致。来源、读取轨迹和运行前后哈希见 `music-import-durability-a2-inputs.json`、`music-import-durability-a2-inputs-before.json`、`music-import-durability-a2-trace.jsonl`。

这些是执行实际产品类的受控主机组合检查。文件系统、SDK picker、Native Store/FD 传输和 AVPlayer 均为受控边界；新实例恢复用持久状态副本创建新 VM，**没有模拟结果冒称真实进程崩溃、断电、OHOS grant、Native fsync、设备生命周期或真实解码/播放证明**。真实 SDK、HAP、设备及完整 Windows 对齐由 Root 另外验证。

## 原请求与派发不变量

1. 文件选择授权先完整捕获原字节，保持已有 150 MiB 单文件、512 MiB 缓存和 20 份选择预算；不持久化 provider URI。曲库仍由实际 Native Core Store 独占。
2. `MusicLibrary.beginImport` 先固定完整原请求、原 begin 和原 FD import wire。`musicImportWire` 按实际 Rust `ImportRequest` 字段顺序序列化；运行中的测试逐一与 v29 实际 Store 产生的 `request_json` 全文比对，不能从旧字段选择性重造请求。
3. 新必需 `MusicLibraryHooks.persistImport` 接到实际 `MusicFiles.persistImport`。原 FD wire 的完整 write、fsync、close **全部 ACK 后**且原 owner epoch 仍有效，才调用 Native `import_begin`。写入、fsync 或 close 失败、页面消失、epoch 改变均不越过此门。
4. 原 request.json 已存在时只接受完全相同原文，重新打开原文件并 fsync/close；不截断或改写它。现有原文缺失、改变、符号链接、未知内容、残缺原文均保留，并阻止派发。磁盘完整原文在同进程落盘 ACK 丢失后，只能由原注册对象的固定 `originalRequest` 证明对应更新；Native begin 前仍须重新取得同一文件 fsync/close ACK。
5. `unissued` 表示注册尚未核对，并非证明 Native 没有提交。重启读取完整原 wire 只恢复原 request、原 begin、原 inspect/reconcile。`spool_resume` 只执行用户明确请求的 inspect；缺失/Unknown inspect 不生成替代 track/op/CAS。原 begin retry 必须显式执行，继续使用完整原请求和原 CAS，并再次经过原 wire 的持久化 ACK 门。
6. Pending inspect、retained Pending reconcile、Ready inspect/reconcile 和缓存清理各自核对原请求。没有自动 FD 重放；用户明确 `import_continue` 才能用同一 wire 和原捕获字节继续。Ready 必须完整匹配原请求且确认 bytes retained，才允许 finish 清理。
7. 已确认 Ready 后的同进程部分清理仍保留原注册对象、原 plan 和 `releaseStarted`。残缺清理目录不能进入完整源恢复或再次导入；只可显式继续原清理。新实例面对残留或未知目录继续保留并报告诊断，不能凭目录名重建清理授权。

| 边界 | 受控组合检查证明的行为 |
| --- | --- |
| 原 wire write/fsync/close 失败 | 0 Native begin /0 FD import，原缓存保留 |
| wire ACK 后页面 epoch 变化 | wire 已保存，Native begin 未派发；新实例可原 inspect 和原 begin retry |
| Native begin 结果丢失、分别未提交/已提交 | 原 IDs/CAS/wire 不变；恢复不自动 begin 或 FD import |
| Pending 无 retained 字节 | reconcile 仍 Pending，只有显式同 wire 继续才流入音频 |
| Pending 已 retained 或已 Ready | inspect/reconcile/finish 不再次发送音频 |
| 恢复后原 CAS 已过期 | 原 begin 失败时保留原 CAS，不重造替代导入 |
| 同进程 cleanup unlink/rmdir 失败 | 保留原对象，retry_io/recover 不将残缺源呈现为完整文件 |

## 显式 picker 往返不变量

- 每次用户明确启动 picker 只创建一个当前 `MusicWorkbench` 实例持有的 ticket。返回必须原 ticket、同一仍 owned 且未 disposed 的页面；一般 Native、文件和播放器异步没有 ticket 的 epoch 例外。
- 系统 picker 可能先引起后台，再返回前台。返回 Promise 可先于或后于前台事件；若先返回，则保留这一次结果，等待同一页面明确调用 `foreground()`，**后台不读取或修改 Native 曲库**。歌词的已授权完整文件捕获仍由原 picker 请求完成，结果保留在该 ticket 中。
- 只 ticket 核对后的返回可重新捕获当前 owner，并同步本次 run 的错误所有权。后续完整 Native 库读取、CAS、捕获和写入仍逐步严格检查这个 epoch。读取前后都重检 writable 状态，不能覆盖既有 Unknown 或原 read/import/mutation plan。
- 音乐选择返回后先重新读取实际完整曲库，再捕获和生成新的原请求；CAS 使用这次实际读取的 revision。取消选择不执行新的 Store 读取、文件捕获或写入。
- 歌词选择结果保留完整原文和 picker 前原 track/title；前台返回后的完整 Native 读取重新确认原 track/import-op/bytes/hash。目标被替换/退役、读取 Unknown 或歌词超限时，原文仍在原目标下展示，不能写入另一曲目或裁剪后保存。
- 页面 dispose 取消仅这一 ticket，无 Store 写入；普通旧 epoch 的 Native 回复仍被拒绝。前台恢复也不自动恢复播放。

## 修复记录与证据边界

扩展检查第一次 112 项运行得到 111 PASS/1 FAIL，揭示真实同进程问题：request 完整写入后 fsync/close ACK 丢失，`recover` 将预期固定 sidecar 从空到完整的变化当作身份漂移，删除注册对象，使显式原 begin retry 不能继续。修复只允许原对象已固定 wire 的完整对应更新；没有放宽未知文件或原请求不匹配。随后同一 112 项检查通过，并加入取消 picker 与前台返回后超限全文保留，最终 114 项通过。此早期结果来自工具运行输出，没有被冻结的输入清单，因此不作为最终资格证明。

证据收集器 `a1` 实际 114 tests PASS，但收集器把 uppercase 存储哈希与 lowercase trace 哈希比较，误判重复 `EditorDraft.ets` 读取漂移，资格收集失败。`music-import-durability-a1-tests.log` 与 trace 保留，`music-import-durability-a1-failure.json` 标记 `RUNNER_QUALIFICATION_FAILED`；a1 不计为完整输入资格通过。大小写规范和运行前清单保存修复后，独立新 label `a2` 完成全部检查并通过。

冻结清单：`music-import-worker-freeze.json`。a2 测试日志 SHA256：`8290E173321CCD24FDE1D2F6C7FB9B32C03E82BBC47DFC0837D7C1B25C9A751E`。

仍未关闭：实际重启/崩溃/断电保证、真实 DocumentViewPicker grant/生命周期、实际 OHOS request 文件 fsync 与目录持久化、Native新实现执行、设备真实音频和 UI 验收、未知/跨进程残缺缓存显式管理、完整 HMOS/Windows 功能对齐。
