import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import 'generated/host.capnp.dart' as wire;
import 'plugin_library.dart' show PluginLibraryEntry;

enum PluginContentAction {
  create('create-content'),
  edit('edit-content'),
  read('read-content'),
  lookup('operation');

  const PluginContentAction(this.capability);
  final String capability;
  static bool permits(String capability) =>
      values.any((action) => action.capability == capability);
}

abstract interface class PluginContentBackend {
  Future<PluginContentReceipt> runPluginContent(
    PluginLibraryEntry entry,
    BigInt catalogRevision,
    PluginContentRequest task,
  );
}

class PluginContentRequest {
  PluginContentRequest({
    required this.action,
    required this.card,
    required this.requestId,
    this.typeId = '',
    this.title = '',
    this.preview = '',
    this.operation = '',
    BigInt? revision,
    BigInt? offset,
    List<int> body = const [],
    this.length = 32768,
  }) : revision = revision ?? BigInt.zero,
       offset = offset ?? BigInt.zero,
       body = Uint8List.fromList(body).asUnmodifiableView() {
    void identity(String value) {
      if (value.isEmpty ||
          utf8.encode(value).length > 256 ||
          RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(value)) {
        throw const FormatException('Invalid card or operation identity');
      }
    }

    identity(card);
    identity(requestId);
    if (action == PluginContentAction.create) identity(typeId);
    if (action == PluginContentAction.lookup) identity(operation);
    if ((action == PluginContentAction.edit ||
            action == PluginContentAction.read) &&
        (this.revision <= BigInt.zero || this.revision.bitLength > 64)) {
      throw const FormatException(
        'Card revision must be a positive unsigned integer',
      );
    }
    if (this.body.length > 32768 ||
        utf8.encode(title).length > 16384 ||
        utf8.encode(preview).length > 16384 ||
        this.offset.isNegative ||
        this.offset > BigInt.from(8 * 1024 * 1024) ||
        length < 1 ||
        length > 32768) {
      throw const FormatException('Card task exceeds its content limits');
    }
  }
  final PluginContentAction action;
  final String card, requestId, typeId, title, preview, operation;
  final BigInt revision, offset;
  final Uint8List body;
  final int length;

  void encode(wire.PluginContentTaskBuilder task) {
    task.card = card;
    task.requestId = requestId;
    switch (action) {
      case PluginContentAction.create:
        task.initCreate()
          ..typeId = typeId
          ..title = title
          ..body = body;
      case PluginContentAction.edit:
        task.initEdit()
          ..revisionBigInt = revision
          ..title = title
          ..body = body
          ..preview = preview;
      case PluginContentAction.read:
        task.initRead()
          ..revisionBigInt = revision
          ..offsetBigInt = offset
          ..length = length;
      case PluginContentAction.lookup:
        task.lookup = operation;
    }
  }

  PluginContentReceipt decode(wire.PluginContentResultReader result) {
    if (result.card != card || result.requestId != requestId) {
      throw const FormatException(
        'Receipt identity differs from the confirmed card task',
      );
    }
    final status = result.status;
    final revision = result.revisionBigInt;
    final bytes = Uint8List.fromList(result.bytes ?? const []);
    final digest = Uint8List.fromList(result.digest ?? const []);
    final op = result.operation ?? '';
    final event = result.event ?? '';
    final failure = result.failure ?? '';
    final total = result.totalLengthBigInt;
    final valid = switch (status) {
      wire.PluginContentStatus.committed =>
        action != PluginContentAction.read &&
            op ==
                (action == PluginContentAction.lookup
                    ? operation
                    : requestId) &&
            revision > BigInt.zero &&
            event.isNotEmpty &&
            digest.length == 32 &&
            bytes.isEmpty,
      wire.PluginContentStatus.chunk =>
        action == PluginContentAction.read &&
            revision == this.revision &&
            result.offsetBigInt == offset &&
            total >= offset &&
            total <= BigInt.from(8 * 1024 * 1024) &&
            BigInt.from(bytes.length) <= total - offset &&
            bytes.length <= length &&
            (bytes.isNotEmpty || total == offset) &&
            digest.length == 32,
      wire.PluginContentStatus.absent =>
        action == PluginContentAction.lookup &&
            op == operation &&
            revision == BigInt.zero &&
            bytes.isEmpty &&
            digest.isEmpty,
      wire.PluginContentStatus.rejected =>
        failure.isNotEmpty && bytes.isEmpty && digest.isEmpty,
      _ => false,
    };
    if (!valid) throw const FormatException('Invalid card task result');
    if (status == wire.PluginContentStatus.chunk &&
        offset == BigInt.zero &&
        BigInt.from(bytes.length) == total &&
        sha256.convert(bytes) != Digest(digest)) {
      throw const FormatException('Card body differs from the reported digest');
    }
    return PluginContentReceipt(
      status!,
      card,
      requestId,
      op,
      revision,
      event,
      digest,
      bytes,
      result.offsetBigInt,
      total,
      failure,
    );
  }
}

class PluginContentReceipt {
  const PluginContentReceipt(
    this.status,
    this.card,
    this.requestId,
    this.operation,
    this.revision,
    this.event,
    this.digest,
    this.bytes,
    this.offset,
    this.totalLength,
    this.failure,
  );
  final wire.PluginContentStatus status;
  final String card, requestId, operation, event, failure;
  final BigInt revision, offset, totalLength;
  final Uint8List digest, bytes;
}
