@0xc9b3e8a44cf7d321;
# Independent runtime-only process control. No field creates authority.
struct Capabilities {
  read @0 :Bool; events @1 :Bool; write @2 :Bool; closeInput @3 :Bool;
  interrupt @4 :Bool; terminate @5 :Bool; resizePty @6 :Bool;
}
struct ReadQuery { afterSeq @0 :UInt64; maxBytes @1 :UInt32; maxEvents @2 :UInt16; waitMs @3 :UInt16; }
struct Size { rows @0 :UInt16; cols @1 :UInt16; }
struct Request {
  version @0 :UInt16; schemaSha256 @1 :Data; requestId @2 :Text;
  handle @3 :Data; generation @4 :UInt64;
  union {
    discover @5 :Void; read @6 :ReadQuery; events @7 :ReadQuery;
    write @8 :Data; closeInput @9 :Void; interrupt @10 :Void;
    terminate @11 :Void; resize @12 :Size;
  }
}
enum OutputStream { stdout @0; stderr @1; pty @2; }
struct Output { stream @0 :OutputStream; chunk @1 :Data; }
struct Exit { exitCode @0 :Int32; hasSandboxDenied @1 :Bool; sandboxDenied @2 :Bool; }
struct ProcessEvent {
  seq @0 :UInt64;
  union { output @1 :Output; exited @2 :Exit; closed @3 :Void; }
}
struct OutputPage {
  events @0 :List(ProcessEvent); nextSeq @1 :UInt64; floorSeq @2 :UInt64;
  gap @3 :Bool; exited @4 :Bool; hasExitCode @5 :Bool; exitCode @6 :Int32;
  closed @7 :Bool; hasFailure @8 :Bool; failure @9 :Text;
}
enum Failure {
  invalid @0; contract @1; limit @2; correlation @3; denied @4;
  conflict @5; notFound @6; unknown @7; unsupported @8; closed @9;
}
struct Reply {
  version @0 :UInt16; schemaSha256 @1 :Data; requestId @2 :Text;
  requestSha256 @3 :Data; generation @4 :UInt64;
  union { capabilities @5 :Capabilities; page @6 :OutputPage; accepted @7 :Void; rejected @8 :Failure; }
}
