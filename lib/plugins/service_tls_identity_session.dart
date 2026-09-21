import 'package:flutter/foundation.dart';
import 'service_run_control.dart';
import 'service_tls_draft.dart';

/// Owns pending management operations across unmounts. Never retries a write.
class TlsIdentitySession extends ChangeNotifier {
  TlsIdentitySession(this.backend);
  static final _sessions = Expando<TlsIdentitySession>(
    'TLS identity management',
  );
  factory TlsIdentitySession.forBackend(WorkbenchTlsIdentityControl backend) =>
      _sessions[backend] ??= TlsIdentitySession(backend);
  final WorkbenchTlsIdentityControl backend;
  final draft = ServiceTlsDraft();
  List<ServiceTlsIdentityInfo> records = const [];
  bool busy = false, trusted = false, uncertain = false, disposed = false;
  bool useSaved = false;
  String? notice;
  ServiceCommandFailure? failure;
  ServiceTlsIdentityChoice? selected;
  ServiceTlsIdentityInfo? receipt;
  bool get canWrite => !disposed && trusted && !busy && !uncertain;
  bool current(ServiceTlsIdentityChoice choice) =>
      trusted &&
      records.any(
        (r) =>
            !r.disabled &&
            r.choice.key == choice.key &&
            r.choice.revision == choice.revision &&
            listEquals(r.choice.certificateSha256, choice.certificateSha256),
      );
  bool get canUse =>
      canWrite &&
      (useSaved
          ? selected != null && current(selected!)
          : draft.accepted &&
                draft.checked?.validity?.validAt(DateTime.now()) == true);
  void changed() {
    if (!disposed) notifyListeners();
  }

  void select(ServiceTlsIdentityChoice choice) {
    if (!canWrite || !current(choice)) return;
    selected = ServiceTlsIdentityChoice(
      reference: choice.reference,
      revision: choice.revision,
      certificateSha256: choice.certificateSha256,
    );
    useSaved = true;
    changed();
  }

  void useFile() {
    if (canWrite) {
      useSaved = false;
      changed();
    }
  }

  void acknowledgeUncertain() {
    if (disposed || busy || !trusted || !uncertain) return;
    uncertain = false;
    notice = null;
    // Retain the original failure identity for diagnostics; no command replay.
    changed();
  }

  void _require(bool condition) {
    if (!condition) throw const FormatException('TLS identity changed');
  }

  Future<void> save({ServiceTlsIdentityInfo? previous}) async {
    if (!canWrite) return;
    final selectedFile = draft.checked;
    if (!draft.accepted ||
        selectedFile?.validity?.validAt(DateTime.now()) != true) {
      notice = 'invalid';
      changed();
      return;
    }
    final reference = previous?.choice.reference ?? Uint8List(0);
    final revision = previous?.choice.revision ?? BigInt.zero;
    if (previous != null &&
        !records.any(
          (r) =>
              r.choice.key == previous.choice.key &&
              r.choice.revision == revision &&
              listEquals(
                r.choice.certificateSha256,
                previous.choice.certificateSha256,
              ),
        )) {
      notice = 'invalid';
      changed();
      return;
    }
    await _write(() async {
      final result = await backend.saveTlsIdentity(
        selection: selectedFile!,
        reference: reference,
        expectedRevision: revision,
      );
      ServiceRunValidation.tlsIdentity(result.choice);
      _require(
        !result.disabled &&
            result.choice.revision == revision + BigInt.one &&
            listEquals(
              result.choice.certificateSha256,
              selectedFile.certificateSha256,
            ) &&
            (previous == null
                ? !records.any((r) => r.choice.key == result.choice.key)
                : result.choice.key == previous.choice.key),
      );
      return result;
    });
  }

  Future<void> disable(ServiceTlsIdentityInfo previous) async {
    if (!canWrite || previous.disabled || !current(previous.choice)) return;
    await _write(() async {
      final result = await backend.disableTlsIdentity(previous);
      ServiceRunValidation.tlsIdentity(result.choice);
      _require(
        result.disabled &&
            result.choice.key == previous.choice.key &&
            result.choice.revision == previous.choice.revision + BigInt.one &&
            listEquals(
              result.choice.certificateSha256,
              previous.choice.certificateSha256,
            ),
      );
      return result;
    });
  }

  Future<void> _write(
    Future<ServiceTlsIdentityInfo> Function() operation,
  ) async {
    busy = true;
    trusted = false;
    notice = null;
    changed();
    var succeeded = false;
    try {
      receipt = await operation();
      notice = 'saved';
      succeeded = true;
    } catch (error) {
      uncertain = true;
      failure = error is ServiceCommandFailure ? error : null;
      notice = 'writeUnknown';
    } finally {
      busy = false;
      changed();
    }
    if (succeeded && !disposed) await refresh();
  }

  Future<void> refresh() async {
    if (busy || disposed) return;
    busy = true;
    trusted = false;
    changed();
    try {
      Uint8List? after;
      Uint8List? snapshot;
      final rows = <ServiceTlsIdentityInfo>[];
      String? lastRefHex;
      while (rows.length < 128) {
        final page = await backend.tlsIdentityPage(
          after: after,
          snapshot: snapshot,
        );
        if (disposed) return;
        ServiceRunValidation.identity(page.snapshot);
        if (page.identities.length > 16) {
          throw const FormatException('pageOverflow');
        }
        if (snapshot == null) {
          snapshot = page.snapshot;
        } else if (!listEquals(snapshot, page.snapshot)) {
          throw const FormatException('snapshotChanged');
        }
        if (page.identities.isEmpty) {
          if (page.next != null) {
            throw const FormatException('emptyPageWithNext');
          }
          break;
        }
        for (final info in page.identities) {
          if (rows.length >= 128) {
            throw const FormatException('tooManyRows');
          }
          final reference = ServiceRunValidation.identity(
            info.choice.reference,
          );
          final refHex = reference
              .map((b) => b.toRadixString(16).padLeft(2, '0'))
              .join();
          if (lastRefHex != null && refHex.compareTo(lastRefHex) <= 0) {
            throw const FormatException('referenceOrder');
          }
          // Validates choice structure; throws FormatException on invalid.
          ServiceRunValidation.tlsIdentity(info.choice);
          lastRefHex = refHex;
          rows.add(info);
        }
        final hasFull = page.identities.length == 16;
        if (page.next != null) {
          if (!hasFull || rows.length >= 128) {
            throw const FormatException('unexpectedNext');
          }
          final expected = rows.last.choice.reference;
          if (!listEquals(page.next, expected)) {
            throw const FormatException('nextMismatch');
          }
          after = page.next;
        } else {
          break;
        }
      }
      if (!disposed) {
        records = List.unmodifiable(rows);
        trusted = true;
        if (notice == 'loadFailed') notice = null;
      }
    } catch (_) {
      if (!disposed) {
        notice = 'loadFailed';
        changed();
      }
    } finally {
      busy = false;
      changed();
    }
  }

  @override
  void dispose() {
    disposed = true;
    super.dispose();
  }
}
