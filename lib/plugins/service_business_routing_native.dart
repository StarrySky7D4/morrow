part of 'workbench_native.dart';

bool _isScheduler(host.Action action) => switch (action) {
  host.Action.httpStart ||
  host.Action.ioStatus ||
  host.Action.ioPoll ||
  host.Action.ioRead ||
  host.Action.ioCancel ||
  host.Action.ioRepair ||
  host.Action.ioAcknowledge ||
  host.Action.serviceRunStart ||
  host.Action.serviceRunStatus ||
  host.Action.commandSubmit ||
  host.Action.commandStatus ||
  host.Action.commandRead ||
  host.Action.commandCancel => true,
  _ => false,
};

class _HostResponseError extends StateError {
  _HostResponseError(super.message);
}

extension _ServiceBusinessRouting on RustWorkbench {
  void _checkBusinessReply(
    host.Action action,
    host.ResponseReader reply, {
    required bool updatePresentation,
  }) {
    if (updatePresentation) {
      writable = !reply.readOnly;
      final notice = reply.maintenanceWarning ?? '';
      maintenanceWarning = notice.isEmpty ? null : notice;
    }
    if ((reply.error ?? '').isEmpty) return;
    if (action == host.Action.query) {
      throw QueryFailure(
        reply.error!,
        terminal: reply.uiCode == 100 || reply.uiCode == 101,
        capacity: reply.uiCode == 101 || reply.uiCode == 102,
      );
    }
    throw _HostResponseError(reply.error!);
  }

  void _observeScheduler(
    host.Action action,
    host.ResponseReader reply, {
    Uint8List? expectedTask,
  }) {
    if (action == host.Action.serviceRunStart ||
        action == host.Action.serviceRunStatus) {
      final snapshot = ServiceRunCodec.snapshot(reply);
      _serviceRoute = snapshot.phase == ServiceRunPhase.exited
          ? null
          : snapshot;
      _serviceStopping =
          snapshot.phase == ServiceRunPhase.stopping ||
          snapshot.task.storage == IoStoragePhase.stopping;
      _serviceAdmissionUncertain = false;
      return;
    }
    if (action != host.Action.ioStatus &&
        action != host.Action.ioPoll &&
        action != host.Action.ioRead &&
        action != host.Action.ioCancel &&
        action != host.Action.ioRepair &&
        action != host.Action.ioAcknowledge) {
      return;
    }
    final snapshot = HttpTaskCodec.snapshot(reply.ioState);
    if (action == host.Action.ioAcknowledge) {
      if (snapshot.key != null || snapshot.storage != IoStoragePhase.local) {
        throw const FormatException('IO task acknowledgement incomplete');
      }
      if (expectedTask != null &&
          RustWorkbench._same(_serviceRoute?.task.key, expectedTask)) {
        _serviceRoute = null;
        _serviceStopping = false;
      }
    } else {
      if (expectedTask != null &&
          expectedTask.isNotEmpty &&
          !RustWorkbench._same(snapshot.key, expectedTask)) {
        throw const FormatException('IO task response identity changed');
      }
      final active = _serviceRoute;
      if (active != null &&
          RustWorkbench._same(snapshot.key, active.task.key!)) {
        switch (snapshot.storage) {
          case IoStoragePhase.local:
          case IoStoragePhase.reclaimed:
          case IoStoragePhase.recoveryRequired:
          case IoStoragePhase.unavailable:
            _serviceRoute = null;
            _serviceStopping = false;
          case IoStoragePhase.stopping:
            _serviceStopping = true;
          case IoStoragePhase.running:
            break;
        }
      }
    }
    if (snapshot.storage == IoStoragePhase.local && snapshot.key == null) {
      _serviceAdmissionUncertain = false;
    }
  }

