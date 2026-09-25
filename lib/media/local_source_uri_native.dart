import 'dart:io';

String encodeLocalSource(String path) => File(path).uri.toString();
String decodeLocalSource(String value) => Uri.parse(value).toFilePath();
