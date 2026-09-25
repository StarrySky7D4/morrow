import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:morrow_studio/plugins/generated/content_api.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_content_codec.dart';

Uint8List recordFrame(
  BigInt revision, {
  int formatVersion = 1,
  bool badDigest = false,
  String envelopeId = 'card-1',
  int? completeCount,
  int ambiguousCompletion = 2,
  BigInt? assetBytes,
}) {
  final message = MessageBuilder();
  final envelope = message.initRoot(wire.envelopeFactory);
  envelope.version = 1;
  envelope.digest = Uint8List.fromList(
    badDigest ? List<int>.filled(32, 0) : contract.content_apiDigest,
  );
  envelope.kind = wire.EnvelopeKind.record;
  envelope.id = envelopeId;
  envelope.operation = '';
  envelope.sourceRevisionBigInt = revision;
  envelope.revisionBigInt = revision;
  final card = envelope.initRecord();
  card.id = 'card-1';
  card.title = 'title';
  card.revisionBigInt = revision;
  card.formatVersion = formatVersion;
  card.description = '';
  card.category = '灵感';
  card.stage = '待整理';
  card.hypothesis = '';
  card.conclusion = '';
  card.projectedStage = '待整理';
  if (assetBytes != null) {
    final asset = card.initAssets(1)[0];
    asset.id = 'asset-1';
    asset.name = 'asset.bin';
    asset.kind = 'file';
    asset.bytesBigInt = assetBytes;
  }
  card.completeCount = completeCount ?? (formatVersion == 2 ? 0 : 0);
  card.incompleteCount = formatVersion == 2 ? 1 : 0;
  card.ambiguousCount = formatVersion == 2 ? 1 : 0;
  if (formatVersion == 2) {
    final tasks = card.initTasks(2);
    final first = tasks[0];
    first.id = 'task-1';
    first.text = 'open';
    first.completion = 0;
    final second = tasks[1];
    second.id = 'task-2';
    second.text = 'duplicate';
    second.completion = ambiguousCompletion;
    second.legacyCompleted = true;
    second.legacyDuplicates = 2;
    final bytes = Uint8List.fromList([0x08, 0x01, 0xa2, 0x06, 0x01, 0xff]);
    final origin = card.initOrigin();
    origin.cardId = 'card-1';
    origin.sourceRevisionBigInt = revision - BigInt.one;
    origin.sourceSha256 = Uint8List.fromList(sha256.convert(bytes).bytes);
    origin.migratorVersion = 1;
    origin.targetVersion = 2;
    origin.originalProperties = bytes;
    origin.historicalProjectStage = '待整理';
    origin.originalTitle = 'original';
  }
  return message.serialize();
}

Uint8List planFrame(BigInt revision, {String operation = 'migrate-card-1'}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.content_apiDigest);
  out.kind = wire.EnvelopeKind.plan;
  out.id = 'card-1';
  out.operation = operation;
  out.sourceRevisionBigInt = revision;
  out.revisionBigInt = revision;
  return message.serialize();
}

Uint8List commitFrame(
  BigInt sourceRevision, {
  String operation = 'edit-card-1',
  BigInt? reportedSource,
}) {
  final revision = sourceRevision + BigInt.one;
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.content_apiDigest);
  out.kind = wire.EnvelopeKind.commit;
  out.id = 'card-1';
  out.operation = operation;
  out.sourceRevisionBigInt = reportedSource ?? sourceRevision;
  out.revisionBigInt = revision;
  final card = out.initRecord();
  card.id = 'card-1';
  card.title = 'title';
  card.revisionBigInt = revision;
  card.formatVersion = 1;
  card.description = '';
  card.category = '灵感';
  card.stage = '待整理';
  card.hypothesis = '';
  card.conclusion = '';
  card.projectedStage = '待整理';
  return message.serialize();
}

