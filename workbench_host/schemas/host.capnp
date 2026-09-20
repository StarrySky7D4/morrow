@0xeefcf786d6838bda;
# Private trusted UI/host connection. Native selected paths never reach a guest.
enum Action { read @0; page @1; mutate @2; importFile @3; exportFile @4; service @5; query @6; readPreferences @7; savePreferences @8; capture @9; beginPreferences @10; appendPreferences @11; finishPreferences @12; abortPreferences @13; readPreferencesPart @14; backupProtection @15; backupSnapshot @16; pluginState @17; pluginConfigure @18; uiOpen @19; uiEvent @20; uiClose @21; openCaptureScope @22; closeCaptureScope @23; beginCaptureUpload @24; appendCaptureUpload @25; finishPaste @26; finishCapturedSave @27; abortCaptureUpload @28; pluginCatalog @29; pluginInspect @30; pluginImport @31; pluginApprove @32; pluginRemove @33; pluginTransform @34; externalUiOpen @35; externalUiEvent @36; externalUiClose @37; readUiLocale @38; saveUiLocale @39; pluginApproveIo @40; credentialPage @41; credentialSave @42; credentialDisable @43; endpointPage @44; endpointSave @45; endpointDisable @46; httpStart @47; ioStatus @48; ioPoll @49; ioRead @50; ioCancel @51; ioRepair @52; ioAcknowledge @53; }
struct Request {
 version @0 :UInt16; digest @1 :Data; action @2 :Action;
 id @3 :Text; operation @4 :Text; revision @5 :UInt64;
 payload @6 :Data; selectedPath @7 :Text; name @8 :Text; kind @9 :Text;
 cursor @10 :Text; limit @11 :UInt32; attachment @12 :Text;
 transfer @13 :Text; offset @14 :UInt64; totalLength @15 :UInt64; sha256 @16 :Data;
 captureScope @17 :Text; captureParent @18 :Text;
 approvedCapabilities @19 :List(Text); handler @20 :Text; inputType @21 :Text; outputType @22 :Text;
 catalogRevisionBound @23 :Bool;
 approvedIoCapabilities @24 :List(Text);
 credentialReference @25 :Data; credentialSnapshot @26 :Data; credentialCursor @27 :Data;
 credentialHeader @28 :Text; credentialSecret @29 :Text; credentialDays @30 :UInt32;
 endpointCursor @31 :Data; endpointSnapshot @32 :Data; endpointReference @33 :Data;
 endpointRegistryRevision @34 :UInt64; endpointDays @35 :UInt32; endpointPolicy @36 :EndpointPolicy;
 httpStart @37 :HttpStart; ioKey @38 :Data;
}
struct Response {
 version @0 :UInt16; digest @1 :Data; payload @2 :Data;
 revision @3 :UInt64; ids @4 :List(Text); cursor @5 :Text;
 readOnly @6 :Bool; error @7 :Text;
 transfer @8 :Text; offset @9 :UInt64; totalLength @10 :UInt64; sha256 @11 :Data;
 maintenanceWarning @12 :Text;
 uiView @13 :Text; uiGeneration @14 :UInt64; uiSerial @15 :UInt64;
 uiFailure @16 :Text; uiCode @17 :UInt16;
 pluginEnabled @18 :Bool; pluginApproved @19 :Bool; pluginAvailable @20 :Bool;
 captureScope @21 :Text; captureTicket @22 :Text;
 plugins @23 :List(PluginEntry);
 credentials @24 :List(CredentialInfo); credentialSnapshot @25 :Data; credentialCursor @26 :Data;
 endpoints @27 :List(EndpointInfo); endpointSnapshot @28 :Data; endpointCursor @29 :Data;
 ioState @30 :IoState; ioResult @31 :IoResult;
}

