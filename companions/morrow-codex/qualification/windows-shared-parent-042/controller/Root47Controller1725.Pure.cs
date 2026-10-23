// SOURCE1725 pure JSON, journal and stage decisions used by the actual controller.
// No native calls, time queries, VM/approval claims or automatic execution.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
namespace C28.RootP1682 {
 internal sealed class JsonNumber1725 {internal readonly string Raw;internal JsonNumber1725(string raw){Raw=raw;}}
 internal static class Json1725 {
  internal static object Parse(string text){return new Parser(text).All();}
  sealed class Parser {
   readonly string text;int at,nodes;
   internal Parser(string value){if(value==null||value.Length>131072)throw new InvalidOperationException("JSON_CAP");text=value;}
   void Need(bool b){if(!b)throw new InvalidOperationException("STRICT_JSON");}
   void Space(){while(at<text.Length&&(text[at]==' '||text[at]=='\n'||text[at]=='\r'||text[at]=='\t'))at++;}
   internal object All(){object v=Value(0);Space();Need(at==text.Length);return v;}
   object Value(int depth){
    Need(depth<=32&&++nodes<=8192);Space();Need(at<text.Length);char c=text[at];
    if(c=='{'){
     at++;Dictionary<string,object> obj=new Dictionary<string,object>(StringComparer.Ordinal);
     HashSet<string> keys=new HashSet<string>(StringComparer.OrdinalIgnoreCase);
     Space();if(at<text.Length&&text[at]=='}'){at++;return obj;}
     for(;;){Space();string key=String();Need(keys.Add(key));Space();Need(at<text.Length&&text[at++]==':');obj.Add(key,Value(depth+1));Space();
      Need(at<text.Length);char end=text[at++];if(end=='}')return obj;Need(end==',');}
    }
    if(c=='['){
     at++;List<object> list=new List<object>();Space();if(at<text.Length&&text[at]==']'){at++;return list;}
     for(;;){Need(list.Count<1024);list.Add(Value(depth+1));Space();Need(at<text.Length);char end=text[at++];if(end==']')return list;Need(end==',');}
    }
    if(c=='"')return String();
    foreach(string token in new string[]{"true","false","null"}){
     if(at+token.Length<=text.Length&&System.String.CompareOrdinal(text,at,token,0,token.Length)==0){at+=token.Length;return token=="null"?null:(object)(token=="true");}
    }
    int begin=at;if(text[at]=='-')at++;Need(at<text.Length);
    if(text[at]=='0')at++;else{Need(text[at]>='1'&&text[at]<='9');while(at<text.Length&&text[at]>='0'&&text[at]<='9')at++;}
    if(at<text.Length&&text[at]=='.'){at++;int start=at;while(at<text.Length&&text[at]>='0'&&text[at]<='9')at++;Need(at>start);}
    if(at<text.Length&&(text[at]=='e'||text[at]=='E')){at++;if(at<text.Length&&(text[at]=='+'||text[at]=='-'))at++;int start=at;while(at<text.Length&&text[at]>='0'&&text[at]<='9')at++;Need(at>start);}
    return new JsonNumber1725(text.Substring(begin,at-begin));
   }
   string String(){
    Need(at<text.Length&&text[at++]=='"');StringBuilder b=new StringBuilder();
    while(at<text.Length){
     char c=text[at++];if(c=='"')return b.ToString();Need(c>=32);
     if(c!='\\'){b.Append(c);continue;}Need(at<text.Length);c=text[at++];
     if(c=='"'||c=='\\'||c=='/')b.Append(c);
     else if(c=='b')b.Append('\b');else if(c=='f')b.Append('\f');else if(c=='n')b.Append('\n');else if(c=='r')b.Append('\r');else if(c=='t')b.Append('\t');
     else if(c=='u'){
      Need(at<=text.Length-4);ushort code;Need(UInt16.TryParse(text.Substring(at,4),NumberStyles.AllowHexSpecifier,CultureInfo.InvariantCulture,out code));at+=4;
      if(code>=0xd800&&code<=0xdbff){
       Need(at<=text.Length-6&&text[at]=='\\'&&text[at+1]=='u');at+=2;ushort low;
       Need(UInt16.TryParse(text.Substring(at,4),NumberStyles.AllowHexSpecifier,CultureInfo.InvariantCulture,out low)&&low>=0xdc00&&low<=0xdfff);at+=4;b.Append((char)code);b.Append((char)low);
      }else{Need(code<0xdc00||code>0xdfff);b.Append((char)code);}
     }else Need(false);
    }
    throw new InvalidOperationException("JSON_UNTERMINATED");
   }
  }
  internal static Dictionary<string,object> Obj(object value){Dictionary<string,object> v=value as Dictionary<string,object>;if(v==null)throw new InvalidOperationException("JSON_OBJECT");return v;}
  internal static object Get(Dictionary<string,object> obj,string key){object v;if(!obj.TryGetValue(key,out v))throw new InvalidOperationException("JSON_FIELD");return v;}
  internal static string Str(object value){string s=value as string;if(s==null)throw new InvalidOperationException("JSON_STRING");return s;}
  internal static ulong UInt(object value){
   JsonNumber1725 n=value as JsonNumber1725;string s=n==null?value as string:n.Raw;ulong u;
   if(s==null||s.Length<1||s.Length>20||(s.Length>1&&s[0]=='0'))throw new InvalidOperationException("JSON_UINT");
   foreach(char c in s)if(c<'0'||c>'9')throw new InvalidOperationException("JSON_UINT");
   if(!UInt64.TryParse(s,NumberStyles.None,CultureInfo.InvariantCulture,out u))throw new InvalidOperationException("JSON_UINT");return u;
  }
  internal static ulong UIntNumber(object value){if(!(value is JsonNumber1725))throw new InvalidOperationException("JSON_NUMBER_REQUIRED");return UInt(value);}
  internal static bool Bool(object value){if(!(value is bool))throw new InvalidOperationException("JSON_BOOL");return (bool)value;}
  // Bounded ROOT reports use structured primitive DATA, never exception bodies.
  internal static string Encode(object value){StringBuilder b=new StringBuilder();Write(b,value,0);if(b.Length>131072)throw new InvalidOperationException("REPORT_CAP");return b.ToString();}
  static void Quote(StringBuilder b,string s){b.Append('"');foreach(char c in s){if(c=='"'||c=='\\'){b.Append('\\');b.Append(c);}else if(c<32)b.Append("\\u"+((int)c).ToString("x4"));else b.Append(c);}b.Append('"');}
  static void Write(StringBuilder b,object v,int depth){
   if(depth>32)throw new InvalidOperationException("REPORT_DEPTH");if(v==null){b.Append("null");return;}
   if(v is string){Quote(b,(string)v);return;}if(v is bool){b.Append((bool)v?"true":"false");return;}
   if(v is JsonNumber1725){b.Append(((JsonNumber1725)v).Raw);return;}
   if(v is int||v is uint||v is long||v is ulong){b.Append(Convert.ToString(v,CultureInfo.InvariantCulture));return;}
   Dictionary<string,object> d=v as Dictionary<string,object>;
   if(d!=null){b.Append('{');bool first=true;foreach(KeyValuePair<string,object> p in d){if(!first)b.Append(',');first=false;Quote(b,p.Key);b.Append(':');Write(b,p.Value,depth+1);}b.Append('}');return;}
   System.Collections.IEnumerable e=v as System.Collections.IEnumerable;
   if(e!=null){b.Append('[');bool first=true;foreach(object o in e){if(!first)b.Append(',');first=false;Write(b,o,depth+1);}b.Append(']');return;}
   throw new InvalidOperationException("REPORT_TYPE");
  }
 }
 internal sealed class Step1725 {
  internal readonly int Ordinal,FirstLine,LastLine;internal readonly string Command;
  internal int? ReservationSequence,ReturnSequence;internal bool Returned;
  internal Step1725(int ordinal,int first,int last,string command){Ordinal=ordinal;FirstLine=first;LastLine=last;Command=command;}
 }
 internal static class Plan1725 {
  internal const string Firmware="b2dd7c5f-88ee-4d69-8aeb-d0290e60950e";
  internal static Step1725[] NewSteps(){
   Step1725[] steps=new Step1725[47];int first=0;
   for(int ordinal=1;ordinal<=47;ordinal++){
    int last=first;while(last+1<InputProtocol1701.Count&&InputProtocol1701.Step(last+1)==ordinal)last++;
    string command=ordinal==1?"begin":ordinal==47?"exit":InputProtocol1701.Text(first);
    steps[ordinal-1]=new Step1725(ordinal,first,last,command);first=last+1;
   }
   if(first!=92)throw new InvalidOperationException("CURRENT_PLAN_SHAPE");return steps;
  }
 }
 internal sealed class JournalRecord1725 {
  internal int Sequence,Start,End;internal string Kind,Sha256;internal object Payload;
 }
 internal sealed class JournalPrefix1725 {
  internal readonly byte[] Bytes=new byte[33554432];
  internal int Length,Parsed,Records;internal bool Failed;internal readonly JournalRecord1725[] Rows=new JournalRecord1725[256];
  readonly UTF8Encoding utf8=new UTF8Encoding(false,true);
  internal void CompareAndStore(int offset,byte[] block,int count,int oldLength) {
   if(Failed)throw new InvalidOperationException("JOURNAL_ALREADY_FAILED");
   if(offset<0||count<0||count>block.Length||offset>Bytes.Length-count||oldLength!=Length)throw new InvalidOperationException("JOURNAL_PREFIX_RANGE");
   int compare=Math.Min(count,Math.Max(0,oldLength-offset));
   for(int i=0;i<compare;i++)if(Bytes[offset+i]!=block[i]){Failed=true;throw new InvalidOperationException("JOURNAL_PREFIX_REWRITE");}
   int copy=count-compare;if(copy>0)Buffer.BlockCopy(block,compare,Bytes,offset+compare,copy);
  }
  internal List<JournalRecord1725> Commit(int length) {
   if(Failed)throw new InvalidOperationException("JOURNAL_ALREADY_FAILED");
   try{return CommitCore(length);}catch{Failed=true;throw;}
  }
  List<JournalRecord1725> CommitCore(int length) {
   if(length<Length||length>Bytes.Length)throw new InvalidOperationException("JOURNAL_SHRINK_OR_CAP");Length=length;
   List<JournalRecord1725> added=new List<JournalRecord1725>();
   while(Parsed<Length){
    int end=Parsed;while(end<Length&&Bytes[end]!=10){if(end-Parsed>=131072)throw new InvalidOperationException("RECORD_CAP");end++;}
    if(end==Length)break;int count=end-Parsed;if(count<1||count>131072||Records>=256)throw new InvalidOperationException("RECORD_COUNT_OR_SIZE");
    string raw=utf8.GetString(Bytes,Parsed,count);if(raw.Length>0&&raw[0]=='\ufeff')throw new InvalidOperationException("JOURNAL_BOM");
    Dictionary<string,object> record=Json1725.Obj(Json1725.Parse(raw));
    if(record.Count!=3)throw new InvalidOperationException("RECORD_KEYS");ulong sequence=Json1725.UIntNumber(Json1725.Get(record,"sequence"));
    if(sequence!=(ulong)Records||sequence>255)throw new InvalidOperationException("JOURNAL_SEQUENCE");
    byte[] line=new byte[count];Buffer.BlockCopy(Bytes,Parsed,line,0,count);
    JournalRecord1725 item=new JournalRecord1725{Sequence=Records,Start=Parsed,End=end+1,Kind=Json1725.Str(Json1725.Get(record,"kind")),Payload=Json1725.Get(record,"payload"),Sha256=InputRules1701.Hash(line)};
    Rows[Records]=item;Records++;Parsed=end+1;added.Add(item);
   }
   if(Length-Parsed>131072)throw new InvalidOperationException("PARTIAL_RECORD_CAP");return added;
  }
 }
 internal sealed class StepGate1725 {
  internal readonly Step1725[] Steps=Plan1725.NewSteps();internal int Completed;
  internal Step1725 Current;internal bool Failed,GuestExit0,CleanupObserved;internal string First;
  internal readonly string ShellSha;
#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION
  readonly RefusalGate039 Refusal=new RefusalGate039();
#endif
  internal StepGate1725(string shell){ShellSha=shell;}
  void Need(bool b,string why){if(!b){Failed=true;if(First==null)First=why;throw new InvalidOperationException(why);}}
  internal void Expect(int index){
   Need(!Failed&&index==Completed&&index>=0&&index<47&&(Current==null||Current.Returned),"PENDING_NOT_CLOSED");
   Current=Steps[index];
  }
  internal void Accept(JournalRecord1725 record){
   try{AcceptCore(record);}catch{Failed=true;if(First==null)First="JOURNAL_GATE_PARSE_OR_SEMANTIC";throw;}
  }
  void AcceptCore(JournalRecord1725 record){
   Need(!Failed,"STICKY_JOURNAL_FAILURE");
   string kind=record.Kind;
   if(record.Sequence==0){
    Need(kind=="acceptance","FIRST_RECORD_ACCEPTANCE");Dictionary<string,object> p=Json1725.Obj(record.Payload);
    Need(Json1725.Str(Json1725.Get(p,"VM_run"))=="PENDING_ACTUAL_STEPS"&&!Json1725.Bool(Json1725.Get(p,"release_eligible")),"INITIAL_NOT_ACCEPTANCE");return;
   }
   Need(kind!="acceptance","DUPLICATE_ACCEPTANCE");
   if(kind=="UNKNOWN_OR_REJECTED_NO_REPLAY"||kind=="SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY"||kind=="INTERACTIVE_OR_EVIDENCE_LOST_STATE_RETAINED"){Need(false,"ACTUAL_JOURNAL_FAILURE");}
   if(kind=="ATTEMPT_RESERVED_OUTCOME_UNKNOWN"){
    Need(Current!=null&&!Current.Returned&&Current.Command!="exit"&&!Current.ReservationSequence.HasValue,"DUPLICATE_OR_UNEXPECTED_RESERVATION");
    Dictionary<string,object> payload=Json1725.Obj(record.Payload);string step=Json1725.Str(Json1725.Get(payload,"step"));
    Need(step==(Current.Command=="begin"?"guest-identity":Current.Command),"WRONG_RESERVATION");
    if(Current.Command=="begin"){
     Dictionary<string,object> review=Json1725.Obj(Json1725.Get(payload,"review"));
     Need(Json1725.Str(Json1725.Get(review,"expected_uuid"))==Plan1725.Firmware&&Json1725.Str(Json1725.Get(review,"shell"))==RootPPrebirth.Shell
      &&Json1725.Str(Json1725.Get(review,"shell_sha256"))==ShellSha,"BEGIN_RESERVATION_SOURCE");
    }
    Current.ReservationSequence=record.Sequence;return;
   }
   if(kind=="guest-identity-actual-exit"){
    Need(Current!=null&&!Current.Returned&&Current.Command=="begin"&&Current.ReservationSequence.HasValue&&!GuestExit0,"GUEST_EXIT_WITHOUT_RESERVATION");
    Dictionary<string,object> p=Json1725.Obj(record.Payload);Need(Json1725.Bool(Json1725.Get(p,"success"))&&Json1725.UIntNumber(Json1725.Get(p,"exit_code"))==0,"GUEST_PROBE_FAILED");GuestExit0=true;return;
   }
   if(kind=="normal-cleanup-retired"){
    Need(Current!=null&&!Current.Returned&&Current.Command=="finish-cleanup"&&Current.ReservationSequence.HasValue&&!CleanupObserved&&Json1725.Obj(record.Payload).Count>0,"CLEANUP_WITHOUT_SOURCE");
    CleanupObserved=true;return;
   }
   bool success=kind=="guest-identity-validated"||kind=="STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE"
    ||kind=="ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE"||kind=="FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE";
   if(!success)return; // known banners/status kinds are NOT false failures or success
   Need(Current!=null&&!Current.Returned,"DUPLICATE_OR_UNEXPECTED_RETURN");
   if(Current.Command=="exit"){
    Need(kind=="FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE"&&CleanupObserved,"EXIT_WITHOUT_CLEANUP");
#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION
    Refusal.Complete();
#endif
    Dictionary<string,object> p=Json1725.Obj(record.Payload);
    Need(Json1725.Bool(Json1725.Get(p,"sealed_native_observed"))&&!Json1725.Bool(Json1725.Get(p,"prior_business_or_step_blocked"))
     &&Json1725.Obj(Json1725.Get(p,"normal_cleanup_report")).Count>0,"EXIT_ACTUAL_PAYLOAD");
   }else{
    Need(Current.ReservationSequence.HasValue&&record.Sequence>Current.ReservationSequence.Value,"RETURN_WITHOUT_RESERVATION");
    if(Current.Command=="begin"){
     Need(kind=="guest-identity-validated"&&GuestExit0,"BEGIN_RETURN_WITHOUT_PROBE0");
     Dictionary<string,object> p=Json1725.Obj(record.Payload);
     Need(p.Count==10&&Json1725.Str(Json1725.Get(p,"schema"))=="morrow-dedicated-hyperv-guest-identity-v1"
      &&Json1725.Str(Json1725.Get(p,"firmware_uuid"))==Plan1725.Firmware
      &&Json1725.Str(Json1725.Get(p,"manufacturer"))=="Microsoft Corporation"&&Json1725.Str(Json1725.Get(p,"model"))=="Virtual Machine"
      &&Json1725.Bool(Json1725.Get(p,"hypervisor_present"))&&Json1725.Bool(Json1725.Get(p,"administrator"))
      &&Json1725.Bool(Json1725.Get(p,"process_64_bit"))&&Json1725.Bool(Json1725.Get(p,"os_64_bit")),"ACTUAL_GUEST_IDENTITY");
     string name=Json1725.Str(Json1725.Get(p,"machine_name")),time=Json1725.Str(Json1725.Get(p,"observed_utc"));
     Need(name.Length>0&&name.Length<=64&&time.Length>0&&time.Length<=64,"GUEST_IDENTITY_BOUNDS");
    }else if(Current.Command.StartsWith("sealed-",StringComparison.Ordinal)){
     Need(kind=="ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE"&&Json1725.Obj(record.Payload).Count>0,"SEALED_CAUSAL_RETURN");
#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION
     Refusal.Accept(Current.Command,Json1725.Obj(record.Payload));
#endif
     if(Current.Command=="sealed-basic")Need(Json1725.Str(Json1725.Get(Json1725.Obj(record.Payload),"witness"))=="basic","BASIC_RETURN_WITNESS");
    }else Need(kind=="STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE"&&Json1725.Str(record.Payload)==Current.Command,"WRONG_COMMAND_RETURN");
   }
   Current.ReturnSequence=record.Sequence;Current.Returned=true;Completed++;
  }
 }
}
