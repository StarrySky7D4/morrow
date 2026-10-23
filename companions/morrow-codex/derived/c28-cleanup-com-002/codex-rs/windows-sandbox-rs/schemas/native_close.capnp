@0xd9a841e352b7c604;

# Derived Morrow Windows native-close protocol v8, typed Cap'n Proto wire revision 1.
# Runtime internal contracts are strictly typed Cap'n Proto; JSON is never accepted.
# Wire data conveys reported facts and correlation tags only; cleanup authority stays in host state machines.

enum WireFormatKind {
  unspecified @0;
  capnpTyped @1;
}

enum EnvelopeKind {
  unspecified @0;
  versionEightFrame @1;
  receiptAuthFacts @2;
  runtimePacketAuthBody @3;
}

struct NativeCloseEnvelope {
  magic @0 :UInt64;
  wireRevision @1 :UInt16;
  wireFormat @2 :WireFormatKind;
  envelopeKind @3 :EnvelopeKind;
  frame @4 :VersionEightWireFrame;
}

struct ReceiptAuthFactsEnvelope {
  magic @0 :UInt64;
  wireRevision @1 :UInt16;
  wireFormat @2 :WireFormatKind;
  envelopeKind @3 :EnvelopeKind;
  facts @4 :ReportedNativeCloseFacts;
}

struct RuntimePacketAuthBodyEnvelope {
  magic @0 :UInt64;
  wireRevision @1 :UInt16;
  wireFormat @2 :WireFormatKind;
  envelopeKind @3 :EnvelopeKind;
  body @4 :RuntimePacketAuthBody;
}

struct MaybeI32 {
  union {
    none @0 :Void;
    some @1 :Int32;
  }
}

struct MaybeAuthTag {
  union {
    none @0 :Void;
    some @1 :Data;
  }
}

struct WireChildIdentity {
  pid @0 :UInt32;
  threadId @1 :UInt32;
  creationTimeTicks @2 :UInt64;
}

struct RuntimeRunnerIdentity {
  processId @0 :UInt32;
  threadId @1 :UInt32;
  creationTimeTicks @2 :UInt64;
}

enum ObservationProof {
  notObserved @0;
  notProven @1;
  provenOnOriginal @2;
}

enum TimeoutDisposition {
  completedWithinBudget @0;
  timedOut @1;
}

enum StageOrderingFact {
  notObserved @0;
  notProven @1;
  provenInStrictOrder @2;
  orderViolated @3;
}

enum OriginalProcessWaitProof {
  notObserved @0;
  notProven @1;
  queryOnlyWithoutWaitSignal @2;
  waitTimedOutOrStillActive @3;
  signaledExitedOnOriginalHandle @4;
}

struct ChildNativeExitWitnessed {
  identity @0 :WireChildIdentity;
  rawExitCodeDword @1 :UInt32;
  businessExitCode @2 :Int32;
  waitOnOriginalHandle @3 :OriginalProcessWaitProof;
  timeoutDisposition @4 :TimeoutDisposition;
}

struct ChildNativeExitFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    notCreatedPreLaunch @2 :Void;
    waitTimedOutOrIncomplete @3 :WireChildIdentity;
    witnessed @4 :ChildNativeExitWitnessed;
  }
}

struct PipeEofFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    cleanEofObserved @2 :Void;
    readFailedBeforeEof @3 :MaybeI32;
  }
}

struct WorkerNativeJoinFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    stillRunning @2 :Void;
    joinedTimelyClean @3 :Void;
    joinedWithWorkerError @4 :Void;
    joinedLateAfterBudget @5 :Void;
    timedOutStillRunning @6 :Void;
    waitFailed @7 :MaybeI32;
    panicked @8 :Void;
  }
}

struct HandleCloseFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    closedVerifiedOnce @2 :Void;
    notApplicableAbsent @3 :Void;
    stillOpen @4 :Void;
    closeDebtRetained @5 :MaybeI32;
  }
}

struct OutputStreamWorkerFact {
  pipeEof @0 :PipeEofFact;
  readerJoin @1 :WorkerNativeJoinFact;
  handleClose @2 :HandleCloseFact;
}

struct StderrStreamWorkerFact {
  union {
    mergedConptyOrAbsent @0 :HandleCloseFact;
    separateStream @1 :OutputStreamWorkerFact;
  }
}

struct InputPeerEofFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    cleanPostChildExitFrameBoundaryEof @2 :Void;
    prematureDisconnectBeforeChildExit @3 :Void;
    partialFrameAtEof @4 :Void;
    frameTooLarge @5 :Void;
    malformedFrame @6 :Void;
    operationAborted @7 :Void;
    ioError @8 :MaybeI32;
  }
}

