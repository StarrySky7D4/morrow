import 'package:file_selector/file_selector.dart';

class PreparedAudio {
  const PreparedAudio(this.file, {this.release});
  final XFile file;
  final Future<void> Function()? release;
  Future<void> dispose() async => release?.call();
}
