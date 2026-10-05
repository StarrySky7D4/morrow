# dev.9 文字草稿与编辑器跟进

日期：2026-10-05。版本 `0.1.0-hmos-dev.9` / `1000009`。本轮交付独立的开发文字草稿，不关闭与 Windows 的完整功能对齐目标。

## 最终构建

- 未签名双架构 HAP：`entry/build/default/outputs/default/entry-default-unsigned.hap`，22,685,409 字节，SHA-256 `4BA0830D729B7D2021B54DBC27DD50724AC565EC2098204B76242F4952C96096`。
- API26 / SDK `26.0.0.105`；最终包覆盖安装成功，见 `device-focused-final-install.log`，`bundle-focused-final.json` 确认 dev.9 / 1000009。现有健康设备为 `127.0.0.1:5557`，软件 `7.0.0.106`，1320×2232 px / 440 vp。ARM64 仅编译，x64 另有实际运行证据。
- ARM64 原生库 SHA-256 `B32D4D6C7E647CF4B7C57A432535E9D2A42688E5D02F6C30B49AFE637B8C4E2B`；x64 `0DF409447482794AAC1364A8D555EAC0653C29D1CE543A98FA7286B90A757BEC`。
- 最终构建见 `rust-arm64-accepted.log`、`rust-x64-accepted.log`、`hap-focused-selection-final-build.log`。前一次构建因本工程日志文件暂时 EBUSY 失败；没有清理全局缓存或停止其他构建，后续正常重试成功。保留既有 Function.bind 与控件 system capacity 警告；没有发布签名资格。

## 来源与实现范围

原 `editor_draft.proto` 与纯 Rust 模型逐字节复用，分别为 `8F80DB114BCC480D9553AE60916170B1982C9CDE0D0452753A35B491279FDDDC`、`D11632512506F357A73112568A6E8856AA74E87500B91709C87083A50DAC6C8E`。原 journal 事务/历史分支按开发 HostRuntime 适配，出处与边界记录在 `rust/editor-draft-reference.json`。冻结共享快照没有全量替换；上游观察见 `upstream-audit.md`。

私有 journal 保存完整原始文字、UTF-16 选区/composing 字段、固定完整来源和世代 CAS。历史回执与当前世代/active 分开，精确弃稿可重查，弃稿身份不能复活。识别私有卡片后先完整验证，再从业务列表/查询排除。新建空草稿不创建业务卡。保持 16 活动槽、256 累计身份及 64 MiB 活动预算，业务卡仍限 256 张。

ArkTS 模型串行去抖保存，冻结请求拥有 scope 和完整快照；旧回执不替换新输入。未知结果保留原序列化请求，只允许显式核对。关闭前等待最新确认；放弃有确认窗口。正式业务回执仅在完整原始值与提交快照一致、没有待提交 todos/捕获阻断时清理草稿。新输入或来源变化保留旧草稿并显示冲突，不能覆盖新修订。

独立审阅修正了：清理未知时类别选择绕过冻结门；task_add 发起/回执必须核对完整文字且组合输入结束，不能清空合法或异常的新候选；迟到回调与 scope 切换；已确认历史在当前 journal 损坏时误降为 not_committed。相关冻结保护已进入最终包。

UI 增加草稿列表、恢复、保留和弃稿入口；标题上限 60、正文 20000、假设 5000、结论 10000，实验字段为多行。正文可用宽度达到 Flutter 的 590 vp 条件时并排编辑/预览，较窄视口切换预览。宽屏并排与真实 IME 会话本轮尚未做设备验收。

## 已执行验证

