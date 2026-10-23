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
   Need(InputProtocol1701.Count==1564&&InputProtocol1701.NormalBytes==60469&&InputProtocol1701.FrameCapacity==1564);
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
   Need(bytes==60469&&Object.ReferenceEquals(originalFrames,ledger.Frames));
   ledger.Seal();Need(ledger.EndIfSealed()&&ledger.Ready());
   var final=ledger.Snapshot();Need(final.Frames.Length==1564&&final.DeliveredBytes==60469);
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