# Trusted editor observations. Selection offsets count UTF-16 code units, not UTF-8 bytes.
struct PastePart { ticket @0 :Text; literal @1 :Text; selection @2 :Text; }
struct PasteEvent {
 id @0 :Text; field @1 :Text; before @2 :Text;
 startUtf16 @3 :UInt32; endUtf16 @4 :UInt32;
 parts @5 :List(PastePart); after @6 :Text;
}
struct PasteUpload { scope @0 :Text; event @1 :PasteEvent; }
struct AttachmentAlias { id @0 :Text; location @1 :Text; name @2 :Text; }
struct EditorSnapshot {
 title @0 :Text; description @1 :Text; hypothesis @2 :Text;
 conclusion @3 :Text; todos @4 :Text; aliases @5 :List(AttachmentAlias);
}
struct CapturedSave {
 scope @0 :Text; operation @1 :Text; target @2 :Text; revision @3 :UInt64;
 payload @4 :Data; snapshot @5 :EditorSnapshot;
}

# Selected-package management on the private trusted application channel only.
# These descriptors grant no runtime capability and are not a guest SDK contract.
struct PluginHandler {
 name @0 :Text; inputType @1 :Text; outputType @2 :Text;
 maxInputBytes @3 :UInt32; maxOutputBytes @4 :UInt32;
}
struct PluginEntry {
 packageId @0 :Text; name @1 :Text; packageVersion @2 :Text; digest @3 :Data;
 enabled @4 :Bool; builtin @5 :Bool; available @6 :Bool;
 declared @7 :List(Text); approved @8 :List(Text);
 handlers @9 :List(PluginHandler); dependencies @10 :List(Text); issue @11 :Text;
 declaredIo @12 :List(Text); approvedIo @13 :List(Text);
 ioHandlers @14 :List(Text);
}

# Redacted administration metadata only. No ciphertext or secret readback.
struct CredentialInfo {
 reference @0 :Data; revision @1 :UInt64; createdMs @2 :UInt64;
 expiresMs @3 :UInt64; disabled @4 :Bool;
}

# Trusted UI policy administration; these values do not restore active grants.
struct EndpointPolicy {
 packageId @0 :Text; packageDigest @1 :Data; origin @2 :Text; profile @3 :UInt16;
 methods @4 :List(Text); credentialReference @5 :Data; rootCertificate @6 :Data;
 maxRequestBytes @7 :UInt32; maxResponseBytes @8 :UInt32; maxHeaderBytes @9 :UInt32;
 maxConcurrent @10 :UInt16; timeoutMs @11 :UInt32; maxFrameBytes @12 :UInt32;
}
struct EndpointInfo {
 reference @0 :Data; revision @1 :UInt64; createdMs @2 :UInt64; expiresMs @3 :UInt64;
 disabled @4 :Bool; policy @5 :EndpointPolicy;
}

# Experimental HTTP-forward input profile; no destination or secret is supplied here.
struct HttpHeader { name @0 :Text; value @1 :Data; }
struct HttpStart {
 submission @0 :Data; endpoint @1 :Data; endpointRevision @2 :UInt64;
 packageDigest @3 :Data; registryRevision @4 :UInt64;
 method @5 :Text; target @6 :Text; headers @7 :List(HttpHeader); body @8 :Data; timeoutMs @9 :UInt32;
}
# 0 local, 1 running, 2 stopping, 3 reclaimed, 4 recoveryRequired, 5 unavailable.
# delivery: 0 absent, 1 pending, 2 ready, 3 consumed, 4 unavailable.
struct IoState {
 key @0 :Data; submission @1 :Data; storage @2 :UInt16; delivery @3 :UInt16;
 hasExit @4 :Bool; execution @5 :UInt16; disconnect @6 :UInt16; maintenance @7 :UInt16;
}
struct IoResult {
 present @0 :Bool; cancelled @1 :Bool; unknown @2 :Bool; calls @3 :UInt32; chargedBytes @4 :UInt64;
 executionFault @5 :UInt16; exitCode @6 :Int32;
 hasHttp @7 :Bool; status @8 :UInt16; httpStatus @9 :UInt16; headers @10 :List(HttpHeader); body @11 :Data;
}
