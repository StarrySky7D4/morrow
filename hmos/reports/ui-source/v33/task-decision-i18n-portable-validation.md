# v33 任务九语检查的仓库可复现性修复

2026-10-09。发布前Root发现先前default catalog直接读取本地未跟踪的 `build/win-cloud-20261005`，GitHub新clone没有这些ARB时会在测试收集阶段失败。这是检查工具缺口；旧a1/a2结果及原说明仍保留为历史证据。

## 实际变更

- 新增仓库fixture `hmos/tool/fixtures/task-decision-flutter-catalog.json`。27条期望文案来自实际Flutter九语 `l10n/parts/main.<locale>.arb` 与 `packages/morrow_i18n/lib/l10n/app_<locale>.arb` 的三个设计key。每条parts/assembled逐字相同；提取前后18份实际文件的路径、字节数与SHA256完全一致，两个manifest均保存在fixture。没有以UiStrings反向构造期望值。
- default catalog、12项测试及 `generate-ui-strings.cjs --check-task-decisions` 只读固定fixture和仓库产品源；不会因当前Flutter源缺失而回退或改用另一来源。报告明确referenceMode=pinned、liveSourceFileCount=0。
- 另保留显式 `--check-task-decisions-live` / `task-decision-i18n.cjs --check-live`，核对当前18份实际ARB的parts/assembled文案、全文件摘要、与fixture一致性及实际UiStrings查找。源缺失或漂移会失败，不能静默切回fixture。
- `task-decision-i18n.cjs --export-live-fixture <new-output.json>` 可从实际18份源再次导出，输出已存在时拒绝覆盖。该操作仅在显式调用时读取本地Flutter源；保留来源与提取时间。
- runner新增显式 `--clean-checkout` 与 `--live-source`，两种模式不能混用。未修改Index、UiStrings或其他产品文件；默认历史全集生成器也没有运行。

## 冻结验证

`node hmos/tool/run-task-decision-i18n.cjs pinned-clean-a2 --clean-checkout`：**12/12 PASS**，0失败、取消或跳过，7个仓库输入前后摘要一致。runner将必要仓库文件复制到无build目录的隔离树，用继承到子生成器的preload拒绝树外文件读取。3个实际Node进程共审计13次文件读取，**外部读取0**，全部复制文件及guard字节未改变，执行后仍无build目录。详细证据：`task-decision-i18n-pinned-clean-a2-result.json`、`-clean-checkout.json`、`-inputs-before.json`、`-inputs-after.json`、`-tests.log`。

`node hmos/tool/run-task-decision-i18n.cjs pinned-live-a1 --live-source`：**12/12 PASS**，显式live check同时核对18份实际Flutter源及27条期望与实际查找；25项输入（7项仓库输入+18份实际ARB）前后一致。详细证据：`task-decision-i18n-pinned-live-a1-*`。最终clean-a2在runner最后一次窄改后执行；较早clean-a1证据未覆盖。

fixture SHA256：`462b23ed8e32ce23d88f06f602af560172d8ac84fe19ba7922d4684163db7f61`。UiStrings保持SHA256 `17801dd05b1afd9684f452b0e5b736ffba7c5520fb5413a8b6d002216cb1a576`。

这证明固定源文案/实际翻译查询与测试不再依赖本地未跟踪构建目录；没有运行Git克隆、SDK或设备，也没有声称ArkUI排版、读屏或所有语言的设备效果已验收。Root的最终全量校验及发布必须包含此fixture和冻结工具，并使用新的发布标签。
