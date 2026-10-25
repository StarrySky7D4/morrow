import 'dart:typed_data';
import 'package:morrow_studio/plugins/directory_task_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';

final directoryKey = Uint8List.fromList(List.filled(32, 8));
DirectoryTaskRequest directoryRequest() => DirectoryTaskRequest(
  submission: Uint8List.fromList(List.filled(32, 7)),
  packageId: 'test.directory',
  packageDigest: Uint8List.fromList(List.filled(32, 3)),
  registryRevision: BigInt.one,
  handler: 'directory.immediate-selected',
  selectedPath: r'C:\selected',
  maxEntries: 1024,
  maxMetadataBytes: BigInt.from(1048576),
  timeoutMs: 10000,
);
IoTaskSnapshot directoryLocal() => IoTaskSnapshot(
  storage: IoStoragePhase.local,
  delivery: IoDeliveryPhase.absent,
  exit: null,
);
IoTaskSnapshot directoryState(
  DirectoryTaskRequest request, {
  IoStoragePhase storage = IoStoragePhase.running,
  IoDeliveryPhase delivery = IoDeliveryPhase.ready,
}) => IoTaskSnapshot(
  key: directoryKey,
  submission: request.submission,
  storage: storage,
  delivery: delivery,
  exit: storage == IoStoragePhase.reclaimed
      ? const IoTaskExit(
          execution: IoJobError.none,
          disconnect: IoJobError.none,
          maintenance: IoJobError.none,
        )
      : null,
);

class DirectoryFake implements DirectoryTaskBackend, WorkbenchIoTaskControl {
  int starts = 0, runs = 0, reads = 0, guestReads = 0, cancels = 0, acks = 0;
  bool loseStart = false, loseRun = false, loseRead = false, loseAck = false;
  late DirectoryTaskRequest request;
  IoTaskSnapshot state = directoryLocal();
  Future<DirectoryTaskRead> Function()? delayedGuest;
  @override
  Future<IoTaskSnapshot> startDirectory(DirectoryTaskRequest value) async {
    starts++;
    request = value;
    state = directoryState(value);
    if (loseStart) throw StateError('lost start');
    return state;
  }

  @override
  Future<IoTaskSnapshot> runDirectoryGuest(Uint8List key, int timeoutMs) async {
    runs++;
    state = directoryState(request);
    if (loseRun) throw StateError('lost run acknowledgement');
    return state;
  }

  @override
  Future<DirectoryTaskRead> readDirectory(Uint8List key) async {
    reads++;
    state = directoryState(request, delivery: IoDeliveryPhase.consumed);
    return DirectoryTaskRead(
      snapshot: state,
      result: DirectoryTaskCaptured(70, BigInt.from(7000)),
    );
  }

  @override
  Future<DirectoryTaskRead> readDirectoryGuest(Uint8List key) async {
    guestReads++;
    if (delayedGuest != null) return delayedGuest!();
    state = directoryState(
      request,
      storage: IoStoragePhase.stopping,
      delivery: IoDeliveryPhase.consumed,
    );
    if (loseRead) throw StateError('read reply lost after consumption');
    return DirectoryTaskRead(
      snapshot: state,
      result: DirectoryTaskGuest(
        frame: Uint8List.fromList([1, 2, 3]),
        unknown: false,
        executionFault: 0,
        exitCode: 0,
        hostCalls: 4,
      ),
    );
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async => state;
  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async => state;
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    cancels++;
    state = directoryState(
      request,
      storage: IoStoragePhase.stopping,
      delivery: IoDeliveryPhase.consumed,
    );
    return state;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    acks++;
    state = directoryLocal();
    if (loseAck) throw StateError('lost ack');
    return state;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}
