import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import 'channel_task_models.dart';

/// A read-only selection owned by one open workbench. Reading never grants a
/// guest a page and never writes a cursor. Each read verifies the original scope.
final class CardChangesWatch extends ChangeNotifier {
  CardChangesWatch._(this._backend);
  static final _owners = Expando<CardChangesWatch>('reviewed card watches');
  static CardChangesWatch forBackend(ChangesObservationBackend backend) =>
      _owners[backend] ??= CardChangesWatch._(backend);
  static void closeFor(ChangesObservationBackend backend) =>
      _owners[backend]?.stop();

  static const lifetime = Duration(hours: 1);
  static const interval = Duration(seconds: 3);
  final ChangesObservationBackend _backend;
  int _generation = 0;
  Timer? _next, _expiry;
  bool active = false, expired = false;
  Future<void>? _read;
  Object? failure;
  String? packageId;
  Uint8List? _digest;
  BigInt? _revision;
  ChangesWindowReview? selection;
  ReviewedChangesProgress? progress;

  bool matches(String id, Uint8List digest, BigInt revision) =>
      packageId == id &&
      _revision == revision &&
      _digest != null &&
      ChannelValidation.same(_digest!, digest);

  Future<void> start(
    String id,
    Uint8List digest,
    BigInt revision,
    ChangesWindowReview selected,
  ) async {
    if (active) throw StateError('A card watch is already active');
    _digest = ChannelValidation.identity(digest);
    ChannelValidation.u64(revision);
    packageId = id;
    _revision = revision;
    selection = selected;
    _generation++;
    active = true;
    expired = false;
    failure = null;
    progress = null;
    _expiry = Timer(lifetime, () {
      expired = true;
      _end();
    });
    notifyListeners();
    await check();
  }

  Future<void> check() {
    if (!active) return Future.value();
    final generation = _generation;
    return _read ??= Future<void>.microtask(() => _observe(generation));
  }

  Future<void> _observe(int generation) async {
    if (!active || generation != _generation) return;
    _next?.cancel();
    try {
      final next = await _backend.observeChangesCheckpoint(
        packageId!,
        _digest!,
        _revision!,
        selection!,
      );
      if (!active || generation != _generation) return;
      // A busy owner has no new evidence for this selection.
      progress = next;
      _next = Timer(interval, () => unawaited(check()));
    } catch (error) {
      if (!active || generation != _generation) return;
      failure = error;
      _end();
      return;
    } finally {
      if (generation == _generation) {
        _read = null;
        notifyListeners();
      }
    }
  }

  void _end() {
    _generation++;
    _next?.cancel();
    _expiry?.cancel();
    active = false;
    _read = null;
    progress = null;
    notifyListeners();
  }

  void stop() {
    expired = false;
    failure = null;
    selection = null;
    packageId = null;
    _digest = null;
    _revision = null;
    _end();
  }
}
