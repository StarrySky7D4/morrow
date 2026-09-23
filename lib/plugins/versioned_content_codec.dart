import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';

import 'generated/content_api.capnp.dart' as wire;
import 'generated/identity.dart' as contract;
import 'versioned_content.dart';

/// The private UI protocol carries unsigned u64 through signed Dart ints.
abstract final class VersionedContentCodec {
  static final BigInt maxU64 = (BigInt.one << 64) - BigInt.one;

  static BigInt unsigned(int carrier) => BigInt.from(carrier).toUnsigned(64);

  static int wireU64(BigInt value) {
    if (value < BigInt.zero || value > maxU64) {
      throw const FormatException('Content revision is outside u64');
    }
    return value.toSigned(64).toInt();
  }

  static bool _same(List<int>? a, List<int> b) {
    if (a == null || a.length != b.length) return false;
    for (var i = 0; i < b.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static void _header(int version, Uint8List? digest) {
    if (version != 1 || !_same(digest, contract.content_apiDigest)) {
      throw const FormatException('Versioned content protocol mismatch');
    }
  }

  static MessageReader _read(Uint8List bytes) {
    if (bytes.length < 8 || bytes.length > 128 * 1024) {
      throw const FormatException('Versioned content frame length');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('Versioned content segments');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) {
      throw const FormatException('Versioned content header');
    }
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) {
        throw const FormatException('Versioned content segment length');
      }
    }
    if (total != bytes.length) {
      throw const FormatException('Trailing versioned content bytes');
    }
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: 16384,
        nestingLimit: 16,
        maxSegments: 512,
      ),
    );
  }

  static wire.EnvelopeReader _envelope(
    Uint8List bytes,
    wire.EnvelopeKind kind,
  ) {
    final value = _read(bytes).getRoot(wire.envelopeFactory);
    _header(value.version, value.digest);
    if (value.kind != kind || value.id == null || value.id!.isEmpty) {
      throw const FormatException('Unexpected versioned content envelope');
    }
    return value;
  }

  static List<String> _texts(ListReader<String?>? values) {
    final result = <String>[];
    for (final value in values ?? <String?>[]) {
      if (value == null) throw const FormatException('Missing text list item');
      result.add(value);
    }
    return result;
  }

  static VersionedAsset _asset(wire.AssetReader value) {
    final id = value.id;
    final name = value.name;
    final kind = value.kind;
    if (id == null || id.isEmpty || name == null || kind == null) {
      throw const FormatException('Invalid content asset');
    }
    return VersionedAsset(
      id: id,
      name: name,
      kind: kind,
      bytes: unsigned(value.bytes),
    );
  }

  static VersionedContentRecord _card(wire.CardReader value) {
    final id = value.id;
    final title = value.title;
    final revision = unsigned(value.revision);
    if (id == null ||
        id.isEmpty ||
        title == null ||
        revision == BigInt.zero ||
        (value.formatVersion != 1 && value.formatVersion != 2)) {
      throw const FormatException('Unsupported or invalid content record');
    }
    final assets = <VersionedAsset>[
      for (final item in value.assets ?? <wire.AssetReader>[]) _asset(item),
    ];
    final tasks = <VersionedTask>[];
    final seen = <String>{};
    final counts = [0, 0, 0];
    for (final item in value.tasks ?? <wire.TaskReader>[]) {
      final taskId = item.id;
      final text = item.text;
      final state = item.completion;
      if (taskId == null ||
          taskId.isEmpty ||
          !seen.add(taskId) ||
          text == null ||
          text.isEmpty ||
          state >= 3) {
        throw const FormatException('Invalid versioned TaskId or completion');
      }
      counts[state]++;
      tasks.add(
        VersionedTask(
          id: taskId,
          text: text,
          completion: VersionedTaskCompletion.values[state],
          legacyCompleted: item.legacyCompleted,
          legacyDuplicates: item.legacyDuplicates,
        ),
      );
    }
    VersionedOrigin? origin;
    if (value.origin case final source?) {
      final sha = source.sourceSha256;
      final original = source.originalProperties;
      if (source.cardId != id ||
          source.sourceRevision == 0 ||
          source.migratorVersion != 1 ||
          source.targetVersion != 2 ||
          sha == null ||
          sha.length != 32 ||
          original == null ||
          !_same(sha, sha256.convert(original).bytes) ||
          source.originalTitle == null ||
          source.historicalProjectStage == null) {
        throw const FormatException('Invalid migration origin');
      }
      final mapping = <VersionedTaskMapping>[];
      var index = 0;
      for (final item in source.mapping ?? <wire.MappingReader>[]) {
        if (item.sourceIndex != index++ ||
            item.taskId == null ||
            item.taskId!.isEmpty) {
          throw const FormatException('Invalid migration mapping');
        }
        mapping.add(
          VersionedTaskMapping(
            sourceIndex: item.sourceIndex,
            taskId: item.taskId!,
          ),
        );
      }
      origin = VersionedOrigin(
        cardId: id,
        sourceRevision: unsigned(source.sourceRevision),
        sourceSha256: sha,
        migratorVersion: source.migratorVersion,
        targetVersion: source.targetVersion,
        originalProperties: original,
        mapping: mapping,
        historicalProjectStage: source.historicalProjectStage!,
        originalTitle: source.originalTitle!,
      );
    }
    final todos = _texts(value.todos);
    final completed = _texts(value.completed);
    final retired = _texts(value.retiredTaskIds);
    if (value.formatVersion == 1) {
      if (tasks.isNotEmpty ||
          origin != null ||
          retired.isNotEmpty ||
          value.ambiguousCount != 0 ||
          value.completeCount + value.incompleteCount != todos.length) {
        throw const FormatException('Invalid legacy record projection');
      }
    } else if (todos.isNotEmpty ||
        completed.isNotEmpty ||
        value.completeCount != counts[1] ||
        value.incompleteCount != counts[0] ||
        value.ambiguousCount != counts[2]) {
      throw const FormatException('Invalid task counts');
    }
    final description = value.description;
    final category = value.category;
    final stage = value.stage;
    final hypothesis = value.hypothesis;
    final conclusion = value.conclusion;
    final projectedStage = value.projectedStage;
    if (description == null ||
        category == null ||
        stage == null ||
        hypothesis == null ||
        conclusion == null ||
        projectedStage == null ||
        (value.deleted && value.deletedAt == 0)) {
      throw const FormatException('Incomplete versioned record');
    }
    return VersionedContentRecord(
      id: id,
      title: title,
      revision: revision,
      formatVersion: value.formatVersion,
      description: description,
      category: category,
      stage: stage,
      hypothesis: hypothesis,
      conclusion: conclusion,
      favorite: value.favorite,
      assets: assets,
      icon: value.icon,
      color: value.color,
      deleted: value.deleted,
      deletedAt: unsigned(value.deletedAt),
      todos: todos,
      completed: completed,
      tasks: tasks,
      origin: origin,
      retiredTaskIds: retired,
      projectedStage: projectedStage,
      completeCount: value.completeCount,
      incompleteCount: value.incompleteCount,
      ambiguousCount: value.ambiguousCount,
    );
  }

  static VersionedContentRecord decodeRecord(
    Uint8List bytes, {
    required String id,
    required BigInt outerRevision,
  }) {
    final value = _envelope(bytes, wire.EnvelopeKind.record);
    final record = value.record;
    final revision = unsigned(value.revision);
    if (value.id != id ||
        value.operation != '' ||
        record == null ||
        value.repeated ||
        unsigned(value.sourceRevision) != revision ||
        revision != outerRevision) {
      throw const FormatException('Versioned read identity mismatch');
    }
    final result = _card(record);
    if (result.id != id || result.revision != revision) {
      throw const FormatException('Versioned record revision mismatch');
    }
    return result;
  }

  static TasksMigrationPlan decodePlan(
    Uint8List bytes, {
    required String id,
    required BigInt outerRevision,
  }) {
    final value = _envelope(bytes, wire.EnvelopeKind.plan);
    final revision = unsigned(value.sourceRevision);
    if (value.id != id ||
        value.operation == null ||
        value.operation!.isEmpty ||
        value.record != null ||
        value.repeated ||
        unsigned(value.revision) != revision ||
        revision != outerRevision ||
        revision == BigInt.zero) {
      throw const FormatException('Migration plan identity mismatch');
    }
    return TasksMigrationPlan(
      id: id,
      operation: value.operation!,
      sourceRevision: revision,
    );
  }

  static VersionedCommitReceipt decodeCommit(
    Uint8List bytes, {
    required String id,
    required String operation,
    required BigInt sourceRevision,
    required BigInt outerRevision,
  }) {
    final value = _envelope(bytes, wire.EnvelopeKind.commit);
    final revision = unsigned(value.revision);
    if (value.id != id ||
        value.operation != operation ||
        unsigned(value.sourceRevision) != sourceRevision ||
        revision != outerRevision ||
        revision <= sourceRevision ||
        value.record == null) {
      throw const FormatException('Commit receipt identity mismatch');
    }
    final historical = _card(value.record!);
    if (historical.id != id || historical.revision != revision) {
      throw const FormatException('Historical commit record mismatch');
    }
    return VersionedCommitReceipt(
      id: id,
      operation: operation,
      revision: revision,
      repeated: value.repeated,
    );
  }

  static Uint8List encodeTaskEdit(TaskEditCommand command) {
    final builder = MessageBuilder();
    final out = builder.initRoot(wire.taskEditFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.content_apiDigest);
    out.action = wire.TaskAction.values[command.kind.index];
    switch (command.kind) {
      case TaskEditKind.setCompletion:
        out.taskId = command.taskId;
        out.complete = command.complete;
      case TaskEditKind.rename:
        out.taskId = command.taskId;
        out.text = command.text;
      case TaskEditKind.reorder:
        final order = out.initOrder(command.order.length);
        for (var i = 0; i < command.order.length; i++) {
          order[i] = command.order[i];
        }
      case TaskEditKind.setStage:
      case TaskEditKind.completeAllAndSetStage:
        out.text = command.text;
      case TaskEditKind.add:
        out.taskId = command.taskId;
        out.text = command.text;
      case TaskEditKind.remove:
        out.taskId = command.taskId;
    }
    final bytes = builder.serialize();
    if (bytes.length > 128 * 1024) {
      throw const FormatException('Task edit budget');
    }
    return bytes;
  }

  static Uint8List encodeCardEdit(CardEditCommand command) {
    final builder = MessageBuilder();
    final out = builder.initRoot(wire.cardEditFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.content_apiDigest);
    out.action = wire.CardAction.values[command.kind.index];
    switch (command.kind) {
      case CardEditKind.edit:
        final fields =
            command.fields ??
            (throw const FormatException('Missing card fields'));
        final dst = out.initFields();
        if (fields.icon < 0 ||
            fields.icon > 0xffff ||
            fields.color < 0 ||
            fields.color > 0xffffffff) {
          throw const FormatException('Card icon or color is out of range');
        }
        dst.title = fields.title;
        dst.description = fields.description;
        dst.hypothesis = fields.hypothesis;
        dst.conclusion = fields.conclusion;
        dst.icon = fields.icon;
        dst.color = fields.color;
        final assets = dst.initAssets(fields.assets.length);
        for (var i = 0; i < fields.assets.length; i++) {
          final source = fields.assets[i];
          final asset = assets[i];
          asset.id = source.id;
          asset.name = source.name;
          asset.kind = source.kind;
          asset.bytes = wireU64(source.bytes);
        }
      case CardEditKind.setFavorite:
        out.favorite = command.favorite;
      case CardEditKind.setCategory:
        out.category = command.category;
        out.stage = command.stage;
      case CardEditKind.delete:
      case CardEditKind.restore:
        break;
    }
    final bytes = builder.serialize();
    if (bytes.length > 128 * 1024) {
      throw const FormatException('Card edit budget');
    }
    return bytes;
  }

  static Uint8List encodeQuery(
    String section,
    String filter,
    String text,
    String sort,
  ) {
    final builder = MessageBuilder();
    final out = builder.initRoot(wire.queryFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.content_apiDigest);
    out.section = section;
    out.filter = filter;
    out.text = text;
    out.sort = sort;
    final bytes = builder.serialize();
    if (bytes.length > 128 * 1024) throw const FormatException('Query budget');
    return bytes;
  }
}
