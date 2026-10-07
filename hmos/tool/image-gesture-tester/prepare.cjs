const fs = require('node:fs'), path = require('node:path');

function prepare(destination) {
  const work = path.resolve(__dirname, 'work');
  const project = path.resolve(destination || path.join(work, 'build-' + Date.now()));
  if (!project.startsWith(work + path.sep) || project === work) throw new Error('Tester builds must stay in the owned work directory');
  if (fs.existsSync(project)) throw new Error('Existing tester project is preserved; choose a fresh build directory');
  const files = {};
  const json = (name, value) => { files[name] = JSON.stringify(value, null, 2) + '\n'; };
  json('build-profile.json5', { app: { signingConfigs: [], products: [{ name: 'default', targetSdkVersion: '26.0.0',
    compatibleSdkVersion: '26.0.0', runtimeOS: 'HarmonyOS', buildOption: { strictMode: { caseSensitiveCheck: true, useNormalizedOHMUrl: true } } }],
    buildModeSet: [{ name: 'debug' }, { name: 'release' }] }, modules: [{ name: 'entry', srcPath: './entry',
    targets: [{ name: 'default', applyToProducts: ['default'] }] }] });
  json('oh-package.json5', { modelVersion: '26.0.0', description: 'Independent image multi-pointer test harness', dependencies: {}, devDependencies: {} });
  json('hvigor/hvigor-config.json5', { modelVersion: '26.0.0', dependencies: {}, execution: { daemon: false } });
  files['hvigorfile.ts'] = "import { appTasks } from '@ohos/hvigor-ohos-plugin';\nexport default { system: appTasks, plugins: [] };\n";
  files['entry/hvigorfile.ts'] = "import { hapTasks } from '@ohos/hvigor-ohos-plugin';\nexport default { system: hapTasks, plugins: [] };\n";
  json('entry/build-profile.json5', { apiType: 'stageMode', buildOption: { resOptions: { copyCodeResource: { enable: false } } },
    targets: [{ name: 'default' }, { name: 'ohosTest' }] });
  json('entry/oh-package.json5', { name: 'entry', version: '1.0.0', description: 'Non-launching UITest runner', dependencies: {} });
  json('AppScope/app.json5', { app: { bundleName: 'dev.morrow.hmos.gesturetester', vendor: 'Morrow Test',
    versionCode: 200001, versionName: '0.1.0-image-tester.20', icon: '$media:probe_icon', label: '$string:app_name' } });
  json('AppScope/resources/base/element/string.json', { string: [{ name: 'app_name', value: 'Morrow Gesture Tester' }] });
  files['AppScope/resources/base/media/probe_icon.svg'] = '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" rx="12" fill="#7662BA"/><circle cx="23" cy="32" r="7" fill="white"/><circle cx="41" cy="32" r="7" fill="white"/></svg>\n';
  json('entry/src/main/module.json5', { module: { name: 'entry', type: 'entry', description: '$string:module_desc',
    mainElement: 'ProbeAbility', deviceTypes: ['phone', 'tablet'], deliveryWithInstall: true, installationFree: false,
    pages: '$profile:main_pages', abilities: [{ name: 'ProbeAbility', srcEntry: './ets/entryability/ProbeAbility.ets',
      exported: false, label: '$string:app_name', icon: '$media:probe_icon',
      startWindowIcon: '$media:probe_icon', startWindowBackground: '$color:start_window_background' }] } });
  json('entry/src/main/resources/base/element/string.json', { string: [{ name: 'module_desc', value: 'Independent UITest harness' }] });
  json('entry/src/main/resources/base/element/color.json', { color: [{ name: 'start_window_background', value: '#FFFFFF' }] });
  json('entry/src/main/resources/base/profile/main_pages.json', { src: ['pages/ProbePage'] });
  files['entry/src/main/ets/entryability/ProbeAbility.ets'] = "import { UIAbility } from '@kit.AbilityKit';\nexport default class ProbeAbility extends UIAbility {}\n";
  files['entry/src/main/ets/pages/ProbePage.ets'] = "@Entry\n@Component\nstruct ProbePage { build() { Column() { Text('Independent UITest harness') }.width('100%').height('100%') } }\n";
  json('entry/src/ohosTest/module.json5', { module: { name: 'entry_test', type: 'feature', description: '$string:module_test_desc',
    mainElement: 'ProbeTestAbility', deviceTypes: ['phone', 'tablet'], deliveryWithInstall: true, installationFree: false,
    abilities: [{ name: 'ProbeTestAbility', srcEntry: './ets/testability/ProbeTestAbility.ets', exported: false,
      label: '$string:app_name', icon: '$media:probe_icon', startWindowIcon: '$media:probe_icon',
      startWindowBackground: '$color:start_window_background' }] } });
  json('entry/src/ohosTest/resources/base/element/string.json', { string: [{ name: 'module_test_desc', value: 'Image multi-pointer probes' }] });
  files['entry/src/ohosTest/ets/testability/ProbeTestAbility.ets'] = "import { UIAbility } from '@kit.AbilityKit';\nexport default class ProbeTestAbility extends UIAbility {}\n";
  files['entry/src/ohosTest/ets/testrunner/ImageGestureRunner.ets'] = fs.readFileSync(path.join(__dirname, 'ImageGestureRunner.ets'), 'utf8');
  files['entry/src/ohosTest/ets/testrunner/ImagePointerPlan.ets'] = fs.readFileSync(path.join(__dirname, 'ImagePointerPlan.ets'), 'utf8');
  for (const [name, content] of Object.entries(files)) {
    const target = path.join(project, name); fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content, { encoding: 'utf8', flag: 'wx' });
  }
  return { project, bundleName: 'dev.morrow.hmos.gesturetester', testModule: 'entry_test',
    testRunner: '/ets/testrunner/ImageGestureRunner', files: Object.keys(files).length,
    installation: 'NOT_RUN', device: 'NOT_RUN' };
}
module.exports = { prepare };
if (require.main === module) process.stdout.write(JSON.stringify(prepare(process.argv[2]), null, 2) + '\n');
