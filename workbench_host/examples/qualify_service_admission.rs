//! Local diagnostic on a new qualification library; never logs bearer tokens.
use morrow_workbench_host::Workbench;
fn main()->morrow_workbench_host::Result<()> {
 let mut args=std::env::args().skip(1);let root=args.next().ok_or("fresh root")?;let package=args.next().ok_or("package")?;
 let package=morrow_core::plugin_package::catalog::read_file(std::path::Path::new(&package))?;
 let mut host=Workbench::open_managed(std::path::Path::new(&root),Some(package))?;
 host.save_ui_locale("diagnostic-admission",0,"en")?;
 match host.issue_service_authentication(&[],0,"alice",1) {
  Ok(issued)=>println!("{}",serde_json::json!({"stage":"authentication","status":"passed","revision":issued.info.revision,"token_logged":false})),
  Err(error)=>println!("{}",serde_json::json!({"stage":"authentication","status":"failed","error":error.to_string(),"debug":format!("{error:?}"),"token_logged":false})),
 }
 host.finish()?;Ok(())
}
