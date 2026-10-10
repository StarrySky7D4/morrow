# C28 阶段 20：Windows SDK 后续构建准备

截至 2026-10-10，阶段 20 完成三组本地源码与构建方案准备，尚未执行新的 Cargo 构建、Wasm 行为验证或 Windows 沙箱运行。本次开发分支更新仅同步脱敏状态文档；产品源码相对 `bca6de996c539136e359685a2f714fc3d4fa0e61` 不变。

## 新增准备与证据范围

| 工作 | 已完成 | 尚未通过 |
| --- | --- | --- |
| Cargo checkout 来源校验 | 准备固定范围的对象、树、索引、配置与物理集合验证器，以及完整新 Cargo home 的校验接口；源码仅完成 AST 静态解析 | 两个验证器尚未执行，未生成根审核绑定或实际验证读回；不能据此接受旧构建或后续 helper 构建 |
| 当前 runner/setup 的匹配构建 | 静态核对完整 1,651 文件源码、827 包 vendor 与冻结 Git 字节及物理集合；原锁中 827 registry 条目的版本/checksum 匹配，10 个 Git 条目和 56 个本地包条目具备声明来源 | 原 harness 根入口下选择依赖包的两个 bin 尚未实际运行；严格缓存校验器未绑定，无构建 READY、无新 helper 产物。直接使用 derived workspace 仍缺 `exec-server/tests/support`，未添加占位包或修改锁 |
| 可公开 session Wasm 的重建 | 从已推提交提取 310 个原有公开源码文件，锁定 4 个本地包及 23 个 registry 包；guest 锁与原构建锁字节一致，23 份离线归档 checksum 和精确版本索引记录齐备；保存路径重映射与七步会话资格方案 | 未运行 Cargo metadata、编译、全字节产物隐私检查或真实 Wasm 行为验证；没有新二进制或新哈希绑定。SDK 测试专用输入闭包也不在此 guest 准备范围内 |

上述静态核对不替代 Cargo 的实际依赖/feature 选择，也不证明编译、链接或运行通过。310 个公开源码文件和已选产品源码均与已推提交一致，没有重复计作新增实现。本机专用构建入口、原始路径表和缓存控制文件继续保存在本地，不整体公开。

## 保留的失败与开放事项

阶段 18 的 build003 仍为 **Cargo 退出 0、外层退出 1**。原始输入前后守卫相等、已保存产物的来源读回，不能抵消新 checkout 后置守卫失败；本轮没有补记通过或重新执行该构建。checkout 新增对象与元数据的实际来源接受仍须经过新验证器的独立执行。

旧 runner/setup 与当前依赖源码不完全匹配，不能用于声明当前三个程序已形成匹配 manifest。原 session Wasm 含本机路径，保持原始资格字节且不公开；公开 Git 树仍缺该夹具，完整构建状态为 `NOT_RUN_MISSING_SESSION_FIXTURE`。后续应从真实 guest 源码重建，记录不同的新哈希，并验证全部会话调用与收据；不通过二进制字符串修改或替换原身份绕过资格。

原生 Start 保持 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`，不得自动重放。owner finish、factory release、cleanup/join、真实断连、Windows 生产沙箱资格及后续 11 项原始库测试仍未闭合。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

## 后续顺序

1. 审核并实际运行严格 checkout 校验器，保存独立读回；保留原失败记录。
2. 绑定新 helper 构建校验接口，实际验证原 harness 根的包/目标选择，再离线编译当前库、runner 与 setup，形成匹配来源和产物身份。
3. 独立重建公开 guest，完成新字节隐私检查和真实七步会话资格，再考虑接入公开 harness。
4. 在明确授权范围内完成 Windows 生命周期与安全执行复验。Agent 会话层与安全执行层验收后，暂停并准备下一次测试预览；不等待扩展执行层完工。

应用版本仍为 `0.1.9-test.58+62`。本次只更新既有开发分支，不更新 main、tag 或 Release；不触发新的构建、VM 操作或手动 CI。
