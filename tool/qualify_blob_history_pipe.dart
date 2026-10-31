// Windows process-transport qualification. JSON files are local evidence only;
// requests and responses use the production Cap'n Proto codec and native pipe.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import '../lib/plugins/blob_history_delivery_models.dart';
import '../lib/plugins/blob_history_delivery_native.dart';
import '../lib/plugins/blob_upload_history_packet.dart';
import '../lib/plugins/generated/host.capnp.dart' as host;
import '../lib/plugins/generated/identity.dart' as contract;
import '../lib/plugins/host_request.dart';
import '../lib/plugins/workbench_channel_native.dart';

void require(bool value, String detail) {
  if (!value) throw StateError(detail);
}

bool same(List<int>? a, List<int>? b) =>
    a != null &&
    b != null &&
    a.length == b.length &&
    List.generate(a.length, (i) => i).every((i) => a[i] == b[i]);
Uint8List unhex(String s) => Uint8List.fromList([
  for (var i = 0; i < s.length; i += 2)
    int.parse(s.substring(i, i + 2), radix: 16),
]);
Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
String hex(List<int> bytes) =>
    bytes.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
void save(File f, Object value) {
  require(!f.existsSync(), 'fresh receipt required');
  f.writeAsStringSync(
    '${const JsonEncoder.withIndent('  ').convert(value)}\n',
    flush: true,
  );
}

final class LostReply implements Exception {}

final class Pipe {
  Pipe(this.channel) : input = StreamIterator(channel.output);
  final NativeWorkbenchChannel channel;
  final StreamIterator<List<int>> input;
  List<int> pending = [];
  int offset = 0;
  bool loseNextApproval = false;
  final List<String> actions = [];
  int erasedReplies = 0;
  Future<Uint8List> take(int size) async {
    final result = Uint8List(size);
    for (var written = 0; written < size;) {
      if (offset == pending.length) {
        require(await input.moveNext(), 'unexpected original host EOF');
        pending = input.current;
        offset = 0;
      }
      final available = pending.length - offset;
      final count = available < size - written ? available : size - written;
      result.setRange(written, written + count, pending, offset);
      offset += count;
      written += count;
    }
    return result;
  }

  Future<T> exchange<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
  }) async {
    actions.add(action.name);
    await sendHostRequest(
      action,
      configure: configure,
      clearAfterSend: true,
      send: channel.send,
    );
    final header = await take(4);
    final length = ByteData.sublistView(header).getUint32(0, Endian.little);
    require(length >= 8 && length <= 128 * 1024, 'bounded reply frame');
    final frame = await take(length);
    try {
      final reply = MessageReader.deserialize(
        frame,
        MessageReaderOptions(
          traversalLimitInWords: 16384,
          nestingLimit: 20,
          maxSegments: 512,
        ),
      ).getRoot(host.responseFactory);
      require(
        reply.version == 1 && same(reply.digest, contract.hostDigest),
        'host contract identity',
      );
      require(
        reply.error == null || reply.error!.isEmpty,
        'native host rejected the explicit request: ${reply.error}',
      );
      if (loseNextApproval &&
          action == host.Action.blobHistoryDeliveryApprove) {
        loseNextApproval = false;
        throw LostReply();
      }
      return decode(reply);
    } finally {
      if (clearReply) {
        frame.fillRange(0, frame.length, 0);
        erasedReplies++;
      }
    }
  }

  Future<int> closeOriginal() async {
    await channel.closeInput();
    require(offset == pending.length, 'unconsumed response bytes');
    require(
      !await input.moveNext(),
      'unexpected reply after final acknowledgement',
    );
    return channel.exitCode;
  }
}

