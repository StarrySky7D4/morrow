import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'service_control.dart';

enum ServiceNotice { loadFailed, writeUnknown, invalid, saved, tokenDiscarded }

bool _same(List<int> a, List<int> b) => listEquals(a, b);
void _require(bool condition) {
  if (!condition) throw const FormatException('Service receipt changed');
}

bool _principals(List<ServicePrincipal> a, List<ServicePrincipal> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i].id != b[i].id ||
        !_same(a[i].authenticationReference, b[i].authenticationReference) ||
        a[i].scopes.length != b[i].scopes.length) {
      return false;
    }
    for (var j = 0; j < a[i].scopes.length; j++) {
      final x = a[i].scopes[j], y = b[i].scopes[j];
      if (x.kind != y.kind ||
          x.cardId != y.cardId ||
          x.attachmentId != y.attachmentId) {
        return false;
      }
    }
  }
  return true;
}

bool _policy(ServicePublication a, ServicePublication b) =>
    a.configId == b.configId &&
    _same(a.configDigest, b.configDigest) &&
    a.listenAddress == b.listenAddress &&
    a.tlsRequired == b.tlsRequired &&
    a.method == b.method &&
    a.path == b.path &&
    a.queryPath == b.queryPath;

/// Writes belong to the backend session, even while settings are unmounted.
/// No timer, automatic retry, listener, or second data store is created here.
class ServiceSession extends ChangeNotifier {
  ServiceSession(this.backend);
  static final _sessions = Expando<ServiceSession>('Service management');
  factory ServiceSession.forBackend(WorkbenchServiceControl backend) =>
      _sessions[backend] ??= ServiceSession(backend);

  final WorkbenchServiceControl backend;
  List<StoredServiceConfig> configs = const [];
  List<StoredServiceAuthority> authorities = const [];
  bool busy = false, trusted = false, uncertain = false;
  ServiceNotice? notice;
  IssuedServiceAuthentication? issued;
  int _viewers = 0, _presentation = 0;
  bool _disposed = false;
  bool get canWrite => !_disposed && trusted && !busy && !uncertain;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  void attach() {
    if (!_disposed) _viewers++;
  }

  void detach() {
    if (_viewers > 0) _viewers--;
    if (_viewers == 0) {
      _presentation++;
      clearToken();
    }
  }

  void clearToken() {
    issued?.dispose();
    issued = null;
    _changed();
  }

  @override
  void dispose() {
    _disposed = true;
    _presentation++;
    issued?.dispose();
    issued = null;
    super.dispose();
  }

  void acknowledgeUncertain() {
    // This is acknowledgement of a fresh metadata inspection, not a retry or
    // a claim that the previous operation was rolled back.
    if (_disposed || busy || !trusted || !uncertain) return;
    uncertain = false;
    notice = null;
    _changed();
  }

  Future<void> refresh() async {
    if (_disposed || busy) return;
    busy = true;
    trusted = false;
    _changed();
    try {
      final nextConfigs = <StoredServiceConfig>[];
      String? cursor;
      Uint8List? snapshot;
      do {
        final page = await backend.serviceConfigPage(
          after: cursor,
          snapshot: snapshot,
        );
        if (_disposed) return;
        ServiceValidation.digest(page.snapshot);
        _require(snapshot == null || _same(snapshot, page.snapshot));
        snapshot ??= page.snapshot;
        _require(page.configs.length <= 1);
        for (final c in page.configs) {
          ServiceValidation.storedConfig(c);
          _require(
            cursor == null ||
                ServiceValidation.compareBytes(
                      utf8.encode(cursor),
                      utf8.encode(c.id),
                    ) <
                    0,
          );
          nextConfigs.add(c);
        }
        _require(nextConfigs.length <= 128);
        _require(
          page.next == null ||
              (page.configs.length == 1 &&
                  page.next == page.configs.single.id &&
                  nextConfigs.length < 128),
        );
        cursor = page.next;
      } while (cursor != null);

      final nextAuthorities = <StoredServiceAuthority>[];
      Uint8List? after;
      snapshot = null;
      do {
        final page = await backend.serviceAuthorityPage(
          after: after,
          snapshot: snapshot,
        );
        if (_disposed) return;
        ServiceValidation.digest(page.snapshot);
        _require(snapshot == null || _same(snapshot, page.snapshot));
        snapshot ??= page.snapshot;
        _require(page.records.length <= 2);
        for (final a in page.records) {
          ServiceValidation.authority(a);
          _require(
            nextAuthorities.isEmpty ||
                ServiceValidation.compareBytes(
                      nextAuthorities.last.reference,
                      a.reference,
                    ) <
                    0,
          );
          nextAuthorities.add(a);
        }
        _require(nextAuthorities.length <= 512);
        _require(
          page.next == null ||
              (page.records.isNotEmpty &&
                  _same(page.next!, page.records.last.reference) &&
                  nextAuthorities.length < 512),
        );
        after = page.next;
      } while (after != null);
      configs = List.unmodifiable(nextConfigs);
      authorities = List.unmodifiable(nextAuthorities);
      trusted = true;
      if (uncertain) {
        notice = ServiceNotice.writeUnknown;
      } else if (notice == ServiceNotice.loadFailed) {
        notice = null;
      }
    } catch (_) {
      trusted = false;
      notice = uncertain
          ? ServiceNotice.writeUnknown
          : ServiceNotice.loadFailed;
    } finally {
      busy = false;
      _changed();
    }
  }

