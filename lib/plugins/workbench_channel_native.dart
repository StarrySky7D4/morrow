import 'dart:io';
import 'dart:typed_data';
import 'workbench_channel.dart';

final class NativeWorkbenchChannel implements WorkbenchChannel {
  NativeWorkbenchChannel(this.process);
  final Process process;
  @override
  Stream<List<int>> get output => process.stdout;
  @override
  Stream<List<int>> get diagnostics => process.stderr;
  @override
  Future<int> get exitCode => process.exitCode;
  @override
  Future<void> send(Uint8List frame) async {
    final header = ByteData(4)..setUint32(0, frame.length, Endian.little);
    process.stdin.add(header.buffer.asUint8List());
    process.stdin.add(frame);
    await process.stdin.flush();
  }

  @override
  Future<void> closeInput() => process.stdin.close();
}
