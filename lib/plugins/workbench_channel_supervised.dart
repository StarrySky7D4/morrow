import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'workbench_channel.dart';
import 'workbench_supervision.dart';

/// Windows production transport. GracefulClose keeps the lease alive until
/// real worker/host exit. A wrapper exit alone never confirms owner release.
final class SupervisedWorkbenchChannel implements WorkbenchChannel {
  SupervisedWorkbenchChannel(
    this.process, {
    required this.hostSha256,
    required this.supervisorSha256,
    required this.expectedProfile,
    required this.packageSha256,
  }) {
    unawaited(_exit.future.catchError((Object _) => -1));
    process.stdout.listen(
      _receive,
      onError: _fail,
      onDone: () {
        _outputEnded = true;
        if (_buffer.isNotEmpty) {
          _fail(const FormatException('Truncated supervisor frame'));
        }
        _finish();
      },
    );
    process.stderr.listen(
      (bytes) {
        _recordDiagnostic(bytes);
        _diagnostics.add(bytes);
      },
      onError: _fail,
      onDone: () {
        _diagnosticsEnded = true;
        _finish();
      },
    );
    process.exitCode.then((code) {
      _code = code;
      _heartbeat?.cancel();
      _finish();
    }, onError: _fail);
    _heartbeat = Timer.periodic(const Duration(milliseconds: 200), (_) {
      if (_code == null) unawaited(_send(2, Uint8List(0)).catchError(_fail));
    });
  }
  final Process process;
  final String supervisorSha256, hostSha256, packageSha256, expectedProfile;
  final supervision = ValueNotifier<WorkbenchSupervisionState?>(null);
  final _output = StreamController<List<int>>();
  final _diagnostics = StreamController<List<int>>();
  final _exit = Completer<int>();
  final _buffer = <int>[];
  final _diagnosticBytes = <int>[];
  void _recordDiagnostic(List<int> bytes) {
    _diagnosticBytes.addAll(bytes.take(8192 - _diagnosticBytes.length));
  }

  WorkbenchSupervisionState? _identity;
  Future<void> _writes = Future.value();
  Timer? _heartbeat;
  int? _code;
  bool _outputEnded = false,
      _diagnosticsEnded = false,
      _closing = false,
      _failed = false;
  @override
  Stream<List<int>> get output => _output.stream;
  @override
  Stream<List<int>> get diagnostics => _diagnostics.stream;
  @override
  Future<int> get exitCode => _exit.future;
  Future<void> _send(int kind, Uint8List bytes) {
    final next = _writes.then((_) async {
      if (_failed || _code != null) {
        throw const WorkbenchSupervisionUnconfirmed(
          'Supervisor connection has ended',
        );
      }
      final header = ByteData(4)..setUint32(0, bytes.length + 1, Endian.little);
      process.stdin.add(header.buffer.asUint8List());
      process.stdin.add([kind]);
      process.stdin.add(bytes);
      await process.stdin.flush();
    });
    _writes = next.then<void>((_) {}, onError: (Object _, StackTrace _) {});
    return next;
  }

  @override
  Future<void> send(Uint8List frame) {
    if (_closing || _failed || frame.isEmpty || frame.length > 128 * 1024) {
      return Future.error(
        const WorkbenchSupervisionUnconfirmed('Business admission is closed'),
      );
    }
    return _send(1, frame);
  }

  @override
  Future<void> closeInput() {
    if (_closing) return _writes;
    _closing = true;
    // Explicit ordered close, not transport EOF. Heartbeats remain active while
    // the original host drains; no timeout can manufacture WorkerExit<Storage>.
    return _send(3, Uint8List(0));
  }