  Future<T> _throughService<T>(
    host.Action action,
    ServiceRunSnapshot service, {
    void Function(host.RequestBuilder)? configure,
    required Duration requestTimeout,
    required T Function(host.ResponseReader) decode,
    required bool clearReply,
    required bool updatePresentation,
  }) async {
    final task = service.task.key!;
    final random = Random.secure();
    final submission = Uint8List.fromList(
      List.generate(32, (_) => random.nextInt(256)),
    );
    if (submission.every((v) => v == 0)) submission[0] = 1;
    final elapsed = Stopwatch()..start();
    Uint8List? command;
    var submitted = false;
    var deliveryFinished = false;

    ServiceCommandFailure failure(
      String message, {
      bool? unknown,
      OwnerCommandTerminal? terminal,
    }) => ServiceCommandFailure(
      message: message,
      task: task,
      submission: submission,
      command: command,
      outcomeUnknown: unknown ?? submitted,
      terminal: terminal,
    );

    Duration remaining() {
      if (_failure != null) {
        throw failure('Content service transport outcome is unknown');
      }
      if (_closingProcess != null) throw failure('Content service is closing');
      final budget = requestTimeout - elapsed.elapsed;
      if (budget <= Duration.zero) {
        throw failure('Service command timed out; reconcile before retrying');
      }
      return budget;
    }

    Future<R> control<R>(
      host.Action control, {
      required void Function(host.RequestBuilder) configure,
      required R Function(host.ResponseReader) decode,
    }) => _exchangeDecoded(
      control,
      configure: configure,
      decode: decode,
      requestTimeout: remaining(),
      remainingBudget: remaining,
      clearReply: true,
      updatePresentation: false,
    );

    bool sameRun() => RustWorkbench._same(_serviceRoute?.task.key, task);
    Future<void> pause() async {
      final budget = remaining();
      await Future<void>.delayed(
        budget < const Duration(milliseconds: 20)
            ? budget
            : const Duration(milliseconds: 20),
      );
      remaining();
    }

    try {
      var observed = service;
      while (observed.phase == ServiceRunPhase.starting) {
        if (!sameRun() || _serviceStopping) {
          throw failure(
            'Service stopped before command admission',
            unknown: false,
          );
        }
        observed = await control(
          host.Action.serviceRunStatus,
          configure: (r) => r.ioKey = task,
          decode: (r) => ServiceRunCodec.snapshot(r, expectedKey: task),
        );
        if (observed.phase == ServiceRunPhase.starting) await pause();
      }
      if (!sameRun() ||
          _serviceStopping ||
          observed.phase != ServiceRunPhase.running) {
        throw failure('Service is not accepting commands', unknown: false);
      }
      Uint8List? input;
      late OwnerCommandSnapshot status;
      try {
        await sendHostRequest(
          action,
          configure: configure,
          clearAfterSend: true,
          maxBytes: 64 * 1024,
          send: (bytes) async {
            input = Uint8List.fromList(bytes);
          },
        );
        remaining();
        // From the first attempted send onward, a transport/decoding failure
        // cannot prove that this command was never admitted. Do not replay it.
        submitted = true;
        try {
          status = await control(
            host.Action.commandSubmit,
            configure: (r) {
              r.ioKey = task;
              r.commandSubmission = submission;
              r.payload = input!;
            },
            decode: (r) =>
                ServiceRunCodec.command(r, expectedSubmission: submission),
          );
        } on _HostResponseError {
          // A valid clean admission rejection contains no accepted command.
          submitted = false;
          rethrow;
        }
      } finally {
        input?.fillRange(0, input!.length, 0);
      }
      command = status.key;
      while (true) {
        if (status.delivery != OwnerCommandDelivery.pending ||
            !sameRun() ||
            _serviceStopping) {
          final result = await control(
            host.Action.commandRead,
            configure: (r) {
              r.ioKey = task;
              r.commandKey = command!;
            },
            decode: (r) => ServiceRunCodec.read(r, expectedKey: command!),
          );
          try {
            status = result.snapshot;
            if (status.delivery == OwnerCommandDelivery.consumed) {
              deliveryFinished = true;
              final bytes = result.payload;
              if (bytes == null ||
                  status.terminal != OwnerCommandTerminal.none) {
                throw failure(
                  'Service command result unavailable; reconcile before retrying',
                  unknown: status.terminal != OwnerCommandTerminal.cancelled,
                  terminal: status.terminal,
                );
              }
              final reply = RustWorkbench.readMessage(
                bytes,
              ).getRoot(host.responseFactory);
              if (reply.version != 1 ||
                  !RustWorkbench._same(reply.digest, contract.hostDigest)) {
                throw failure(
                  'Service command response contract mismatch',
                  unknown: true,
                );
              }
              _checkBusinessReply(
                action,
                reply,
                updatePresentation: updatePresentation,
              );
              final decoded = decode(reply);
              // Typed sensitive decoders own their result before disposal. Raw
              // ResponseReader callers must retain the original backing bytes.
              if (!clearReply) result.takePayload();
              return decoded;
            }
          } finally {
            result.dispose();
          }
        }
        await pause();
        status = await control(
          host.Action.commandStatus,
          configure: (r) {
            r.ioKey = task;
            r.commandKey = command!;
          },
          decode: (r) => ServiceRunCodec.command(r, expectedKey: command!),
        );
      }
    } catch (error, stack) {
      if (submitted &&
          !deliveryFinished &&
          command != null &&
          _closingProcess == null &&
          _failure == null) {
        // A timeout stops delivery of this exact command. Cancellation may not
        // undo effects, and it must not delay the caller behind a lost pipe.
        unawaited(
          cancelServiceCommand(
            task,
            command,
          ).then<void>((_) {}, onError: (Object _, StackTrace _) {}),
        );
      }
      if (error is ServiceCommandFailure ||
          !submitted ||
          deliveryFinished &&
              (error is _HostResponseError || error is QueryFailure)) {
        Error.throwWithStackTrace(error, stack);
      }
      Error.throwWithStackTrace(
        failure(
          'Service command outcome is unknown; query its submission before retrying',
        ),
        stack,
      );
    }
  }
}
