import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';
import 'attachment.dart';

const _androidFiles = MethodChannel('dev.morrow/files');

Future<void> _androidFile(String method, IdeaAttachment attachment) async {
  try {
    await _androidFiles.invokeMethod<bool>(method, {
      'path': attachment.source.location,
      'name': attachment.source.name,
    });
  } on PlatformException catch (error) {
    throw FormatException(error.message ?? '文件操作失败，请重试。');
  }
}

Future<void> exportAttachment(IdeaAttachment attachment) async {
  if (Platform.isAndroid) {
    await _androidFile('export', attachment);
    return;
  }
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
  if (Platform.isAndroid) {
    await _androidFile('open', attachment);
    return;
  }
  if (!await file.exists() ||
      !await launchUrl(file.uri, mode: LaunchMode.externalApplication)) {
    throw const FormatException('无法打开文件，请先安装对应应用，或将附件另存后打开。');
  }
}
