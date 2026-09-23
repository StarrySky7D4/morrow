part of 'workbench_native.dart';

/// Shares the draft lane; a missing mutation reply never triggers a retry.
final class NativeEditorDraftHandoffControl
    implements EditorDraftHandoffControl {
  NativeEditorDraftHandoffControl(this._owner);
  final RustWorkbench _owner;
  NativeEditorDraftControl get _draft =>
      _owner.editorDrafts as NativeEditorDraftControl;

  @override
  Future<EditorDraftRecord> handoff(EditorDraftHandoffRequest proposal) {
    final Uint8List body;
    try {
      body = EditorDraftCodec.encodeHandoff(proposal);
    } catch (error) {
      return Future.error(
        EditorDraftHandoffFailure(
          request: proposal,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _draft._job(() async {
      final request = proposal.request;
      var token = '';
      var outcomeUnknown = false;
      try {
        final begin = await _owner._call(
          host.Action.beginEditorDraft,
          configure: (outer) {
            outer.operation = request.operation;
            outer.totalLength = body.length;
            outer.sha256 = Uint8List.fromList(sha256.convert(body).bytes);
          },
        );
        token = begin.transfer ?? '';
        if (token.isEmpty || begin.offset != 0) {
          throw const FormatException('Invalid draft handoff upload admission');
        }
        for (var offset = 0; offset < body.length;) {
          final end = (offset + NativeEditorDraftControl._chunkBytes).clamp(
            0,
            body.length,
          );
          final reply = await _owner._call(
            host.Action.appendEditorDraft,
            configure: (outer) {
              outer.transfer = token;
              outer.offset = offset;
              outer.payload = Uint8List.sublistView(body, offset, end);
            },
          );
          if (reply.transfer != token ||
              VersionedContentCodec.unsigned(reply.offset) !=
                  BigInt.from(end)) {
            throw const FormatException(
              'Draft handoff upload receipt mismatch',
            );
          }
          offset = end;
        }
        outcomeUnknown = true;
        final reply = await _owner._call(
          host.Action.finishEditorDraftHandoff,
          configure: (outer) {
            outer.transfer = token;
            outer.operation = request.operation;
            _draft._identity(
              outer,
              request.cardId,
              request.draftId,
              BigInt.zero,
            );
          },
        );
        final (envelope, revision) = await _draft._download(reply);
        _draft._context(
          envelope,
          request.cardId,
          request.draftId,
          request.operation,
          BigInt.zero,
        );
        final record = _draft._record(envelope, revision);
        final link = record.parentLink;
        if (!record.active ||
            record.generation != BigInt.one ||
            record.retirement != null ||
            link == null ||
            !RustWorkbench._same(
              EditorDraftCodec.encodeHandoff(
                EditorDraftHandoffRequest(
                  request: record.request,
                  parentLink: link,
                ),
              ),
              body,
            )) {
          throw const FormatException('Draft handoff receipt changed proposal');
        }
        return record;
      } catch (error) {
        await _draft._abort(token);
        throw EditorDraftHandoffFailure(
          request: proposal,
          outcomeUnknown: outcomeUnknown,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftRecord> retireParent(
    EditorDraftRetirementRequest proposal,
  ) {
    try {
      EditorDraftCodec.validateRetirement(proposal);
    } catch (error) {
      return Future.error(
        EditorDraftRetirementFailure(
          request: proposal,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _draft._job(() async {
      // After dispatch a readback failure cannot prove no commit.
      try {
        final (envelope, revision) = await _draft._requestEnvelope(
          host.Action.retireEditorDraftParent,
          configure: (outer) {
            _draft._identity(
              outer,
              proposal.cardId,
              proposal.childDraftId,
              proposal.parentGeneration,
            );
            outer.name = proposal.parentDraftId;
            outer.kind = proposal.childOperation;
            outer.operation = proposal.operation;
          },
        );
        _draft._context(
          envelope,
          proposal.cardId,
          proposal.parentDraftId,
          proposal.operation,
          proposal.parentGeneration,
        );
        final record = _draft._record(envelope, revision);
        final marker = record.retirement;
        if (record.active ||
            record.currentActive ||
            record.generation != proposal.parentGeneration + BigInt.one ||
            record.currentGeneration != record.generation ||
            marker == null ||
            marker.childDraftId != proposal.childDraftId ||
            marker.childOperation != proposal.childOperation ||
            marker.operation != proposal.operation ||
            marker.parentGeneration != proposal.parentGeneration) {
          throw const FormatException(
            'Draft parent retirement receipt mismatch',
          );
        }
        return record;
      } catch (error) {
        throw EditorDraftRetirementFailure(
          request: proposal,
          outcomeUnknown: true,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftLineagePage> listLineages({
    String cursor = '',
    int limit = 32,
  }) => _draft._job(() => _list(cursor, limit));

  Future<EditorDraftLineagePage> _list(String cursor, int limit) async {
    EditorDraftCodec.validateLineagePageRequest(cursor: cursor, limit: limit);
    final (envelope, revision) = await _draft._requestEnvelope(
      host.Action.listEditorDraftLineages,
      configure: (outer) {
        outer.cursor = cursor;
        outer.limit = limit;
      },
    );
    _draft._context(envelope, '', '', '', BigInt.zero);
    final page = envelope.lineagePage;
    if (revision != BigInt.zero ||
        envelope.kind != EditorDraftResultKind.lineages ||
        page == null ||
        page.requestCursor != cursor ||
        page.requestLimit != limit) {
      throw const FormatException('Draft lineage page context mismatch');
    }
    return page;
  }

  @override
  Future<List<EditorDraftLineage>> discoverLineages() => _draft._job(() async {
    final results = <EditorDraftLineage>[];
    final children = <(String, String)>{};
    final parents = <(String, String)>{};
    var cursor = '';
    while (true) {
      final page = await _list(cursor, 32);
      for (final item in page.lineages) {
        if (item.cursor.compareTo(cursor) <= 0 ||
            !children.add((item.cardId, item.childDraftId)) ||
            !parents.add((item.cardId, item.parentLink.parentDraftId)) ||
            results.length >= 256) {
          throw const FormatException(
            'Draft lineage discovery changed or exceeded limit',
          );
        }
        cursor = item.cursor;
        results.add(item);
      }
      if (page.nextCursor.isEmpty) return List.unmodifiable(results);
      if (page.lineages.isEmpty ||
          page.nextCursor != cursor ||
          results.length >= 256) {
        throw const FormatException('Draft lineage discovery did not advance');
      }
    }
  });
}
