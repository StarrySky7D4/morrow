part of 'workbench_native.dart';

/// Serializes whole draft transfers, independently of settings transfers.
/// Operation identities belong to the caller's frozen proposal. This adapter
/// never invents a replacement operation or submits business card edits.
final class NativeEditorDraftControl implements EditorDraftControl {
  NativeEditorDraftControl(this._owner);
  final RustWorkbench _owner;
  Future<void> _tail = Future.value();
  static const _maxBytes = 8 * 1024 * 1024;
  static const _chunkBytes = 32768;

  Future<T> _job<T>(Future<T> Function() run) {
    final result = Completer<T>();
    _tail = _tail.then((_) async {
      try {
        result.complete(await run());
      } catch (error, stack) {
        result.completeError(error, stack);
      }
    });
    return result.future;
  }

  Future<void> _abort(String token) async {
    if (token.isEmpty) return;
    try {
      await _owner._call(
        host.Action.abortEditorDraftTransfer,
        configure: (request) => request.transfer = token,
      );
    } catch (_) {
      // The original failure remains authoritative. This only frees a buffer.
    }
  }

  Future<(EditorDraftEnvelope, BigInt)> _download(
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
        throw const FormatException('Invalid editor draft transfer');
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
          throw const FormatException('Editor draft transfer mismatch');
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
        throw const FormatException('Editor draft transfer digest mismatch');
      }
      return (EditorDraftCodec.decodeEnvelope(body), revision);
    } catch (_) {
      await _abort(token);
      rethrow;
    }
  }

  Future<(EditorDraftEnvelope, BigInt)> _requestEnvelope(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
  }) async {
    // Known before sending: even a lost first response can release exactly
    // this request's reply buffer. The correlation is not a business operation.
    final correlation = 'draft-request-${_owner._sequence++}';
    try {
      return await _download(
        await _owner._call(
          action,
          configure: (request) {
            configure?.call(request);
            request.transfer = correlation;
          },
        ),
      );
    } catch (_) {
      await _abort(correlation);
      rethrow;
    }
  }

  void _context(
    EditorDraftEnvelope envelope,
    String cardId,
    String draftId,
    String operation,
    BigInt expectedGeneration,
  ) {
    if (envelope.cardId != cardId ||
        envelope.draftId != draftId ||
        envelope.operation != operation ||
        envelope.expectedGeneration != expectedGeneration) {
      throw const FormatException('Editor draft response context mismatch');
    }
  }

  EditorDraftRecord _record(EditorDraftEnvelope envelope, BigInt revision) {
    final record = envelope.record;
    if (envelope.kind != EditorDraftResultKind.record ||
        record == null ||
        record.currentGeneration != revision) {
      throw const FormatException('Invalid editor draft record response');
    }
    return record;
  }

  void _identity(
    host.RequestBuilder request,
    String cardId,
    String draftId,
    BigInt generation,
  ) {
    request.id = cardId;
    request.attachment = draftId;
    request.revision = VersionedContentCodec.wireU64(generation);
  }

  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) {
    // Encode before admission, so the exact proposal is fixed while queued.
    final Uint8List body;
    try {
      body = EditorDraftCodec.encodeWrite(request);
    } catch (error) {
      return Future.error(
        EditorDraftSaveFailure(
          request: request,
          outcomeUnknown: false,
          cause: error,
        ),
      );
    }
    return _job(() async {
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
          throw const FormatException('Invalid editor draft upload admission');
        }
        for (var offset = 0; offset < body.length;) {
          final end = (offset + _chunkBytes).clamp(0, body.length);
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
            throw const FormatException('Editor draft upload receipt mismatch');
          }
          offset = end;
        }
        outcomeUnknown = true;
        final reply = await _owner._call(
          host.Action.finishEditorDraft,
          configure: (outer) {
            outer.transfer = token;
            outer.operation = request.operation;
            _identity(
              outer,
              request.cardId,
              request.draftId,
              request.expectedGeneration,
            );
          },
        );
        final (envelope, revision) = await _download(reply);
        _context(
          envelope,
          request.cardId,
          request.draftId,
          request.operation,
          request.expectedGeneration,
        );
        final record = _record(envelope, revision);
        if (!record.active ||
            record.generation != request.expectedGeneration + BigInt.one ||
            !RustWorkbench._same(
              EditorDraftCodec.encodeWrite(record.request),
              body,
            )) {
          throw const FormatException(
            'Editor draft save receipt changed proposal',
          );
        }
        return record;
      } catch (error) {
        await _abort(token);
        throw EditorDraftSaveFailure(
          request: request,
          outcomeUnknown: outcomeUnknown,
          cause: error,
        );
      }
    });
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) =>
      _job(() async {
        final (envelope, revision) = await _requestEnvelope(
          host.Action.readEditorDraft,
          configure: (request) =>
              _identity(request, cardId, draftId, BigInt.zero),
        );
        _context(envelope, cardId, draftId, '', BigInt.zero);
        if (envelope.kind == EditorDraftResultKind.absent) {
          if (revision != BigInt.zero) {
            throw const FormatException('Absent editor draft has a revision');
          }
          return null;
        }
        final record = _record(envelope, revision);
        if (record.generation != record.currentGeneration ||
            record.active != record.currentActive ||
            record.repeated) {
          throw const FormatException(
            'Editor draft read returned historical state',
          );
        }
        return record;
      });

  @override
  Future<List<EditorDraftSummary>> list() => _job(() async {
    final (envelope, revision) = await _requestEnvelope(
      host.Action.listEditorDrafts,
    );
    _context(envelope, '', '', '', BigInt.zero);
    if (envelope.kind != EditorDraftResultKind.list ||
        envelope.summaries == null ||
        revision != BigInt.zero) {
      throw const FormatException('Invalid editor draft list response');
    }
    return envelope.summaries!;
  });

  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) => _job(() async {
    final (envelope, revision) = await _requestEnvelope(
      host.Action.discardEditorDraft,
      configure: (request) {
        _identity(request, cardId, draftId, expectedGeneration);
        request.operation = operation;
      },
    );
    _context(envelope, cardId, draftId, operation, expectedGeneration);
    final record = _record(envelope, revision);
    if (record.active ||
        record.currentActive ||
        record.generation != expectedGeneration + BigInt.one) {
      throw const FormatException('Editor draft discard receipt mismatch');
    }
    return record;
  });

  @override
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  ) => _job(() async {
    final (envelope, revision) = await _requestEnvelope(
      host.Action.importEditorDraftAsset,
      configure: (request) {
        _identity(request, cardId, draftId, generation);
        request.selectedPath = path;
        request.name = name;
        request.kind = kind;
        request.totalLength = VersionedContentCodec.wireU64(bytes);
      },
    );
    _context(envelope, cardId, draftId, '', generation);
    final asset = envelope.asset;
    if (envelope.kind != EditorDraftResultKind.imported ||
        asset == null ||
        revision != generation ||
        asset.name != name ||
        asset.kind != kind ||
        asset.bytes != bytes) {
      throw const FormatException('Editor draft imported asset mismatch');
    }
    return asset;
  });

  @override
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  ) => _job(() async {
    final (envelope, revision) = await _requestEnvelope(
      host.Action.exportEditorDraftAsset,
      configure: (request) {
        _identity(request, cardId, draftId, generation);
        request.name = assetId;
        request.selectedPath = path;
      },
    );
    _context(envelope, cardId, draftId, '', generation);
    if (envelope.kind != EditorDraftResultKind.exported ||
        revision != generation) {
      throw const FormatException('Invalid editor draft export response');
    }
  });
}
