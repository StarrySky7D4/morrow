'use strict';
const assert=require('node:assert/strict'),d=require('./driver.cjs'),a=require('./actions.cjs');
const labels=['扁平 · 默认','Neumorphism · 新拟态','纸感','黏土','Fluent 流畅','粗野主义','工业风'];
const current=process.argv[2],target=process.argv[3],label=process.argv[4];assert.ok(labels.includes(current)&&labels.includes(target)&&current!==target);assert.match(label,/^dev22-[a-z0-9-]+$/);
assert.equal(d.flat(d.read()).filter(x=>x.n.type==='Text'&&x.n.text===current).length,1,'Settings picker must be closed and current style observed');
d.click(label+'-open-picker',{text:current,type:'Text'});
for(let index=0;index<4;index++){
  const nodes=d.flat(d.read()),scroll=nodes.filter(x=>x.n.type==='Scroll'&&x.n.bounds[3]>x.n.bounds[1]);assert.equal(scroll.length,1);
  const [l,t,r,b]=scroll[0].n.bounds;
  const matches=nodes.filter(x=>x.n.type==='Text'&&x.n.text===target&&x.n.bounds[1]>=t&&x.n.bounds[3]<=b&&x.n.bounds[3]>x.n.bounds[1]);
  if(matches.length===1){d.click(label+'-select',{text:target,type:'Text'});break;}
  assert.equal(matches.length,0);assert.ok(index<3,'No fully visible observed style target');
  d.once(label+'-picker-scroll-'+index,['shell','uitest','uiInput','swipe',Math.round((l+r)/2),b-120,Math.round((l+r)/2),t+120,350]);
}
d.capture(label+'-settings-a');d.capture(label+'-settings-b');
// Picker scrolling may leave the underlying Settings at a lower offset.
// Inspect again after closing/reopening; absence offscreen is not selection failure.
a.glyph(label+'-close-settings',0xf572);d.capture(label+'-home-a');d.capture(label+'-home-b');
a.glyph(label+'-reopen-settings',0xf0258);
d.capture(label+'-settings-restored-a');d.capture(label+'-settings-restored-b');
assert.equal(d.flat(d.read()).filter(x=>x.n.type==='Text'&&x.n.text===target).length,1,'Chosen style must be displayed after Settings is reopened');
