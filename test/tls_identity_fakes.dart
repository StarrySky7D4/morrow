import 'dart:async';
import 'dart:typed_data';
import 'package:morrow_studio/plugins/service_run_control.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
ServiceTlsIdentityInfo row(
  int key, {
  int revision = 1,
  bool disabled = false,
}) => ServiceTlsIdentityInfo(
  choice: ServiceTlsIdentityChoice(
    reference: id(key),
    revision: BigInt.from(revision),
    certificateSha256: id(31),
  ),
  disabled: disabled,
);
ServiceTlsIdentityPage page(
  List<ServiceTlsIdentityInfo> rows, {
  Uint8List? next,
  Uint8List? snapshot,
}) => ServiceTlsIdentityPage(
  identities: rows,
  snapshot: snapshot ?? id(99),
  next: next,
);

class Backend
    implements WorkbenchTlsIdentityControl, WorkbenchServiceTlsControl {
  List<ServiceTlsIdentityInfo> rows = [];
  int reads = 0, saves = 0, disables = 0;
  Completer<ServiceTlsIdentityInfo>? pending;
  Future<ServiceTlsIdentityPage> Function(Uint8List?, Uint8List?)? onPage;
  @override
  Future<ServiceTlsIdentityPage> tlsIdentityPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    reads++;
    return onPage == null ? page(rows) : onPage!(after, snapshot);
  }

  @override
  Future<ServiceTlsSelection> inspectServiceTls({
    required String certificatePath,
    required String privateKeyPath,
  }) async => ServiceTlsSelection(
    certificatePath: certificatePath,
    privateKeyPath: privateKeyPath,
    certificateSha256: id(31),
    validity: const ServiceTlsValidity(
      notBeforeSeconds: 0,
      notAfterSeconds: 253402300799,
    ),
  );
  @override
  Future<ServiceTlsIdentityInfo> saveTlsIdentity({
    required ServiceTlsSelection selection,
    required Uint8List reference,
    required BigInt expectedRevision,
  }) async {
    saves++;
    if (pending != null) return pending!.future;
    final result = ServiceTlsIdentityInfo(
      choice: ServiceTlsIdentityChoice(
        reference: reference.isEmpty ? id(rows.length + 1) : reference,
        revision: expectedRevision + BigInt.one,
        certificateSha256: selection.certificateSha256,
      ),
      disabled: false,
    );
    rows = [...rows.where((r) => r.choice.key != result.choice.key), result]
      ..sort((a, b) => a.choice.key.compareTo(b.choice.key));
    return result;
  }

  @override
  Future<ServiceTlsIdentityInfo> disableTlsIdentity(
    ServiceTlsIdentityInfo expected,
  ) async {
    disables++;
    final result = row(
      expected.choice.reference.first,
      revision: expected.choice.revision.toInt() + 1,
      disabled: true,
    );
    rows = [...rows.where((r) => r.choice.key != result.choice.key), result]
      ..sort((a, b) => a.choice.key.compareTo(b.choice.key));
    return result;
  }
}
