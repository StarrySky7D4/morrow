import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'blob_history_delivery_models.dart';

/// Retained by the original backend, independently of a dialog's lifetime.
/// Recovery is in-process only. Detaching a view never sends another request.
final class BlobHistoryDeliverySession {
  BlobHistoryDeliverySession._(this.backend);
  static final _sessions = Expando<BlobHistoryDeliverySession>();
  static BlobHistoryDeliverySession forBackend(
    BlobHistoryDeliveryBackend backend,
  ) => _sessions[backend] ??= BlobHistoryDeliverySession._(backend);

  final BlobHistoryDeliveryBackend backend;
  Object? _view;
  void Function()? _changed;
  bool _busy = false, _reviewed = false, _approveSent = false;
  bool _cancelSent = false, _ackSent = false;
  int _completed = 0;
  BlobHistoryDeliveryRequest? _request;
  BlobHistoryDeliveryPreview? _preview;
  BlobHistoryDeliveryReceipt? _receipt;
  Uint8List? _key;
  bool _observationRequired = false;

  bool get busy => _busy;
  bool get reviewed => _reviewed;
  bool get observationRequired => _observationRequired;
  BlobHistoryDeliveryRequest? get request => _request;
  BlobHistoryDeliveryPreview? get preview => _preview;
  BlobHistoryDeliveryReceipt? get receipt => _receipt;
  bool get canPrepare =>
      !_busy && _view != null && _request == null && _completed < 512;
  bool get canReview =>
      !_busy &&
      !_observationRequired &&
      _preview != null &&
      !_approveSent &&
      !_cancelSent &&
      _receipt?.consumed != true;
  bool get canApprove => canReview && _reviewed;
  bool get canCancel =>
      !_busy &&
      _key != null &&
      !_approveSent &&
      !_cancelSent &&
      _receipt?.consumed != true;
  bool get canRead => !_busy && _view != null && _request != null;
  bool get canAcknowledge =>
      !_busy && !_ackSent && _receipt?.canAcknowledge == true;
  bool get canFinish => !_busy && _receipt?.acknowledged == true;

  Object attachView(void Function() changed) {
    if (_view != null) {
      throw StateError('History delivery view already attached');
    }
    _reviewed = false;
    _changed = changed;
    return _view = Object();
  }

  void detachView(Object view) {
    if (!identical(_view, view)) return;
    _view = null;
    _changed = null;
    _reviewed = false;
  }

  bool _current(Object view) => identical(_view, view);
  void review(Object view, bool value) {
    if (!_current(view) || !canReview) return;
    _reviewed = value;
    _changed?.call();
  }

  static bool _same(List<int>? a, List<int>? b) =>
      a != null &&
      b != null &&
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);
  static bool _sameRequest(
    BlobHistoryDeliveryRequest a,
    BlobHistoryDeliveryRequest b,
  ) =>
      _same(a.requestId, b.requestId) &&
      _same(a.submission, b.submission) &&
      a.packageId == b.packageId &&
      _same(a.packageDigest, b.packageDigest) &&
      a.registryRevision == b.registryRevision &&
      a.handler == b.handler &&
      a.outputType == b.outputType &&
      a.timeoutMs == b.timeoutMs;

  void _acceptPreview(BlobHistoryDeliveryPreview value) {
    if (_request == null ||
        !_sameRequest(value.request, _request!) ||
        value.key.length != 32 ||
        value.key.every((n) => n == 0) ||
        (_key != null && !_same(_key, value.key)) ||
        !_same(sha256.convert(value.packet).bytes, value.packetSha256) ||
        !_same(value.selectedHistory.submission, _request!.submission)) {
      throw const FormatException('Original history disclosure changed');
    }
    if (_preview != null &&
        (!_same(_preview!.packet, value.packet) ||
            _preview!.taskId != value.taskId)) {
      throw const FormatException('Original history preview changed');
    }
    _key = value.key;
    _preview = value;
  }

  void _acceptReceipt(BlobHistoryDeliveryReceipt value) {
    final prior = _receipt;
    if (_request == null ||
        !_same(value.requestId, _request!.requestId) ||
        value.key.length != 32 ||
        value.key.every((n) => n == 0) ||
        (_key != null && !_same(_key, value.key)) ||
        (prior != null &&
            ((prior.consumed && !value.consumed) ||
                (prior.dispatched && !value.dispatched) ||
                (prior.finished && !value.finished) ||
                (prior.acknowledged && !value.acknowledged) ||
                (prior.finished && prior.error != value.error) ||
                ((prior.close == BlobHistoryDeliveryClose.failed ||
                        prior.close == BlobHistoryDeliveryClose.unknown) &&
                    prior.close != value.close)))) {
      throw const FormatException('Original history receipt changed');
    }
    _key = value.key;
    _receipt = value;
  }

  Future<bool> _run(Object view, Future<void> Function() operation) async {
    if (!_current(view) || _busy) return false;
    _busy = true;
    _reviewed = false;
    _observationRequired = false;
    _changed?.call();
    try {
      await operation();
      return true;
    } catch (_) {
      _observationRequired = true;
      return false;
    } finally {
      _busy = false;
      _changed?.call();
    }
  }

  Future<bool> prepare(Object view, BlobHistoryDeliveryRequest request) {
    if (!_current(view) || !canPrepare) return Future.value(false);
    // Consume the local attempt before any exchange, including synchronous errors.
    _request = request;
    return _run(
      view,
      () async =>
          _acceptPreview(await backend.prepareBlobHistoryDelivery(request)),
    );
  }

  Future<bool> approve(Object view) {
    if (!_current(view) || !canApprove) return Future.value(false);
    final original = _preview!;
    _approveSent = true;
    return _run(
      view,
      () async =>
          _acceptReceipt(await backend.approveBlobHistoryDelivery(original)),
    );
  }

  Future<bool> read(Object view) {
    if (!_current(view) || !canRead) return Future.value(false);
    return _run(view, () async {
      final original = _request!;
      final key = _key;
      final value = key == null
          ? await backend.discoverBlobHistoryDelivery(
              original.requestId,
              expectedOutputType: original.outputType,
            )
          : await backend.readBlobHistoryDelivery(
              key,
              original.requestId,
              expectedOutputType: original.outputType,
            );
      // Absent is not proof that a lost preparation failed. Keep its identity.
      if (value == null) throw StateError('Original disclosure not observable');
      _acceptReceipt(value);
      if (!value.consumed && !_approveSent && !_cancelSent) {
        _acceptPreview(
          await backend.previewBlobHistoryDelivery(value.key, original),
        );
      }
    });
  }

  Future<bool> cancel(Object view) {
    if (!_current(view) || !canCancel) return Future.value(false);
    _cancelSent = true;
    return _run(
      view,
      () async => _acceptReceipt(
        await backend.cancelBlobHistoryDelivery(_key!, _request!.requestId),
      ),
    );
  }

  Future<bool> acknowledge(Object view) {
    if (!_current(view) || !canAcknowledge) return Future.value(false);
    final original = _receipt!;
    _ackSent = true;
    return _run(
      view,
      () async => _acceptReceipt(
        await backend.acknowledgeBlobHistoryDelivery(original),
      ),
    );
  }

  bool finish(Object view) {
    if (!_current(view) || !canFinish) return false;
    _completed++;
    _request = null;
    _preview = null;
    _receipt = null;
    _key = null;
    _reviewed = false;
    _approveSent = false;
    _cancelSent = false;
    _ackSent = false;
    _observationRequired = false;
    _changed?.call();
    return true;
  }
}
