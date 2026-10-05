const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const root = path.resolve(__dirname, '..');
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex').toUpperCase();
const manifestFile = path.join(root, 'reports/build-manifest.json');
const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
manifest.createdUtc = new Date().toISOString();
manifest.hmosVersion = JSON.parse(fs.readFileSync(path.join(root, 'AppScope/app.json5'), 'utf8')).app.versionName;
const version = /hmos-dev\.(\d+)$/.exec(manifest.hmosVersion)?.[1];
if (!version) throw new Error('Expected HMOS development version');
const reportDir = `reports/ui-source/v${version}`;
if (!fs.existsSync(path.join(root, reportDir, 'validation.md'))) throw new Error('Write validation evidence before updating manifest');
const artifact = path.join(root, manifest.artifact.path);
manifest.artifact.bytes = fs.statSync(artifact).size;
manifest.artifact.sha256 = hash(artifact);
const newInputs = ['entry/src/main/ets/model/QueryCoordinator.ets', 'rust/src/query_plan_v2.rs', 'rust/query-plan-reference.json', 'entry/src/main/ets/model/CardDataSource.ets', 'entry/src/main/ets/model/Appearance.ets', 'entry/src/main/ets/model/UiStrings.ets', 'entry/src/main/ets/model/Markdown.ets', 'entry/src/main/ets/pages/ColorWheel.ets', 'entry/src/main/ets/pages/SurfaceRelief.ets'];
const inputs = new Set([...manifest.inputs.map(x => x.path), ...newInputs]);
for (const p of ['entry/src/main/ets/model/EditorDraft.ets', 'rust/src/draft_bridge.rs',
  'rust/src/editor_draft.rs', 'rust/editor-draft-reference.json', 'rust/editor-draft-model/Cargo.toml',
  'rust/editor-draft-model/build.rs', 'rust/editor-draft-model/src/wrapper.rs', 'rust/editor-draft-model/src/lib.rs',
  'rust/editor-draft-model/schemas/editor_draft.proto']) inputs.add(p);
for (const p of ['entry/src/main/ets/model/EditorPaste.ets', 'entry/src/main/ets/pages/MarkdownPreview.ets',
  'rust/src/markdown.rs', 'entry/src/main/ets/model/CardOrder.ets']) inputs.add(p);
for (const p of ['entry/src/main/ets/model/AttachmentFiles.ets', 'entry/src/main/ets/model/Attachments.ets',
  'rust/src/attachment_bridge.rs', 'rust/src/file_stream.rs', 'rust/src/editor_draft_staging.rs',
  'rust/src/bin/hmos-attachment-check.rs', 'rust/src/attachment_integration_tests.rs',
  'rust/src/historical_retry_tests.rs', 'rust/src/editor_draft_staging_tests.rs',
  'rust/editor-draft-staging-reference.json', 'rust/editor-draft-staging-model/Cargo.toml',
  'rust/editor-draft-staging-model/build.rs', 'rust/editor-draft-staging-model/src/wrapper.rs',
  'rust/editor-draft-staging-model/schemas/editor_draft_staging.proto']) inputs.add(p);
manifest.inputs = [...inputs].map(p => ({ path: p, sha256: hash(path.join(root, p)) }));
manifest.uiValidation = `${reportDir}/validation.md`;
manifest.upstreamDrift = 'reports/reference-drift.json';
fs.writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + '\n');
const refFile = path.join(root, 'reports/ui-source/flutter-reference.json');
const ref = JSON.parse(fs.readFileSync(refFile, 'utf8'));
ref.observedUtc = new Date().toISOString();
ref.head = cp.execFileSync('git', ['-C', ref.root, 'rev-parse', 'HEAD'], {encoding:'utf8'}).trim();
ref.note = 'Current source observation. Screenshot provenance and verification scope are recorded in validation.md; the upstream working tree is active, not frozen.';
const extra = ['lib/plugins/query_coordinator.dart','lib/plugins/workbench_ids.dart','workbench_host/src/query_plan_v2.rs','plugins/workbench/src/query_v2.rs','plugins/workbench/src/query_v2_codec.rs','lib/workspace_viewport.dart','lib/stable_masonry_grid.dart','lib/render_stable_masonry_grid.dart','lib/little_tips.dart','lib/visual_style_picker.dart','lib/style_depth_slider.dart','lib/neumorphic_controls.dart','lib/versioned_task_panel.dart','plugins/workbench/src/tasks_v2.rs','lib/component_material_page.dart','lib/color_compass.dart','lib/fonts/font_settings.dart','lib/liquid_glass.dart','lib/music/music_panel.dart'];
extra.push('lib/plugins/editor_draft_session.dart', 'lib/plugins/editor_draft_workspace.dart',
  'workbench_host/src/editor_draft.rs', 'workbench_host/src/editor_draft/model.rs', 'workbench_host/schemas/editor_draft.proto');
extra.push('lib/content/idea_markdown.dart', 'lib/content/rich_content.dart', 'plugins/workbench/src/capture.rs');
extra.push('lib/attachments/attachment_view.dart', 'lib/attachments/clipboard_import.dart',
  'lib/attachments/office_clipboard.dart', 'lib/attachments/file_access_native.dart',
  'workbench_host/src/editor_draft_staging.rs', 'workbench_host/schemas/editor_draft_staging.proto');
extra.push('lib/card_order_preferences.dart', 'lib/pages/card_order.dart', 'lib/hold_reorder.dart',
  'lib/pages/component_menus.dart', 'lib/component_context_menu.dart', 'lib/pages/workspace_pages.dart');
for (const language of ['en','ja','ko','de','fr','es','pt','ru']) extra.push(`packages/morrow_i18n/lib/l10n/app_${language}.arb`);
ref.files = [...new Set([...ref.files.map(x => x.path), ...extra])].map(p => ({ path: p, sha256: hash(path.join(ref.root, p)) }));
fs.writeFileSync(path.join(root, reportDir, 'flutter-reference.json'), JSON.stringify(ref, null, 2) + '\n');
console.log(JSON.stringify({ version: manifest.hmosVersion, bytes: manifest.artifact.bytes, sha256: manifest.artifact.sha256, inputs: manifest.inputs.length }));
