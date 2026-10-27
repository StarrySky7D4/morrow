import 'dart:async';
import 'package:morrow_studio/plugins/directory_task_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'directory_task_test_support.dart';

DirectoryTaskRequest nativeDirectoryRequest() {
  final old = directoryRequest();
  return DirectoryTaskRequest(
    submission: old.submission,
    packageId: old.packageId,
    packageDigest: old.packageDigest,
    registryRevision: old.registryRevision,
    handler: old.handler,
    selectedPath: '',
    nativePicker: true,
    maxEntries: old.maxEntries,
    maxMetadataBytes: old.maxMetadataBytes,
    timeoutMs: old.timeoutMs,
  );
}

class PendingNativeDirectory extends DirectoryFake
    implements NativeDirectoryPickerCapabilities {
  final startReply = Completer<IoTaskSnapshot>();
  int statusReads = 0;
  @override
  bool get supportsNativeDirectoryPicker => true;
  @override
  Future<IoTaskSnapshot> startDirectory(DirectoryTaskRequest value) {
    starts++;
    request = value;
    state = directoryState(value, delivery: IoDeliveryPhase.pending);
    return startReply.future;
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statusReads++;
    return state;
  }
}