  void _receive(List<int> bytes) {
    if (_failed) return;
    _buffer.addAll(bytes);
    try {
      while (_buffer.length >= 4) {
        final length = ByteData.sublistView(
          Uint8List.fromList(_buffer.take(4).toList()),
        ).getUint32(0, Endian.little);
        if (length < 1 || length > 128 * 1024 + 1) {
          throw const FormatException('Supervisor frame budget');
        }
        if (_buffer.length < length + 4) return;
        final kind = _buffer[4];
        final payload = Uint8List.fromList(_buffer.sublist(5, 4 + length));
        _buffer.fillRange(0, 4 + length, 0);
        _buffer.removeRange(0, 4 + length);
        if (kind == 1) {
          if (_identity == null) {
            throw const FormatException('Host data before supervisor identity');
          }
          _output.add(payload);
        } else if (kind == 3) {
          _recordDiagnostic(payload);
          _diagnostics.add(payload);
        } else if (kind == 2) {
          final state = WorkbenchSupervisionState.decode(payload);
          if (state.record['version'] != 2 ||
              state.record['supervisor_sha256'] != supervisorSha256 ||
              state.record['profile'] != expectedProfile ||
              state.record['supervisor_pid'] != process.pid ||
              state.record['host_sha256'] != hostSha256 ||
              state.record['package_sha256'] != packageSha256) {
            throw const FormatException(
              'Supervisor artifact/process binding mismatch',
            );
          }
          final prior = _identity;
          if (prior == null) {
            if (state.phase != 'Running') {
              throw const FormatException('Missing initial supervisor owner');
            }
            _identity = state;
          } else {
            for (final key in [
              'profile',
              'generation',
              'incarnation',
              'child_pid',
              'supervisor_pid',
              'supervisor_creation_filetime',
              'child_creation_filetime',
              'supervisor_sha256',
              'host_sha256',
              'package_sha256',
            ]) {
              if (prior.record[key] != state.record[key]) {
                throw const FormatException(
                  'Supervisor session identity changed',
                );
              }
            }
          }
          if (supervision.value?.record['gate_closed'] == true &&
              state.record['gate_closed'] != true) {
            throw const FormatException(
              'Supervisor reopened revoked admission',
            );
          }
          if (state.released && !_closing) {
            throw const FormatException('Unrequested normal shutdown proof');
          }
          supervision.value = state;
          if (state.uncertain) {
            _output.addError(
              const WorkbenchSupervisionUnconfirmed(
                'Resource or durable owner recovery is unconfirmed',
              ),
            );
          }
        } else {
          throw const FormatException('Unknown supervisor frame');
        }
      }
    } catch (error, stack) {
      _fail(error, stack);
    }
  }

  void _fail(Object error, [StackTrace? stack]) {
    // A queued heartbeat flush may fail after verified process/EOF completion.
    // Its already-closed transport cannot admit work or change the saved verdict.
    if (_failed || _exit.isCompleted) return;
    _failed = true;
    _heartbeat?.cancel();
    _output.addError(error, stack);
    _buffer.fillRange(0, _buffer.length, 0);
    _buffer.clear();
    // Connection loss requests fail-closed cleanup; never kill or replay here.
    unawaited(_closeTransport());
  }

  Future<void>? _transportClose;
  Future<void> _closeTransport() => _transportClose ??= (() async {
    // IOSink.close can throw synchronously while its flush owns the stream.
    // Retire the single ordered writer before closing this exact pipe owner.
    try {
      await _writes;
    } catch (_) {}
    try {
      await process.stdin.close();
    } catch (_) {}
  })();
  void _finish() {
    if (_code == null ||
        !_outputEnded ||
        !_diagnosticsEnded ||
        _exit.isCompleted) {
      return;
    }
    final proof = supervision.value;
    if (!_failed && _code == 0 && proof?.released == true) {
      _exit.complete(0);
    } else {
      _exit.completeError(
        WorkbenchSupervisionUnconfirmed(
          'Supervisor exited $_code without verified normal durable owner release. ${utf8.decode(_diagnosticBytes, allowMalformed: true)}',
        ),
      );
    }
    unawaited(_output.close());
    unawaited(_diagnostics.close());
    unawaited(_closeTransport());
  }
}
