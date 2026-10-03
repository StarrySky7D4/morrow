# Windows x64 原生 SDK codec 矩阵 — 2026-10-03

Clang 22.1.0 与 MSVC 19.44.35227 分别构建、执行七个原始 SDK fixture，**14 次 build／14 次 run 均真实 exit 0**；另四次新 wrapper object 编译与四次断言预处理均 exit 0。没有改变任何原 fixture 的断言，不把此矩阵与 Rust 库、旧 guest 或产品通过数相加。

| 原 fixture | Clang build / run | MSVC build / run |
| --- | --- | --- |
| c_channel.c | 0 / 0 | 0 / 0 |
| cpp_channel.cpp | 0 / 0 | 0 / 0 |
| c_channel_guardpage.c | 0 / 0 | 0 / 0 |
| cpp_channel_guardpage.cpp | 0 / 0 | 0 / 0 |
| c_transport.c | 0 / 0 | 0 / 0 |
| cpp_transport.cpp | 0 / 0 | 0 / 0 |
| cpp_codec.cpp | 0 / 0 | 0 / 0 |

Clang 使用 C11/C++17、-Wall/-Wextra/-Werror；MSVC 使用 /utf-8 /W4 /WX /MD 和 C11/C++17。双方均明确取消 NDEBUG、强制包含断言 guard；四次预处理观察到实际 _wassert。四次 guard-page 程序各记录27个拒绝条件，仅在该进程新分配的两页内存上操作。

327 SDK、57 frozen 原件、W03 五项复用产物、11个 channel vectors、七个 fixture、两个 wrapper 和16个 fixture数据文件测试前后 SHA256 相同。SDK DLL/import library 复用有界 SDK-only W03 产物，wrapper object重新编译；未重建／重封任何旧 guest/provider，也未执行 Cargo、Core/runtime/Workbench 或真实库。

分支 codex/windows-sdk-qualification-20261003，HEAD 63f38d4a8a5bf453248dc7532197bee6980dc86f；HEAD tree a8a74bc4563fae735e9b695f50db4e979ac1393d。8e77d5d6f448291ab3029976aad07a2c4d6b93f8 是此前 W02–W04 待提交源码封存身份，不是本次新生成的树或提交。

## 原始结果与独立复核

W05 result SHA256：f5ddaef3dba7eb076aef5138006c56be03462a6399ded355b99b85bc67648c70。
原 text-evidence ZIP SHA256：272aa90062eca5021482b9ae4bc071a6f0d21fbf735266685f0cf5ab35614cac，181197 bytes／200 entries，manifest 199项。该ZIP保留文本、原始命令／stdout／stderr／exit、producer与断言gate，未包含exe/object/DLL；产物实际身份另有记录。

独立只读复核完成1293项证据比较、blocking 0：44条实际命令回执一致、20项产物及439项输入记录当前摘要一致，ZIP全成员／CRC／SHA一致；没有重新运行编译器或fixture。独立 review-result SHA256：00e5aab2e70f9f1fd7c900c130e1f0060da29d78c26c3d6d474f9e5612693d4b。

cl /Bv 辅助版本诊断真实 exit 2（D8003 missing source filename）原样保留，不重试，也不计为矩阵功能失败。

## token 表述更正及边界

producer 使用普通 Popen/CREATE_NO_WINDOW，14个实际子进程句柄在 wait 前直接取样，均 elevated=true；不是由父进程推断。独立复核未检查取样瞬间进程仍存活，因此原阶段报告的“while live”应限定为“实际子进程句柄在 wait 前直接观察”，不宣称另有瞬时 liveness 证明。原ZIP保持不可变，补充说明另存。

仅获 Windows x64 本地 codec／callback／所有权／合成内存边界的限定证据。普通 token、恶意 native 隔离、默认 committed-tool bounded runner、真正第三方、生产channel／GUI、其他平台及整个SDK冻结未获资格。未绕过runner gate，未提交／推送／CI／发布，未运行外部API或操作真实DB、DPAPI／账户密钥及系统设置。旧阶段的产品通过／失败各绑定其原产物，不能升级为本次资格。