enum InputTerminationFact {
  noTerminationObserved @0;
  explicitTerminateReceived @1;
  terminatedOnDisconnect @2;
}

struct InputStreamWorkerFact {
  peerEof @0 :InputPeerEofFact;
  readerJoin @1 :WorkerNativeJoinFact;
  terminationDisposition @2 :InputTerminationFact;
}

struct IoStreamsAndWorkersFacts {
  stdout @0 :OutputStreamWorkerFact;
  stderr @1 :StderrStreamWorkerFact;
  input @2 :InputStreamWorkerFact;
}

enum StdinWriteDisposition {
  allWritesSucceeded @0;
  writeFailedBeforeClose @1;
}

struct StdinCloseDebtRetained {
  closeCount @0 :UInt32;
  osError @1 :MaybeI32;
}

struct StdinHandleCloseFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    initiallyNotOpen @2 :Void;
    closedOnceVerified @3 :StdinWriteDisposition;
    stillOpen @4 :Void;
    closedMultipleTimes @5 :UInt32;
    closeDebtRetained @6 :StdinCloseDebtRetained;
  }
}

enum ConptyClosureFact {
  notObserved @0;
  notProven @1;
  notUsedPipeMode @2;
  closedBeforeOutputJoinVerified @3;
  stillActiveOrPoisoned @4;
}

struct OriginalHandlesCloseFacts {
  conptyClosure @0 :ConptyClosureFact;
  stdinClosure @1 :StdinHandleCloseFact;
  childPrimaryThreadHandle @2 :HandleCloseFact;
  childProcessHandle @3 :HandleCloseFact;
  closeOrderingAfterIoJoins @4 :StageOrderingFact;
}

struct AdmittedVerifiedOriginalJob {
  jobInstanceId @0 :UInt64;
  childMembershipVerified @1 :ObservationProof;
}

struct JobAdmissionFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    jobMissing @2 :Void;
    admittedVerifiedOriginalJob @3 :AdmittedVerifiedOriginalJob;
  }
}

struct JobDrainedToActiveZero {
  totalProcesses @0 :UInt32;
  activeProcesses @1 :UInt32;
  totalTerminatedProcesses @2 :UInt32;
  orderingAfterChildHandleClose @3 :StageOrderingFact;
}

struct JobActiveZeroDrainFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    drainedToActiveZero @2 :JobDrainedToActiveZero;
    drainTimedOut @3 :UInt32;
    drainFailed @4 :MaybeI32;
  }
}

enum JobForbiddenOpFact {
  notInvoked @0;
  invokedViolation @1;
}

enum ExtraJobMembersOriginFact {
  notObserved @0;
  notProven @1;
  singleProcessObservedOnly @2;
  extraMembersObservedOriginNotProvenByJobHelper @3;
}

struct OriginalJobLifecycleFacts {
  admission @0 :JobAdmissionFact;
  activeZeroDrain @1 :JobActiveZeroDrainFact;
  extraMemberOrigin @2 :ExtraJobMembersOriginFact;
  preserveDescendants @3 :JobForbiddenOpFact;
  terminateJob @4 :JobForbiddenOpFact;
}

struct BusinessExitWriterFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    joinedTimelyBeforeReceipt @2 :Int32;
    stillRunning @3 :Void;
    writeOrJoinFailed @4 :Void;
    panicked @5 :Void;
  }
}

struct PriorControlWritersFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    allJoinedAndQuiesced @2 :UInt16;
    writerStillRunning @3 :Void;
    controlWriteFailed @4 :Void;
    precontextWriterRetained @5 :Void;
  }
}

enum SenderReceiptWriterFact {
  notObserved @0;
  notProven @1;
  pendingCurrentReceiptFlushAndClose @2;
  claimedJoinedBeforeSend @3;
  writeFailed @4;
}

struct WriterClosureFacts {
  businessExitWriter @0 :BusinessExitWriterFact;
  priorControlAndFrameWriters @1 :PriorControlWritersFact;
  senderReceiptWriter @2 :SenderReceiptWriterFact;
}

enum LeaseRetirementFact {
  notObserved @0;
  notProven @1;
  activeOrLoaned @2;
  parkedUnresolved @3;
  outOfRunnerScopePendingHostOwner @4;
  retiredCleanly @5;
}

struct AttemptLeaseRetirementFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    activeOrLoaned @2 :Void;
    parkedUnresolved @3 :Void;
    readyToRetireOnFinalReceiptFlush @4 :UInt64;
  }
}

