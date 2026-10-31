import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_studio/plugins/fs_watch_models.dart';
import 'package:morrow_studio/plugins/fs_watch_native.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;

int checks = 0;
void check(bool ok, String label) {
  if (!ok) throw StateError(label);
  checks++;
}

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
void rejects(void Function() f, String label) {
  try {
    f();
  } on FormatException {
    checks++;
    return;
  }
  throw StateError('Did not refuse: $label');
}

class WireFixture {
  final root = MessageBuilder().initRoot(host.responseFactory);
  late final ioState = root.initIoState();
  late final fsWatchState = root.initFsWatchState();
  late final fsWatchResult = root.initFsWatchResult();
  final deliveries = <host.FsWatchDeliveryBuilder>[];
  host.ResponseReader asReader() => root.asReader();
}

WireFixture response({bool finished = false, bool receipt = false}) {
  final r = WireFixture();
  final io = r.ioState;
  io.key = id(11);
  io.submission = id(12);
  io.storage = finished ? 3 : 1;
  io.delivery = 1;
  io.hasExit = finished;
  final s = r.fsWatchState;
  s.ready = finished && !receipt;
  s.delivered = receipt;
  s.phase = finished ? 4 : 1;
  s.frames = receipt ? 2 : 0;
  if (receipt) {
    final v = r.fsWatchResult;
    v.present = true;
    v.success = true;
    v.originalWorkerJoined = true;
    v.nativeRetirementComplete = true;
    v.hasDisconnect = true;
    v.hasMaintenance = true;
    v.hasSnapshot = true;
    v.pickerStarted = true;
    v.pickerClosed = true;
    v.ancestorObjects = 4;
    v.ancestorClosed = 4;
    v.hasNative = true;
    v.nativeHandles = 2;
    v.nativeClosed = 2;
    v.chargedBytesBigInt = (BigInt.one << 53) + BigInt.one;
    final frames = v.initDeliveries(2);
    r.deliveries.addAll([frames[0], frames[1]]);
    for (var i = 0; i < 2; i++) {
      frames[i].sequenceBigInt = BigInt.from(i + 1);
      frames[i].digest = id(20 + i);
      frames[i].fuelRemainingBigInt = (BigInt.one << 63) - BigInt.from(i);
    }
  }
  return r;
}

