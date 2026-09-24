part of 'workbench_native.dart';

/// Persistent handoff intents share the complete editor-draft transfer lane.
/// Discovery and inspection never replay a business mutation.
final class NativeEditorDraftHandoffProposalControl
    implements EditorDraftHandoffProposalControl {
  NativeEditorDraftHandoffProposalControl(this._owner);
  final RustWorkbench _owner;
  NativeEditorDraftControl get _draft =>
      _owner.editorDrafts as NativeEditorDraftControl;

  EditorDraftHandoffProposalFailure _failure(
    String card,
    String parent,
    String operation,
    bool unknown,
    Object cause,
  ) => EditorDraftHandoffProposalFailure(
    cardId: card,
    parentDraftId: parent,
    childOperation: operation,
    outcomeUnknown: unknown,
    cause: cause,
  );

  EditorDraftHandoffProposalRecord _record(
    EditorDraftEnvelope envelope,
    BigInt revision,
    String card,
    String parent,
    String operation,
  ) {
    _draft._context(envelope, card, parent, operation, BigInt.zero);
    final record = envelope.handoffProposal;
    if (envelope.kind != EditorDraftResultKind.handoffProposal ||
        record == null ||
        record.summary.revision != revision ||
        record.summary.cardId != card ||
        record.summary.parentDraftId != parent ||
        record.summary.childOperation != operation) {
      throw const FormatException('Draft handoff proposal receipt mismatch');
    }
    return record;
  }

  @override
  Future<EditorDraftHandoffProposalRecord> prepare(
    EditorDraftHandoffProposal proposal,
  ) {
    final request = proposal.handoff.request;
    final parent = proposal.handoff.parentLink.parentDraftId;
    final Uint8List body;
    try {
      // Freeze before admission: later work must not alter an in-flight intent.
      body = EditorDraftCodec.encodeHandoffProposal(proposal);
    } catch (error) {
      return Future.error(
        _failure(request.cardId, parent, request.operation, false, error),
      );
    }
    return _draft._job(() async {
      var token = '';
      var unknown = false;
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
          throw const FormatException(
            'Draft proposal upload admission mismatch',
          );
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
              EditorDraftCodec.unsigned(reply.offset) != BigInt.from(end)) {
            throw const FormatException(
              'Draft proposal upload receipt mismatch',
            );
          }
          offset = end;
        }
        // From dispatch onward, missing/invalid replies do not prove no commit.
        unknown = true;
        final reply = await _owner._call(
          host.Action.prepareEditorDraftHandoffProposal,
          configure: (outer) {
            _draft._identity(outer, request.cardId, parent, BigInt.zero);
            outer.operation = request.operation;
            outer.transfer = token;
          },
        );
        final (envelope, revision) = await _draft._download(reply);
        final record = _record(
          envelope,
          revision,
          request.cardId,
          parent,
          request.operation,
        );
        if (!RustWorkbench._same(
          body,
          EditorDraftCodec.encodeHandoffProposal(record.proposal),
        )) {
          throw const FormatException('Draft proposal frozen request changed');
        }
        return record;
      } catch (error) {
        throw _failure(
          request.cardId,
          parent,
          request.operation,
          unknown,
          error,
        );
      } finally {
        // Frees only this upload/reply. Never cancels the persisted proposal.
        await _draft._abort(token);
      }
    });
  }

  Future<(EditorDraftEnvelope, BigInt)> _send(
    host.Action action,
    String card,
    String parent,
    String operation,
  ) => _draft._requestEnvelope(
    action,
    configure: (outer) {
      _draft._identity(outer, card, parent, BigInt.zero);
      outer.operation = operation;
    },
  );

  @override
  Future<EditorDraftHandoffProposalRecord?> inspect({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) {
    EditorDraftCodec.validateHandoffProposalIdentity(
      cardId,
      parentDraftId,
      childOperation,
    );
    return _draft._job(() async {
      final (envelope, revision) = await _send(
        host.Action.inspectEditorDraftHandoffProposal,
        cardId,
        parentDraftId,
        childOperation,
      );
      _draft._context(
        envelope,
        cardId,
        parentDraftId,
        childOperation,
        BigInt.zero,
      );
      if (envelope.kind == EditorDraftResultKind.absent &&
          revision == BigInt.zero) {
        return null;
      }
      return _record(envelope, revision, cardId, parentDraftId, childOperation);
    });
  }

  Future<EditorDraftHandoffProposalRecord> _mutate(
    host.Action action,
    String card,
    String parent,
    String operation,
    Set<EditorDraftHandoffProposalStatus> statuses,
  ) {
    try {
      EditorDraftCodec.validateHandoffProposalIdentity(card, parent, operation);
    } catch (error) {
      return Future.error(_failure(card, parent, operation, false, error));
    }
    return _draft._job(() async {
      try {
        final (envelope, revision) = await _send(
          action,
          card,
          parent,
          operation,
        );
        final record = _record(envelope, revision, card, parent, operation);
        if (!statuses.contains(record.summary.status)) {
          throw const FormatException(
            'Draft proposal mutation status mismatch',
          );
        }
        return record;
      } catch (error) {
        throw _failure(card, parent, operation, true, error);
      }
    });
  }

  @override
  Future<EditorDraftHandoffProposalRecord> complete({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _mutate(
    host.Action.completeEditorDraftHandoffProposal,
    cardId,
    parentDraftId,
    childOperation,
    {
      EditorDraftHandoffProposalStatus.childCommitted,
      EditorDraftHandoffProposalStatus.parentRetired,
    },
  );
  @override
  Future<EditorDraftHandoffProposalRecord> retire({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _mutate(
    host.Action.retireEditorDraftHandoffProposal,
    cardId,
    parentDraftId,
    childOperation,
    {EditorDraftHandoffProposalStatus.parentRetired},
  );
  @override
  Future<EditorDraftHandoffProposalRecord> cancel({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _mutate(
    host.Action.cancelEditorDraftHandoffProposal,
    cardId,
    parentDraftId,
    childOperation,
    {EditorDraftHandoffProposalStatus.cancelled},
  );

  Future<EditorDraftHandoffProposalPage> _page(String cursor, int limit) async {
    EditorDraftCodec.validateHandoffProposalPageRequest(
      cursor: cursor,
      limit: limit,
    );
    final (envelope, revision) = await _draft._requestEnvelope(
      host.Action.listEditorDraftHandoffProposals,
      configure: (outer) {
        outer.cursor = cursor;
        outer.limit = limit;
      },
    );
    _draft._context(envelope, '', '', '', BigInt.zero);
    final page = envelope.handoffProposalPage;
    if (envelope.kind != EditorDraftResultKind.handoffProposals ||
        revision != BigInt.zero ||
        page == null ||
        page.requestCursor != cursor ||
        page.requestLimit != limit) {
      throw const FormatException('Draft proposal page context mismatch');
    }
    return page;
  }

  @override
  Future<EditorDraftHandoffProposalPage> page({
    String cursor = '',
    int limit = 32,
  }) => _draft._job(() => _page(cursor, limit));

  @override
  Future<List<EditorDraftHandoffProposalSummary>> discover({
    int pageSize = 32,
  }) {
    EditorDraftCodec.validateHandoffProposalPageRequest(
      cursor: '',
      limit: pageSize,
    );
    // One lane admission for the entire scan; draft writes cannot interleave.
    return _draft._job(() async {
      final results = <EditorDraftHandoffProposalSummary>[];
      final operations = <String>{};
      final children = <(String, String)>{};
      var cursor = '';
      while (true) {
        final page = await _page(cursor, pageSize);
        for (final item in page.proposals) {
          if (item.cursor.compareTo(cursor) <= 0 ||
              !operations.add(item.childOperation) ||
              !children.add((item.cardId, item.childDraftId)) ||
              results.length >= 256) {
            throw const FormatException(
              'Draft proposal discovery changed or exceeded limit',
            );
          }
          cursor = item.cursor;
          results.add(item);
        }
        if (page.nextCursor.isEmpty) return List.unmodifiable(results);
        if (page.proposals.isEmpty ||
            page.nextCursor != cursor ||
            results.length >= 256) {
          throw const FormatException(
            'Draft proposal discovery did not advance',
          );
        }
      }
    });
  }
}
