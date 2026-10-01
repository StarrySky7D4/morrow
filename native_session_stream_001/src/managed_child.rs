//! Safe adapter shared by legacy Tokio launch and atomically contained launch.
use std::{io, process::ExitStatus,pin::Pin,task::{Context,Poll},time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::Child;
use morrow_native_pipe_win::job::{JobChild,JobStdin,JobStdinProof};


pub(crate) enum Process { Legacy(Child), Contained(JobChild) }
pub(crate) struct ManagedChild {
    process: Process,
    pub stdin: Option<ManagedStdin>,
    control_proof:Option<JobStdinProof>,
    pub stdout: Option<Box<dyn AsyncRead + Unpin + Send>>,
    pub stderr: Option<Box<dyn AsyncRead + Unpin + Send>>,
}
impl ManagedChild {
    pub fn legacy(mut child: Child) -> Self {
        Self { stdin:child.stdin.take().map(ManagedStdin::Legacy), control_proof:None, stdout:child.stdout.take().map(|p|Box::new(p) as _), stderr:child.stderr.take().map(|p|Box::new(p) as _), process:Process::Legacy(child) }
    }
    pub fn contained(mut child: JobChild,close_budget:Duration) -> Self {
        let control_proof=child.stdin.as_ref().map(JobStdin::proof);
        Self { stdin:child.stdin.take().map(|writer|ManagedStdin::Contained{writer:Some(writer),close_budget}), control_proof, stdout:child.stdout.take().map(|p|Box::new(p) as _), stderr:child.stderr.take().map(|p|Box::new(p) as _), process:Process::Contained(child) }
    }
    pub fn control_reclamation_finished(&self)->bool {match self.process {Process::Legacy(_)=>true,Process::Contained(_)=>self.control_proof.as_ref().is_some_and(JobStdinProof::finished)}}
    pub fn control_closed_and_reaped(&self)->bool {match self.process {Process::Legacy(_)=>true,Process::Contained(_)=>self.control_proof.as_ref().is_some_and(JobStdinProof::closed_and_reaped)}}
    pub fn id(&self)->Option<u32> { match &self.process { Process::Legacy(p)=>p.id(), Process::Contained(p)=>Some(p.pid()) } }
    pub fn resume(&mut self)->io::Result<()> { match &mut self.process { Process::Legacy(_)=>Ok(()),Process::Contained(p)=>p.resume() } }
    pub fn start_kill(&mut self)->io::Result<()> { match &mut self.process { Process::Legacy(p)=>p.start_kill(),Process::Contained(p)=>p.start_kill() } }
    pub async fn wait(&mut self)->io::Result<ExitStatus> { match &mut self.process { Process::Legacy(p)=>p.wait().await,Process::Contained(p)=>p.wait().await } }
}

pub(crate) enum ManagedStdin {
 Legacy(tokio::process::ChildStdin),
 Contained{writer:Option<JobStdin>,close_budget:Duration},
}
impl ManagedStdin {
 pub fn write_unreported(&self)->bool {match self{Self::Legacy(_)=>false,Self::Contained{writer,..}=>writer.as_ref().is_some_and(JobStdin::write_unreported)}}
}
impl Drop for ManagedStdin {
 fn drop(&mut self){if let Self::Contained{writer,close_budget}=self{if let Some(writer)=writer.take(){let _=writer.reclaim_bounded(*close_budget);}}}
}
impl AsyncWrite for ManagedStdin {
 fn poll_write(self:Pin<&mut Self>,cx:&mut Context<'_>,bytes:&[u8])->Poll<io::Result<usize>> {match self.get_mut(){Self::Legacy(p)=>Pin::new(p).poll_write(cx,bytes),Self::Contained{writer:Some(p),..}=>Pin::new(p).poll_write(cx,bytes),_=>Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))}}
 fn poll_flush(self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {match self.get_mut(){Self::Legacy(p)=>Pin::new(p).poll_flush(cx),Self::Contained{writer:Some(p),..}=>Pin::new(p).poll_flush(cx),_=>Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))}}
 fn poll_shutdown(self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {match self.get_mut(){Self::Legacy(p)=>Pin::new(p).poll_shutdown(cx),Self::Contained{writer:Some(p),..}=>Pin::new(p).poll_shutdown(cx),_=>Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))}}
}
