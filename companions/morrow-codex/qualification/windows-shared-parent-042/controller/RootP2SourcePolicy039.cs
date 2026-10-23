// A separately compiled P2 qualification protocol. These journal predicates
// are evidence gates only; they do not create native/SDK/cleanup authority.
#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION
using System;
using System.Collections.Generic;
namespace C28.RootP1682 {
 internal sealed class RefusalGate039 {
  int stage; bool failed;
  internal ulong Attempt {get; private set;}
  void Need(bool value){if(!value)throw new InvalidOperationException("P2_SOURCE_POLICY_REFUSAL_EVIDENCE");}
  bool B(Dictionary<string,object> p,string key){return Json1725.Bool(Json1725.Get(p,key));}
  ulong U(Dictionary<string,object> p,string key){return Json1725.UIntNumber(Json1725.Get(p,key));}
  string S(Dictionary<string,object> p,string key){return Json1725.Str(Json1725.Get(p,key));}
  internal void Accept(string command,Dictionary<string,object> p){
   try{
    Need(!failed);
    if(command=="sealed-qualify-source-policy-refusal"){
     Need(stage==0&&p.Count==6&&B(p,"original_factory_input_armed")&&B(p,"denial_only_policy")
      &&!B(p,"stored_password_changed")&&!B(p,"account_changed")
      &&!B(p,"native_logon_observed")&&!B(p,"cleanup_authority"));stage=1;
    }else if(command=="sealed-native-start"){
     Need(stage==1&&p.Count==9&&B(p,"native_start_refused")&&S(p,"reason")=="KNOWN_SOURCE_TARGET_POLICY_REJECTED"&&!p.ContainsKey("win32_error")
      &&U(p,"startup_attempt")>0&&B(p,"local_and_sdk_slots_retired")&&Json1725.Get(p,"actual_started_handle")==null
      &&Json1725.Get(p,"exit_code")==null&&!B(p,"normal_terminal_ack_claimed")
      &&!B(p,"claim_replay_allowed")&&!B(p,"outer_worker_factory_setup_retired"));
     Attempt=U(p,"startup_attempt");stage=2;
    }else if(command=="sealed-refusal-join"){
     Need(stage==2&&p.Count==10&&B(p,"actual_worker_join")&&B(p,"original_task_acknowledged")
      &&B(p,"local_and_sdk_slots_retired")&&B(p,"no_business_process_started")
      &&S(p,"original_operation_id").Length>0&&S(p,"original_operation_id").Length<=128
      &&U(p,"original_record_revision")>0&&B(p,"durable_invocation_spent")
      &&S(p,"historical_phase")=="DispatchUnknown"&&!B(p,"execution_facts_synthesized")&&!B(p,"claim_replay_allowed"));stage=3;
    }else if(command=="sealed-basic-observe"||command=="sealed-native-join")Need(false);
   }catch{failed=true;throw;}
  }
  internal void Complete(){try{Need(!failed&&stage==3&&Attempt>0);}catch{failed=true;throw;}}
 }
 public static class P2SourcePolicyPure039 {
  static int passed;
  static readonly string[] Commands={"sealed-qualify-source-policy-refusal","sealed-native-start","sealed-refusal-join"};
  static readonly string[] Payloads={
   "{\"original_factory_input_armed\":true,\"denial_only_policy\":true,\"stored_password_changed\":false,\"account_changed\":false,\"native_logon_observed\":false,\"cleanup_authority\":false}",
   "{\"native_start_refused\":true,\"reason\":\"KNOWN_SOURCE_TARGET_POLICY_REJECTED\",\"startup_attempt\":1,\"local_and_sdk_slots_retired\":true,\"actual_started_handle\":null,\"exit_code\":null,\"normal_terminal_ack_claimed\":false,\"claim_replay_allowed\":false,\"outer_worker_factory_setup_retired\":false}",
   "{\"actual_worker_join\":true,\"original_task_acknowledged\":true,\"local_and_sdk_slots_retired\":true,\"no_business_process_started\":true,\"original_operation_id\":\"same-original-op\",\"original_record_revision\":3,\"durable_invocation_spent\":true,\"historical_phase\":\"DispatchUnknown\",\"execution_facts_synthesized\":false,\"claim_replay_allowed\":false}"};
  static Dictionary<string,object> Data(int index){return Json1725.Obj(Json1725.Parse(Payloads[index]));}
  static void Assert(bool value){if(!value)throw new Exception("PURE_ASSERT");}
  static void Pass(string name){Console.WriteLine("PASS P2SourcePolicy039::"+name);passed++;}
  static void Reject(Action action){bool caught=false;try{action();}catch(InvalidOperationException){caught=true;}Assert(caught);}
  static RefusalGate039 Prior(int through){var gate=new RefusalGate039();for(int i=0;i<through;i++)gate.Accept(Commands[i],Data(i));return gate;}
  public static int Main(string[] args){
   if(args.Length!=1||args[0]!="--p2-refusal-pure")return 78;
   int wire=0;for(int i=0;i<InputProtocol1701.Count;i++)wire+=InputProtocol1701.Wire(i).Length;
   Assert(InputProtocol1701.Count==92&&wire==InputProtocol1701.NormalBytes&&wire!=3523);
   var plan=Plan1725.NewSteps();Assert(plan.Length==47);
   for(int i=0;i<3;i++)Assert(plan[39+i].Command==Commands[i]);
   Assert(InputProtocol1701.TerminalStatus=="P2_SOURCE_POLICY_REFUSAL_WORKFLOW_CLOSED_NOT_ORIGINAL47_OR_NINE_QUALIFICATION");
   Pass("separate_profile_exact_wire_and_three_ordered_steps");
   string mode,tag,pin;string t=new string('a',32),p=new string('b',64);
   Assert(!Invocation1725.Parse(new[]{"--run-1725",t,p},out mode,out tag,out pin));
   Assert(Invocation1725.Parse(new[]{"--run-p2-source-policy-refusal",t,p},out mode,out tag,out pin));
   Pass("p2_binary_requires_distinct_explicit_run_flag");
   Prior(3).Complete();Pass("only_complete_three_stage_refusal_evidence_accepted");
   // Exact original case037 output omitted a Win32 error. A fabricated nullable
   // field or a logon-error code is an extra diagnostic and remains rejected.
   object[] wrongCodes={null,new JsonNumber1725("0"),new JsonNumber1725("1326")};
   for(int i=0;i<wrongCodes.Length;i++){
    var gate=Prior(1);var wrong=Data(1);wrong.Add("win32_error",wrongCodes[i]);
    Reject(()=>gate.Accept(Commands[1],wrong));Pass("spurious_win32_field_"+i+"_refused");
   }
   for(int stage=0;stage<3;stage++){
    int selected=stage;Reject(()=>Prior(selected).Complete());Pass("incomplete_stage_"+stage+"_cannot_finish");
    foreach(string key in new List<string>(Data(stage).Keys)){
     var gate=Prior(stage);var missing=Data(stage);missing.Remove(key);
     Reject(()=>gate.Accept(Commands[selected],missing));
     Reject(()=>gate.Accept(Commands[selected],Data(selected)));
     Pass("missing_"+stage+"_"+key+"_and_late_correction_stays_failed");
     var original=Data(stage)[key];var invalid=Data(stage);
     invalid[key]=original is bool?(object)(!(bool)original):original==null?(object)new JsonNumber1725("0"):
      original is JsonNumber1725?(object)new JsonNumber1725("0"):(object)"";
     gate=Prior(stage);Reject(()=>gate.Accept(Commands[selected],invalid));Pass("invalid_"+stage+"_"+key+"_refused");
    }
    var extra=Data(stage);extra.Add("unreviewed_authority",true);var extraGate=Prior(stage);
    Reject(()=>extraGate.Accept(Commands[selected],extra));Pass("extra_payload_"+stage+"_refused");
    var repeated=Prior(stage+1);Reject(()=>repeated.Accept(Commands[selected],Data(selected)));Pass("repeated_stage_"+stage+"_refused");
   }
   Reject(()=>Prior(0).Accept(Commands[1],Data(1)));Pass("native_refusal_without_qualified_input_refused");
   Reject(()=>Prior(1).Accept(Commands[2],Data(2)));Pass("worker_join_without_native_refusal_refused");
   Reject(()=>Prior(2).Accept("sealed-basic-observe",new Dictionary<string,object>()));Pass("business_observation_is_not_refusal");
   Reject(()=>Prior(2).Accept("sealed-native-join",new Dictionary<string,object>()));Pass("normal_terminal_join_is_not_refusal");
   // Exercise the actual controller StepGate hook, not just the helper.
   var outer=new StepGate1725(RolePins1725.ShellSha);outer.Completed=39;outer.Expect(39);outer.Current.ReservationSequence=10;
   outer.Accept(new JournalRecord1725{Sequence=11,Kind="ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",Payload=Data(0)});
   outer.Expect(40);outer.Current.ReservationSequence=12;
   Reject(()=>outer.Accept(new JournalRecord1725{Sequence=13,Kind="ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",Payload=Json1725.Parse("{\"actual_started_handle\":1}")}));
   Assert(outer.Failed&&outer.Completed==40);Pass("actual_controller_gate_rejects_normal_started_payload_and_latches");
   Console.WriteLine("P2_PURE_PASSED="+passed);return 0;
  }
 }
}
#endif