FsWatchRequest request({String? path}) => FsWatchRequest(
  submission: id(12),
  packageId: 'org.example.watch',
  digest: id(13),
  revision: (BigInt.one << 64) - BigInt.one,
  handler: 'observe',
  path: path ?? r'D:\build_cache\one',
  limits: FsWatchLimits(
    frames: 2,
    wireBytes: BigInt.from(4096),
    windowMs: 29000,
  ),
);
Future<void> main() async {
  var calls = 0;
  host.RequestReader? sent;
  WireFixture Function() next = () => response();
  var loseReply = false;
  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    calls++;
    check(clearReply, 'every watch exchange clears its private response');
    final message = MessageBuilder();
    final out = message.initRoot(host.requestFactory);
    out.action = action;
    configure?.call(out);
    sent = MessageReader.deserialize(
      message.serialize(),
    ).getRoot(host.requestFactory);
    if (loseReply) throw StateError('original reply unavailable');
    return decode(next().asReader());
  }

  final client = NativeFsWatchClient(exchange);
  final raw =
      'D:\\build_cache\\ leading ${String.fromCharCodes([0xd800, 0xd83d, 0xde00])} trailing ';
  final original = request(path: raw);
  await client.startFsWatch(original);
  check(
    calls == 1 && sent!.action == host.Action.fsWatchStart,
    'exactly one start exchange',
  );
  final wire = sent!.fsWatchStart!;
  check(
    wire.registryRevisionBigInt == (BigInt.one << 64) - BigInt.one,
    'full UInt64 revision',
  );
  final bytes = ByteData.sublistView(wire.nominatedPathUtf16Le!);
  check(
    bytes.lengthInBytes == raw.codeUnits.length * 2 &&
        List.generate(
              raw.codeUnits.length,
              (i) => bytes.getUint16(i * 2, Endian.little),
            ).join(',') ==
            raw.codeUnits.join(','),
    'raw UTF16 preserves isolated surrogate emoji spaces and drive spelling',
  );
  check(
    wire.maxFrames == 2 &&
        wire.maxWireBytesBigInt == BigInt.from(4096) &&
        wire.windowMs == 29000,
    'exact original ceilings',
  );
  final digest = id(25);
  await client.approveFsWatch('org.example.watch', digest, BigInt.one, true);
  digest[0] = 44;
  check(
    sent!.action == host.Action.pluginApproveFsWatch &&
        sent!.sha256![0] == 25 &&
        sent!.fsWatchApproved,
    'independent exact package category decision',
  );
  final beforeLost = calls;
  loseReply = true;
  try {
    await client.pollFsWatch(id(11));
    throw FormatException('unexpected result');
  } on StateError {
    check(
      calls == beforeLost + 1,
      'lost original reply has no automatic retry',
    );
  }
  loseReply = false;
  next = () {
    final r = response();
    r.ioState.submission = id(99);
    return r;
  };
  try {
    await client.startFsWatch(original);
    throw StateError('foreign submission accepted');
  } on FormatException {
    checks++;
  }
  next = () {
    final r = response();
    r.ioState.key = id(99);
    return r;
  };
  try {
    await client.pollFsWatch(id(11));
    throw StateError('foreign key accepted');
  } on FormatException {
    checks++;
  }

  var r = response(finished: true, receipt: true);
  final result = NativeFsWatchClient.decode(
    r.asReader(),
    allowResult: true,
  ).result!;
  check(
    result.success &&
        result.workerJoined &&
        result.knownClosed &&
        result.deliveries.length == 2,
    'complete receipt remains separate from IO acknowledgement',
  );
  check(
    result.chargedBytes == (BigInt.one << 53) + BigInt.one &&
        result.deliveries.first.fuelRemaining == BigInt.one << 63,
    'exact Uint64 result counters',
  );
  rejects(
    () => NativeFsWatchClient.decode(r.asReader(), allowResult: false),
    'receipt in a poll response',
  );
  final mutations = <String, void Function(WireFixture)>{
    'ready and delivered': (r) => r.fsWatchState.ready = true,
    'receipt without actual exit': (r) => r.ioState.hasExit = false,
    'unknown success': (r) => r.fsWatchResult.unknown = true,
    'success without original worker': (r) =>
        r.fsWatchResult.originalWorkerJoined = false,
    'false native retirement': (r) => r.fsWatchResult.nativeClosed = 1,
    'missing ancestor close': (r) => r.fsWatchResult.ancestorClosed = 3,
    'open picker with complete retirement': (r) =>
        r.fsWatchResult.pickerClosed = false,
    'false native absence': (r) => r.fsWatchResult.hasNative = false,
    'bad sequence': (r) => r.deliveries[1].sequenceBigInt = BigInt.from(3),
    'zero frame identity': (r) => r.deliveries[1].digest = Uint8List(32),
    'renewed fuel': (r) =>
        r.deliveries[1].fuelRemainingBigInt = (BigInt.one << 63) + BigInt.one,
    'receipt count changed': (r) => r.fsWatchState.frames = 1,
    'frame budget overflow': (r) => r.fsWatchState.frames = 65,
    'unknown phase': (r) => r.fsWatchState.phase = 5,
    'unknown execution': (r) => r.deliveries[0].executionFault = 99,
    'absent result with success data': (r) => r.fsWatchResult.present = false,
  };
  for (final entry in mutations.entries) {
    final bad = response(finished: true, receipt: true);
    entry.value(bad);
    rejects(
      () => NativeFsWatchClient.decode(bad.asReader(), allowResult: true),
      entry.key,
    );
  }
  final unknown = response(finished: true, receipt: true);
  unknown.fsWatchResult.success = false;
  unknown.fsWatchResult.error = 10;
  unknown.fsWatchResult.detail = 'Unknown';
  unknown.fsWatchResult.unknown = true;
  unknown.fsWatchResult.nativeRetirementComplete = false;
  unknown.fsWatchResult.nativeClosed = 1;
  unknown.fsWatchResult.initDeliveries(0);
  check(
    !NativeFsWatchClient.decode(
      unknown.asReader(),
      allowResult: true,
    ).result!.knownClosed,
    'unknown retained instead of promoted to closed',
  );
  check(
    NativeFsWatchClient.decode(
              unknown.asReader(),
              allowResult: true,
            ).snapshot.frames ==
            2 &&
        NativeFsWatchClient.decode(
          unknown.asReader(),
          allowResult: true,
        ).result!.deliveries.isEmpty,
    'Unknown preserves historical progress after original payload destruction',
  );
  FsWatchValidation.handler(List.filled(128, 'é').join());
  checks++;
  rejects(
    () => FsWatchValidation.handler(List.filled(129, 'é').join()),
    'UTF8 handler byte ceiling',
  );
  rejects(() => FsWatchValidation.handler('handler/name'), 'handler delimiter');
  rejects(
    () => FsWatchValidation.handler('handler\u0080name'),
    'handler control',
  );
  for (final path in ['relative', r'C:relative', 'D:\\bad\u0000path']) {
    rejects(() => request(path: path), 'invalid nomination');
  }
  rejects(
    () => FsWatchLimits(frames: 65, wireBytes: BigInt.one, windowMs: 1),
    'frame ceiling',
  );
  rejects(
    () =>
        FsWatchLimits(frames: 1, wireBytes: BigInt.from(4194305), windowMs: 1),
    'byte ceiling',
  );
  rejects(
    () => FsWatchLimits(frames: 1, wireBytes: BigInt.one, windowMs: 30001),
    'time ceiling',
  );
  print('STAGED_WATCH_WIRE_CHECKS_PASS checks=$checks native_calls=0 UI=false');
}
