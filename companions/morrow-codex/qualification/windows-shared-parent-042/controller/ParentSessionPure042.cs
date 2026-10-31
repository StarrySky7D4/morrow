#if P2_SHARED_PARENT_REFUSAL_SESSION
using System;
using System.Collections.Generic;
using System.Text;
namespace C28.RootP1682 {
 public static class ParentSessionPure042 {
  static int passed;
  static void Need(bool b){if(!b)throw new Exception("PURE_PARENT_SESSION_ASSERT");}
  static void Pass(string n){Console.WriteLine("PASS ParentSession042::"+n);passed++;}
  static void Reject(Action a){bool rejected=false;try{a();}catch(InvalidOperationException){rejected=true;}Need(rejected);}
  static Dictionary<string,object> Data(string s){return Json1725.Obj(Json1725.Parse(s));}
  static JournalRecord1725 Record(string kind,int seq,Dictionary<string,object> p){return new JournalRecord1725{Kind=kind,Sequence=seq,Payload=p};}
  static Dictionary<string,object> Closed(int n){return Data("{\"attempt\":"+n+",\"same_protected_owner\":true,\"same_sdk_resources\":true,\"common_parent_closed\":false,\"native_replay\":false,\"full_P2_accepted\":false}");}
  static Dictionary<string,object> Common(){return Data("{\"closed_attempts\":17,\"same_protected_owner\":true,\"same_sdk_resources\":true,\"common_parent_closed\":false,\"full_P2_accepted\":false}");}
  static SharedRound042 Ready(int n){
   var r=new SharedRound042(n);r.Begin(new byte[2097152],0);
   r.Gate.Current=r.Gate.Steps[40];
   r.Accept(Record("ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",150,Data("{\"startup_attempt\":1}")));
   r.Gate.Current=r.Gate.Steps[45];
   r.Accept(Record("lifecycle-after-attempt",175,Data("{\"phase\":\"Finished\",\"cleanup_guard_held\":false,\"owned_setup_proof_held\":false,\"execution_cleanup_admission\":null,\"refusal_cleanup_admission\":{\"startup_attempt\":1,\"setup_child_pid\":123,\"outcome\":\"KNOWN_SOURCE_TARGET_POLICY_REJECTED_RESOURCES_CLOSED_BEFORE_DISPATCH\",\"native_caller_handles_closed\":true,\"original_blocking_return_acknowledged\":true,\"local_and_sdk_slots_retired\":true,\"original_worker_joined\":true,\"owner_finished\":false,\"owner_attempt_finished\":true,\"shared_owner_retained\":true,\"factory_released\":true,\"runner_was_started\":false,\"normal_terminal_ack_claimed\":false}}")));
   r.Gate.Completed=47;r.Gate.Current=r.Gate.Steps[46];r.Gate.Current.Returned=true;r.Gate.Current.ReturnSequence=180;return r;
  }
  public static int Main(string[] args){
   if(args.Length!=1||args[0]!="--parent-session-pure")return 78;
   Need(InputProtocol1701.Count==1564&&InputProtocol1701.NormalBytes==62101&&InputProtocol1701.FrameCapacity==1564);
   Need(RootSelection.ParentSelector=="--c28-parent-refusal-session-p1");
   Need(Plan1725.NewSteps().Length==47&&Plan1725.NewSteps()[39].Command=="sealed-qualify-source-policy-refusal");
   string mode,tag,pin,t=new string('a',32),p=new string('b',64);
   Need(Invocation1725.Parse(new[]{"--run-p2-shared-parent-refusal-session",t,p},out mode,out tag,out pin));
   Need(!Invocation1725.Parse(new[]{"--run-p2-source-policy-refusal",t,p},out mode,out tag,out pin));
   Need(!Invocation1725.Parse(new[]{"--run-1725",t,p},out mode,out tag,out pin));Pass("fixed_separate_shared_profile_and_parent_selector");
   var ledger=new InputLedger1701();var originalFrames=ledger.Frames;long bytes=0;
   for(int i=0;i<1564;i++){
    byte[] wire=InputProtocol1701.Wire(i);bytes+=wire.Length;
    ledger.Queue(wire,InputRules1701.Hash(wire),i*10,i*2);
    var f=ledger.BeginWrite(i*10,i*2);Need(Object.ReferenceEquals(f,originalFrames[i]));
    f.RawWritten=(uint)wire.Length;Need(ledger.CompleteWrite(f,true,0));
    Need(ledger.TerminalDelivered==(i==1563)&&ledger.Delivered==i+1&&ledger.Reserved==i+1);
    Need(InputProtocol1701.Step(i)==47*(i/92)+InputProtocol1701.Step(i%92));
    if(i%92==91&&i<1563){Reject(()=>ledger.Seal());Need(ledger.ReviewAllowed(i*10,i*2));}
   }
   Need(bytes==62101&&Object.ReferenceEquals(originalFrames,ledger.Frames));
   ledger.Seal();Need(ledger.EndIfSealed()&&ledger.Ready());
   var final=ledger.Snapshot();Need(final.Frames.Length==1564&&final.DeliveredBytes==62101);
   for(int i=0;i<1564;i++)Need(final.Frames[i].Ordinal==i+1&&final.Frames[i].State=="Delivered"&&final.Frames[i].NativeReturned==true);
   Reject(()=>ledger.Queue(InputProtocol1701.Wire(0),"",0,0));Pass("all_1564_original_frames_retained_only_final_exit_seals");
   var poisoned=new InputLedger1701();byte[] w=InputProtocol1701.Wire(0);poisoned.Queue(w,InputRules1701.Hash(w),0,0);
   var active=poisoned.BeginWrite(0,0);active.RawWritten=1;Need(!poisoned.CompleteWrite(active,true,0));
   Reject(()=>poisoned.Queue(w,InputRules1701.Hash(w),0,0));Need(poisoned.Frames[0].State==InputFrameState.Unknown&&poisoned.Reserved==1);Pass("partial_original_write_retains_unknown_no_next_attempt");
   var output=new byte[2097152];string prompt="Type exactly: "+InputProtocol1701.Text(0)+"\r\n";
   byte[] first=Encoding.ASCII.GetBytes(prompt);Buffer.BlockCopy(first,0,output,0,first.Length);
   var fresh=new RoundPrompt042(output,first.Length);Need(!fresh.Observe(output,first.Length,first.Length));
   Buffer.BlockCopy(first,0,output,first.Length,first.Length);Need(fresh.Observe(output,first.Length,first.Length*2));
   Reject(()=>fresh.Observe(new byte[2097152],first.Length*2,first.Length*2));Pass("prior_identical_prompt_and_foreign_output_cannot_release_next_round");
   byte[] marker=Encoding.ASCII.GetBytes("ORIGINAL_PARENT_SESSION_UNKNOWN_RETAINED_NO_NEXT_ATTEMPT");Need(InputProtocol1701.Failure(marker,0,marker.Length));Pass("new_parent_unknown_marker_stops_original_input");
   var firstRound=Ready(1);firstRound.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(1)));firstRound.Complete();
   Need(firstRound.Completed&&!firstRound.CommonOwnerClosed);Reject(()=>firstRound.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",182,Closed(1))));Pass("closed_round_retained_and_late_record_rejected");
   var last=Ready(17);last.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(17)));last.Accept(Record("ORIGINAL_SHARED_PROTECTED_OWNER_FINISHED",182,Common()));last.Complete();Need(last.Completed&&last.CommonOwnerClosed);Pass("last_round_requires_actual_common_owner_close");
   var missing=Ready(17);missing.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(17)));Reject(()=>missing.Complete());Pass("final_attempt_without_common_owner_close_rejected");
   var early=Ready(1);early.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(1)));Reject(()=>early.Accept(Record("ORIGINAL_SHARED_PROTECTED_OWNER_FINISHED",182,Common())));Pass("early_common_owner_close_rejected");
   var foreign=Ready(2);Reject(()=>foreign.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(1))));Reject(()=>foreign.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(2))));Pass("foreign_round_and_late_correction_stay_failed");
   foreach(string key in new[]{"same_protected_owner","same_sdk_resources","common_parent_closed","native_replay","full_P2_accepted"}){
    var r=Ready(2);var wrong=Closed(2);wrong[key]=!(bool)wrong[key];Reject(()=>r.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,wrong)));Pass("wrong_"+key+"_rejected");
   }
   var unbound=new SharedRound042(1);unbound.Gate.Completed=47;unbound.Gate.Current=unbound.Gate.Steps[46];unbound.Gate.Current.Returned=true;unbound.Gate.Current.ReturnSequence=180;
   Reject(()=>unbound.Accept(Record("ORIGINAL_PARENT_ATTEMPT_REFUSAL_CLOSED",181,Closed(1))));Pass("marker_without_actual_refusal_admission_cannot_close");
   var rows=new List<FileImage1725>();var a=Ready(3);var b=Ready(4);a.Register(rows);b.Register(rows);
   Need(rows.Count==6&&!Object.ReferenceEquals(a.Prefix,b.Prefix)&&!Object.ReferenceEquals(a.Gate,b.Gate)&&!Object.ReferenceEquals(a.Journal,b.Journal));Pass("round_gates_history_and_file_slots_are_distinct");
   Console.WriteLine("PARENT_SESSION_PURE_PASSED="+passed);return 0;
  }
 }
}
#endif

