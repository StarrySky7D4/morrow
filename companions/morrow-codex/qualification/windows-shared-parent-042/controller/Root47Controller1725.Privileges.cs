// SOURCE1725: one read-only primary-token eligibility check on the original
// controller creator thread. No privilege adjustment, impersonation or new login.
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using N=C28.RootP1682.RootPPrebirth.Native;
namespace C28.RootP1682 {
 [StructLayout(LayoutKind.Sequential)]
 internal struct Luid1725 {internal uint Low;internal int High;}
 internal sealed class PrivilegePresence1725 {
  internal bool Observed,AssignPresent,QuotaPresent;internal uint? AssignAttributes,QuotaAttributes;
  internal bool Eligible {get{return Observed&&AssignPresent&&QuotaPresent;}}
 }
 internal static class PrivilegeRules1725 {
  internal static PrivilegePresence1725 Parse(byte[] bytes,int length,Luid1725 assign,Luid1725 quota){
   if(bytes==null||length<4||length>65536||length>bytes.Length)throw new InvalidOperationException("TOKEN_PRIVILEGES_LENGTH");
   ulong a=Key(assign),q=Key(quota);if(a==q)throw new InvalidOperationException("TOKEN_PRIVILEGE_LUID_COLLISION");
   uint count=BitConverter.ToUInt32(bytes,0);
   if(count>4096||4L+12L*count!=length)throw new InvalidOperationException("TOKEN_PRIVILEGES_LAYOUT");
   PrivilegePresence1725 result=new PrivilegePresence1725();HashSet<ulong> seen=new HashSet<ulong>();
   for(int i=0;i<(int)count;i++){
    int at=4+12*i;Luid1725 id=new Luid1725{Low=BitConverter.ToUInt32(bytes,at),High=BitConverter.ToInt32(bytes,at+4)};
    ulong key=Key(id);if(!seen.Add(key))throw new InvalidOperationException("TOKEN_PRIVILEGE_DUPLICATE_LUID");
    uint attrs=BitConverter.ToUInt32(bytes,at+8);
    if(key==a){result.AssignPresent=true;result.AssignAttributes=attrs;}
    if(key==q){result.QuotaPresent=true;result.QuotaAttributes=attrs;}
   }
   result.Observed=true;return result; // PRESENT; enabled/disabled attributes do not grant or deny presence.
  }
  static ulong Key(Luid1725 id){return ((ulong)(uint)id.High<<32)|id.Low;}
 }
 internal sealed class TokenPreflight1725 {
  internal readonly Slot Token=new Slot("CONTROLLER_PRIMARY_TOKEN"),Memory=new Slot("TOKEN_PRIVILEGES_BUFFER");
  internal readonly byte[] Captured=new byte[65536];
  internal Luid1725 AssignLuid,QuotaLuid;internal bool? AssignLookupReturned,QuotaLookupReturned,OpenReturned,QueryReturned,CloseReturned;
  internal int AssignLookupError,QuotaLookupError,OpenError,QueryError,CloseError;internal uint ReturnedBytes;
  internal PrivilegePresence1725 Presence=new PrivilegePresence1725();
  internal string Status="NOT_OBSERVED_NO_P_T_OR_PRODUCT_EFFECTS";
  internal bool Check(Io1725 io){
   io.Check();Status="READONLY_PRIMARY_TOKEN_PREFLIGHT_PENDING_NO_P_T_OR_PRODUCT_EFFECTS";
   bool ok=Native1725.LookupPrivilegeValueW(null,"SeAssignPrimaryTokenPrivilege",out AssignLuid);
   AssignLookupError=ok?0:Marshal.GetLastWin32Error();AssignLookupReturned=ok;io.Need(ok,"ASSIGN_PRIVILEGE_LUID_QUERY");
   io.Check();ok=Native1725.LookupPrivilegeValueW(null,"SeIncreaseQuotaPrivilege",out QuotaLuid);
   QuotaLookupError=ok?0:Marshal.GetLastWin32Error();QuotaLookupReturned=ok;io.Need(ok,"QUOTA_PRIVILEGE_LUID_QUERY");
   io.Check();Token.Begin();
   // Borrowed current-process pseudo handle is an API argument, never owned or closed.
   ok=N.OpenProcessToken(N.GetCurrentProcess(),8,ref Token.Raw);OpenError=ok?0:Marshal.GetLastWin32Error();OpenReturned=ok;
   Token.Capture(Token.Raw,ok,OpenError);io.Need(Token.State==ResourceState.Open,"PREFLIGHT_ORIGINAL_TOKEN_OPEN");
   io.Check();Memory.Begin();Memory.Raw=Marshal.AllocHGlobal(65536);Memory.Capture(Memory.Raw,Memory.Raw!=IntPtr.Zero,0);
   io.Need(Memory.State==ResourceState.Open,"PREFLIGHT_QUERY_BUFFER");
   io.Check();ReturnedBytes=0;
   ok=N.GetTokenInformation(Token.Raw,3,Memory.Raw,65536,ref ReturnedBytes); // ONE fixed-cap TokenPrivileges query; no sizing retry.
   QueryError=ok?0:Marshal.GetLastWin32Error();QueryReturned=ok;io.Need(ok&&ReturnedBytes>=4&&ReturnedBytes<=65536,"PREFLIGHT_TOKEN_PRIVILEGES_QUERY");
   Marshal.Copy(Memory.Raw,Captured,0,(int)ReturnedBytes);io.Check();
   Presence=PrivilegeRules1725.Parse(Captured,(int)ReturnedBytes,AssignLuid,QuotaLuid);
   Status=Presence.Eligible?"PRESENT_BOTH_NOT_AN_ENABLED_OR_PRODUCT_AUTHORITY":"KNOWN_INELIGIBLE_NO_P_T_OR_PRODUCT_EFFECTS";
   io.Check();Token.BeginClose();ok=N.CloseHandle(Token.Raw);CloseError=ok?0:Marshal.GetLastWin32Error();CloseReturned=ok;
   io.Need(Token.Closed(ok,CloseError),"PREFLIGHT_TOKEN_CHECKED_CLOSE_NO_RETRY");
   io.Check();Memory.BeginClose();Marshal.FreeHGlobal(Memory.Raw);Memory.State=ResourceState.MemoryRetired;io.Check();
   return Presence.Eligible;
  }
  internal Dictionary<string,object> Data(){
   return Driver1725.D("scope","CONTROLLER_PRIMARY_TOKEN_ONLY_NOT_ACCOUNT_POLICY_OR_CHILD_TOKEN","status",Status,"observed",Presence.Observed,
    "required_assign_present",Presence.AssignPresent,"required_quota_present",Presence.QuotaPresent,"enabled_required",false,
    "assign_attributes",Presence.AssignAttributes,"quota_attributes",Presence.QuotaAttributes,
    "assign_luid_low",AssignLuid.Low,"assign_luid_high",AssignLuid.High,"quota_luid_low",QuotaLuid.Low,"quota_luid_high",QuotaLuid.High,
    "assign_lookup_returned",AssignLookupReturned,"quota_lookup_returned",QuotaLookupReturned,"assign_lookup_error",AssignLookupError,"quota_lookup_error",QuotaLookupError,
    "query_returned",QueryReturned,"query_bytes",ReturnedBytes,"query_error",QueryError,"token_open_returned",OpenReturned,"token_open_error",OpenError,
    "token_state",Token.State.ToString(),"token_close_returned",CloseReturned,"token_close_error",CloseError,"buffer_state",Memory.State.ToString(),
    "privilege_adjustments",0,"new_login_attempts",0);
  }
  internal static class Native1725 {
   [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
   internal static extern bool LookupPrivilegeValueW(string system,string name,out Luid1725 luid);
  }
 }
}
