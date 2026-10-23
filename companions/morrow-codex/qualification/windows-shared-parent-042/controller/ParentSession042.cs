#if P2_SHARED_PARENT_REFUSAL_SESSION
using System;
using System.Collections.Generic;
namespace C28.RootP1682 {
 internal sealed class SharedRound042 {
  internal readonly int Ordinal;
  internal readonly StepGate1725 Gate=new StepGate1725(RolePins1725.ShellSha);
  internal readonly JournalPrefix1725 Prefix=new JournalPrefix1725();
  internal readonly FileImage1725 Evidence,Journal,Report;
  internal RoundPrompt042 Prompt {get;private set;}
  internal int PromptBaseline {get;private set;}
  internal bool AttemptClosed {get;private set;}
  internal bool CommonOwnerClosed {get;private set;}
  internal bool Completed {get;private set;}
  internal bool Failed {get;private set;}
  internal int AttemptSequence=-1,CommonSequence=-1;
  internal Dictionary<string,object> ReportSummary;
  ulong NativeAttempt;bool FinishedAdmission;
  internal SharedRound042(int ordinal){
   if(ordinal<1||ordinal>17)throw new InvalidOperationException("FIXED_ATTEMPT_RANGE");
   Ordinal=ordinal;string label=ordinal.ToString("D2");
   Evidence=new FileImage1725("ATTEMPT_"+label+"_EVIDENCE");
   Journal=new FileImage1725("ATTEMPT_"+label+"_JOURNAL");
   Report=new FileImage1725("ATTEMPT_"+label+"_REPORT");
  }
  internal void Register(List<FileImage1725> originalResources){originalResources.Add(Evidence);originalResources.Add(Journal);originalResources.Add(Report);}
  void Need(bool b){if(!b){Failed=true;throw new InvalidOperationException("SHARED_PARENT_ATTEMPT_EVIDENCE");}}
  static bool B(Dictionary<string,object> p,string k){return Json1725.Bool(Json1725.Get(p,k));}
  static ulong U(Dictionary<string,object> p,string k){return Json1725.UIntNumber(Json1725.Get(p,k));}
  static string S(Dictionary<string,object> p,string k){return Json1725.Str(Json1725.Get(p,k));}
  internal void Begin(byte[] original,int lowerBound){
   try{Need(!Failed&&Prompt==null&&!Completed);PromptBaseline=lowerBound;Prompt=new RoundPrompt042(original,lowerBound);}catch{Failed=true;throw;}
  }
  internal void Accept(JournalRecord1725 record){
   try{
    Need(!Failed&&!Completed);
    var p=record.Payload as Dictionary<string,object>;
    if(record.Kind=="ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE"&&Gate.Current!=null&&Gate.Current.Command=="sealed-native-start"){
     Need(NativeAttempt==0&&p!=null);NativeAttempt=U(p,"startup_attempt");Need(NativeAttempt>0);
    }
    if(record.Kind=="lifecycle-after-attempt"&&p!=null&&S(p,"phase")=="Finished"){
     Need(!FinishedAdmission&&NativeAttempt>0&&Gate.Current!=null&&Gate.Current.Command=="finish-cleanup");
     Need(Json1725.Get(p,"execution_cleanup_admission")==null&&!B(p,"cleanup_guard_held")&&!B(p,"owned_setup_proof_held"));
     var a=Json1725.Obj(Json1725.Get(p,"refusal_cleanup_admission"));
     Need(U(a,"startup_attempt")==NativeAttempt&&U(a,"setup_child_pid")>0
      &&S(a,"outcome")=="KNOWN_SOURCE_TARGET_POLICY_REJECTED_RESOURCES_CLOSED_BEFORE_DISPATCH"
      &&!a.ContainsKey("win32_error")&&!a.ContainsKey("native_start_handles_closed")
      &&B(a,"native_caller_handles_closed")&&B(a,"original_blocking_return_acknowledged")
      &&B(a,"local_and_sdk_slots_retired")&&B(a,"original_worker_joined")
      &&!B(a,"owner_finished")&&B(a,"owner_attempt_finished")&&B(a,"shared_owner_retained")
      &&B(a,"factory_released")&&!B(a,"runner_was_started")&&!B(a,"normal_terminal_ack_claimed"));
     FinishedAdmission=true;
    }
    if(record.Kind=="ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED"){
     Need(Gate.Completed==47&&Gate.Current.Returned&&FinishedAdmission&&!AttemptClosed&&p!=null&&p.Count==6
      &&U(p,"attempt")==((ulong)Ordinal)&&B(p,"same_protected_owner")&&B(p,"same_sdk_resources")
      &&!B(p,"common_parent_closed")&&!B(p,"native_replay")&&!B(p,"full_P2_accepted")
      &&record.Sequence>Gate.Current.ReturnSequence.Value);
     AttemptClosed=true;AttemptSequence=record.Sequence;
    }
    if(record.Kind=="ORIGINAL_SHARED_PROTECTED_OWNER_FINISHED"){
     Need(Ordinal==17&&AttemptClosed&&!CommonOwnerClosed&&p!=null&&p.Count==5
      &&U(p,"closed_attempts")==17&&B(p,"same_protected_owner")&&B(p,"same_sdk_resources")
      &&!B(p,"common_parent_closed")&&!B(p,"full_P2_accepted")&&record.Sequence>AttemptSequence);
     CommonOwnerClosed=true;CommonSequence=record.Sequence;
    }
   }catch{Failed=true;throw;}
  }
  internal void Complete(){
   Need(!Failed&&!Completed&&Prompt!=null&&Gate.Completed==47&&!Gate.Failed&&FinishedAdmission
    &&AttemptClosed&&(Ordinal==17?CommonOwnerClosed:!CommonOwnerClosed)&&Prefix.Parsed==Prefix.Length);
   Completed=true;
  }
 }
}
#endif
