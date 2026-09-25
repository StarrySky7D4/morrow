import 'package:web/web.dart' as web;
import '../media/texture_storage_web.dart' as storage;
import 'attachment.dart';

Future<void> exportAttachment(IdeaAttachment attachment) async {
  final local = attachment.source.local;
  // Download the immutable browser Blob directly. Converting an entire file
  // to Dart bytes and then to another Blob needlessly doubles large previews.
  final url = local
      ? web.URL.createObjectURL(await storage.resolveBlob(attachment.source))
      : attachment.source.location;
  final anchor = web.HTMLAnchorElement()
    ..href = url
    ..download = attachment.source.name;
  web.document.body?.append(anchor);
  anchor.click();
  anchor.remove();
  Future<void>.delayed(const Duration(seconds: 30), () {
    if (local) web.URL.revokeObjectURL(url);
  });
}

Future<void> openAttachment(IdeaAttachment attachment) =>
    exportAttachment(attachment);
