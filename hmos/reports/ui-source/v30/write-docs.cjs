'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const report = __dirname, root = path.resolve(report, '../../..');
const read = n => JSON.parse(fs.readFileSync(path.join(report, n)));
const models = read('models-final2-result.json'), sdk = read('dev30-music-ui-a2-sdk-result.json'), artifact = read('dev30-music-ui-a2-artifact.json'), pkg = read('dev30-music-ui-a2-package-check.json');
assert.equal(models.exitCode, 0); assert.equal(models.sourceIdentityVerified, true); assert.equal(sdk.status, 'PASS'); assert.equal(pkg.status, 'PASS');
assert.equal(models.passed, 1118); assert.equal(pkg.modules.length, 8);
function summary(link) { return `2026-10-09 **v30 当前音乐界面检查点**：实际 Index 接入 MusicWorkbench、文件选择与完整原件导入、播放列表/持久选曲、播放/暂停/seek、排序/移除、离线歌词导入/全文弹层及歌词页脚。界面布局依据 Flutter 音乐面板和 little_tips 源码；外层沿用现有组件材质，默认页脚透明。恢复只读取、不创建播放器或自动播放。后台暂停保留原播放器/FD，返回不自动续播；附件媒体须先确认音乐实际暂停。Unknown 不阻止原播放器停音，原导入核对不生成新 ID；未保存全文绑定原曲目，已知部分缓存清理只重试同原对象。

最终实际 ETS/tool **${models.passed}/${models.tests} PASS，0 fail/skip/cancel**（${models.testFiles} suite 文件、${models.inputs} 项输入前后一致，${models.durationMs}ms）；完整 API26 **SUCCESS /17.975s**，34/34 tasks 执行，323 复制/366 仓库输入一致；**八音乐模块均在实际产品 entry graph 检查并 emit**，四项包内原生库核对 PASS。Rust/C++ 实现未变，283 项 native 来源和双 ABI 静态库精确复用 v29；本轮不新增 Rust 测试/构建资格。最终 HAP **${artifact.bytes.toLocaleString('en-US')}B**，SHA256 \`${artifact.sha256}\`。

版本 **dev20/1000020**，包 unsigned/uninstalled，当前 HDC target 为空、模拟器进程未运行，设备验收 **NOT_RUN**。实际文件提供者/音频 codec/声音/seek/拖拽/全样式渲染、pre-begin 导入持久恢复、重启后部分清理残留、在线歌词/解密/封面/原件 GC/protected 与完整 Flutter/Windows parity 均 **OPEN**。交付仅 \`codex/ArkTsUI\`，不并入主线。详见 [v30 验证](${link})。

以下 v29 及更早内容保留为历史；各轮数字和“当前”只指其当轮范围。本轮资格以上述 v30 摘要、最新表和验证记录为准。

`; }
for (const name of ['README.md', 'docs/PARITY.md', 'docs/ALIGNMENT_PLAN.md']) {
  const file = path.join(root, name), old = fs.readFileSync(file, 'utf8'); assert.ok(!old.includes('**v30 当前音乐界面检查点**'));
  const heading = /^#[^\r\n]+\r?\n(?:\r?\n)+/.exec(old); assert.ok(heading);
  const at = heading[0].length;
  let updated = old.slice(0, at) + summary(name === 'README.md' ? 'reports/ui-source/v30/validation.md' : '../reports/ui-source/v30/validation.md') + old.slice(at);
  if (name === 'README.md') updated = updated.replace('文末音乐/分发当前表按 v29 更新，本轮资格以以上摘要及验证记录为准。', '该段仅记录 v29 历史资格。');
  if (name === 'docs/PARITY.md') updated = updated.replace(/^\| 歌词\/媒体与格式解密 \|.*$/m,
    '| 歌词/媒体与格式解密 | Flutter music_panel/little_tips + Native 独立曲库 | v30 实际 Index 接 Library/Files/Playback/Platform 与面板/页脚/全文弹层，八模块产品 graph 检查/emit；离线标准原件、持久选择/排序/退役、后台/媒体互斥与 Unknown 原停音有有界主机覆盖。设备 codec/声音/拖拽/渲染 NOT_RUN；pre-begin/重启部分清理/在线/解密/metadata/封面/GC/protected/full parity OPEN，见[v30验证](../reports/ui-source/v30/validation.md) |');
  if (name === 'docs/ALIGNMENT_PLAN.md') {
    updated = updated.replace(/^\| 音乐、歌词、解密 \|.*$/m,
      '| 音乐、歌词、解密 | v30 实际 Index 音乐操作、播放列表、离线歌词全文/页脚；复用实际 Library/Files/Playback 与 Rust 曲库，八音乐模块进入实际产品图，后台/Unknown 停音、原请求恢复和已知部分清理有界覆盖 | 下一阶段验真实提供者/声音/seek/上下首/后台/中断/重启/FD与全部样式渲染；补 pre-begin/重启部分清理/在线/解密/metadata/封面/GC/protected/full parity，见[v30验证](../reports/ui-source/v30/validation.md) |');
    updated = updated.replace(/^\| 分发与资格 \|.*$/m,
      '| 分发与资格 | dev20/1000020，仅codex/ArkTsUI。v30 final1118模型/151来源，API26 17.975s/34执行任务/323复制/366仓库输入及8模块emit/4包库核对PASS；31,413,203B/FB81FEF1… unsigned/uninstalled；283 native 来源与双ABI精确复用v29，本轮无新Rust测试/构建，见[v30验证](../reports/ui-source/v30/validation.md) | HDC target空且模拟器进程未运行，设备NOT_RUN；签名/ARM64运行/HUKS与完整对齐OPEN；a1 SDK失败及模型final来源drift保留，资格只授 final2 + a2产品包 |');
  }
  fs.writeFileSync(file, updated);
}
const validation = `# v30 Music UI integration

2026-10-09，基线 \`c7a6efbc51c0615fccd29a65d745dfeb4a17f321\`，仅 \`codex/ArkTsUI\`。目标仍为逐步追平实际 Flutter/Windows 界面和大部分实现，未授予完整产品通过。

实际 Index 现接入 MusicWorkbench → Library/Files/Playback/Platform；文件 picker、整件 FD import/export、持久曲库和选曲/重排/退役复用 v29 Native 实现。音乐面板按既有 Flutter 源码布局，播放列表折叠保留组件/scroll，拖动以真实 track/source/revision 围栏；默认 footer 无独立框/blur/阴影。歌词全文弹层保全已保存或未保存原文，未保存候选拥有原曲目 ID/标题；Native 原限额保留，超限不裁剪、不冒称保存。

Library owner 为页面实例与前台 epoch，播放器 owner 仅页面实例，前后台独立通报。启动不打开音频/不写 selected/不自动播放。后台清空旧播放意图，迟到 admission/acquire/factory/load 不复活旧播放；保留既有 player/FD。附件播放前实际等待音乐 pause ACK，失败不得启动另一播放器；曲库/歌词 Unknown 不阻止已有音乐暂停。重排/移除其他曲目保留稳定音源，原移除写入确认后关闭已退役的 owned player，不从恢复流程自动播放其他曲目。

## 当前资格

- [完整主机结果](models-final2-result.json)：${models.passed}/${models.tests} PASS，0 fail/skip/cancel，${models.testFiles} 测试文件，${models.inputs} 实际来源前后逐字节一致，${models.durationMs}ms；[原日志](models-final2-tests.log) SHA256 \`${models.sha256}\`。提供者/Store transport/AVPlayer/组件 rendering 和 lifecycle completion 为受控 seam，不能代替真实设备。
- 分项属于上述全量范围，不重复累计：[播放器77](music-playback-lifecycle-sdk-hook-validation.md)、[文件清理22](music-files-cleanup-validation.md)、[组件25](music-ui-components-validation.md)、[实际 composition65](music-workbench-final-result.json)，以及实际 Index 7 项。Library 只做 nominal 继承类型声明修复，其行为仍由完整原 DTO 与模型检查覆盖。
- [完整产品 SDK](dev30-music-ui-a2-sdk-result.json)：SUCCESS 17.975s，34 tasks 全执行；[323复制输入](dev30-music-ui-a2-source-copy.json)、[366仓库来源](dev30-music-ui-a2-repository-inputs.json) 与当前源码一致。八实际音乐模块列在产品 filesInfo，均生成 .ts/protoBin；不是改入口的探针，见[产品模块/原生库核对](dev30-music-ui-a2-package-check.json)。
- [Native 精确复用](dev30-music-ui-a2-native-reuse.json)：283 来源与 v29 静态库一致；本轮未重新构建或跑 Rust。ARM64 58,110,214B/DD86DF95…，x64 56,515,718B/E9C66A49…；新包的四项原生库与本次实际 stripped outputs 逐字节一致。
- [最终 HAP](dev30-music-ui-a2-artifact.json)：${artifact.bytes.toLocaleString('en-US')}B / \`${artifact.sha256}\`，dev20/1000020，unsigned/uninstalled。设备状态见[当前观察](device-state.json)：target 空、模拟器进程无，本轮设备 NOT_RUN；旧 v27 设备观察不可授予本包资格。

## 失败记录与边界

初次 [a1 SDK](dev30-music-ui-a1-sdk-result.json) 编译失败，涉及函数接口 literal、nominal identity 类型、组件 position 名称冲突和 Progress 缺 value；修复后 a2 完整编译通过，原 source copy/日志保留。package verifier 初版把 filesInfo 的实际 .ts 当作 .ets 搜索而拒绝；检查脚本按真实输出后核对八模块及原生库通过，没有重写 HAP。

[models-final](models-final-result.json) 1118 项全部通过，但测试文件运行中增加两项歌词归属断言，来源 drift，资格拒绝。冻结后重跑 final2，全部来源一致。Controller a1 旧完整 spool 预期与 releaseStarted 部分清理语义不符，旧失败结果保留，修正断言并增同原对象恢复检查后 final65通过。所有 raw logs 保留原字节。

已知 partial-release 仅在安全路径/普通文件检查完成后、首次 unlink 前标记；同进程 recover 保留原registry身份但不列为完整可再导入 spool，原 Ready 计划以同对象 finish 重试。未知重启残留不盲认/删除，跨重启该清理协议 OPEN。导入 Native begin 前的完整持久 wire 尚未单独落盘；缺 sidecar 且真实 Pending 完整 request/meta 精确匹配时只给明确原候选选择，不借此授 pre-begin 全崩溃恢复。原 wire 保存后只核对原请求，不创建替代 operation/CAS/track；Unknown 不自动 FD 重放。

仍需设备验证 picker/授权/FD、标准文件解码和声音、seek、上下首/EOF、后台/返回/附件互斥、drag、全部主题/材质/宽度/框线和重启。在线歌词、解密、metadata/封面、原件 GC、protected/HUKS/正式宿主、ARM64实际运行、签名和完整 Flutter/Windows parity OPEN。
`;
fs.writeFileSync(path.join(report, 'validation.md'), validation, { flag: 'wx' });
console.log(JSON.stringify({ status: 'PASS', documents: 4, models: models.passed, artifact: artifact.sha256, device: 'NOT_RUN' }));
