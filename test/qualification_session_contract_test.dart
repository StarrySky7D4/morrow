// Pure configuration/codec/bookkeeping tests. These vectors are NOT an owner,
// product response or runtime qualification; no Process/FFI/transport injection.
import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/qualification_session_contract.dart';

String hex(String pair, int count) => List.filled(count,pair).join();
Uint8List bytes(int n, int value) => Uint8List.fromList(List.filled(n, value));
List<Object> planFields() => [
  'c28-product-session-qualification-v1',
  'C:/review/c28-product-session-${hex('01',16)}', 'review-1', hex('01',16),
  'C:/review/host.exe', 10, hex('02',32),
  'C:/review/supervisor.exe', 10, hex('03',32),
  'C:/review/session.package', 10, hex('04',32),
  'C:/review/fixture.exe', 425912,
  'cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b',
  40000, 20000, 60000,
];
QualificationSessionPlan decodePlan(List<int> raw) {
  final data = Uint8List.fromList(raw);
  return QualificationSessionPlan.decode(QualificationSessionConfiguration(
      planPath: 'C:/review/plan.json', planBytes: data.length,
      planSha256: sha256.convert(data).toString()), data);
}
QualificationSessionRequest request(QualificationSessionOperation op) =>
    QualificationSessionRequest(operation: op, id: bytes(32,1), planDigest: bytes(32,2),
      nonce: bytes(16,3), task: bytes(32,op == QualificationSessionOperation.start ? 0 : 4),
      barrierRequest: bytes(32,op == QualificationSessionOperation.start ||
          op == QualificationSessionOperation.acknowledge ? 5 : 0),
      barrierReply: bytes(32,op == QualificationSessionOperation.start ||
          op == QualificationSessionOperation.acknowledge ? 6 : 0));
Uint8List vector(QualificationSessionRequest q) {
  final out = Uint8List(256);
  out.setRange(0,8,ascii.encode('MROWQR42')); out[8]=1; out[9]=q.operation.index+1;
  out[11]=q.operation == QualificationSessionOperation.acknowledge ? 0 : 1;
  for (final row in <(int,List<int>)>[(12,q.id),(44,q.planDigest),(76,q.nonce),
    (92,bytes(32,q.operation == QualificationSessionOperation.acknowledge ? 0 : 4)),
    (124,bytes(32,q.operation == QualificationSessionOperation.acknowledge ? 4 : 0)),
    (156,sha256.convert(q.encode()).bytes),(188,bytes(32,7)),(220,bytes(32,8))]) {
    out.setRange(row.$1,row.$1+row.$2.length,row.$2);
  }
  out[252]=1; out[254]=1;
  return out;
}
QualificationSessionReply decodeVector(QualificationSessionRequest q, Uint8List raw) =>
    QualificationSessionReply.decode(q,q.encode(),raw,incarnation: bytes(32,7),profile: bytes(32,8));

