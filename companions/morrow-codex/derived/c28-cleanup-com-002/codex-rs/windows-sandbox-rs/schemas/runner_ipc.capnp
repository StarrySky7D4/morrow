@0xf4b8dbfdc1a78b90;

# Derived Morrow Windows parent/Runner transport, wire revision 1.
# All peers must be built against this schema; JSON is never a fallback.
# Numeric control versions describe lifecycle semantics, not wire format.
struct Envelope {
  magic @0 :UInt64;
  wireRevision @1 :UInt16;
  schema @2 :Data;
  controlVersion @3 :UInt8;
  union {
    hello @4 :Hello;
    helloAck @5 :HelloAck;
    controlRequest @6 :ControlRequest;
    controlResult @7 :ControlResult;
    spawnRequest @8 :SpawnRequest;
    spawnReady @9 :SpawnReady;
    output @10 :Output;
    stdin @11 :Data;
    closeStdin @12 :Void;
    resize @13 :Resize;
    exit @14 :Exit;
    error @15 :Error;
    terminate @16 :Void;
  }
}
struct MaybeText { union { none @0 :Void; some @1 :Text; } }
struct MaybeU64 { union { none @0 :Void; some @1 :UInt64; } }
struct MaybeU32 { union { none @0 :Void; some @1 :UInt32; } }
struct MaybeI32 { union { none @0 :Void; some @1 :Int32; } }
struct Hello { nonce @0 :Data; schema @1 :Data; }
struct Controls { closeStdin @0 :Bool; resize @1 :Bool; }
struct HelloAck { hello @0 :Hello; controls @1 :Controls; }
enum ControlOperation { closeStdin @0; resize @1; }
enum ControlStatus { applied @0; rejected @1; unsupported @2; unknown @3; }
struct ControlRequest {
  id @0 :UInt64;
  nonce @1 :Data;
  operation @2 :ControlOperation;
  rows @3 :UInt16;
  cols @4 :UInt16;
  argsDigest @5 :Data;
}
struct ControlResult { request @0 :ControlRequest; status @1 :ControlStatus; osError @2 :MaybeI32; }
struct EnvEntry { key @0 :Text; value @1 :Text; }
enum Network { restricted @0; enabled @1; }
enum Access { read @0; write @1; deny @2; }
enum MissingPath { unspecified @0; skip @1; }
struct SpecialPath {
  union {
    root @0 :Void;
    minimal @1 :Void;
    projectRoots @2 :MaybeText;
    tmpdir @3 :Void;
    slashTmp @4 :Void;
    unknown @5 :UnknownPath;
  }
}
struct UnknownPath { path @0 :Text; subpath @1 :MaybeText; }
struct FsPath { union { uri @0 :Text; glob @1 :Text; special @2 :SpecialPath; } }
struct FsEntry { path @0 :FsPath; access @1 :Access; missing @2 :MissingPath; }
struct RestrictedFs { entries @0 :List(FsEntry); globScanMaxDepth @1 :MaybeU64; }
struct ManagedFs { union { restricted @0 :RestrictedFs; unrestricted @1 :Void; } }
struct ManagedProfile { fileSystem @0 :ManagedFs; network @1 :Network; }
struct PermissionProfile {
  union { managed @0 :ManagedProfile; disabled @1 :Void; external @2 :Network; }
}
struct SpawnRequest {
  command @0 :List(Text);
  cwd @1 :Text;
  env @2 :List(EnvEntry);
  permissionProfile @3 :PermissionProfile;
  workspaceRoots @4 :List(Text);
  codexHome @5 :Text;
  realCodexHome @6 :Text;
  capSids @7 :List(Text);
  networkProxyRestrictingSid @8 :MaybeText;
  timeoutMs @9 :MaybeU64;
  tty @10 :Bool;
  stdinOpen @11 :Bool;
  privateDesktopName @12 :MaybeText;
}
struct SpawnReady { processId @0 :UInt32; controls @1 :Controls; }
enum OutputStream { stdout @0; stderr @1; }
struct Output { data @0 :Data; stream @1 :OutputStream; }
struct Resize { rows @0 :UInt16; cols @1 :UInt16; }
struct Exit { exitCode @0 :Int32; timedOut @1 :Bool; }
enum ErrorStage { readSpawnRequest @0; spawnChild @1; writeSpawnReady @2; terminalWitness @3; }
struct Error { message @0 :Text; stage @1 :ErrorStage; windowsErrorCode @2 :MaybeU32; }
