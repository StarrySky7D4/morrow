"""Read explicit local evidence only; emit a closed scalar projection.

No network, subprocess, VM, credential, PE, Start, retry, cleanup or write API.
Never emit input paths, raw payloads, messages or exception text.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

VERSION = "C28_NATIVE_DIAGNOSTIC_V3_SCALAR_PROJECTION_001"
MAX_JOURNAL = 33554432
MAX_RECORD = 131072
MAX_CONTROLLER = 4096
MAX_CONTROLLER_FILES = 64

WORKER_STAGES = frozenset("NOT_OBSERVED QUEUED RECEIVED COMMAND_STARTED CHECKPOINT OWNER_MISMATCH PREPARE_IO PORT_START PORT_REJECTED PROVIDER_OBSERVED REGISTER_PROCESS REGISTER_REJECTED CONTROL_VETO RECEIVER_ERROR RECEIVER_DISCONNECTED REPLY_SEND_FAILED RECEIVED_OK CANCELLED PORT_ABSENT".split())
WORKER_ERRORS = frozenset("NOT_OBSERVED Invalid Limit Busy Cancelled Unknown Unavailable Maintenance Disconnect Unsupported R2Invalid R2Contract R2Limit R2Correlation R2Denied R2Conflict R2NotFound R2CommitUnknown R2Storage RegistrationError".split())
NATIVE_STAGES = frozenset("NOT_OBSERVED OWNER_CHECK ORIGINAL_TOOL_CHECK REGISTRY_RESERVE FACT_RESERVE R2_EXECUTE_CLAIMED CALLBACK_ENTERED BACKEND_START_TASK BACKEND_START_RETURNED START_WAIT_TIMEOUT START_TASK_OR_RESULT_ERROR PREINVOKE_REJECTED OBSERVER_REJECTED STARTED_TOOL_READ VALIDATE_STARTED_TOOL POSTSTART_VETO PROVIDER_READY".split())
NATIVE_ERRORS = frozenset("NOT_OBSERVED R2Invalid R2Contract R2Limit R2Correlation R2Denied R2Conflict R2NotFound R2CommitUnknown R2Storage BackendError StartWaitTimeout TaskOrResultError ObserverRejected PostStartVeto".split())
BACKEND_ERRORS = frozenset("NOT_OBSERVED Spawn WebSocketConnectTimeout WebSocketConnect WebSocketConfiguration InitializeTimedOut ApplicationNetworkPolicy Closed Disconnected ProvisioningFailed Json HttpRequest Protocol ProvisioningModeConflict ServerInvalidRequest ServerInvalidParams ServerInternal ServerOther EnvironmentRegistryHttp EnvironmentRegistryConfig EnvironmentRegistryAuth EnvironmentRegistryRequest ConnectionAttempt".split())
OUTER_ERRORS = frozenset("Invalid Limit Busy Cancelled Unknown Unavailable Maintenance Disconnect Unsupported".split()) | frozenset("Session(" + e + ")" for e in "Invalid Contract Limit Correlation Denied Conflict NotFound CommitUnknown Storage".split())
STEPS = {
    "begin", "inventory", "roots", "setup", "materialize", "open-owner",
    "production-factory", "release-factory", "reap-factory-scheduler",
    "finish-owner", "prepare-cleanup", "finish-cleanup", "exit",
    "sealed-basic", "sealed-native-start", "sealed-trusted-approve-claim",
    "sealed-session-context", "sealed-session-worker", "sealed-session-run",
    "sealed-session-join", "sealed-native-review", "sealed-native-context",
    "sealed-native-worker", "sealed-propose", "sealed-trusted-review",
    "sealed-basic-observe", "sealed-native-join", "sealed-cleanup",
    "sealed-repair-cleanup",
}
for _family in ("session", "controls", "proposal"):
    for _action in ("review", "install", "base-select", "base-enable", "wrapper-select", "approve", "wrapper-enable"):
        STEPS.add("sealed-" + _family + "-" + _action)
for _base in ("sealed-cleanup", "sealed-repair-cleanup", "reap-factory-scheduler"):
    for _attempt in range(1, 5):
        STEPS.add(_base + "-" + str(_attempt))
STEPS = frozenset(STEPS)
# H008 live.rs:60 reserves this begin-internal step. It is not a controller
# command; keep it out of controller Pending/Command acceptance.
JOURNAL_STEPS = STEPS | frozenset(("guest-identity",))
KINDS = frozenset((
    "acceptance", "guest-identity-validated", "guest-identity-actual-exit", "lifecycle-initial",
    "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", "lifecycle-after-attempt",
    "STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE", "UNKNOWN_OR_REJECTED_NO_REPLAY",
    "ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",
    "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", "lifecycle-observed",
    "original-owner-observed", "FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE",
))


def alternatives(values):
    return "(?:" + "|".join(re.escape(s) for s in sorted(values)) + ")"


# Exact Debug formatting in commands.rs:106-116 and borrowed.rs:69-79.
DIAGNOSTIC = re.compile(
    r"native start delivery Unknown: " + alternatives(OUTER_ERRORS)
    + r'; diagnostic=CommandDiagnosticV1 \{ worker_stage: "(?P<worker_stage>' + alternatives(WORKER_STAGES)
    + r')", worker_error_class: "(?P<worker_error_class>' + alternatives(WORKER_ERRORS)
    + r')", delivery_stage: "(?P<delivery_stage>' + alternatives(WORKER_STAGES)
    + r')", delivery_error_class: "(?P<delivery_error_class>' + alternatives(WORKER_ERRORS)
    + r')", command_started_observed: (?P<command_started_observed>true|false), provider_observed: (?P<provider_observed>true|false) \}; native=NativeStartDiagnosticV1 \{ stage: "(?P<stage>' + alternatives(NATIVE_STAGES)
    + r')", error_class: "(?P<error_class>' + alternatives(NATIVE_ERRORS)
    + r')", invocation_entered_observed: (?P<invocation_entered_observed>true|false), backend_task_entered_observed: (?P<backend_task_entered_observed>true|false), actual_handle_observed: (?P<actual_handle_observed>true|false), provider_ready_observed: (?P<provider_ready_observed>true|false) \}'
)
COMMAND_FIELDS = ("worker_stage", "worker_error_class", "delivery_stage", "delivery_error_class", "command_started_observed", "provider_observed")
NATIVE_FIELDS = ("stage", "error_class", "invocation_entered_observed", "backend_task_entered_observed", "actual_handle_observed", "provider_ready_observed")
NATIVE_FIELDS_V2 = ("stage", "error_class", "backend_error_class", "invocation_entered_observed", "backend_task_entered_observed", "actual_handle_observed", "provider_ready_observed")
# A separate exact grammar; no optional-field or substring fallback to V1.
# Both grammars are accepted only by fullmatch over the entire input string.
DIAGNOSTIC_V2 = re.compile(
    r"native start delivery Unknown: " + alternatives(OUTER_ERRORS)
    + r'; diagnostic=CommandDiagnosticV1 \{ worker_stage: "(?P<worker_stage>' + alternatives(WORKER_STAGES)
    + r')", worker_error_class: "(?P<worker_error_class>' + alternatives(WORKER_ERRORS)
    + r')", delivery_stage: "(?P<delivery_stage>' + alternatives(WORKER_STAGES)
    + r')", delivery_error_class: "(?P<delivery_error_class>' + alternatives(WORKER_ERRORS)
    + r')", command_started_observed: (?P<command_started_observed>true|false), provider_observed: (?P<provider_observed>true|false) \}; native=NativeStartDiagnosticV2 \{ stage: "(?P<stage>' + alternatives(NATIVE_STAGES)
    + r')", error_class: "(?P<error_class>' + alternatives(NATIVE_ERRORS)
    + r')", backend_error_class: "(?P<backend_error_class>' + alternatives(BACKEND_ERRORS)
    + r')", invocation_entered_observed: (?P<invocation_entered_observed>true|false), backend_task_entered_observed: (?P<backend_task_entered_observed>true|false), actual_handle_observed: (?P<actual_handle_observed>true|false), provider_ready_observed: (?P<provider_ready_observed>true|false) \}'
)
FIXED_NONDIAGNOSTIC = frozenset(("unexpected native start reply; Unknown", "native start timeout; Unknown; no replay"))

# Exact sealed start_diagnostic.rs enums, with no arbitrary strings or numbers.
LOWER_STAGES = frozenset("NotObserved RunnerSelection RequestPrepare ProcessMapReserve MatchedRouteCheck CodexHomeResolve PermissionResolve SandboxBaseReady AccountSelect SetupRefresh CapabilityResolve BlockingTransportTask DesktopPolicy RunnerResolve RunnerPinVerify RegisteredAlias PrivateDesktop PipeCreateIn PipeCreateOut RunnerLogon RegisteredImageVerify PipeConnectIn PipeConnectOut ControlHello SpawnRequestWrite SpawnReadyRead CheckedDriverAssemble SpawnReturned AsUserFilesystemPolicy AsUserOfflinePolicy AsUserHomeAndCwdIdentity AsUserWriteRoots AsUserReadDeny AsUserWriteDeny AsUserReadRoots AsUserExistingCapabilities AsUserCapabilitySid AsUserCapabilityCount AsUserOwnedCredentials".split())
LOWER_ERRORS = frozenset("NotObserved InvalidParams InvalidRequest ServerInternal OtherRpc RouteRejected PinRejected IdentityRejected ControlRejected IoAccessDenied IoNotFound IoInvalidInput IoTimedOut IoInterrupted IoBrokenPipe IoOther OsReported RunnerReported BlockingCancelled BlockingPanic Unclassified ManagedProfileRequired RestrictedFilesystemRequired".split())
RUNNER_STAGES = frozenset("NotObserved ReadSpawnRequest SpawnChild WriteSpawnReady TerminalWitness".split())
OS_CLASSES = frozenset("NotObserved AccessDenied FileNotFound PathNotFound LogonFailure AccountDisabled NoLogonSession OperationAborted TimedOut Other".split())
LOWER_COMPLETIONS = frozenset("NotObserved InProgressOrUnwound ReturnedError ReturnedBackendHandle UnsupportedBackend".split())
BACKEND_HISTORY_SCOPE = "OWNER_LATEST_CALL_NOT_OPERATION_PROOF"
U64_MAX = 18446744073709551615


def attempt_grammar(prefix):
    return (r"WindowsStartAttempt \{ stage: (?P<" + prefix + "stage>" + alternatives(LOWER_STAGES)
            + r"), error: (?P<" + prefix + "error>" + alternatives(LOWER_ERRORS)
            + r"), runner_stage: (?P<" + prefix + "runner_stage>" + alternatives(RUNNER_STAGES)
            + r"), os_class: (?P<" + prefix + "os_class>" + alternatives(OS_CLASSES) + r") \}")


LOWER_GRAMMAR = (
    r"(?:None|Some\(LowerBackendCallObservation \{ call_sequence: (?P<call_sequence>[1-9][0-9]{0,19}), "
    r"observation: WindowsStartSnapshot \{ current: " + attempt_grammar("current_")
    + r", first_attempt: (?:None|Some\(" + attempt_grammar("first_") + r"\)), "
    r"attempt: (?P<attempt>[012]), completion: (?P<completion>" + alternatives(LOWER_COMPLETIONS)
    + r") \} \}\))"
)
# V1/V2 grammars remain unchanged. V3 is a separate whole-string grammar.
DIAGNOSTIC_V3 = re.compile(
    DIAGNOSTIC_V2.pattern.removesuffix(r" \}").replace("NativeStartDiagnosticV2", "NativeStartDiagnosticV3")
    + r', backend_history_scope: "' + BACKEND_HISTORY_SCOPE
    + r'", latest_backend_call_observation: ' + LOWER_GRAMMAR + r" \}"
)


def attempt_fields(fields, prefix):
    return {name: fields[prefix + name] for name in ("stage", "error", "runner_stage", "os_class")}


def valid_attempt_shape(attempt):
    error = attempt["error"]
    if error == "RunnerReported":
        return attempt["runner_stage"] != "NotObserved"
    if error == "OsReported":
        return attempt["runner_stage"] == "NotObserved" and attempt["os_class"] != "NotObserved"
    return attempt["runner_stage"] == "NotObserved" and attempt["os_class"] == "NotObserved"


def lower_projection(fields):
    # None is unavailable/unobserved historical data, never proof of no effect.
    if fields["call_sequence"] is None:
        return True, None
    sequence = int(fields["call_sequence"])
    if not 1 <= sequence <= U64_MAX:
        return False, None
    current = attempt_fields(fields, "current_")
    first = attempt_fields(fields, "first_") if fields["first_stage"] is not None else None
    attempt = int(fields["attempt"])
    completion = fields["completion"]
    default = {name: "NotObserved" for name in current}
    if not valid_attempt_shape(current) or (first is not None and not valid_attempt_shape(first)):
        return False, None
    if completion in ("NotObserved", "UnsupportedBackend"):
        valid = attempt == 0 and first is None and current == default
    else:
        valid = ((attempt == 1 and first is None)
                 or (attempt == 2 and first is not None and first["error"] != "NotObserved"))
        if completion == "ReturnedError":
            valid = valid and current["error"] != "NotObserved"
        elif completion == "ReturnedBackendHandle":
            valid = valid and current == dict(default, stage="SpawnReturned")
    if not valid:
        return False, None
    return True, {"call_sequence": sequence, "observation": {
        "current": current, "first_attempt": first, "attempt": attempt, "completion": completion}}


#2364: separate, exact current DTO branch. Legacy four-field V3 is unchanged.
# Descriptors are reviewed source constants, never inferred from input text.
def _closed2364(values):
    return ("enum", {v: None for v in values.split()})


def _struct2364(name, fields):
    return ("struct", name, fields)


def _option2364(value):
    return ("option", value)


def _fact2364(value):
    return ("enum", {"Observed": ("value", value),
                     "NotObserved": ("value", "ContextUnavailable")})


_U32_2364 = ("uint", 4294967295)
_U64_2364 = ("uint", U64_MAX)
_BOOL_2364 = ("bool",)
_UNIT_2364 = ("unit",)
_SCHEMA2364 = {}
_SCHEMA2364["ContextApiStage"] = _closed2364(
    "QueryPrimaryOpen QueryUser QueryLogonGroups QuerySession QueryRestrictions "
    "QueryAppContainer QueryPrimaryClose CallerUser CallerLogonGroups CallerSession "
    "IntendedSid DuplicateSourceOpen DuplicateToken StationDescriptor DesktopDescriptor "
    "StationVisibility DesktopVisibility")
_failure2364 = {v: None for v in
    "NotAttempted NotApplicable OriginalUnavailable Busy Poisoned BoundExceeded InvalidBuffer".split()}
_failure2364["Win32"] = ("fields", (("code", _U32_2364),))
_SCHEMA2364["ContextFailure"] = ("enum", dict(_failure2364))
_unavailable2364 = dict(_failure2364)
_unavailable2364["Upstream"] = ("fields", (
    ("stage", "ContextApiStage"), ("reason", "ContextFailure")))
_SCHEMA2364["ContextUnavailable"] = ("enum", _unavailable2364)
_SCHEMA2364["TokenOpenPurpose"] = _closed2364("NotRequested PrimaryQuery DuplicationSource")
_SCHEMA2364["TokenOutputAllocation"] = _closed2364("NotAllocated AllocatedOriginal UncertainOutput")
_SCHEMA2364["TokenAllocationObservation"] = _struct2364("TokenAllocationObservation", (
    ("api_returned_success", _BOOL_2364), ("allocation", "TokenOutputAllocation"),
    ("win32_error", _option2364(_U32_2364))))
_SCHEMA2364["TokenOpenObservation"] = _struct2364("TokenOpenObservation", (
    ("purpose", "TokenOpenPurpose"), ("requested_mask", _U32_2364),
    ("result", _fact2364("TokenAllocationObservation"))))
_SCHEMA2364["PrimaryTokenReadResults"] = _struct2364("PrimaryTokenReadResults", tuple(
    (name, _fact2364(_UNIT_2364)) for name in
    ("user", "logon_groups", "session", "restrictions", "app_container")))
_SCHEMA2364["StartupQueryStages"] = _struct2364("StartupQueryStages", (
    ("primary_open", "TokenOpenObservation"), ("primary_reads", "PrimaryTokenReadResults"),
    ("duplicate_source_open", "TokenOpenObservation"),
    ("impersonation_duplicate", _fact2364("TokenAllocationObservation"))))
_SCHEMA2364["StartupApplicationSource"] = _closed2364(
    "VerifiedResolvedPinnedImage RegisteredAliasNotYetVerified")
_SCHEMA2364["StartupCallParameters"] = _struct2364("StartupCallParameters", (
    ("creation_flags", _U32_2364), ("logon_flags", _U32_2364),
    ("registered_profile_branch", _BOOL_2364),
    ("environment_is_null_user_profile_source", _BOOL_2364),
    ("application_source", "StartupApplicationSource"), ("cwd_equals_request_cwd", _BOOL_2364),
    ("cwd_under_request_workspace_root", _fact2364(_BOOL_2364)),
    ("desktop_pointer_is_same_owned_qualified_name", _BOOL_2364),
    ("station_still_same_borrowed_object", _fact2364(_BOOL_2364))))
_close2364 = {v: None for v in "NotAllocated Held ClosedVerified".split()}
_close2364["CloseFailed"] = ("fields", (("code", _U32_2364),))
_SCHEMA2364["QueryHandleClose"] = ("enum", _close2364)
_SCHEMA2364["StartupTokenRelations"] = _struct2364("StartupTokenRelations", tuple(
    (name, _fact2364(_U32_2364 if name == "runner_logon_group_attributes" else _BOOL_2364))
    for name in ("user_equals_intended_sandbox", "user_equals_caller_user",
                 "logon_sid_equals_caller_logon_sid", "runner_logon_group_attributes",
                 "session_equals_caller_session", "has_restrictions", "is_app_container")))
_SCHEMA2364["StartupAccessResult"] = _struct2364("StartupAccessResult", (
    ("access_status", _BOOL_2364), ("granted_mask", _U32_2364),
    ("denial_win32_error", _option2364(_U32_2364))))
_SCHEMA2364["AuthzAllocation"] = _closed2364(
    "NotAttempted InFlight FailedNoOutput Held FailedUncertainOutput InvalidSuccessOutput")
_SCHEMA2364["AuthzRetirement"] = _closed2364(
    "NotAllocated Held InFlight FreedVerified Failed BlockedByContext Uncertain")
_SCHEMA2364["AuthzResourceObservation"] = _struct2364("AuthzResourceObservation", (
    ("allocation", "AuthzAllocation"), ("retirement", "AuthzRetirement"),
    ("initialization_error", _U32_2364), ("retirement_error", _U32_2364)))
_SCHEMA2364["AuthzObjectState"] = _closed2364(
    "NotAttempted PrimaryUnavailable RestrictedToken AppContainer ResourceManagerUnavailable "
    "ContextUnavailable ObjectIdentityUnavailable DescriptorUnavailable VisibilityUnavailable "
    "InvalidBuffer BoundExceeded NullOrAbsentDacl OverlappingLayout UnsupportedAce PrincipalSelf "
    "InFlight ApiFailed ReplyReturned")
_SCHEMA2364["AuthzObjectMapping"] = _closed2364("NotSelected NoninteractiveStation VisibleStation Desktop")
_SCHEMA2364["AuthzObjectObservation"] = _struct2364("AuthzObjectObservation", (
    ("state", "AuthzObjectState"), ("mapping", "AuthzObjectMapping"),
    ("error", _U32_2364), ("granted_mask", _U32_2364)))
_AUTHZ_SCOPE2364 = "QUERY8_LOCAL_RM5_MAPPED_SIMPLE_DACL_ONLY_MIC_SACL_CAP_NATIVE_NOT_PROVEN"
_SCHEMA2364["AuthzDaclProjection"] = _struct2364("AuthzDaclProjection", (
    ("scope", ("string", (_AUTHZ_SCOPE2364,))),
    ("resource_manager", "AuthzResourceObservation"), ("context", "AuthzResourceObservation"),
    ("station", "AuthzObjectObservation"), ("desktop", "AuthzObjectObservation"),
    ("first_failure", _option2364(("tuple", (
        ("string", ("InitializeResourceManager", "InitializeContextFromToken",
                    "StationAuthzAccessCheck", "DesktopAuthzAccessCheck",
                    "FreeContext", "FreeResourceManager")),
        _option2364(_U32_2364)))))))
_SCHEMA2364["StartupObjectObservation"] = _struct2364("StartupObjectObservation", (
    *((name, _fact2364(_BOOL_2364)) for name in (
        "name_equals_creation_object", "visible", "receiving_user_input",
        "descriptor_owner_equals_runner_user", "descriptor_group_equals_runner_logon_sid",
        "dacl_present", "dacl_null")),
    ("descriptor_read", _fact2364(_UNIT_2364)), ("access_check_invoked", _BOOL_2364),
    ("access", _fact2364("StartupAccessResult"))))
_SCHEMA2364["WindowsStartupContext"] = _struct2364("WindowsStartupContext", (
    ("startup_id", _U64_2364), ("call", _fact2364("StartupCallParameters")),
    ("original_runner_identity", _option2364(("tuple", (_U32_2364, _U64_2364)))),
    ("query_status", _fact2364(_UNIT_2364)), ("query_stages", "StartupQueryStages"),
    ("runner_before_token_query", _fact2364("WindowsStartRunnerObservation")),
    ("runner_after_token_query", _fact2364("WindowsStartRunnerObservation")),
    ("token", "StartupTokenRelations"), ("station", "StartupObjectObservation"),
    ("desktop", "StartupObjectObservation"),
    ("query_handle_closes", ("array", "QueryHandleClose", 2)),
    ("query_slot_closes", ("array", "QueryHandleClose", 3)),
    ("authz_dacl", "AuthzDaclProjection")))
_SCHEMA2364["WindowsOsError"] = ("enum", {v: None for v in OS_CLASSES})
_SCHEMA2364["WindowsStartRunnerState"] = ("enum", {
    "RunningAtObservation": None, "Exited": ("fields", (("raw_dword", _U32_2364),)),
    "SignaledExitQueryFailed": ("fields", (("os_class", "WindowsOsError"),)),
    "WaitFailed": ("fields", (("os_class", "WindowsOsError"),)),
    "UnexpectedWait": ("fields", (("wait_result", _U32_2364),))})
_SCHEMA2364["WindowsStartRunnerObservation"] = _struct2364("WindowsStartRunnerObservation", (
    ("startup_id", _U64_2364), ("runner_pid", _U32_2364),
    ("runner_creation_filetime_100ns", _U64_2364), ("state", "WindowsStartRunnerState")))
_SCHEMA2364["LowerStage"] = ("enum", {v: None for v in LOWER_STAGES})
_SCHEMA2364["LowerError"] = ("enum", {v: None for v in LOWER_ERRORS})
_SCHEMA2364["RunnerStage"] = ("enum", {v: None for v in RUNNER_STAGES})
_SCHEMA2364["LowerCompletion"] = ("enum", {v: None for v in LOWER_COMPLETIONS})
_SCHEMA2364["WindowsStartAttempt"] = _struct2364("WindowsStartAttempt", (
    ("stage", "LowerStage"), ("error", "LowerError"), ("runner_stage", "RunnerStage"),
    ("os_class", "WindowsOsError"),
    ("original_runner", _option2364("WindowsStartRunnerObservation")),
    ("startup_context", _option2364("WindowsStartupContext"))))
_SCHEMA2364["WindowsStartSnapshot"] = _struct2364("WindowsStartSnapshot", (
    ("current", "WindowsStartAttempt"), ("first_attempt", _option2364("WindowsStartAttempt")),
    ("attempt", ("uint", 2)), ("completion", "LowerCompletion")))
_SCHEMA2364["LowerBackendCallObservation"] = _struct2364("LowerBackendCallObservation", (
    ("call_sequence", _U64_2364), ("observation", "WindowsStartSnapshot")))
_SCHEMA2364["SameReviewedInvocationStartDataOnly"] = _struct2364(
    "SameReviewedInvocationStartDataOnly", (("snapshot", "WindowsStartSnapshot"),))


class _CurrentReader2364:
    def __init__(self, text):
        if type(text) is not str or not 1 <= len(text) <= 65536:
            raise Rejected()
        self.text, self.pos, self.nodes = text, 0, 0

    def take(self, literal):
        if not self.text.startswith(literal, self.pos):
            raise Rejected()
        self.pos += len(literal)

    def name(self):
        match = re.compile(r"[A-Za-z_][A-Za-z_0-9]{0,79}").match(self.text, self.pos)
        if match is None:
            raise Rejected()
        self.pos = match.end()
        return match.group()

    def fields(self, fields, depth):
        self.take(" { ")
        result = {}
        for index, (name, descriptor) in enumerate(fields):
            if index:
                self.take(", ")
            self.take(name + ": ")
            result[name] = self.read(descriptor, depth + 1)
        self.take(" }")
        return result

    def read(self, descriptor, depth=0):
        self.nodes += 1
        if depth > 32 or self.nodes > 8192:
            raise Rejected()
        if type(descriptor) is str:
            descriptor = _SCHEMA2364[descriptor]
        tag = descriptor[0]
        if tag == "uint":
            match = re.compile(r"(?:0|[1-9][0-9]{0,19})(?![0-9])").match(self.text, self.pos)
            if match is None:
                raise Rejected()
            self.pos = match.end()
            result = int(match.group())
            if result > descriptor[1]:
                raise Rejected()
            return result
        if tag == "bool":
            name = self.name()
            if name not in ("true", "false"):
                raise Rejected()
            return name == "true"
        if tag == "unit":
            self.take("()")
            return None
        if tag == "string":
            for value in descriptor[1]:
                literal = '"' + value + '"'
                if self.text.startswith(literal, self.pos):
                    self.take(literal)
                    return value
            raise Rejected()
        if tag == "option":
            name = self.name()
            if name == "None":
                return None
            if name != "Some":
                raise Rejected()
            self.take("(")
            result = self.read(descriptor[1], depth + 1)
            self.take(")")
            return result
        if tag in ("tuple", "array"):
            values = descriptor[1] if tag == "tuple" else (descriptor[1],) * descriptor[2]
            self.take("(" if tag == "tuple" else "[")
            result = []
            for index, value in enumerate(values):
                if index:
                    self.take(", ")
                result.append(self.read(value, depth + 1))
            self.take(")" if tag == "tuple" else "]")
            return result
        if tag == "enum":
            name = self.name()
            if name not in descriptor[1]:
                raise Rejected()
            branch = descriptor[1][name]
            if branch is None:
                return name
            if branch[0] == "value":
                self.take("(")
                value = self.read(branch[1], depth + 1)
                self.take(")")
                return {"variant": name, "value": value}
            return {"variant": name, **self.fields(branch[1], depth)}
        if tag == "struct":
            self.take(descriptor[1])
            result = self.fields(descriptor[2], depth)
            _validate2364(descriptor[1], result)
            return result
        raise Rejected()


def _observed2364(value):
    return type(value) is dict and value.get("variant") == "Observed"


def _authz_first2364(value):
    for name, resource in (("InitializeResourceManager", value["resource_manager"]),
                           ("InitializeContextFromToken", value["context"])):
        if resource["allocation"] in ("FailedNoOutput", "FailedUncertainOutput"):
            return [name, resource["initialization_error"]]
        if resource["allocation"] in ("InFlight", "InvalidSuccessOutput"):
            return [name, None]
    for name, obj in (("StationAuthzAccessCheck", value["station"]),
                      ("DesktopAuthzAccessCheck", value["desktop"])):
        if obj["state"] == "ApiFailed" or (obj["state"] == "ReplyReturned" and obj["error"] != 0):
            return [name, obj["error"]]
        if obj["state"] == "InFlight" or (obj["state"] == "ReplyReturned" and obj["granted_mask"] == 0):
            return [name, None]
    for name, resource in (("FreeContext", value["context"]),
                           ("FreeResourceManager", value["resource_manager"])):
        if resource["retirement"] == "Failed":
            return [name, resource["retirement_error"]]
        if resource["retirement"] in ("InFlight", "Uncertain"):
            return [name, None]
    return None


def _validate2364(kind, value):
    if kind == "WindowsStartRunnerObservation":
        if not all(value[k] > 0 for k in
                   ("startup_id", "runner_pid", "runner_creation_filetime_100ns")):
            raise Rejected()
    elif kind == "TokenAllocationObservation":
        if value["api_returned_success"] == (value["win32_error"] is not None):
            raise Rejected()
        if not value["api_returned_success"] and value["allocation"] == "AllocatedOriginal":
            raise Rejected()
    elif kind == "TokenOpenObservation":
        if _observed2364(value["result"]) and (
                value["purpose"] == "NotRequested" or value["requested_mask"] == 0):
            raise Rejected()
        if value["purpose"] == "NotRequested" and value["requested_mask"] != 0:
            raise Rejected()
    elif kind == "AuthzResourceObservation":
        if (value["allocation"] not in ("FailedNoOutput", "FailedUncertainOutput")
                and value["initialization_error"] != 0):
            raise Rejected()
        if value["retirement"] != "Failed" and value["retirement_error"] != 0:
            raise Rejected()
    elif kind == "AuthzDaclProjection":
        if value["first_failure"] != _authz_first2364(value):
            raise Rejected()
    elif kind == "WindowsStartupContext":
        identity = value["original_runner_identity"]
        if identity is not None and (identity[0] == 0 or identity[1] == 0):
            raise Rejected()
        for field in ("runner_before_token_query", "runner_after_token_query"):
            sample = value[field]
            if _observed2364(sample):
                sample = sample["value"]
                if (sample["startup_id"] != value["startup_id"]
                        or identity != [sample["runner_pid"], sample["runner_creation_filetime_100ns"]]):
                    raise Rejected()
    elif kind == "WindowsStartAttempt":
        if not valid_attempt_shape(value):
            raise Rejected()
        original, context = value["original_runner"], value["startup_context"]
        if original is not None and context is not None:
            if (original["startup_id"] != context["startup_id"]
                    or context["original_runner_identity"] not in (
                        None, [original["runner_pid"], original["runner_creation_filetime_100ns"]])):
                raise Rejected()
    elif kind == "WindowsStartSnapshot":
        # Keep the existing mutually exclusive attempt/completion rules.
        fields = {"call_sequence": "1", "attempt": str(value["attempt"]),
                  "completion": value["completion"], "first_stage": None}
        for prefix, attempt in (("current_", value["current"]), ("first_", value["first_attempt"])):
            if attempt is not None:
                fields.update({prefix + k: attempt[k] for k in
                               ("stage", "error", "runner_stage", "os_class")})
        if not lower_projection(fields)[0]:
            raise Rejected()
        if value["completion"] in ("NotObserved", "UnsupportedBackend"):
            if value["current"]["original_runner"] is not None or value["current"]["startup_context"] is not None:
                raise Rejected()
    elif kind == "LowerBackendCallObservation" and value["call_sequence"] == 0:
        raise Rejected()


_CURRENT_PREFIX2364 = re.compile(
    DIAGNOSTIC_V2.pattern.removesuffix(r" \}").replace("NativeStartDiagnosticV2", "NativeStartDiagnosticV3")
    + r', backend_history_scope: "' + BACKEND_HISTORY_SCOPE
    + r'", latest_backend_call_observation: (?P<full_tail2364>[\s\S]{1,65536})')


def current_diagnostic2364(payload):
    if type(payload) is not str or len(payload) > MAX_RECORD:
        return {"parse_status": "REJECTED"}
    match = _CURRENT_PREFIX2364.fullmatch(payload)
    if match is None:
        return {"parse_status": "REJECTED"}
    try:
        reader = _CurrentReader2364(match.group("full_tail2364"))
        lower = reader.read(_option2364("LowerBackendCallObservation"))
        reviewed = "FIELD_ABSENT"
        if reader.text.startswith(", reviewed_invocation_start: ", reader.pos):
            reader.take(", reviewed_invocation_start: ")
            if reader.text.startswith('"AMBIGUOUS_UNAVAILABLE"', reader.pos):
                reader.take('"AMBIGUOUS_UNAVAILABLE"')
                reviewed = "AMBIGUOUS_UNAVAILABLE"
            elif reader.text.startswith("ReviewedInvocationStartUnboundOrUnavailable", reader.pos):
                reader.take("ReviewedInvocationStartUnboundOrUnavailable")
                reviewed = "ReviewedInvocationStartUnboundOrUnavailable"
            else:
                reviewed = {"SameReviewedInvocationStartDataOnly":
                            reader.read("SameReviewedInvocationStartDataOnly")}
        reader.take(" }")
        if reader.pos != len(reader.text):
            raise Rejected()
        fields = match.groupdict()
        command = {k: fields[k] == "true" if k.endswith("_observed") else fields[k]
                   for k in COMMAND_FIELDS}
        native = {k: fields[k] == "true" if k.endswith("_observed") else fields[k]
                  for k in NATIVE_FIELDS_V2}
        native.update({"backend_history_scope": BACKEND_HISTORY_SCOPE,
                       "latest_backend_call_observation": lower,
                       "reviewed_invocation_start": reviewed,
                       "projection_format": "CURRENT_FULL_SNAPSHOT_DATA_ONLY"})
        return {"parse_status": "OK", "command_diagnostic": command, "native_diagnostic": native}
    except (Rejected, KeyError, TypeError, ValueError, RecursionError):
        return {"parse_status": "REJECTED"}


def safe_diagnostic(payload):
    if type(payload) is not str:
        return {"parse_status": "REJECTED"}
    if payload in FIXED_NONDIAGNOSTIC:
        return {"parse_status": "MISSING"}
    match = DIAGNOSTIC.fullmatch(payload)
    native_fields = NATIVE_FIELDS
    if match is None:
        match = DIAGNOSTIC_V2.fullmatch(payload)
        native_fields = NATIVE_FIELDS_V2
    if match is None:
        match = DIAGNOSTIC_V3.fullmatch(payload)
        if match is None:
            return current_diagnostic2364(payload)
        valid, lower = lower_projection(match.groupdict())
        if not valid:
            return {"parse_status": "REJECTED"}
        fields = match.groupdict()
        command = {k: fields[k] == "true" if k.endswith("_observed") else fields[k] for k in COMMAND_FIELDS}
        native = {k: fields[k] == "true" if k.endswith("_observed") else fields[k] for k in NATIVE_FIELDS_V2}
        native["backend_history_scope"] = BACKEND_HISTORY_SCOPE
        native["latest_backend_call_observation"] = lower
        return {"parse_status": "OK", "command_diagnostic": command, "native_diagnostic": native}
    fields = match.groupdict()
    for name in COMMAND_FIELDS + native_fields:
        if name.endswith("_observed"):
            fields[name] = fields[name] == "true"
    return {"parse_status": "OK",
            "command_diagnostic": {k: fields[k] for k in COMMAND_FIELDS},
            "native_diagnostic": {k: fields[k] for k in native_fields}}


class Rejected(Exception):
    pass


def unique_object(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise Rejected()
        obj[key] = value
    return obj


def decode(raw):
    def reject_constant(_):
        raise Rejected()
    return json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object,
                      parse_constant=reject_constant)


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def project_journal(raw):
    result = {"parse_status": "OK", "records": []}
    if len(raw) > MAX_JOURNAL:
        return {"parse_status": "REJECTED", "records": []}
    if not raw:
        return {"parse_status": "MISSING", "records": []}
    lines = raw.split(b"\n")
    if lines[-1]:
        result["parse_status"] = "TRUNCATED"
    lines.pop()  # Incomplete final record is never parsed or echoed.
    if len(lines) > 256:
        return {"parse_status": "REJECTED", "records": []}
    reserved_step = None
    for index, line in enumerate(lines):
        try:
            if not line or len(line) > MAX_RECORD:
                raise Rejected()
            record = decode(line)
            if type(record) is not dict or set(record) != {"sequence", "kind", "payload"}:
                raise Rejected()
            if not integer(record["sequence"], 0, 255) or record["sequence"] != index:
                raise Rejected()
            kind = record["kind"]
            payload = record["payload"]
            row = {"sequence": index, "kind": "NOT_ALLOWLISTED", "parse_status": "REJECTED"}
            if type(kind) is not str or kind not in KINDS:
                reserved_step = None
                row["parse_status"] = "MISSING"  # Valid envelope; excluded kind.
                result["records"].append(row)
                continue
            row.update(kind=kind, parse_status="OK")
            if kind == "ATTEMPT_RESERVED_OUTCOME_UNKNOWN":
                reserved_step = None
                if type(payload) is dict and type(payload.get("step")) is str and payload["step"] in JOURNAL_STEPS:
                    row["step"] = payload["step"]
                    reserved_step = payload["step"]
                else:
                    row["parse_status"] = "REJECTED"
            elif kind in ("ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE", "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", "UNKNOWN_OR_REJECTED_NO_REPLAY"):
                # Ordered attribution only; never operation/authority proof.
                if reserved_step is not None:
                    row["step"] = reserved_step
                if kind == "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY":
                    row.update(safe_diagnostic(payload))
                elif kind == "UNKNOWN_OR_REJECTED_NO_REPLAY":
                    row["parse_status"] = "MISSING"
                reserved_step = None
            elif kind == "STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE":
                if type(payload) is str and payload in STEPS:
                    row["step"] = payload
                else:
                    row["parse_status"] = "REJECTED"
                reserved_step = None
            result["records"].append(row)
        except Exception:
            # Reject malformed envelope/sequence globally rather than renumber.
            return {"parse_status": "REJECTED", "records": []}
    return result


CONTROL_STATES = frozenset(("WAITING_EXPLICIT_START", "TERMINAL_VALIDATION_PENDING", "GROUP_RESERVED_NO_ADDITIONAL_AUTHORITY", "RESERVED_UNKNOWN_BEFORE_SEND"))
CONTROL_STATUSES = frozenset(("UNKNOWN_SAME_CONTROLLER_RETAINED", "ORIGINAL_RESULT_BLOCKED_QUEUE_STOPPED", "ALL_ORIGINAL_STEP_RESULTS_OBSERVED_NOT_ACCEPTANCE", "OBSERVATION_BUDGET_STOPPED_OUTCOME_NOT_RECLASSIFIED"))
CONTROL_SCOPES = frozenset(("ORIGINAL_LIVE_PROCESS_NOT_ACCEPTANCE", "SAME_PROCESS_OBSERVATION_NOT_AUTHORITY_OR_ACCEPTANCE", "CAPTURE_ONLY_REQUIRES_INDEPENDENT_PRODUCT_REVIEW"))
CONTROL_BOOL = frozenset("NoSystemAcceptance Administrator AttemptAbsent FixedCollisionFree NeverRetry SameControllerRetained NoReplay NoCleanup NoTTLRefresh Kill Cleanup Reconnect Replay NoSDKAcceptance Blocked CaptureFailed ActualTerminalObserved ActualReadersEOF JournalCaughtUp NoHumanPause JournalParsed SDKComplete SystemRestorationGuaranteed NoAuthorityGranted Written".split())
CONTROL_BYTES = frozenset(("JournalBytes", "TerminalJournalLength", "FrozenTerminalJournalLength", "ActualJournalBytes"))
CONTROL_EXIT = frozenset(("ActualExit", "ActualExitIfObserved"))
CONTROL_INT = {"Attempt": (1, 1), "NextIndex": (0, 8), "ElapsedMilliseconds": (0, 2147483647), "OriginalAuthorityMilliseconds": (60000, 60000), "StopSchedulingBudgetMilliseconds": (40000, 40000)}
CONTROL_FILES = frozenset(("controller-preflight.json", "fresh-guest-check.json", "start-reserved-unknown.json", "guest-started.json", "guest-actual-terminal.json", "capture-final.json", "unknown-retained.json"))
CONTROL_FILENAME = re.compile(r"(?:poll-[0-9]{4}|attempt-[0-9]{2}|delivery-[0-9]{2}|group-(?:session-four|native-eight)-(?:reserved|returned|stopped))\.json")


def controller_allowed(name):
    return name in CONTROL_FILES or CONTROL_FILENAME.fullmatch(name) is not None


def project_controller(raw):
    if len(raw) > MAX_CONTROLLER:
        return {"parse_status": "REJECTED"}
    try:
        obj = decode(raw)
        if type(obj) is not dict:
            raise Rejected()
        fields = {}
        rejected = False
        for key, value in obj.items():
            allowed = False
            recognized = True
            if key in CONTROL_BOOL:
                allowed = type(value) is bool
            elif key in CONTROL_BYTES:
                allowed = value is None or integer(value, 0, MAX_JOURNAL)
            elif key in CONTROL_EXIT:
                allowed = value is None or integer(value, -2147483648, 4294967295)
            elif key in CONTROL_INT:
                allowed = integer(value, *CONTROL_INT[key])
            elif key == "State":
                allowed = type(value) is str and value in CONTROL_STATES
            elif key == "Status":
                allowed = type(value) is str and value in CONTROL_STATUSES
            elif key == "Scope":
                allowed = type(value) is str and value in CONTROL_SCOPES
            elif key in ("PendingCommand", "Pending", "Command"):
                allowed = type(value) is str and (value == "" or value in STEPS)
            elif key == "Group":
                allowed = type(value) is str and value in ("session-four", "native-eight")
            elif key == "CredentialLogging":
                allowed = value == "NONE"
            elif key == "Applied":
                allowed = value == "NOT_PROVEN"
            elif key == "NextCommand":
                allowed = value == "WAIT_ORIGINAL_JOURNAL_OBSERVATION"
            else:
                recognized = False  # PID/UUID/Commands/Confirmation/hash/text dropped.
            if recognized:
                if allowed:
                    fields[key] = value
                else:
                    rejected = True
        if rejected:
            return {"parse_status": "REJECTED"}
        if not fields:
            return {"parse_status": "MISSING"}
        return {"parse_status": "OK", "controller_status": fields}
    except Exception:
        return {"parse_status": "REJECTED"}


def read_bounded(name, limit, permitted):
    path = Path(name)
    if not path.is_absolute() or str(path).startswith(("\\\\", "//")) or not permitted(path.name):
        raise Rejected()
    for candidate in (path,) + tuple(path.parents):
        info = candidate.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise Rejected()
    if not stat.S_ISREG(path.lstat().st_mode):
        raise Rejected()
    with path.open("rb") as handle:
        before = os.fstat(handle.fileno())
        if before.st_size > limit:
            raise Rejected()
        raw = handle.read(limit + 1)
        after = os.fstat(handle.fileno())
        if len(raw) > limit or (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise Rejected()
        return raw


class SafeParser(argparse.ArgumentParser):
    def error(self, _message):
        raise Rejected()


def main(arguments):
    parser = SafeParser(prog="project_journal.py", description="Explicit local files only; closed scalar projection; no business execution.")
    parser.add_argument("--journal", metavar="ABSOLUTE_JOURNAL_FILE")
    parser.add_argument("--expected-journal-sha256", metavar="HEX64")
    parser.add_argument("--controller-record", action="append", default=[], metavar="ABSOLUTE_FIXED_JSON_FILE")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(arguments)
    if args.self_test:
        if args.journal or args.expected_journal_sha256 or args.controller_record:
            raise Rejected()
        return self_test()
    if not args.journal or len(args.controller_record) > MAX_CONTROLLER_FILES:
        raise Rejected()
    expected = args.expected_journal_sha256
    if expected is not None and re.fullmatch(r"[0-9a-fA-F]{64}", expected) is None:
        raise Rejected()
    result = {"schema": VERSION, "input_match": None, "journal": {"parse_status": "REJECTED", "records": []}, "controller": []}
    try:
        raw = read_bounded(args.journal, MAX_JOURNAL, lambda n: n in ("journal.log", "journal.jsonl"))
        if expected is not None:
            result["input_match"] = hashlib.sha256(raw).hexdigest() == expected.lower()
        if result["input_match"] is not False:
            result["journal"] = project_journal(raw)
    except Exception:
        pass
    for record in args.controller_record:
        try:
            result["controller"].append(project_controller(read_bounded(record, MAX_CONTROLLER, controller_allowed)))
        except Exception:
            result["controller"].append({"parse_status": "REJECTED"})
    print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    bad = result["journal"]["parse_status"] in ("REJECTED", "TRUNCATED") or any(r["parse_status"] == "REJECTED" for r in result["journal"]["records"] + result["controller"])
    return 2 if bad else 0


def current_self_test2364(check, records, row):
    # Exact public payload saved by ROOT2356; source-only data fixture.
    # It remains an Unknown failure. No original object is queried or executed.
    actual2356 = "native start delivery Unknown: Unknown; diagnostic=CommandDiagnosticV1 { worker_stage: \"PORT_REJECTED\", worker_error_class: \"R2CommitUnknown\", delivery_stage: \"RECEIVER_DISCONNECTED\", delivery_error_class: \"Unknown\", command_started_observed: true, provider_observed: false }; native=NativeStartDiagnosticV3 { stage: \"BACKEND_START_RETURNED\", error_class: \"BackendError\", backend_error_class: \"ServerInternal\", invocation_entered_observed: true, backend_task_entered_observed: true, actual_handle_observed: false, provider_ready_observed: false, backend_history_scope: \"OWNER_LATEST_CALL_NOT_OPERATION_PROOF\", latest_backend_call_observation: Some(LowerBackendCallObservation { call_sequence: 1, observation: WindowsStartSnapshot { current: WindowsStartAttempt { stage: PermissionResolve, error: ServerInternal, runner_stage: NotObserved, os_class: NotObserved, original_runner: None, startup_context: None }, first_attempt: None, attempt: 1, completion: ReturnedError } }), reviewed_invocation_start: SameReviewedInvocationStartDataOnly { snapshot: WindowsStartSnapshot { current: WindowsStartAttempt { stage: PermissionResolve, error: ServerInternal, runner_stage: NotObserved, os_class: NotObserved, original_runner: None, startup_context: None }, first_attempt: None, attempt: 1, completion: ReturnedError } } }"
    parsed = safe_diagnostic(actual2356)
    check(parsed["parse_status"] == "OK")
    native = parsed["native_diagnostic"]
    check(native["projection_format"] == "CURRENT_FULL_SNAPSHOT_DATA_ONLY")
    check(parsed["command_diagnostic"]["worker_error_class"] == "R2CommitUnknown")
    check(native["error_class"] == "BackendError")
    check(native["backend_error_class"] == "ServerInternal")
    check(native["actual_handle_observed"] is False)
    check(native["provider_ready_observed"] is False)
    snap = native["latest_backend_call_observation"]["observation"]
    check(snap["current"]["stage"] == "PermissionResolve")
    check(snap["current"]["error"] == "ServerInternal")
    check(snap["current"]["original_runner"] is None)
    check(snap["current"]["startup_context"] is None)
    check(native["reviewed_invocation_start"]["SameReviewedInvocationStartDataOnly"]["snapshot"] == snap)
    check("release_eligible" not in native and "native_complete" not in native)
    projected = project_journal(records([
        row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", actual2356)]))
    check(projected["records"][0]["kind"] == "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY")
    check(projected["records"][0]["native_diagnostic"]["actual_handle_observed"] is False)

    def attempt(stage="AsUserWriteRoots", error="Unclassified", original="None", context="None"):
        return ("WindowsStartAttempt { stage: " + stage + ", error: " + error
                + ", runner_stage: NotObserved, os_class: NotObserved, original_runner: "
                + original + ", startup_context: " + context + " }")

    def snapshot(current, first="None", number="1", completion="ReturnedError"):
        return ("WindowsStartSnapshot { current: " + current + ", first_attempt: "
                + first + ", attempt: " + number + ", completion: " + completion + " }")

    def fixture(current_snapshot, reviewed=None):
        prefix = actual2356.split(", latest_backend_call_observation: ", 1)[0]
        tail = ("Some(LowerBackendCallObservation { call_sequence: 1, observation: "
                + current_snapshot + " })")
        if reviewed is not None:
            tail += ", reviewed_invocation_start: " + reviewed
        return prefix + ", latest_backend_call_observation: " + tail + " }"

    def accepts(text):
        value = safe_diagnostic(text)
        check(value["parse_status"] == "OK")
        check(value["native_diagnostic"]["actual_handle_observed"] is False)
        check(value["native_diagnostic"]["provider_ready_observed"] is False)
        return value["native_diagnostic"]

    first = snapshot(attempt())
    accepts(fixture(first))
    for reviewed in ('"AMBIGUOUS_UNAVAILABLE"', "ReviewedInvocationStartUnboundOrUnavailable",
                     "SameReviewedInvocationStartDataOnly { snapshot: " + first + " }"):
        accepts(fixture(first, reviewed))
    prior = "Some(" + attempt("PermissionResolve", "ManagedProfileRequired") + ")"
    accepts(fixture(snapshot(attempt(), prior, "2")))
    for error in ("ManagedProfileRequired", "RestrictedFilesystemRequired", "IoAccessDenied"):
        value = accepts(fixture(snapshot(attempt("AsUserOfflinePolicy", error))))
        check(value["latest_backend_call_observation"]["observation"]["current"]["error"] == error)

    unobserved = "NotObserved(NotAttempted)"
    opening = ("TokenOpenObservation { purpose: NotRequested, requested_mask: 0, result: "
               + unobserved + " }")
    reads = "PrimaryTokenReadResults { " + ", ".join(
        name + ": " + unobserved for name in
        ("user", "logon_groups", "session", "restrictions", "app_container")) + " }"
    queries = ("StartupQueryStages { primary_open: " + opening + ", primary_reads: " + reads
               + ", duplicate_source_open: " + opening
               + ", impersonation_duplicate: " + unobserved + " }")
    token = "StartupTokenRelations { " + ", ".join(
        name + ": " + unobserved for name in (
            "user_equals_intended_sandbox", "user_equals_caller_user",
            "logon_sid_equals_caller_logon_sid", "runner_logon_group_attributes",
            "session_equals_caller_session", "has_restrictions", "is_app_container")) + " }"
    obj = ("StartupObjectObservation { " + ", ".join(
        name + ": " + unobserved for name in (
            "name_equals_creation_object", "visible", "receiving_user_input",
            "descriptor_owner_equals_runner_user", "descriptor_group_equals_runner_logon_sid",
            "dacl_present", "dacl_null", "descriptor_read"))
           + ", access_check_invoked: false, access: " + unobserved + " }")
    resource = ("AuthzResourceObservation { allocation: NotAttempted, retirement: NotAllocated, "
                "initialization_error: 0, retirement_error: 0 }")
    authz_obj = ("AuthzObjectObservation { state: NotAttempted, mapping: NotSelected, "
                 "error: 0, granted_mask: 0 }")
    authz = ('AuthzDaclProjection { scope: "' + _AUTHZ_SCOPE2364
             + '", resource_manager: ' + resource + ", context: " + resource
             + ", station: " + authz_obj + ", desktop: " + authz_obj + ", first_failure: None }")
    context = ("WindowsStartupContext { startup_id: 7, call: " + unobserved
               + ", original_runner_identity: Some((9, 11)), query_status: " + unobserved
               + ", query_stages: " + queries + ", runner_before_token_query: " + unobserved
               + ", runner_after_token_query: " + unobserved + ", token: " + token
               + ", station: " + obj + ", desktop: " + obj
               + ", query_handle_closes: [NotAllocated, NotAllocated], "
               "query_slot_closes: [NotAllocated, NotAllocated, NotAllocated], authz_dacl: " + authz + " }")
    original = ("WindowsStartRunnerObservation { startup_id: 7, runner_pid: 9, "
                "runner_creation_filetime_100ns: 11, state: Exited { raw_dword: 4294967295 } }")
    full = fixture(snapshot(attempt(original="Some(" + original + ")", context="Some(" + context + ")")))
    value = accepts(full)
    current = value["latest_backend_call_observation"]["observation"]["current"]
    check(current["original_runner"]["state"] == {"variant": "Exited", "raw_dword": 4294967295})
    check(current["startup_context"]["call"] == {"variant": "NotObserved", "value": "NotAttempted"})
    check(current["startup_context"]["query_slot_closes"] == ["NotAllocated"] * 3)
    # Native scalar is explicitly runner-labelled; a logical child Exit cannot
    # replace the DTO or become a runner DWORD.
    rejects = [
        actual2356 + " trailing", actual2356.replace("ServerInternal", "ServerInternalFuture"),
        actual2356.replace("original_runner: None", "original_runner: null"),
        actual2356.replace("startup_context: None", "extra: 0, startup_context: None"),
        actual2356.replace("attempt: 1", "attempt: true"),
        actual2356.replace("attempt: 1", "attempt: 2"),
        actual2356.replace("attempt: 1", "attempt: 0"),
        actual2356.replace("ReturnedError", "ReturnedBackendHandle"),
        actual2356.replace("SameReviewedInvocationStartDataOnly", "SameReviewedInvocationStartDataOnlyFuture"),
        actual2356.replace("snapshot: WindowsStartSnapshot", "snapshot: WindowsStartSnapshotExtra"),
        actual2356.replace("call_sequence: 1", "call_sequence: 0"),
        actual2356.replace("call_sequence: 1", "call_sequence: 18446744073709551616"),
        full.replace("raw_dword: 4294967295", "raw_dword: 4294967296"),
        full.replace("raw_dword: 4294967295", "raw_dword: -1"),
        full.replace("raw_dword: 4294967295", "raw_dword: true"),
        full.replace("runner_pid: 9", "runner_pid: 0"),
        full.replace("original_runner_identity: Some((9, 11))", "original_runner_identity: Some((10, 11))"),
        full.replace("startup_id: 7, call:", "startup_id: 8, call:"),
        full.replace("Exited { raw_dword: 4294967295 }", "Message::Exit { code: 0 }"),
        full.replace("NotObserved(NotAttempted)", "Observed(true)", 1),
        full.replace("NotAllocated, NotAllocated, NotAllocated", "NotAllocated, NotAllocated"),
        full.replace("retirement_error: 0", "retirement_error: 5", 1),
        full.replace("first_failure: None", 'first_failure: Some(("FreeContext", Some(5)))'),
        full.replace("dacl_null: NotObserved(NotAttempted)", "dacl_null: NotObserved(Unlisted)"),
        full.replace(_AUTHZ_SCOPE2364, "arbitrary private text"),
        full.replace("authz_dacl:", "unknown_field: 1, authz_dacl:"),
        fixture(snapshot(attempt("AsUserWriteRoots", "Unclassified")).replace(
            "os_class: NotObserved", "os_class: AccessDenied")),
    ]
    for bad in rejects:
        check(safe_diagnostic(bad) == {"parse_status": "REJECTED"})
        rejected = project_journal(records([row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", bad)]))
        check(rejected["records"][0]["parse_status"] == "REJECTED")
        check("native_diagnostic" not in rejected["records"][0])
    # Pure exact DTO grammar coverage beyond the absent-context real2356 record.
    for reason in ("NotAttempted", "NotApplicable", "OriginalUnavailable", "Busy", "Poisoned",
                   "BoundExceeded", "InvalidBuffer", "Win32 { code: 4294967295 }",
                   "Upstream { stage: QueryUser, reason: Win32 { code: 5 } }"):
        accepts(full.replace("call: NotObserved(NotAttempted)", "call: NotObserved(" + reason + ")", 1))
    for state in ("RunningAtObservation", "SignaledExitQueryFailed { os_class: AccessDenied }",
                  "WaitFailed { os_class: Other }", "UnexpectedWait { wait_result: 258 }"):
        accepts(full.replace("Exited { raw_dword: 4294967295 }", state))
    for text, descriptor in (
        ("Observed(())", _fact2364(_UNIT_2364)),
        ("Observed(false)", _fact2364(_BOOL_2364)),
        ("CloseFailed { code: 5 }", "QueryHandleClose"),
        ("TokenAllocationObservation { api_returned_success: false, allocation: NotAllocated, "
         "win32_error: Some(5) }", "TokenAllocationObservation"),
        ("TokenAllocationObservation { api_returned_success: true, allocation: AllocatedOriginal, "
         "win32_error: None }", "TokenAllocationObservation"),
    ):
        reader = _CurrentReader2364(text)
        reader.read(descriptor)
        check(reader.pos == len(text))
    for text in (
        "TokenAllocationObservation { api_returned_success: false, allocation: AllocatedOriginal, win32_error: Some(5) }",
        "TokenAllocationObservation { api_returned_success: true, allocation: AllocatedOriginal, win32_error: Some(5) }",
        "TokenAllocationObservation { api_returned_success: false, allocation: NotAllocated, win32_error: None }",
    ):
        rejected = False
        try:
            _CurrentReader2364(text).read("TokenAllocationObservation")
        except Rejected:
            rejected = True
        check(rejected)
    # Fixed parser byte/node/depth bounds, no arbitrary strings or side effects.
    check(current_diagnostic2364("x" * (MAX_RECORD + 1)) == {"parse_status": "REJECTED"})


def v3_self_test(check, diagnostic, diagnostic_v2, records, row, private):
    # All fixture text and journal/controller bytes below are synthetic memory.
    def attempt(stage="NotObserved", error="NotObserved", runner="NotObserved", os_class="NotObserved"):
        return ("WindowsStartAttempt { stage: " + stage + ", error: " + error
                + ", runner_stage: " + runner + ", os_class: " + os_class + " }")

    def observation(current=None, first=None, number=1, completion="InProgressOrUnwound", sequence=1):
        return ("Some(LowerBackendCallObservation { call_sequence: " + str(sequence)
                + ", observation: WindowsStartSnapshot { current: " + (current or attempt("RunnerSelection"))
                + ", first_attempt: " + ("None" if first is None else "Some(" + first + ")")
                + ", attempt: " + str(number) + ", completion: " + completion + " } })")

    def diagnostic_v3(lower="None"):
        return (diagnostic_v2.removesuffix(" }").replace("NativeStartDiagnosticV2", "NativeStartDiagnosticV3")
                + ', backend_history_scope: "' + BACKEND_HISTORY_SCOPE
                + '", latest_backend_call_observation: ' + lower + " }")

    def accept(candidate):
        projected = safe_diagnostic(candidate)
        check(projected["parse_status"] == "OK")
        check(set(projected) == {"parse_status", "command_diagnostic", "native_diagnostic"})
        check(set(projected["command_diagnostic"]) == set(COMMAND_FIELDS))
        check(set(projected["native_diagnostic"]) == set(NATIVE_FIELDS_V2) | {
            "backend_history_scope", "latest_backend_call_observation"})
        check(projected["native_diagnostic"]["backend_history_scope"] == BACKEND_HISTORY_SCOPE)
        check(private not in json.dumps(projected))
        return projected["native_diagnostic"]["latest_backend_call_observation"]

    check(safe_diagnostic(diagnostic)["parse_status"] == "OK")
    check(safe_diagnostic(diagnostic_v2)["parse_status"] == "OK")
    check(accept(diagnostic_v3()) is None)
    for completion in ("NotObserved", "UnsupportedBackend"):
        lower = accept(diagnostic_v3(observation(attempt(), number=0, completion=completion)))
        check(lower["observation"]["attempt"] == 0)
        check(lower["observation"]["completion"] == completion)
    for sequence in (1, U64_MAX):
        lower = accept(diagnostic_v3(observation(sequence=sequence)))
        check(lower["call_sequence"] == sequence)
        check(type(lower["call_sequence"]) is int)
    for stage in LOWER_STAGES:
        lower = accept(diagnostic_v3(observation(attempt(stage))))
        check(lower["observation"]["current"]["stage"] == stage)
    for error in LOWER_ERRORS:
        runner = "SpawnChild" if error == "RunnerReported" else "NotObserved"
        os_class = "Other" if error == "OsReported" else "NotObserved"
        completion = "InProgressOrUnwound" if error == "NotObserved" else "ReturnedError"
        lower = accept(diagnostic_v3(observation(attempt("RunnerLogon", error, runner, os_class), completion=completion)))
        check(lower["observation"]["current"]["error"] == error)
    #2357: precise appended labels only, in both original attempt slots.
    # These are synthetic four-field V3 records, not actual VM/native witnesses.
    asuser_stages2357 = (
        "AsUserFilesystemPolicy",
        "AsUserOfflinePolicy",
        "AsUserHomeAndCwdIdentity",
        "AsUserWriteRoots",
        "AsUserReadDeny",
        "AsUserWriteDeny",
        "AsUserReadRoots",
        "AsUserExistingCapabilities",
        "AsUserCapabilitySid",
        "AsUserCapabilityCount",
        "AsUserOwnedCredentials",
    )
    for stage in asuser_stages2357:
        for error in ("Unclassified", "IoInvalidInput"):
            current = attempt(stage, error)
            lower = accept(diagnostic_v3(observation(current, completion="ReturnedError")))
            check(lower["observation"]["current"] == {
                "stage": stage, "error": error,
                "runner_stage": "NotObserved", "os_class": "NotObserved"})
            prior = attempt(stage, error)
            lower = accept(diagnostic_v3(observation(
                attempt("SetupRefresh", "Unclassified"), prior, 2, "ReturnedError")))
            check(lower["observation"]["first_attempt"]["stage"] == stage)
        for invalid in (stage.lower(), '"' + stage + '"', stage + "Future",
                        stage + ", stage: " + stage):
            check(safe_diagnostic(diagnostic_v3(observation(
                attempt(invalid, "Unclassified"), completion="ReturnedError")))
                == {"parse_status": "REJECTED"})
    for error in ("ManagedProfileRequired", "RestrictedFilesystemRequired"):
        lower = accept(diagnostic_v3(observation(
            attempt("PermissionResolve", error), completion="ReturnedError")))
        check(lower["observation"]["current"]["error"] == error)
        for invalid in (error.lower(), '"' + error + '"', error + "Future"):
            check(safe_diagnostic(diagnostic_v3(observation(
                attempt("PermissionResolve", invalid), completion="ReturnedError")))
                == {"parse_status": "REJECTED"})
    # Do not infer a new grammar/OS relationship from new enum membership.
    # The old projection deliberately still rejects these unsupported shapes.
    for current in (
        attempt("AsUserWriteRoots", "IoInvalidInput", os_class="AccessDenied"),
        attempt("AsUserWriteRoots", "Unclassified", runner="SpawnChild"),
    ):
        check(safe_diagnostic(diagnostic_v3(observation(
            current, completion="ReturnedError"))) == {"parse_status": "REJECTED"})
        rejected = project_journal(records([
            row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY",
                diagnostic_v3(observation(current, completion="ReturnedError")))]))
        check(rejected["records"][0]["parse_status"] == "REJECTED")
        check("native_diagnostic" not in rejected["records"][0])
    for runner in RUNNER_STAGES:
        error = "NotObserved" if runner == "NotObserved" else "RunnerReported"
        lower = accept(diagnostic_v3(observation(attempt("SpawnReadyRead", error, runner))))
        check(lower["observation"]["current"]["runner_stage"] == runner)
    for os_class in OS_CLASSES:
        error = "NotObserved" if os_class == "NotObserved" else "OsReported"
        lower = accept(diagnostic_v3(observation(attempt("RunnerLogon", error, os_class=os_class))))
        check(lower["observation"]["current"]["os_class"] == os_class)
    first = attempt("RunnerLogon", "OsReported", os_class="AccountDisabled")
    for number, prior in ((1, None), (2, first)):
        lower = accept(diagnostic_v3(observation(attempt("SpawnReturned"), prior, number, "ReturnedBackendHandle")))
        check(lower["observation"]["completion"] == "ReturnedBackendHandle")
        check(lower["observation"]["attempt"] == number)
        # No synthesis of effects, SDK completion, operation authority or receipt.
        check(set(lower["observation"]) == {"current", "first_attempt", "attempt", "completion"})
    lower = accept(diagnostic_v3(observation(attempt("SetupRefresh"), first, 2)))
    check(lower["observation"]["first_attempt"]["os_class"] == "AccountDisabled")
    lower = accept(diagnostic_v3(observation(attempt("SetupRefresh", "IoAccessDenied"), first, 2, "ReturnedError")))
    check(lower["observation"]["current"]["error"] == "IoAccessDenied")
    check(lower["observation"]["first_attempt"]["error"] == "OsReported")
    for os_class in ("AccountDisabled", "LogonFailure", "NoLogonSession"):
        prior = attempt("SpawnReadyRead", "RunnerReported", "SpawnChild", os_class)
        lower = accept(diagnostic_v3(observation(attempt("SetupRefresh"), prior, 2)))
        check(lower["observation"]["first_attempt"]["os_class"] == os_class)
    # V3 preserves every fixed upper enum and the exact lower/upper separation.
    upper_none = diagnostic_v3()
    for values, field, old in ((WORKER_STAGES, "worker_stage", "PORT_REJECTED"),
                               (WORKER_ERRORS, "worker_error_class", "R2CommitUnknown"),
                               (WORKER_STAGES, "delivery_stage", "RECEIVER_ERROR"),
                               (WORKER_ERRORS, "delivery_error_class", "Unknown"),
                               (NATIVE_STAGES, "stage", "BACKEND_START_RETURNED"),
                               (NATIVE_ERRORS, "error_class", "BackendError"),
                               (BACKEND_ERRORS, "backend_error_class", "ServerInternal")):
        for value in values:
            accept(upper_none.replace(field + ': "' + old + '"', field + ': "' + value + '"'))
    for error in OUTER_ERRORS:
        accept(upper_none.replace("delivery Unknown: Unknown;", "delivery Unknown: " + error + ";"))
    base = diagnostic_v3(observation())
    second = diagnostic_v3(observation(attempt("SetupRefresh"), first, 2))
    bad_inputs = [private + base, base + private, base + "\n", base + "\x00", {"message": base}, None,
                  base.replace("true", '"true"', 1), base.replace("false", '"false"', 1),
                  base.replace(BACKEND_HISTORY_SCOPE, private),
                  base.replace('backend_history_scope: "', 'backend_history_scope: "\\'),
                  base.replace("call_sequence: 1", "call_sequence: 0"),
                  base.replace("call_sequence: 1", "call_sequence: -1"),
                  base.replace("call_sequence: 1", "call_sequence: 01"),
                  base.replace("call_sequence: 1", "call_sequence: +1"),
                  base.replace("call_sequence: 1", "call_sequence: " + str(U64_MAX + 1)),
                  base.replace("call_sequence: 1", 'call_sequence: "1"'),
                  base.replace("call_sequence: 1", "call_sequence: true"),
                  base.replace("call_sequence: 1", "call_sequence: 1.0"),
                  base.replace("attempt: 1", "attempt: 3"), base.replace("attempt: 1", "attempt: 256"),
                  base.replace("attempt: 1", "attempt: 01"), base.replace("attempt: 1", 'attempt: "1"'),
                  base.replace("attempt: 1", "attempt: 0"), second.replace("attempt: 2", "attempt: 1"),
                  base.replace("attempt: 1", "attempt: 2"), second.replace(first, attempt()),
                  base.replace("completion: InProgressOrUnwound", "completion: UnsupportedBackend"),
                  base.replace("completion: InProgressOrUnwound", "completion: NotObserved"),
                  base.replace("completion: InProgressOrUnwound", "completion: ReturnedError"),
                  base.replace("completion: InProgressOrUnwound", "completion: ReturnedBackendHandle"),
                  base.replace("runner_stage: NotObserved", "runner_stage: SpawnChild"),
                  base.replace("os_class: NotObserved", "os_class: Other"),
                  base.replace("error: NotObserved", "error: OsReported"),
                  base.replace("error: NotObserved", "error: RunnerReported"),
                  base.replace("first_attempt: None", "first_attempt: Some(None)"),
                  base.replace("stage: RunnerSelection, error:", "error: NotObserved, stage: RunnerSelection, error:"),
                  base.replace("call_sequence: 1, observation:", "observation:"),
                  base.replace("call_sequence: 1, observation:", "call_sequence: 1, extra: " + private + ", observation:"),
                  base.replace("latest_backend_call_observation:", "extra: true, latest_backend_call_observation:"),
                  base.replace("NativeStartDiagnosticV3", "NativeStartDiagnosticV2"),
                  base.replace("NativeStartDiagnosticV3", "NativeStartDiagnosticV1"),
                  base.replace(" } })", " } }) }")]
    for field, old in (("stage", "RunnerSelection"), ("error", "NotObserved"),
                       ("runner_stage", "NotObserved"), ("os_class", "NotObserved"),
                       ("completion", "InProgressOrUnwound")):
        for value in (private, old.lower(), '"' + old + '"', old + "\\x00", old + "\\\"" ):
            bad_inputs.append(base.replace(field + ": " + old, field + ": " + value))
        bad_inputs.append(base.replace(field + ": " + old, field + ": " + old + ", " + field + ": " + old))
    for bad in bad_inputs:
        check(safe_diagnostic(bad) == {"parse_status": "REJECTED"})
        rejected = project_journal(records([row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", bad)]))
        check(rejected["records"][0]["parse_status"] == "REJECTED")
        check("native_diagnostic" not in rejected["records"][0])
        check(private not in json.dumps(rejected))
    for candidate in (upper_none, base, second,
                      diagnostic_v3(observation(attempt("SpawnReturned"), completion="ReturnedBackendHandle"))):
        projected = project_journal(records([
            row(0, "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", {"step": "sealed-native-start", "review": private}),
            row(1, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", candidate)]))
        check(projected["parse_status"] == "OK")
        check(projected["records"][1]["step"] == "sealed-native-start")
        check(projected["records"][1]["native_diagnostic"]["backend_history_scope"] == BACKEND_HISTORY_SCOPE)
        check(private not in json.dumps(projected))
    for bad in (base.replace("None", "null"), base.replace("WindowsStartSnapshot", private),
                base.replace("LowerBackendCallObservation", private),
                second.replace("first_attempt: Some(", 'first_attempt: "Some('),
                base.replace("current: WindowsStartAttempt", "current: {message: " + private + "}, WindowsStartAttempt")):
        check(safe_diagnostic(bad) == {"parse_status": "REJECTED"})


def self_test():
    # Memory-only synthetic fixtures. No input/file/VM access in this branch.
    checks = 0
    private = "SYNTHETIC_PRIVATE_PAYLOAD_ACCOUNT_SID_PATH_ARGV_ENV"
    diagnostic = ('native start delivery Unknown: Unknown; diagnostic=CommandDiagnosticV1 { worker_stage: "PORT_REJECTED", worker_error_class: "R2CommitUnknown", delivery_stage: "RECEIVER_ERROR", delivery_error_class: "Unknown", command_started_observed: true, provider_observed: false }; native=NativeStartDiagnosticV1 { stage: "BACKEND_START_RETURNED", error_class: "BackendError", invocation_entered_observed: true, backend_task_entered_observed: true, actual_handle_observed: false, provider_ready_observed: false }')

    def check(condition):
        nonlocal checks
        if not condition:
            raise Rejected()
        checks += 1

    def records(rows):
        return b"".join(json.dumps(r).encode() + b"\n" for r in rows)

    def row(seq, kind, payload):
        return {"sequence": seq, "kind": kind, "payload": payload}

    parsed = safe_diagnostic(diagnostic)
    check(parsed["parse_status"] == "OK")
    check(set(parsed["command_diagnostic"]) == set(COMMAND_FIELDS))
    check(set(parsed["native_diagnostic"]) == set(NATIVE_FIELDS))
    check(parsed["native_diagnostic"]["actual_handle_observed"] is False)
    for bad in (private + diagnostic, diagnostic + private, diagnostic + "\n", diagnostic.replace('"BackendError"', '"' + private + '"'), diagnostic.replace("true", '"true"', 1), {"message": diagnostic}, None):
        check(safe_diagnostic(bad) == {"parse_status": "REJECTED"})
    for text in FIXED_NONDIAGNOSTIC:
        check(safe_diagnostic(text) == {"parse_status": "MISSING"})
    for stage in WORKER_STAGES:
        check(safe_diagnostic(diagnostic.replace('worker_stage: "PORT_REJECTED"', 'worker_stage: "' + stage + '"'))["parse_status"] == "OK")
    for error in WORKER_ERRORS:
        check(safe_diagnostic(diagnostic.replace('worker_error_class: "R2CommitUnknown"', 'worker_error_class: "' + error + '"'))["parse_status"] == "OK")
    for stage in NATIVE_STAGES:
        check(safe_diagnostic(diagnostic.replace('stage: "BACKEND_START_RETURNED"', 'stage: "' + stage + '"'))["parse_status"] == "OK")
    for error in NATIVE_ERRORS:
        check(safe_diagnostic(diagnostic.replace('error_class: "BackendError"', 'error_class: "' + error + '"'))["parse_status"] == "OK")
    raw = records([row(0, "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", {"step": "sealed-native-start", "review": private}), row(1, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", diagnostic)])
    journal = project_journal(raw)
    check(journal["parse_status"] == "OK")
    check(journal["records"][1]["step"] == "sealed-native-start")
    check(journal["records"][1]["native_diagnostic"]["error_class"] == "BackendError")
    check(private not in json.dumps(journal))
    check(project_journal(raw[:-1])["parse_status"] == "TRUNCATED")
    check(len(project_journal(raw[:-1])["records"]) == 1)
    check(project_journal(b'') == {"parse_status": "MISSING", "records": []})
    for bad in (b'\xff\n', b'{"sequence":0,"sequence":0,"kind":"acceptance","payload":null}\n', records([row(True, "acceptance", None)]), records([row(1, "acceptance", None)]), records([row(0, "acceptance", None), row(0, "acceptance", None)]), b'\n', b'{"sequence":0,"kind":"acceptance","payload":NaN}\n', records([row(i, "acceptance", None) for i in range(257)]), b'x' * (MAX_RECORD + 1) + b'\n'):
        check(project_journal(bad) == {"parse_status": "REJECTED", "records": []})
    rejected = project_journal(records([row(0, private, private)]))
    check(private not in json.dumps(rejected))
    check(rejected["records"][0]["kind"] == "NOT_ALLOWLISTED")
    wrong_shape = project_journal(records([row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", {"message": diagnostic})]))
    check(wrong_shape["records"][0]["parse_status"] == "REJECTED")
    begin = project_journal(records([
        row(0, "acceptance", {"ignored": private}),
        row(1, "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", {"step": "guest-identity", "review": private}),
        row(2, "guest-identity-actual-exit", {"ignored": private}),
        row(3, "guest-identity-validated", {"ignored": private}),
        row(4, "lifecycle-initial", {"ignored": private})]))
    check(begin["parse_status"] == "OK")
    check(all(r["parse_status"] == "OK" for r in begin["records"]))
    check(begin["records"][1]["step"] == "guest-identity")
    check(set(begin["records"][2]) == {"sequence", "kind", "parse_status"})
    check(private not in json.dumps(begin))
    check("guest-identity" not in STEPS and "guest-identity" in JOURNAL_STEPS)
    unknown_step = project_journal(records([row(0, "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", {"step": private})]))
    check(unknown_step["records"][0]["parse_status"] == "REJECTED")
    check(private not in json.dumps(unknown_step))
    check(project_controller(b'{"Command":"guest-identity"}') == {"parse_status": "REJECTED"})
    control = project_controller(json.dumps({"State": "WAITING_EXPLICIT_START", "NoSystemAcceptance": True, "HostPid": private, "VmId": private, "CredentialLogging": "NONE", "message": private}).encode())
    check(control["parse_status"] == "OK")
    check(private not in json.dumps(control))
    for obj in ({"Status": private}, {"Blocked": "true"}, {"PendingCommand": private}, {"JournalBytes": MAX_JOURNAL + 1}, {"ActualExit": True}, {"NextIndex": -1}):
        check(project_controller(json.dumps(obj).encode()) == {"parse_status": "REJECTED"})
    check(project_controller(b'{"Blocked":true,"Blocked":false}') == {"parse_status": "REJECTED"})
    check(project_controller(b'[]') == {"parse_status": "REJECTED"})
    check(project_controller(json.dumps({"message": private}).encode()) == {"parse_status": "MISSING"})
    check(project_controller(b'x' * (MAX_CONTROLLER + 1)) == {"parse_status": "REJECTED"})
    for name in CONTROL_FILES:
        check(controller_allowed(name))
    for name in ("poll-0001.json", "attempt-01.json", "delivery-01.json", "group-native-eight-stopped.json", "group-session-four-returned.json"):
        check(controller_allowed(name))
    for name in ("password.json", "poll-0001.json.bak", "group-" + private + "-stopped.json", "Out.log", "Err.log"):
        check(not controller_allowed(name))
    # Successor cases remain memory-only and include all 23 closed class names.
    diagnostic_v2 = diagnostic.replace("NativeStartDiagnosticV1", "NativeStartDiagnosticV2").replace(
        'error_class: "BackendError", invocation_entered_observed:',
        'error_class: "BackendError", backend_error_class: "ServerInternal", invocation_entered_observed:')
    v2 = safe_diagnostic(diagnostic_v2)
    check(v2["parse_status"] == "OK")
    check(set(v2["command_diagnostic"]) == set(COMMAND_FIELDS))
    check(set(v2["native_diagnostic"]) == set(NATIVE_FIELDS_V2))
    check(len(v2["command_diagnostic"]) + len(v2["native_diagnostic"]) == 13)
    check(len(parsed["command_diagnostic"]) + len(parsed["native_diagnostic"]) == 12)
    check(v2["native_diagnostic"]["backend_error_class"] == "ServerInternal")
    check(v2["native_diagnostic"]["actual_handle_observed"] is False)
    for error in BACKEND_ERRORS:
        candidate = diagnostic_v2.replace('backend_error_class: "ServerInternal"', 'backend_error_class: "' + error + '"')
        projected = safe_diagnostic(candidate)
        check(projected["parse_status"] == "OK")
        check(projected["native_diagnostic"]["backend_error_class"] == error)
    for bad in (
        private + diagnostic_v2, diagnostic_v2 + private, diagnostic_v2 + "\n",
        diagnostic_v2.replace('backend_error_class: "ServerInternal"', 'backend_error_class: "' + private + '"'),
        diagnostic_v2.replace('backend_error_class: "ServerInternal"', 'backend_error_class: "serverinternal"'),
        diagnostic_v2.replace('backend_error_class: "ServerInternal"', 'backend_error_class: 16'),
        diagnostic_v2.replace("true", '"true"', 1),
        diagnostic_v2.replace("false", '"false"'),
        diagnostic_v2.replace('provider_ready_observed: false }', 'provider_ready_observed: false, extra: "' + private + '" }'),
        diagnostic_v2.replace('backend_error_class: "ServerInternal", ', ''),
        diagnostic_v2.replace('backend_error_class: "ServerInternal", ', 'backend_error_class: "ServerInternal", backend_error_class: "Spawn", '),
        diagnostic_v2.replace('backend_error_class: "ServerInternal", invocation_entered_observed: true', 'invocation_entered_observed: true, backend_error_class: "ServerInternal"'),
        diagnostic_v2.replace("NativeStartDiagnosticV2", "NativeStartDiagnosticV1"),
        diagnostic_v2.replace("NativeStartDiagnosticV2", "NativeStartDiagnosticV3"),
        diagnostic_v2.replace('stage: "BACKEND_START_RETURNED"', 'stage: "' + private + '"'),
        diagnostic_v2.replace('error_class: "BackendError"', 'error_class: "' + private + '"'),
        diagnostic_v2.replace('worker_stage: "PORT_REJECTED"', 'worker_stage: "' + private + '"'),
        diagnostic_v2.replace('provider_observed: false };', 'provider_observed: false, extra: true };'),
        {"message": diagnostic_v2}, None):
        check(safe_diagnostic(bad) == {"parse_status": "REJECTED"})
    v2_journal = project_journal(records([
        row(0, "ATTEMPT_RESERVED_OUTCOME_UNKNOWN", {"step": "sealed-native-start", "review": private}),
        row(1, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", diagnostic_v2)]))
    check(v2_journal["parse_status"] == "OK")
    check(v2_journal["records"][1]["step"] == "sealed-native-start")
    check(v2_journal["records"][1]["native_diagnostic"]["backend_error_class"] == "ServerInternal")
    check(private not in json.dumps(v2_journal))
    for payload in ({"message": diagnostic_v2}, diagnostic_v2 + private):
        rejected_v2 = project_journal(records([row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", payload)]))
        check(rejected_v2["records"][0]["parse_status"] == "REJECTED")
        check("native_diagnostic" not in rejected_v2["records"][0])
        check(private not in json.dumps(rejected_v2))
    missing = project_journal(records([row(0, "SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", "native start timeout; Unknown; no replay")]))
    check(missing["records"][0]["parse_status"] == "MISSING")
    check("native_diagnostic" not in missing["records"][0])
    check(project_journal(b"x" * (MAX_JOURNAL + 1)) == {"parse_status": "REJECTED", "records": []})
    check(project_journal(b'{"sequence":0,"kind":"acceptance","payload":{"nested":1,"nested":2}}\n') == {"parse_status": "REJECTED", "records": []})
    check(project_journal(records([dict(row(0, "acceptance", None), extra=private)])) == {"parse_status": "REJECTED", "records": []})
    v1_v2_checks = checks
    v3_self_test(check, diagnostic, diagnostic_v2, records, row, private)
    current_self_test2364(check, records, row)
    print(json.dumps({"schema": VERSION, "parse_status": "SYNTHETIC_TESTS_PASSED", "checks": checks,
                      "v1_v2_checks": v1_v2_checks, "v3_checks": checks - v1_v2_checks}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except Exception:
        print(json.dumps({"schema": VERSION, "parse_status": "REJECTED_ARGUMENTS_OR_INTERNAL_FAILURE"}, sort_keys=True))
        sys.exit(2)
