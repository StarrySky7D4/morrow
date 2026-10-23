// ROOT QA bridge: only the four reviewed pure test classes; no runtime entry.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
namespace C28.RootP1682 {
 public static class QaUnitMain1855 {
  static string Name(MethodInfo m) { return "Root47Controller1725::"+m.DeclaringType.Name+"::"+m.Name; }
  static List<MethodInfo> Methods() {
   List<MethodInfo> all=new List<MethodInfo>();
   Type[] types={typeof(PureTests1682),typeof(InputPureTests1701),typeof(MetadataPureTests1718),typeof(ControllerPureTests1725)};
   int[] expected={19,17,16,21};
   for(int k=0;k<types.Length;k++) {
    MethodInfo[] methods=types[k].GetMethods(BindingFlags.Public|BindingFlags.Static|BindingFlags.DeclaredOnly);
    if(methods.Length!=expected[k])throw new InvalidOperationException("EXACT_PURE_CLASS_COUNT");
    foreach(MethodInfo m in methods) {
     if(m.ReturnType!=typeof(void)||m.GetParameters().Length!=0)throw new InvalidOperationException("PURE_SIGNATURE");
     foreach(char c in m.Name)if(!((c>='a'&&c<='z')||(c>='A'&&c<='Z')||(c>='0'&&c<='9')||c=='_'))throw new InvalidOperationException("PURE_NAME");
     all.Add(m);
    }
   }
   all.Sort(delegate(MethodInfo a,MethodInfo b){return StringComparer.Ordinal.Compare(Name(a),Name(b));});
   if(all.Count!=73)throw new InvalidOperationException("EXACT_SEVENTY_THREE_PURE_METHODS");
   for(int i=1;i<all.Count;i++)if(Name(all[i-1])==Name(all[i]))throw new InvalidOperationException("DUPLICATE_PURE_METHOD");
   return all;
  }
  public static int Main(string[] args) {
   try {
    List<MethodInfo> methods=Methods();
    if(args.Length==3&&args[0]=="--list"&&args[1]=="--format"&&args[2]=="terse") {
     foreach(MethodInfo m in methods)Console.WriteLine(Name(m)+": test");return 0;
    }
    if(args.Length!=4||args[1]!="--exact"||args[2]!="--nocapture"||args[3]!="--test-threads=1")return 78;
    MethodInfo selected=null;foreach(MethodInfo m in methods)if(args[0]==Name(m))selected=m;
    if(selected==null)return 78;
    Console.WriteLine("running 1 test");bool pass=false;
    try{selected.Invoke(null,null);pass=true;}
    catch(TargetInvocationException e){Exception actual=e.InnerException??e;Console.Error.WriteLine(actual.GetType().FullName+": "+actual.Message);}
    catch(Exception e){Console.Error.WriteLine(e.GetType().FullName+": "+e.Message);}
    Console.WriteLine("test "+args[0]+" ... "+(pass?"ok":"FAILED"));
    Console.WriteLine("test result: "+(pass?"ok. 1 passed; 0 failed":"FAILED. 0 passed; 1 failed")+"; 0 ignored; 0 measured; "+(methods.Count-1).ToString(CultureInfo.InvariantCulture)+" filtered out;");
    return pass?0:101;
   }catch(Exception e){Console.Error.WriteLine(e.GetType().FullName+": "+e.Message);return 78;}
  }
 }
}