struct LeaseRetirementFacts {
  runnerInputAndStdinLease @0 :LeaseRetirementFact;
  inputLease @1 :LeaseRetirementFact;
  keeperLease @2 :LeaseRetirementFact;
  quotaLease @3 :LeaseRetirementFact;
  attemptLease @4 :AttemptLeaseRetirementFact;
}

enum CloseDebtTarget {
  hostRunnerProcessHandle @0;
  hostInputPipeWriter @1;
  hostOutputPipeReader @2;
  hostStartupPipeAlias @3;
  hostWorkerThreadAnchor @4;
  hostDesktopHandle @5;
  hostDesktopToken @6;
  hostDesktopSecurityDescriptor @7;
  childProcessHandle @8;
  childPrimaryThreadHandle @9;
  childStdoutHandle @10;
  childStderrHandle @11;
  childStdinHandle @12;
  spawnPipeAuxiliaryHandle @13;
}

struct CloseDebtRetained {
  debtCount @0 :UInt8;
  firstTarget @1 :CloseDebtTarget;
  firstOsError @2 :MaybeI32;
}

struct CloseDebtFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    zeroDebtProven @2 :Void;
    debtRetained @3 :CloseDebtRetained;
  }
}

enum ProtocolStickyFailureKind {
  runtimeTransportFailed @0;
  startupValidationFailed @1;
  spawnChildFailed @2;
  writeSpawnReadyFailed @3;
  childWaitFailed @4;
  childExitCodeUnavailable @5;
  childTimedOut @6;
  explicitUserTerminate @7;
  conptyLockPoisoned @8;
  stdoutForwardingFailed @9;
  stdoutJoinTimedOut @10;
  stdoutWorkerPanicked @11;
  stderrForwardingFailed @12;
  stderrJoinTimedOut @13;
  stderrWorkerPanicked @14;
  businessExitGateViolation @15;
  businessExitWriteFailed @16;
  inputPartialFrame @17;
  inputFrameTooLarge @18;
  inputMalformedFrame @19;
  inputProtocolViolation @20;
  inputIoError @21;
  inputPrematureDisconnect @22;
  inputControlWriteFailed @23;
  inputStdinCloseFailed @24;
  inputJoinTimedOut @25;
  inputWorkerPanicked @26;
  childHandleCloseFailed @27;
  jobPreserveDescendantsInvoked @28;
  jobTerminateInvoked @29;
  jobDrainFailed @30;
  jobDrainTimedOut @31;
  ownerMutexPoisoned @32;
}

enum ProtocolFailureStage {
  startupAndHandshake @0;
  childWait @1;
  outputDrainAndJoin @2;
  businessExitWrite @3;
  inputDrainAndJoin @4;
  handleClosure @5;
  jobActiveZeroDrain @6;
  leaseRetirement @7;
}

enum ReplayDisposition {
  noReplaySealed @0;
  unsealedInvalid @1;
}

struct FirstErrorRecorded {
  kind @0 :ProtocolStickyFailureKind;
  stage @1 :ProtocolFailureStage;
  osError @2 :MaybeI32;
  replayPolicy @3 :ReplayDisposition;
}

struct FirstErrorFact {
  union {
    notObserved @0 :Void;
    noneObserved @1 :Void;
    recorded @2 :FirstErrorRecorded;
  }
}

enum PoisonTarget {
  childProcessOwnerMutex @0;
  conptyOwnerMutex @1;
  outputPipeWriterMutex @2;
  stdinCloseDebtMutex @3;
  stickyLatchMutex @4;
  attemptSlotMutex @5;
  hostOwnerMutex @6;
}

struct PoisonStateFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    noPoisonObserved @2 :Void;
    poisonObserved @3 :PoisonTarget;
  }
}

struct StickyUnknownStateFact {
  union {
    notObserved @0 :Void;
    notProven @1 :Void;
    clear @2 :Void;
    latchedStickyUnknown @3 :ReplayDisposition;
  }
}

struct DebtAndStickyUnknownFacts {
  closeDebt @0 :CloseDebtFact;
  firstError @1 :FirstErrorFact;
  poisonState @2 :PoisonStateFact;
  stickyUnknownState @3 :StickyUnknownStateFact;
}

struct ReportedNativeCloseFacts {
  childExit @0 :ChildNativeExitFact;
  ioStreamsAndWorkers @1 :IoStreamsAndWorkersFacts;
  handlesClose @2 :OriginalHandlesCloseFacts;
  jobLifecycle @3 :OriginalJobLifecycleFacts;
  writers @4 :WriterClosureFacts;
  leases @5 :LeaseRetirementFacts;
  debtAndStickyUnknown @6 :DebtAndStickyUnknownFacts;
}

