part of 'workbench_native.dart';

/// Durable import calls share the entire draft transfer lane with the older
/// draft controller. A single RPC queue is insufficient for chunked replies.
final class NativeEditorDraftImportControl implements EditorDraftImportControl {
  NativeEditorDraftImportControl(this._owner);
  final RustWorkbench _owner;
  static const _maxBytes = 8 * 1024 * 1024;
  static const _chunkBytes = 32768;

  NativeEditorDraftControl get _draft =>
      _owner.editorDrafts as NativeEditorDraftControl;

  Future<T> _job<T>(Future<T> Function() run) => _draft._job(run);

  Future<void> _abort(String token) => _draft._abort(token);

  Future<(EditorDraftImportSnapshot, BigInt)> _download(
    host.ResponseReader response,
  ) async {
    final token = response.transfer ?? '';
    final total = VersionedContentCodec.unsigned(response.totalLength);
    final revision = VersionedContentCodec.unsigned(response.revision);
    final digest = List<int>.of(response.sha256 ?? const <int>[]);
    try {
      if (token.isEmpty ||
          total <= BigInt.zero ||
          total > BigInt.from(_maxBytes) ||
          digest.length != 32) {
        throw const FormatException('Invalid draft import transfer');
      }
      final length = total.toInt();
      final bytes = BytesBuilder(copy: false);
      var offset = 0;
      while (true) {
        final part = response.payload;
        if (response.transfer != token ||
            VersionedContentCodec.unsigned(response.totalLength) != total ||
            VersionedContentCodec.unsigned(response.revision) != revision ||
            VersionedContentCodec.unsigned(response.offset) !=
                BigInt.from(offset) ||
            !RustWorkbench._same(response.sha256, digest) ||
            part == null ||
            part.isEmpty ||
            part.length > _chunkBytes ||
            part.length > length - offset) {
          throw const FormatException('Draft import transfer mismatch');
        }
        bytes.add(Uint8List.fromList(part));
        offset += part.length;
        if (offset == length) break;
        response = await _owner._call(
          host.Action.readEditorDraftPart,
          configure: (request) {
            request.transfer = token;
            request.offset = offset;
          },
        );
      }
      final body = bytes.takeBytes();
      if (!RustWorkbench._same(sha256.convert(body).bytes, digest)) {
        throw const FormatException('Draft import transfer digest mismatch');
      }
      return (EditorDraftImportCodec.decodeEnvelope(body), revision);
    } catch (_) {
      await _abort(token);
      rethrow;
    }
  }

  Future<(EditorDraftImportSnapshot, BigInt)> _requestEnvelope(
    host.Action action, {
    required void Function(host.RequestBuilder) configure,
  }) async {
    final correlation = 'draft-request-${_owner._sequence++}';
    try {
      return await _download(
        await _owner._call(
          action,
          configure: (request) {
            configure(request);
            request.transfer = correlation;
          },
        ),
      );
    } catch (_) {
      await _abort(correlation);
      rethrow;
    }
  }

  void _identity(
    host.RequestBuilder out,
    String cardId,
    String draftId,
    BigInt generation,
    String operation,
    String importOperation,
  ) {
    out.id = cardId;
    out.attachment = draftId;
    out.revision = EditorDraftImportCodec.wireU64(generation);
    out.operation = operation;
    out.name = importOperation;
  }

  void _context(
    EditorDraftImportSnapshot snapshot,
    BigInt outerRevision,
    String cardId,
    String draftId,
    BigInt generation,
    String operation,
    String importOperation,
  ) {
    if (snapshot.cardId != cardId ||
        snapshot.draftId != draftId ||
        snapshot.expectedGeneration != generation ||
        snapshot.operation != operation ||
        snapshot.importOperation != importOperation ||
        snapshot.currentGeneration != outerRevision) {
      throw const FormatException('Draft import response context mismatch');
    }
  }

  EditorDraftImportRecord _record(EditorDraftImportSnapshot snapshot) {
    if (snapshot.kind != EditorDraftImportResultKind.record ||
        snapshot.record == null) {
      throw const FormatException('Missing draft import record response');
    }
    return snapshot.record!;
  }