| 范围 | 结果与证据 |
|---|---|
| Windows 实际 Rust Engine | 26/26 PASS，`rust-host-accepted-tests.log`：14 项 journal 与原 12 项业务/查询。包括重启、原始 UTF-16 值、固定来源/CAS、精确历史、弃稿/身份与预算、损坏拒绝及历史已提交后读回失败仍 committed |
| 原始纯 Rust 模型 | 10/10 PASS，`rust-draft-model.log`；原模型能验证 parent 字段不代表 HMOS 已实现该能力，运行期阶段门明确拒绝 |
| 实际 ArkTS 纯模型 | 26/26 PASS：16 项草稿、5 项查询、5 项数据源，分别见 `arkts-draft-model-test.log`、`arkts-query-model-test.log`、`arkts-card-model-test.log`；覆盖精确未知核对、旧回包、新输入合并、flush/销毁、选区/composing 与 u64 精度；不等于 ArkUI 渲染 |
| 最终源码 OHOS x64 runner | 20 项 PASS，`native-accepted-self-check.log`；在新的隔离 fixture `/data/local/tmp/hmos-dev9-native-20261005c` 执行，不读取应用或桌面正式库 |
| 草稿流程补验包（C5756DE2） | 9/9 PASS，`draft-final/result.json`、`draft-final-device.log`。独立 fixture `HMOS-draft-20261005-C` 验证空草稿不建卡、保留不提交、中文/emoji/多行/前导空格/末尾 LF 重启恢复、编辑中最新自动保存重启恢复、正式提交单一身份并清理草稿、已有卡片未提交文字不改源记录/重启、取消弃稿和确认弃稿重启后仍 inactive。业务身份保持 `b6f32cb3-6cf6-48ac-bf34-772c3356d14f`；后续选区初始化修正另行复验，不将旧包替代最终包 |
| 待办与来源变化补验包（C5756DE2） | 4/4 PASS，`task-final/result.json` 与三段 `task-final-device.log`、`task-final-continuation.log`、`task-final-discard-verification.log`。完整普通文字只添加一个任务并清理已消费输入；收藏操作保留未提交 todos 并显示来源冲突；重启恢复 pending todos；显式弃稿后重新打开业务编辑器，确认 raw todos 空且原任务恰好一个。中文/emoji 由实际控件输入，真实 IME 候选会话仍未验收 |
| 最终 HAP 未编辑查看 | 2/2 PASS，`readonly-final/result.json`、`readonly-final-device.log`：打开已有 C 卡片仅查看/滚动并关闭，不创建草稿；重启后仍无该草稿。未聚焦的初始化 selection 不消耗槽位，实际文字与已聚焦的选区仍走正常保存模型 |
| 最终 HAP 完整草稿复跑 | 9/9 PASS，`draft-release/result.json`、`draft-release-device.log`；在最终 4BA0830D 包上重新执行同一完整流程，包含中文/emoji/前导空格/末尾 LF。独立 fixture `HMOS-draft-20261005-D` 的业务身份 `3cf7d1b4-3d89-4831-8ca2-675f8b872076` 在正式保存、既有卡草稿和弃稿后均保持，最终重启图为 `draft-release/discarded-reopened.png`。加上只读查看为最终包 11 项；C5756DE2 补验范围单独保留 |

设备验收辅助只通过观察到的控件操作，不写应用数据库。诊断保留在本地：空输入框在 UI 树里省略 text；官方输入成功返回 No Error 曾被脚本误判；卡片中心点击被保存提示覆盖。分别用实际布局检查、精确文字回读、显式关闭提示和独立续验入口核对，没有重发建卡请求。最终验收使用独立目录保存，避免把旧截图当成最终包证据。

中间包的生命周期防护误用了设置 NativePreview 的 mounted 状态，导致编辑控件显示的文字未进入草稿，重启后只恢复初始空值。最终包改用独立 pageAlive，随 Index 页面出现/销毁更新，设置预览挂载仅用于预览。只恢复并检查了唯一测试 B 的已知 journal 身份 `ad6cd56b-efb6-4dc1-a6cc-daf9026e1e61`，确认标题/正文均空后显式弃稿，见 `diagnostic-empty-draft-discard.log`。没有删除业务卡或其他草稿；最终用独立 C fixture 重跑。

待办补验的两次辅助断言分别停在尚未滚入可见区的收藏按钮、以及不显示任务明细的灵感卡片。随后按实际 UI 查找控件，并在弃稿后重新打开编辑器核对任务；没有重发 task_add、收藏或弃稿操作。fixture C 保留为开发验收数据，原有业务数据未清理。

追加状态检查发现未聚焦的初始化 selection 会让仅查看卡片创建 journal。最终修正按每个 scope 的实际 focus/blur 过滤初始化选区；同一编辑器确认业务修订后只保留仍聚焦字段，不将访问过的所有字段迁移。先核对并显式弃掉本轮 C 的已知诊断 journal `26414ae3-bffa-492f-a620-d28869a1d6ef`，再完成最终包只读查看/重启检查。没有清理其他卡片或草稿。

构建清单记录 272 个输入；磁盘与暂存区的原始字节、HAP 大小/哈希、版本一致。可用 `tool/verify-build-manifest.cjs --staged` 复核。生成原生库与 HAP 保存在本地构建目录，不将其作为 Git 源码提交。

## 仍开放的边界

当前为 `development-unsealed`，未实现正式 Storage.prepare_write、HUKS、owner 和 audit 封存。附件、导入消费、前驱证据、parent/retirement 字段在运行期明确拒绝；正式业务原请求跨进程恢复、captured S1/S2 和父子交接尚未实现。纯模型测试里的 S1/S2 只指两次文字快照，不是正式捕获资格。

未知草稿保存请求、业务请求和清理请求仍只在当前进程保留；强制终止后只能恢复已确认 journal，不能保证最后一个未确认字符。普通控件可将已保存候选串恢复为文字，不能恢复原 IME 候选会话、方向/affinity 的完整 Flutter 语义。真实候选替换/取消、内存帧时、七风格矩阵、宽屏编辑器和 ARM64 真机未验收；此前版本证据保留各自范围。平台接口核对见 `platform-notes.md`。

仅提交并推送 `codex/ArkTsUI`，不合并或推送 `main`。持续追平工作仍开放。