void main() {
  test('exact u64 fields preserve the top bit and adjacent revisions', () {
    final top = BigInt.one << 63;
    final max = VersionedContentCodec.maxU64;
    final precise = BigInt.one << 53;
    for (final value in [
      precise - BigInt.one,
      precise,
      precise + BigInt.one,
      top - BigInt.one,
      top,
      top + BigInt.one,
      max - BigInt.one,
      max,
    ]) {
      if (kIsWeb && value.toSigned(64).abs() > BigInt.from(9007199254740991)) {
        expect(
          () => VersionedContentCodec.wireU64(value),
          throwsFormatException,
        );
      } else {
        expect(
          VersionedContentCodec.unsigned(VersionedContentCodec.wireU64(value)),
          value,
        );
      }
      final record = VersionedContentCodec.decodeRecord(
        recordFrame(value),
        id: 'card-1',
        outerRevision: value,
      );
      expect(record.revision, value);
      expect(record.formatVersion, 1);
    }
    final withAsset = VersionedContentCodec.decodeRecord(
      recordFrame(max - BigInt.one, assetBytes: max),
      id: 'card-1',
      outerRevision: max - BigInt.one,
    );
    expect(withAsset.assets.single.bytes, max);
    expect(
      () => VersionedContentCodec.wireU64(-BigInt.one),
      throwsFormatException,
    );
    expect(
      () => VersionedContentCodec.wireU64(max + BigInt.one),
      throwsFormatException,
    );
  });

  test('V2 keeps ambiguity, independent stage and migration provenance', () {
    final revision = (BigInt.one << 63) + BigInt.one;
    final record = VersionedContentCodec.decodeRecord(
      recordFrame(revision, formatVersion: 2),
      id: 'card-1',
      outerRevision: revision,
    );
    expect(record.projectedStage, '待整理');
    expect(record.tasks.map((task) => task.completion), [
      VersionedTaskCompletion.incomplete,
      VersionedTaskCompletion.legacyAmbiguous,
    ]);
    expect(
      [record.completeCount, record.incompleteCount, record.ambiguousCount],
      [0, 1, 1],
    );
    expect(record.origin!.sourceRevision, revision - BigInt.one);
    expect(record.origin!.originalProperties.last, 0xff);
    expect(
      () => record.origin!.originalProperties[0] = 0,
      throwsUnsupportedError,
    );
  });

  test('wrong digest, identity, revision, format and counts are rejected', () {
    final revision = BigInt.from(42);
    for (final bytes in [
      recordFrame(revision, badDigest: true),
      recordFrame(revision, envelopeId: 'other'),
      recordFrame(revision, formatVersion: 3),
      recordFrame(revision, formatVersion: 2, completeCount: 1),
    ]) {
      expect(
        () => VersionedContentCodec.decodeRecord(
          bytes,
          id: 'card-1',
          outerRevision: revision,
        ),
        throwsFormatException,
      );
    }
    expect(
      () => VersionedContentCodec.decodeRecord(
        recordFrame(revision),
        id: 'card-1',
        outerRevision: revision + BigInt.one,
      ),
      throwsFormatException,
    );
  });

  test(
    'plan and commit bind identity, operation, and exact unsigned source',
    () {
      final source = (BigInt.one << 63) + BigInt.from(17);
      final plan = VersionedContentCodec.decodePlan(
        planFrame(source),
        id: 'card-1',
        outerRevision: source,
      );
      expect(plan.sourceRevision, source);
      expect(plan.operation, 'migrate-card-1');
      expect(
        () => VersionedContentCodec.decodePlan(
          planFrame(source),
          id: 'other',
          outerRevision: source,
        ),
        throwsFormatException,
      );
      final receipt = VersionedContentCodec.decodeCommit(
        commitFrame(source),
        id: 'card-1',
        operation: 'edit-card-1',
        sourceRevision: source,
        outerRevision: source + BigInt.one,
      );
      expect(receipt.revision, source + BigInt.one);
      expect(
        () => VersionedContentCodec.decodeCommit(
          commitFrame(source, operation: 'different'),
          id: 'card-1',
          operation: 'edit-card-1',
          sourceRevision: source,
          outerRevision: source + BigInt.one,
        ),
        throwsFormatException,
      );
      expect(
        () => VersionedContentCodec.decodeCommit(
          commitFrame(source, reportedSource: source - BigInt.one),
          id: 'card-1',
          operation: 'edit-card-1',
          sourceRevision: source,
          outerRevision: source + BigInt.one,
        ),
        throwsFormatException,
      );
    },
  );

  test('unknown completion and trailing frame bytes are rejected', () {
    final revision = BigInt.from(42);
    expect(
      () => VersionedContentCodec.decodeRecord(
        recordFrame(revision, formatVersion: 2, ambiguousCompletion: 3),
        id: 'card-1',
        outerRevision: revision,
      ),
      throwsFormatException,
    );
    final trailing = Uint8List.fromList([...recordFrame(revision), 0]);
    expect(
      () => VersionedContentCodec.decodeRecord(
        trailing,
        id: 'card-1',
        outerRevision: revision,
      ),
      throwsFormatException,
    );
  });

  test('card edit rejects icon and color truncation before serialization', () {
    CardEditCommand edit(int icon, int color) => CardEditCommand.edit(
      CardEditFields(
        title: 'title',
        description: '',
        hypothesis: '',
        conclusion: '',
        icon: icon,
        color: color,
        assets: const [],
      ),
    );
    for (final value in [
      edit(-1, 0),
      edit(0x10000, 0),
      edit(0, -1),
      edit(0, 0x100000000),
    ]) {
      expect(
        () => VersionedContentCodec.encodeCardEdit(value),
        throwsFormatException,
      );
    }
    expect(
      VersionedContentCodec.encodeCardEdit(edit(0xffff, 0xffffffff)),
      isNotEmpty,
    );
  });

  test('task command encodes only canonical action fields', () {
    final command = TaskEditCommand.add('new-task', 'text');
    final bytes = VersionedContentCodec.encodeTaskEdit(command);
    final reader = MessageReader.deserialize(
      bytes,
    ).getRoot(wire.taskEditFactory);
    expect(reader.version, 1);
    expect(reader.digest, contract.content_apiDigest);
    expect(reader.action, wire.TaskAction.add);
    expect(reader.taskId, 'new-task');
    expect(reader.text, 'text');
    expect(reader.order, isNull);
  });
}
