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
LOWER_STAGES = frozenset("NotObserved RunnerSelection RequestPrepare ProcessMapReserve MatchedRouteCheck CodexHomeResolve PermissionResolve SandboxBaseReady AccountSelect SetupRefresh CapabilityResolve BlockingTransportTask DesktopPolicy RunnerResolve RunnerPinVerify RegisteredAlias PrivateDesktop PipeCreateIn PipeCreateOut RunnerLogon RegisteredImageVerify PipeConnectIn PipeConnectOut ControlHello SpawnRequestWrite SpawnReadyRead CheckedDriverAssemble SpawnReturned".split())
LOWER_ERRORS = frozenset("NotObserved InvalidParams InvalidRequest ServerInternal OtherRpc RouteRejected PinRejected IdentityRejected ControlRejected IoAccessDenied IoNotFound IoInvalidInput IoTimedOut IoInterrupted IoBrokenPipe IoOther OsReported RunnerReported BlockingCancelled BlockingPanic Unclassified".split())
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
            return {"parse_status": "REJECTED"}
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
    print(json.dumps({"schema": VERSION, "parse_status": "SYNTHETIC_TESTS_PASSED", "checks": checks,
                      "v1_v2_checks": v1_v2_checks, "v3_checks": checks - v1_v2_checks}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except Exception:
        print(json.dumps({"schema": VERSION, "parse_status": "REJECTED_ARGUMENTS_OR_INTERNAL_FAILURE"}, sort_keys=True))
        sys.exit(2)
