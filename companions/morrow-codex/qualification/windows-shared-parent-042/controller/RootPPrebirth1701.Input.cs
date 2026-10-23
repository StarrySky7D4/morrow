// SOURCE1701: same-owner controlled stdin. No host stdin, callback from caller,
// automatic script, second creator, replay or extra P inherited handles.
using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Threading;
namespace C28.RootP1682 {
 internal enum InputPhase { Waiting,Queued,InFlight,Delivered,Sealing,Ended,Poisoned }
 internal enum InputFrameState { Unused,Queued,InFlight,Delivered,Unknown }
 internal sealed class InputFrame {
  internal readonly byte[] Wire=new byte[512];
  internal int Length,Ordinal;internal string Sha;internal bool ExitLine;
  internal long ReviewedOut,ReviewedErr,WriteStartOut,WriteStartErr;
  internal uint RawWritten;internal bool? Returned;internal int Error;
  internal InputFrameState State;
 }
 internal static class InputRules1701 {
  internal static byte[] Line(string text) {
   if(text==null||text.Length<1||text.Length>510)throw new InvalidOperationException("INPUT_LINE_1_TO_510_ASCII");
   byte[] wire=new byte[text.Length+2];
   for(int i=0;i<text.Length;i++){
    if(text[i]<32||text[i]>126)throw new InvalidOperationException("INPUT_LINE_PRINTABLE_ASCII_NO_EOL");
    wire[i]=(byte)text[i];
   }
   wire[wire.Length-2]=13;wire[wire.Length-1]=10;return wire; //512 includes CRLF
  }
  internal static bool Exit(byte[] wire) {
   return wire.Length==6&&wire[0]==101&&wire[1]==120&&wire[2]==105&&wire[3]==116&&wire[4]==13&&wire[5]==10;
  }
  internal static string Hash(byte[] bytes) {
   using(SHA256Managed sha=new SHA256Managed())return Rules.HexBytes(sha.ComputeHash(bytes));
  }
 }
 internal sealed class InputLedger1701 {
  internal readonly object Gate=new object();
  internal readonly InputFrame[] Frames=new InputFrame[InputProtocol1701.FrameCapacity];
  internal readonly byte[] OriginalWriteBuffer=new byte[512];
  internal InputPhase Phase=InputPhase.Waiting;
  internal int Reserved,Attempts,Returns,Delivered;internal long RequestedBytes,ReportedSuccessBytes,DeliveredBytes;
  internal InputFrame Active;internal bool TerminalDelivered;
  internal InputLedger1701(){for(int i=0;i<Frames.Length;i++)Frames[i]=new InputFrame();}
  internal void Queue(byte[] wire,string hash,long reviewedOut,long reviewedErr) {
   lock(Gate){
    if((Phase!=InputPhase.Waiting&&Phase!=InputPhase.Delivered)||TerminalDelivered||Reserved>=InputProtocol1701.FrameCapacity
     ||wire==null||wire.Length<3||wire.Length>512||RequestedBytes+wire.Length>65536||!InputProtocol1701.Matches(Reserved,wire))
     throw new InvalidOperationException("INPUT_NO_QUEUE_OR_REPLAY");
    InputFrame f=Frames[Reserved];Buffer.BlockCopy(wire,0,f.Wire,0,wire.Length);
    f.Length=wire.Length;f.Ordinal=Reserved+1;f.Sha=hash;f.ExitLine=InputProtocol1701.Terminal(Reserved,wire);
    f.ReviewedOut=reviewedOut;f.ReviewedErr=reviewedErr;f.State=InputFrameState.Queued;
    Active=f;Reserved++;RequestedBytes+=wire.Length;Phase=InputPhase.Queued;
   }
  }
  internal InputFrame BeginWrite(long output,long error) {
   lock(Gate){
    if(Phase!=InputPhase.Queued)return null;
    if(Attempts>=InputProtocol1701.FrameCapacity)throw new InvalidOperationException("INPUT_WRITE_ATTEMPT_CAP");
    InputFrame f=Active;Buffer.BlockCopy(f.Wire,0,OriginalWriteBuffer,0,f.Length);
    f.WriteStartOut=output;f.WriteStartErr=error;f.RawWritten=0;f.Returned=null;
    f.State=InputFrameState.InFlight;Phase=InputPhase.InFlight;Attempts++;return f;
   }
  }
  internal bool CompleteWrite(InputFrame f,bool returned,int error) {
   lock(Gate){
    if(!Object.ReferenceEquals(Active,f)||f.State!=InputFrameState.InFlight)throw new InvalidOperationException("INPUT_ORIGINAL_FRAME");
    f.Returned=returned;f.Error=error;Returns++;
    bool full=returned&&f.RawWritten==(uint)f.Length;
    if(returned&&f.RawWritten<=(uint)f.Length)ReportedSuccessBytes+=f.RawWritten;
    if(full){Delivered++;DeliveredBytes+=f.Length;f.State=InputFrameState.Delivered;if(f.ExitLine)TerminalDelivered=true;}
    else f.State=InputFrameState.Unknown;
    if(!full||Phase==InputPhase.Poisoned){Phase=InputPhase.Poisoned;return false;}
    Phase=InputPhase.Delivered;return true;
   }
  }
  internal void Poison() {
   lock(Gate){Phase=InputPhase.Poisoned;
    // Keep InFlight until a real return is latched. Never zero raw outputs/buffers.
    if(Active!=null&&Active.State==InputFrameState.Queued)Active.State=InputFrameState.Unknown;}
  }
  internal bool ReviewAllowed(long outSeen,long errSeen) {
   lock(Gate){
    if(Phase==InputPhase.Waiting)return outSeen>0||errSeen>0;
    return Phase==InputPhase.Delivered&&!TerminalDelivered&&Active!=null;
    // The next command may follow a journal-only stage. ROOT must validate its
    // real receipt externally; confirmations additionally require actual prompt bytes.
   }
  }
  internal int DeliveredCount(){lock(Gate){return Delivered;}}
  internal long OutputBaseline(){lock(Gate){return Active==null?0:Active.WriteStartOut;}}
  internal void Seal() {
   lock(Gate){
    if(Phase!=InputPhase.Delivered||!TerminalDelivered||Delivered!=Reserved||Reserved!=InputProtocol1701.Count||RequestedBytes!=InputProtocol1701.NormalBytes||Returns!=Attempts||Attempts!=Reserved
     ||DeliveredBytes!=RequestedBytes)throw new InvalidOperationException("INPUT_NOT_FULLY_DELIVERED_EXIT");
    Phase=InputPhase.Sealing;
   }
  }
  internal bool EndIfSealed() {lock(Gate){if(Phase!=InputPhase.Sealing)return false;Phase=InputPhase.Ended;return true;}}
  internal bool Ready() {
   lock(Gate){return Phase==InputPhase.Ended&&TerminalDelivered&&Reserved==InputProtocol1701.Count&&RequestedBytes==InputProtocol1701.NormalBytes&&Reserved==Delivered
    &&Attempts==Reserved&&Returns==Attempts&&DeliveredBytes==RequestedBytes&&ReportedSuccessBytes==RequestedBytes;}
  }
  internal InputObservation1701 Snapshot() {
   lock(Gate){
    InputFrameObservation1701[] rows=new InputFrameObservation1701[Reserved];
    for(int i=0;i<Reserved;i++){
     InputFrame f=Frames[i];rows[i]=new InputFrameObservation1701{Ordinal=f.Ordinal,WireBytes=f.Length,Sha256=f.Sha,State=f.State.ToString(),
      NativeReturned=f.Returned,RawWritten=f.State==InputFrameState.Queued?(uint?)null:f.RawWritten,Error=f.Error,
      ReviewedStdout=f.ReviewedOut,ReviewedStderr=f.ReviewedErr,NativeWriteStartStdout=f.WriteStartOut,NativeWriteStartStderr=f.WriteStartErr};
    }
    return new InputObservation1701{State=Phase.ToString(),Reserved=Reserved,WriteAttempts=Attempts,NativeReturns=Returns,Delivered=Delivered,
     RequestedBytes=RequestedBytes,ReportedSuccessfulBytes=ReportedSuccessBytes,DeliveredBytes=DeliveredBytes,Frames=rows};
   }
  }
 }
 internal static class InputProofRules1701 {
  internal static bool NativeReady(Completion original,bool deliveredAndEnded,bool signaled,uint? code) {
   return original!=null&&original.Ready()&&deliveredAndEnded&&signaled&&code==0;
  }
 }
 internal static class ReviewTicketRules1701 {
  internal static void Validate(ReviewTicket1701 current,ReviewTicket1701 offered,object context,int delivered,bool reviewAllowed) {
   if(offered==null||!Object.ReferenceEquals(current,offered)||!Object.ReferenceEquals(offered.Context,context)
    ||offered.Used||offered.Delivered!=delivered||!reviewAllowed)throw new InvalidOperationException("WRONG_OR_STALE_INPUT_REVIEW");
  }
 }
 public sealed class InputFrameObservation1701 {
  public int Ordinal,WireBytes,Error;public string Sha256,State;public bool? NativeReturned;public uint? RawWritten;
  public long ReviewedStdout,ReviewedStderr,NativeWriteStartStdout,NativeWriteStartStderr;
 }
 public sealed class InputObservation1701 {
  public string State;public int Reserved,WriteAttempts,NativeReturns,Delivered;
  public long RequestedBytes,ReportedSuccessfulBytes,DeliveredBytes;public InputFrameObservation1701[] Frames;
  public uint? NativeThreadId,NativeThreadDword;public ulong? NativeThreadCreationFileTime;public bool NativeThreadSignaled;
  public int NativeLoopRounds,NativeFinishWaitRounds;
  public string Source="ROOT_SAME_INSTANCE_REVIEWED_LINE_SEAM_NOT_HOST_STDIN_OR_PRODUCT_CAPABILITY";
  public string HostStdinEOF="NOT_APPLICABLE_NOT_READ";
  public string DownstreamEOF="CLOSED_ONLY_AFTER_ORIGINAL_P0_OUTPUT_EOF2_AND_INPUT_NATIVE_THREAD0_NOT_A_WORKFLOW_PROOF";
 }
 public sealed class ReviewTicket1701 {
  internal readonly object Context;internal readonly int Delivered;internal readonly long Out,Err;
  internal bool Used;
  internal ReviewTicket1701(object context,int delivered,long output,long error){Context=context;Delivered=delivered;Out=output;Err=error;}
 }
 public sealed class ControlledObservation1701 {
  public CaptureObservation Capture;public InputObservation1701 Input;public byte[] Stdout,Stderr;
  public long StdoutThrough,StderrThrough;public ReviewTicket1701 Review;
  public int NextLineOrdinal,NextStepOrdinal;public string ExpectedLine,NextLineKind;
  public string Boundary="OUTPUT_REVIEW_TICKET_IS_NOT_PRODUCT_APPROVAL_OR_47_9_SUCCESS";
 }
 public static partial class RootPPrebirth {
  public sealed class ControlledSession1701 {
   readonly Owner owner;ReviewTicket1701 current;long outSeen,errSeen;bool closed;
   internal ControlledSession1701(Owner original){owner=original;}
   public ControlledObservation1701 Observe(int maxPerStream) {
    try{return ObserveCore(maxPerStream);}catch{current=null;owner.Input.Ledger.Poison();owner.Fail("ROOT_OBSERVATION_UNWIND");throw;}
   }
   ControlledObservation1701 ObserveCore(int maxPerStream) {
    if(maxPerStream<1||maxPerStream>512){owner.Fail("ROOT_OUTPUT_VIEW_CAP");throw new InvalidOperationException("ROOT_OUTPUT_VIEW_CAP");}
    if(Volatile.Read(ref owner.Unknown)==0&&!closed){
     try{owner.ObserveOnce();}catch{owner.Fail("CONTROLLED_OBSERVATION_FAILED");}
    }
    byte[] output=owner.CopyOutput(owner.Out,outSeen,maxPerStream);outSeen+=output.Length;
    byte[] error=owner.CopyOutput(owner.Err,errSeen,maxPerStream);errSeen+=error.Length;
    current=null;
    if(Volatile.Read(ref owner.Unknown)==0&&!closed&&outSeen==Volatile.Read(ref owner.Out.Published)&&errSeen==Volatile.Read(ref owner.Err.Published)
     &&owner.Input.Ledger.ReviewAllowed(outSeen,errSeen)
     &&InputProtocol1701.Prompt(owner.Out.Evidence,(int)owner.Input.Ledger.OutputBaseline(),(int)outSeen,owner.Input.Ledger.DeliveredCount()))
     current=new ReviewTicket1701(this,owner.Input.Ledger.DeliveredCount(),outSeen,errSeen);
    CaptureObservation capture=owner.Snapshot(closed?"CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY":Volatile.Read(ref owner.Unknown)!=0?"UNKNOWN_ORIGINALS_RETAINED_NO_REPLAY":"CONTROLLED_INPUT_PENDING");capture.Input=null; // one history copy in this transport view
    return new ControlledObservation1701{Capture=capture,
     Input=owner.InputSnapshot(),Stdout=output,Stderr=error,StdoutThrough=outSeen,StderrThrough=errSeen,Review=current,
     NextLineOrdinal=owner.Input.Ledger.DeliveredCount()+1,NextStepOrdinal=InputProtocol1701.Step(owner.Input.Ledger.DeliveredCount()),ExpectedLine=InputProtocol1701.Text(owner.Input.Ledger.DeliveredCount()),NextLineKind=InputProtocol1701.Kind(owner.Input.Ledger.DeliveredCount())};
   }
   // One ROOT reviewed line only. No list/script callback, no automatic confirm,
   // no queued second frame. Native return is observed later, not fabricated here.
   public void OfferReviewedLine(ReviewTicket1701 ticket,string exactWithoutEol) {
    try{
     owner.Check();if(closed)throw new InvalidOperationException("INPUT_ALREADY_CLOSED_NO_QUERY");owner.ObserveOnce();
     if(owner.ProcessHasSignaled)throw new InvalidOperationException("INPUT_PROCESS_ALREADY_ENDED");
     ReviewTicketRules1701.Validate(current,ticket,this,owner.Input.Ledger.DeliveredCount(),ticket!=null&&owner.Input.Ledger.ReviewAllowed(ticket.Out,ticket.Err));
     byte[] wire=InputRules1701.Line(exactWithoutEol);string sha=InputRules1701.Hash(wire);
     ticket.Used=true;current=null;
     owner.Input.Ledger.Queue(wire,sha,ticket.Out,ticket.Err);
    }catch{owner.Input.Ledger.Poison();owner.Fail("ROOT_INPUT_OFFER_REJECTED");throw;}
   }
   public CaptureObservation FinishAfterDeliveredExit() {
    try{
     owner.Check();if(closed)throw new InvalidOperationException("NO_SECOND_FINISH");
     current=null;owner.FinishControlled();closed=true;return owner.Snapshot("CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY");
    }catch{owner.Input.Ledger.Poison();owner.Fail("ROOT_CONTROLLED_FINISH_UNRESOLVED");return owner.Snapshot("UNKNOWN_ORIGINALS_RETAINED_NO_REPLAY");}
   }
   public CaptureObservation WithholdFurtherInput() {
    try{owner.Check();}catch{owner.Fail("ROOT_WITHHOLD_WRONG_OR_UNRESOLVED_CALLER");}
    current=null;owner.Input.Ledger.Poison();owner.Fail("ROOT_DRIVER_WITHHELD_FURTHER_INPUT");
    return owner.Snapshot("UNKNOWN_ORIGINALS_RETAINED_NO_REPLAY");
   }
  }
  public static ControlledSession1701 BeginControlled(RootSelection selection) {
   // The legacy1682 namespace remains parsable for its immutable pure tests
   // only. The sole effectful1701 entry refuses that old runtime namespace.
   if(selection==null)throw new InvalidOperationException("UNBOUND_NOT_EXECUTABLE_78");
   selection.Validate();
   if(selection.Install!=@"C:\Morrow-C28-RootP1701-bin-"+selection.Tag)throw new InvalidOperationException("FRESH1701_INSTALL_REQUIRED");
   if(Interlocked.Exchange(ref attempted,1)!=0)throw new InvalidOperationException("NO_REPLAY_78");
   Owner o=new Owner(selection);retained=o;ControlledSession1701 session=new ControlledSession1701(o);
   try{o.Start();}catch{o.Fail("CONTROLLED_P_START_FAILED");}
   return session;
  }
  internal sealed class InputPump1701 {
   internal readonly Slot Thread;internal readonly InputLedger1701 Ledger=new InputLedger1701();
   internal readonly Native.ThreadStart Callback;internal uint Id;internal ulong? Created;internal uint? Exit;internal bool Signaled;
   internal int Finished,Polls,FinishPolls;
   internal InputPump1701(Owner original){Thread=original.NewSlot("ROOT_INPUT_NATIVE_THREAD");Callback=delegate(IntPtr unused){return original.PumpInput();};}
  }
  internal sealed partial class Owner {
   internal readonly InputPump1701 Input;
   internal bool ProcessHasSignaled{get{return Facts.ProcessSignal;}}
   readonly DwordQuery InputQuery=new DwordQuery();
   internal void StartInputThread() {
    Check();Input.Thread.Begin();
    IntPtr raw=Native.CreateThread(IntPtr.Zero,new UIntPtr(1048576),Input.Callback,IntPtr.Zero,0,ref Input.Id);
    int error=raw==IntPtr.Zero?System.Runtime.InteropServices.Marshal.GetLastWin32Error():0;
    Accept(Input.Thread,raw,raw!=IntPtr.Zero,error);
    Need(Input.Id!=0&&Native.GetThreadId(raw)==Input.Id,"ORIGINAL_INPUT_THREAD_ID");
    Native.FT created,exit,kernel,user;
    Need(Native.GetThreadTimes(raw,out created,out exit,out kernel,out user)&&created.Value!=0,"ORIGINAL_INPUT_THREAD_CREATION");
    Input.Created=created.Value;
   }
   internal uint PumpInput() {
    try{
     for(;;){
      if(Volatile.Read(ref Unknown)!=0){Input.Ledger.Poison();return 1;}
      if(++Input.Polls>1200000||!Rules.Budget(Clock.ElapsedMilliseconds,Plan.DeadlineMs)){
       Input.Ledger.Poison();Fail("ROOT_INPUT_DEADLINE_OR_POLL_CAP");return 2;}
      if(Input.Ledger.EndIfSealed())return 0;
      InputFrame frame=Input.Ledger.BeginWrite(Volatile.Read(ref Out.Published),Volatile.Read(ref Err.Published));
      if(frame==null){Thread.Sleep(5);continue;}
      // Direct ref to the original frame output; no retry/append after any partial
      // result. Frame wire and OriginalWriteBuffer stay unchanged until retirement.
      bool ok=Native.WriteFile(InputWrite.Raw,Input.Ledger.OriginalWriteBuffer,(uint)frame.Length,ref frame.RawWritten,IntPtr.Zero);
      int error=ok?0:System.Runtime.InteropServices.Marshal.GetLastWin32Error();
      if(!Input.Ledger.CompleteWrite(frame,ok,error)){Fail("ROOT_INPUT_WRITE_FAILED_PARTIAL_OR_POISONED");return 3;}
     }
    }catch{Input.Ledger.Poison();Fail("ROOT_INPUT_NATIVE_THREAD_UNWIND");return 4;}
    finally{Volatile.Write(ref Input.Finished,1);}
   }
   internal byte[] CopyOutput(Reader reader,long after,int maximum) {
    long published=Volatile.Read(ref reader.Published);
    if(after<0||after>published)throw new InvalidOperationException("OUTPUT_CURSOR");
    int take=(int)Math.Min((long)maximum,published-after);byte[] b=new byte[take];
    Buffer.BlockCopy(reader.Evidence,(int)after,b,0,take);return b;
   }
   internal InputObservation1701 InputSnapshot() {
    InputObservation1701 view=Input.Ledger.Snapshot();view.NativeThreadId=Input.Id==0?(uint?)null:Input.Id;
    view.NativeThreadCreationFileTime=Input.Created;view.NativeThreadDword=Input.Exit;view.NativeThreadSignaled=Input.Signaled;
    view.NativeLoopRounds=Input.Polls;view.NativeFinishWaitRounds=Input.FinishPolls;return view;
   }
   internal void FinishControlled() {
    Check();
    // First wait for genuine P/thread0 and BOTH output EOF/native reader0.
    // Keep stdin writer OPEN: never drive C to exit with synthetic EOF.
    InputObservation1701 before=Input.Ledger.Snapshot();
    Need(before.State=="Delivered"&&Input.Ledger.TerminalDelivered,"NO_DELIVERED_EXIT_LINE");
    while(!Facts.Ready()){ObserveOnce();if(!Facts.Ready())Thread.Sleep(5);}
    Check();Input.Ledger.Seal();
    while(!Input.Signaled){Check();Need(++Input.FinishPolls<=1200000,"INPUT_FINITE_FINISH_WAIT_CAP");Input.Signaled=Signal(Input.Thread);if(!Input.Signaled)Thread.Sleep(5);}
    Input.Exit=Code(Input.Thread,false);
    Need(Input.Exit==0&&Input.Ledger.Ready(),"ORIGINAL_INPUT_THREAD_OR_DELIVERY_UNRESOLVED");
    Retire();
   }
   void RetireInput() {
    Check();Need(InputProofRules1701.NativeReady(Facts,Input.Ledger.Ready(),Input.Signaled,Input.Exit),"INPUT_NATIVE_CLOSURE_REQUIRED");
    Close(Input.Thread);Close(InputWrite);
   }
  }
 }
}