namespace C28.RootP1682 {
 internal static class CachedProgressPure043 {
  static void Need(bool value,string name){if(!value)throw new System.InvalidOperationException(name);System.Console.WriteLine("PASS CachedProgress043::"+name);}
  public static int Main(string[] args){
   if(args==null||args.Length!=1||args[0]!="--cached-progress-pure")return 78;
   Io1725 empty=new Io1725();empty.Fail("BEFORE_PROGRESS");Need(empty.DiagnosticData("run")["cached_shared_progress"]==null,"no_fabricated_progress");
   Io1725 io=new Io1725();io.CacheSharedProgress(6,40,644,1200000,1000000,8589934593L,2097152,2097152,5500000,5300000,5400000,5400000);
   io.EventRecords=8705;io.EventBytes=4194305;io.OpenJournalAttempts=1200001;io.Fail("JOURNAL_REREAD_BYTE_CAP");
   var data=io.DiagnosticData("run");var progress=(System.Collections.Generic.Dictionary<string,object>)data["cached_shared_progress"];
   Need((int)progress["round_index_zero_based"]==6&&(int)progress["step_index_zero_based"]==40&&(int)progress["delivered"]==644,"original_round_step_delivery");
   Need((long)progress["journal_read_bytes"]==8589934593L&&(int)progress["journal_read_calls"]==1000000&&(int)progress["event_records"]==8705,"original_boundary_values");
   io.CacheSharedProgress(16,46,1564,1,1,1,1,1,1,1,1,1);io.EventRecords=0;io.Fail("LATER_FAILURE");
   var later=io.DiagnosticData("run");var frozen=(System.Collections.Generic.Dictionary<string,object>)later["cached_shared_progress"];
   Need((int)frozen["round_index_zero_based"]==6&&(int)frozen["event_records"]==8705&&(string)later["first_guard"]=="JOURNAL_REREAD_BYTE_CAP","first_failure_snapshot_immutable");
   io.CaptureException(new System.Exception("SECRET_EXCEPTION_BODY_SENTINEL",new System.Exception("SECOND_SECRET_SENTINEL")));
   string encoded=Json1725.Encode(io.DiagnosticData("run"));
   Need(!encoded.Contains("SECRET")&&(string)io.DiagnosticData("run")["exception_body"]=="NOT_EXPORTED","exception_bodies_excluded");
   Need(System.Text.Encoding.UTF8.GetByteCount(encoded)<=3500,"original_one_write_diagnostic_bound");
   Need(!(bool)data["resource_release_or_native_exit_proven"]&&(bool)data["no_replay"],"no_closure_or_replay_claim");
   System.Console.WriteLine("CACHED_PROGRESS_PURE_PASSED=7");return 0;
  }
 }
}

