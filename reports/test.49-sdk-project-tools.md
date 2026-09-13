# test.49：三语言插件项目工具与完整包执行

日期 2026-09-13。应用 `0.1.9-test.49+54`；核心、宿主、审计、运行时和 SDK 源码包 test.49。工作台 guest 源码包仍 test.13，链接当前 SDK。数据库格式14不变；guest ABI2／runtime7／task3／UI1／dependency-call1 和 guest-v1-rc1 原件未修改。第一方 AGPL-3.0-only，无新增 unsafe。本轮为本地开发，未推送或发布 Release。

## 完成的开发流程

`tool/morrow_plugin.py` 提供 new／doctor／build／pack／check／transform，覆盖 C11、C++17、Rust 的内容、转换、UI、依赖四类模板。plugin.toml 为有界、严格类型的构建输入；唯一正式运行清单仍为核心生成的 PB＋LZ4 包，不引入第二套运行期配置。

每次 pack 先成功编译本次源码，再以 pack-v2 生成临时候选，执行 PreparedPackage 准备检查，最后向 dist 按完整归档摘要不可变发布并逐字节核对。失败不继续后续步骤，不按时间选择旧模块。check 只准备，不运行、授权或解析依赖；transform 明确执行一次，使用隔离临时内容库且无内容授权，输出关联通过后无覆盖发布。需要依赖的模板必须交由批准依赖锁的宿主执行。

核心 CLI 新增 pack-v2／pack-v2-catalog，保留旧模式；支持七项能力、处理器、含逗号的完整版本范围、依赖调用和预算。运行时新增 plugin_check，区分参数/准备错误、业务失败、执行失败及输出错误。Python 顶层失败统一非零，具体底层原因保留在诊断；不把未知发布结果宣传为已确认未发布。

新增可重复入口 `python -B -X utf8 tool/verify_plugin_projects.py`，默认新建含空格和 Unicode 的独立目录，保留项目、日志和产物，不覆盖以前证据。生成项目及受信任工具链可能执行本机构建脚本、宏与编译器代码，不是源码沙箱。当前工具由本仓库辅助，未完成独立预编译 SDK 分发。

## 审查修复

- 编译前扫描整个既有 build 树，拒绝所有层级的符号链接／reparse，最多200000项；不跟随目录链接。真实深层 symlink 测试覆盖三语言且确认编译器未调用，不能将此表述为敌对并发进程隔离或所有 junction 形态穷举。
- C/C++ 链接最大内存取自项目预算，已实际编译32 MiB模块并解析 Wasm 内存段为512页；运行时仍独立取宿主政策与清单的较小限制。
- 名称和版本128 UTF-8字节、SemVer UInt64核心数字、提供者范围长度、标识符和 Unicode 控制字符提前校验。完整版本范围语义仍由核心验证。
- 错误 Cargo 表／数组／路径类型返回明确 ToolError；SDK与宿主核对五份Schema及runtime/task/UI/依赖调用版本。实际SDK副本版本漂移而Schema不变时，在编译前拒绝。
- 最终审查发现验证脚本初版仅以非零退出判断预期失败，可能把未来环境故障误记通过。本轮初版日志各原因均经核对正确；脚本已改为同时核对具体业务失败／拒覆盖／目标编译错误／安装阶段损坏容器诊断，负例未声明预期诊断会直接拒绝。

## 实际验证

| 范围 | 本轮结果 |
| --- | --- |
| 项目工具单元／负面测试 | 23/23 PASS，无skip；含12模板、实际链接、Cargo错误配置和版本漂移 |
| 验证脚本失败分类回归 | 6/6 PASS；无关环境错误、缺诊断、错阶段及错退出分类不能计通过；最终入口合计29项 |
| 核心打包CLI | test.49 release 7/7 PASS |
| 运行时检查CLI | test.49 release 8/8 PASS，实际旧三语言包及WAT负面模块 |
| 新生成项目完整包 | 三语言×四类共12个直接执行PASS；未由资格工具重编译或重打包 |
| 可重复项目验证入口 | 新的Unicode/空格目录从零生成、打包、执行、失败保护全部PASS |
| 固定旧SDK兼容 | 14项Python检查及12项旧二进制执行PASS，36受pin约束文件原件不变 |
| 严格静态检查 | core/runtime all-targets release Clippy -D warnings及格式检查PASS |
| Flutter真实宿主集成 | 9文件41项PASS |
| Windows应用 | Release编译47.2秒；版本49+54；四项实际自检PASS |

