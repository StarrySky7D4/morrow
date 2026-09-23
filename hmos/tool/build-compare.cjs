const fs = require('node:fs'), path = require('node:path');
const root = path.resolve(__dirname, '../reports/ui-source');
const pages = [['home','首页','mobile'],['inbox','收件箱','inbox'],['projects','小项目','projects'],['lab','实验室','lab'],['favorites','收藏','favorites'],['settings','外观设置','settings'],['font','字体','font'],['editor','编辑弹窗','editor']];
for (const [id,,ref] of pages) {
  for (const file of [`v3/flutter-${ref}.png`,`v3/final/${id}.png`]) {
    if (!fs.existsSync(path.join(root,file))) throw Error(`Missing screenshot: ${file}`);
  }
}
const html = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Morrow · Flutter / HMOS dev.3</title>
<style>body{margin:0;background:#f9f8fc;color:#302d43;font:15px system-ui,"Microsoft YaHei",sans-serif}main{max-width:1160px;margin:auto;padding:28px}h1{font-size:26px}p{color:#777184;line-height:1.8}nav{display:flex;flex-wrap:wrap;gap:8px;margin:24px 0}button{padding:10px 17px;border:1px solid #ded8ea;border-radius:11px;background:white;color:#7662ba;cursor:pointer}button[aria-selected=true]{background:#7662ba;color:white}.pair{display:grid;grid-template-columns:1fr 1fr;gap:20px}.pair[hidden]{display:none}figure{margin:0;background:white;border:1px solid #ded8ea;border-radius:18px;overflow:hidden}figcaption{padding:16px;font-weight:600;display:flex;justify-content:space-between;gap:8px}a{color:#7662ba}figcaption a{font-size:12px;font-weight:400}.screen{aspect-ratio:440/676;overflow:hidden}img{display:block;width:100%;height:auto}.hmos img{margin-top:-8.864%}footer{margin-top:24px;color:#777184;font-size:13px;line-height:1.9}@media(max-width:600px){main{padding:14px}.pair{gap:8px}figcaption{padding:10px;font-size:12px}figcaption a{font-size:10px}button{padding:8px 12px}}</style>
<main><h1>对照 Flutter 源码补齐 UI</h1><p>左侧直接运行活跃 Flutter 工作树的 MorrowApp；右侧为 HMOS dev.3 最终包的真实模拟器截图。此页裁去鸿蒙系统栏以对齐 440 逻辑像素的应用区域，原图保留完整系统栏。数据与系统字体不同，不作逐像素等价声明。</p>
<nav role="tablist" aria-label="页面">${pages.map(([id,label],i)=>`<button role="tab" aria-controls="${id}" aria-selected="${i===0}" onclick="show('${id}',this)">${label}</button>`).join('')}</nav>
${pages.map(([id,label,ref],i)=>`<section class="pair" role="tabpanel" id="${id}" ${i?'hidden':''}><figure><figcaption>Flutter · ${label}<a href="v3/flutter-${ref}.png">原图</a></figcaption><div class="screen"><img src="v3/flutter-${ref}.png" alt="Flutter ${label}"></div></figure><figure><figcaption>HMOS dev.3 · ${label}<a href="v3/final/${id}.png">原图</a></figcaption><div class="screen hmos"><img src="v3/final/${id}.png" alt="HMOS ${label}"></div></figure></section>`).join('\n')}
<footer>已补入专用分类卡片、材质子页、色盘、字体与语言选择、日常清单、音乐空状态和正文预览。折射 shader、附件/媒体、字体文件导入、完整 Markdown、完整九语动态文案及原全部动效仍有差距。<br><a href="../../docs/UI_DESIGN_DEV3.md">实现与边界</a> · <a href="v3/validation.md">验证记录</a> · <a href="../build-manifest.json">构建哈希</a></footer></main>
<script>function show(id,button){document.querySelectorAll('.pair').forEach(e=>e.hidden=e.id!==id);document.querySelectorAll('[role=tab]').forEach(e=>e.setAttribute('aria-selected',String(e===button)));}</script></html>`;
fs.writeFileSync(path.join(root,'compare.html'),html);
console.log(`Verified ${pages.length} screenshot pairs.`);
