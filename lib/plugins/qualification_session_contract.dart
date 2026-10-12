// Qualification-only fixed contract. Pure bytes/configuration: no IO, process,
// FFI, owner factory, transport injection or opt-in side effects on import.
import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';

const qualificationSessionCompiled =
    bool.fromEnvironment('MORROW_C28_SESSION_QUALIFICATION', defaultValue: false);
const qualificationSessionContractSha256 =
    '4d80a6972fd548b6bc9f36ff230ec94343a0280f7d75974739f2edca6c0179df';

Never _qBad(String reason) => throw FormatException(reason);
void _qNeed(bool value, String reason) { if (!value) _qBad(reason); }
bool _qSame(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  var difference = 0;
  for (var i = 0; i < a.length; i++) { difference |= a[i] ^ b[i]; }
  return difference == 0;
}
Uint8List qualificationHex(String value, int length, {bool nonzero = true}) {
  _qNeed(RegExp('^[0-9a-f]{${length * 2}}\$').hasMatch(value), 'Qualification hex');
  final result = Uint8List.fromList([
    for (var i = 0; i < value.length; i += 2)
      int.parse(value.substring(i, i + 2), radix: 16),
  ]);
  _qNeed(!nonzero || result.any((b) => b != 0), 'Qualification zero identity');
  return result.asUnmodifiableView();
}
String qualificationHexBytes(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
String qualificationWindowsPath(String value) {
  _qNeed(value.length <= 240 && RegExp(r'^[A-Za-z]:/').hasMatch(value)
      && value.codeUnits.every((b) => b >= 32 && b < 127)
      && !value.contains('\\') && !value.substring(2).contains(':'),
      'Qualification drive-absolute ASCII path');
  for (final part in value.substring(3).split('/')) {
    _qNeed(part.isNotEmpty && part != '.' && part != '..'
        && !part.endsWith('.') && !part.endsWith(' ')
        && !RegExp(r'[<>"|?*]').hasMatch(part), 'Qualification path component');
    _qNeed(!RegExp(r'^(con|prn|aux|nul|conin\$|conout\$|com[1-9]|lpt[1-9])$',
        caseSensitive: false).hasMatch(part.split('.').first.trimRight()), 'Qualification device path');
  }
  return value;
}

/// An external reviewed plan pin, not permission inferred from environment.
final class QualificationSessionConfiguration {
  QualificationSessionConfiguration({required String planPath,
    required this.planBytes, required this.planSha256})
      : planPath = qualificationWindowsPath(planPath) {
    _qNeed(planBytes >= 1 && planBytes <= 16384, 'Qualification plan byte cap');
    qualificationHex(planSha256, 32);
  }
  final String planPath, planSha256;
  final int planBytes;
  void requireCompiled() {
    if (!qualificationSessionCompiled) {
      throw StateError('Qualification support is compile-time disabled');
    }
  }
  List<String> get trailer => List.unmodifiable([
    '--qualification-session-plan', planPath, '$planBytes', planSha256,
  ]);
  Map<String, String> get environment => Map.unmodifiable({
    'MORROW_C28_SESSION_QUALIFICATION': planSha256,
  });
}

final class QualificationArtifactPin {
  const QualificationArtifactPin(this.path, this.bytes, this.sha256);
  final String path, sha256;
  final int bytes;
}

/// Exactly the frozen 19-entry array, including canonical raw-byte equality.
final class QualificationSessionPlan {
  QualificationSessionPlan._(this.configuration, this.profileRoot, this.runId,
      this.nonceHex, this.host, this.supervisor, this.package, this.fixture);
  static QualificationSessionPlan decode(
      QualificationSessionConfiguration configuration, Uint8List raw) {
    _qNeed(raw.length == configuration.planBytes && raw.length <= 16384
        && sha256.convert(raw).toString() == configuration.planSha256,
        'Qualification external plan pin');
    final decoded = jsonDecode(utf8.decode(raw));
    _qNeed(decoded is List && decoded.length == 19, 'Qualification exact plan array');
    final row = decoded as List<dynamic>;
    _qNeed(_qSame(utf8.encode(jsonEncode(row)), raw), 'Qualification canonical compact plan');
    String text(int index) {
      final value = row[index];
      _qNeed(value is String && value.codeUnits.every((b) => b >= 32 && b < 127),
          'Qualification ASCII plan field');
      return value as String;
    }
    int size(int index, int cap) {
      final value = row[index];
      _qNeed(value is int && value > 0 && value <= cap, 'Qualification exact integer size');
      return value as int;
    }
    QualificationArtifactPin artifact(int index, int cap) {
      final path = qualificationWindowsPath(text(index));
      final bytes = size(index + 1, cap), digest = text(index + 2);
      qualificationHex(digest, 32);
      return QualificationArtifactPin(path, bytes, digest);
    }
    _qNeed(text(0) == 'c28-product-session-qualification-v1', 'Qualification plan schema');
    final profile = qualificationWindowsPath(text(1)), run = text(2), nonce = text(3);
    _qNeed(RegExp(r'^[A-Za-z0-9-]{1,64}$').hasMatch(run), 'Qualification run identity');
    qualificationHex(nonce, 16);
    _qNeed(profile.split('/').last == 'c28-product-session-$nonce', 'Qualification nonce profile');
    final host = artifact(4, 268435456), supervisor = artifact(7, 268435456);
    final package = artifact(10, 33554432), fixture = artifact(13, 425912);
    _qNeed({host.sha256, supervisor.sha256, package.sha256, fixture.sha256}.length == 4,
        'Qualification distinct artifact content roles');
    _qNeed({host.path.toLowerCase(),supervisor.path.toLowerCase(),
        package.path.toLowerCase(),fixture.path.toLowerCase()}.length == 4,
        'Qualification distinct artifact paths');
    _qNeed(fixture.bytes == 425912 && fixture.sha256 ==
        'cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b',
        'Qualification immutable fixture');
    _qNeed(row[16] is int && row[16] == 40000 && row[17] is int
        && row[17] == 20000 && row[18] is int && row[18] == 60000,
        'Qualification fixed deadlines');
    return QualificationSessionPlan._(configuration, profile, run, nonce,
        host, supervisor, package, fixture);
  }
  final QualificationSessionConfiguration configuration;
  final String profileRoot, runId, nonceHex;
  final QualificationArtifactPin host, supervisor, package, fixture;
  String get sessionId => 'c28-product-session-$nonceHex';
  Uint8List get nonce => qualificationHex(nonceHex, 16);
  Uint8List get planDigest => qualificationHex(configuration.planSha256, 32);
}

enum QualificationSessionOperation { start, stop, poll, acknowledge }
enum QualificationSessionStorage {
  local, running, stopping, reclaimed, recoveryRequired, unavailable,
}

final class QualificationSessionRequest {
  QualificationSessionRequest({required this.operation, required List<int> id,
    required List<int> planDigest, required List<int> nonce,
    required List<int> task, required List<int> barrierRequest,
    required List<int> barrierReply})
      : id = _qFixed(id, 32, true), planDigest = _qFixed(planDigest, 32, true),
        nonce = _qFixed(nonce, 16, true), task = _qFixed(task, 32, false),
        barrierRequest = _qFixed(barrierRequest, 32, false),
        barrierReply = _qFixed(barrierReply, 32, false) {
    final starting = operation == QualificationSessionOperation.start;
    _qNeed(starting == this.task.every((b) => b == 0), 'Qualification actual task rule');
    final barrier = starting || operation == QualificationSessionOperation.acknowledge;
    _qNeed(barrier == this.barrierRequest.any((b) => b != 0)
        && barrier == this.barrierReply.any((b) => b != 0), 'Qualification delivery barrier');
  }
  final QualificationSessionOperation operation;
  final Uint8List id, planDigest, nonce, task, barrierRequest, barrierReply;
  Uint8List encode() {
    final out = Uint8List(188);
    out.setRange(0, 8, ascii.encode('MROWQF42'));
    out[8] = 1; out[9] = operation.index + 1;
    for (final pair in <(int, List<int>)>[(12,id), (44,planDigest), (76,nonce),
        (92,task), (124,barrierRequest), (156,barrierReply)]) {
      out.setRange(pair.$1, pair.$1 + pair.$2.length, pair.$2);
    }
    return out;
  }
}
Uint8List _qFixed(List<int> raw, int size, bool nonzero) {
  _qNeed(raw.length == size && raw.every((b) => b >= 0 && b <= 255)
      && (!nonzero || raw.any((b) => b != 0)), 'Qualification fixed byte field');
  return Uint8List.fromList(raw).asUnmodifiableView();
}

/// Pure decoder only. It cannot create a controller or supply a transport.
final class QualificationSessionReply {
  QualificationSessionReply._(this.storage, this.task, this.acknowledgedTask,
      this.incarnation, this.profile, this.healthyReclaimed,
      this.contextRetained, this.fenced);
  static QualificationSessionReply decode(QualificationSessionRequest request,
      Uint8List actualRequest, Uint8List raw, {required List<int> incarnation,
      required List<int> profile}) {
    _qNeed(_qSame(request.encode(), actualRequest) && raw.length == 256
        && _qSame(raw.sublist(0, 8), ascii.encode('MROWQR42'))
        && raw[8] == 1 && raw[9] == request.operation.index + 1 && raw[10] == 0
        && raw[11] < QualificationSessionStorage.values.length,
        'Qualification exact response frame');
    for (final pair in <(int, List<int>)>[(12,request.id), (44,request.planDigest),
        (76,request.nonce), (156,sha256.convert(actualRequest).bytes),
        (188,_qFixed(incarnation, 32, true)), (220,_qFixed(profile, 32, true))]) {
      _qNeed(_qSame(raw.sublist(pair.$1, pair.$1 + pair.$2.length), pair.$2),
          'Qualification original request/owner correlation');
    }
    _qNeed(raw[252] == 1 && raw.sublist(253).every((b) => b == 0 || b == 1),
        'Qualification original binding/boolean fields');
    final storage = QualificationSessionStorage.values[raw[11]];
    _qNeed(raw[253] == 0 || storage == QualificationSessionStorage.reclaimed,
        'Qualification healthy real reclaimed only');
    final task = raw.sublist(92,124), acknowledged = raw.sublist(124,156);
    if (request.operation == QualificationSessionOperation.acknowledge) {
      _qNeed(task.every((b) => b == 0) && _qSame(acknowledged, request.task)
          && storage == QualificationSessionStorage.local, 'Qualification actual ACK shape');
    } else {
      _qNeed(task.any((b) => b != 0) && acknowledged.every((b) => b == 0)
          && (request.operation == QualificationSessionOperation.start || _qSame(task, request.task)),
          'Qualification actual retained task');
    }
    return QualificationSessionReply._(storage, _qFixed(task,32,false),
        _qFixed(acknowledged,32,false), _qFixed(incarnation,32,true),
        _qFixed(profile,32,true), raw[253] == 1, raw[254] == 1, raw[255] == 1);
  }
  final QualificationSessionStorage storage;
  final Uint8List task, acknowledgedTask, incarnation, profile;
  final bool healthyReclaimed, contextRetained, fenced;
}

/// Pure bookkeeping, never authority or a transport. A validated effect is
/// retained even when the caller's subsequent gate/evidence check throws.
final class QualificationSessionAttemptState {
  bool _start = false, _stop = false, _ack = false;
  Uint8List? _task, _acknowledged;
  List<int>? get taskKey => _task;
  List<int>? get acknowledgedTask => _acknowledged;
  void requireStartBarrier({required bool originalOpenCompleted,
      required bool actualNoneDecoded, required List<int>? startupRequestDigest,
      required List<int>? startupReplyDigest}) {
    if (!_start || !originalOpenCompleted || !actualNoneDecoded ||
        startupRequestDigest == null || startupReplyDigest == null) {
      throw StateError('Qualification actual startup/None barrier unavailable');
    }
    _qFixed(startupRequestDigest,32,true);
    _qFixed(startupReplyDigest,32,true);
  }
  void reserve(QualificationSessionOperation operation) {
    if (operation == QualificationSessionOperation.start) {
      if (_start) throw StateError('Qualification Start already attempted');
      _start = true;
    } else if (operation == QualificationSessionOperation.stop) {
      if (_stop) throw StateError('Qualification Stop already attempted');
      _stop = true;
    } else if (operation == QualificationSessionOperation.acknowledge) {
      if (_ack) throw StateError('Qualification ACK already attempted');
      _ack = true;
    }
    if (operation != QualificationSessionOperation.start && _task == null) {
      throw StateError('Qualification actual task is unknown');
    }
  }
  void retainValidated(QualificationSessionReply reply, QualificationSessionOperation operation) {
    if (operation == QualificationSessionOperation.acknowledge) {
      if (_task == null || !_qSame(reply.acknowledgedTask, _task!)) {
        throw StateError('Qualification ACK history foreign key');
      }
      _acknowledged = reply.acknowledgedTask;
      _task = null;
    } else {
      if (_task != null && !_qSame(reply.task, _task!)) {
        throw StateError('Qualification foreign actual task');
      }
      _task = reply.task;
    }
  }
}