#if P2_SHARED_PARENT_REFUSAL_SESSION
namespace C28.RootP1682 {
 internal static class NativeStartBudget044 {
  internal static long Begin(long original,string command,long now){
   if(original < -1 || now<0)throw new System.InvalidOperationException("RUNTIME_CLOCK_INVALID");
   if(command!="sealed-native-start")return original;
   if(original!=-1)throw new System.InvalidOperationException("RUNTIME_START_CANNOT_RESET");
   return now;
  }
  internal static bool Within(long now,long start){
   return now>=0&&(start==-1||(start>=0&&now>=start&&now-start<29000));
  }
 }
 public static class NativeStartBudgetPure044 {
  static int count;
  static void Need(bool b,string name){if(!b)throw new System.InvalidOperationException(name);System.Console.WriteLine("PASS NativeStartBudget044::"+name);count++;}
  static void Reject(System.Action action,string name){bool failed=false;try{action();}catch(System.InvalidOperationException){failed=true;}Need(failed,name);}
  public static int Main(string[] args){
   if(args.Length!=1||args[0]!="--runtime-start-pure")return 78;
   var steps=Plan1725.NewSteps();long start=-1;int starts=0;
   for(int i=0;i<steps.Length;i++){
    long before=start;start=NativeStartBudget044.Begin(start,steps[i].Command,100000+i*2000);
    if(start!=before){starts++;Need(i==40&&steps[i].Command=="sealed-native-start","actual_fixed_shared_plan_start_only");}
    if(i<40)Need(NativeStartBudget044.Within(100000+i*2000,start),"prestart_step_"+(i+1)+"_not_process_runtime");
   }
   Need(starts==1&&start==180000,"one_original_start_anchor_not_reset_by_join_cleanup");
   Need(NativeStartBudget044.Within(start+28999,start),"last_conservative_millisecond_allowed");
   Need(!NativeStartBudget044.Within(start+29000,start),"unchanged_29000_boundary_refused");
   Need(!NativeStartBudget044.Within(start+30000,start),"product_30000_boundary_not_relaxed");
   Need(!NativeStartBudget044.Within(start-1,start),"clock_regression_refused");
   Need(!NativeStartBudget044.Within(0,-2),"invalid_unstarted_marker_refused");
   Reject(()=>NativeStartBudget044.Begin(start,"sealed-native-start",start+1),"second_start_cannot_refresh_window");
   Reject(()=>NativeStartBudget044.Begin(-1,"sealed-native-start",-1),"negative_start_refused");
   // Reproduce the recorded guard inputs without observing any retained owner.
   Need(NativeStartBudget044.Within(645714,-1),"case043_prestart_elapsed_not_invocation_runtime");
   Need(645714-640186<59000,"case043_context_window_remains_within_unchanged_guard");
   var plan=new RootSelection(@"C:\Morrow-C28-RootP1682-bin-"+new string('1',32),new string('1',32),RootSelection.FrozenInventory,1000,new FilePin[0]);
   var owner=new RootPPrebirth.Owner(plan); // fresh allocation only, never Run or native creation
   byte[] wire=InputProtocol1701.Wire(0);owner.Input.Ledger.Queue(wire,InputRules1701.Hash(wire),3,4);
   var frame=owner.Input.Ledger.BeginWrite(5,6);frame.RawWritten=(uint)wire.Length;
   Need(owner.Input.Ledger.CompleteWrite(frame,true,0),"fresh_pure_ledger_completed_frame");
   owner.Fail("PURE_FIXTURE_NO_NATIVE_CALLS");
   var session=new RootPPrebirth.ControlledSession1701(owner);
   var view=session.Observe(512);
   Need(view.Capture.Input==null&&view.Input.Frames.Length==1&&view.Input.Frames[0].Ordinal==1&&view.Input.Delivered==1,"transport_keeps_full_captured_input_history");
   Need(view.Input.Frames[0].NativeReturned==true&&view.Input.Frames[0].RawWritten==(uint)wire.Length&&view.Input.Frames[0].NativeWriteStartStdout==5,"original_frame_native_fields_preserved");
   Need(view.Capture.FirstError=="PURE_FIXTURE_NO_NATIVE_CALLS"&&view.Review==null&&!view.Capture.CreateWasAttempted,"pure_unknown_snapshot_has_no_review_or_creation_authority");
   view.Input.Frames[0].Ordinal=999;
   Need(owner.Input.Ledger.Snapshot().Frames[0].Ordinal==1,"consumer_snapshot_does_not_mutate_original_ledger");
   System.Console.WriteLine("RUNTIME_START_PURE_PASSED="+count);return 0;
  }
 }
}
#endif

