// SOURCE1725 pure parser/step-gate tests only. These fixtures are NOT real
// journal/VM/business receipts and cannot be supplied to the operational driver.
using System;
using System.Collections.Generic;
using System.Text;
namespace C28.RootP1682 {
 public static class ControllerPureTests1725 {
  public static void cached_failure_diagnostic_keeps_first_guard_and_actual_or_unobserved_return_fields(){
   Io1725 empty=new Io1725();empty.At("PURE_STAGE",RootPPrebirth.Shell);empty.Fail("FIRST_GUARD");
   Dictionary<string,object> a=empty.DiagnosticData("gather"),native=(Dictionary<string,object>)a["last_native"];
   Assert((bool)a["unknown"]&&(int)a["target_role_index"]==4&&native["returned"]==null&&native["error"]==null&&native["value_returned"]==null);
   Io1725 cached=new Io1725();cached.At("PURE_ORIGINAL_STAGE",RootPPrebirth.Shell);cached.NativeBegin("FILE_BASIC");cached.NativeResult(false,5);cached.Fail("ORIGINAL_GUARD");
   cached.At("PURE_LATER_STAGE",null);cached.NativeBegin("CLOSE_HANDLE");cached.NativeResult(true,0);cached.Fail("LATER_GUARD");
   Dictionary<string,object> b=cached.DiagnosticData("gather");native=(Dictionary<string,object>)b["last_native"];
   Assert((string)b["first_guard"]=="ORIGINAL_GUARD"&&(string)b["source_stage"]=="PURE_ORIGINAL_STAGE"&&(int)b["target_role_index"]==4);
   Assert((string)native["api"]=="FILE_BASIC"&&(bool)native["returned"]==false&&(int)native["error"]==5);
   // Synthetic cache inputs test real data rules only; no Win32 call/close witness.
  }
  public static void cached_exception_diagnostic_is_bounded_sticky_and_never_exports_body(){
   Exception error=new InvalidOperationException("SECRET_PATH_OR_CLAIM");
   for(int i=0;i<8;i++)error=new InvalidOperationException("SECRET_PATH_OR_CLAIM",error);
   Io1725 io=new Io1725();io.At("PURE_EXCEPTION_STAGE",RootPPrebirth.Shell);io.CaptureException(error);io.Fail("PURE_GUARD");
   io.CaptureException(new ArgumentException("LATER_SECRET"));Dictionary<string,object> data=io.DiagnosticData("PRIVATE_PATH_NOT_A_MODE");
   object[] chain=(object[])data["exceptions"];string text=Json1725.Encode(data);
   Assert(chain.Length==4&&(bool)data["exception_chain_truncated"]&&(bool)data["exception_captured"]);
   Assert((string)((Dictionary<string,object>)chain[0])["type"]=="InvalidOperationException");
   Assert(!text.Contains("SECRET")&&!text.Contains("PRIVATE_PATH_NOT_A_MODE")&&text.Contains("NOT_EXPORTED"));
   Assert((string)data["mode"]=="REDACTED"&&!(bool)data["resource_release_or_native_exit_proven"]);
  }
  static byte[] Privileges(params uint[] words){
   byte[] bytes=new byte[4+4*words.Length];Buffer.BlockCopy(BitConverter.GetBytes((uint)(words.Length/3)),0,bytes,0,4);
   for(int i=0;i<words.Length;i++)Buffer.BlockCopy(BitConverter.GetBytes(words[i]),0,bytes,4+4*i,4);return bytes;
  }
  public static void primary_privilege_default_is_unobserved_but_disabled_present_is_eligible(){
   Assert(!new PrivilegePresence1725().Observed&&!new PrivilegePresence1725().Eligible);
   byte[] bytes=Privileges(3,0,0,5,0,0);
   PrivilegePresence1725 p=PrivilegeRules1725.Parse(bytes,bytes.Length,new Luid1725{Low=3},new Luid1725{Low=5});
   Assert(p.Observed&&p.Eligible&&p.AssignAttributes==0&&p.QuotaAttributes==0);
  }
  public static void primary_privilege_presence_uses_full_actual_luid_not_enabled_or_low_word(){
   byte[] bytes=Privileges(3,1,2,5,0,2);
   PrivilegePresence1725 p=PrivilegeRules1725.Parse(bytes,bytes.Length,new Luid1725{Low=3},new Luid1725{Low=5});
   Assert(p.Observed&&!p.AssignPresent&&p.AssignAttributes==null&&p.QuotaPresent&&!p.Eligible);
   byte[] empty=Privileges();PrivilegePresence1725 q=PrivilegeRules1725.Parse(empty,empty.Length,new Luid1725{Low=3},new Luid1725{Low=5});
   Assert(q.Observed&&!q.AssignPresent&&!q.QuotaPresent&&!q.Eligible);
  }
  public static void primary_privilege_bad_layout_duplicate_or_colliding_luids_never_qualify(){
   Luid1725 a=new Luid1725{Low=3},q=new Luid1725{Low=5};byte[] bytes=Privileges(3,0,0,3,0,2);
   Reject(delegate{PrivilegeRules1725.Parse(bytes,bytes.Length,a,q);});
   byte[] good=Privileges(3,0,0,5,0,0);
   Reject(delegate{PrivilegeRules1725.Parse(good,good.Length-1,a,q);});
   Reject(delegate{PrivilegeRules1725.Parse(good,good.Length,a,a);});
   byte[] excessive=new byte[4];Buffer.BlockCopy(BitConverter.GetBytes((uint)4097),0,excessive,0,4);
   Reject(delegate{PrivilegeRules1725.Parse(excessive,excessive.Length,a,q);});
  }
  static void Assert(bool b){if(!b)throw new InvalidOperationException("CONTROLLER_PURE_ASSERTION");}
  static void Reject(Action f){bool bad=false;try{f();}catch(InvalidOperationException){bad=true;}catch(DecoderFallbackException){bad=true;}Assert(bad);}
  static Dictionary<string,object> D(params object[] p){return Driver1725.D(p);}
  static JournalRecord1725 R(int sequence,string kind,object payload){return new JournalRecord1725{Sequence=sequence,Kind=kind,Payload=payload};}
  static Dictionary<string,object> Guest(){
   return D("schema","morrow-dedicated-hyperv-guest-identity-v1","manufacturer","Microsoft Corporation","model","Virtual Machine",
    "firmware_uuid",Plan1725.Firmware,"machine_name","SYNTHETIC_PARSER_ONLY","hypervisor_present",true,"administrator",true,"process_64_bit",true,"os_64_bit",true,"observed_utc","synthetic-parser-test");
  }
  static StepGate1725 Begun(){
   StepGate1725 g=new StepGate1725(RolePins1725.ShellSha);g.Expect(0);
   g.Accept(R(0,"acceptance",D("VM_run","PENDING_ACTUAL_STEPS","release_eligible",false)));
   g.Accept(R(1,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","guest-identity","review",D("expected_uuid",Plan1725.Firmware,"shell",RootPPrebirth.Shell,"shell_sha256",RolePins1725.ShellSha))));
   g.Accept(R(2,"guest-identity-actual-exit",D("success",true,"exit_code",new JsonNumber1725("0"))));
   g.Accept(R(3,"guest-identity-validated",Guest()));return g;
  }
  static byte[] Record(int sequence,string kind,object payload){return new UTF8Encoding(false,true).GetBytes(Json1725.Encode(D("sequence",sequence,"kind",kind,"payload",payload))+"\n");}
  static List<JournalRecord1725> Feed(JournalPrefix1725 p,byte[] full){int old=p.Length;p.CompareAndStore(0,full,full.Length,old);return p.Commit(full.Length);}
  public static void default_or_arbitrary_invocation_denies_without_native(){
   string mode,tag,pin;Assert(!Invocation1725.Parse(new string[0],out mode,out tag,out pin));
   Assert(!Invocation1725.Parse(new string[]{"--run-1725",new string('1',32)},out mode,out tag,out pin));
   Assert(!Invocation1725.Parse(new string[]{"--run-1725",@"C:\other",new string('a',64)},out mode,out tag,out pin));
   Assert(Invocation1725.Parse(new string[]{"--gather-pins-1725",new string('1',32)},out mode,out tag,out pin)&&mode=="gather");
  }
  public static void fixed_plan_preserves_begin_exit_and_47_to_92_shape(){
   Step1725[] p=Plan1725.NewSteps();Assert(p.Length==47&&p[0].Command=="begin"&&p[0].FirstLine==p[0].LastLine);
   Assert(p[46].Command=="exit"&&p[46].FirstLine==91&&p[46].LastLine==91);
   for(int i=1;i<46;i++)Assert(p[i].LastLine==p[i].FirstLine+1);
  }
  public static void half_line_is_not_parsed_until_real_newline_and_valid_utf8(){
   JournalPrefix1725 p=new JournalPrefix1725();byte[] full=Record(0,"acceptance",D("VM_run","PENDING_ACTUAL_STEPS","release_eligible",false));
   byte[] half=new byte[full.Length-1];Buffer.BlockCopy(full,0,half,0,half.Length);
   Assert(Feed(p,half).Count==0&&p.Parsed==0&&p.Records==0);
   Assert(Feed(p,full).Count==1&&p.Parsed==p.Length&&p.Rows[0].Sequence==0);
  }
  public static void original_prefix_rewrite_poison_is_sticky_and_never_reparsed(){
   JournalPrefix1725 p=new JournalPrefix1725();byte[] b=Record(0,"status",D());Feed(p,b);byte[] wrong=(byte[])b.Clone();wrong[0]=(byte)'[';
   Reject(delegate{p.CompareAndStore(0,wrong,wrong.Length,p.Length);});Assert(p.Failed);Reject(delegate{p.Commit(p.Length);});
  }
  public static void shrink_and_out_of_order_or_duplicate_sequence_fail(){
   JournalPrefix1725 p=new JournalPrefix1725();Feed(p,Record(0,"status",D()));Reject(delegate{p.Commit(p.Length-1);});
   JournalPrefix1725 q=new JournalPrefix1725();Reject(delegate{Feed(q,Record(1,"status",D()));});Assert(q.Failed);
   JournalPrefix1725 r=new JournalPrefix1725();byte[] one=Record(0,"status",D()),two=new byte[one.Length*2];Buffer.BlockCopy(one,0,two,0,one.Length);Buffer.BlockCopy(one,0,two,one.Length,one.Length);
   Reject(delegate{Feed(r,two);});
  }
  public static void malformed_utf8_duplicate_case_keys_and_noninteger_sequence_reject(){
   JournalPrefix1725 p=new JournalPrefix1725();Reject(delegate{Feed(p,new byte[]{255,10});});
   Reject(delegate{Json1725.Parse("{\"a\":1,\"A\":2}");});
   JournalPrefix1725 q=new JournalPrefix1725();byte[] text=Encoding.ASCII.GetBytes("{\"sequence\":\"0\",\"kind\":\"status\",\"payload\":{}}\n");
   Reject(delegate{Feed(q,text);});
  }
  public static void reservation_unknown_is_pending_not_failure_and_banner_text_is_not_a_gate(){
   StepGate1725 g=Begun();g.Expect(1);
   g.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory","review",D())));
   g.Accept(R(5,"lifecycle-after-attempt","normal banner mentions Unknown/noReplay"));
   Assert(!g.Failed&&!g.Current.Returned&&g.Current.ReservationSequence==4&&g.Completed==1);
  }
  public static void missing_return_keeps_pending_and_prevents_next_step(){
   StepGate1725 g=Begun();g.Expect(1);g.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));
   Reject(delegate{g.Expect(2);});Assert(g.Failed&&!g.Steps[1].Returned);
  }
  public static void wrong_or_duplicate_reservation_rejects_before_success(){
   StepGate1725 a=Begun();a.Expect(1);Reject(delegate{a.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","setup")));});
   StepGate1725 b=Begun();b.Expect(1);b.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));
   Reject(delegate{b.Accept(R(5,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));});
  }
  public static void wrong_command_or_return_without_reservation_is_not_success(){
   StepGate1725 a=Begun();a.Expect(1);Reject(delegate{a.Accept(R(4,"STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE","inventory"));});
   StepGate1725 b=Begun();b.Expect(1);b.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));
   Reject(delegate{b.Accept(R(5,"STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE","setup"));});
  }
  public static void ordinary_return_closes_exact_pending_once_not_all_gates(){
   StepGate1725 g=Begun();g.Expect(1);g.Accept(R(4,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));
   g.Accept(R(5,"STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE","inventory"));Assert(g.Current.Returned&&g.Completed==2&&g.Current.ReturnSequence==5);
   Reject(delegate{g.Accept(R(6,"STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE","inventory"));});
  }
  public static void exact_failure_kinds_stop_future_progress_without_text_guessing(){
   foreach(string kind in new string[]{"UNKNOWN_OR_REJECTED_NO_REPLAY","SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY","INTERACTIVE_OR_EVIDENCE_LOST_STATE_RETAINED"}){
    StepGate1725 g=Begun();g.Expect(1);Reject(delegate{g.Accept(R(4,kind,D()));});
    Reject(delegate{g.Accept(R(5,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","inventory")));});Assert(g.Failed);
   }
  }
  public static void begin_requires_probe0_and_current_uuid_source_fields(){
   StepGate1725 g=new StepGate1725(RolePins1725.ShellSha);g.Expect(0);
   g.Accept(R(1,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","guest-identity","review",D("expected_uuid",Plan1725.Firmware,"shell",RootPPrebirth.Shell,"shell_sha256",RolePins1725.ShellSha))));
   Reject(delegate{g.Accept(R(2,"guest-identity-validated",Guest()));});
   Dictionary<string,object> bad=Guest();bad["firmware_uuid"]="00000000-0000-0000-0000-000000000000";
   StepGate1725 b=new StepGate1725(RolePins1725.ShellSha);b.Expect(0);
   b.Accept(R(1,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","guest-identity","review",D("expected_uuid",Plan1725.Firmware,"shell",RootPPrebirth.Shell,"shell_sha256",RolePins1725.ShellSha))));
   b.Accept(R(2,"guest-identity-actual-exit",D("success",true,"exit_code",new JsonNumber1725("0"))));
   Reject(delegate{b.Accept(R(3,"guest-identity-validated",bad));});
  }
  public static void sealed_return_is_linked_to_one_exact_pending_reservation(){
   StepGate1725 g=new StepGate1725(RolePins1725.ShellSha);g.Completed=7;g.Expect(7); // pure starting state only
   g.Accept(R(20,"ATTEMPT_RESERVED_OUTCOME_UNKNOWN",D("step","sealed-basic")));
   g.Accept(R(21,"ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",D("witness","basic","authority_created",false)));
   Assert(g.Completed==8&&g.Current.Returned);
  }
  public static void exit_needs_actual_cleanup_and_unblocked_sealed_payload_not_a_generic_ok(){
   StepGate1725 g=new StepGate1725(RolePins1725.ShellSha);g.Completed=46;g.Expect(46);
   Reject(delegate{g.Accept(R(200,"FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE",D("sealed_native_observed",true,"prior_business_or_step_blocked",false,"normal_cleanup_report",D("retired",true))));});
   StepGate1725 b=new StepGate1725(RolePins1725.ShellSha);b.Completed=46;b.CleanupObserved=true;b.Expect(46);
   Reject(delegate{b.Accept(R(201,"FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE",D("sealed_native_observed",true,"prior_business_or_step_blocked",true,"normal_cleanup_report",D("retired",true))));});
  }
  public static void full_known_role_and_shell_pins_are_data_not_mutation_or_vm_authority(){
   Assert(RolePins1725.Sha.Length==4&&RolePins1725.Bytes[0]==30112768&&RolePins1725.ShellBytes==450560);
   Assert(RolePins1725.ShellSha=="3247bcfd60f6dd25f34cb74b5889ab10ef1b3ec72b4d4b3d95b5b25b534560b8");
   Reject(delegate{Json1725.Parse("{\"approval\":true} trailing");});
  }
 }
}
