import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/capture.capnp.dart' as capture;
import 'package:morrow_studio/plugins/generated/content_api.capnp.dart'
    as content;
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as draft;
import 'package:morrow_studio/plugins/generated/editor_draft_staging_api.capnp.dart'
    as staging;
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/studio.capnp.dart' as studio;
import 'package:morrow_studio/plugins/generated/tasks.capnp.dart' as tasks;
import 'package:morrow_studio/plugins/generated/workbench.capnp.dart'
    as workbench;

void main() {
  test('Float64 wire bytes preserve sign, fractions and nonzero defaults', () {
    for (final value in [0.0, -0.0, 0.76, -12.5, 1.0e100, double.infinity]) {
      final message = MessageBuilder();
      final appearance = message.initRoot(studio.appearanceFactory);
      appearance.opacity = value;
      appearance.componentBlur = value; // Wire default is 22, not zero.
      final bytes = message.serialize();
      final data = ByteData.sublistView(bytes);
      final expected = ByteData(8)..setFloat64(0, value, Endian.little);
      final mask = ByteData(8)..setFloat64(0, 22, Endian.little);
      for (var word = 0; word < 8; word += 4) {
        expect(data.getUint32(16 + 8 + word, Endian.little),
            expected.getUint32(word, Endian.little));
        expect(data.getUint32(16 + 72 + word, Endian.little),
            (expected.getUint32(word, Endian.little) ^ mask.getUint32(word, Endian.little)).toUnsigned(32));
      }
      final read = MessageReader.deserialize(bytes).getRoot(studio.appearanceFactory);
      expect(read.opacity, value);
      expect(read.componentBlur, value);
      expect(read.opacity.isNegative, value.isNegative);
      expect(read.componentBlur.isNegative, value.isNegative);
    }
    final defaults = MessageBuilder().initRoot(studio.appearanceFactory).asReader();
    expect(defaults.componentBlur, 22.0);
    expect(defaults.componentOpacity, 0.76);
  });
  for (final literal in [
    '0',
    '4294967295',
    '9007199254740991',
    '9007199254740993',
    '9223372036854775807',
    '9223372036854775808',
    '18446744073709551615',
  ]) {
    test('unsigned 64-bit $literal preserves exact bytes', () {
      final expected = BigInt.parse(literal);
      final message = MessageBuilder();
      final request = message.initRoot(host.requestFactory);
      request.revisionBigInt = expected;
      final bytes = message.serialize();
      // One segment: segment header, root pointer, then revision at data + 8.
      final data = ByteData.sublistView(bytes);
      expect(
        data.getUint32(24, Endian.little),
        (expected & BigInt.from(0xffffffff)).toInt(),
      );
      expect(data.getUint32(28, Endian.little), (expected >> 32).toInt());
      final read = MessageReader.deserialize(
        bytes,
      ).getRoot(host.requestFactory);
      expect(read.revisionBigInt, expected);
      if (kIsWeb &&
          expected.toSigned(64).abs() > BigInt.from(9007199254740991)) {
        expect(() => read.revision, throwsRangeError);
      }
    });
  }
  test('unsigned decode accepts independently supplied high and low words', () {
    final message = MessageBuilder();
    message.initRoot(host.requestFactory);
    final bytes = message.serialize();
    final data = ByteData.sublistView(bytes);
    data.setUint32(24, 1, Endian.little);
    data.setUint32(28, 0x200000, Endian.little);
    expect(
      MessageReader.deserialize(
        bytes,
      ).getRoot(host.requestFactory).revisionBigInt,
      BigInt.parse('9007199254740993'),
    );
  });
  test('signed 64-bit and rejected writes retain their exact values', () {
    final message = MessageBuilder();
    final value = message.initRoot(studio.serviceRequestFactory);
    for (final expected in [
      -(BigInt.one << 63),
      BigInt.from(-1),
      (BigInt.one << 63) - BigInt.one,
    ]) {
      value.valueBigInt = expected;
      expect(
        MessageReader.deserialize(
          message.serialize(),
        ).getRoot(studio.serviceRequestFactory).valueBigInt,
        expected,
      );
    }
    expect(() => value.valueBigInt = BigInt.one << 63, throwsRangeError);
    final request = MessageBuilder().initRoot(host.requestFactory);
    request.revisionBigInt = BigInt.one;
    expect(() => request.revisionBigInt = BigInt.from(-1), throwsRangeError);
    expect(() => request.revisionBigInt = BigInt.one << 64, throwsRangeError);
    expect(request.asReader().revisionBigInt, BigInt.one);
    if (kIsWeb) {
      expect(() => request.revision = 9007199254740992, throwsRangeError);
    }
  });
  test('all workbench bindings select platform-compatible reflection', () {
    for (final available in [
      capture.schemaReflectionAvailable,
      content.schemaReflectionAvailable,
      draft.schemaReflectionAvailable,
      staging.schemaReflectionAvailable,
      host.schemaReflectionAvailable,
      studio.schemaReflectionAvailable,
      tasks.schemaReflectionAvailable,
      workbench.schemaReflectionAvailable,
    ]) {
      expect(available, !kIsWeb);
    }
    expect(
      draft.schemaId('resultKindSchema'),
      BigInt.parse('f57686d6980b2b61', radix: 16),
    );
    expect(
      staging.schemaId('phaseSchema'),
      BigInt.parse('f0dc3510d8debea5', radix: 16),
    );
    expect(
      host.schemaId('actionSchema'),
      BigInt.parse('8f969b341ddab10b', radix: 16),
    );
  });

  for (final selection in [-2147483648, -1, 0, 2147483647]) {
    test('draft text and signed selection $selection survive the wire', () {
      final message = MessageBuilder();
      final value = message.initRoot(draft.textValueFactory);
      value.text = '草稿😀';
      value.selectionBase = selection;
      value.selectionExtent = 4;
      value.affinity = 1;
      value.directional = true;
      value.composingStart = -1;
      value.composingEnd = -1;

      final result = MessageReader.deserialize(
        message.serialize(),
      ).getRoot(draft.textValueFactory);
      expect(result.text, '草稿😀');
      expect(result.selectionBase, selection);
      expect(result.selectionExtent, 4);
      expect(result.affinity, 1);
      expect(result.directional, isTrue);
      expect(result.composingStart, -1);
      expect(result.composingEnd, -1);
    });
  }
}
