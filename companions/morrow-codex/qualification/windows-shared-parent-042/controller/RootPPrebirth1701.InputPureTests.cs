// New1701 pure state/protocol tests. Original sixteen-test file is unchanged.
// No native calls. The real ledger/validator/retirement decisions are exercised.
using System;
namespace C28.RootP1682 {
 public static class InputPureTests1701 {
  static void Assert(bool x){if(!x)throw new InvalidOperationException("INPUT_PURE_ASSERTION");}
  static void Reject(Action a){bool rejected=false;try{a();}catch(InvalidOperationException){rejected=true;}Assert(rejected);}
  static InputFrame Submit(InputLedger1701 l,int index){
   byte[] wire=InputProtocol1701.Wire(index);l.Queue(wire,InputRules1701.Hash(wire),index+1,index+1);return l.BeginWrite(index+2,index+2);
  }
  static InputLedger1701 DeliveredAll(){
   InputLedger1701 l=new InputLedger1701();
   for(int i=0;i<InputProtocol1701.Count;i++){InputFrame f=Submit(l,i);f.RawWritten=(uint)f.Length;Assert(l.CompleteWrite(f,true,0));}
   return l;
  }
  static Completion Native(){return new Completion{Identity=true,TokenWindow=true,ResumedOnce=true,ProcessSignal=true,PrimarySignal=true,
   OutEof=true,ErrEof=true,OutReaderSignal=true,ErrReaderSignal=true,ProcessCode=0,PrimaryCode=0,OutReaderCode=0,ErrReaderCode=0};}
  public static void exact_current047_shape_is_47_steps_92_individual_crlf_lines_3523_bytes(){
   int bytes=0;for(int i=0;i<InputProtocol1701.Count;i++){byte[] w=InputProtocol1701.Wire(i);Assert(w.Length<=512&&w[w.Length-2]==13&&w[w.Length-1]==10);bytes+=w.Length;}
   Assert(InputProtocol1701.Count==92&&bytes==3523&&InputProtocol1701.Step(0)==1&&InputProtocol1701.Step(91)==47);
   Assert(InputProtocol1701.Kind(0)=="confirmation"&&InputProtocol1701.Text(91)=="exit");
  }
  public static void line_limit_includes_crlf_and_no_blank_multiline_nonascii_or_secret_freeform(){
   Assert(InputRules1701.Line(new string('A',510)).Length==512);
   Reject(delegate{InputRules1701.Line(new string('A',511));});Reject(delegate{InputRules1701.Line("");});
   Reject(delegate{InputRules1701.Line("inventory\nsetup");});Reject(delegate{InputRules1701.Line("inventory\r");});
   Reject(delegate{InputRules1701.Line("\0");});Reject(delegate{InputRules1701.Line("\u4e2d");});
   InputLedger1701 l=new InputLedger1701();Reject(delegate{l.Queue(InputRules1701.Line("arbitrary-password-text"),"not-used",0,0);});Assert(l.Reserved==0);
  }
  public static void begin_is_confirmation_only_and_exit_cannot_be_submitted_early(){
   InputLedger1701 l=new InputLedger1701();
   Reject(delegate{l.Queue(InputRules1701.Line("begin"),"x",0,0);});
   Reject(delegate{l.Queue(InputRules1701.Line("exit"),"x",0,0);});
   Assert(l.Reserved==0&&l.RequestedBytes==0);
  }
  public static void second_frame_is_never_queued_while_first_queued_or_write_inflight(){
   InputLedger1701 l=new InputLedger1701();byte[] w=InputProtocol1701.Wire(0);
   l.Queue(w,InputRules1701.Hash(w),1,0);
   Reject(delegate{l.Queue(InputProtocol1701.Wire(1),"x",2,0);});
   InputFrame f=l.BeginWrite(2,0);Assert(f!=null&&l.Attempts==1);
   Reject(delegate{l.Queue(InputProtocol1701.Wire(1),"x",3,0);});
   Assert(l.Reserved==1&&f.Returned==null);
  }
  public static void false_partial_write_retains_original_bytes_raw_count_and_cannot_append_retry(){
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);byte[] saved=(byte[])f.Wire.Clone();
   f.RawWritten=3;Assert(!l.CompleteWrite(f,false,109));
   Assert(f.RawWritten==3&&f.Returned==false&&f.Error==109&&l.DeliveredBytes==0&&l.ReportedSuccessBytes==0&&Rules.Equal(saved,f.Wire));
   Assert(l.BeginWrite(8,8)==null);Reject(delegate{l.Queue(InputProtocol1701.Wire(1),"x",8,8);});Reject(delegate{l.Seal();});
  }
  public static void true_short_write_counts_only_reported_bytes_but_never_delivers_line(){
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);f.RawWritten=2;
   Assert(!l.CompleteWrite(f,true,0)&&l.ReportedSuccessBytes==2&&l.Delivered==0&&l.DeliveredBytes==0);
   Assert(l.Phase==InputPhase.Poisoned&&l.Returns==1&&l.Attempts==1);
  }
  public static void exception_inflight_or_root_withhold_preserves_buffer_and_no_return_is_not_zero_success(){
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);f.RawWritten=4;l.Poison();
   Assert(f.State==InputFrameState.InFlight&&f.Returned==null&&f.RawWritten==4&&l.Returns==0);
   Assert(l.BeginWrite(9,9)==null&&!l.Ready());Reject(delegate{l.Seal();});
  }
  public static void full_return_after_poison_records_actual_delivery_without_resurrecting_clean_state(){
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);l.Poison();f.RawWritten=(uint)f.Length;
   Assert(!l.CompleteWrite(f,true,0)&&l.Delivered==1&&l.DeliveredBytes==f.Length&&l.Phase==InputPhase.Poisoned&&!l.Ready());
  }
  public static void full_writes_do_not_by_themselves_prove_thread_end_or_product_execution(){
   InputLedger1701 l=DeliveredAll();Assert(l.Delivered==92&&l.DeliveredBytes==3523&&!l.Ready());
   l.Seal();Assert(!l.Ready());Assert(l.EndIfSealed()&&l.Ready());
   Assert(!InputProofRules1701.NativeReady(Native(),l.Ready(),false,0));
   Assert(!InputProofRules1701.NativeReady(Native(),l.Ready(),true,null));
   Assert(!InputProofRules1701.NativeReady(Native(),l.Ready(),true,7));
   Assert(InputProofRules1701.NativeReady(Native(),l.Ready(),true,0)); // local retirement only
  }
  public static void seal_requires_all_actual_frames_and_exact_final_exit_not_an_eof_shortcut(){
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);f.RawWritten=(uint)f.Length;l.CompleteWrite(f,true,0);
   Reject(delegate{l.Seal();});Assert(!l.EndIfSealed()&&!l.Ready());
   Assert(!InputRules1701.Exit(InputRules1701.Line("exit ")));
   Assert(InputRules1701.Exit(InputRules1701.Line("exit")));
  }
  public static void no_post_exit_frame_replay_or_second_native_write(){
   InputLedger1701 l=DeliveredAll();
   Reject(delegate{l.Queue(InputProtocol1701.Wire(0),"x",100,100);});
   Assert(l.BeginWrite(101,101)==null&&l.Attempts==92&&l.Returns==92);
   l.Seal();l.EndIfSealed();Reject(delegate{l.Queue(InputProtocol1701.Wire(0),"x",102,102);});
  }
  public static void command_review_after_real_write_return_does_not_fabricate_journal_stage_success(){
   InputLedger1701 l=new InputLedger1701();Assert(!l.ReviewAllowed(0,0)&&l.ReviewAllowed(1,0));
   InputFrame f=Submit(l,0);f.RawWritten=(uint)f.Length;l.CompleteWrite(f,true,0);
   Assert(l.ReviewAllowed(f.WriteStartOut,f.WriteStartErr)); // no new stdout required for journal-only stages
   Assert(l.ReviewAllowed(f.WriteStartOut+1,f.WriteStartErr)); // still NOT a business PASS
  }
  public static void wrong_context_stale_or_reused_ticket_cannot_be_reconstructed_from_data(){
   object a=new object(),b=new object();ReviewTicket1701 t=new ReviewTicket1701(a,1,10,0);
   ReviewTicketRules1701.Validate(t,t,a,1,true);
   Reject(delegate{ReviewTicketRules1701.Validate(t,t,b,1,true);});
   Reject(delegate{ReviewTicketRules1701.Validate(t,new ReviewTicket1701(a,1,10,0),a,1,true);});
   Reject(delegate{ReviewTicketRules1701.Validate(t,t,a,2,true);});
   t.Used=true;Reject(delegate{ReviewTicketRules1701.Validate(t,t,a,1,true);});
  }
  public static void confirmation_requires_complete_exact_actual_stdout_line_not_generic_prompt(){
   string expected=InputProtocol1701.Text(0);
   byte[] good=System.Text.Encoding.ASCII.GetBytes("Type exactly: "+expected+"\r\n");
   Assert(InputProtocol1701.Prompt(good,0,good.Length,0));
   Assert(!InputProtocol1701.Prompt(good,0,good.Length-1,0));
   byte[] wrong=System.Text.Encoding.ASCII.GetBytes("Type exactly: "+expected+" \n");Assert(!InputProtocol1701.Prompt(wrong,0,wrong.Length,0));
   byte[] embedded=System.Text.Encoding.ASCII.GetBytes("xType exactly: "+expected+"\n");Assert(!InputProtocol1701.Prompt(embedded,0,embedded.Length,0));
   Assert(!InputProtocol1701.Prompt(good,good.Length,good.Length,0));
   Assert(!InputProtocol1701.Prompt(good,0,good.Length,92));
  }
  public static void actual_c_retained_failure_marker_blocks_without_an_exception_body_or_fake_success(){
   byte[] failure=System.Text.Encoding.ASCII.GetBytes("x; original same-process state retained. No automatic retry.\n");
   Assert(InputProtocol1701.Failure(failure,0,failure.Length));
   Assert(!InputProtocol1701.Failure(failure,0,failure.Length-2));
   byte[] normal=System.Text.Encoding.ASCII.GetBytes("ordinary evidence\n");Assert(!InputProtocol1701.Failure(normal,0,normal.Length));
  }
  public static void native_input_completion_cannot_substitute_for_original_output_eof_primary_or_p(){
   Completion f=Native();f.ErrEof=false;Assert(!InputProofRules1701.NativeReady(f,true,true,0));
   f=Native();f.PrimarySignal=false;Assert(!InputProofRules1701.NativeReady(f,true,true,0));
   f=Native();f.ProcessCode=7;Assert(!InputProofRules1701.NativeReady(f,true,true,0));
   Assert(!InputProofRules1701.NativeReady(new Completion{ProcessCode=0},true,true,0));
   Assert(!InputProofRules1701.NativeReady(Native(),false,true,0));
  }
  public static void expiry_or_poison_does_not_clear_original_pipe_or_make_missing_native_dword_clean(){
   Slot pipe=new Slot("ORIGINAL_INPUT_WRITE");pipe.Begin();pipe.Capture(new IntPtr(81),true,0);
   InputLedger1701 l=new InputLedger1701();InputFrame f=Submit(l,0);l.Poison();
   Assert(!Rules.Budget(10,10)&&pipe.Raw==new IntPtr(81)&&pipe.State==ResourceState.Open&&f.Returned==null&&!l.Ready());
   DwordQuery q=new DwordQuery{Raw=0,Returned=null,AfterSignal=false};Assert(q.ValidValue==null);
  }
 }
}
