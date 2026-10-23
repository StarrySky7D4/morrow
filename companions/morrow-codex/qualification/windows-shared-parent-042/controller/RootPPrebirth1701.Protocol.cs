// Read-only public1703 DATA, matched to frozen1674 command order.
// No queued actions, automatic approvals, VM route, credentials or input effects.
using System;
namespace C28.RootP1682 {
 internal static class InputProtocol1701 {
#if P2_SHARED_PARENT_REFUSAL_SESSION
  internal const int Count=1564,NormalBytes=60469;
  internal const string RunFlag="--run-p2-shared-parent-refusal-session",TerminalStatus="P2_SEVENTEEN_SHARED_PARENT_REFUSALS_CLOSED_NOT_FULL_P2_OR_RELEASE";
#elif P2_SOURCE_POLICY_REFUSAL_QUALIFICATION
  internal const int Count=92,NormalBytes=3557;
  internal const string RunFlag="--run-p2-source-policy-refusal",TerminalStatus="P2_SOURCE_POLICY_REFUSAL_WORKFLOW_CLOSED_NOT_ORIGINAL47_OR_NINE_QUALIFICATION";
#else
  internal const int Count=92,NormalBytes=3523;
  internal const string RunFlag="--run-1725",TerminalStatus="47_STEP_RETURNS_AND_ORIGINAL_HELPER_CLOSED_NOT_NINE_GATE_QUALIFICATION";
#endif
  static readonly string[] Lines={
   "READ_ACTUAL_LOCAL_HYPERV_GUEST_IDENTITY_AND_CREATE_FRESH_EVIDENCE",
   "inventory",
   "READ_ONLY_FIXED_SANDBOX_IDENTITY_AND_REGISTRATION_INVENTORY",
   "roots",
   "CREATE_ONLY_FRESH_SYNTHETIC_VM_ROOT",
   "setup",
   "APPLY_FIXED_SANDBOX_ACCOUNTS_DPAPI_ACL_WFP_IN_DISPOSABLE_VM",
   "materialize",
   "MATERIALIZE_AND_PIN_CHECKED_RUNNER_IN_OWN_SYNTHETIC_HOME",
   "open-owner",
   "OPEN_NEW_ORIGINAL_PROTECTED_WORKBENCH_IN_SYNTHETIC_ROOT",
   "production-factory-v8",
   "CREATE_REAL_EXPLICIT_V8_NATIVE_CLOSE_FACTORY_WITHOUT_STARTING_CHILD",
   "sealed-basic",
   "CREATE_FRESH_OWNED_BASIC_FILES_AND_LOOPBACK_LISTENER_WITH_PINNED_GUEST_SID_QUERY",
   "sealed-session-review",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_REVIEW",
   "sealed-session-install",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_INSTALL",
   "sealed-session-base-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_BASE_SELECT",
   "sealed-session-base-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_BASE_ENABLE",
   "sealed-session-wrapper-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WRAPPER_SELECT",
   "sealed-session-approve",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_APPROVE",
   "sealed-session-wrapper-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WRAPPER_ENABLE",
   "sealed-session-context",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_CONTEXT",
   "sealed-session-worker",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WORKER",
   "sealed-session-run",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_RUN",
   "sealed-session-join",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_JOIN",
   "sealed-native-review",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_REVIEW",
   "sealed-controls-review",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_REVIEW",
   "sealed-controls-install",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_INSTALL",
   "sealed-controls-base-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_BASE_SELECT",
   "sealed-controls-base-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_BASE_ENABLE",
   "sealed-controls-wrapper-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_WRAPPER_SELECT",
   "sealed-controls-approve",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_APPROVE",
   "sealed-controls-wrapper-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_WRAPPER_ENABLE",
   "sealed-proposal-review",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_REVIEW",
   "sealed-proposal-install",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_INSTALL",
   "sealed-proposal-base-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_BASE_SELECT",
   "sealed-proposal-base-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_BASE_ENABLE",
   "sealed-proposal-wrapper-select",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_WRAPPER_SELECT",
   "sealed-proposal-approve",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_APPROVE",
   "sealed-proposal-wrapper-enable",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_WRAPPER_ENABLE",
   "sealed-native-context",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_CONTEXT",
   "sealed-native-worker",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_WORKER",
   "sealed-propose",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSE",
   "sealed-trusted-review",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_TRUSTED_REVIEW",
   "sealed-trusted-approve-claim",
   "TRUSTED_APPROVE_AND_CLAIM_ONLY_THE_ACTUAL_SEALED_GUEST_PROPOSAL",
#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION
   "sealed-qualify-source-policy-refusal",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_QUALIFY_SOURCE_POLICY_REFUSAL",
   "sealed-native-start",
   "START_ONE_REVIEWED_CHECKED_SANDBOX_CHILD_IN_APPROVED_DISPOSABLE_GUEST",
   "sealed-refusal-join",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_REFUSAL_JOIN",
#else
   "sealed-native-start",
   "START_ONE_REVIEWED_CHECKED_SANDBOX_CHILD_IN_APPROVED_DISPOSABLE_GUEST",
   "sealed-basic-observe",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_BASIC_OBSERVE",
   "sealed-native-join",
   "EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_JOIN",
#endif
   "release-factory",
   "RELEASE_IDLE_FACTORY_AND_SYNCHRONOUSLY_JOIN_ITS_UNIQUE_SCHEDULER",
   "finish-owner",
   "FINISH_ORIGINAL_OWNER_ONLY_AFTER_REAL_JOINS",
   "prepare-cleanup",
   "DISABLE_OWNED_SANDBOX_ACCOUNTS_AND_STOP_THEIR_PROCESSES",
   "finish-cleanup",
   "REMOVE_ONLY_OWNED_SANDBOX_RESOURCES_AND_PROTECTIONS",
   "exit"
  };
  static readonly int[] Steps={1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13,14,14,15,15,16,16,17,17,18,18,19,19,20,20,21,21,22,22,23,23,24,24,25,25,26,26,27,27,28,28,29,29,30,30,31,31,32,32,33,33,34,34,35,35,36,36,37,37,38,38,39,39,40,40,41,41,42,42,43,43,44,44,45,45,46,46,47};
  static readonly string[] Kinds={"confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","command","confirmation","exit"};
  internal static string Text(int index){if(index<0||index>=Count)return null;return Lines[index%92];}
  internal static string Kind(int index){if(index<0||index>=Count)return null;return Kinds[index%92];}
  internal static int Step(int index){if(index<0||index>=Count)return 0;return 47*(index/92)+Steps[index%92];}
  internal static byte[] Wire(int index){string s=Text(index);if(s==null)throw new InvalidOperationException("INPUT_PROTOCOL_END");return InputRules1701.Line(s);}
  internal static bool Matches(int index,byte[] wire){return index>=0&&index<Count&&Rules.Equal(Wire(index),wire);}
  // A literal, complete line in the ORIGINAL published stdout prefix. This only
  // correlates a confirmation prompt; it does not prove business success/authority.
#if P2_SHARED_PARENT_REFUSAL_SESSION
  internal const int FrameCapacity=1564;
  internal static bool Terminal(int index,byte[] wire){return index==Count-1&&InputRules1701.Exit(wire);}
#else
  internal const int FrameCapacity=128;
  internal static bool Terminal(int index,byte[] wire){return InputRules1701.Exit(wire);}
#endif
  internal static bool Failure(byte[] original,int from,int through) {
   string[] markers={"ORIGINAL_PARENT_SESSION_UNKNOWN_RETAINED_NO_NEXT_ATTEMPT","FAIL_PRESERVED_NO_AUTOMATIC_RETRY","; original same-process state retained. No automatic retry.","INTERACTIVE_OR_EVIDENCE_LOST_STATE_RETAINED","Interactive/evidence channel ended:"};
   if(original==null||from<0||through<from||through>original.Length)return true;
   foreach(string token in markers)for(int at=from;at+token.Length<=through;at++){
    bool match=true;for(int i=0;i<token.Length;i++)if(original[at+i]!=(byte)token[i]){match=false;break;}
    if(match)return true;
   }
   return false;
  }
  internal static bool Prompt(byte[] output,int from,int through,int index) {
   if(index<0||index>=Count)return false;
   if(Kind(index)!="confirmation")return true;
   string token="Type exactly: "+Text(index);
   if(output==null||from<0||through<from||through>output.Length)return false;
   for(int at=from;at+token.Length<through;at++){
    if(at>0&&output[at-1]!=10)continue;
    bool match=true;for(int i=0;i<token.Length;i++)if(output[at+i]!=(byte)token[i]){match=false;break;}
    if(!match)continue;int end=at+token.Length;
    if(output[end]==10||(output[end]==13&&end+1<through&&output[end+1]==10))return true;
   }
   return false;
  }
 }
}
