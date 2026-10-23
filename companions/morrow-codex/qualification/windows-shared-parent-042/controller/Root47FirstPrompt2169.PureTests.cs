// SOURCE2169 synthetic byte-only cases; not original process/VM evidence.
using System;
using System.Text;
namespace C28.RootP1682 {
 public static class FirstPromptPureTests2169 {
  static byte[] Fresh(){return new byte[FirstPromptGate2169.StdoutCapacity];}
  static byte[] Bytes(string text){return Encoding.ASCII.GetBytes(text);}
  static string Prompt(){return "Type exactly: "+InputProtocol1701.Text(0);}
  static void Put(byte[] target,int at,byte[] bytes){Buffer.BlockCopy(bytes,0,target,at,bytes.Length);}
  static void Assert(bool value){if(!value)throw new InvalidOperationException("FIRST_PROMPT_PURE_ASSERT");}
  static void Reject(Action body){bool rejected=false;try{body();}catch(InvalidOperationException){rejected=true;}Assert(rejected);}
  public static void complete_lf_line_at_stream_start_is_required(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   byte[] line=Bytes(Prompt()+"\n");Put(output,0,line);
   Assert(gate.Observe(output,0,line.Length));Assert(gate.CheckedThrough==line.Length);
  }
  public static void complete_crlf_line_after_description_is_allowed(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   byte[] line=Bytes("fixed description\r\n"+Prompt()+"\r\n");Put(output,0,line);
   Assert(gate.Observe(output,0,line.Length));
  }
  public static void missing_character_or_trailing_newline_cannot_be_ready(){
   foreach(string text in new string[]{Prompt(),Prompt()+"\r",Prompt().Substring(1)+"\n",Prompt()+" \n"}){
    byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
    byte[] value=Bytes(text);Put(output,0,value);Assert(!gate.Observe(output,0,value.Length));
   }
  }
  public static void stderr_only_537_byte_diagnostic_never_enters_stdout_gate(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   byte[] diagnostic=new byte[537];for(int i=0;i<diagnostic.Length;i++)diagnostic[i]=(byte)'0';
   Put(diagnostic,0,Bytes("C28ERR1 P "));diagnostic[diagnostic.Length-1]=10;
   // No stderr parameter exists. A diagnostic cannot set any gate field.
   Assert(diagnostic.Length==537);Assert(!gate.Observe(output,0,0));
   Assert(gate.NewBytesExamined==0);
  }
  public static void arbitrary_stdout_or_embedded_prompt_is_not_a_full_line(){
   foreach(string text in new string[]{"arbitrary stdout\n","C28ERR1 P "+Prompt()+"\n",
    "x"+Prompt()+"\n",Prompt()+"x\n",Prompt()+"\r\r\n"}){
    byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
    byte[] value=Bytes(text);Put(output,0,value);Assert(!gate.Observe(output,0,value.Length));
   }
  }
  public static void all_split_boundaries_and_crlf_boundary_preserve_byte_match(){
   byte[] line=Bytes("description\n"+Prompt()+"\r\n");
   for(int split=0;split<=line.Length;split++){
    byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
    Buffer.BlockCopy(line,0,output,0,split);
    bool first=gate.Observe(output,0,split);
    Assert(first==(split==line.Length));
    Buffer.BlockCopy(line,split,output,split,line.Length-split);
    Assert(gate.Observe(output,split,line.Length));
    Assert(gate.NewBytesExamined==line.Length);
   }
  }
  public static void each_single_byte_increment_is_examined_once_not_full_rescan(){
   byte[] line=Bytes("description\n"+Prompt()+"\n");
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   for(int i=0;i<line.Length;i++){
    output[i]=line[i];Assert(gate.Observe(output,i,i+1)==(i+1==line.Length));
    Assert(gate.NewBytesExamined==i+1);
   }
   Assert(gate.Observe(output,line.Length,line.Length));
   Assert(gate.NewBytesExamined==line.Length);
  }
  public static void fixed_buffer_size_and_fresh_initial_state_are_required(){
   Reject(delegate{new FirstPromptGate2169(null,0);});
   Reject(delegate{new FirstPromptGate2169(new byte[16],0);});
   Reject(delegate{new FirstPromptGate2169(new byte[FirstPromptGate2169.StdoutCapacity+1],0);});
   Reject(delegate{new FirstPromptGate2169(Fresh(),1);});
   byte[] old=Fresh();Put(old,0,Bytes(Prompt()+"\n"));
   Reject(delegate{new FirstPromptGate2169(old,0);});
  }
  public static void foreign_buffer_old_cursor_and_out_of_range_are_sticky_rejections(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   Reject(delegate{gate.Observe(Fresh(),0,0);});
   Reject(delegate{gate.Observe(output,0,0);});
   output=Fresh();gate=new FirstPromptGate2169(output,0);
   output[0]=(byte)'x';Assert(!gate.Observe(output,0,1));
   Reject(delegate{gate.Observe(output,0,1);});
   Reject(delegate{gate.Observe(output,1,1);});
   output=Fresh();gate=new FirstPromptGate2169(output,0);
   Reject(delegate{gate.Observe(output,0,output.Length+1);});
   output=Fresh();gate=new FirstPromptGate2169(output,0);
   Reject(delegate{gate.Observe(output,0,-1);});
  }
  public static void a_matched_scalar_cannot_hide_rewritten_original_prompt_bytes(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   byte[] line=Bytes(Prompt()+"\n");Put(output,0,line);Assert(gate.Observe(output,0,line.Length));
   output[0]=(byte)'x';
   Reject(delegate{gate.Observe(output,line.Length,line.Length);});
  }
  public static void separate_controller_cannot_reuse_other_attempt_seen_or_cursor(){
   byte[] first=Fresh();FirstPromptGate2169 a=new FirstPromptGate2169(first,0);
   byte[] line=Bytes(Prompt()+"\n");Put(first,0,line);Assert(a.Observe(first,0,line.Length));
   byte[] second=Fresh();FirstPromptGate2169 b=new FirstPromptGate2169(second,0);
   Assert(!b.Observe(second,0,0));Assert(b.CheckedThrough==0);
   Reject(delegate{b.Observe(first,0,line.Length);});
  }
  public static void missing_prompt_with_full_capacity_scans_only_new_bytes(){
   byte[] output=Fresh();FirstPromptGate2169 gate=new FirstPromptGate2169(output,0);
   for(int i=0;i<output.Length;i++)output[i]=(byte)'x';
   Assert(!gate.Observe(output,0,output.Length));
   Assert(gate.NewBytesExamined==output.Length);
   Assert(!gate.Observe(output,output.Length,output.Length));
   Assert(gate.NewBytesExamined==output.Length);
  }
 }
}
