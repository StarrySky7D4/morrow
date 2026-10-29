import 'dart:typed_data';
import 'blob_upload_models.dart';
import 'io_task_models.dart';

/// Retained by the original backend pair, not by a widget. A lost acknowledgement
/// is never permission to submit the upload again. This is in-process recovery;
/// it does not claim durable recovery after a host/process restart.
final class BlobUploadSession {
  BlobUploadSession._(this.backend, this.io);
  static final _sessions = Expando<Expando<BlobUploadSession>>();
  static BlobUploadSession forBackend(
    BlobUploadBackend backend,
    WorkbenchIoTaskControl io,
  ) {
    final peers = _sessions[backend] ??= Expando<BlobUploadSession>();
    return peers[io] ??= BlobUploadSession._(backend, io);
  }

  final BlobUploadBackend backend;
  final WorkbenchIoTaskControl io;
  Object? _view;
  bool _busy = false, _cancelRequested = false, _cancelSent = false;
  bool _readAttempted = false, _deliveryLost = false, _confirmed = false;
  bool _ackAttempted = false;
  BlobUploadRequest? _attempt;
  Uint8List? _submission, _originalKey;
  BlobUploadSnapshot? _snapshot;
  BlobUploadResult? _result;
  IoTaskSnapshot? _idle;
  String? _failure;
  int _completed = 0;
  final List<BlobUploadReply> _history = [];
  BlobUploadMetadataReview? _metadataReview;
  final Set<BlobUploadMetadataPhase> _metadataAttempts = {};

  bool get busy => _busy;

  /// Read-only correlation for a trusted view of this original attempt.
  Uint8List? get submission => _submission;
  Uint8List? get originalKey => _originalKey;
  bool get acknowledgementUncertain => _ackAttempted && _submission != null;
  BlobUploadRequest? get attempt => _attempt;
  BlobUploadSnapshot? get snapshot => _snapshot;
  BlobUploadResult? get result => _result;
  bool get deliveryLost => _deliveryLost;
  String? get failure => _failure;
  List<BlobUploadReply> get history => List.unmodifiable(_history);
  BlobUploadMetadataReview? get metadataReview => _metadataReview;
  bool get canReviewMetadata =>
      !_busy &&
      _view != null &&
      backend is BlobUploadMetadataBackend &&
      _originalKey != null &&
      _snapshot?.task.exit != null &&
      (_snapshot?.recoveryRequired == true || _ackAttempted);
  bool get canCompleteMetadata =>
      !_busy &&
      _view != null &&
      _metadataReview != null &&
      !_metadataAttempts.contains(_metadataReview!.phase);
  bool get canStart =>
      !_busy &&
      _view != null &&
      _submission == null &&
      _completed < 512 &&
      _idle?.key == null &&
      _idle?.storage == IoStoragePhase.local;
  bool get canRead =>
      !_busy &&
      !_readAttempted &&
      _snapshot?.ready == true &&
      _snapshot?.delivered == false;
  bool get canAcknowledge =>
      !_busy &&
      !_ackAttempted &&
      _snapshot?.delivered == true &&
      _snapshot?.recoveryRequired == false &&
      _snapshot?.task.storage == IoStoragePhase.reclaimed &&
      _snapshot?.task.exit != null &&
      ((!_deliveryLost && _result != null) || _confirmed);

  Object attachView() {
    if (_view != null) throw StateError('Upload view already attached');
    return _view = Object();
  }

  bool _current(Object view) => identical(_view, view);
  Future<void> detachView(Object view, {bool cancelOriginal = true}) async {
    if (!_current(view)) return;
    _view = null;
    _metadataReview = null;
    // A planning view can delegate cancellation to its retained plan owner,
    // including an approval whose upload key has not been delivered yet.
    if (!cancelOriginal) return;
    if (_submission == null) return;
    _cancelRequested = true;
    if (!_busy) await _cancelOriginal();
  }