void main() {
  test('default compile barrier refuses opt-in before any effects', () {
    final config = QualificationSessionConfiguration(planPath:'C:/review/plan.json',
      planBytes:1,planSha256:hex('01',32));
    if (!qualificationSessionCompiled) expect(config.requireCompiled, throwsStateError);
    expect(config.trailer, ['--qualification-session-plan','C:/review/plan.json','1',hex('01',32)]);
    expect(config.environment, {'MORROW_C28_SESSION_QUALIFICATION':hex('01',32)});
    for (final size in [0,16385]) {
      expect(()=>QualificationSessionConfiguration(planPath:'C:/review/plan.json',
        planBytes:size,planSha256:hex('01',32)),throwsFormatException);
    }
  });
  test('canonical plan and external pin reject ambiguous forms', () {
    final fields = planFields();
    expect(decodePlan(utf8.encode(jsonEncode(fields))).runId,'review-1');
    for (final raw in [utf8.encode('${jsonEncode(fields)}\n'),
      [239,187,191,...utf8.encode(jsonEncode(fields))],
      utf8.encode(jsonEncode([...fields,'extra']))]) {
      expect(()=>decodePlan(raw),throwsFormatException);
    }
    for (final mutation in <(int,Object)>[(0,'unknown'),(3,hex('00',16)),(16,40001),
      (17,20000.0),(14,425911),(1,'C:/review/../profile'),(4,'C:/review/CON.txt'),
      (4,'C:/review/file:ads'),(4,'C:/review/name.'),(2,'non ascii é')]) {
      final changed = [...fields]; changed[mutation.$1]=mutation.$2;
      expect(()=>decodePlan(utf8.encode(jsonEncode(changed))),throwsFormatException);
    }
    final raw = Uint8List.fromList(utf8.encode(jsonEncode(fields)));
    final wrong = QualificationSessionConfiguration(planPath:'C:/review/plan.json',
      planBytes:raw.length,planSha256:hex('09',32));
    expect(()=>QualificationSessionPlan.decode(wrong,raw),throwsFormatException);
  });
  test('fixed frames refuse foreign key/owner and wrong barrier shape', () {
    for (final op in QualificationSessionOperation.values) {
      final q=request(op); expect(q.encode().length,188);
      expect(decodeVector(q,vector(q)).storage,
          op==QualificationSessionOperation.acknowledge ? QualificationSessionStorage.local : QualificationSessionStorage.running);
      for (final offset in [0,8,9,10,12,44,76,156,188,220,252,254]) {
        final bad=vector(q); bad[offset]=bad[offset] == 1 ? 2 : bad[offset]^1;
        expect(()=>decodeVector(q,bad),throwsFormatException);
      }
      final badBoolean=vector(q); badBoolean[255]=2;
      expect(()=>decodeVector(q,badBoolean),throwsFormatException);
      final fenced=vector(q); fenced[255]=1;
      expect(decodeVector(q,fenced).fenced,isTrue);
      if (op != QualificationSessionOperation.start) {
        final wrongKey=vector(q); wrongKey[op==QualificationSessionOperation.acknowledge ? 124 : 92]^=1;
        expect(()=>decodeVector(q,wrongKey),throwsFormatException);
      }
      expect(()=>decodeVector(q,Uint8List.fromList([...vector(q),0])),throwsFormatException);
    }
    expect(()=>QualificationSessionRequest(operation:QualificationSessionOperation.stop,
      id:bytes(32,1),planDigest:bytes(32,2),nonce:bytes(16,3),task:bytes(32,4),
      barrierRequest:bytes(32,5),barrierReply:bytes(32,0)),throwsFormatException);
    expect(()=>QualificationSessionRequest(operation:QualificationSessionOperation.start,
      id:bytes(32,1),planDigest:bytes(32,2),nonce:bytes(16,3),task:bytes(32,0),
      barrierRequest:bytes(32,0),barrierReply:bytes(32,0)),throwsFormatException);
  });
  test('actual consumed context bit is observation, never worker success authority', () {
    final start=request(QualificationSessionOperation.start);
    final raw=vector(start); raw[254]=0; // Original Start consumed external Option.
    final decoded=decodeVector(start,raw);
    expect(decoded.contextRetained,isFalse);
    expect(decoded.storage,QualificationSessionStorage.running);
    expect(decoded.fenced,isFalse);
    final state=QualificationSessionAttemptState(); state.reserve(start.operation);
    state.retainValidated(decoded,start.operation);
    expect(state.taskKey,bytes(32,4));
    expect(()=>state.reserve(start.operation),throwsStateError);
    final debt=decodeVector(start,vector(start)); // Explicit external debt observation.
    expect(debt.contextRetained,isTrue);
    expect(debt.healthyReclaimed,isFalse); // Does not manufacture a healthy exit.
    expect(debt.task,decoded.task);
  });
  test('reservation precedes early failure and forbids Start replay', () {
    final state=QualificationSessionAttemptState();
    state.reserve(QualificationSessionOperation.start);
    expect(()=>state.requireStartBarrier(originalOpenCompleted:false,
      actualNoneDecoded:true,startupRequestDigest:bytes(32,5),
      startupReplyDigest:bytes(32,6)),throwsStateError);
    expect(()=>state.reserve(QualificationSessionOperation.start),throwsStateError);
    expect(state.taskKey,isNull);
  });
  test('local Start barrier rejects absent None and zero/missing startup pair', () {
    final state=QualificationSessionAttemptState();
    state.reserve(QualificationSessionOperation.start);
    expect(()=>state.requireStartBarrier(originalOpenCompleted:true,
      actualNoneDecoded:false,startupRequestDigest:bytes(32,5),
      startupReplyDigest:bytes(32,6)),throwsStateError);
    expect(()=>state.requireStartBarrier(originalOpenCompleted:true,
      actualNoneDecoded:true,startupRequestDigest:bytes(32,0),
      startupReplyDigest:bytes(32,6)),throwsFormatException);
    expect(()=>state.requireStartBarrier(originalOpenCompleted:true,
      actualNoneDecoded:true,startupRequestDigest:bytes(32,5),
      startupReplyDigest:null),throwsStateError);
  });
  test('pure known-key ledger survives postdecode gate failure and consumed ACK', () {
    final state=QualificationSessionAttemptState(), start=request(QualificationSessionOperation.start);
    state.reserve(start.operation);
    final decoded=decodeVector(start,vector(start));
    expect(() { state.retainValidated(decoded,start.operation);
      throw StateError('postdecode original gate closed'); },throwsStateError);
    expect(state.taskKey,bytes(32,4));
    final ack=request(QualificationSessionOperation.acknowledge);
    state.reserve(ack.operation);
    expect(() { state.retainValidated(decodeVector(ack,vector(ack)),ack.operation);
      throw StateError('evidence write failed'); },throwsStateError);
    expect(state.taskKey,isNull); expect(state.acknowledgedTask,bytes(32,4));
    expect(()=>state.reserve(ack.operation),throwsStateError);
  });
}
