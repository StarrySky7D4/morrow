#if P2_SHARED_PARENT_REFUSAL_SESSION
// SOURCE2169 pure first-prompt guard. Same-controller original stdout only.
// No IO, native query, clock, authority, stderr, prior log or readiness parameter.
using System;
namespace C28.RootP1682 {
 internal sealed class RoundPrompt042 {
  internal const int StdoutCapacity=2097152,MaxTokenBytes=254;
  readonly byte[] Original,Token;
  int Through,Matched,LineStart,FoundStart=-1,FoundEnd=-1;
  bool Candidate=true,PendingCr,Seen,Poisoned;
  long Examined;
  internal RoundPrompt042(byte[] original,int initialLength){
   if(original==null||original.Length!=StdoutCapacity||initialLength<0||initialLength>original.Length)throw new InvalidOperationException("ROUND_PROMPT_ORIGINAL_BOUND");
   Through=initialLength;LineStart=initialLength;Candidate=initialLength==0||original[initialLength-1]==10;
   string expected=InputProtocol1701.Text(0);
   if(expected==null||expected.Length==0)throw new InvalidOperationException("FIRST_PROMPT_FIXED_PROTOCOL");
   string text="Type exactly: "+expected;
   if(InputProtocol1701.Kind(0)!="confirmation"||text.Length<1||text.Length>MaxTokenBytes)
    throw new InvalidOperationException("FIRST_PROMPT_FIXED_PROTOCOL");
   Token=new byte[text.Length];
   for(int i=0;i<text.Length;i++){
    if(text[i]<32||text[i]>126)throw new InvalidOperationException("FIRST_PROMPT_ASCII");
    Token[i]=(byte)text[i];
   }
   Original=original;
  }
  internal int CheckedThrough {get{return Through;}}
  internal long NewBytesExamined {get{return Examined;}}
  void Need(bool value){
   if(!value){Poisoned=true;throw new InvalidOperationException("FIRST_PROMPT_ORIGINAL_CURSOR");}
  }
  internal bool Observe(byte[] original,int from,int through){
   Need(!Poisoned);
   Need(Object.ReferenceEquals(original,Original)&&original.Length==StdoutCapacity);
   Need(from==Through&&from>=0&&through>=from&&through<=Original.Length);
   if(!Seen){
    for(int at=from;at<through;at++){
     byte value=Original[at];Examined++;
     if(Candidate){
      if(PendingCr){
       if(value==10){FoundStart=LineStart;FoundEnd=at+1;Seen=true;}
       else {Candidate=false;PendingCr=false;}
      }else if(Matched<Token.Length){
       if(value==Token[Matched])Matched++;
       else Candidate=false;
      }else if(value==10){FoundStart=LineStart;FoundEnd=at+1;Seen=true;}
      else if(value==13)PendingCr=true;
      else Candidate=false;
     }
     if(Seen)break;
     if(value==10){Candidate=true;PendingCr=false;Matched=0;LineStart=at+1;}
    }
   }
   Through=through;
   if(Seen){
    // Revalidate ONLY the fixed matched line, never a scalar-seen shortcut.
    Need(FoundStart>=0&&FoundEnd<=through&&FoundEnd>FoundStart);
    Need(FoundStart==0||Original[FoundStart-1]==10);
    int suffix=FoundEnd-FoundStart-Token.Length;
    Need(suffix==1||suffix==2);
    for(int i=0;i<Token.Length;i++)Need(Original[FoundStart+i]==Token[i]);
    if(suffix==1)Need(Original[FoundEnd-1]==10);
    else Need(Original[FoundEnd-2]==13&&Original[FoundEnd-1]==10);
   }
   return Seen;
  }
 }
}

#endif
