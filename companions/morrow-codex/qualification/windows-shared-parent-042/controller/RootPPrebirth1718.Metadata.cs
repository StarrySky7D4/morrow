// SOURCE1718: actual file/ancestor comparison rule. No OS effects or hash bypass.
using System;
namespace C28.RootP1682 {
 internal static class FileMetadataRules1718 {
  internal const ulong SystemShellBytes=450560;
  internal const string SystemShellSha="3247bcfd60f6dd25f34cb74b5889ab10ef1b3ec72b4d4b3d95b5b25b534560b8";
  internal static bool SystemShellPath(string path){return String.Equals(path,RootPPrebirth.Shell,StringComparison.OrdinalIgnoreCase);}
  internal static bool ReadOnlyShellOpen(string path,bool directory,uint access,uint share,uint disposition,bool inherit){
   return !SystemShellPath(path)||(!directory&&access==0x80000000u&&share==1&&disposition==3&&!inherit);
  }
  // Observation shape ONLY. A null digest permits original-handle readonly
  // hashing/stability observation; it never admits a selector or creates P/T.
  internal static bool ObservedLinks(FilePin pin){
   if(pin==null)return false;
   if(!SystemShellPath(pin.Path))return pin.Links==1;
   return !pin.Directory&&pin.Links==2&&pin.Bytes==SystemShellBytes&&pin.Attributes==32
    &&(String.IsNullOrEmpty(pin.Sha256)||pin.Sha256==SystemShellSha);
  }
  internal static bool BoundLinks(FilePin pin){
   return ObservedLinks(pin)&&(!SystemShellPath(pin.Path)||pin.Sha256==SystemShellSha);
  }
  static bool Admitted(FilePin pin) {
   if(pin==null||pin.Path==null||pin.Volume==0||!Rules.Hex(pin.FileId,32)||pin.CreationFileTime==0
    ||!ObservedLinks(pin)||(pin.Attributes&0x400)!=0||((pin.Attributes&16)!=0)!=pin.Directory)return false;
   try{Rules.LocalPath(pin.Path.Length>0?Char.ToUpperInvariant(pin.Path[0])+pin.Path.Substring(1):pin.Path);}catch(InvalidOperationException){return false;}
   return true;
  }
  internal static bool Same(FilePin before,FilePin observed) {
   if(!Admitted(before)||!Admitted(observed))return false;
   if(!String.Equals(before.Path,observed.Path,StringComparison.OrdinalIgnoreCase)
    ||before.Directory!=observed.Directory||before.Volume!=observed.Volume||before.FileId!=observed.FileId
    ||before.CreationFileTime!=observed.CreationFileTime||before.Attributes!=observed.Attributes
    ||before.Links!=observed.Links)return false;
   // Directory size and last-write describe contents, not stable identity.
   // The original observed values remain in before/final DATA; no normalization.
   if(before.Directory)return true;
   // Ordinary files/PEs retain BOTH metadata gates plus the caller's full SHA.
   return before.Bytes==observed.Bytes&&before.ModifiedFileTime==observed.ModifiedFileTime;
  }
  internal static FilePin WithDigest(FilePin actual,string digest) {
   return new FilePin(actual.Path,actual.Directory,actual.Volume,actual.FileId,actual.Bytes,
    actual.CreationFileTime,actual.ModifiedFileTime,actual.Attributes,actual.Links,digest);
  }
 }
}
