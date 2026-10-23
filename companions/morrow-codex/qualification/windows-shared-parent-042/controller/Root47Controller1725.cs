// SOURCE1725 guest-only explicit Main/controller. Default/invalid invocation
// returns78 before native effects. Valid invocation is EFFECTFUL and requires
// ROOT's external real approval/VM/controller/loader/deployment checks.
using System;
using System.Collections.Generic;
using System.Text;
using System.Threading;
namespace C28.RootP1682 {
 public static class Root47Controller1725 {
  static Driver1725 retained;static int attempted;
  public static int Main(string[] args){
   string mode,tag,pin;
   if(!Invocation1725.Parse(args,out mode,out tag,out pin))return 78;
   if(Interlocked.Exchange(ref attempted,1)!=0)return 78;
   Driver1725 owner=new Driver1725(mode,tag,pin);retained=owner;
   try{return owner.Run();}catch(Exception error){owner.CacheFailure(error);owner.KeepUnknown();return 79;} // KeepUnknown never returns
  }
 }
 internal static class Invocation1725 {
  internal static bool Parse(string[] args,out string mode,out string tag,out string pin){
   mode=null;tag=null;pin=null;
   if(args==null||args.Length<2||!Rules.Hex(args[1],32))return false;
   if(args[0]=="--gather-pins-1725"&&args.Length==2){mode="gather";tag=args[1];return true;}
   if(args[0]==InputProtocol1701.RunFlag&&args.Length==3&&Rules.Hex(args[2],64)){mode="run";tag=args[1];pin=args[2];return true;}
   return false;
  }
 }
#if !P2_SHARED_PARENT_REFUSAL_SESSION
 internal sealed class Driver1725 {
  readonly string Mode,Tag,BindingSha,Install,Namespace,JournalPath,CollectorPath;
  readonly Io1725 Io=new Io1725();readonly JournalPrefix1725 Prefix=new JournalPrefix1725();
  readonly TokenPreflight1725 Preflight=new TokenPreflight1725();
  readonly StepGate1725 Gate=new StepGate1725(RolePins1725.ShellSha);
  readonly byte[] Stdout=new byte[2097152],Stderr=new byte[2097152];
  readonly FirstPromptGate2169 FirstPrompt;
  RootSelection Selection;RootPPrebirth.ControlledSession1701 Session;ControlledObservation1701 Last;
  CaptureObservation Final;int OutLength,ErrLength,Delivered,MainPolls,JournalReadCalls;
  long JournalReadBytes,CriticalStart=-1,ContextStart=-1,StageStart;int StepIndex;bool Finished,InitialAbsent;
  bool DiagnosticAttempted;
  internal void CacheFailure(Exception error){try{Io.CaptureException(error);}catch{}}
  internal Driver1725(string mode,string tag,string binding){
   Mode=mode;Tag=tag;BindingSha=binding;Install=@"C:\Morrow-C28-RootP1701-bin-"+tag;
   Namespace=@"C:\Morrow-C28-A1-047-"+tag;JournalPath=Namespace+@"\case.evidence\journal.jsonl";
   CollectorPath=@"C:\Morrow-C28-Root47-1725-"+mode+"-"+tag;
   FirstPrompt=new FirstPromptGate2169(Stdout,OutLength);
  }
  internal int Run(){
   Io.Begin();Io.Open(Io.Root,@"C:\",true,0x80000000,1,3,false);
   if(Mode=="gather"){Gather();Io.NormalCloseAll();return 0;}
   // A missing or known invalid selector can return78 ONLY after actual checked
   // retirement of its new readonly handles; no P/controller output was created.
   bool bound=false;
   try{bound=LoadBinding();}catch{if(Io.Unknown)throw;}
   if(!bound){Io.NormalCloseAll();return 78;}
   // Missing privilege is a known NoP/T result ONLY after the original token,
   // query buffer and all readonly controller inputs have actually retired.
   // Only this new public collector is created for the known-ineligible receipt;
   // no product namespace, P or T is created on this return77 path.
   if(!Preflight.Check(Io)){
    Io.CreateCollector(CollectorPath);
    Io.Event(D("event","KNOWN_INELIGIBLE_NO_P_T_OR_PRODUCT_EFFECTS","facts",Preflight.Data(),
     "scope","THIS_CONTROLLER_INVOCATION_BEFORE_BEGIN_CONTROLLED_NOT_GLOBAL_PROCESS_ABSENCE"));
    Io.NormalCloseAll();return 77;
   }
   Io.FreshAbsent(Namespace);Io.FreshAbsent(Namespace+@"\case.evidence");Io.FreshAbsent(JournalPath);InitialAbsent=true;
   Io.CreateCollector(CollectorPath);
   Io.Event(D("event","ROOT_SELECTOR_BOUND_NOT_HUMAN_OR_VM_AUTHORITY","tag",Tag,"binding_sha256",BindingSha));
   Io.Event(D("event","READONLY_PRIMARY_TOKEN_PRIVILEGES_PRESENT_NOT_ENABLED_OR_APPROVAL","facts",Preflight.Data()));
   Io.At("BEGIN_CONTROLLED",null);
   Session=RootPPrebirth.BeginControlled(Selection); // same original native caller for every following call
   for(StepIndex=0;StepIndex<47;StepIndex++){
    Gate.Expect(StepIndex);StageStart=Io.Clock.ElapsedMilliseconds;
    if(StepIndex==19)CriticalStart=StageStart; // before native-review creates the actual30s intent
    if(StepIndex==34)ContextStart=StageStart; // before native-context, never extend its60s lease
    Step1725 step=Gate.Current;
    Io.Event(D("event","STEP_EXPECTED_NOT_APPLIED","ordinal",step.Ordinal,"command",step.Command));
    for(int line=step.FirstLine;line<=step.LastLine;line++){
     for(;;){
      Tick();
      if(Last.Review!=null&&Last.Input.Delivered==line){
       // Only first offer needs the actual complete prompt in THIS stdout.
       // stderr, diagnostic records and scalar progress cannot satisfy it.
       if(StepIndex!=0||line!=0||FirstPrompt.Observe(Stdout,FirstPrompt.CheckedThrough,OutLength))break;
      }
      Thread.Sleep(5);
     }
     Io.Need(Last.NextLineOrdinal==line+1&&Last.ExpectedLine==InputProtocol1701.Text(line),"SAME_INSTANCE_NEXT_LINE");
     Session.OfferReviewedLine(Last.Review,InputProtocol1701.Text(line)); // one line, never a batch
     do{Tick();if(Delivered<line+1)Thread.Sleep(5);}while(Delivered<line+1);
    }
    while(!step.Returned){Tick();if(!step.Returned)Thread.Sleep(5);}
    Io.Need(Gate.Completed==StepIndex+1&&Delivered==step.LastLine+1,"STAGE_NOT_CLOSED");
    Io.Event(D("event","STEP_RETURNED_NOT_NINE_GATE_ACCEPTANCE","ordinal",step.Ordinal,"command",step.Command,
     "reservation_sequence",step.ReservationSequence.HasValue?(object)step.ReservationSequence.Value:null,
     "return_sequence",step.ReturnSequence.Value,"delivered",Delivered,"stdout_through",OutLength,"stderr_through",ErrLength,
     "stdout_prefix_sha256",HashPrefix(Stdout,OutLength),"stderr_prefix_sha256",HashPrefix(Stderr,ErrLength)));
   }
   Io.Need(Delivered==92&&Gate.Completed==47&&Last.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"CURRENT47_TRANSPORT_OR_GATE_INCOMPLETE");
   Final=Session.FinishAfterDeliveredExit();ValidateFinal(Final);
   // Drain only already stored helper prefix; its closed Observe path does not
   // query any closed original handle. Journal is still OUR original read handle.
   for(;;){
    Last=Session.Observe(512);AppendOutput(Last);
    Io.Need(Last.Capture.Status=="CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY"&&Last.Review==null,"CLOSED_DATA_ONLY");
    if(Last.Stdout.Length==0&&Last.Stderr.Length==0)break;
   }
   Io.Need(Final.StdoutBytes==OutLength&&Final.StderrBytes==ErrLength
    &&Final.StdoutSha256==HashPrefix(Stdout,OutLength)&&Final.StderrSha256==HashPrefix(Stderr,ErrLength),"ORIGINAL_FULL_STREAMS_MATCH_HELD_LOGS");
   PollJournal();Io.Need(Prefix.Parsed==Prefix.Length&&Gate.Completed==47,"FINAL_PARTIAL_JOURNAL");
   Io.Event(D("event","ORIGINAL_HELPER_CLOSED_NOT_FULL_PRODUCT_ACCEPTANCE","steps",47,"lines",92,"bytes",InputProtocol1701.NormalBytes,
    "journal_bytes",Prefix.Length,"journal_records",Prefix.Records,"stdout_sha256",HashPrefix(Stdout,OutLength),"stderr_sha256",HashPrefix(Stderr,ErrLength)));
   WriteTerminal();Io.NormalCloseAll();Finished=true;
   // Exit0 follows checked-close of all controller-owned original resources.
   // The terminal file explicitly does NOT pretend to contain its own later close witness.
   return 0;
  }
  bool LoadBinding(){
   string path=@"C:\Morrow-C28-Root47-1725-binding-"+Tag+".json";
   if(!Io.Open(Io.Binding,path,false,0x80000000,1,3,true))return false;
   Io.Need(Io.Binding.First.Bytes>0&&Io.Binding.First.Bytes<=131072,"BINDING_BYTES");
   string actual=Io.Hash(Io.Binding,Io.Binding.First.Bytes,false);if(actual!=BindingSha)throw new InvalidOperationException("ROOT_BINDING_BYTES_SHA");
   Io.Seek(Io.Binding,0);int at=0,wanted=(int)Io.Binding.First.Bytes;
   while(at<wanted){int n=Io.Read(Io.Binding,Io.Scratch,Math.Min(Io.Scratch.Length,wanted-at));Io.Need(n>0,"BINDING_SHORT_READ");Buffer.BlockCopy(Io.Scratch,0,Io.BindingBytes,at,n);at+=n;}
   Io.Need(FileMetadataRules1718.Same(Io.Binding.First,Io.Identity(Io.Binding,path,false)),"BINDING_HELD_STABILITY");
   Io.At("BINDING_PARSE",null);
   string text=new UTF8Encoding(false,true).GetString(Io.BindingBytes,0,wanted);
   Dictionary<string,object> root=Json1725.Obj(Json1725.Parse(text));
   if(root.Count!=7||Json1725.Str(Json1725.Get(root,"schema"))!="ROOT47_BINDING1725_V1"
    ||Json1725.Str(Json1725.Get(root,"tag"))!=Tag||Json1725.Str(Json1725.Get(root,"install"))!=Install
    ||Json1725.Str(Json1725.Get(root,"inventory"))!=RootSelection.FrozenInventory
    ||Json1725.Str(Json1725.Get(root,"shell_sha256"))!=RolePins1725.ShellSha)throw new InvalidOperationException("UNBOUND_SELECTOR");
   ulong deadline=Json1725.UInt(Json1725.Get(root,"deadline_ms"));if(deadline<1||deadline>5500000)throw new InvalidOperationException("SELECTOR_DEADLINE");
   List<object> rows=Json1725.Get(root,"pins") as List<object>;if(rows==null||rows.Count<1||rows.Count>40)throw new InvalidOperationException("SELECTOR_PIN_COUNT");
   FilePin[] pins=new FilePin[rows.Count];
   for(int i=0;i<pins.Length;i++){
    Dictionary<string,object> r=Json1725.Obj(rows[i]);if(r.Count!=10)throw new InvalidOperationException("PIN_KEYS");
    pins[i]=new FilePin(Json1725.Str(Json1725.Get(r,"path")),Json1725.Bool(Json1725.Get(r,"directory")),
     Json1725.UInt(Json1725.Get(r,"volume")),Json1725.Str(Json1725.Get(r,"file_id")),Json1725.UInt(Json1725.Get(r,"bytes")),
     Json1725.UInt(Json1725.Get(r,"creation")),Json1725.UInt(Json1725.Get(r,"modified")),
     checked((uint)Json1725.UInt(Json1725.Get(r,"attributes"))),checked((uint)Json1725.UInt(Json1725.Get(r,"links"))),
     Json1725.Get(r,"sha256")==null?null:Json1725.Str(Json1725.Get(r,"sha256")));
   }
   Io.At("BINDING_SELECTOR_VALIDATE",null);
   Selection=new RootSelection(Install,Tag,RootSelection.FrozenInventory,(int)deadline,pins);RolePins1725.Check(Selection);return true;
  }
  void Gather(){
   Io.At("GATHER_BEGIN",null);
   Io.CreateCollector(CollectorPath);
   RootSelection provisional=new RootSelection(Install,Tag,RootSelection.FrozenInventory,5500000,new FilePin[0]);
   List<string> paths=provisional.Paths();FilePin[] pins=new FilePin[paths.Count];
   for(int i=0;i<paths.Count;i++){
    string path=paths[i];bool file=path==RootPPrebirth.Shell;
    foreach(string name in RootPPrebirth.RoleNames)file|=path==Install+"\\"+name;
    Io.Open(Io.Pins[i],path,!file,0x80000000,1,3,false);
    FilePin observed=Io.Pins[i].First;string digest=file?Io.Hash(Io.Pins[i],observed.Bytes,true):null;
    Io.Need(FileMetadataRules1718.Same(observed,Io.Identity(Io.Pins[i],path,!file)),"GATHER_STABILITY");
    pins[i]=FileMetadataRules1718.WithDigest(observed,digest);
   }
   Io.At("GATHER_SELECTOR_VALIDATE",null);
   Selection=new RootSelection(Install,Tag,RootSelection.FrozenInventory,5500000,pins);RolePins1725.Check(Selection);
   List<object> data=new List<object>();foreach(FilePin pin in pins)data.Add(PinData(pin));
   Dictionary<string,object> binding=D("schema","ROOT47_BINDING1725_V1","tag",Tag,"install",Install,"inventory",RootSelection.FrozenInventory,
    "deadline_ms",5500000,"shell_sha256",RolePins1725.ShellSha,"pins",data);
   Io.Open(Io.Gather,CollectorPath+@"\gathered-binding.json",false,0x40000000,1,1,false);
   Io.Write(Io.Gather,new UTF8Encoding(false,true).GetBytes(Json1725.Encode(binding)+"\n"),131073);
   Io.Need(RootPPrebirth.Native.FlushFileBuffers(Io.Gather.Handle.Raw),"GATHER_FLUSH");
   Io.Event(D("event","GATHER_ONLY_NO_P_NO_APPROVAL","pins",pins.Length));
  }
  void Tick(){
   Io.Check();Io.Need(++MainPolls<=1200000,"MAIN_POLL_CAP");
   long elapsed=Io.Clock.ElapsedMilliseconds;
   long stageLimit=StepIndex==3?600000:120000;
   Io.Need(elapsed-StageStart<stageLimit,"STAGE_BUDGET");
   if(CriticalStart>=0&&StepIndex<=39)Io.Need(elapsed-CriticalStart<29000,"NATIVE_INTENT_CONSERVATIVE29S");
   if(ContextStart>=0&&StepIndex<=39)Io.Need(elapsed-ContextStart<59000,"NATIVE_CONTEXT_CONSERVATIVE59S");
   Last=Session.Observe(512);
   Io.Need(Last.Capture.Status=="CONTROLLED_INPUT_PENDING"&&Last.Capture.FirstError==null&&Last.Capture.CreationFileTime.HasValue,"HELPER_UNKNOWN_OR_STARTUP");
   Io.Need(!Last.Capture.ProcessDword.HasValue||(StepIndex==46&&Last.Input.Delivered==92&&Last.Capture.ProcessDword==0),"P_EXIT_BEFORE_EXPLICIT_FLOW");
   AppendOutput(Last);ObserveDelivery(Last.Input);PollJournal();
  }
  void AppendOutput(ControlledObservation1701 view){
   Io.Need(view.Stdout!=null&&view.Stderr!=null&&view.Stdout.Length<=512&&view.Stderr.Length<=512
    &&view.StdoutThrough==OutLength+view.Stdout.Length&&view.StderrThrough==ErrLength+view.Stderr.Length
    &&view.StdoutThrough<=Stdout.Length&&view.StderrThrough<=Stderr.Length,"HELPER_PREFIX_CURSOR");
   Buffer.BlockCopy(view.Stdout,0,Stdout,OutLength,view.Stdout.Length);OutLength+=view.Stdout.Length;
   Buffer.BlockCopy(view.Stderr,0,Stderr,ErrLength,view.Stderr.Length);ErrLength+=view.Stderr.Length;
  }
  void ObserveDelivery(InputObservation1701 input){
   Io.Need(input!=null&&input.State!="Poisoned"&&input.Delivered>=Delivered&&input.Delivered<=Delivered+1
    &&input.Delivered<=92&&input.NativeReturns==input.Delivered,"INPUT_NATIVE_RETURN_NOT_PENDING_SUCCESS");
   if(input.Delivered==Delivered)return;
   InputFrameObservation1701 f=input.Frames[Delivered];byte[] wire=InputProtocol1701.Wire(Delivered);
   Io.Need(f.Ordinal==Delivered+1&&f.NativeReturned==true&&f.RawWritten==(uint)wire.Length&&f.WireBytes==wire.Length&&f.Sha256==InputRules1701.Hash(wire),"REAL_LINE_DELIVERY");
   Delivered++;
   Io.Event(D("event","ONE_ORIGINAL_LINE_DELIVERED_NOT_STAGE_SUCCESS","line",Delivered,"step",InputProtocol1701.Step(Delivered-1),
    "wire_bytes",f.WireBytes,"wire_sha256",f.Sha256,"stdout_prefix",f.ReviewedStdout,"stderr_prefix",f.ReviewedStderr));
  }
  void PollJournal(){
   Io.Check();Io.Need(InitialAbsent,"NO_INITIAL_FRESH_ABSENCE");
   if(Io.Journal.Handle.State!=ResourceState.Open){
    Io.Need(++Io.OpenJournalAttempts<=1200000,"JOURNAL_OPEN_PROBE_CAP");
    if(Io.JournalNamespace.Handle.State!=ResourceState.Open&&!Io.Open(Io.JournalNamespace,Namespace,true,0x80000000,1,3,true))return;
    if(Io.EvidenceDirectory.Handle.State!=ResourceState.Open&&!Io.Open(Io.EvidenceDirectory,Namespace+@"\case.evidence",true,0x80000000,1,3,true))return;
    if(!Io.Open(Io.Journal,JournalPath,false,0x80000000,3,3,true))return;
    Io.Need(Last!=null&&Last.Capture.CreationFileTime.HasValue&&Io.Journal.First.CreationFileTime>=Last.Capture.CreationFileTime.Value,"JOURNAL_CREATED_AFTER_ORIGINAL_P");
   }
   FilePin before=Io.Identity(Io.Journal,JournalPath,false);StableJournal(before);
   Io.Need(before.Bytes>=(ulong)Prefix.Length&&before.Bytes<=33554432,"JOURNAL_LENGTH");
   int old=Prefix.Length,length=(int)before.Bytes,offset=0;Io.Seek(Io.Journal,0);
   while(offset<length){
    Io.Need(++JournalReadCalls<=1000000,"JOURNAL_READ_CALL_CAP");
    int n=Io.Read(Io.Journal,Io.Scratch,Math.Min(Io.Scratch.Length,length-offset));Io.Need(n>0,"JOURNAL_SHORT_SNAPSHOT");
    JournalReadBytes+=n;Io.Need(JournalReadBytes<=8589934592L,"JOURNAL_REREAD_BYTE_CAP");
    Prefix.CompareAndStore(offset,Io.Scratch,n,old);offset+=n;
   }
   FilePin after=Io.Identity(Io.Journal,JournalPath,false);StableJournal(after);Io.Need(after.Bytes>=(ulong)length,"JOURNAL_SHRANK_DURING_READ");
   foreach(JournalRecord1725 record in Prefix.Commit(length)){
    Gate.Accept(record);
    Io.Event(D("event","ACTUAL_JOURNAL_RECORD","sequence",record.Sequence,"kind",record.Kind,"start",record.Start,"through",record.End,"sha256",record.Sha256));
   }
  }
  void StableJournal(FilePin value){
   FilePin first=Io.Journal.First;
   Io.Need(!value.Directory&&value.Path==first.Path&&value.Volume==first.Volume&&value.FileId==first.FileId
    &&value.CreationFileTime==first.CreationFileTime&&value.Attributes==first.Attributes&&value.Links==1,"ORIGINAL_JOURNAL_IDENTITY");
  }
  void ValidateFinal(CaptureObservation c){
   Io.Need(c.Status=="CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY"&&c.FirstError==null&&c.ProcessDword==0&&c.PrimaryThreadDword==0
    &&c.OutReaderDword==0&&c.ErrReaderDword==0&&c.StdoutEof&&c.StderrEof&&c.OriginalReadersSignaled&&c.CreatorWindowObserved
    &&c.Input!=null&&c.Input.NativeThreadSignaled&&c.Input.NativeThreadDword==0&&c.Input.State=="Ended"
    &&c.Input.Delivered==92&&c.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"ORIGINAL_FINAL_NATIVE_FACTS");
   Io.Need(c.CreatorNativeThreadId==Io.Creator&&c.Pid.HasValue&&c.Pid.Value!=0&&c.PrimaryThreadId.HasValue&&c.PrimaryThreadId.Value!=0
    &&c.CreationFileTime.HasValue&&c.CreationFileTime.Value!=0&&c.PrimaryCreationFileTime.HasValue&&c.PrimaryCreationFileTime.Value!=0
    &&c.OutReaderCreationFileTime.HasValue&&c.OutReaderCreationFileTime.Value!=0&&c.ErrReaderCreationFileTime.HasValue&&c.ErrReaderCreationFileTime.Value!=0
    &&c.Input.NativeThreadId.HasValue&&c.Input.NativeThreadId.Value!=0&&c.Input.NativeThreadCreationFileTime.HasValue&&c.Input.NativeThreadCreationFileTime.Value!=0,"SAME_ORIGINAL_NATIVE_IDENTITIES");
   foreach(SlotObservation slot in c.Slots)Io.Need(slot.State==ResourceState.ClosedVerified||slot.State==ResourceState.ProvenAbsent||slot.State==ResourceState.MemoryRetired,"HELPER_ALIAS_NOT_RETIRED");
   Io.Need(c.OriginalFilePins.Length==Selection.Pins.Length&&c.FinalFilePins.Length==Selection.Pins.Length,"FINAL_PIN_COUNT");
   for(int i=0;i<Selection.Pins.Length;i++){
    Io.Need(FileMetadataRules1718.Same(c.OriginalFilePins[i],c.FinalFilePins[i]),"FINAL_STABLE_PINS");
    if(!Selection.Pins[i].Directory)Io.Need(c.FinalFilePins[i].Sha256==Selection.Pins[i].Sha256,"FINAL_PE_FULL_SHA");
   }
  }
  void WriteTerminal(){
   Io.Open(Io.Terminal,CollectorPath+@"\terminal.json",false,0x40000000,1,1,false);
   List<object> steps=new List<object>();foreach(Step1725 step in Gate.Steps)steps.Add(D("ordinal",step.Ordinal,"command",step.Command,
    "reservation",step.ReservationSequence.HasValue?(object)step.ReservationSequence.Value:null,"returned",step.ReturnSequence.Value));
   Io.Write(Io.Terminal,new UTF8Encoding(false,true).GetBytes(Json1725.Encode(D("status",InputProtocol1701.TerminalStatus,
    "steps",steps,"delivered",Delivered,"input_bytes",InputProtocol1701.NormalBytes,"journal_bytes",Prefix.Length,"journal_sha256",HashPrefix(Prefix.Bytes,Prefix.Length),
    "stdout_bytes",OutLength,"stderr_bytes",ErrLength,"stdout_sha256",HashPrefix(Stdout,OutLength),"stderr_sha256",HashPrefix(Stderr,ErrLength),
    "primary_token_preflight",Preflight.Data(),"original_helper",FinalData(),"controller_resources_before_own_close",Io.ResourceData(),"self_close_witness","NOT_IN_THIS_PRE_CLOSE_FILE_ROOT_MUST_VERIFY_ORIGINAL_CONTROLLER_EXIT0",
    "source_accepted",false,"release_eligible",false,"SDK26_G04_SDK_FREEZE","OPEN"))+"\n"),131073);
   Io.Need(RootPPrebirth.Native.FlushFileBuffers(Io.Terminal.Handle.Raw),"TERMINAL_FLUSH");
  }
  Dictionary<string,object> FinalData(){
   List<object> slots=new List<object>(),queries=new List<object>(),frames=new List<object>(),initial=new List<object>(),last=new List<object>();
   foreach(SlotObservation s in Final.Slots)slots.Add(D("name",s.Name,"state",s.State.ToString(),"original_value_captured",s.OriginalValueCaptured,"api_returned",s.ApiReturned,"error",s.Error));
   foreach(DwordObservation q in Final.DwordQueries)queries.Add(D("name",q.Name,"raw_u32",q.RawValue,"returned",q.Returned,"error",q.Error,"after_original_signal",q.AfterOriginalSignal));
   for(int i=0;i<Final.Input.Delivered;i++){
    InputFrameObservation1701 f=Final.Input.Frames[i];
    frames.Add(D("ordinal",f.Ordinal,"wire_bytes",f.WireBytes,"sha256",f.Sha256,"state",f.State,"native_returned",f.NativeReturned,"raw_written",f.RawWritten,"error",f.Error,
     "reviewed_stdout",f.ReviewedStdout,"reviewed_stderr",f.ReviewedStderr,"write_start_stdout",f.NativeWriteStartStdout,"write_start_stderr",f.NativeWriteStartStderr));
   }
   foreach(FilePin pin in Final.OriginalFilePins)initial.Add(PinData(pin));foreach(FilePin pin in Final.FinalFilePins)last.Add(PinData(pin));
   return D("scope",Final.Scope,"status",Final.Status,"first_error",Final.FirstError,"no_replay",Final.NoReplay,
    "creator_native_thread_id",Final.CreatorNativeThreadId,"pid",Final.Pid,"primary_thread_id",Final.PrimaryThreadId,
    "process_creation_filetime",Final.CreationFileTime,"primary_creation_filetime",Final.PrimaryCreationFileTime,
    "out_reader_creation_filetime",Final.OutReaderCreationFileTime,"err_reader_creation_filetime",Final.ErrReaderCreationFileTime,
    "process_dword",Final.ProcessDword,"primary_dword",Final.PrimaryThreadDword,"out_reader_dword",Final.OutReaderDword,"err_reader_dword",Final.ErrReaderDword,
    "stdout_eof",Final.StdoutEof,"stderr_eof",Final.StderrEof,"original_readers_signaled",Final.OriginalReadersSignaled,"creator_window_observed",Final.CreatorWindowObserved,
    "create_attempted",Final.CreateWasAttempted,"create_returned",Final.CreateReturned,"create_success",Final.CreateSuccess,"create_error",Final.CreateError,"resume_return",Final.ResumeReturn,
    "stdout_bytes",Final.StdoutBytes,"stderr_bytes",Final.StderrBytes,"stdout_sha256",Final.StdoutSha256,"stderr_sha256",Final.StderrSha256,
    "input",D("state",Final.Input.State,"reserved",Final.Input.Reserved,"write_attempts",Final.Input.WriteAttempts,"native_returns",Final.Input.NativeReturns,"delivered",Final.Input.Delivered,
     "requested_bytes",Final.Input.RequestedBytes,"reported_successful_bytes",Final.Input.ReportedSuccessfulBytes,"delivered_bytes",Final.Input.DeliveredBytes,
     "native_thread_id",Final.Input.NativeThreadId,"native_thread_creation_filetime",Final.Input.NativeThreadCreationFileTime,"native_thread_dword",Final.Input.NativeThreadDword,"native_thread_signaled",Final.Input.NativeThreadSignaled,"frames",frames),
    "dword_queries",queries,"slots",slots,"original_file_pins",initial,"final_file_pins",last,
    "business_child_native_facts","NOT_PROVEN_BY_ROOT_P_HELPER","nine_gate_qualification","NOT_GRANTED");
  }
  internal void KeepUnknown(){
   Io.Fail("ROOT_CONTROLLER_UNCERTAINTY_RETAINED");
   if(Session!=null){try{Session.WithholdFurtherInput();}catch{}}
   if(!DiagnosticAttempted){
    DiagnosticAttempted=true;string body="{\"schema\":\"ROOT_CACHED_FAILURE1832_V1\",\"diagnostic\":\"UNAVAILABLE\"}";
    try{string candidate=Json1725.Encode(Io.DiagnosticData(Mode));if(Encoding.UTF8.GetByteCount(candidate)<=3500)body=candidate;}catch{}
    // JSON first, marker LAST: parent commits preceding bytes before it sees
    // the retention marker and stops its original reader. One write attempt.
    try{Console.Error.Write(body+"\nROOT_VM_OBSERVER_UNKNOWN_RETAINED_NO_REPLAY\n");Console.Error.Flush();}catch{}
   }
   // No retries, finalizer/Dispose, output rewrites, cleanup, close, controller
   // exit or replay after any unresolved native/read/parse/close outcome.
   for(;;)Thread.Sleep(1000);
  }
  static Dictionary<string,object> PinData(FilePin p){
   return D("path",p.Path,"directory",p.Directory,"volume",p.Volume.ToString(System.Globalization.CultureInfo.InvariantCulture),
    "file_id",p.FileId,"bytes",p.Bytes.ToString(System.Globalization.CultureInfo.InvariantCulture),"creation",p.CreationFileTime.ToString(System.Globalization.CultureInfo.InvariantCulture),
    "modified",p.ModifiedFileTime.ToString(System.Globalization.CultureInfo.InvariantCulture),"attributes",p.Attributes,"links",p.Links,"sha256",p.Sha256);
  }
  internal static Dictionary<string,object> D(params object[] values){
   Dictionary<string,object> obj=new Dictionary<string,object>(StringComparer.Ordinal);
   for(int i=0;i<values.Length;i+=2)obj.Add((string)values[i],values[i+1]);return obj;
  }
  static string HashPrefix(byte[] bytes,int count){byte[] copy=new byte[count];Buffer.BlockCopy(bytes,0,copy,0,count);return InputRules1701.Hash(copy);}
 }
#else
 internal sealed class Driver1725 {
  readonly string Mode,Tag,BindingSha,Install,Namespace,CollectorPath;
  readonly Io1725 Io=new Io1725();
  readonly TokenPreflight1725 Preflight=new TokenPreflight1725();
  readonly SharedRound042[] Rounds=new SharedRound042[17];int RoundIndex;
  SharedRound042 CurrentRound {get{return Rounds[RoundIndex];}}
  StepGate1725 Gate {get{return CurrentRound.Gate;}}
  JournalPrefix1725 Prefix {get{return CurrentRound.Prefix;}}
  string EvidencePath {get{return Namespace+"\\case-attempt-"+(RoundIndex+1).ToString("D2")+".evidence";}}
  string JournalPath {get{return EvidencePath+"\\journal.jsonl";}}
  int LineOffset {get{return RoundIndex*92;}}
  readonly byte[] Stdout=new byte[2097152],Stderr=new byte[2097152];
  RoundPrompt042 FirstPrompt {get{return CurrentRound.Prompt;}}
  RootSelection Selection;RootPPrebirth.ControlledSession1701 Session;ControlledObservation1701 Last;
  CaptureObservation Final;int OutLength,ErrLength,Delivered,MainPolls,JournalReadCalls;
  long JournalReadBytes,CriticalStart=-1,ContextStart=-1,StageStart;int StepIndex;bool Finished,InitialAbsent;
  bool DiagnosticAttempted;
  internal void CacheFailure(Exception error){try{Io.CaptureException(error);}catch{}}
  internal Driver1725(string mode,string tag,string binding){
   Mode=mode;Tag=tag;BindingSha=binding;Install=@"C:\Morrow-C28-RootP1701-bin-"+tag;
   Namespace=@"C:\Morrow-C28-A1-047-"+tag;
   CollectorPath=@"C:\Morrow-C28-Root47-1725-"+mode+"-"+tag;
   if(mode=="run")for(int i=0;i<17;i++){Rounds[i]=new SharedRound042(i+1);Rounds[i].Register(Io.Resources);}
  }
  internal int Run(){
   Io.Begin();Io.Open(Io.Root,@"C:\",true,0x80000000,1,3,false);
   if(Mode=="gather"){Gather();Io.NormalCloseAll();return 0;}
   // A missing or known invalid selector can return78 ONLY after actual checked
   // retirement of its new readonly handles; no P/controller output was created.
   bool bound=false;
   try{bound=LoadBinding();}catch{if(Io.Unknown)throw;}
   if(!bound){Io.NormalCloseAll();return 78;}
   // Missing privilege is a known NoP/T result ONLY after the original token,
   // query buffer and all readonly controller inputs have actually retired.
   // Only this new public collector is created for the known-ineligible receipt;
   // no product namespace, P or T is created on this return77 path.
   if(!Preflight.Check(Io)){
    Io.CreateCollector(CollectorPath);
    Io.Event(D("attempt",RoundIndex+1,"event","KNOWN_INELIGIBLE_NO_P_T_OR_PRODUCT_EFFECTS","facts",Preflight.Data(),
     "scope","THIS_CONTROLLER_INVOCATION_BEFORE_BEGIN_CONTROLLED_NOT_GLOBAL_PROCESS_ABSENCE"));
    Io.NormalCloseAll();return 77;
   }
   Io.FreshAbsent(Namespace);for(RoundIndex=0;RoundIndex<17;RoundIndex++){Io.FreshAbsent(EvidencePath);Io.FreshAbsent(JournalPath);}RoundIndex=0;InitialAbsent=true;
   Io.CreateCollector(CollectorPath);
   Io.Event(D("attempt",RoundIndex+1,"event","ROOT_SELECTOR_BOUND_NOT_HUMAN_OR_VM_AUTHORITY","tag",Tag,"binding_sha256",BindingSha));
   Io.Event(D("attempt",RoundIndex+1,"event","READONLY_PRIMARY_TOKEN_PRIVILEGES_PRESENT_NOT_ENABLED_OR_APPROVAL","facts",Preflight.Data()));
   Io.At("BEGIN_CONTROLLED",null);
   Session=RootPPrebirth.BeginControlled(Selection); // same original native caller for every following call
   for(RoundIndex=0;RoundIndex<17;RoundIndex++){
    int baseline=RoundIndex==0?0:checked((int)Last.Input.Frames[LineOffset-1].NativeWriteStartStdout);
    CurrentRound.Begin(Stdout,baseline);CriticalStart=-1;ContextStart=-1;
   for(StepIndex=0;StepIndex<47;StepIndex++){
    Gate.Expect(StepIndex);StageStart=Io.Clock.ElapsedMilliseconds;
    if(StepIndex==19)CriticalStart=StageStart; // before native-review creates the actual30s intent
    if(StepIndex==34)ContextStart=StageStart; // before native-context, never extend its60s lease
    Step1725 step=Gate.Current;
    Io.Event(D("attempt",RoundIndex+1,"event","STEP_EXPECTED_NOT_APPLIED","ordinal",step.Ordinal,"command",step.Command));
    for(int line=LineOffset+step.FirstLine;line<=LineOffset+step.LastLine;line++){
     for(;;){
      Tick();
      if(Last.Review!=null&&Last.Input.Delivered==line){
       // Only first offer needs the actual complete prompt in THIS stdout.
       // stderr, diagnostic records and scalar progress cannot satisfy it.
       if(StepIndex!=0||line!=LineOffset||FirstPrompt.Observe(Stdout,FirstPrompt.CheckedThrough,OutLength))break;
      }
      Thread.Sleep(5);
     }
     Io.Need(Last.NextLineOrdinal==line+1&&Last.ExpectedLine==InputProtocol1701.Text(line),"SAME_INSTANCE_NEXT_LINE");
     Session.OfferReviewedLine(Last.Review,InputProtocol1701.Text(line)); // one line, never a batch
     do{Tick();if(Delivered<line+1)Thread.Sleep(5);}while(Delivered<line+1);
    }
    while(!step.Returned){Tick();if(!step.Returned)Thread.Sleep(5);}
    Io.Need(Gate.Completed==StepIndex+1&&Delivered==LineOffset+step.LastLine+1,"STAGE_NOT_CLOSED");
    Io.Event(D("attempt",RoundIndex+1,"event","STEP_RETURNED_NOT_NINE_GATE_ACCEPTANCE","ordinal",step.Ordinal,"command",step.Command,
     "reservation_sequence",step.ReservationSequence.HasValue?(object)step.ReservationSequence.Value:null,
     "return_sequence",step.ReturnSequence.Value,"delivered",Delivered,"stdout_through",OutLength,"stderr_through",ErrLength,
     "stdout_prefix_sha256",HashPrefix(Stdout,OutLength),"stderr_prefix_sha256",HashPrefix(Stderr,ErrLength)));
   }
    StepIndex=46;
    while(!CurrentRound.AttemptClosed||(RoundIndex==16&&!CurrentRound.CommonOwnerClosed)){Tick();Thread.Sleep(5);}
    CurrentRound.Complete();WriteRound();
   }
   RoundIndex=16;
   Io.Need(Delivered==InputProtocol1701.Count&&Gate.Completed==47&&Last.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"CURRENT17_TRANSPORT_OR_GATE_INCOMPLETE");
   Final=Session.FinishAfterDeliveredExit();ValidateFinal(Final);
   // Drain only already stored helper prefix; its closed Observe path does not
   // query any closed original handle. Journal is still OUR original read handle.
   for(;;){
    Last=Session.Observe(512);AppendOutput(Last);
    Io.Need(Last.Capture.Status=="CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY"&&Last.Review==null,"CLOSED_DATA_ONLY");
    if(Last.Stdout.Length==0&&Last.Stderr.Length==0)break;
   }
   Io.Need(Final.StdoutBytes==OutLength&&Final.StderrBytes==ErrLength
    &&Final.StdoutSha256==HashPrefix(Stdout,OutLength)&&Final.StderrSha256==HashPrefix(Stderr,ErrLength),"ORIGINAL_FULL_STREAMS_MATCH_HELD_LOGS");
   PollJournal();Io.Need(Prefix.Parsed==Prefix.Length&&Gate.Completed==47,"FINAL_PARTIAL_JOURNAL");
   Io.Event(D("attempt",RoundIndex+1,"event","ORIGINAL_HELPER_CLOSED_NOT_FULL_PRODUCT_ACCEPTANCE","attempts",17,"steps",799,"lines",1564,"bytes",InputProtocol1701.NormalBytes,
    "journal_bytes",Prefix.Length,"journal_records",Prefix.Records,"stdout_sha256",HashPrefix(Stdout,OutLength),"stderr_sha256",HashPrefix(Stderr,ErrLength)));
   WriteTerminal();Io.NormalCloseAll();Finished=true;
   // Exit0 follows checked-close of all controller-owned original resources.
   // The terminal file explicitly does NOT pretend to contain its own later close witness.
   return 0;
  }
  bool LoadBinding(){
   string path=@"C:\Morrow-C28-Root47-1725-binding-"+Tag+".json";
   if(!Io.Open(Io.Binding,path,false,0x80000000,1,3,true))return false;
   Io.Need(Io.Binding.First.Bytes>0&&Io.Binding.First.Bytes<=131072,"BINDING_BYTES");
   string actual=Io.Hash(Io.Binding,Io.Binding.First.Bytes,false);if(actual!=BindingSha)throw new InvalidOperationException("ROOT_BINDING_BYTES_SHA");
   Io.Seek(Io.Binding,0);int at=0,wanted=(int)Io.Binding.First.Bytes;
   while(at<wanted){int n=Io.Read(Io.Binding,Io.Scratch,Math.Min(Io.Scratch.Length,wanted-at));Io.Need(n>0,"BINDING_SHORT_READ");Buffer.BlockCopy(Io.Scratch,0,Io.BindingBytes,at,n);at+=n;}
   Io.Need(FileMetadataRules1718.Same(Io.Binding.First,Io.Identity(Io.Binding,path,false)),"BINDING_HELD_STABILITY");
   Io.At("BINDING_PARSE",null);
   string text=new UTF8Encoding(false,true).GetString(Io.BindingBytes,0,wanted);
   Dictionary<string,object> root=Json1725.Obj(Json1725.Parse(text));
   if(root.Count!=7||Json1725.Str(Json1725.Get(root,"schema"))!="ROOT47_BINDING1725_V1"
    ||Json1725.Str(Json1725.Get(root,"tag"))!=Tag||Json1725.Str(Json1725.Get(root,"install"))!=Install
    ||Json1725.Str(Json1725.Get(root,"inventory"))!=RootSelection.FrozenInventory
    ||Json1725.Str(Json1725.Get(root,"shell_sha256"))!=RolePins1725.ShellSha)throw new InvalidOperationException("UNBOUND_SELECTOR");
   ulong deadline=Json1725.UInt(Json1725.Get(root,"deadline_ms"));if(deadline<1||deadline>5500000)throw new InvalidOperationException("SELECTOR_DEADLINE");
   List<object> rows=Json1725.Get(root,"pins") as List<object>;if(rows==null||rows.Count<1||rows.Count>40)throw new InvalidOperationException("SELECTOR_PIN_COUNT");
   FilePin[] pins=new FilePin[rows.Count];
   for(int i=0;i<pins.Length;i++){
    Dictionary<string,object> r=Json1725.Obj(rows[i]);if(r.Count!=10)throw new InvalidOperationException("PIN_KEYS");
    pins[i]=new FilePin(Json1725.Str(Json1725.Get(r,"path")),Json1725.Bool(Json1725.Get(r,"directory")),
     Json1725.UInt(Json1725.Get(r,"volume")),Json1725.Str(Json1725.Get(r,"file_id")),Json1725.UInt(Json1725.Get(r,"bytes")),
     Json1725.UInt(Json1725.Get(r,"creation")),Json1725.UInt(Json1725.Get(r,"modified")),
     checked((uint)Json1725.UInt(Json1725.Get(r,"attributes"))),checked((uint)Json1725.UInt(Json1725.Get(r,"links"))),
     Json1725.Get(r,"sha256")==null?null:Json1725.Str(Json1725.Get(r,"sha256")));
   }
   Io.At("BINDING_SELECTOR_VALIDATE",null);
   Selection=new RootSelection(Install,Tag,RootSelection.FrozenInventory,(int)deadline,pins);RolePins1725.Check(Selection);return true;
  }
  void Gather(){
   Io.At("GATHER_BEGIN",null);
   Io.CreateCollector(CollectorPath);
   RootSelection provisional=new RootSelection(Install,Tag,RootSelection.FrozenInventory,5500000,new FilePin[0]);
   List<string> paths=provisional.Paths();FilePin[] pins=new FilePin[paths.Count];
   for(int i=0;i<paths.Count;i++){
    string path=paths[i];bool file=path==RootPPrebirth.Shell;
    foreach(string name in RootPPrebirth.RoleNames)file|=path==Install+"\\"+name;
    Io.Open(Io.Pins[i],path,!file,0x80000000,1,3,false);
    FilePin observed=Io.Pins[i].First;string digest=file?Io.Hash(Io.Pins[i],observed.Bytes,true):null;
    Io.Need(FileMetadataRules1718.Same(observed,Io.Identity(Io.Pins[i],path,!file)),"GATHER_STABILITY");
    pins[i]=FileMetadataRules1718.WithDigest(observed,digest);
   }
   Io.At("GATHER_SELECTOR_VALIDATE",null);
   Selection=new RootSelection(Install,Tag,RootSelection.FrozenInventory,5500000,pins);RolePins1725.Check(Selection);
   List<object> data=new List<object>();foreach(FilePin pin in pins)data.Add(PinData(pin));
   Dictionary<string,object> binding=D("schema","ROOT47_BINDING1725_V1","tag",Tag,"install",Install,"inventory",RootSelection.FrozenInventory,
    "deadline_ms",5500000,"shell_sha256",RolePins1725.ShellSha,"pins",data);
   Io.Open(Io.Gather,CollectorPath+@"\gathered-binding.json",false,0x40000000,1,1,false);
   Io.Write(Io.Gather,new UTF8Encoding(false,true).GetBytes(Json1725.Encode(binding)+"\n"),131073);
   Io.Need(RootPPrebirth.Native.FlushFileBuffers(Io.Gather.Handle.Raw),"GATHER_FLUSH");
   Io.Event(D("attempt",RoundIndex+1,"event","GATHER_ONLY_NO_P_NO_APPROVAL","pins",pins.Length));
  }
  void Tick(){
   Io.Check();Io.Need(++MainPolls<=1200000,"MAIN_POLL_CAP");
   long elapsed=Io.Clock.ElapsedMilliseconds;
   long stageLimit=StepIndex==3?600000:120000;
   Io.Need(elapsed-StageStart<stageLimit,"STAGE_BUDGET");
   if(CriticalStart>=0&&StepIndex<=40)Io.Need(elapsed-CriticalStart<29000,"NATIVE_INTENT_CONSERVATIVE29S");
   if(ContextStart>=0&&StepIndex<=40)Io.Need(elapsed-ContextStart<59000,"NATIVE_CONTEXT_CONSERVATIVE59S");
   Last=Session.Observe(512);
   Io.Need(Last.Capture.Status=="CONTROLLED_INPUT_PENDING"&&Last.Capture.FirstError==null&&Last.Capture.CreationFileTime.HasValue,"HELPER_UNKNOWN_OR_STARTUP");
   Io.Need(!Last.Capture.ProcessDword.HasValue||(RoundIndex==16&&StepIndex==46&&Last.Input.Delivered==InputProtocol1701.Count&&Last.Capture.ProcessDword==0),"P_EXIT_BEFORE_EXPLICIT_FLOW");
   AppendOutput(Last);ObserveDelivery(Last.Input);PollJournal();
  }
  void AppendOutput(ControlledObservation1701 view){
   Io.Need(view.Stdout!=null&&view.Stderr!=null&&view.Stdout.Length<=512&&view.Stderr.Length<=512
    &&view.StdoutThrough==OutLength+view.Stdout.Length&&view.StderrThrough==ErrLength+view.Stderr.Length
    &&view.StdoutThrough<=Stdout.Length&&view.StderrThrough<=Stderr.Length,"HELPER_PREFIX_CURSOR");
   Buffer.BlockCopy(view.Stdout,0,Stdout,OutLength,view.Stdout.Length);OutLength+=view.Stdout.Length;
   Buffer.BlockCopy(view.Stderr,0,Stderr,ErrLength,view.Stderr.Length);ErrLength+=view.Stderr.Length;
  }
  void ObserveDelivery(InputObservation1701 input){
   Io.Need(input!=null&&input.State!="Poisoned"&&input.Delivered>=Delivered&&input.Delivered<=Delivered+1
    &&input.Delivered<=InputProtocol1701.Count&&input.NativeReturns==input.Delivered,"INPUT_NATIVE_RETURN_NOT_PENDING_SUCCESS");
   if(input.Delivered==Delivered)return;
   InputFrameObservation1701 f=input.Frames[Delivered];byte[] wire=InputProtocol1701.Wire(Delivered);
   Io.Need(f.Ordinal==Delivered+1&&f.NativeReturned==true&&f.RawWritten==(uint)wire.Length&&f.WireBytes==wire.Length&&f.Sha256==InputRules1701.Hash(wire),"REAL_LINE_DELIVERY");
   Delivered++;
   Io.Event(D("attempt",RoundIndex+1,"event","ONE_ORIGINAL_LINE_DELIVERED_NOT_STAGE_SUCCESS","line",Delivered,"step",InputProtocol1701.Step(Delivered-1),
    "wire_bytes",f.WireBytes,"wire_sha256",f.Sha256,"stdout_prefix",f.ReviewedStdout,"stderr_prefix",f.ReviewedStderr));
  }
  void PollJournal(){
   Io.Check();Io.Need(InitialAbsent,"NO_INITIAL_FRESH_ABSENCE");
   if(CurrentRound.Journal.Handle.State!=ResourceState.Open){
    Io.Need(++Io.OpenJournalAttempts<=1200000,"JOURNAL_OPEN_PROBE_CAP");
    if(Io.JournalNamespace.Handle.State!=ResourceState.Open&&!Io.Open(Io.JournalNamespace,Namespace,true,0x80000000,1,3,true))return;
    if(CurrentRound.Evidence.Handle.State!=ResourceState.Open&&!Io.Open(CurrentRound.Evidence,EvidencePath,true,0x80000000,1,3,true))return;
    if(!Io.Open(CurrentRound.Journal,JournalPath,false,0x80000000,3,3,true))return;
    Io.Need(Last!=null&&Last.Capture.CreationFileTime.HasValue&&CurrentRound.Journal.First.CreationFileTime>=Last.Capture.CreationFileTime.Value,"JOURNAL_CREATED_AFTER_ORIGINAL_P");
   }
   FilePin before=Io.Identity(CurrentRound.Journal,JournalPath,false);StableJournal(before);
   Io.Need(before.Bytes>=(ulong)Prefix.Length&&before.Bytes<=33554432,"JOURNAL_LENGTH");
   int old=Prefix.Length,length=(int)before.Bytes,offset=0;Io.Seek(CurrentRound.Journal,0);
   while(offset<length){
    Io.Need(++JournalReadCalls<=1000000,"JOURNAL_READ_CALL_CAP");
    int n=Io.Read(CurrentRound.Journal,Io.Scratch,Math.Min(Io.Scratch.Length,length-offset));Io.Need(n>0,"JOURNAL_SHORT_SNAPSHOT");
    JournalReadBytes+=n;Io.Need(JournalReadBytes<=8589934592L,"JOURNAL_REREAD_BYTE_CAP");
    Prefix.CompareAndStore(offset,Io.Scratch,n,old);offset+=n;
   }
   FilePin after=Io.Identity(CurrentRound.Journal,JournalPath,false);StableJournal(after);Io.Need(after.Bytes>=(ulong)length,"JOURNAL_SHRANK_DURING_READ");
   foreach(JournalRecord1725 record in Prefix.Commit(length)){
    Gate.Accept(record);CurrentRound.Accept(record);
    Io.Event(D("attempt",RoundIndex+1,"event","ACTUAL_JOURNAL_RECORD","sequence",record.Sequence,"kind",record.Kind,"start",record.Start,"through",record.End,"sha256",record.Sha256));
   }
  }
  void StableJournal(FilePin value){
   FilePin first=CurrentRound.Journal.First;
   Io.Need(!value.Directory&&value.Path==first.Path&&value.Volume==first.Volume&&value.FileId==first.FileId
    &&value.CreationFileTime==first.CreationFileTime&&value.Attributes==first.Attributes&&value.Links==1,"ORIGINAL_JOURNAL_IDENTITY");
  }
  void ValidateFinal(CaptureObservation c){
   Io.Need(c.Status=="CLOSED_ORIGINAL_P_AND_OUTER_IO_ONLY"&&c.FirstError==null&&c.ProcessDword==0&&c.PrimaryThreadDword==0
    &&c.OutReaderDword==0&&c.ErrReaderDword==0&&c.StdoutEof&&c.StderrEof&&c.OriginalReadersSignaled&&c.CreatorWindowObserved
    &&c.Input!=null&&c.Input.NativeThreadSignaled&&c.Input.NativeThreadDword==0&&c.Input.State=="Ended"
    &&c.Input.Delivered==InputProtocol1701.Count&&c.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"ORIGINAL_FINAL_NATIVE_FACTS");
   Io.Need(c.CreatorNativeThreadId==Io.Creator&&c.Pid.HasValue&&c.Pid.Value!=0&&c.PrimaryThreadId.HasValue&&c.PrimaryThreadId.Value!=0
    &&c.CreationFileTime.HasValue&&c.CreationFileTime.Value!=0&&c.PrimaryCreationFileTime.HasValue&&c.PrimaryCreationFileTime.Value!=0
    &&c.OutReaderCreationFileTime.HasValue&&c.OutReaderCreationFileTime.Value!=0&&c.ErrReaderCreationFileTime.HasValue&&c.ErrReaderCreationFileTime.Value!=0
    &&c.Input.NativeThreadId.HasValue&&c.Input.NativeThreadId.Value!=0&&c.Input.NativeThreadCreationFileTime.HasValue&&c.Input.NativeThreadCreationFileTime.Value!=0,"SAME_ORIGINAL_NATIVE_IDENTITIES");
   foreach(SlotObservation slot in c.Slots)Io.Need(slot.State==ResourceState.ClosedVerified||slot.State==ResourceState.ProvenAbsent||slot.State==ResourceState.MemoryRetired,"HELPER_ALIAS_NOT_RETIRED");
   Io.Need(c.OriginalFilePins.Length==Selection.Pins.Length&&c.FinalFilePins.Length==Selection.Pins.Length,"FINAL_PIN_COUNT");
   for(int i=0;i<Selection.Pins.Length;i++){
    Io.Need(FileMetadataRules1718.Same(c.OriginalFilePins[i],c.FinalFilePins[i]),"FINAL_STABLE_PINS");
    if(!Selection.Pins[i].Directory)Io.Need(c.FinalFilePins[i].Sha256==Selection.Pins[i].Sha256,"FINAL_PE_FULL_SHA");
   }
  }
  List<object> RoundSteps(){
   var steps=new List<object>();foreach(Step1725 step in Gate.Steps)steps.Add(D("ordinal",step.Ordinal,"command",step.Command,
    "reservation",step.ReservationSequence.HasValue?(object)step.ReservationSequence.Value:null,"returned",step.ReturnSequence.Value));return steps;
  }
  void WriteRound(){
   Io.Need(CurrentRound.Completed&&CurrentRound.ReportSummary==null,"ROUND_REPORT_ONCE_AFTER_ACTUAL_CLOSURE");
   var frames=new List<object>();
   for(int i=LineOffset;i<LineOffset+92;i++){
    InputFrameObservation1701 f=Last.Input.Frames[i];byte[] expected=InputProtocol1701.Wire(i);
    Io.Need(f.Ordinal==i+1&&f.State=="Delivered"&&f.NativeReturned==true&&f.Error==0&&f.RawWritten==expected.Length&&f.Sha256==InputRules1701.Hash(expected),"ROUND_ORIGINAL_FRAME");
    frames.Add(D("ordinal",f.Ordinal,"wire_bytes",f.WireBytes,"sha256",f.Sha256,"state",f.State,"native_returned",f.NativeReturned,"raw_written",f.RawWritten,"error",f.Error,
     "reviewed_stdout",f.ReviewedStdout,"reviewed_stderr",f.ReviewedStderr,"write_start_stdout",f.NativeWriteStartStdout,"write_start_stderr",f.NativeWriteStartStderr));
   }
   string reportPath=CollectorPath+"\\attempt-"+(RoundIndex+1).ToString("D2")+".json";
   var body=D("schema","ORIGINAL_SHARED_PARENT_ROUND042_V1","attempt",RoundIndex+1,
    "steps",RoundSteps(),"first_line",LineOffset+1,"last_line",LineOffset+92,"input_bytes",3557,"frames",frames,
    "journal_path",JournalPath,"journal_bytes",Prefix.Length,"journal_sha256",HashPrefix(Prefix.Bytes,Prefix.Length),"journal_records",Prefix.Records,
    "original_attempt_closed_sequence",CurrentRound.AttemptSequence,"original_common_owner_closed_sequence",CurrentRound.CommonSequence,
    "prompt_native_write_baseline",CurrentRound.PromptBaseline,"same_original_parent_pid",Last.Capture.Pid,
    "same_original_parent_creation",Last.Capture.CreationFileTime,"parent_lifetime_closed",false,"source_accepted",false,"release_eligible",false);
   byte[] raw=new UTF8Encoding(false,true).GetBytes(Json1725.Encode(body)+"\n");
   Io.Open(CurrentRound.Report,reportPath,false,0x40000000,1,1,false);Io.Write(CurrentRound.Report,raw,131073);
   Io.Need(RootPPrebirth.Native.FlushFileBuffers(CurrentRound.Report.Handle.Raw),"ROUND_REPORT_FLUSH");
   CurrentRound.ReportSummary=D("attempt",RoundIndex+1,"path",reportPath,"bytes",raw.Length,"sha256",InputRules1701.Hash(raw),
    "journal_path",JournalPath,"journal_bytes",Prefix.Length,"journal_sha256",HashPrefix(Prefix.Bytes,Prefix.Length),"journal_records",Prefix.Records,
    "first_line",LineOffset+1,"last_line",LineOffset+92,"original_attempt_closed_sequence",CurrentRound.AttemptSequence,
    "original_common_owner_closed_sequence",CurrentRound.CommonSequence);
  }
  void WriteTerminal(){
   var rounds=new List<object>();foreach(SharedRound042 round in Rounds){
    Io.Need(round.Completed&&!round.Failed&&round.ReportSummary!=null,"ALL_SEVENTEEN_ORIGINAL_ROUNDS_CLOSED");rounds.Add(round.ReportSummary);
   }
   // All earlier journals remain pinned and append-only. Re-read each original
   // handle after final P0, rejecting any late record or rewrite since its report.
   for(RoundIndex=0;RoundIndex<17;RoundIndex++){
    PollJournal();Io.Need(Prefix.Parsed==Prefix.Length&&HashPrefix(Prefix.Bytes,Prefix.Length)==Json1725.Str(CurrentRound.ReportSummary["journal_sha256"]),"FINAL_ORIGINAL_ROUND_JOURNAL_STABLE");
   }
   RoundIndex=16;
   Io.Open(Io.Terminal,CollectorPath+@"\terminal.json",false,0x40000000,1,1,false);
   Io.Write(Io.Terminal,new UTF8Encoding(false,true).GetBytes(Json1725.Encode(D("status",InputProtocol1701.TerminalStatus,
    "attempts",17,"steps_total",799,"rounds",rounds,"delivered",Delivered,"input_bytes",InputProtocol1701.NormalBytes,
    "stdout_bytes",OutLength,"stderr_bytes",ErrLength,"stdout_sha256",HashPrefix(Stdout,OutLength),"stderr_sha256",HashPrefix(Stderr,ErrLength),
    "primary_token_preflight",Preflight.Data(),"original_helper",FinalData(),"controller_resources_before_own_close",Io.ResourceData(),
    "self_close_witness","NOT_IN_THIS_PRE_CLOSE_FILE_ROOT_MUST_VERIFY_ORIGINAL_CONTROLLER_EXIT0",
    "source_accepted",false,"release_eligible",false,"SDK26_G04_SDK_FREEZE","OPEN"))+"\n"),131073);
   Io.Need(RootPPrebirth.Native.FlushFileBuffers(Io.Terminal.Handle.Raw),"TERMINAL_FLUSH");
  }
  Dictionary<string,object> FinalData(){
   List<object> slots=new List<object>(),queries=new List<object>(),frames=new List<object>(),initial=new List<object>(),last=new List<object>();
   foreach(SlotObservation s in Final.Slots)slots.Add(D("name",s.Name,"state",s.State.ToString(),"original_value_captured",s.OriginalValueCaptured,"api_returned",s.ApiReturned,"error",s.Error));
   foreach(DwordObservation q in Final.DwordQueries)queries.Add(D("name",q.Name,"raw_u32",q.RawValue,"returned",q.Returned,"error",q.Error,"after_original_signal",q.AfterOriginalSignal));
   foreach(SharedRound042 round in Rounds)frames.Add(round.ReportSummary);
   foreach(FilePin pin in Final.OriginalFilePins)initial.Add(PinData(pin));foreach(FilePin pin in Final.FinalFilePins)last.Add(PinData(pin));
   return D("scope",Final.Scope,"status",Final.Status,"first_error",Final.FirstError,"no_replay",Final.NoReplay,
    "creator_native_thread_id",Final.CreatorNativeThreadId,"pid",Final.Pid,"primary_thread_id",Final.PrimaryThreadId,
    "process_creation_filetime",Final.CreationFileTime,"primary_creation_filetime",Final.PrimaryCreationFileTime,
    "out_reader_creation_filetime",Final.OutReaderCreationFileTime,"err_reader_creation_filetime",Final.ErrReaderCreationFileTime,
    "process_dword",Final.ProcessDword,"primary_dword",Final.PrimaryThreadDword,"out_reader_dword",Final.OutReaderDword,"err_reader_dword",Final.ErrReaderDword,
    "stdout_eof",Final.StdoutEof,"stderr_eof",Final.StderrEof,"original_readers_signaled",Final.OriginalReadersSignaled,"creator_window_observed",Final.CreatorWindowObserved,
    "create_attempted",Final.CreateWasAttempted,"create_returned",Final.CreateReturned,"create_success",Final.CreateSuccess,"create_error",Final.CreateError,"resume_return",Final.ResumeReturn,
    "stdout_bytes",Final.StdoutBytes,"stderr_bytes",Final.StderrBytes,"stdout_sha256",Final.StdoutSha256,"stderr_sha256",Final.StderrSha256,
    "input",D("state",Final.Input.State,"reserved",Final.Input.Reserved,"write_attempts",Final.Input.WriteAttempts,"native_returns",Final.Input.NativeReturns,"delivered",Final.Input.Delivered,
     "requested_bytes",Final.Input.RequestedBytes,"reported_successful_bytes",Final.Input.ReportedSuccessfulBytes,"delivered_bytes",Final.Input.DeliveredBytes,
     "native_thread_id",Final.Input.NativeThreadId,"native_thread_creation_filetime",Final.Input.NativeThreadCreationFileTime,"native_thread_dword",Final.Input.NativeThreadDword,"native_thread_signaled",Final.Input.NativeThreadSignaled,"round_reports",frames),
    "dword_queries",queries,"slots",slots,"original_file_pins",initial,"final_file_pins",last,
    "business_child_native_facts","NOT_PROVEN_BY_ROOT_P_HELPER","nine_gate_qualification","NOT_GRANTED");
  }
  internal void KeepUnknown(){
   Io.Fail("ROOT_CONTROLLER_UNCERTAINTY_RETAINED");
   if(Session!=null){try{Session.WithholdFurtherInput();}catch{}}
   if(!DiagnosticAttempted){
    DiagnosticAttempted=true;string body="{\"schema\":\"ROOT_CACHED_FAILURE1832_V1\",\"diagnostic\":\"UNAVAILABLE\"}";
    try{string candidate=Json1725.Encode(Io.DiagnosticData(Mode));if(Encoding.UTF8.GetByteCount(candidate)<=3500)body=candidate;}catch{}
    // JSON first, marker LAST: parent commits preceding bytes before it sees
    // the retention marker and stops its original reader. One write attempt.
    try{Console.Error.Write(body+"\nROOT_VM_OBSERVER_UNKNOWN_RETAINED_NO_REPLAY\n");Console.Error.Flush();}catch{}
   }
   // No retries, finalizer/Dispose, output rewrites, cleanup, close, controller
   // exit or replay after any unresolved native/read/parse/close outcome.
   for(;;)Thread.Sleep(1000);
  }
  static Dictionary<string,object> PinData(FilePin p){
   return D("path",p.Path,"directory",p.Directory,"volume",p.Volume.ToString(System.Globalization.CultureInfo.InvariantCulture),
    "file_id",p.FileId,"bytes",p.Bytes.ToString(System.Globalization.CultureInfo.InvariantCulture),"creation",p.CreationFileTime.ToString(System.Globalization.CultureInfo.InvariantCulture),
    "modified",p.ModifiedFileTime.ToString(System.Globalization.CultureInfo.InvariantCulture),"attributes",p.Attributes,"links",p.Links,"sha256",p.Sha256);
  }
  internal static Dictionary<string,object> D(params object[] values){
   Dictionary<string,object> obj=new Dictionary<string,object>(StringComparer.Ordinal);
   for(int i=0;i<values.Length;i+=2)obj.Add((string)values[i],values[i+1]);return obj;
  }
  static string HashPrefix(byte[] bytes,int count){byte[] copy=new byte[count];Buffer.BlockCopy(bytes,0,copy,0,count);return InputRules1701.Hash(copy);}
 }
#endif
}
