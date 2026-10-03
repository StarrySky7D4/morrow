# Windows SDK 独立源码分发与项目工具 — 2026-10-03

状态：**PASS_SCOPED_SOURCE_DISTRIBUTION_METADATA**。应用后的100个相关工具测试通过；最终ROOT创建的119文件源码SDK ZIP及异地目录验证通过，27个三语言starter生成／带锁预检通过；默认21项目与原基线字节及完整TOML完全一致。只证明源码可独立交付与元数据流程，整个SDK仍未冻结。

新增 `tool/package_plugin_sdk.py`，导出／核验 bounded source-only SDK；`tool/morrow_plugin.py --sdk-only` 支持独立开发目录中的新建、预检和原source-lock流程。保持默认宿主契约检查，SDK-only的pack/check/transform在子进程与输出前拒绝。未新增运行权限、宿主二进制、SDK语言或guest能力。

原始缺口在只复制SDK/两工具的新目录真实复现：new exit1、缺Core schema、未创建项目。候选review发现并修正自报inventory绕过绑定观察、必需文件遗漏、Cargo本地输入闭包和累积预算；v1/v2/v3/最终004证据及原ZIP分别保留。最终候选独立静态复核PASS，限定允许本地应用，不是SDK冻结。

应用后真实100个文件／ZIP／项目元数据回归全部通过，0 skip/ignore；实际外部compiler/host边界以mock/哨兵拒绝。首次同集回归真实exit1，channel pack的3个子用例因合成host缺Core Cargo.toml而先拒；仅补复制当前Core清单，原channel诊断／无child／无输出断言不改，严格gate不改。首次raw logs和最小fixture增量原样保存。测试源码清单与真实Git index测试前后恒同。

源码导出在任何temp/output前核对，并在发布前重查唯一Core的10份canonical schema和declared versions；Core不打包。SDK-only核对19份public header、C/CPP源、file-backed Rust模块、生成版本输入／别名与native ABI观察。源码mask和声明观察不是Rust/C/C++编译、宏展开或schema/manual decoder语义证明；无独立比较来源的wire版本没有被伪造为已双重核验。清单为本地byte inventory，不是签名。

ZIP覆盖库源、固定contracts、许可/NOTICE、9profile×3语言starter、项目/lock/导出工具和docs。新增完整清单没有替换或放宽原morrow-sdk-source-v1 sdk.lock；62 library pins保持。无Core/runtime、SDK测试、precompiled DLL/guest或frozen兼容包。拒绝路径逃逸/link/reparse/duplicate/预算/hash/覆写，累计预算在读入下一文件前约束；不支持的Cargo patch/replace/外部workspace/额外target形状继续拒绝。Registry依赖和offline cache须由开发者另备，不在分发验证中解析或下载。

普通token、真正第三方构建/安装/业务、doctor/compiler、默认committed-tool runner、完整网络/文件API、生产UI/权限生命周期及其他平台仍未获资格；不使用本轮结果宣布整个SDK稳定。327 SDK与57原件不重建／重封，不改Linux process/controller/recovery；未读真实库、DPAPI或账户密钥，未运行外部API／CI，未提交／推送／Release。失落stage16未恢复。

## 最终ROOT来源与异地验证

最终recorded命令109条，包括5条只读Git和104条文件／ZIP／元数据Python命令；没有compiler／Cargo／host／guest业务。默认21项目new＋validate共42条exit0，生成README、项目source、Cargo元数据、sdk.lock与未改工具baseline原bytes和完整TOML逐项equal，验证不改项目。

最终源码ZIP直接由ROOT严格导出，create／verify-zip／异地verify-directory均exit0；119 source payload＋1 manifest，共120 entries。异地路径包含空格，完整目录不含Core/runtime、SDK tests、预编译native或guest，库source-lock仍是原62 pins。27starter = 21原profile三语言＋channel3＋channel-directory3；54条实际new／strict validate全exit0，原template source逐字节相同、每个lock真实核验。没有将metadata通过称为已构建／执行这些template。

另一个合成项目的5步显式锁流程exit依次0／1／0／1／0：无锁new、strict缺锁拒绝、创建锁、重复创建拒绝并保持旧bytes、strict校验。两个exit1都是预期诊断且不输出／覆写；没有用任意非零冒充成功，也未更新锁。

最终SDK source ZIP：243579 bytes，SHA256 666e12ddfb8c6ffaf51c1781925d24c615aef85001beb34b6f79baccb1601566。
ZIP内原始SDK_DISTRIBUTION_MANIFEST.json：18157 bytes，SHA256 9be4979e84bf6665124daff68be504a2f62297609de9a4690272df8289f6122a。证据中的重新排版JSON副本SHA256为2c7a37ba4d61fbba358462e04a469710fbf4f76c211d5db6c41b9d30d6db6ebd；两者语义等同、原bytes不同，摘要不可替代。

最终验证中的327 SDK、57原件、被改工具/docs、Core只读输入、真实Git index／HEAD/tree/status和异地bundle前后恒同。采样结束后仅补本报告（不在SDK分发payload中），SDK选中工具/docs保持固定；本轮完整源码树和增量patch另封存。默认bounded runner仍要求准确committed-tool身份，本轮没有绕gate或自动提交以取资格。
