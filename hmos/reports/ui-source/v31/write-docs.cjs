'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const repo = path.resolve(__dirname, '../../../..');
const read = name => JSON.parse(fs.readFileSync(path.join(__dirname, name)));
const models = read('models-final-result.json'), sdk = read('dev31-music-ui-a1-sdk-result.json');
const artifact = read('dev31-music-ui-a1-artifact.json'), packaged = read('dev31-music-ui-a1-package-check.json');
assert.equal(models.passed, 1158); assert.ok(models.sourceIdentityVerified); assert.equal(sdk.status, 'PASS'); assert.equal(packaged.status, 'PASS');
const summary = link => `2026-10-09 **v31 当前检查点**：补齐音乐内层七种 Flutter 风格，当前曲目及选中行保持透明，flat 不增加框线；Index 传入实际色盘 surface。完整原导入请求须写入、fsync、关闭全部确认后才派发 Native begin，恢复保留原 IDs/CAS 并显式核对。同一页面的文件选择器往返使用一次性 ticket，返回前台后重读实际曲库；歌词目标变化时保留原目标下的完整原文。

完整实际 ETS/tool **1158/1158 PASS，0 fail/skip/cancel**，48 测试文件、153 输入前后一致；API26 **SUCCESS /33.904s**，34 tasks 全执行，323 复制/366 仓库输入一致，八音乐模块实际检查并 emit，四项包内原生库核对 PASS。283 native 来源和双 ABI 静态库精确复用 v29，本轮没有新 Rust 构建或测试资格。版本 **dev21/1000021**，HAP **31,438,704B /92258D5D…**，unsigned/uninstalled。

新版本设备验收 **NOT_RUN**；现有模拟器已安装上一轮 dev20，旧草稿全文读回和 dev20 启动仅为限定证据。真实 picker/grant、声音/seek/后台/重启、全样式像素与所有界面框线仍待验证；在线歌词、解密、封面、原件 GC/protected 和完整 Flutter/Windows 对齐 **OPEN**。仅推送 \`codex/ArkTsUI\`，不并入主线，见 [v31 验证](${link})。

以下 v30 及更早段落均为历史记录；旧段落中的“当前”、设备状态和资格仅指其当轮范围。最新资格以上述 v31 及当前表为准。

`;
for (const relative of ['hmos/README.md', 'hmos/docs/PARITY.md', 'hmos/docs/ALIGNMENT_PLAN.md']) {
  const file = path.join(repo, relative); let text = fs.readFileSync(file, 'utf8');
  assert.ok(!text.includes('**v31 当前检查点**'));
  const header = text.match(/^#[^\r\n]*\r?\n\r?\n/); assert.ok(header);
  const link = relative === 'hmos/README.md' ? 'reports/ui-source/v31/validation.md' : '../reports/ui-source/v31/validation.md';
  text = header[0] + summary(link) + text.slice(header[0].length);
  if (relative.endsWith('PARITY.md')) {
    text = text.replace(/^\| 歌词\/媒体与格式解密[^\r\n]*$/m, '| 歌词/媒体与格式解密 | Flutter music_panel/little_tips + Native 独立曲库 | v31 原导入 wire 先持久确认、picker 单次前台接续、七种内层风格及透明 child；actual Index/八模块产品图覆盖。设备 codec/声音/拖拽/全样式/框线/崩溃恢复 NOT_RUN；重启部分清理/在线/解密/metadata/封面/GC/protected/full parity OPEN，见[v31验证](../reports/ui-source/v31/validation.md) |');
    text = text.replace(/^\| 平台分发[^\r\n]*$/m, '| 平台分发 | DevEco API26 | dev21/1000021，仅codex/ArkTsUI、不并main。v31 1158/1158模型、153输入一致；283 native 来源/双ABI库精确复用v29。完整产品API26 SUCCESS33.904s/34执行任务，323复制/366输入及4包库核对PASS，HAP31,438,704B/92258D5D… unsigned/uninstalled；设备仅上一轮dev20安装启动，v31新设备NOT_RUN，签名/ARM64运行/HUKS/full parity OPEN，见[v31验证](../reports/ui-source/v31/validation.md) |');
  }
  if (relative.endsWith('ALIGNMENT_PLAN.md')) text = text.replace(/^\| 音乐、歌词、解密[^\r\n]*$/m, '| 音乐、歌词、解密 | v31 完整原 wire 派发前持久确认、同页 picker ticket/完整原目标歌词、七种内层风格；保留实际 Library/Files/Playback/Rust 复用 | 验真实提供者/声音/seek/上下首/后台/中断/重启/FD与全部样式和框线；补重启部分清理/在线/解密/metadata/封面/GC/protected/full parity，见[v31验证](../reports/ui-source/v31/validation.md) |');
  fs.writeFileSync(file, text);
}
const validation = `# v31 发布前验证

2026-10-09，基线 \`4c04f97e6beb580d33f14f9540db98129fc50180\`。用户要求推送 GitHub；仅交付 \`codex/ArkTsUI\`，不合并主线。完整追平目标仍 OPEN。

本轮将 Native begin 之前的完整原 FD request 持久确认接入实际 MusicLibrary/Files/Workbench；恢复和显式 retry 保留原 IDs、CAS 和 wire，无自动 FD 重放。同页系统 picker 的一次返回可在前台事件后重新绑定 owner，普通旧异步仍拒绝；歌词全文先绑定原目标，再核对新实际 Store 快照。音乐内层依据 Flutter fill=false 补齐七种风格、阴影/渐变/描边和实际 palette，移除额外底色。详见[持久化与生命周期](music-import-durability-validation.md)和[内层绘制](music-ui-inset-validation.md)。

## 冻结来源与结果

- [完整主机检查](models-final-result.json)：1158/1158 PASS，0 fail/skip/cancel，48 个测试文件，153 项来源前后完全一致，34505.2487ms；[日志](models-final-tests.log) SHA256 \`${models.sha256}\`。执行实际 ETS/组件/Index 方法和 Store 产生的 DTO；provider、Native transport、AVPlayer、生命周期和 Canvas rasterizer 为受控边界。
- [持久化组合](music-import-durability-a2-result.json) 114/114、21 来源/20 actual reads；[绘制调用](music-ui-inset-final-result.json) 13/13、5 inputs/4 Flutter 与 SDK refs。均属于上述全量范围，不重复累计。a1 组合 tests 全通过但采集器哈希大小写错误导致资格失败；原失败和日志保留，a2 独立重新资格通过。
- [完整 API26 构建](dev31-music-ui-a1-sdk-result.json)：SUCCESS /33.904s，34/34 tasks 执行、0 up-to-date；[323 复制文件](dev31-music-ui-a1-source-copy.json)和[366 仓库来源](dev31-music-ui-a1-repository-inputs.json)构建前后完全一致。实际产品入口检查并 emit 八音乐模块，非替换入口探针；[包内模块与四原生库](dev31-music-ui-a1-package-check.json) PASS。保留 SDK 平台能力及异常处理警告，不声称所有设备能力支持。
- [Native 精确复用](dev31-music-ui-a1-native-reuse.json)：283 Rust/C++ 来源与 v29 完全一致，ARM64 58,110,214B/DD86DF95…、x64 56,515,718B/E9C66A49…；本轮未重新构建或测试 Rust。四包内 .so 与本次 stripped outputs 一致，不声称 .so 与旧包逐字节相同。
- [未签名 HAP](dev31-music-ui-a1-artifact.json)：**${artifact.bytes.toLocaleString('en-US')}B /${artifact.sha256}**，dev21/1000021，未安装、非签名发布。

## 设备与剩余边界

[设备状态](device-state.json)新鲜只读确认当前安装 dev20/1000020，来自上一轮不可变 v30 HAP，见[安装与启动回执](device/installation-v30.json)。已停止的原模拟器实例从原 snapshot 启动，无 reset/新建；[原草稿读回](device/original-draft-preservation.json)保持 14 卡片/4 草稿及原 title/body，未业务保存、退役或清理。dev20 启动可见不授予本轮 dev21 音乐、UI 或崩溃恢复资格。自生成 WAV/LRC 仅为准备，本轮没有导入或播放它们。

v31 的真实文件选择授权/生命周期、OHOS fsync/关闭、实际重启/断电、解码/声音/seek/EOF、后台互斥、drag、全主题/宽度/像素和所有界面未知框线验收均 **NOT_RUN**。未知或跨重启部分清理残留保持不盲认、不删除。在线歌词、解密、metadata/封面、GC/protected/HUKS/正式宿主、ARM64 运行、签名和完整 Flutter/Windows parity **OPEN**。

准备脚本首次调用使用未匹配的 label，断言在复制/构建/写资格证据前拒绝；随后以脚本实际允许的 dev31-music-ui-a1 执行，未修改生产代码或跳过检查。
`;
fs.writeFileSync(path.join(__dirname, 'validation.md'), validation, { flag: 'wx' });
console.log(JSON.stringify({status:'PASS', docs:3, validation:'v31/validation.md'}));