  Future<bool> _write(
    void Function() validate,
    Future<void> Function() send,
  ) async {
    if (!canWrite) return false;
    try {
      validate();
    } catch (_) {
      notice = ServiceNotice.invalid;
      _changed();
      return false;
    }
    busy = true;
    notice = null;
    _changed();
    var success = false;
    try {
      await send();
      success = true;
      if (notice != ServiceNotice.tokenDiscarded) notice = ServiceNotice.saved;
    } catch (_) {
      uncertain = true;
      notice = ServiceNotice.writeUnknown;
    } finally {
      trusted = false;
      busy = false;
      _changed();
    }
    // Observation never repeats the mutation, including after a lost receipt.
    if (!_disposed) await refresh();
    return success;
  }

  Future<bool> saveConfig(ServiceConfigUpdate request) {
    StoredServiceConfig? old;
    return _write(
      () {
        ServiceValidation.configUpdate(request);
        if (request.id.isNotEmpty) {
          old = configs.where((c) => c.id == request.id).firstOrNull;
          _require(old != null && old!.revision == request.expectedRevision);
        }
      },
      () async {
        final result = await backend.saveServiceConfig(request);
        ServiceValidation.storedConfig(result);
        _require(
          result.revision == request.expectedRevision + BigInt.one &&
              !result.disabled &&
              result.service == request.service &&
              result.handler == request.handler &&
              result.retentionMs == request.retentionMs &&
              _same(result.packageDigest, request.packageDigest) &&
              _principals(result.principals, request.principals),
        );
        if (old case final previous?) {
          _require(
            result.id == previous.id &&
                _same(result.namespace, previous.namespace) &&
                result.approvalReferences.length ==
                    previous.approvalReferences.length,
          );
          for (var i = 0; i < previous.approvalReferences.length; i++) {
            _require(
              _same(
                result.approvalReferences[i],
                previous.approvalReferences[i],
              ),
            );
          }
        } else {
          _require(
            !configs.any((c) => c.id == result.id) &&
                result.approvalReferences.length == 1,
          );
        }
      },
    );
  }

  Future<bool> disableConfig(StoredServiceConfig previous) => _write(
    () {
      ServiceValidation.storedConfig(previous);
      _require(
        configs.any(
          (c) => c.id == previous.id && c.revision == previous.revision,
        ),
      );
      ServiceValidation.revision(
        previous.revision,
        updating: !previous.disabled,
      );
    },
    () async {
      final result = await backend.disableServiceConfig(
        previous.id,
        previous.revision,
      );
      ServiceValidation.storedConfig(result);
      _require(
        result.id == previous.id &&
            result.revision ==
                previous.revision +
                    (previous.disabled ? BigInt.zero : BigInt.one) &&
            result.disabled &&
            _same(result.namespace, previous.namespace) &&
            result.service == previous.service &&
            result.handler == previous.handler &&
            _same(result.packageDigest, previous.packageDigest) &&
            result.retentionMs == previous.retentionMs &&
            _principals(result.principals, previous.principals) &&
            result.approvalReferences.length ==
                previous.approvalReferences.length,
      );
      for (var i = 0; i < previous.approvalReferences.length; i++) {
        _require(
          _same(result.approvalReferences[i], previous.approvalReferences[i]),
        );
      }
    },
  );

