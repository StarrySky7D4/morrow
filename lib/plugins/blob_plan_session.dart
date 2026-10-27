import 'dart:typed_data';
import 'package:crypto/crypto.dart' as crypto;
import 'blob_plan_models.dart';
import 'io_task_models.dart';

/// Retained by the original backend pair across view replacement. This covers
/// only this living host session; it does not claim recovery after a restart.
final class BlobPlanSession {
  BlobPlanSession._(this.backend, this.io);
  static final _sessions = Expando<Expando<BlobPlanSession>>();
  static BlobPlanSession forBackend(
    BlobPlanBackend backend,
    WorkbenchIoTaskControl io,
  ) {
    final peers = _sessions[backend] ??= Expando<BlobPlanSession>();
    return peers[io] ??= BlobPlanSession._(backend, io);
  }

  final BlobPlanBackend backend;
  final WorkbenchIoTaskControl io;
  Object? _view;
  bool _busy = false,
      _idle = false,
      _readAttempted = false,
      _deliveryLost = false;
  bool _approvalAttempted = false, _ackAttempted = false;
  bool _cancelRequested = false,
      _planCancelSent = false,
      _uploadCancelSent = false;
  BlobPlanRequest? _request;
  Uint8List? _inputHash;
  BlobPlanSnapshot? _snapshot;
  BlobPlanResult? _result;
  String? _failure;
  int _completed = 0;
  final List<BlobPlanReply> _history = [];
  bool get busy => _busy;
  BlobPlanRequest? get request => _request;
  BlobPlanSnapshot? get snapshot => _snapshot;
  BlobPlanResult? get result => _result;
  bool get deliveryLost => _deliveryLost;
  bool get approvalAttempted => _approvalAttempted;
  bool get acknowledgementUncertain => _ackAttempted && _request != null;
  String? get failure => _failure;
  List<BlobPlanReply> get history => List.unmodifiable(_history);
  bool get canStart =>
      !_busy && _view != null && _idle && _request == null && _completed < 1024;
  bool get canRead =>
      !_busy &&
      !_readAttempted &&
      _snapshot?.ready == true &&
      _snapshot?.delivered == false;
  bool get canApprove =>
      !_busy &&
      _view != null &&
      !_approvalAttempted &&
      !_cancelRequested &&
      !_deliveryLost &&
      _snapshot?.delivered == true &&
      _snapshot?.originalWorkerJoined == true &&
      _snapshot?.consumed == false &&
      _snapshot?.error == null &&
      _result?.view != null &&
      _result?.error == null;
  bool get canAcknowledge =>
      !_busy &&
      !_ackAttempted &&
      _snapshot?.running == false &&
      _snapshot?.delivered == true;
  Object attachView() {
    if (_view != null) throw StateError('Planning view already attached');
    return _view = Object();
  }

  bool _current(Object view) => identical(view, _view);
  Future<void> detachView(Object view, {bool cancelOriginal = true}) async {
    if (!_current(view)) return;
    _view = null;
    if (_request == null) return;
    // A consumed plan may have an already joined/reclaimed upload whose finish
    // decision was sent. Its trusted view can close without cancelling again;
    // the original request and unresolved acknowledgement remain retained.
    if (!cancelOriginal && _snapshot?.consumed == true) return;
    _cancelRequested = true;
    if (!_busy) {
      _busy = true;
      try {
        await _cancelOriginal();
      } finally {
        _busy = false;
      }
    }
  }

