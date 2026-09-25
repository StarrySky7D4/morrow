// Persistent keys in daemon-media/textures. Object URLs and workbench previews
// are session-owned and must never be persisted as appearance/music sources.
final _key = RegExp(
  r'^(?:[0-9]{1,20}|media-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$',
);
// The existing workbench contract represents local sources as file URIs.
// This virtual directory maps exclusively to browser database keys; it is
// never passed to the OS filesystem or fetched over the network.
const _prefix = 'file:///__morrow_browser_media__/';

String encodeLocalSource(String key) {
  if (!_key.hasMatch(key)) {
    throw const FormatException('Expected a persistent browser media key');
  }
  return '$_prefix$key';
}

String decodeLocalSource(String value) {
  if (!value.startsWith(_prefix)) {
    throw const FormatException('This media requires import on this device');
  }
  final key = value.substring(_prefix.length);
  if (!_key.hasMatch(key)) {
    throw const FormatException('Invalid persistent browser media reference');
  }
  return key;
}