  Future<bool> issueAuthentication({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String principalId,
    required int lifetimeDays,
  }) {
    final ownedReference = Uint8List.fromList(reference);
    final presentation = _presentation;
    return _write(
      () {
        ServiceValidation.referenceRevision(
          ownedReference,
          expectedRevision,
          create: true,
        );
        ServiceValidation.revision(
          expectedRevision,
          zero: true,
          updating: true,
        );
        ServiceValidation.principalId(principalId);
        ServiceValidation.days(lifetimeDays);
        if (ownedReference.isNotEmpty) {
          _require(
            authorities.any(
              (a) =>
                  _same(a.reference, ownedReference) &&
                  a.revision == expectedRevision &&
                  a.kind == 1 &&
                  a.principalId == principalId,
            ),
          );
        }
      },
      () async {
        clearToken();
        final result = await backend.issueServiceAuthentication(
          reference: ownedReference,
          expectedRevision: expectedRevision,
          principalId: principalId,
          lifetimeDays: lifetimeDays,
        );
        var retained = false;
        try {
          final a = result.authority;
          ServiceValidation.authority(a);
          _require(
            a.kind == 1 &&
                !a.disabled &&
                a.principalId == principalId &&
                a.revision == expectedRevision + BigInt.one &&
                a.expiresMs - a.createdMs ==
                    BigInt.from(lifetimeDays * 86400000) &&
                !result.token.isDisposed &&
                result.token.bytes.length == 64 &&
                result.token.bytes.every(
                  (v) => v >= 48 && v <= 57 || v >= 97 && v <= 102,
                ),
          );
          _require(
            ownedReference.isEmpty
                ? !authorities.any((v) => _same(v.reference, a.reference))
                : _same(a.reference, ownedReference),
          );
          if (!_disposed && _viewers > 0 && presentation == _presentation) {
            issued = result;
            retained = true;
          } else {
            notice = ServiceNotice.tokenDiscarded;
          }
        } finally {
          if (!retained) result.dispose();
        }
      },
    );
  }

  Future<bool> disableAuthority(StoredServiceAuthority previous) => _write(
    () {
      ServiceValidation.authority(previous);
      ServiceValidation.revision(
        previous.revision,
        updating: !previous.disabled,
      );
      _require(
        authorities.any(
          (a) =>
              _same(a.reference, previous.reference) &&
              a.revision == previous.revision,
        ),
      );
    },
    () async {
      final result = await backend.disableServiceAuthority(
        previous.reference,
        previous.revision,
      );
      ServiceValidation.authority(result);
      _require(
        _same(result.reference, previous.reference) &&
            result.revision ==
                previous.revision +
                    (previous.disabled ? BigInt.zero : BigInt.one) &&
            result.disabled &&
            result.kind == previous.kind &&
            result.createdMs == previous.createdMs &&
            result.expiresMs == previous.expiresMs &&
            result.principalId == previous.principalId &&
            (previous.publication == null
                ? result.publication == null
                : result.publication != null &&
                      _policy(result.publication!, previous.publication!)),
      );
      if (issued != null &&
          _same(issued!.authority.reference, previous.reference)) {
        clearToken();
      }
    },
  );

  Future<bool> savePublication(ServicePublicationUpdate request) => _write(
    () {
      ServiceValidation.publicationUpdate(request);
      _require(
        const [
          'GET',
          'HEAD',
          'POST',
          'PUT',
          'PATCH',
          'DELETE',
          'OPTIONS',
        ].contains(request.policy.method),
      );
      _require(
        configs.any(
          (c) =>
              c.id == request.policy.configId &&
              !c.disabled &&
              c.revision == request.configRevision &&
              _same(c.digest, request.policy.configDigest) &&
              c.approvalReferences.any((r) => _same(r, request.reference)),
        ),
      );
      final previous = authorities
          .where((a) => _same(a.reference, request.reference))
          .firstOrNull;
      _require(
        previous == null
            ? request.expectedRevision == BigInt.zero
            : previous.kind == 2 &&
                  previous.revision == request.expectedRevision,
      );
    },
    () async {
      final result = await backend.saveServicePublication(request);
      ServiceValidation.authority(result);
      _require(
        result.kind == 2 &&
            !result.disabled &&
            _same(result.reference, request.reference) &&
            result.revision == request.expectedRevision + BigInt.one &&
            result.publication != null &&
            _policy(result.publication!, request.policy) &&
            result.expiresMs - result.createdMs <=
                BigInt.from(request.lifetimeDays * 86400000),
      );
    },
  );
}
