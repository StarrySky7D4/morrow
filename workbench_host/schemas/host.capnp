@0xeefcf786d6838bda;
# Private trusted UI/host connection. Native selected paths never reach a guest.
enum Action { read @0; page @1; mutate @2; importFile @3; exportFile @4; service @5; query @6; readPreferences @7; savePreferences @8; capture @9; beginPreferences @10; appendPreferences @11; finishPreferences @12; abortPreferences @13; readPreferencesPart @14; backupProtection @15; backupSnapshot @16; pluginState @17; pluginConfigure @18; uiOpen @19; uiEvent @20; uiClose @21; openCaptureScope @22; closeCaptureScope @23; beginCaptureUpload @24; appendCaptureUpload @25; finishPaste @26; finishCapturedSave @27; abortCaptureUpload @28; pluginCatalog @29; pluginInspect @30; pluginImport @31; pluginApprove @32; pluginRemove @33; pluginTransform @34; externalUiOpen @35; externalUiEvent @36; externalUiClose @37; readUiLocale @38; saveUiLocale @39; pluginApproveIo @40; credentialPage @41; credentialSave @42; credentialDisable @43; endpointPage @44; endpointSave @45; endpointDisable @46; httpStart @47; ioStatus @48; ioPoll @49; ioRead @50; ioCancel @51; ioRepair @52; ioAcknowledge @53; serviceConfigPage @54; serviceConfigSave @55; serviceConfigDisable @56; serviceAuthorityPage @57; serviceAuthenticationIssue @58; serviceAuthorityDisable @59; servicePublicationSave @60; serviceRunStart @61; serviceRunStatus @62; commandSubmit @63; commandStatus @64; commandRead @65; commandCancel @66; commandFrameBegin @67; commandFrameAppend @68; commandFrameFinish @69; commandFrameAbort @70; serviceTlsInspect @71; tlsIdentityPage @72; tlsIdentitySave @73; tlsIdentityDisable @74; readUiFont @75; saveUiFont @76; pendingPreferences @77; acknowledgePreferences @78; abandonPreferences @79; readVersioned @80; pageVersioned @81; planTasksMigration @82; migrateTasks @83; editTasks @84; editCard @85; queryVersioned @86; finishCapturedCard @87; inspectEditorRecoveries @88; resumeEditorRecovery @89; acknowledgeEditorRecovery @90; abandonEditorRecovery @91; beginEditorDraft @92; appendEditorDraft @93; finishEditorDraft @94; abortEditorDraftTransfer @95; readEditorDraft @96; readEditorDraftPart @97; listEditorDrafts @98; discardEditorDraft @99; importEditorDraftAsset @100; exportEditorDraftAsset @101; beginEditorDraftImport @102; completeEditorDraftImport @103; inspectEditorDraftImport @104; listEditorDraftImports @105; exportEditorDraftImport @106; abandonEditorDraftImport @107; reconcileEditorDraftImports @108; prepareEditorDraftImportDecision @109; inspectEditorDraftImportDecision @110; listEditorDraftImportDecisions @111; cancelEditorDraftImportDecision @112; listEditorDraftImportDecisionScopes @113; finishEditorDraftHandoff @114; retireEditorDraftParent @115; listEditorDraftLineages @116; prepareEditorDraftHandoffProposal @117; inspectEditorDraftHandoffProposal @118; listEditorDraftHandoffProposals @119; completeEditorDraftHandoffProposal @120; retireEditorDraftHandoffProposal @121; cancelEditorDraftHandoffProposal @122; inspectEditorCommit @123; fileStart @124; fileChunk @125; fileFinish @126; fileRead @127; mutationStart @128; mutationSubmit @129; mutationStatus @130; mutationRead @131; mutationCancelCommand @132; mutationReconcile @133; mutationDiscover @134; guestMutationStart @135; guestMutationSubmit @136; guestMutationStatus @137; guestMutationRead @138; guestMutationCancelCommand @139; channelPrepare @140; channelAppend @141; channelRun @142; channelStatus @143; channelClose @144; channelReadSent @145; }
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
 serviceConfig @39 :ServiceConfigUpdate; servicePublication @40 :ServicePublicationUpdate;
 serviceReference @41 :Data; serviceSnapshot @42 :Data; serviceCursor @43 :Data;
 principalId @44 :Text; serviceDays @45 :UInt32;
 serviceRun @46 :ServiceRunStart; commandKey @47 :Data; commandSubmission @48 :Data;
 serviceTls @49 :ServiceTlsSelection;
 uiFont @50 :UiFont;
 fileStart @51 :FileStart;
 mutationStart @52 :MutationStart; mutationCommand @53 :MutationCommand;
 mutationCommandId @54 :UInt64; mutationReconcile @55 :MutationReconcile; mutationDiscover @56 :MutationDiscover; guestMutationStart @57 :GuestMutationStart; guestMutationCommand @58 :GuestMutationCommand;
 channelPrepare @59 :ChannelPrepare; channelAppend @60 :ChannelAppend; channelRun @61 :ChannelRun;
 channelKey @62 :Data; channelSequence @63 :UInt64; channelOffset @64 :UInt32; channelLimit @65 :UInt32;
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
 serviceConfigs @32 :List(ServiceConfigInfo); serviceAuthorities @33 :List(ServiceAuthorityInfo);
 serviceSnapshot @34 :Data; serviceCursor @35 :Data;
 issuedToken @36 :Data;
 serviceRun @37 :ServiceRunState; ownerCommand @38 :OwnerCommandState;
 serviceTls @39 :ServiceTlsSelection;
 tlsIdentities @40 :List(TlsIdentityInfo);
 uiFont @41 :UiFont;
 preferencesOperation @42 :Text;
 # Derived from the library's original operation/evidence, never a guest claim.
 preferencesCommitted @43 :Bool;
 preferencesConflict @44 :Bool;
 editorRecoveries @45 :List(EditorRecovery);
 editorCommitProof @46 :EditorCommitProof;
 fileResult @47 :FileResult;
 mutationState @48 :MutationState; mutationResult @49 :MutationResult;
 mutationCommandId @50 :UInt64; guestMutationState @51 :GuestMutationState; guestMutationResult @52 :GuestMutationResult;
 channelState @53 :ChannelState; channelSent @54 :ChannelSent;
}