  Future<EditorDraftImportSnapshot> _write(
    host.Action action,
    EditorDraftImportRequest request,
    String selectedPath,
  ) {
    final Uint8List body;
    try {
      body = EditorDraftImportCodec.encodeRequest(request);
    } catch (error) {
      return Future.error(
        EditorDraftImportFailure(
          intent: request,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _job(() async {
      try {
        final (snapshot, revision) = await _requestEnvelope(
          action,
          configure: (outer) {
            _identity(
              outer,
              request.cardId,
              request.draftId,
              request.expectedGeneration,
              request.operation,
              request.operation,
            );
            outer.payload = body;
            outer.selectedPath = selectedPath;
          },
        );
        _context(
          snapshot,
          revision,
          request.cardId,
          request.draftId,
          request.expectedGeneration,
          request.operation,
          request.operation,
        );
        final record = _record(snapshot);
        if (!EditorDraftImportCodec.sameRequest(record.request, request) ||
            (action == host.Action.beginEditorDraftImport &&
                record.phase == EditorDraftImportPhase.ready &&
                !record.bytesRetained)) {
          throw const FormatException('Draft import receipt changed proposal');
        }
        return snapshot;
      } catch (error) {
        throw EditorDraftImportFailure(
          intent: request,
          outcomeUnknown: true,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest request) =>
      _write(host.Action.beginEditorDraftImport, request, '');

  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  }) => _write(host.Action.completeEditorDraftImport, request, selectedPath);

  @override
  Future<EditorDraftImportSnapshot> inspect(
    String cardId,
    String draftId,
    String importOperation,
  ) => _job(() async {
    EditorDraftImportCodec.identity(cardId);
    EditorDraftImportCodec.identity(draftId);
    EditorDraftImportCodec.operation(importOperation);
    final (snapshot, revision) = await _requestEnvelope(
      host.Action.inspectEditorDraftImport,
      configure: (outer) => _identity(
        outer,
        cardId,
        draftId,
        BigInt.zero,
        importOperation,
        importOperation,
      ),
    );
    _context(
      snapshot,
      revision,
      cardId,
      draftId,
      BigInt.zero,
      importOperation,
      importOperation,
    );
    if (snapshot.kind != EditorDraftImportResultKind.record &&
        snapshot.kind != EditorDraftImportResultKind.absent) {
      throw const FormatException('Invalid draft import inspection response');
    }
    return snapshot;
  });

  @override
  Future<EditorDraftImportSnapshot> list(String cardId, String draftId) =>
      _job(() async {
        EditorDraftImportCodec.identity(cardId);
        EditorDraftImportCodec.identity(draftId);
        final (snapshot, revision) = await _requestEnvelope(
          host.Action.listEditorDraftImports,
          configure: (outer) =>
              _identity(outer, cardId, draftId, BigInt.zero, '', ''),
        );
        _context(snapshot, revision, cardId, draftId, BigInt.zero, '', '');
        if (snapshot.kind != EditorDraftImportResultKind.list ||
            snapshot.records == null) {
          throw const FormatException('Invalid draft import list response');
        }
        return snapshot;
      });

  @override
  Future<EditorDraftImportSnapshot> export(
    String cardId,
    String draftId,
    BigInt currentGeneration,
    String importOperation,
    String selectedPath,
  ) => _job(() async {
    EditorDraftImportCodec.identity(cardId);
    EditorDraftImportCodec.identity(draftId);
    EditorDraftImportCodec.operation(importOperation);
    EditorDraftImportCodec.u64(
      currentGeneration,
      positive: true,
      canBeMax: false,
    );
    final (snapshot, revision) = await _requestEnvelope(
      host.Action.exportEditorDraftImport,
      configure: (outer) {
        _identity(
          outer,
          cardId,
          draftId,
          currentGeneration,
          importOperation,
          importOperation,
        );
        outer.selectedPath = selectedPath;
      },
    );
    _context(
      snapshot,
      revision,
      cardId,
      draftId,
      currentGeneration,
      importOperation,
      importOperation,
    );
    if (snapshot.kind != EditorDraftImportResultKind.exported ||
        snapshot.exportSha256 == null) {
      throw const FormatException('Invalid draft import export response');
    }
    return snapshot;
  });

  Future<EditorDraftImportSnapshot> _decisionCall(
    host.Action action,
    EditorDraftImportAbandon intent, {
    bool allowAbsent = false,
  }) async {
    final (snapshot, revision) = await _requestEnvelope(
      action,
      configure: (outer) => _identity(
        outer,
        intent.cardId,
        intent.draftId,
        intent.currentGeneration,
        intent.operation,
        intent.importOperation,
      ),
    );
    _context(
      snapshot,
      revision,
      intent.cardId,
      intent.draftId,
      intent.currentGeneration,
      intent.operation,
      intent.importOperation,
    );
    if (allowAbsent && snapshot.kind == EditorDraftImportResultKind.absent) {
      return snapshot;
    }
    final decision = snapshot.decision;
    if (snapshot.kind != EditorDraftImportResultKind.decision ||
        decision == null ||
        decision.operation != intent.operation ||
        decision.request.cardId != intent.cardId ||
        decision.request.draftId != intent.draftId ||
        decision.request.operation != intent.importOperation ||
        decision.expectedGeneration != intent.currentGeneration) {
      throw const FormatException('Draft import decision identity mismatch');
    }
    return snapshot;
  }

  Future<EditorDraftImportSnapshot> _decisionWrite(
    host.Action action,
    EditorDraftImportAbandon intent,
  ) {
    try {
      EditorDraftImportCodec.validateAbandon(intent);
    } catch (error) {
      return Future.error(
        EditorDraftImportFailure(
          intent: intent,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _job(() async {
      try {
        return await _decisionCall(action, intent);
      } catch (error) {
        if (error is EditorDraftImportFailure) rethrow;
        throw EditorDraftImportFailure(
          intent: intent,
          outcomeUnknown: true,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon intent,
  ) => _decisionWrite(host.Action.prepareEditorDraftImportDecision, intent);

  @override
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  ) => _job(() async {
    EditorDraftImportCodec.validateAbandon(intent);
    return _decisionCall(
      host.Action.inspectEditorDraftImportDecision,
      intent,
      allowAbsent: true,
    );
  });

  @override
  Future<EditorDraftImportScopePage> listDecisionScopes({
    String cursor = '',
    int limit = 32,
  }) => _job(() => _listDecisionScopesUnlocked(cursor: cursor, limit: limit));

  Future<EditorDraftImportScopePage> _listDecisionScopesUnlocked({
    String cursor = '',
    int limit = 32,
  }) async {
    EditorDraftImportCodec.validateCursor(cursor);
    if (limit < 1 || limit > 32) {
      throw const FormatException('Invalid decision scope page limit');
    }
    final (snapshot, revision) = await _requestEnvelope(
      host.Action.listEditorDraftImportDecisionScopes,
      configure: (outer) {
        outer.cursor = cursor;
        outer.limit = limit;
      },
    );
    _context(snapshot, revision, '', '', BigInt.zero, '', '');
    if (snapshot.kind != EditorDraftImportResultKind.scopes ||
        snapshot.scopes == null ||
        snapshot.requestCursor != cursor ||
        snapshot.requestLimit != limit) {
      throw const FormatException('Global decision scope page mismatch');
    }
    return EditorDraftImportScopePage(
      requestCursor: cursor,
      requestLimit: limit,
      nextCursor: snapshot.nextCursor,
      scopes: snapshot.scopes!,
    );
  }

  @override
  Future<EditorDraftImportDecisionPage> listDecisions(
    String cardId,
    String draftId, {
    String cursor = '',
    int limit = 32,
  }) => _job(
    () => _listDecisionsUnlocked(cardId, draftId, cursor: cursor, limit: limit),
  );

  Future<EditorDraftImportDecisionPage> _listDecisionsUnlocked(
    String cardId,
    String draftId, {
    String cursor = '',
    int limit = 32,
  }) async {
    EditorDraftImportCodec.identity(cardId);
    EditorDraftImportCodec.identity(draftId);
    EditorDraftImportCodec.validateCursor(cursor);
    if (limit < 1 || limit > 32) {
      throw const FormatException('Invalid draft import decision page request');
    }
    final (snapshot, revision) = await _requestEnvelope(
      host.Action.listEditorDraftImportDecisions,
      configure: (outer) {
        _identity(outer, cardId, draftId, BigInt.zero, '', '');
        outer.cursor = cursor;
        outer.limit = limit;
      },
    );
    _context(snapshot, revision, cardId, draftId, BigInt.zero, '', '');
    if (snapshot.kind != EditorDraftImportResultKind.decisions ||
        snapshot.decisions == null ||
        snapshot.requestCursor != cursor ||
        snapshot.requestLimit != limit) {
      throw const FormatException('Draft import decision page mismatch');
    }
    return EditorDraftImportDecisionPage(
      cardId: cardId,
      draftId: draftId,
      requestCursor: cursor,
      requestLimit: limit,
      nextCursor: snapshot.nextCursor,
      decisions: snapshot.decisions!,
      currentGeneration: snapshot.currentGeneration,
      mainActive: snapshot.mainActive,
      stagingRevision: snapshot.stagingRevision,
    );
  }

  @override
  Future<List<EditorDraftImportDecision>> discoverDecisions() => _job(() async {
    final decisions = <EditorDraftImportDecision>[];
    final seenScopes = <(String, String)>{};
    final seenOperations = <String>{};
    final seenScopeCursors = <String>{};
    var scopeCursor = '';
    do {
      final scopes = await _listDecisionScopesUnlocked(cursor: scopeCursor);
      if (scopes.scopes.isEmpty && scopes.nextCursor.isNotEmpty) {
        throw const FormatException('Empty nonterminal decision scope page');
      }
      for (final scope in scopes.scopes) {
        if (!seenScopes.add((scope.cardId, scope.draftId)) ||
            seenScopes.length > 256) {
          throw const FormatException('Duplicate or excessive decision scopes');
        }
        final seenDecisionCursors = <String>{};
        var decisionCursor = '';
        do {
          final page = await _listDecisionsUnlocked(
            scope.cardId,
            scope.draftId,
            cursor: decisionCursor,
          );
          if (page.decisions.isEmpty && page.nextCursor.isNotEmpty) {
            throw const FormatException('Empty nonterminal decision page');
          }
          for (final decision in page.decisions) {
            if (decision.request.cardId != scope.cardId ||
                decision.request.draftId != scope.draftId ||
                !seenOperations.add(decision.operation) ||
                decisions.length >= 256) {
              throw const FormatException('Duplicate or excessive decisions');
            }
            decisions.add(decision);
          }
          final next = page.nextCursor;
          if (next.isEmpty) break;
          if (next == decisionCursor ||
              !seenDecisionCursors.add(next) ||
              seenDecisionCursors.length > 256) {
            throw const FormatException('Decision cursor did not advance');
          }
          decisionCursor = next;
        } while (true);
      }
      final next = scopes.nextCursor;
      if (next.isEmpty) break;
      if (next == scopeCursor ||
          !seenScopeCursors.add(next) ||
          seenScopeCursors.length > 256) {
        throw const FormatException('Decision scope cursor did not advance');
      }
      scopeCursor = next;
    } while (true);
    return List<EditorDraftImportDecision>.unmodifiable(decisions);
  });

  @override
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  ) async {
    final snapshot = await _decisionWrite(
      host.Action.cancelEditorDraftImportDecision,
      intent,
    );
    if (snapshot.decision?.status !=
        EditorDraftImportDecisionStatus.cancelled) {
      throw EditorDraftImportFailure(
        intent: intent,
        outcomeUnknown: true,
        cause: const FormatException('Draft import decision was not cancelled'),
      );
    }
    return snapshot;
  }

  @override
  Future<EditorDraftImportSnapshot> abandon(EditorDraftImportAbandon intent) {
    try {
      EditorDraftImportCodec.validateAbandon(intent);
    } catch (error) {
      return Future.error(
        EditorDraftImportFailure(
          intent: intent,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _job(() async {
      try {
        // Keep Prepare and Commit inside one transfer job; the public
        // prepareDecision method would deadlock by nesting the same queue.
        final prepared = await _decisionCall(
          host.Action.prepareEditorDraftImportDecision,
          intent,
        );
        final decision = prepared.decision!;
        if (decision.status == EditorDraftImportDecisionStatus.cancelled ||
            decision.status == EditorDraftImportDecisionStatus.conflict) {
          throw EditorDraftImportFailure(
            intent: intent,
            outcomeUnknown: false,
            cause: StateError('Draft import decision cannot be committed'),
          );
        }
        final (snapshot, revision) = await _requestEnvelope(
          host.Action.abandonEditorDraftImport,
          configure: (outer) => _identity(
            outer,
            intent.cardId,
            intent.draftId,
            intent.currentGeneration,
            intent.operation,
            intent.importOperation,
          ),
        );
        _context(
          snapshot,
          revision,
          intent.cardId,
          intent.draftId,
          intent.currentGeneration,
          intent.operation,
          intent.importOperation,
        );
        final record = _record(snapshot);
        if (record.phase != EditorDraftImportPhase.retired ||
            !EditorDraftImportCodec.sameRequest(
              record.request,
              decision.request,
            )) {
          throw const FormatException(
            'Draft import abandon receipt changed decision',
          );
        }
        return snapshot;
      } catch (error) {
        if (error is EditorDraftImportFailure) rethrow;
        throw EditorDraftImportFailure(
          intent: intent,
          outcomeUnknown: true,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftImportSnapshot> reconcile(String cardId, String draftId) =>
      _job(() async {
        EditorDraftImportCodec.identity(cardId);
        EditorDraftImportCodec.identity(draftId);
        final (snapshot, revision) = await _requestEnvelope(
          host.Action.reconcileEditorDraftImports,
          configure: (outer) =>
              _identity(outer, cardId, draftId, BigInt.zero, '', ''),
        );
        _context(snapshot, revision, cardId, draftId, BigInt.zero, '', '');
        if (snapshot.kind != EditorDraftImportResultKind.reconciled ||
            snapshot.records == null) {
          throw const FormatException(
            'Invalid draft import reconcile response',
          );
        }
        return snapshot;
      });
}