  static bool _same(Uint8List? a, Uint8List? b) =>
      a != null &&
      b != null &&
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);
  void _accept(BlobUploadSnapshot value) {
    if (_submission == null ||
        !_same(value.task.submission, _submission) ||
        value.task.key == null ||
        (_originalKey != null && !_same(value.task.key, _originalKey))) {
      throw const FormatException('Original upload identity changed');
    }
    _snapshot = value;
    _originalKey = value.task.key;
  }

  Future<bool> _run(Object view, Future<void> Function() operation) async {
    if (!_current(view) || _busy) return false;
    _busy = true;
    _failure = null;
    try {
      await operation();
      return true;
    } catch (_) {
      // Do not surface plugin payloads, paths or exception contents in a UI.
      _failure = 'original-task-requires-observation';
      return false;
    } finally {
      if (_cancelRequested) await _cancelOriginal();
      _busy = false;
    }
  }

  Future<void> _cancelOriginal() async {
    final key = _originalKey;
    if (key == null || _cancelSent || _ackAttempted) return;
    // Mark before the exchange; an absent response must not send another cancel.
    _cancelSent = true;
    try {
      await io.cancelIo(key);
    } catch (_) {
      _failure = 'original-cancellation-requires-observation';
    }
  }

  Future<bool> refresh(Object view) => _run(view, () async {
    if (_submission == null) {
      _idle = await io.ioStatus();
      return;
    }
    var key = _originalKey;
    if (key == null) {
      final current = await io.ioStatus();
      if (!_same(current.submission, _submission) || current.key == null) {
        // A different/no task is not proof our start failed. Retain the attempt.
        throw StateError('Original upload not observable');
      }
      key = current.key!;
    }
    _accept(await backend.statusBlobUpload(key));
    if (_snapshot!.delivered && _result == null) _deliveryLost = true;
    if (_snapshot!.ready && !_snapshot!.delivered) _readAttempted = false;
  });
  Future<bool> start(Object view, BlobUploadRequest request) {
    if (!canStart || !_current(view)) return Future.value(false);
    BlobUploadValidation.request(request);
    // Freeze the attempt before dispatch. Even a synchronous backend failure
    // cannot authorize automatic replay under a new identity.
    _attempt = request;
    _submission = request.submission;
    _idle = null;
    return _run(view, () async {
      _accept(await backend.startBlobUpload(request));
    });
  }

  /// Adopt only the host-generated identity returned by an approved original
  /// plan. This observes an existing upload; it never synthesizes another input.
  Future<bool> adoptOriginal(Object view, Uint8List key, Uint8List submission) {
    if (!_current(view) || _busy || _completed >= 512) {
      return Future.value(false);
    }
    final exactKey = HttpTaskValidation.identity(key),
        exactSubmission = HttpTaskValidation.identity(submission);
    if (_submission != null &&
        (!_same(_submission, exactSubmission) ||
            (_originalKey != null && !_same(_originalKey, exactKey)))) {
      return Future.value(false);
    }
    _submission = exactSubmission;
    _originalKey = exactKey;
    _idle = null;
    return _run(view, () async {
      _accept(await backend.statusBlobUpload(exactKey));
      if (_snapshot!.delivered && _result == null) _deliveryLost = true;
    });
  }

  Future<bool> read(Object view) {
    if (!canRead) return Future.value(false);
    return _run(view, () async {
      _readAttempted = true;
      try {
        final value = await backend.readBlobUpload(_snapshot!.task.key!);
        _accept(value.snapshot);
        if (!_current(view)) {
          _deliveryLost = true;
          return;
        }
        if (value.result == null) {
          // A pending read is retryable only with an explicit new user action
          // and only when native status says it was not consumed.
          _readAttempted = value.snapshot.delivered;
          _deliveryLost = value.snapshot.delivered;
        } else {
          _result = value.result;
          _deliveryLost = false;
        }
      } catch (_) {
        _deliveryLost = true;
        rethrow;
      }
    });
  }

  Future<bool> cancel(Object view) => _run(view, () async {
    _cancelRequested = true;
    await _cancelOriginal();
  });
  Future<bool> inspect(Object view) => _run(view, () async {
    final key = _snapshot?.task.key;
    if (key == null) throw StateError('Original key unavailable');
    _confirmed = false;
    _accept(await backend.inspectBlobUpload(key));
  });
  Future<bool> confirm(Object view, BlobUploadObservation expected) =>
      _run(view, () async {
        final snapshot = _snapshot;
        if (snapshot == null ||
            !snapshot.delivered ||
            snapshot.observed != expected) {
          throw StateError('Original history has not been reviewed');
        }
        _accept(await backend.confirmBlobUpload(snapshot.task.key!, expected));
        // A detached view cannot approve acknowledgement of an unread result.
        _confirmed = _current(view) && !_snapshot!.recoveryRequired;
      });
  Future<bool> repair(Object view) => _run(view, () async {
    final key = _snapshot?.task.key;
    if (key == null) throw StateError('Original key unavailable');
    _metadataReview = null;
    await io.repairIo(key);
    _accept(await backend.statusBlobUpload(key));
  });
  Future<bool> reviewMetadata(Object view) {
    if (!canReviewMetadata) return Future.value(false);
    return _run(view, () async {
      _metadataReview = null;
      final value = await (backend as BlobUploadMetadataBackend)
          .reviewBlobUploadMetadata(_originalKey!);
      if (!_same(value.key, _originalKey) ||
          !_same(value.submission, _submission) ||
          _metadataAttempts.contains(value.phase) ||
          ((value.phase == BlobUploadMetadataPhase.acknowledgement) !=
              _ackAttempted)) {
        throw const FormatException('Original metadata review changed');
      }
      if (_current(view)) _metadataReview = value;
    });
  }

  Future<bool> completeMetadata(Object view) {
    if (!canCompleteMetadata) return Future.value(false);
    return _run(view, () async {
      final review = _metadataReview!;
      _metadataReview = null;
      _metadataAttempts.add(review.phase);
      final value = await (backend as BlobUploadMetadataBackend)
          .completeBlobUploadMetadata(review);
      if (review.phase == BlobUploadMetadataPhase.acknowledgement) {
        if (value.upload != null || !_ackAttempted) {
          throw const FormatException('Original acknowledgement changed');
        }
        _rememberAcknowledged(value.task);
      } else {
        if (value.upload == null) {
          throw const FormatException('Original upload record unavailable');
        }
        _accept(value.upload!);
      }
    });
  }

  void _rememberAcknowledged(IoTaskSnapshot idle) {
    if (idle.key != null ||
        idle.submission != null ||
        idle.storage != IoStoragePhase.local) {
      throw StateError('Original acknowledgement unresolved');
    }
    _history.add(BlobUploadReply(_snapshot!, _result));
    _completed++;
    _attempt = null;
    _submission = null;
    _originalKey = null;
    _snapshot = null;
    _result = null;
    _idle = idle;
    _readAttempted = false;
    _deliveryLost = false;
    _confirmed = false;
    _cancelRequested = false;
    _cancelSent = false;
    _ackAttempted = false;
    _metadataReview = null;
    _metadataAttempts.clear();
  }

  Future<bool> acknowledge(Object view) {
    if (!canAcknowledge) return Future.value(false);
    return _run(view, () async {
      final closed = _snapshot!;
      // A missing response may follow a completed acknowledgement. Retain this
      // attempt and permit observation only; never send that decision twice.
      _ackAttempted = true;
      _metadataReview = null;
      final idle = await io.acknowledgeIo(closed.task.key!);
      _rememberAcknowledged(idle);
    });
  }
}
