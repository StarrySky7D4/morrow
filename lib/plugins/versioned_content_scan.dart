import 'versioned_content.dart';

/// Reads the entire typed content library without creating migration or edit intents.
///
/// [knownRevisions] is a synchronous snapshot of revisions observed by another
/// path in the same session. It is checked once more without an await before the
/// result is published, so a concurrent new or updated card is not omitted.
Future<List<VersionedContentRecord>> scanVersionedContent(
  VersionedContentControl content, {
  Map<String, BigInt> Function()? knownRevisions,
  int pageLimit = 128,
}) async {
  if (pageLimit < 1 || pageLimit > 4096) {
    throw const FormatException('Invalid versioned scan page limit');
  }
  final maxU64 = (BigInt.one << 64) - BigInt.one;
  final records = <String, VersionedContentRecord>{};
  final seenIds = <String>{};
  final seenCursors = <String>{''};
  var cursor = '';

  void validateRevision(BigInt revision) {
    if (revision <= BigInt.zero || revision > maxU64) {
      throw const FormatException('Invalid versioned content revision');
    }
  }

  Future<void> readInto(String id) async {
    final record = await content.read(id);
    if (record.id != id) {
      throw const FormatException('Versioned read returned a different ID');
    }
    if (record.formatVersion != 1 && record.formatVersion != 2) {
      throw const FormatException('Unsupported versioned content format');
    }
    validateRevision(record.revision);
    final previous = records[id];
    if (previous != null && record.revision < previous.revision) {
      throw StateError('Versioned content revision went backwards');
    }
    records[id] = record;
  }

  while (true) {
    final page = await content.page(cursor: cursor, limit: pageLimit);
    if (page.ids.length > pageLimit) {
      throw const FormatException('Versioned page exceeds requested limit');
    }
    for (final id in page.ids) {
      if (id.isEmpty || !seenIds.add(id)) {
        throw const FormatException('Duplicate or empty versioned page ID');
      }
      await readInto(id);
    }
    if (page.nextCursor.isEmpty) break;
    if (!seenCursors.add(page.nextCursor)) {
      throw StateError('Versioned page cursor cycle');
    }
    cursor = page.nextCursor;
  }

  Map<String, BigInt> knownNow() {
    final values = Map<String, BigInt>.of(knownRevisions!());
    for (final entry in values.entries) {
      if (entry.key.isEmpty) {
        throw const FormatException('Empty known versioned content ID');
      }
      validateRevision(entry.value);
    }
    return values;
  }

  if (knownRevisions != null) {
    for (var attempt = 0; attempt < 3; attempt++) {
      final known = knownNow();
      for (final entry in known.entries) {
        final scanned = records[entry.key];
        if (scanned != null && scanned.revision >= entry.value) continue;
        await readInto(entry.key);
      }
      // The provider is synchronous. Nothing can interleave between this
      // comparison and returning the immutable snapshot.
      final latest = knownNow();
      if (latest.entries.every((entry) {
        final scanned = records[entry.key];
        return scanned != null && scanned.revision >= entry.value;
      })) {
        return List<VersionedContentRecord>.unmodifiable(records.values);
      }
    }
    throw StateError('Versioned content changed during page scan');
  }
  return List<VersionedContentRecord>.unmodifiable(records.values);
}
