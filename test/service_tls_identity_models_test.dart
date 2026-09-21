import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';

void main() {
  Uint8List bytes(int n) => Uint8List.fromList(List.filled(n, 1));

  test('choice owns reference copy', () {
    final ref = bytes(4);
    final digest = bytes(4);
    final c = ServiceTlsIdentityChoice(
      reference: ref,
      revision: BigInt.from(1),
      certificateSha256: digest,
    );
    ref[0] = 9;
    digest[0] = 9;
    expect(c.reference[0], 1);
    expect(c.certificateSha256[0], 1);
  });

  test('choice arrays immutable', () {
    final c = ServiceTlsIdentityChoice(
      reference: bytes(4),
      revision: BigInt.from(1),
      certificateSha256: bytes(4),
    );
    expect(() => c.reference[0] = 2, throwsUnsupportedError);
    expect(() => c.certificateSha256[0] = 2, throwsUnsupportedError);
  });

  test('info owns choice and disabled', () {
    final ref = bytes(3);
    final choice = ServiceTlsIdentityChoice(
      reference: ref,
      revision: BigInt.from(2),
      certificateSha256: bytes(3),
    );
    final info = ServiceTlsIdentityInfo(choice: choice, disabled: false);
    ref[0] = 7;
    expect(info.choice.reference[0], 1);
    expect(info.disabled, isFalse);
  });

  test('page owns snapshot and identities', () {
    final snap = bytes(32);
    final next = bytes(32);
    final list = <ServiceTlsIdentityInfo>[];
    final page = ServiceTlsIdentityPage(
      identities: list,
      snapshot: snap,
      next: next,
    );
    snap[0] = 8;
    next[0] = 8;
    list.add(
      ServiceTlsIdentityInfo(
        choice: ServiceTlsIdentityChoice(
          reference: bytes(1),
          revision: BigInt.zero,
          certificateSha256: bytes(1),
        ),
        disabled: true,
      ),
    );
    expect(page.snapshot[0], 1);
    expect(page.next![0], 1);
    expect(page.identities, isEmpty);
  });

  test('page arrays and list immutable', () {
    final page = ServiceTlsIdentityPage(
      identities: const <ServiceTlsIdentityInfo>[],
      snapshot: bytes(1),
      next: bytes(1),
    );
    expect(() => page.snapshot[0] = 4, throwsUnsupportedError);
    expect(() => page.next![0] = 4, throwsUnsupportedError);
    expect(() => page.identities.clear(), throwsUnsupportedError);
  });

  test(
    'tlsIdentity validation rejects malformed references digests and revisions',
    () {
      ServiceTlsIdentityChoice choice({
        Uint8List? ref,
        Uint8List? digest,
        BigInt? revision,
      }) => ServiceTlsIdentityChoice(
        reference: ref ?? bytes(32),
        certificateSha256: digest ?? bytes(32),
        revision: revision ?? BigInt.one,
      );
      expect(() => ServiceRunValidation.tlsIdentity(choice()), returnsNormally);
      for (final invalid in [
        choice(ref: Uint8List(32)),
        choice(ref: bytes(31)),
        choice(digest: Uint8List(32)),
        choice(digest: bytes(31)),
        choice(revision: BigInt.zero),
        choice(revision: BigInt.one << 63),
      ]) {
        expect(
          () => ServiceRunValidation.tlsIdentity(invalid),
          throwsFormatException,
        );
      }
    },
  );
}