struct EditorCommitProof {
 id @0 :Text; operation @1 :Text; digest @2 :Data;
 sourceRevision @3 :UInt64; committedRevision @4 :UInt64;
}

# Private application lifecycle, not a guest capability. Every start explicitly
# binds current desired-state revisions and ceilings; no saved state auto-starts.
struct ServiceRunStart {
 submission @0 :Data; configId @1 :Text; configDigest @2 :Data; configRevision @3 :UInt64;
 publication @4 :Data; publicationRevision @5 :UInt64;
 packageId @6 :Text; packageDigest @7 :Data; registryRevision @8 :UInt64;
 lifetimeMs @9 :UInt32; maxJobs @10 :UInt64; maxBytes @11 :UInt64;
 maxCalls @12 :UInt32; maxJobBytes @13 :UInt64; maxTotalBytes @14 :UInt64;
 maxRequestBytes @15 :UInt32; maxResponseBytes @16 :UInt32;
 maxHeaderBytes @17 :UInt32; maxConcurrent @18 :UInt16; timeoutMs @19 :UInt32;
 outbound @20 :List(ServiceEndpointSelection);
 tls @21 :ServiceTlsSelection;
 protectedTls @22 :ProtectedTlsIdentityRef;
}
struct ServiceTlsSelection { certificatePath @0 :Text; privateKeyPath @1 :Text; certificateSha256 @2 :Data; validity @3 :ServiceTlsValidity; }
struct ServiceTlsValidity { notBeforeSeconds @0 :Int64; notAfterSeconds @1 :Int64; }
struct ServiceEndpointSelection { reference @0 :Data; revision @1 :UInt64; }
# phase 0 starting, 1 running, 2 stopping, 3 actually reclaimed.
# optional outcomes: 0 pending, 1 success, 2 invalid, 3 denied, 4 limit,
# 5 cancelled, 6 timeout, 7 transport, 8 closed.
struct ServiceRunState {
 task @0 :IoState; submission @1 :Data; phase @2 :UInt16; address @3 :Text;
 bind @4 :UInt16; listener @5 :UInt16; supervision @6 :UInt16;
}
# delivery 0 pending, 1 ready (read still rechecks authority), 2 consumed.
# terminal 0 absent, 1 busy, 2 closed, 3 limit, 4 cancelled, 5 unknown, 6 consumed.
# A consumed success has terminal 0. No read response is replayable.
# commandRead alone permits a 256 KiB outer frame; its payload is one original
# private business response of at most 128 KiB. All other frames remain 128 KiB.
struct OwnerCommandState {
 key @0 :Data; submission @1 :Data; delivery @2 :UInt16;
 started @3 :Bool; terminal @4 :UInt16;
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
 mutationSupported @15 :Bool;
 mutationBudget @16 :MutationBudget;
 channelSupported @17 :Bool; channelHandlers @18 :List(Text); channelKinds @19 :List(UInt16);
 channelBudget @20 :ChannelBudget;
}
struct MutationBudget { maxJobBytes @0 :UInt64; maxBytes @1 :UInt64; }

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

