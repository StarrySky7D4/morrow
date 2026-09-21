import 'dart:typed_data';

class ServiceTlsIdentityChoice {
  final Uint8List reference;
  final BigInt revision;
  final Uint8List certificateSha256;

  ServiceTlsIdentityChoice({
    required Uint8List reference,
    required this.revision,
    required Uint8List certificateSha256,
  }) : reference = Uint8List.fromList(reference).asUnmodifiableView(),
       certificateSha256 = Uint8List.fromList(
         certificateSha256,
       ).asUnmodifiableView();

  String get key =>
      reference.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
}

class ServiceTlsIdentityInfo {
  final ServiceTlsIdentityChoice choice;
  final bool disabled;

  ServiceTlsIdentityInfo({
    required ServiceTlsIdentityChoice choice,
    required this.disabled,
  }) : choice = ServiceTlsIdentityChoice(
         reference: choice.reference,
         revision: choice.revision,
         certificateSha256: choice.certificateSha256,
       );
}

class ServiceTlsIdentityPage {
  final List<ServiceTlsIdentityInfo> identities;
  final Uint8List snapshot;
  final Uint8List? next;

  ServiceTlsIdentityPage({
    required List<ServiceTlsIdentityInfo> identities,
    required Uint8List snapshot,
    Uint8List? next,
  }) : identities = List.unmodifiable(identities),
       snapshot = Uint8List.fromList(snapshot).asUnmodifiableView(),
       next = next == null
           ? null
           : Uint8List.fromList(next).asUnmodifiableView();
}
