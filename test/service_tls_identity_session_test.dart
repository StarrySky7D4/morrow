import 'dart:async';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_tls_identity_session.dart';
import 'tls_identity_fakes.dart';

void main() {
  test('accepts exact final page with null next', () async {
    final b = Backend();
    b.onPage = (after, snapshot) async {
      expect(after, isNull);
      return page(List.generate(16, (i) => row(i + 1)));
    };
    final s = TlsIdentitySession(b);
    await s.refresh();
    expect(b.reads, 1);
    expect(s.trusted, isTrue);
    expect(s.records, hasLength(16));
    expect(s.selected, isNull);
    s.dispose();
  });

  test('rejects malformed pages', () async {
    final cases = <Backend>[];
    final first = Backend()
      ..onPage = (after, snapshot) async =>
          page(List.generate(17, (i) => row(i + 1)));
    cases.add(first);
    final zero = Backend()
      ..onPage = (after, snapshot) async =>
          page([row(1)], snapshot: Uint8List(32));
    cases.add(zero);
    final emptyNext = Backend()
      ..onPage = (after, snapshot) async => page(const [], next: id(7));
    cases.add(emptyNext);
    for (final b in cases) {
      final s = TlsIdentitySession(b);
      await s.refresh();
      expect(s.trusted, isFalse);
      expect(s.selected, isNull);
      expect(b.rows, isEmpty);
      s.dispose();
    }
  });

  test('refresh preserves stale selection until explicit reselect', () async {
    final b = Backend();
    final original = row(1, revision: 1);
    b.onPage = (after, snapshot) async => page([original]);
    final s = TlsIdentitySession(b);
    await s.refresh();
    expect(b.reads, 1);
    s.select(original.choice);
    expect(s.canUse, isTrue);

    final updated = row(1, revision: 2);
    b.onPage = (after, snapshot) async => page([updated]);
    await s.refresh();
    expect(b.reads, 2);
    expect(s.selected!.revision, original.choice.revision);
    expect(s.canUse, isFalse);

    s.select(updated.choice);
    expect(s.canUse, isTrue);
    expect(s.selected!.revision, updated.choice.revision);
    s.dispose();
  });

  test(
    '128 rows are bounded and a changing snapshot never publishes partial rows',
    () async {
      for (final fault in [
        'none',
        'overflow',
        'snapshot',
        'duplicate',
        'cursor',
      ]) {
        final b = Backend();
        b.onPage = (after, snapshot) async {
          final start = after == null ? 1 : after.first + 1;
          final rows = List.generate(16, (i) => row(start + i));
          if (fault == 'duplicate' && after != null) rows[0] = row(after.first);
          return page(
            rows,
            snapshot: id(fault == 'snapshot' && after != null ? 98 : 99),
            next: start == 113 && fault != 'overflow'
                ? null
                : id(fault == 'cursor' ? 3 : start + 15),
          );
        };
        final s = TlsIdentitySession(b);
        await s.refresh();
        expect(s.trusted, fault == 'none');
        expect(s.records.length, fault == 'none' ? 128 : 0);
        expect(b.reads, lessThanOrEqualTo(8));
        s.dispose();
      }
    },
  );
  test(
    'pending and Unknown writes survive backend session reuse without retry',
    () async {
      final b = Backend()..pending = Completer<ServiceTlsIdentityInfo>();
      final s = TlsIdentitySession.forBackend(b);
      await s.refresh();
      s.draft.checked = await b.inspectServiceTls(
        certificatePath: '/cert',
        privateKeyPath: '/key',
      );
      s.draft.accepted = true;
      final write = s.save();
      expect(s.busy, isTrue);
      expect(identical(s, TlsIdentitySession.forBackend(b)), isTrue);
      await s.save();
      expect(b.saves, 1);
      b.rows = [row(1)];
      final lost = ServiceCommandFailure(
        message: 'lost',
        task: id(10),
        submission: id(11),
        command: id(12),
        outcomeUnknown: true,
      );
      b.pending!.completeError(lost);
      await write;
      expect(s.uncertain, isTrue);
      expect(s.failure, same(lost));
      await s.refresh();
      await s.save();
      expect(b.saves, 1);
      expect(s.canWrite, isFalse);
      s.acknowledgeUncertain();
      expect(s.canWrite, isTrue);
      expect(s.records, hasLength(1));
      expect(b.saves, 1);
      expect(s.draft.checked!.certificatePath, '/cert');
      s.dispose();
    },
  );
  test(
    'replacement and disable leave original selection stale until explicit choice',
    () async {
      final b = Backend()..rows = [row(1)];
      final s = TlsIdentitySession(b);
      await s.refresh();
      s.select(b.rows.single.choice);
      s.draft.checked = await b.inspectServiceTls(
        certificatePath: '/newcert',
        privateKeyPath: '/newkey',
      );
      s.draft.accepted = true;
      await s.save(previous: b.rows.single);
      expect(b.saves, 1);
      expect(s.selected!.revision, BigInt.one);
      expect(s.canUse, isFalse);
      s.select(s.records.single.choice);
      expect(s.canUse, isTrue);
      await s.disable(s.records.single);
      expect(b.disables, 1);
      expect(s.canUse, isFalse);
      expect(s.records.single.disabled, isTrue);
      s.dispose();
    },
  );
}
