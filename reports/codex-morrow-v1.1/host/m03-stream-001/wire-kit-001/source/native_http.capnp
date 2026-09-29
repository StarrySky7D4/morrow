@0xba19abb12fecd3fd;
# Sole M03 native HTTP stream authority, major3 revision1.
# uint32 LE payload byte length + one unpacked Capnp message; 8..32768 aligned8.
# See README.md for direction, identity, approval, credit and orthogonal completion rules.
enum Kind {
  challenge @0;
  hello @1;
  welcome @2;
  query @3;
  state @4;
  denied @5;
  stop @6;
  close @7;
  dataOffer @8;
  dataBind @9;
  dataBound @10;
  httpPrepare @11;
  requestChunk @12;
  httpProposed @13;
  httpApproved @14;
  httpCommit @15;
  responseHead @16;
  bodyChunk @17;
  httpCredit @18;
  creditState @19;
  httpCancel @20;
  cancelAccepted @21;
  httpTerminal @22;
  requestClosed @23;
}
enum IntentPhase {
  absent @0;
  prepared @1;
  unknown @2;
  observed @3;
  cancelledBeforeDispatch @4;
}
enum NetworkPhase {
  idle @0;
  awaitingHead @1;
  streaming @2;
  eof @3;
  failed @4;
  cancelled @5;
}
enum OwnerPhase {
  preparing @0;
  active @1;
  revoked @2;
  closing @3;
  closingUnconfirmed @4;
  released @5;
}
struct Channel {
  locator @0 :Text;
  nonce @1 :Data;
  maxChunkBytes @2 :UInt32;
  creditLimit @3 :UInt32;
}
struct Header {
  name @0 :Text;
  value @1 :Data;
}
struct Prepare {
  method @0 :Text;
  absoluteTarget @1 :Text;
  headers @2 :List(Header);
  bodyBytes @3 :UInt32;
  bodySha256 @4 :Data;
  responseLimitBytes @5 :UInt32;
}
struct Chunk {
  offset @0 :UInt64;
  bytes @1 :Data;
}
struct Decision {
  proposalRef @0 :Data;
  bodySha256 @1 :Data;
  requestSha256 @2 :Data;
  httpGrantRef @3 :Data;
  endpointRef @4 :Data;
  bodyBytes @5 :UInt32;
  sendBudget @6 :UInt32;
  responseLimitBytes @7 :UInt32;
}
struct Head {
  status @0 :UInt16;
  headers @1 :List(Header);
  remoteAddress @2 :Text;
}
struct Credit {
  consumedOffset @0 :UInt64;
  parserYieldedBytes @1 :UInt64;
  drainDiscardedBytes @2 :UInt64;
  cancelDiscardedBytes @3 :UInt64;
  windowBytes @4 :UInt32;
  maxChunkBytes @5 :UInt32;
  errorConsumedBytes @6 :UInt64;
}
struct Progress {
  intent @0 :IntentPhase;
  network @1 :NetworkPhase;
  owner @2 :OwnerPhase;
  httpStatus @3 :UInt16;
  errorCode @4 :UInt32;
  receivedOffset @5 :UInt64;
  reservedOffset @6 :UInt64;
  issuedOffset @7 :UInt64;
  osCompletedOffset @8 :UInt64;
  peerConsumedOffset @9 :UInt64;
  parserYieldedBytes @10 :UInt64;
  drainDiscardedBytes @11 :UInt64;
  cancelDiscardedBytes @12 :UInt64;
  lastWriteOrdinal @13 :UInt64;
  revokePersisted @14 :Bool;
  revokeApplied @15 :Bool;
  httpEof @16 :Bool;
  responseMaterialStored @17 :Bool;
  workerJoined @18 :Bool;
  connectReaped @19 :Bool;
  readReaped @20 :Bool;
  writeReaped @21 :Bool;
  dataClosed @22 :Bool;
  childExited @23 :Bool;
  stdoutEof @24 :Bool;
  stderrEof @25 :Bool;
  ownerReleased @26 :Bool;
  workerStarted @27 :Bool;
  errorConsumedBytes @28 :UInt64;
  requestClosed @29 :Bool;
}
struct Frame {
  major @0 :UInt16;
  revision @1 :UInt16;
  kind @2 :Kind;
  sequence @3 :UInt64;
  session @4 :UInt64;
  instanceEpoch @5 :UInt64;
  revocationGeneration @6 :UInt64;
  childPid @7 :UInt32;
  code @8 :UInt32;
  remainingMs @9 :UInt64;
  nonce @10 :Data;
  schemaSha256 @11 :Data;
  artifactSha256 @12 :Data;
  executionConfigSha256 @13 :Data;
  requestBudget @14 :UInt64;
  capabilities @15 :UInt64;
  reserved @16 :UInt64;
  operationId @17 :Data;
  attempt @18 :UInt64;
  payload :union {
    none @19 :Void;
    channel @20 :Channel;
    prepare @21 :Prepare;
    chunk @22 :Chunk;
    decision @23 :Decision;
    head @24 :Head;
    credit @25 :Credit;
    progress @26 :Progress;
  }
}
