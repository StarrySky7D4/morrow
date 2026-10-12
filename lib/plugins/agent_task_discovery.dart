import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';

/// A bounded observation of the original host slot, never an execution grant.
enum AgentTaskKind { none, runningAgent, retainedAgentExit, otherTask, unavailable }

final class AgentTaskObservation {
  AgentTaskObservation(this.kind, List<int>? taskKey)
      : taskKey = taskKey == null ? null : _bytes(taskKey).asUnmodifiableView() {
    final keyed = kind == AgentTaskKind.runningAgent ||
        kind == AgentTaskKind.retainedAgentExit;
    if (keyed != (this.taskKey != null) ||
        this.taskKey != null &&
            (this.taskKey!.length != 32 || this.taskKey!.every((v) => v == 0))) {
      throw const FormatException('Invalid Agent observation');
    }
  }

  /// runningAgent means the original worker is retained. It does not prove
  /// OS execution, liveness, join, successful completion or Attach eligibility.
  final AgentTaskKind kind;
  final Uint8List? taskKey;
}

abstract interface class AgentTaskDiscovery {
  bool get supportsAgentTaskDiscovery;
  Future<AgentTaskObservation> observeAgentTask();
}

typedef AgentTaskObservationExchange =
    Future<T> Function<T>(Uint8List frame, T Function(Uint8List reply) decode);

/// The original Workbench constructs this with its fixed codec and queue.
/// Observation is explicit: no timer, stored authority, Attach, Start or retry.
final class NativeAgentTaskDiscovery implements AgentTaskDiscovery {
  NativeAgentTaskDiscovery(this._exchange, this._supported);
  final AgentTaskObservationExchange _exchange;
  final bool Function() _supported;
  final AgentTaskDiscoveryCodec _codec = AgentTaskDiscoveryCodec();
  bool _busy = false, _healthy = true;

  @override
  bool get supportsAgentTaskDiscovery => _healthy && _supported();

  @override
  Future<AgentTaskObservation> observeAgentTask() async {
    if (!supportsAgentTaskDiscovery) {
      throw StateError('Agent observation unavailable');
    }
    if (_busy) throw StateError('Agent observation already pending');
    final random = Random.secure();
    final id = Uint8List.fromList(List.generate(32, (_) => random.nextInt(256)));
    final frame = _codec.encode(id);
    _busy = true;
    try {
      return await _exchange(frame, (reply) => _codec.decode(id, frame, reply));
    } catch (_) {
      // A failed observation supplies no fallback key and is never retried.
      // This read-only client does not mutate the session controller's Unknown.
      _healthy = false;
      rethrow;
    } finally {
      _busy = false;
      frame.fillRange(0, frame.length, 0);
      id.fillRange(0, id.length, 0);
    }
  }
}

Uint8List _bytes(List<int> input) {
  if (input.any((v) => v < 0 || v > 255)) {
    throw const FormatException('Invalid observation bytes');
  }
  return Uint8List.fromList(input);
}

bool _same(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  var different = 0;
  for (var i = 0; i < a.length; i++) { different |= a[i] ^ b[i]; }
  return different == 0;
}

final class _Reader extends StructReader {
  _Reader(super.raw);
}

final class _Builder extends StructBuilder {
  _Builder(super.raw);
  @override
  _Reader asReader() => _Reader(rawToReader());
}

final class _Factory extends StructFactory<_Reader, _Builder> {
  _Factory(this.dataWords, this.ptrWords);
  @override
  final int dataWords;
  @override
  final int ptrWords;
  @override
  StructSchemaInfo get schema => StructSchemaInfo(
      id: 0, displayName: 'private_agent_task_discovery', shortName: 'Observation',
      dataWords: dataWords, pointerWords: ptrWords, fields: const []);
  @override
  _Reader fromRawReader(RawStructReader raw) => _Reader(raw);
  @override
  _Builder fromRawBuilder(RawStructBuilder raw) => _Builder(raw);
}

/// Independent AD31 contract; frozen R2, host and PS30 schemas are unchanged.
/// Both messages must equal their complete canonical re-encoding, including
/// empty pointers and unused data. Qualification requires generated Rust fixtures.
final class AgentTaskDiscoveryCodec {
  static const maxFrameBytes = 16 * 1024;
  static final prefix = _bytes(ascii.encode('MROWAD31')).asUnmodifiableView();
  static final schema = _bytes([
    for (var i = 0; i < 64; i += 2)
      int.parse('9357ac5a01dc00c22551f0d3a399fa1a2498780283f7e700b0b966bbc449ddef'.substring(i, i + 2), radix: 16),
  ]).asUnmodifiableView();

  Uint8List encode(List<int> id) {
    if (id.length != 32 || id.every((v) => v == 0)) _bad();
    final exactId = _bytes(id);
    final message = MessageBuilder(initialCapacityWords: 1024);
    final root = message.initRoot(_Factory(1, 2));
    root.setUint16Field(0, 1);
    root.setDataField(0, schema);
    root.setDataField(1, exactId);
    return _frame(message);
  }

  AgentTaskObservation decode(List<int> id, Uint8List request, Uint8List reply) {
    if (!_same(encode(id), request)) _bad();
    final root = _root(reply);
    final kindIndex = root.getUint16Field(2);
    if (root.getUint16Field(0) != 1 || kindIndex >= AgentTaskKind.values.length ||
        !_same(root.getDataField(0) ?? [], schema) ||
        !_same(root.getDataField(1) ?? [], id) ||
        !_same(root.getDataField(2) ?? [], sha256.convert(request).bytes)) {
      _bad();
    }
    final key = root.getDataField(3) ?? <int>[];
    final result = AgentTaskObservation(AgentTaskKind.values[kindIndex],
        key.isEmpty ? null : key);
    if (!_same(_canonicalReply(id, request, result), reply)) _bad();
    return result;
  }

  Uint8List _canonicalReply(List<int> id, Uint8List request, AgentTaskObservation value) {
    final message = MessageBuilder(initialCapacityWords: 1024);
    final root = message.initRoot(_Factory(1, 4));
    root.setUint16Field(0, 1);
    root.setDataField(0, schema);
    root.setDataField(1, _bytes(id));
    root.setDataField(2, _bytes(sha256.convert(request).bytes));
    root.setUint16Field(2, value.kind.index);
    root.setDataField(3, value.taskKey ?? Uint8List(0));
    return _frame(message);
  }

  Uint8List _frame(MessageBuilder message) {
    final bytes = Uint8List.fromList([...prefix, ...message.serialize()]);
    if (bytes.length > maxFrameBytes) _bad();
    return bytes;
  }

  _Reader _root(Uint8List bytes) {
    if (bytes.length < 24 || bytes.length > maxFrameBytes ||
        !_same(bytes.sublist(0, 8), prefix)) {
      _bad();
    }
    final body = Uint8List.sublistView(bytes, 8);
    if (body.length % 8 != 0) _bad();
    final header = ByteData.sublistView(body);
    // This fixed tiny profile has a single canonical segment.
    if (header.getUint32(0, Endian.little) != 0 ||
        8 + header.getUint32(4, Endian.little) * 8 != body.length) {
      _bad();
    }
    final reader = MessageReader.deserialize(body, MessageReaderOptions(
        traversalLimitInWords: maxFrameBytes ~/ 8 * 4, nestingLimit: 8, maxSegments: 1));
    final root = reader.getRoot(_Factory(1, 4));
    if (root.raw.dataWords != 1 || root.raw.ptrWords != 4) _bad();
    return root;
  }

  Never _bad() => throw const FormatException('Agent observation receipt mismatch');
}
