import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:url_launcher/url_launcher.dart';
import 'attachment.dart';

Future<void> exportAttachment(IdeaAttachment attachment) async {
  final destination = await getSaveLocation(
    suggestedName: attachment.source.name,
  );
  if (destination != null) {
    await XFile(attachment.source.location).saveTo(destination.path);
  }
}

Future<void> openAttachment(IdeaAttachment attachment) async {
  // Attachments remain data. Executable/script formats can only be exported.
  const blocked = [
    'exe',
    'com',
    'msi',
    'bat',
    'cmd',
    'ps1',
    'vbs',
    'js',
    'lnk',
    'url',
    'scr',
    'reg',
  ];
  if (blocked.contains(attachment.extension)) {
    await exportAttachment(attachment);
    return;
  }
  final file = File(attachment.source.location);
  if (!await file.exists() ||
      !await launchUrl(file.uri, mode: LaunchMode.externalApplication)) {
    throw const FormatException('无法打开文件，请先安装对应应用，或将附件另存后打开。');
  }
}