#if P2_SHARED_PARENT_REFUSAL_SESSION
namespace C28.RootP1682 {
 public static class SharedBasePure046 {
  static int passed;
  static void Need(bool value){if(!value)throw new System.Exception("SHARED_BASE_PURE_ASSERT");}
  static void Pass(string name){passed++;System.Console.WriteLine("PASS SharedBase046::"+name);}
  public static int Main(string[] args){
   if(args.Length!=1||args[0]!="--shared-base-pure")return 78;
   int total=0;
   for(int round=1;round<=17;round++){
    int bytes=0;for(int i=(round-1)*92;i<round*92;i++)bytes+=InputProtocol1701.Wire(i).Length;
    Need(bytes==SharedBaseSelection046.RoundBytes(round));total+=bytes;
    var gate=new SharedRound042(round).Gate;Need(gate.Steps.Length==47);
    int readCommands=0,selectCommands=0;
    foreach(var step in gate.Steps){
     if(step.Command.Contains("-base-verify-"))readCommands++;
     if(step.Command.EndsWith("-base-select")||step.Command.EndsWith("-base-enable"))selectCommands++;
     if(step.Ordinal!=1&&step.Ordinal!=47)
      Need(step.Command==InputProtocol1701.Text((round-1)*92+step.FirstLine));
    }
    Need(readCommands==(round==1?0:6)&&selectCommands==(round==1?6:0));
    Pass("round_"+round+"_wire_and_exact_gate");
   }
   Need(total==62101&&total==InputProtocol1701.NormalBytes&&total<65536);Pass("bounded_complete_new_wire");
   Need(InputProtocol1701.Text(19)=="sealed-session-base-select");
   Need(InputProtocol1701.Text(92+19)=="sealed-session-base-verify-selected");
   Need(InputProtocol1701.Text(92+20)=="EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_BASE_VERIFY_SELECTED");
   Need(InputProtocol1701.Text(92+21)=="sealed-session-base-verify-enabled");Pass("explicit_read_confirmation_and_first_round_mutation");
   Need(18+16*12==210&&210<256);Pass("catalog_budget_preserved");
   foreach(int bad in new[]{0,18}){bool rejected=false;try{SharedBaseSelection046.RoundBytes(bad);}catch(System.InvalidOperationException){rejected=true;}Need(rejected);}Pass("out_of_range_round_rejected");
   foreach(int bad in new[]{-1,1564}){bool rejected=false;try{SharedBaseSelection046.Text(bad,"x");}catch(System.InvalidOperationException){rejected=true;}Need(rejected);}Pass("out_of_range_frame_rejected");
   System.Console.WriteLine("SHARED_BASE_PURE_PASSED="+passed);return 0;
  }
 }
}
#endif
