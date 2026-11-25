# Diagnostic only: run in a disposable Windows worker account.
# Exercises private temporary fixtures; does not change repository ACLs or services.
# No product policy, assertion, retry, or timeout is modified.
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Security.AccessControl;
public static class TokenMatrix742 {
 [StructLayout(LayoutKind.Sequential)] struct SA {public IntPtr Sid;public uint Attributes;}
 [StructLayout(LayoutKind.Sequential)] struct GM {public uint Read,Write,Execute,All;}
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool CreateRestrictedToken(IntPtr t,uint f,uint dc,IntPtr ds,uint pc,IntPtr ps,uint rc,[In] SA[] rs,out IntPtr nt);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool DuplicateToken(IntPtr t,int level,out IntPtr nt);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool AccessCheck(byte[] sd,IntPtr t,uint access,ref GM map,IntPtr priv,ref uint size,out uint granted,out bool status);
 static void Need(bool ok){if(!ok)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}

 [DllImport("advapi32.dll",SetLastError=true)] static extern bool ImpersonateLoggedOnUser(IntPtr t);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool RevertToSelf();
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool DeleteFile(string path);
 static string DeleteProbe(IntPtr token) {
  string root=Path.Combine(Path.GetTempPath(),"webmux742-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(root);
  string result="";
  try {foreach(int mask in new int[]{0x1f01ff,0x1f01bf}){
   string parent=Path.Combine(root,mask.ToString("x"));Directory.CreateDirectory(parent);
   var user=WindowsIdentity.GetCurrent().User;
   var acl=new DirectorySecurity();acl.SetOwner(user);acl.SetAccessRuleProtection(true,false);
   acl.AddAccessRule(new FileSystemAccessRule(user,(FileSystemRights)mask,InheritanceFlags.ContainerInherit|InheritanceFlags.ObjectInherit,PropagationFlags.None,AccessControlType.Allow));
   Directory.SetAccessControl(parent,acl);
   string child=Path.Combine(parent,"outside.txt");File.WriteAllText(child,"outside");
   Need(ImpersonateLoggedOnUser(token));bool deleted;int err;
   try {deleted=DeleteFile(child);err=deleted?0:Marshal.GetLastWin32Error();} finally {Need(RevertToSelf());}
   result+="actual DeleteFile parent_user_mask=0x"+mask.ToString("x")+" deleted="+deleted+" error="+err+" remains="+File.Exists(child)+"\n";
  }} finally {Directory.Delete(root,true);}return result;
 }

 [DllImport("advapi32.dll",SetLastError=true)] static extern bool SetTokenInformation(IntPtr t,int cls,ref SA info,uint length);
 static void SetLow(IntPtr t){var sid=new SecurityIdentifier("S-1-16-4096");byte[] b=new byte[sid.BinaryLength];sid.GetBinaryForm(b,0);IntPtr p=Marshal.AllocHGlobal(b.Length);try{Marshal.Copy(b,0,p,b.Length);SA label=new SA{Sid=p,Attributes=0x20};if(!SetTokenInformation(t,25,ref label,(uint)(Marshal.SizeOf(typeof(SA))+b.Length)))throw new Exception("SetLow: "+Marshal.GetLastWin32Error());}finally{Marshal.FreeHGlobal(p);}}

 [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSecurityDescriptorToSecurityDescriptor(string s,uint r,out IntPtr sd,out uint size);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetSecurityDescriptorSacl(IntPtr sd,out bool present,out IntPtr sacl,out bool def);
 [DllImport("advapi32.dll",CharSet=CharSet.Unicode)] static extern uint SetNamedSecurityInfo(string name,int type,uint info,IntPtr owner,IntPtr group,IntPtr dacl,IntPtr sacl);
 [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr ptr);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool RemoveDirectory(string path);
 static void SetLabel(string path,string level){IntPtr sd;uint size;Need(ConvertStringSecurityDescriptorToSecurityDescriptor("S:(ML;OICI;NW;;;"+level+")",1,out sd,out size));try{bool present,def;IntPtr sacl;Need(GetSecurityDescriptorSacl(sd,out present,out sacl,out def));uint err=SetNamedSecurityInfo(path,1,0x10,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero,sacl);if(err!=0)throw new Exception("SetLabel "+err);}finally{LocalFree(sd);}}
 static string WorkspaceProbe(IntPtr token){
  string root=Path.Combine(Path.GetTempPath(),"webmux742-low-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(root);string result="";
  try {
   var user=WindowsIdentity.GetCurrent().User;var cap=new SecurityIdentifier("S-1-5-21-111111111-222222222-333333333-4444");
   var acl=new DirectorySecurity();acl.SetOwner(user);acl.SetAccessRuleProtection(true,false);
   acl.AddAccessRule(new FileSystemAccessRule(user,FileSystemRights.FullControl,InheritanceFlags.ContainerInherit|InheritanceFlags.ObjectInherit,PropagationFlags.None,AccessControlType.Allow));
   acl.AddAccessRule(new FileSystemAccessRule(cap,(FileSystemRights)0x1301bf,InheritanceFlags.ContainerInherit|InheritanceFlags.ObjectInherit,PropagationFlags.None,AccessControlType.Allow));
   Directory.SetAccessControl(root,acl);SetLabel(root,"LW");
   string child=Path.Combine(root,"workspace.txt");File.WriteAllText(child,"workspace");string git=Path.Combine(root,".git");Directory.CreateDirectory(git);SetLabel(git,"ME");
   var gitacl=Directory.GetAccessControl(git);gitacl.AddAccessRule(new FileSystemAccessRule(cap,(FileSystemRights)0x1301bf,InheritanceFlags.ContainerInherit|InheritanceFlags.ObjectInherit,PropagationFlags.None,AccessControlType.Deny));Directory.SetAccessControl(git,gitacl);
   Need(ImpersonateLoggedOnUser(token));bool deleted,gitdeleted;int err,gerr;
   try {deleted=DeleteFile(child);err=deleted?0:Marshal.GetLastWin32Error();gitdeleted=RemoveDirectory(git);gerr=gitdeleted?0:Marshal.GetLastWin32Error();}finally{Need(RevertToSelf());}
   result+="low-workspace file deleted="+deleted+" error="+err+" medium-protected-git deleted="+gitdeleted+" error="+gerr+"\n";
  }finally{Directory.Delete(root,true);}return result;
 }

 public static string Run(){
  IntPtr original;Need(OpenProcessToken(GetCurrentProcess(),0x000f01ff,out original));
  var sid=WindowsIdentity.GetCurrent().User.Value;
  var sd=new RawSecurityDescriptor("O:"+sid+"G:SYD:(A;;FA;;;"+sid+")");byte[] bytes=new byte[sd.BinaryLength];sd.GetBinaryForm(bytes,0);
  var rs=new SecurityIdentifier("S-1-5-21-111111111-222222222-333333333-4444");byte[] sb=new byte[rs.BinaryLength];rs.GetBinaryForm(sb,0);IntPtr ptr=Marshal.AllocHGlobal(sb.Length);Marshal.Copy(sb,0,ptr,sb.Length);
  var result="synthetic descriptor: current user FILE_ALL_ACCESS only; no restricting SID grant\n";
  try{foreach(int mode in new int[]{0,1,2}){uint flags=mode==0?5u:13u;IntPtr rt;Need(CreateRestrictedToken(original,flags,0,IntPtr.Zero,0,IntPtr.Zero,1,new SA[]{new SA{Sid=ptr,Attributes=0}},out rt));try{if(mode==2)SetLow(rt);result+="low_integrity="+(mode==2)+"\n";IntPtr it;Need(DuplicateToken(rt,2,out it));try{if(mode>0)result+=DeleteProbe(it);if(mode==2)result+=WorkspaceProbe(it);foreach(uint access in new uint[]{1,2,0x40,0x10000,0x40000,0x80000}){GM gm=new GM{Read=0x120089,Write=0x120116,Execute=0x1200a0,All=0x1f01ff};uint n=4096;IntPtr priv=Marshal.AllocHGlobal((int)n);try{uint grant;bool status;Need(AccessCheck(bytes,it,access,ref gm,priv,ref n,out grant,out status));result+="flags="+flags+" access=0x"+access.ToString("x")+" allowed="+status+" granted=0x"+grant.ToString("x")+"\n";}finally{Marshal.FreeHGlobal(priv);}}}finally{CloseHandle(it);}}finally{CloseHandle(rt);}}}finally{Marshal.FreeHGlobal(ptr);CloseHandle(original);}return result;
 }
}
'@
try {[TokenMatrix742]::Run()} catch {$_.Exception.ToString();throw}
