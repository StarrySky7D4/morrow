part of 'workbench_native.dart';

/// Private native adapter. Commit envelopes describe the original operation;
/// every successful mutation reads the card again before exposing current state.
/// The original operation committed, but its current card could not be read.
/// Retain this receipt to retry only the exact same operation.
final class VersionedCommittedRefreshFailure
    extends WorkbenchCommittedRefreshFailure {
  const VersionedCommittedRefreshFailure(this.receipt, Object cause)
    : super(cause);
  final VersionedCommitReceipt receipt;
}

final class NativeVersionedContent implements VersionedContentControl {
  NativeVersionedContent(this._owner);
  final RustWorkbench _owner;

  Future<VersionedContentRecord> _readCurrent(
    String id, {
    BigInt? minimum,
  }) async {
    if (id.isEmpty) throw const FormatException('Missing content ID');
    for (var attempt = 0; attempt < 3; attempt++) {
      final current = await _owner._callDecoded<VersionedContentRecord>(
        host.Action.readVersioned,
        configure: (request) => request.id = id,
        decode: (reply) => VersionedContentCodec.decodeRecord(
          reply.payload ?? Uint8List(0),
          id: id,
          outerRevision: VersionedContentCodec.unsigned(reply.revision),
        ),
      );
      final known = _owner._revisions[id];
      if (current.revision < (minimum ?? BigInt.zero) ||
          (known != null &&
              current.revision < VersionedContentCodec.unsigned(known))) {
        continue;
      }
      // No await between the last revision comparison and publication.
      _owner._rememberRevision(
        id,
        VersionedContentCodec.wireU64(current.revision),
      );
      _owner._knownDeleted[id] = current.deleted;
      return current;
    }
    throw StateError('Current versioned content changed during read');
  }

  @override
  Future<VersionedContentRecord> read(String id) => _readCurrent(id);

  @override
  Future<VersionedContentPage> page({
    String cursor = '',
    int limit = 128,
  }) async {
    if (limit < 1 || limit > 4096) {
      throw const FormatException('Invalid versioned page limit');
    }
    final response = await _owner._callDecoded<VersionedContentPage>(
      host.Action.pageVersioned,
      configure: (request) {
        request.cursor = cursor;
        request.limit = limit;
      },
      decode: (reply) {
        final ids = <String>[];
        final seen = <String>{};
        for (final id in reply.ids ?? <String?>[]) {
          if (id == null || id.isEmpty || !seen.add(id)) {
            throw const FormatException('Invalid versioned page ID');
          }
          ids.add(id);
        }
        if (ids.length > limit) {
          throw const FormatException('Versioned page exceeds limit');
        }
        return VersionedContentPage(ids: ids, nextCursor: reply.cursor ?? '');
      },
    );
    return response;
  }

  @override
  Future<TasksMigrationPlan> planMigration(String id) {
    if (id.isEmpty) throw const FormatException('Missing content ID');
    return _owner._callDecoded<TasksMigrationPlan>(
      host.Action.planTasksMigration,
      configure: (request) => request.id = id,
      decode: (reply) => VersionedContentCodec.decodePlan(
        reply.payload ?? Uint8List(0),
        id: id,
        outerRevision: VersionedContentCodec.unsigned(reply.revision),
      ),
    );
  }

  Future<VersionedMutationResult> _commit(
    host.Action action,
    String operation,
    String id,
    BigInt sourceRevision, {
    Uint8List? payload,
  }) async {
    if (operation.isEmpty || id.isEmpty || sourceRevision == BigInt.zero) {
      throw const FormatException('Missing versioned mutation identity');
    }
    final carrier = VersionedContentCodec.wireU64(sourceRevision);
    late final VersionedCommitReceipt receipt;
    try {
      receipt = await _owner._callDecoded<VersionedCommitReceipt>(
        action,
        configure: (request) {
          request.id = id;
          request.operation = operation;
          request.revision = carrier;
          if (payload != null) request.payload = payload;
        },
        decode: (reply) => VersionedContentCodec.decodeCommit(
          reply.payload ?? Uint8List(0),
          id: id,
          operation: operation,
          sourceRevision: sourceRevision,
          outerRevision: VersionedContentCodec.unsigned(reply.revision),
        ),
      );
    } on _HostResponseError catch (error) {
      // The host emits this marker only after its fresh lookup confirms
      // Absent for the card and operation in this serialized request.
      if ((action == host.Action.editTasks || action == host.Action.editCard) &&
          error.code == 1001 &&
          error.emptyPayload &&
          error.revision == 0) {
        throw VersionedMutationNoCommit(
          id: id,
          operation: operation,
          sourceRevision: sourceRevision,
        );
      }
      rethrow;
    }
    // Fence later reads at the confirmed revision before awaiting refresh.
    _owner._rememberRevision(
      id,
      VersionedContentCodec.wireU64(receipt.revision),
    );
    try {
      final current = await _readCurrent(id, minimum: receipt.revision);
      return VersionedMutationResult(receipt: receipt, current: current);
    } catch (error) {
      throw VersionedCommittedRefreshFailure(receipt, error);
    }
  }

  @override
  Future<VersionedMutationResult> migrate(TasksMigrationPlan plan) => _commit(
    host.Action.migrateTasks,
    plan.operation,
    plan.id,
    plan.sourceRevision,
  );

  @override
  Future<VersionedMutationResult> editTasks(
    String operation,
    String id,
    BigInt revision,
    TaskEditCommand command,
  ) => _commit(
    host.Action.editTasks,
    operation,
    id,
    revision,
    payload: VersionedContentCodec.encodeTaskEdit(command),
  );

  @override
  Future<VersionedMutationResult> editCard(
    String operation,
    String id,
    BigInt revision,
    CardEditCommand command,
  ) => _commit(
    host.Action.editCard,
    operation,
    id,
    revision,
    payload: VersionedContentCodec.encodeCardEdit(command),
  );

  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    required String operation,
  }) async {
    if (operation.isEmpty) {
      throw const FormatException('Missing query operation');
    }
    final payload = VersionedContentCodec.encodeQuery(
      section,
      filter,
      text,
      sort,
    );
    final ids = <String>[];
    final seenIds = <String>{};
    final seenCursors = <String>{};
    var cursor = '';
    do {
      final page = await _owner
          ._callDecoded<({List<String> ids, String cursor})>(
            host.Action.queryVersioned,
            configure: (request) {
              request.operation = operation;
              request.payload = payload;
              request.cursor = cursor;
              request.limit = 128;
            },
            decode: (reply) {
              final pageIds = <String>[];
              for (final id in reply.ids ?? <String?>[]) {
                if (id == null || id.isEmpty) {
                  throw const FormatException('Invalid query ID');
                }
                pageIds.add(id);
              }
              if (pageIds.length > 128) {
                throw const FormatException('Query page exceeds limit');
              }
              return (ids: pageIds, cursor: reply.cursor ?? '');
            },
          );
      for (final id in page.ids) {
        if (!seenIds.add(id) || ids.length >= 4096) {
          throw const FormatException('Repeated or oversized query result');
        }
        ids.add(id);
      }
      cursor = page.cursor;
      if (cursor.isNotEmpty && page.ids.isEmpty) {
        throw const FormatException('Empty nonterminal query page');
      }
      if (cursor.isNotEmpty && !seenCursors.add(cursor)) {
        throw const FormatException('Repeated query cursor');
      }
    } while (cursor.isNotEmpty);
    return List.unmodifiable(ids);
  }
}
