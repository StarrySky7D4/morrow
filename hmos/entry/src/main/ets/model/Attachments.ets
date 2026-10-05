import { AssetSelection } from './EditorDraft';

export class ImportRequest {
  card_id: string = '';
  draft_id: string = '';
  operation_id: string = '';
  expected_generation: string = '0';
  name: string = '';
  kind: string = 'file';
  byte_length: string = '0';
  sha256: string = '';
}
export class ImportRecord {
  request: ImportRequest = new ImportRequest();
  asset_id: string = '';
  phase: string = 'pending';
  current_active: boolean = false;
  bytes_retained: boolean = false;
  staging_revision: string = '0';
  repeated: boolean = false;
}
export class ImportCommand {
  action: string = 'import_file';
  import_request: ImportRequest = new ImportRequest();
}
export interface AssetView {
  id: string; name: string; kind: string; byte_length: string; sha256: string; media_type: string;
}
export function attachmentKind(name: string): string {
  const suffix = name.toLowerCase().split('.').pop() ?? '';
  if (suffix === 'gif') { return 'gif'; }
  if (['png', 'jpg', 'jpeg', 'webp', 'bmp', 'heic', 'avif'].includes(suffix)) { return 'image'; }
  if (['mp4', 'mov', 'mkv', 'webm', 'avi'].includes(suffix)) { return 'video'; }
  if (['mp3', 'flac', 'aac', 'm4a', 'wav', 'ogg'].includes(suffix)) { return 'audio'; }
  return 'file';
}
export function sameImport(a: ImportRequest, b: ImportRequest): boolean {
  return a.card_id === b.card_id && a.draft_id === b.draft_id && a.operation_id === b.operation_id &&
    a.expected_generation === b.expected_generation && a.name === b.name && a.kind === b.kind &&
    a.byte_length === b.byte_length && a.sha256 === b.sha256;
}
export function selectedImport(record: ImportRecord, request: ImportRequest): AssetSelection {
  if (!sameImport(record.request, request) || record.phase !== 'ready' || !record.current_active ||
    !record.bytes_retained || !record.asset_id || !/^[0-9a-f]{64}$/.test(request.sha256)) {
    throw new Error('附件尚未确认可选，原导入记录已保留。');
  }
  const selected = new AssetSelection(); selected.origin = 2; selected.asset_id = record.asset_id;
  return selected;
}
export function attachmentSize(bytes: string): string {
  const value = Number(bytes);
  return value < 1024 ? bytes + ' B' : value < 1024 * 1024 ? (value / 1024).toFixed(1) + ' KiB' :
    (value / (1024 * 1024)).toFixed(1) + ' MiB';
}
// Use the same MaterialIcons glyphs as Flutter AttachmentTile.
export function attachmentIcon(asset: AssetView): number {
  if (asset.kind === 'image' || asset.kind === 'gif') { return 0xf120; }
  if (asset.kind === 'video') { return 0xf1f5; }
  if (asset.kind === 'audio') { return 0xeea6; }
  const suffix = asset.name.toLowerCase().split('.').pop() ?? '';
  return ['blend', 'obj', 'fbx', 'stl', 'dwg', 'dxf', 'step', 'stp'].includes(suffix) ? 0xf497 : 0xf12d;
}
export function attachmentDetails(asset: AssetView): string {
  const dot = asset.name.lastIndexOf('.');
  const extension = dot > 0 ? asset.name.slice(dot + 1).toUpperCase() : '';
  return (extension ? extension + ' · ' : '') + attachmentSize(asset.byte_length);
}
