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
    bool qualificationStartupObservationEnabled = false,
    void Function(void Function())? registerQualificationStartupFailure,
  }) : _qualificationStartupObservationEnabled = qualificationStartupObservationEnabled {
    if (!qualificationStartupObservationEnabled &&
        registerQualificationStartupFailure != null) {
      throw ArgumentError('Failed-startup marking requires qualification mode');
    }
    // Only the original open() catch retains this constructor-time closure.
    // It is never published through the presentation notifier.
    registerQualificationStartupFailure?.call(_markQualificationStartupFailed);
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
      if (_code == null && (!_startupFailed || _normalCloseRequested)) unawaited(_send(2, Uint8List(0)).catchError(_fail));
    });
  }
  final Process process;
  final bool _qualificationStartupObservationEnabled;
  final String supervisorSha256, hostSha256, packageSha256, expectedProfile;
  final supervision = ValueNotifier<WorkbenchSupervisionState?>(null);
  final _output = StreamController<List<int>>();
  final _diagnostics = StreamController<List<int>>();
  final _exit = Completer<int>();
  final _buffer = <int>[];
  final _diagnosticBytes = <int>[];
  int _diagnosticBytesObserved = 0;
  void _recordDiagnostic(List<int> bytes) {
    _diagnosticBytesObserved += bytes.length;
    _diagnosticBytes.addAll(bytes.take(8192 - _diagnosticBytes.length));
  }

  WorkbenchSupervisionState? _identity;
  // Only the original receive path publishes this after all identity checks.
  // The public presentation notifier is not an admission capability.
  WorkbenchSupervisionState? _validatedState;
  WorkbenchSupervisionState? _initialValidatedState;
  bool _startupFailed = false, _normalCloseRequested = false;
  bool _quarantineSent = false, _quarantineWritten = false;
  bool _quarantineWritePending = false;
  // Independent deeply immutable copies, never read back from public state.
  Map<String, Object?>? _preparedQuarantine, _failedStartupQuarantine;

  void _markQualificationStartupFailed() {
    if (_startupFailed) return;
    _startupFailed = true;
    _closing = true; // Revoke local business admission before any await.
    if (_normalCloseRequested) return; // Never repurpose a normal close.
    _heartbeat?.cancel();
    // Keep this pipe open for the one kind4 request. The original supervisor's
    // unchanged lease clock, not a fabricated Dart status, observes loss.
    try {
      _requestQuarantineIfAllowed();
    } catch (error, stack) {
      // Preserve open()'s primary exception even if existing wire proof is bad.
      _fail(error, stack);
    }
  }

  void _requestQuarantineIfAllowed() {
    final state = _validatedState;
    final initial = _identity;
    if (!_startupFailed || _normalCloseRequested || _quarantineSent ||
        _failed || _code != null || initial == null || state == null ||
        !state.uncertain ||
        state.record['abnormal_resource_closure_protocol'] is! int ||
        state.record['abnormal_resource_closure_protocol'] != 1 ||
        initial.record['abnormal_resource_closure_protocol'] is! int ||
        initial.record['abnormal_resource_closure_protocol'] != 1) return;
    validateWorkbenchQuarantineIdentity(state.record, initial.record);
    if (state.record['gate_closed'] != true ||
        state.ownerPhase != 'Retained' || state.record['normal_shutdown'] == true) {
      throw const FormatException('Invalid failed-startup closing status');
    }
    _quarantineSent = true; // Latch before scheduling, including write failure.
    _quarantineWritePending = true;
    unawaited(_send(4, Uint8List(0)).then<void>((_) {
      _quarantineWritten = true;
      _quarantineWritePending = false;
      _finish();
    }, onError: (Object error, StackTrace stack) {
      _quarantineWritePending = false;
      _fail(error, stack);
      _finish();
    }));
  }

  /// Incomplete data copied only from this original private receive path.
  /// This snapshot grants neither private control nor owner-release proof.
  WorkbenchStartupObservation qualificationStartupObservation({
    required String stage,
    required Object primaryFailure,
    Object? secondaryCloseFailure,
    Object? observerFailure,
  }) {
    if (!_qualificationStartupObservationEnabled) {
      throw StateError('Startup observation requires qualification mode');
    }
    return WorkbenchStartupObservation(
      stage: stage, primaryFailure: primaryFailure,
      initialValidatedState: _initialValidatedState,
      latestValidatedState: _validatedState,
      diagnosticBytes: _diagnosticBytes,
      diagnosticBytesObserved: _diagnosticBytesObserved,
      actualExitCode: _code, stdoutEof: _outputEnded,
      stderrEof: _diagnosticsEnded,
      secondaryCloseFailure: secondaryCloseFailure,
      observerFailure: observerFailure,
      failedStartupQuarantine: _failedStartupQuarantine,
    );
  }

  /// Read-only copy of the original validated receive state. Presentation
  /// notifier writes cannot alter this identity or grant admission.
  Map<String, dynamic> get qualificationValidatedIdentity {
    final state = _validatedState;
    if (state == null) throw StateError('Original supervisor identity unavailable');
    return Map<String, dynamic>.unmodifiable(state.record);
  }

  bool get supportsPrivateControl =>
      !_failed &&
      !_closing &&
      _code == null &&
      !_outputEnded &&
      _validatedState?.phase == 'Running' &&
      _validatedState?.record['gate_closed'] == false;
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
      // Retire already queued heartbeats after the open catch marks failure.
      if (kind == 2 && _startupFailed && !_normalCloseRequested) return;
      if (kind == 1 && _startupFailed) {
        throw const WorkbenchSupervisionUnconfirmed('Business admission is closed');
      }
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
    if (_startupFailed && !_normalCloseRequested) return _writes;
    if (_closing) return _writes;
    _normalCloseRequested = true;
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
          if (_startupFailed) throw const FormatException('Business data after startup failure');
          _output.add(payload);
        } else if (kind == 3) {
          _recordDiagnostic(payload);
          _diagnostics.add(payload);
        } else if (kind == 2) {
          if (_preparedQuarantine != null) {
            throw const FormatException('Status after terminal quarantine proof');
          }
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
          if (_validatedState?.record['gate_closed'] == true &&
              state.record['gate_closed'] != true) {
            throw const FormatException(
              'Supervisor reopened revoked admission',
            );
          }
          if (state.released && !_normalCloseRequested) {
            throw const FormatException('Unrequested normal shutdown proof');
          }
          final prepared = state.record['abnormal_resource_closure_prepared'];
          if (prepared != null) {
            if (!_startupFailed || !_quarantineSent || _normalCloseRequested) {
              throw const FormatException('Unrequested quarantine proof');
            }
            _preparedQuarantine = validateWorkbenchQuarantinePrepared(
              state.record, _identity!.record,
            );
          }
          _initialValidatedState ??= state;
          _validatedState = state;
          supervision.value = state;
          _requestQuarantineIfAllowed();
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
        _quarantineWritePending ||
        _exit.isCompleted) {
      return;
    }
    // This evidence is deliberately independent of normal exitCode/release.
    // Actual original process completion and both pipe EOFs are prerequisites.
    if (!_failed && _startupFailed && !_normalCloseRequested &&
        _quarantineWritten && _preparedQuarantine != null && _code == 2) {
      _failedStartupQuarantine = Map<String, Object?>.unmodifiable({
        'version': 1, 'kind': 'failed-startup-quarantined',
        'prepared': _preparedQuarantine,
        'supervisor_exit_confirmed': true, 'actual_supervisor_exit_code': 2,
        'stdout_eof': true, 'stderr_eof': true,
        'business_outcome': 'Unknown', 'owner_phase': 'Retained',
        'normal_shutdown': false, 'accepted': false,
      });
    }
    final proof = _validatedState;
    if (!_failed && _normalCloseRequested && _code == 0 && proof?.released == true) {
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