enum NativeCloseCapabilityOptIn {
  explicitlyOptedInVersionEight @0;
  disabledOrNotRequested @1;
}

struct NativeCloseHandshakeHelloWire {
  protocolVersion @0 :UInt8;
  schema @1 :Data;
  startupAttemptId @2 :UInt64;
  capability @3 :NativeCloseCapabilityOptIn;
  authTag @4 :Data;
}

struct NativeCloseHandshakeAckWire {
  protocolVersion @0 :UInt8;
  schema @1 :Data;
  startupAttemptId @2 :UInt64;
  capability @3 :NativeCloseCapabilityOptIn;
  authTag @4 :Data;
}

struct NativeCloseSpawnReadyWire {
  startupAttemptId @0 :UInt64;
  childIdentity @1 :WireChildIdentity;
  authTag @2 :Data;
}

enum NativeCloseOutputStream {
  stdout @0;
  stderr @1;
}

struct NativeCloseOutputFrameWire {
  stream @0 :NativeCloseOutputStream;
  byteLen @1 :UInt32;
}

enum NativeCloseControlStatus {
  applied @0;
  rejected @1;
  unsupported @2;
  unknown @3;
}

struct NativeCloseControlResultWire {
  requestId @0 :UInt64;
  status @1 :NativeCloseControlStatus;
}

struct NativeCloseBusinessExitWire {
  exitCode @0 :Int32;
  timeoutDisposition @1 :TimeoutDisposition;
}

struct NativeCloseReceiptWire {
  protocolVersion @0 :UInt8;
  schema @1 :Data;
  startupAttemptId @2 :UInt64;
  sequenceNumber @3 :UInt8;
  facts @4 :ReportedNativeCloseFacts;
  authTag @5 :Data;
}

struct RuntimeBootstrapWire {
  schema @0 :Data;
  attempt @1 :UInt64;
  runner @2 :RuntimeRunnerIdentity;
  seed @3 :Data;
  hello @4 :NativeCloseHandshakeHelloWire;
  terminalCloseAckRequired @5 :Bool;
}

enum RuntimeDirection {
  hostToRunner @0;
  runnerToHost @1;
}

struct RuntimeTerminalAck {
  child @0 :WireChildIdentity;
  rawChildExitDword @1 :UInt32;
}

struct RuntimePacketContent {
  union {
    empty @0 :Void;
    runnerMessage @1 :Data;
    terminalAck @2 :RuntimeTerminalAck;
  }
}

struct RuntimePacketEventFrame {
  version @0 :UInt8;
  union {
    handshakeHello @1 :NativeCloseHandshakeHelloWire;
    handshakeAck @2 :NativeCloseHandshakeAckWire;
    spawnReady @3 :NativeCloseSpawnReadyWire;
    output @4 :NativeCloseOutputFrameWire;
    controlResult @5 :NativeCloseControlResultWire;
    businessExit @6 :NativeCloseBusinessExitWire;
    nativeCloseReceipt @7 :NativeCloseReceiptWire;
  }
}

struct MaybeRuntimePacketEvent {
  union {
    none @0 :Void;
    some @1 :RuntimePacketEventFrame;
  }
}

struct RuntimePacketWire {
  schema @0 :Data;
  attempt @1 :UInt64;
  sequence @2 :UInt64;
  direction @3 :RuntimeDirection;
  event @4 :MaybeRuntimePacketEvent;
  content @5 :RuntimePacketContent;
  terminalCloseAck @6 :MaybeAuthTag;
  tag @7 :Data;
}

struct RuntimePacketAuthBody {
  schema @0 :Data;
  attempt @1 :UInt64;
  sequence @2 :UInt64;
  direction @3 :RuntimeDirection;
  event @4 :MaybeRuntimePacketEvent;
  content @5 :RuntimePacketContent;
  terminalCloseAck @6 :MaybeAuthTag;
}

struct VersionEightWireFrame {
  version @0 :UInt8;
  union {
    runtimeBootstrap @1 :RuntimeBootstrapWire;
    runtimePacket @2 :RuntimePacketWire;
    handshakeHello @3 :NativeCloseHandshakeHelloWire;
    handshakeAck @4 :NativeCloseHandshakeAckWire;
    spawnReady @5 :NativeCloseSpawnReadyWire;
    output @6 :NativeCloseOutputFrameWire;
    controlResult @7 :NativeCloseControlResultWire;
    businessExit @8 :NativeCloseBusinessExitWire;
    nativeCloseReceipt @9 :NativeCloseReceiptWire;
  }
}
