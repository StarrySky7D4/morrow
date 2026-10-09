'use strict';
const assert=require('node:assert/strict'),d=require('./driver.cjs');
function glyph(label,point){const tree=d.read(),candidates=d.flat(tree).filter(x=>x.n.type==='Text'&&x.n.text===String.fromCodePoint(point));
  const buttons=[...new Set(candidates.map(x=>[...x.parents].reverse().find(n=>n.type==='Button')).filter(Boolean))];assert.equal(buttons.length,1,'One actual glyph button required');
  const [l,t,r,b]=buttons[0].bounds;return d.once(label,['shell','uitest','uiInput','click',Math.round((l+r)/2),Math.round((t+b)/2)]);}
module.exports={glyph};
if(require.main===module){glyph(process.argv[2],Number(process.argv[3]));d.capture(process.argv[4]);}