# Service desired state only. No raw authentication verifiers cross this channel.
struct ServiceContentScope { kind @0 :UInt16; cardId @1 :Text; attachmentId @2 :Text; }
struct ServicePrincipal { id @0 :Text; authenticationReference @1 :Data; scopes @2 :List(ServiceContentScope); }
struct ServiceConfigUpdate {
 id @0 :Text; expectedRevision @1 :UInt64; registryRevision @2 :UInt64;
 packageId @3 :Text; packageDigest @4 :Data; service @5 :Text; handler @6 :Text;
 retentionMs @7 :UInt64; principals @8 :List(ServicePrincipal);
}
struct ServiceConfigInfo {
 id @0 :Text; revision @1 :UInt64; namespace @2 :Data; retentionMs @3 :UInt64;
 service @4 :Text; handler @5 :Text; packageDigest @6 :Data; disabled @7 :Bool;
 principals @8 :List(ServicePrincipal); approvalReferences @9 :List(Data); digest @10 :Data;
}
struct ServicePublication {
 configId @0 :Text; configDigest @1 :Data; listenAddress @2 :Text; tlsRequired @3 :Bool;
 method @4 :Text; path @5 :Text; queryPath @6 :Text;
}
struct ServicePublicationUpdate {
 reference @0 :Data; expectedRevision @1 :UInt64; configRevision @2 :UInt64;
 registryRevision @3 :UInt64; packageId @4 :Text; lifetimeDays @5 :UInt32;
 policy @6 :ServicePublication;
}
# kind 1 authentication, 2 publication. Token is returned separately on issue only.
struct ServiceAuthorityInfo {
 reference @0 :Data; revision @1 :UInt64; createdMs @2 :UInt64; expiresMs @3 :UInt64;
 disabled @4 :Bool; kind @5 :UInt16; principalId @6 :Text; publication @7 :ServicePublication;
}

# CommandFrame actions run only inside the original owner-command lane.
# transfer is a caller random 64-lowerhex correlation token; begin/finish bind
# totalLength and sha256. append binds offset and <=32 KiB payload.
# One <=128 KiB upload, fixed 120s TTL; finish validates a complete nonscheduler
# request then consumes once. Each outer command remains <=64 KiB.

struct ProtectedTlsIdentityRef { reference @0 :Data; revision @1 :UInt64; certificateSha256 @2 :Data; }
struct TlsIdentityInfo { choice @0 :ProtectedTlsIdentityRef; disabled @1 :Bool; }

# Presentation metadata only; font bytes remain in the local asset store.
struct UiFont { family @0 :Text; asset @1 :Text; name @2 :Text; }

# Private format-aware common-card editor save. TaskId operations remain separate.
struct CapturedCardSave {
 scope @0 :Text; operation @1 :Text; target @2 :Text; revision @3 :UInt64;
 payload @4 :Data; snapshot @5 :EditorSnapshot;
}

# Host-owned pending captured edits. Inspection never replays the operation.
struct EditorRecovery { id @0 :Text; operation @1 :Text; digest @2 :Data; sourceRevision @3 :UInt64; currentRevision @4 :UInt64; title @5 :Text; status @6 :UInt16; }

