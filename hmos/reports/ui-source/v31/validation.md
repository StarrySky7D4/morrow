# v31 发布前验证

2026-10-09，基线 `4c04f97e6beb580d33f14f9540db98129fc50180`。用户要求推送 GitHub；仅交付 `codex/ArkTsUI`，不合并主线。完整追平目标仍 OPEN。

本轮将 Native begin 之前的完整原 FD request 持久确认接入实际 MusicLibrary/Files/Workbench；恢复和显式 retry 保留原 IDs、CAS 和 wire，无自动 FD 重放。同页系统 picker 的一次返回可在前台事件后重新绑定 owner，普通旧异步仍拒绝；歌词全文先绑定原目标，再核对新实际 Store 快照。音乐内层依据 Flutter fill=false 补齐七种风格、阴影/渐变/描边和实际 palette，移除额外底色。详见[持久化与生命周期](music-import-durability-validation.md)和[内层绘制](music-ui-inset-validation.md)。

## 冻结来源与结果

- [完整主机检查](models-final-result.json)：1158/1158 PASS，0 fail/skip/cancel，48 个测试文件，153 项来源前后完全一致，34505.2487ms；[日志](models-final-tests.log) SHA256 `8ADD22306B288D6DFA234410015535C895CE139E90EE08F2465FF7D0D4B5D906`。执行实际 ETS/组件/Index 方法和 Store 产生的 DTO；provider、Native transport、AVPlayer、生命周期和 Canvas rasterizer 为受控边界。
- [持久化组合](music-import-durability-a2-result.json) 114/114、21 来源/20 actual reads；[绘制调用](music-ui-inset-final-result.json) 13/13、5 inputs/4 Flutter 与 SDK refs。均属于上述全量范围，不重复累计。a1 组合 tests 全通过但采集器哈希大小写错误导致资格失败；原失败和日志保留，a2 独立重新资格通过。
- [完整 API26 构建](dev31-music-ui-a1-sdk-result.json)：SUCCESS /33.904s，34/34 tasks 执行、0 up-to-date；[323 复制文件](dev31-music-ui-a1-source-copy.json)和[366 仓库来源](dev31-music-ui-a1-repository-inputs.json)构建前后完全一致。实际产品入口检查并 emit 八音乐模块，非替换入口探针；[包内模块与四原生库](dev31-music-ui-a1-package-check.json) PASS。保留 SDK 平台能力及异常处理警告，不声称所有设备能力支持。
- [Native 精确复用](dev31-music-ui-a1-native-reuse.json)：283 Rust/C++ 来源与 v29 完全一致，ARM64 58,110,214B/DD86DF95…、x64 56,515,718B/E9C66A49…；本轮未重新构建或测试 Rust。四包内 .so 与本次 stripped outputs 一致，不声称 .so 与旧包逐字节相同。
- [未签名 HAP](dev31-music-ui-a1-artifact.json)：**31,438,704B /92258D5D2F38C195383FF7549D58C3876ABA1986C53F69B6BBF32A12D0AE081F**，dev21/1000021，未安装、非签名发布。

## 设备与剩余边界

[设备状态](device-state.json)新鲜只读确认当前安装 dev20/1000020，来自上一轮不可变 v30 HAP，见[安装与启动回执](device/installation-v30.json)。已停止的原模拟器实例从原 snapshot 启动，无 reset/新建；[原草稿读回](device/original-draft-preservation.json)保持 14 卡片/4 草稿及原 title/body，未业务保存、退役或清理。dev20 启动可见不授予本轮 dev21 音乐、UI 或崩溃恢复资格。自生成 WAV/LRC 仅为准备，本轮没有导入或播放它们。

v31 的真实文件选择授权/生命周期、OHOS fsync/关闭、实际重启/断电、解码/声音/seek/EOF、后台互斥、drag、全主题/宽度/像素和所有界面未知框线验收均 **NOT_RUN**。未知或跨重启部分清理残留保持不盲认、不删除。在线歌词、解密、metadata/封面、GC/protected/HUKS/正式宿主、ARM64 运行、签名和完整 Flutter/Windows parity **OPEN**。

准备脚本首次调用使用未匹配的 label，断言在复制/构建/写资格证据前拒绝；随后以脚本实际允许的 dev31-music-ui-a1 执行，未修改生产代码或跳过检查。
