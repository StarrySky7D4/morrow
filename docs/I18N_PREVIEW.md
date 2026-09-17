# test.52 国际化预览

目标版本 `0.1.9-test.52+56`，Windows 本地测试预览。构建与实际运行结果另见 `reports/i18n-test52-preview.md`，版本号不代替验收，也不表示已发布 GitHub Release。

## 产品范围

设置页提供跟随系统、简体中文和 English。导航、筛选、排序、设置、颜色与材质面板、新建／编辑灵感、附件、媒体控件、插件管理和恢复界面使用同一套类型化资源。较长英文允许换行；切换只更新显示，不新建工作台控制器或重置编辑会话。

用户卡片、附件原件、歌词和第三方插件 UI1 的字面内容保留原文。内置文字小工具使用仅第一方入口启用的显示适配：核对原五节点布局与字段后翻译标题、输入标签、计数及空提示，保留宿主原始文档、输入、转换输出、待处理事件和未知布局，不修改或重编译冻结 guest。旧类别／阶段值与 v1 查询协议仍使用原值，界面通过稳定 ID 显示译文。本轮不是业务数据迁移，也没有自动翻译用户内容。宿主提示与插件原始错误分别处理；原始诊断可作为详情保留。

## 资源与存储

开发源为 `l10n/parts` ARB 片段，官方 Flutter gen-l10n / intl 生成类型化调用、参数和复数处理。`tool/build_i18n.py --generate` 同源生成合并 ARB、Dart 缓存、版本化 Protobuf＋LZ4 资源及摘要。界面安装 `L10n.delegate`，实际验证所载资源的长度、版本、语言与编译摘要后交付对应消息。详细格式、边界与生成方法见 [共享包说明](../packages/morrow_i18n/README.md)。

Windows 语言偏好由 Rust 宿主写入独立 `org.morrow.host.ui-preferences` 核心记录，字段为 Protobuf，复用核心持久化压缩与提交机制。保存带操作 ID 和预期修订，重试原操作；只改变语言时不调用业务插件保存配置，因此插件不可用仍能保存语言。未知／损坏记录拒绝覆盖。Dart 只保留运行中的显示快照。

私有 `host.capnp` 追加读写语言操作并更新生成摘要；既有 `studio.capnp`、工作台 v1 业务消息及冻结 `sdk/compat/guest-v1-rc1` 不变。Web／移动端旧宿主仍沿用原存储路径保存界面偏好，本轮未宣称其 Rust 持久化或平台能力完成。

## 构建与验证

准备依赖后执行：

```powershell
python -X utf8 tool/build_i18n.py --generate
flutter analyze --no-pub
flutter test --no-pub
pwsh -NoProfile -File tool/build_rust_workbench_windows.ps1
```

构建脚本附带实际工作树源码 ZIP，在编译前后核对源码集合与摘要，预览包的 SOURCE.txt 指向同批源码附件，不引用不存在的版本标签。该快照不宣称密闭构建或可复现构建回执。已有产物默认保留，不覆盖历史版本。

真实 Windows 预览可使用 `--locale=en` 或 `--locale=zh` 指定显示语言。自动资格验证必须同时给出全新 `--data-directory=...` 和 `--self-check=...`，仅在隔离资料中生成演示记录。它记录真实 Release 渲染和运行结果，不使用用户内容库。

## 后续契约边界

本次完成当前宿主界面双语接入；ROAD-02/04 的整个插件国际化体系仍未冻结。后续单独定义 LiteralText / MessageRef、受保护宿主命名空间、插件资源声明、任务固定语言与格式上下文、缺词策略和 C/C++/Rust 向量。动态语言安装、繁体／RTL 语言翻译、跨平台原生验收以及完整可恢复消息日志另行推进。