# Trusted native selection only. No guest-visible path or persistent file grant.
# Start is once per nonzero random submission. Lost replies are inspected via
# ioStatus; no automatic restart. Chunk uses ioKey, offset and limit (0 = 64 KiB).
struct FileStart {
 submission @0 :Data; packageId @1 :Text; packageDigest @2 :Data;
 registryRevision @3 :UInt64; handler @4 :Text; selectedPath @5 :Text;
 maxBytes @6 :UInt64; timeoutMs @7 :UInt32;
}
# kind: 0 pending, 1 captured, 2 chunk, 3 finished. One delivery per command.
# Captured exposes retained length/hash, never the guest grant reference.
struct FileResult {
 kind @0 :UInt16; length @1 :UInt64; sha256 @2 :Data;
 offset @3 :UInt64; bytes @4 :Data; eof @5 :Bool;
}

# Trusted private file mutation. Request.ioKey carries the task key.
# disposition: 1 create, 2 replace, 3 delete. For create, selectedPath is the root.
struct MutationStart {
 submission @0 :Data; packageId @1 :Text; packageDigest @2 :Data;
 registryRevision @3 :UInt64; disposition @4 :UInt16;
 selectedPath @5 :Text; relativePath @6 :Text; subject @7 :Text;
 approvalSha256 @8 :Data; timeoutMs @9 :UInt32;
}
# Private history-start request. plan is the raw Core RequestRecord container,
# never an OS path; reconciliation still rechecks the current package binding.
struct MutationReconcile {
 submission @0 :Data; packageId @1 :Text; packageDigest @2 :Data;
 registryRevision @3 :UInt64; plan @4 :Data; timeoutMs @5 :UInt32;
}
# kind: 1 prepare, 2 chunk, 3 commit content, 4 execute, 5 query,
# 6 cancel plan, 7 release, 9 build plan, 10 next plans (scanLimit only). plan encodes RequestRecord.
# Build plan accepts only operationId/contentLength/contentSha256; target and
# approval metadata come from the original retained selection. No persistence.
struct MutationDiscover {
 submission @0 :Data; packageId @1 :Text; packageDigest @2 :Data;
 registryRevision @3 :UInt64; subject @4 :Text; disposition @5 :UInt16;
 scanLimit @6 :UInt16; timeoutMs @7 :UInt32; checkpoint @8 :Data;
}
struct MutationCommand {
 submission @0 :Data; kind @1 :UInt16; plan @2 :Data;
 offset @3 :UInt64; bytes @4 :Data;
 operationId @5 :Text; contentLength @6 :UInt64; contentSha256 @7 :Data; scanLimit @8 :UInt16;
}
# kind: 0 select, then the MutationCommand kinds above, 8 reconcile.
# delivery: 0 pending, 1 ready, 2 consumed.
struct MutationState {
 command @0 :UInt64; kind @1 :UInt16; delivery @2 :UInt16;
 selected @3 :Bool; reconcileRequired @4 :Bool; terminal @5 :Bool;
 reference @6 :Data; expectedIdentity @7 :Data;
}
# kind: 0 pending, 1 selected, 2 prepared, 3 staged, 4 created,
# 5 deleted, 6 history, 7 released, 8 plan cancelled, 9 failure,
# 10 reconciled, 11 planned (draft Core RequestRecord; no durable phase),
# 12 plans (original plans/scanned/done only; no phase or effect projection).
# failureLayer: 1 delivery, 2 target. record/outcome are Core containers.
# phase: 0 no record, 1 prepared, 2 outcome unknown, 3 observed,
# 4 cancelled before dispatch. The projection is authoritative for display.
# effect: 0 no effect projection, 1 OS confirmed success, 2 OS rejected.
# Result kind 4/5 alone does not establish that an OS mutation succeeded.
struct MutationResult {
 kind @0 :UInt16; reference @1 :Data; expectedIdentity @2 :Data;
 record @3 :Data; outcome @4 :Data; stagedBytes @5 :UInt64;
 durableContent @6 :Bool; failureLayer @7 :UInt16; failureCode @8 :UInt16;
 phase @9 :UInt16; operationId @10 :Text;
 effect @11 :UInt16; osCode @12 :UInt32; plan @13 :Data;
 plans @14 :List(Data); scanned @15 :UInt32; done @16 :Bool; checkpoint @17 :Data;
}