  static bool _same(Uint8List? a, Uint8List? b) =>
      a != null &&
      b != null &&
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);
  void _accept(BlobPlanSnapshot value) {
    final prior = _snapshot;
    if (_request == null ||
        !_same(value.submission, _request!.submission) ||
        (prior != null &&
            (!_same(value.key, prior.key) ||
                (prior.delivered && !value.delivered) ||
                (prior.consumed && !value.consumed) ||
                (prior.originalWorkerJoined && !value.originalWorkerJoined) ||
                (prior.uploadKey != null &&
                    !_same(value.uploadKey, prior.uploadKey)) ||
                (prior.uploadSubmission != null &&
                    !_same(value.uploadSubmission, prior.uploadSubmission))))) {
      throw const FormatException(
        'Original planning identity or history changed',
      );
    }
    _snapshot = value;
  }

  Future<bool> _run(Object view, Future<void> Function() work) async {
    if (!_current(view) || _busy) return false;
    _busy = true;
    _failure = null;
    try {
      await work();
      return true;
    } catch (_) {
      _failure = 'original-plan-requires-observation';
      return false;
    } finally {
      if (_cancelRequested) await _cancelOriginal();
      _busy = false;
    }
  }

  Future<void> _observe() async {
    final request = _request;
    if (request == null) throw StateError('No original planning request');
    final found = await backend.discoverBlobPlan(request.submission);
    if (found == null)
      throw StateError('Absence does not prove failure or acknowledgement');
    _accept(found);
  }

  Future<void> _cancelOriginal() async {
    if (_request == null || _ackAttempted) return;
    try {
      // A missing approval reply may conceal an already admitted upload.
      // Discovery is read-only and never retries that approval.
      if (_snapshot == null || (_approvalAttempted && !_snapshot!.consumed))
        await _observe();
      final state = _snapshot!;
      if (state.consumed) {
        final key = state.uploadKey;
        if (key != null && !_uploadCancelSent) {
          _uploadCancelSent = true;
          await io.cancelIo(key);
        }
      } else if (!_planCancelSent) {
        _planCancelSent = true;
        _accept(await backend.cancelBlobPlan(state.key));
      }
    } catch (_) {
      _failure = 'original-plan-cancellation-requires-observation';
    }
  }

  Future<bool> refresh(Object view) => _run(view, () async {
    if (_request == null) {
      final current = await io.ioStatus();
      _idle = current.key == null && current.storage == IoStoragePhase.local;
      return;
    }
    await _observe();
    if (_snapshot!.delivered && _result == null) _deliveryLost = true;
    if (_snapshot!.ready && !_snapshot!.delivered) _readAttempted = false;
  });
  Future<bool> start(Object view, BlobPlanRequest request) {
    if (!canStart || !_current(view)) return Future.value(false);
    BlobPlanValidation.request(request);
    _request = request;
    _inputHash = Uint8List.fromList(crypto.sha256.convert(request.input).bytes);
    _idle = false;
    return _run(view, () async {
      _accept(await backend.startBlobPlan(request));
    });
  }

  Future<bool> read(Object view) {
    if (!canRead) return Future.value(false);
    return _run(view, () async {
      _readAttempted = true;
      try {
        final reply = await backend.readBlobPlan(_snapshot!.key);
        if (reply.snapshot == null)
          throw const FormatException('Original plan absent');
        _accept(reply.snapshot!);
        if (!_current(view)) {
          _deliveryLost = true;
          return;
        }
        final result = reply.result,
            proposed = result?.view,
            request = _request!;
        if (proposed != null &&
            (proposed.packageId != request.packageId ||
                !_same(proposed.packageDigest, request.packageDigest) ||
                proposed.registryRevision != request.registryRevision ||
                proposed.plannerHandler != request.handler ||
                !_same(proposed.planningInputSha256, _inputHash))) {
          throw const FormatException(
            'Proposal does not belong to the original input',
          );
        }
        if (result == null) {
          _readAttempted = _snapshot!.delivered;
          _deliveryLost = _snapshot!.delivered;
        } else {
          _result = result;
          _deliveryLost = false;
        }
      } catch (_) {
        _deliveryLost = true;
        rethrow;
      }
    });
  }

  Future<bool> approve(Object view) {
    if (!canApprove || !_current(view)) return Future.value(false);
    // Commit the local decision before any asynchronous exchange. The user can
    // observe or cancel afterwards, but cannot silently repeat this decision.
    _approvalAttempted = true;
    return _run(view, () async {
      _accept(await backend.approveBlobPlan(_snapshot!.key, _result!.view!));
    });
  }

  Future<bool> cancel(Object view) => _run(view, () async {
    _cancelRequested = true;
    await _cancelOriginal();
  });
  Future<bool> acknowledge(Object view) {
    if (!canAcknowledge) return Future.value(false);
    return _run(view, () async {
      // A consumed plan must retain its identity while its original upload is
      // running, awaiting delivery, or awaiting recovery/acknowledgement.
      final idle = await io.ioStatus();
      if (idle.key != null || idle.storage != IoStoragePhase.local)
        throw StateError('Original upload not acknowledged');
      _ackAttempted = true;
      await backend.acknowledgeBlobPlan(_snapshot!.key);
      _history.add(BlobPlanReply(_snapshot, _result));
      _completed++;
      _request = null;
      _inputHash = null;
      _snapshot = null;
      _result = null;
      _idle = true;
      _readAttempted = false;
      _deliveryLost = false;
      _approvalAttempted = false;
      _ackAttempted = false;
      _cancelRequested = false;
      _planCancelSent = false;
      _uploadCancelSent = false;
    });
  }
}
