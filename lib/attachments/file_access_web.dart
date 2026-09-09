import 'dart:js_interop';
import 'package:web/web.dart' as web;
import '../media/texture_repository.dart';
import 'attachment.dart';

Future<void> exportAttachment(IdeaAttachment attachment) async {
  final resolved = await TextureRepository.resolve(attachment.source);
  final url = resolved.bytes == null
      ? resolved.uri
      : web.URL.createObjectURL(web.Blob([resolved.bytes!.toJS].toJS));
  final anchor = web.HTMLAnchorElement()
    ..href = url
    ..download = attachment.source.name;
  web.document.body?.append(anchor);
  anchor.click();
  anchor.remove();
  Future<void>.delayed(const Duration(seconds: 30), () {
    if (resolved.bytes != null) web.URL.revokeObjectURL(url);
    resolved.release?.call();
  });
}

Future<void> openAttachment(IdeaAttachment attachment) =>
    exportAttachment(attachment);
