# 跨版本内容库迁移（Windows）

morrow-migrate-library.exe 支持受保护的 Rust/SQLite 内容库，以及旧版 shared_preferences.json/导出的 JSON 快照。默认只做只读预检。迁移时必须选择一个尚不存在的新目录；工具不会替换、删除或重新绑定原库。请先关闭正在使用原库的工作台。

    morrow-migrate-library.exe --source "C:\旧库\rust-workbench"
    morrow-migrate-library.exe --source "C:\旧库\rust-workbench" --apply --output "D:\Morrow迁移结果"

    morrow-migrate-library.exe --source "C:\旧版\shared_preferences.json"
    morrow-migrate-library.exe --source "C:\旧版\shared_preferences.json" --apply --output "D:\旧版迁移结果"

也可把 workbench.db、任意文件名的 .json 快照作为 --source。JSON 接受 Windows SharedPreferences 的 flutter.daemon.studio.v1 字符串、直接的 daemon.studio.v1 字符串，或顶层 version: 1 且有 ideas 的裸快照。可用 --package <workbench.morrowplugin> 指定插件包；正式打包的工具会优先使用自身旁边 plugins/workbench.morrowplugin。插件包必须能执行当前工作台的真实创建、设置和 TaskId 迁移操作。

SQLite 源格式依据库内 application_id 和 user_version 识别，当前接受受保护的 MORR schema 5–21；未来版本、非 MORR 库或密钥/审计不匹配均拒绝。预检在源库审计锁下，将数据库、WAL、SHM 和保护密钥复制到临时目录，随后只读取这个临时副本，避免 SQLite 在原目录生成侧车文件。复制前后核对源文件集合、长度和 SHA-256。迁移在目标副本内完成：每张 V1 idea 通过现有真实插件与证据事务变为 V2，原有 V2 及其他类型卡片不修改；旧操作回执逐项核对保留。目标会重新做受保护审计和逐卡来源比对。预检会阻止仍标记为活动的编辑恢复、草稿、导入及偏好提案；旧版编辑提案若仅有 revision 1，无法仅凭该记录证明已完成，即使用户已在界面完成操作也会保守阻断，需先由原应用核对并结束待决状态。没有 V1 卡片时同样执行这一阻断，以免复制出状态不明的库。

JSON 路径按旧 Idea 字段导入卡片，重复待办和文本式完成关系原样送入 V1，再逐卡运行 TaskId V2 迁移；导入后从 V2 的 origin 核对创建的 V1 标题、正文和摘要。外观、每日完成项、音乐、语言和字体偏好写入原生设置并读回比较。本地附件流入内容库；本地纹理、音乐和字体复制到目标目录并核对 SHA-256，设置中的本地路径改为目标路径。远程附件缺乏可导入的本地字节会拒绝；远程纹理或音乐 URL 仍依赖原网络地址。未知 JSON 字段、缺失媒体、无效大小或不受支持的设置在预检中拒绝，不会默默丢弃。原始 JSON 字节仍另存于成功目标的 legacy-shared_preferences.json。旧卡片的 time 文本和旧附件 pluginId 没有对应的当前可查询字段，保存在原始 JSON 中；附件导入会分配新 ID。

目标一创建就写入 MIGRATION_INCOMPLETE.txt。若中途失败，副本留在原处供检查，普通应用和 host 拒绝打开；请选择新的空目录重试。仅在审计、卡片/历史/设置与媒体检查结束且 migration-report.json 落盘后，工具才移除标记。成功目标尚未自动设为活动库。检查报告后，可从应用目录显式打开目标：`morrow_studio.exe --data-directory="D:\Morrow迁移结果"`。此路径也让字体仓库直接读取目标内的 `fonts`；通过已有托管库切换入口打开时，字体仍可能依赖原全局字体目录。同一个受保护身份的原库和迁移副本不能同时作为活动库打开。

目标报告列出迁移数量、保留的其他记录类型和验证结果。Rust 内容库之外的插件管理器目录与其他未识别的旁置配置不是目标的活动配置；原目录仍保留它们，不能凭这份报告断言自定义插件配置已经迁入。旧 JSON 快照的 time 和附件原 ID 也仅在原始快照中保留。没有在真实用户资料上运行过本工具；目前验证使用临时受保护库、临时 JSON/媒体和本地编译的真实 TaskId Wasm。