内容模板覆盖七命令、二进制正文／附件、幂等、当前撤权、取消后查询真实提交结果和重开；转换每语言三handler×五输入，共15次真实任务，含业务失败，另验证错误注册；UI验证实际guest表单/编辑输出、会话事件和迟到拒绝，无内容提交；依赖验证Manager批准锁持久重开、调用固定Rust提供者、二进制结果、明确授权后提交及幂等、提供者撤权。

显式CLI三语言含NUL/0xff输入逐字节输出一致；业务失败不写输出；既有输出拒覆盖。同一Rust项目重新编译包字节一致；注入compile_error时不发布旧模块或改变旧包；故意损坏已有摘要包时拒绝且保留损坏字节，测试finally恢复原件。此处仅证明本机同源重复构建，不证明跨机器可复现构建。

初次12项目目录 `build/test49-projects`，执行日志 `build/test49-sdk-projects-qualification.log`。可重复入口初版日志 `build/test49-projects-full-gate.log` 保留；补强负例诊断后的最终完整日志 `build/test49-projects-full-gate-final.log`，最终证据目录 `build/SDK projects 空间 387a4d469e114640a3edd5ebf7deabd7` 内含每步日志及 RESULT.txt。32 MiB实际编译日志 `build/test49-memory32-{c,cpp}.log`。23项最终记录 `build/test49-plugin-project-preflight.log`；最终完整入口再次运行全部29项并验证全部目标失败诊断后通过。故意坏源与损坏包错误是预期负面验证，不是隐藏的构建失败。

SDK旧原件兼容 `build/test49-sdk-compat.log`；CLI最终 `build/test49-package-cli-final.log`、`build/test49-plugin-check-final.log`；Clippy `build/test49-{core,runtime}-clippy.log`。本轮未重复无生产逻辑变更的全量核心／宿主／审计／SDK单元套件，不将test.48全量计数当作本轮新结果。

## Windows产物与存储核对

实际应用截图已查看，侧栏、设置和卡片正常，无Flutter加载错误占位。四项自检分别为原生blur 0/1/12/40及关闭API、实际静音WAV解码与时钟、seek/播放互斥/恢复不自动播放、实际Rust工作台渲染。blur API通过不等于桌面像素比较。

自检数据库格式14、integrity_check=ok；6份证据/6引用、34唯一块/144引用，缺块和孤块均0。1个Ready捕获、1份已发布归档、10分片、1个归档根；账本1档、1,583,875逻辑字节，与归档成本一致。日志 `build/test49-windows-{build,qualification,inspection}.log`，元数据 `build/test49-final-verification.json`。

实际证据目录：`build/workbench-host/test49-final-af3a76a12cc94405a8c3910431e1ed93`。

五个SHA256已独立重算，Windows内宿主及工作台包与生产副本逐字节一致：

| 产物 | SHA256 |
| --- | --- |
| `morrow_studio.exe` | `df6fa1ac28634fab0eccb73bfd8aaf14960fabbe4c6bdbe41ca1187ae71f5626` |
| `morrow-workbench-host.exe` | `ea93de9a1c74ca61ed8f15ef42cb40651c11899c557b06ed019dee46723a250e` |
| `workbench.morrowplugin` | `96d64531aec99f4a84437ba28ef44dbf1d8f07c6f9324ce46303b210a13e0a9b` |
| `morrow-content-replay.exe` | `e3b2a605ed1be9f323e7cf4fecaec181804de70f9292fc8932633be2b629b274` |
| `morrow-audit-check.exe` | `0dfe20c492e505d79a4dbe0fd0e0974ac5a8530eca835e78938c0ee6a439fc53` |

## 剩余工作

阶段结论 **PASS_SCOPED**。基础guest保持候选兼容，SDK源码API、本地ABI和预编译分发仍未全面稳定。下一步通用插件管理／UI扩展和独立插件接入；完整内容API、持久长期任务、依赖图证据、历史清理、旧数据迁移及声明平台逐端验收继续。三语言模板运行不代表独立第三方采用、复杂UI或全平台完成，也不宣告0.2.0锚定。
