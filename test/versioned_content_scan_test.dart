import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_content_scan.dart';

final class _Source implements VersionedContentControl {
  _Source({required this.onPage, required this.onRead});

  final Future<VersionedContentPage> Function(String cursor, int limit) onPage;
  final Future<VersionedContentRecord> Function(String id) onRead;
  final calls = <String>[];

  @override
  Future<VersionedContentPage> page({String cursor = '', int limit = 128}) {
    calls.add('page:$cursor:$limit');
    return onPage(cursor, limit);
  }

  @override
  Future<VersionedContentRecord> read(String id) {
    calls.add('read:$id');
    return onRead(id);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Scanner attempted a mutation or query: $invocation');
}

VersionedContentRecord _record(
  String id,
  BigInt revision, {
  int format = 1,
  bool deleted = false,
}) => VersionedContentRecord(
  id: id,
  title: id,
  revision: revision,
  formatVersion: format,
  description: '',
  category: '灵感',
  stage: '待整理',
  hypothesis: '',
  conclusion: '',
  favorite: false,
  assets: const [],
  icon: 0,
  color: 0,
  deleted: deleted,
  deletedAt: deleted ? BigInt.one : BigInt.zero,
  todos: const [],
  completed: const [],
  tasks: const [],
  origin: null,
  retiredTaskIds: const [],
  projectedStage: '待整理',
  completeCount: 0,
  incompleteCount: 0,
  ambiguousCount: 0,
);

void main() {
  test(
    'retains mixed formats, tombstones, and full unsigned u64 revisions',
    () async {
      final large = (BigInt.one << 53) + BigInt.one;
      final maximum = (BigInt.one << 64) - BigInt.one;
      final source = _Source(
        onPage: (cursor, limit) async => switch (cursor) {
          '' => VersionedContentPage(ids: ['legacy'], nextCursor: 'first'),
          'first' => VersionedContentPage(ids: ['migrated'], nextCursor: ''),
          _ => throw StateError('unexpected cursor'),
        },
        onRead: (id) async => id == 'legacy'
            ? _record(id, large)
            : _record(id, maximum, format: 2, deleted: true),
      );
      final records = await scanVersionedContent(source, pageLimit: 2);
      expect(records.map((record) => record.id), ['legacy', 'migrated']);
      expect(records.map((record) => record.formatVersion), [1, 2]);
      expect(records.map((record) => record.revision), [large, maximum]);
      expect(records.last.deleted, isTrue);
      expect(
        () => records.add(_record('extra', BigInt.one)),
        throwsUnsupportedError,
      );
      expect(source.calls, [
        'page::2',
        'read:legacy',
        'page:first:2',
        'read:migrated',
      ]);
    },
  );

  test(
    'synchronous known revisions catch new and updated IDs after async read',
    () async {
      final entered = Completer<void>();
      final release = Completer<void>();
      final known = <String, BigInt>{};
      final large = (BigInt.one << 53) + BigInt.from(7);
      final source = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async {
          if (id == 'a' && !entered.isCompleted) {
            entered.complete();
            await release.future;
            return _record(id, BigInt.one);
          }
          return _record(
            id,
            id == 'a' ? large : (BigInt.one << 64) - BigInt.one,
          );
        },
      );
      final scanning = scanVersionedContent(
        source,
        knownRevisions: () => known,
      );
      await entered.future;
      known['a'] = large;
      known['new'] = (BigInt.one << 64) - BigInt.one;
      release.complete();
      final records = await scanning;
      expect(records.map((record) => record.id), ['a', 'new']);
      expect(records.first.revision, large);
      expect(records.last.revision, known['new']);
      expect(source.calls.where((call) => call == 'read:a').length, 2);
      expect(source.calls.where((call) => call == 'read:new').length, 1);
    },
  );

  test(
    'rechecks knowledge without await and rejects continuous updates',
    () async {
      final known = <String, BigInt>{'a': BigInt.from(2)};
      var reads = 0;
      final source = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async {
          reads++;
          if (reads == 1) return _record(id, BigInt.one);
          final revision = known[id]!;
          known[id] = revision + BigInt.one;
          return _record(id, revision);
        },
      );
      await expectLater(
        scanVersionedContent(source, knownRevisions: () => known),
        throwsStateError,
      );
      expect(
        reads,
        4,
      ); // Initial page read and exactly three convergence reads.
    },
  );

  test(
    'rejects duplicate IDs, cursor loops, and mismatched read IDs',
    () async {
      final duplicate = _Source(
        onPage: (cursor, limit) async => cursor.isEmpty
            ? VersionedContentPage(ids: ['a'], nextCursor: 'next')
            : VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async => _record(id, BigInt.one),
      );
      await expectLater(scanVersionedContent(duplicate), throwsFormatException);
      final cycle = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: const [], nextCursor: 'same'),
        onRead: (id) async => _record(id, BigInt.one),
      );
      await expectLater(scanVersionedContent(cycle), throwsStateError);
      final wrongId = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async => _record('b', BigInt.one),
      );
      await expectLater(scanVersionedContent(wrongId), throwsFormatException);
    },
  );

  test(
    'rejects unsupported format, rollback, and invalid u64 knowledge',
    () async {
      final unsupported = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async => _record(id, BigInt.one, format: 3),
      );
      await expectLater(
        scanVersionedContent(unsupported),
        throwsFormatException,
      );

      var reads = 0;
      final rollback = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async {
          reads++;
          return _record(id, reads == 1 ? BigInt.from(5) : BigInt.from(4));
        },
      );
      await expectLater(
        scanVersionedContent(
          rollback,
          knownRevisions: () => {'a': BigInt.from(6)},
        ),
        throwsStateError,
      );
      expect(reads, 2);

      final regular = _Source(
        onPage: (cursor, limit) async =>
            VersionedContentPage(ids: ['a'], nextCursor: ''),
        onRead: (id) async => _record(id, BigInt.one),
      );
      await expectLater(
        scanVersionedContent(
          regular,
          knownRevisions: () => {'a': BigInt.one << 64},
        ),
        throwsFormatException,
      );
      await expectLater(
        scanVersionedContent(regular, pageLimit: 0),
        throwsFormatException,
      );
    },
  );
}
