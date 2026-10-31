@0xc59ba7c44c6a3ecf;

# Independent internal contract. No JSON, opaque JSON bytes, dynamic maps or
# authorization-by-deserialization. The original process owner validates facts.
struct Envelope {
  wireVersion @0 :UInt16;
  schemaSha256 @5 :Data;
  union {
    status @1 :Status;
    preview @2 :RecoveryPreview;
    recovery @3 :RecoveryResult;
    failure @4 :ManagementFailure;
  }
}

enum OwnerPhase { unspecified @0; pending @1; active @2; closing @3; released @4; recovered @5; }
enum StatusPhase { unspecified @0; running @1; closing @2; released @3; closingUnconfirmed @4; }
enum OwnerDisposition { unspecified @0; active @1; closing @2; released @3; retained @4; }
enum ObservedTruth { unspecified @0; no @1; yes @2; }
enum BusinessOutcome { unspecified @0; unknown @1; }
enum ProtocolAck { unspecified @0; notApplicable @1; }

struct Identity {
  profile @0 :Data;
  generation @1 :UInt64;
  incarnation @2 :Data;
  hostSha256 @3 :Data;
  packageSha256 @4 :Data;
  supervisorSha256 @5 :Data;
  supervisorPid @6 :UInt32;
  supervisorCreationFiletime @7 :UInt64;
  childPid @8 :UInt32;
  childCreationFiletime @9 :UInt64;
  recoveryHistoryHead @10 :Data;
}

struct ExitObservation {
  union { absent @0 :Void; code @1 :Int32; }
}

struct ResourceFacts {
  resourceReclaimed @0 :Bool;
  gateClosed @1 :Bool;
  treeEmpty @2 :Bool;
  childExit @3 :Bool;
  stdoutEof @4 :Bool;
  stderrEof @5 :Bool;
  controlReaped @6 :Bool;
  businessGateRevoked @7 :Bool;
  normalShutdown @8 :ObservedTruth;
  childExitCode @9 :ExitObservation;
}

struct ReaderJoins {
  version @0 :UInt16;
  stdoutReaderJoined @1 :Bool;
  stderrReaderJoined @2 :Bool;
  controllerReaderJoined @3 :Bool;
}

struct RecoveryBinding {
  version @0 :UInt16;
  originalDigest @1 :Data;
  receiptDigest @2 :Data;
  historyHead @3 :Data;
}

struct OwnerRecord {
  modelVersion @0 :UInt16;
  identity @1 :Identity;
  phase @2 :OwnerPhase;
  resources @3 :ResourceFacts;
  businessOutcome @4 :BusinessOutcome;
  protocolAck @5 :ProtocolAck;
  readerJoins @6 :ReaderJoins;
  recovery @7 :RecoveryBinding;
}

# A durable abnormal record remains Closing/Retained. This extension cannot
# establish supervisor exit, normal shutdown, release, or business completion.
struct AbnormalClosurePrepared {
  version @0 :UInt16;
  identity @1 :Identity;
  committedRecordSha256 @2 :Data;
  resources @3 :ResourceFacts;
  readerJoins @4 :ReaderJoins;
  durableAbnormalRecordConfirmed @5 :Bool;
  ownerDisposition @6 :OwnerDisposition;
  businessOutcome @7 :BusinessOutcome;
  protocolAck @8 :ProtocolAck;
  accepted @9 :Bool;
  supervisorExitConfirmed @10 :Bool;
}

struct Status {
  record @0 :OwnerRecord;
  phase @1 :StatusPhase;
  ownerDisposition @2 :OwnerDisposition;
  durableProof @3 :Bool;
  abnormalResourceClosureProtocol @4 :UInt16;
  prepared @5 :AbnormalClosurePrepared;
}

struct RecoveryPreview {
  profile @0 :Data;
  previewToken @1 :Data;
  originalDigest @2 :Data;
  historyHead @3 :Data;
  eligible @4 :Bool;
  reason @5 :Text;
  resourceRecoveryConfirmed @6 :Bool;
  recoveryOriginalDigest @7 :Data;
  record @8 :OwnerRecord;
  businessOutcome @9 :BusinessOutcome;
}

struct RecoveryResult {
  profile @0 :Data;
  requestToken @1 :Data;
  originalDigest @2 :Data;
  historyHead @3 :Data;
  recovered @4 :Bool;
  businessOutcome @5 :BusinessOutcome;
  normalShutdown @6 :ObservedTruth;
  record @7 :OwnerRecord;
}

struct ManagementFailure {
  eligible @0 :Bool;
  recovered @1 :Bool;
  reason @2 :Text;
  detail @3 :Text;
  businessOutcome @4 :BusinessOutcome;
}
