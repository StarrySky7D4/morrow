const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const originalLayout=d.layout;d.layout=()=>{const tree=originalLayout();assert.deepEqual(tree[0].bounds,[0,0,2232,1320],'keep the test emulator in landscape');return tree;};
const out=process.env.HMOS_REPORT_DIR || path.resolve(__dirname,'../reports/ui-source/v6/verified');fs.mkdirSync(out,{recursive:true});
function node(text){const found=d.flatten(d.layout()).find(x=>x.n.text===text);assert.ok(found,text);return found.n;}
function capture(name){const t=d.layout();fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(t,null,2)+'\n');for(let attempt=0;attempt<3;attempt++){try{d.run('screenshot','--path',path.join(out,name+'.png'));break;}catch(error){if(attempt===2 || !String(error).includes('Screenshot was not created on device'))throw error;}}return t;}
if(!process.argv.includes('--panels-only')) {
const first=node('HMOSHMOS-native-check').bounds[1];
const viewport=d.flatten(d.layout()).find(x=>x.n.type==='Scroll').n.bounds;
const x=Math.round((viewport[0]+viewport[2])/2);
d.run('swipe',String(x),String(viewport[3]-150),String(x),String(viewport[1]+180));
const saved=node('HMOSHMOS-native-check').bounds[1];assert.ok(saved<first-100,'workspace actually scrolled');capture('wide-scrolled');
d.click(node('小项目'));
const intro=node('A LITTLE SPACE FOR BIG IDEAS').bounds[1];assert.ok(intro>=viewport[1] && intro<viewport[1]+20,'fresh page starts at top');capture('wide-projects');
d.click(node('概览'));
assert.ok(Math.abs(node('HMOSHMOS-native-check').bounds[1]-saved)<4,'overview offset restored');capture('wide-restored');
d.click(node('⤢'));capture('wide-settings');
d.click(d.flatten(d.layout()).find(x=>x.n.type==='Button').n);
assert.ok(Math.abs(node('HMOSHMOS-native-check').bounds[1]-saved)<4,'settings return restores overview offset');capture('wide-settings-return');
console.log(JSON.stringify({firstCardY:first,scrolledCardY:saved,projectGreetingY:intro,pass:['top-aligned short page','page-specific offset','settings return offset']}));

}
const bounds=d.flatten(d.layout()).find(x=>x.n.type==='Scroll').n.bounds;
const centerX=Math.round((bounds[0]+bounds[2])/2);
d.run('swipe',String(centerX),String(bounds[1]+180),String(centerX),String(bounds[3]-150));
const expandedX=node('A LITTLE SPACE FOR BIG IDEAS').bounds[0];
d.click(d.flatten(d.layout()).find(x=>x.n.type==='Button').n);
assert.ok(!d.flatten(d.layout()).some(x=>x.n.text==='Morrow'),'sidebar hidden');
assert.ok(node('A LITTLE SPACE FOR BIG IDEAS').bounds[0]<expandedX-150,'workspace expands after sidebar collapse');capture('wide-sidebar-collapsed');
d.click(d.flatten(d.layout()).find(x=>x.n.type==='Button').n);
assert.ok(Math.abs(node('A LITTLE SPACE FOR BIG IDEAS').bounds[0]-expandedX)<4,'sidebar restored');
function appearanceButton(){const t=d.layout(),w=t[0].bounds[2];return d.flatten(t).find(x=>x.n.type==='Button'&&x.n.bounds[0]>w-100&&x.n.bounds[1]<100).n;}
d.click(appearanceButton());assert.ok(!d.flatten(d.layout()).some(x=>x.n.text==='空间外观'),'appearance panel hidden');capture('wide-appearance-collapsed');
d.click(appearanceButton());assert.ok(d.flatten(d.layout()).some(x=>x.n.text==='空间外观'),'appearance panel restored');capture('wide-final');
console.log('PASS sidebar and appearance expansion');
