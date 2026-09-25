part of 'workbench_native.dart';

extension _NativeEditorRecovery on RustWorkbench {
  Future<List<EditorRecovery>> _inspectEditorRecoveries({String? id}) =>
      _callDecoded(
        host.Action.inspectEditorRecoveries,
        configure: (request) {
          if (id != null) request.id = id;
        },
        decode: (reply) {
          final rows = reply.editorRecoveries;
          if (rows == null || rows.length > 16) {
            throw const FormatException('Invalid editor recovery list');
          }
          final seen = <String>{};
          return List<EditorRecovery>.unmodifiable([
            for (final row in rows)
              (() {
                final cardId = row.id ?? '';
                final operation = row.operation ?? '';
                final digest = row.digest;
                final source = row.sourceRevisionBigInt;
                if (cardId.isEmpty ||
                    !seen.add(cardId) ||
                    operation.isEmpty ||
                    (id != null && cardId != id) ||
                    digest == null ||
                    digest.length != 32 ||
                    source == BigInt.zero ||
                    row.status > 2) {
                  throw const FormatException(
                    'Invalid editor recovery identity',
                  );
                }
                return EditorRecovery(
                  id: cardId,
                  title: row.title ?? '',
                  operation: operation,
                  digest: digest,
                  sourceRevision: source,
                  currentRevision: row.currentRevisionBigInt,
                  status: EditorRecoveryStatus.values[row.status],
                );
              })(),
          ]);
        },
      );

  void _writeEditorRecovery(
    host.RequestBuilder request,
    EditorRecovery observed,
  ) {
    if (observed.id.isEmpty ||
        observed.operation.isEmpty ||
        observed.digest.length != 32 ||
        observed.sourceRevision == BigInt.zero) {
      throw const FormatException('Invalid editor recovery decision');
    }
    request.id = observed.id;
    request.operation = observed.operation;
    request.sha256 = Uint8List.fromList(observed.digest);
    request.revisionBigInt = observed.sourceRevision;
  }

  Future<VersionedMutationResult> _resumeEditorRecovery(
    EditorRecovery observed,
  ) async {
    final receipt = await _callDecoded(
      host.Action.resumeEditorRecovery,
      configure: (request) => _writeEditorRecovery(request, observed),
      decode: (reply) => VersionedContentCodec.decodeCommit(
        reply.payload ?? Uint8List(0),
        id: observed.id,
        operation: observed.operation,
        sourceRevision: observed.sourceRevision,
        outerRevision: reply.revisionBigInt,
      ),
    );
    _rememberRevision(observed.id, receipt.revision);
    try {
      final current = await versionedContent.read(observed.id);
      return VersionedMutationResult(receipt: receipt, current: current);
    } catch (error) {
      throw VersionedCommittedRefreshFailure(receipt, error);
    }
  }

  Future<void> _acknowledgeEditorRecovery(EditorRecovery observed) async {
    await _call(
      host.Action.acknowledgeEditorRecovery,
      configure: (request) => _writeEditorRecovery(request, observed),
    );
  }

  Future<void> _abandonEditorRecovery(EditorRecovery observed) async {
    await _call(
      host.Action.abandonEditorRecovery,
      configure: (request) => _writeEditorRecovery(request, observed),
    );
  }
}