Map<String, Object?> receipt(BlobHistoryDeliveryReceipt r) => {
  'key': hex(r.key),
  'request_id': hex(r.requestId),
  'consumed': r.consumed,
  'dispatched': r.dispatched,
  'finished': r.finished,
  'acknowledged': r.acknowledged,
  'close': r.close.name,
  'error': r.error?.name,
  'delivery_error': r.deliveryError?.name,
  'has_execution': r.hasExecution,
  'execution_fault': r.executionFault.name,
  'exit_code': r.exitCode,
  'host_calls': r.hostCalls,
  'fuel_remaining': r.fuelRemaining.toString(),
  'output_type': r.outputType,
  'output_bytes': r.output?.length,
  'output_sha256': r.output == null
      ? null
      : sha256.convert(r.output!).toString(),
};

Future<void> main(List<String> args) async {
  require(
    Platform.isWindows && args.length == 3,
    'usage: HOST_EXE FIXTURE_ROOT NEW_RECEIPT_ROOT',
  );
  final root = Directory(args[1]), out = Directory(args[2]);
  require(
    root.isAbsolute && out.isAbsolute && !out.existsSync(),
    'fresh absolute receipt directory',
  );
  final fixture =
      jsonDecode(File('${root.path}/fixture.json').readAsStringSync())
          as Map<String, dynamic>;
  require(
    fixture['status'] == 'FRESH_PROTECTED_HISTORY_FIXTURE_CLOSED',
    'original fixture process must have closed',
  );
  out.createSync();
  final env = Map<String, String>.from(Platform.environment)
    ..removeWhere((k, _) => k.startsWith('MORROW_'));
  env.addAll({'TEMP': r'D:\build_cache\tmp', 'TMP': r'D:\build_cache\tmp'});
  // No retained supervisor or upload owner is opened. This is a fresh fixture.
  final process = await Process.start(
    args[0],
    [fixture['database'] as String, '${root.path}/no-startup-package.mplugin'],
    environment: env,
    includeParentEnvironment: false,
    mode: ProcessStartMode.normal,
  );
  save(File('${out.path}/host-owner.json'), {
    'pid': process.pid,
    'executable': args[0],
    'original_handle_held': true,
    'new_fixture': root.path,
    'automatic_retry': false,
  });
  final diagnostics = process.stderr.fold<List<int>>(
    [],
    (a, b) => a..addAll(b),
  );
  final pipe = Pipe(NativeWorkbenchChannel(process));
  final client = NativeBlobHistoryDeliveryClient(pipe.exchange);
  BlobHistoryDeliveryRequest request(int n) => BlobHistoryDeliveryRequest(
    requestId: id(n),
    submission: unhex(fixture['submission'] as String),
    packageId: fixture['package_id'] as String,
    packageDigest: unhex(fixture['package_digest'] as String),
    registryRevision: BigInt.parse(fixture['registry_revision'] as String),
    handler: 'history.inspect',
    outputType: 'morrow.blob.upload.history.v1',
    timeoutMs: 120000,
  );
  final results = <Map<String, Object?>>[];
  try {
    require(
      await client.discoverBlobHistoryDelivery(id(99)) == null,
      'absent request discovery',
    );
    for (final n in [1, 2, 3]) {
      final q = request(n),
          preview = await client.prepareBlobHistoryDelivery(request(n));
      final original = preview.selectedHistory;
      require(
        original.originalUnknown && original.operationId == 'original-unknown',
        'original Unknown remains visible',
      );
      final reread = await client.previewBlobHistoryDelivery(preview.key, q);
      require(
        same(reread.packet, preview.packet) && reread.taskId == preview.taskId,
        'stable exact preview',
      );
      final reserved = await client.discoverBlobHistoryDelivery(q.requestId);
      require(
        reserved != null &&
            !reserved.consumed &&
            !reserved.dispatched &&
            !reserved.hasExecution,
        'preview must not dispatch',
      );
      BlobHistoryDeliveryReceipt r;
      if (n == 3) {
        r = await client.cancelBlobHistoryDelivery(preview.key, q.requestId);
        require(
          r.close == BlobHistoryDeliveryClose.notCreated &&
              !r.dispatched &&
              !r.hasExecution &&
              r.error == BlobHistoryDeliveryError.cancelled,
          'cancel before execution',
        );
      } else {
        if (n == 2) {
          pipe.loseNextApproval = true;
          var lost = false;
          try {
            await client.approveBlobHistoryDelivery(preview);
          } on LostReply {
            lost = true;
          }
          require(
            lost,
            'approval reply loss injected after original host response',
          );
          final count = pipe.actions.length;
          var blocked = false;
          try {
            await client.approveBlobHistoryDelivery(preview);
          } on StateError {
            blocked = true;
          }
          require(
            blocked && pipe.actions.length == count,
            'lost reply must not submit another approval',
          );
          r = await client.readBlobHistoryDelivery(
            preview.key,
            q.requestId,
            expectedOutputType: q.outputType,
          );
        } else {
          r = await client.approveBlobHistoryDelivery(preview);
        }
        require(
          r.consumed &&
              r.dispatched &&
              r.finished &&
              r.hasExecution &&
              r.exitCode == 0 &&
              r.close == BlobHistoryDeliveryClose.closed &&
              r.error == null &&
              r.deliveryError == null,
          'original guest execution and checked close',
        );
        require(
          same(r.output, preview.packet),
          'guest returns exact selected public packet',
        );
        require(
          PublicHistoryPacket.decodeTask(
            r.output!,
            preview.taskId,
          ).originalUnknown,
          'delivery does not resolve old upload',
        );
        final again = await client.readBlobHistoryDelivery(
          preview.key,
          q.requestId,
          expectedOutputType: q.outputType,
        );
        require(
          jsonEncode(receipt(again)) == jsonEncode(receipt(r)),
          'read returns the same original receipt',
        );
      }
      final ack = await client.acknowledgeBlobHistoryDelivery(r);
      require(
        ack.acknowledged &&
            ack.close == r.close &&
            ack.hostCalls == r.hostCalls,
        'acknowledgement preserves original facts',
      );
      results.add({
        'scenario': n == 1
            ? 'approved'
            : n == 2
            ? 'lost_reply_explicit_read'
            : 'cancelled',
        'receipt': receipt(ack),
        'original_upload_unknown': original.originalUnknown,
        'preview_packet_sha256': sha256.convert(preview.packet).toString(),
      });
    }
    // Observe the same persisted record again after all three disclosure decisions.
    final last = await client.prepareBlobHistoryDelivery(request(4));
    require(
      last.selectedHistory.originalUnknown &&
          last.selectedHistory.revision == 2,
      'original upload record still unchanged',
    );
    final cancelled = await client.cancelBlobHistoryDelivery(last.key, id(4));
    await client.acknowledgeBlobHistoryDelivery(cancelled);
    final code = await pipe.closeOriginal();
    final stderr = await diagnostics;
    File('${out.path}/host.stderr').writeAsBytesSync(stderr, flush: true);
    require(code == 0, 'original host process returned nonzero: $code');
    save(File('${out.path}/result.json'), {
      'status': 'PASS_SCOPED_DART_NATIVE_CAPNP_HISTORY_PIPE',
      'host_exit_code': code,
      'original_host_process_returned': true,
      'scenarios': results,
      'actions': pipe.actions,
      'erased_reply_frames': pipe.erasedReplies,
      'approval_requests': pipe.actions
          .where((a) => a == host.Action.blobHistoryDeliveryApprove.name)
          .length,
      'same_original_owner': true,
      'simulated_reply_loss_after_actual_response': true,
      'retained_owner_contacts': 0,
      'upload_retries': 0,
      'VM_calls': 0,
      'formal_UI': false,
      'source_accepted': false,
      'release_eligible': false,
    });
    stdout.writeln('PASS_SCOPED_DART_NATIVE_CAPNP_HISTORY_PIPE');
  } catch (error, stack) {
    save(File('${out.path}/failure.json'), {
      'status': 'FAILURE_RETAINED_NOT_ACCEPTED',
      'error': error.toString(),
      'stack': stack.toString(),
      'actions': pipe.actions,
      'automatic_retry': false,
      'original_host_closure_not_inferred': true,
    });
    rethrow;
  }
}
