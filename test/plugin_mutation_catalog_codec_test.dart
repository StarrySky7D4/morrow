import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

PluginLibraryEntry decode({
  bool supported = false,
  bool budget = false,
  BigInt? maxJobBytes,
  BigInt? maxBytes,
}) {
  final message = MessageBuilder();
  final row = message.initRoot(host.pluginEntryFactory);
  row.packageId = 'org.example.catalog';
  row.name = 'Catalog fixture';
  row.packageVersion = '1.0.0';
  row.digest = Uint8List.fromList(List<int>.filled(32, 7));
  row.mutationSupported = supported;
  if (budget) {
    final declared = row.initMutationBudget();
    declared.maxJobBytesBigInt = maxJobBytes ?? BigInt.zero;
    declared.maxBytesBigInt = maxBytes ?? BigInt.zero;
  }
  return decodePluginLibraryEntry(
    MessageReader.deserialize(
      message.serialize(),
    ).getRoot(host.pluginEntryFactory),
  );
}

void main() {
  test('old catalog row defaults to no mutation extension', () {
    final entry = decode();
    expect(entry.mutationSupported, isFalse);
    expect(entry.mutationBudget, isNull);
    expect(decode(supported: true).mutationBudget, isNull);
  });

  test('declared budget needs its mutation feature', () {
    expect(
      () => decode(budget: true, maxJobBytes: BigInt.one, maxBytes: BigInt.one),
      throwsFormatException,
    );
  });

  test('budget pair rejects zero, ceilings and inverted totals', () {
    final maxJob = BigInt.from(32 * 1024 * 1024);
    final maxTotal = BigInt.from(256 * 1024 * 1024);
    for (final (job, total) in [
      (BigInt.zero, BigInt.zero),
      (BigInt.zero, BigInt.one),
      (BigInt.one, BigInt.zero),
      (maxJob + BigInt.one, maxTotal),
      (BigInt.one, maxTotal + BigInt.one),
      (BigInt.two, BigInt.one),
    ]) {
      expect(
        () => decode(
          supported: true,
          budget: true,
          maxJobBytes: job,
          maxBytes: total,
        ),
        throwsFormatException,
        reason: 'job=$job total=$total',
      );
    }
    final maximum = decode(
      supported: true,
      budget: true,
      maxJobBytes: maxJob,
      maxBytes: maxTotal,
    );
    expect(maximum.mutationBudget!.maxJobBytes, maxJob);
    expect(maximum.mutationBudget!.maxBytes, maxTotal);
  });
}
