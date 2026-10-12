import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_task_discovery.dart';

Uint8List hex(String s) => Uint8List.fromList([
  for (var i = 0; i < s.length; i += 2) int.parse(s.substring(i, i + 2), radix: 16),
]);

void main() {
  final root = Platform.environment['MORROW_AGENT_DISCOVERY_FIXTURES'];
  if (root == null || !Directory(root).isAbsolute) {
    throw StateError('Explicit original Rust discovery fixture directory required');
  }
  final vectors = jsonDecode(File('$root/vectors.json').readAsStringSync()) as Map<String, dynamic>;
  final id = hex(vectors['requestIdHex'] as String);
  final request = hex(vectors['requestHex'] as String);
  final rows = (vectors['replies'] as List).cast<Map<String, dynamic>>();
  final codec = AgentTaskDiscoveryCodec();
  Uint8List reply(String kind) => hex(rows.singleWhere((r) => r['kind'] == kind)['replyHex'] as String);

  test('Dart request is byte exact with the actual generated Rust codec', () {
    expect(vectors['schemaSha256'], '9357ac5a01dc00c22551f0d3a399fa1a2498780283f7e700b0b966bbc449ddef');
    expect(codec.encode(id), orderedEquals(request));
    expect(File('$root/request.bin').readAsBytesSync(), orderedEquals(request));
  });

  test('all five original Rust replies decode and completely re-encode canonically', () {
    expect(rows, hasLength(5));
    for (final row in rows) {
      final bytes = hex(row['replyHex'] as String);
      final value = codec.decode(id, request, bytes);
      expect(value.kind.name, row['kind']);
      expect(value.taskKey ?? <int>[], orderedEquals(hex(row['taskHex'] as String)));
      expect(bytes, orderedEquals(File('$root/${row['kind']}.reply.bin').readAsBytesSync()));
    }
  });

  test('retained Agent key has owned immutable bytes and other states have no key', () {
    final bytes = reply('runningAgent');
    final value = codec.decode(id, request, bytes);
    bytes.fillRange(0, bytes.length, 0);
    expect(value.taskKey, orderedEquals(List.filled(32, 7)));
    expect(() => value.taskKey![0] = 0, throwsUnsupportedError);
    for (final kind in [AgentTaskKind.none, AgentTaskKind.otherTask, AgentTaskKind.unavailable]) {
      expect(() => AgentTaskObservation(kind, List.filled(32, 7)), throwsFormatException);
    }
    for (final kind in [AgentTaskKind.runningAgent, AgentTaskKind.retainedAgentExit]) {
      expect(() => AgentTaskObservation(kind, null), throwsFormatException);
      expect(() => AgentTaskObservation(kind, List.filled(32, 0)), throwsFormatException);
    }
  });

  test('every changed reply byte fails framing, correlation, shape or canonical encoding', () {
    final original = reply('none');
    for (var i = 0; i < original.length; i++) {
      final changed = Uint8List.fromList(original)..[i] ^= 1;
      expect(() => codec.decode(id, request, changed), throwsA(anything), reason: 'offset $i');
    }
  });

  test('wrong request, truncated, oversized and trailing replies are rejected', () {
    final original = reply('runningAgent');
    final wrong = Uint8List.fromList(id)..[0] ^= 1;
    expect(() => codec.decode(wrong, codec.encode(wrong), original), throwsFormatException);
    expect(() => codec.decode(id, Uint8List.fromList(request)..[0] ^= 1, original), throwsFormatException);
    expect(() => codec.decode(id, request, Uint8List.sublistView(original, 0, original.length - 1)), throwsA(anything));
    expect(() => codec.decode(id, request, Uint8List(AgentTaskDiscoveryCodec.maxFrameBytes + 1)), throwsFormatException);
    expect(() => codec.decode(id, request, Uint8List.fromList([...original, ...List.filled(8, 0)])), throwsFormatException);
    expect(() => codec.encode(List.filled(32, 0)), throwsFormatException);
    expect(() => codec.encode(List.filled(32, 256)), throwsFormatException);
  });

  // Template bytes came from the real Rust codec. Only this ordinary test
  // peer's correlation fields change for each explicit random observation.
  Uint8List correlate(Uint8List current) {
    final output = reply('none');
    final currentId = Uint8List.sublistView(current, current.length - 32);
    for (final pair in [
      (id, currentId),
      (Uint8List.fromList(sha256.convert(request).bytes), Uint8List.fromList(sha256.convert(current).bytes)),
    ]) {
      var found = 0;
      for (var i = 0; i <= output.length - 32; i++) {
        if (List.generate(32, (j) => output[i + j] == pair.$1[j]).every((v) => v)) {
          output.setRange(i, i + 32, pair.$2); found++;
        }
      }
      expect(found, 1);
    }
    return output;
  }

  test('unavailable observation never dispatches or offers a fallback key', () async {
    var calls = 0;
    Future<T> exchange<T>(Uint8List bytes, T Function(Uint8List) decode) async { calls++; throw StateError('unexpected dispatch'); }
    final client = NativeAgentTaskDiscovery(exchange, () => false);
    expect(client.supportsAgentTaskDiscovery, isFalse);
    await expectLater(client.observeAgentTask(), throwsStateError);
    expect(calls, 0);
  });

  test('one explicit pending observation, no automatic polling, owned input wiped', () async {
    var calls = 0;
    final proceed = Completer<void>();
    late Uint8List sent;
    Future<T> exchange<T>(Uint8List bytes, T Function(Uint8List) decode) async {
      calls++; sent = bytes;
      await proceed.future;
      return decode(correlate(bytes));
    }
    final client = NativeAgentTaskDiscovery(exchange, () => true);
    final pending = client.observeAgentTask();
    await expectLater(client.observeAgentTask(), throwsStateError);
    expect(calls, 1);
    proceed.complete();
    expect((await pending).kind, AgentTaskKind.none);
    expect(sent.every((b) => b == 0), isTrue);
    await Future<void>.delayed(Duration.zero);
    expect(calls, 1);
  });

  test('lost read-only delivery disables this client without automatic resend', () async {
    var calls = 0;
    Future<T> exchange<T>(Uint8List bytes, T Function(Uint8List) decode) async { calls++; throw const FormatException('synthetic EOF'); }
    final client = NativeAgentTaskDiscovery(exchange, () => true);
    await expectLater(client.observeAgentTask(), throwsFormatException);
    expect(client.supportsAgentTaskDiscovery, isFalse);
    await expectLater(client.observeAgentTask(), throwsStateError);
    expect(calls, 1);
  });
}