# Guest mutation is a separate trusted private domain; tokens remain in the host.
struct GuestMutationStart { selection @0 :MutationStart; approvedBudget @1 :MutationBudget; }
struct GuestMutationCommand {
 submission @0 :Data; kind @1 :UInt16; planSha256 @2 :Data;
 offset @3 :UInt64; bytes @4 :Data; operationId @5 :Text;
 contentLength @6 :UInt64; contentSha256 @7 :Data;
}
# delivery: 0 pending, 1 ready, 2 consumed, 3 unavailable.
struct GuestMutationState {
 command @0 :UInt64; kind @1 :UInt16; delivery @2 :UInt16; selected @3 :Bool;
 reference @4 :Data; expectedIdentity @5 :Data; reviewedPlanSha256 @6 :Data;
 approvalDelivered @7 :Bool; permitDelivered @8 :Bool; stagedBytes @9 :UInt64;
 durableContent @10 :Bool; effectAttempted @11 :Bool; reconcileRequired @12 :Bool; terminal @13 :Bool;
}
# kind: 0 pending, 1 owner, 2 exact Core response frame, 3 failure.
# failureKind: 1 job, 2 guest execution, 3 protocol, 4 cancelled.
struct GuestMutationResult {
 kind @0 :UInt16; owner @1 :MutationResult; frame @2 :Data;
 failureKind @3 :UInt16; failureCode @4 :UInt16;
}

# Trusted local caller-supplied channel data. These structs grant no path, SQL,
# network, credential or cloud authority. Each operation is <=128 KiB.
struct ChannelBudget {
 maxChannels @0 :UInt32; maxFrameBytes @1 :UInt32; maxBytes @2 :UInt64;
 maxMessages @3 :UInt64; maxRequests @4 :UInt64; maxDurationMs @5 :UInt64;
}
struct ChannelPrepare {
 submission @0 :Data; packageId @1 :Text; packageDigest @2 :Data;
 registryRevision @3 :UInt64; handler @4 :Text; kind @5 :UInt16;
 duplex @6 :Bool; budget @7 :ChannelBudget; lifetimeMs @8 :UInt32;
 frameCount @9 :UInt32; totalBytes @10 :UInt64;
}
struct ChannelAppend { key @0 :Data; sequence @1 :UInt64; bytes @2 :Data; cursor @3 :Data; }
struct ChannelRun { key @0 :Data; input @1 :Data; }
# phase: 0 loading/prepared, 1 ready, 2 running, 3 completed, 4 closing, 5 closed.
# status uses public channel Status numbers 0..12.
# cleanupProof: 0 pending, 1 no producer existed, 2 actual producer joined.
# producerOutcome: 0 pending, 1 explicit EOF, 2 unknown.
# taskState: 0 pending, 1 success, 2 guest failure, 3 runtime fault, 4 unknown.
# Task completion, cursor ACK, peer observation and two actual joins are distinct.
# snapshotPending preserves the last original observation while a queue is busy.
struct ChannelState {
 key @0 :Data; submission @1 :Data; directory @2 :Data;
 reference @3 :Data; sourceEpoch @4 :Data; phase @5 :UInt16;
 status @6 :UInt16; lastAcked @7 :UInt64; acceptedSequence @8 :UInt64;
 observedSequence @9 :UInt64; cleanupProof @10 :UInt16; producerOutcome @11 :UInt16;
 taskState @12 :UInt16; taskError @13 :Text; outputType @14 :Text; output @15 :Data;
 inputSha256 @16 :Data; uploadedFrames @17 :UInt32; uploadedBytes @18 :UInt64;
 sourceFrames @19 :UInt32; sourceBytes @20 :UInt64; resourceReclaimed @21 :Bool;
 closeRequested @22 :Bool; observedBytes @23 :UInt64; observedSha256 @24 :Data;
 workerJoined @25 :Bool; snapshotPending @26 :Bool;
}
# Original bytes consumed by the actual native peer, retained under the same
# finite local grant. Reading this history never retransmits a Send.
# Sent receipt chunks retain SHA-256 of the complete original payload, not a public Frame digest.
struct ChannelSent { present @0 :Bool; sequence @1 :UInt64; bytes @2 :Data; offset @3 :UInt32; totalBytes @4 :UInt32; bytesSha256 @5 :Data; }
