// Included inside the unchanged original worker tests fixture. No fake provider,
// Workbench factory grant, native Start, ProtectedSession or UI success claim.
mod product_wire_tests {
    use super::*;
    use crate::agent_product_control::{Request as WireRequest,testing};
    use crate::agent_product_control_capnp::{Action as A,Operation as O,Status as S};
    use crate::agent_tasks::agent_product::ProductSessionController;
    use crate::io_tasks::TaskKey;
    fn request(n:u8,op:O)->WireRequest {WireRequest::empty([n;32],[7;32],1,op)}
    fn controller(worker:&AgentWorker<OrdinaryOwner>,owner:HostBinding)->ProductSessionController {
        ProductSessionController::from_original(TaskKey::from_bytes(&[7;32]).unwrap(),owner,worker.native_command_queue())
    }
    fn read(slot:&mut crate::agent_product_control::NativeSlot,owner:HostBinding,token:u64,next:&mut u8)
        ->crate::agent_product_control::Reply {
        let deadline=Instant::now()+Duration::from_secs(5);
        loop {
            *next=next.checked_add(1).unwrap();let mut q=request(*next,O::Read);q.submission=token;
            let r=testing::execute(slot,&q,owner);
            if r.status!=S::Pending {return r}
            assert!(Instant::now()<deadline,"bounded ordinary worker reply timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn real_worker_typed_create_writer_append_snapshot_and_final_gate() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);assert!(context.port().is_none());
        let binding=owner.runtime.binding();let mut worker=spawn(owner,package,context);
        let mut slot=testing::attached(controller(&worker,binding));let mut next=1;
        let opened=testing::execute(&mut slot,&request(next,O::Open),binding);
        assert_eq!(opened.status,S::Submitted);
        // Observation traffic cannot exhaust effect-ID history or prevent the
        // original handle from being read and its rights explicitly reduced.
        for i in 0..80u64 {let mut q=request(240,O::Status);q.id[..8].copy_from_slice(&i.to_le_bytes());
            let observed=testing::execute(&mut slot,&q,binding);assert_eq!(observed.status,S::Observed);assert_eq!(observed.submission,opened.submission);}
        let opened=read(&mut slot,binding,opened.submission,&mut next);assert_eq!(opened.status,S::Opened);
        let generation=opened.generation;
        next+=1;let mut create=request(next,O::Submit);create.generation=generation;create.action=A::Create;create.session_id="session".into();
        let token=testing::execute(&mut slot,&create,binding).submission;
        let created=read(&mut slot,binding,token,&mut next);assert_eq!(created.status,S::Reply);
        assert!(matches!(created.outcome,Some(morrow_agent_session_exec_v1_r2::Outcome::Session(_))));
        next+=1;let mut admit=request(next,O::AdmitWriter);admit.session_id="session".into();
        let token=testing::execute(&mut slot,&admit,binding).submission;
        let writer=read(&mut slot,binding,token,&mut next);assert_eq!(writer.status,S::WriterAdmitted);assert_ne!(writer.writer_slot,0);
        next+=1;let mut open_writer=request(next,O::Submit);open_writer.generation=writer.generation;open_writer.action=A::OpenWriter;
        open_writer.writer_slot=writer.writer_slot;open_writer.session_id="session".into();
        let token=testing::execute(&mut slot,&open_writer,binding).submission;
        let reply=read(&mut slot,binding,token,&mut next);
        let epoch=match reply.outcome {Some(morrow_agent_session_exec_v1_r2::Outcome::Session(s))=>s.epoch,_=>panic!("expected genuine writer session reply")};
        next+=1;let mut append=request(next,O::Submit);append.generation=writer.generation;append.action=A::Append;append.writer_slot=writer.writer_slot;
        append.session_id="session".into();append.epoch=epoch;
        append.events=vec![morrow_agent_session_exec_v1_r2::Event{event_id:"wire-event".into(),body:b"ordinary fixture".to_vec()}];
        let token=testing::execute(&mut slot,&append,binding).submission;
        assert_eq!(read(&mut slot,binding,token,&mut next).status,S::Reply);
        next+=1;let mut snap=request(next,O::Submit);snap.generation=writer.generation;snap.action=A::Snapshot;snap.writer_slot=writer.writer_slot;
        snap.session_id="session".into();snap.limit=16;
        let token=testing::execute(&mut slot,&snap,binding).submission;
        let reply=read(&mut slot,binding,token,&mut next);
        assert!(matches!(&reply.outcome,Some(morrow_agent_session_exec_v1_r2::Outcome::Snapshot(s))if s.events.len()==1 && s.events[0].event.body==b"ordinary fixture"));
        next+=1;let mut close=request(next,O::CloseWriter);close.writer_slot=writer.writer_slot;
        assert_eq!(testing::execute(&mut slot,&close,binding).status,S::Observed);
        next+=1;let mut delivered=request(next,O::Read);delivered.submission=token;
        let unknown=testing::delivery_after_revoke(&mut slot,&delivered,reply);assert!(!unknown.is_empty());
        next+=1;let mut resend=create.clone();resend.id=[next;32];
        assert_eq!(testing::execute(&mut slot,&resend,binding).status,S::Unknown);
        next+=1;let detached=testing::execute(&mut slot,&request(next,O::Detach),binding);assert_eq!(detached.status,S::Detached);assert!(detached.delivery_unknown);
        drop(slot);worker.stop();let exit=join(&mut worker);assert_eq!(exit.owner.runtime.binding(),binding);
    }
    #[test]
    fn wrong_attachment_token_and_duplicate_ids_do_not_consume_original_pending() {
        let _serial=SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        let (owner,package,context)=fixture_with_process(false);let binding=owner.runtime.binding();
        let mut worker=spawn(owner,package,context);let mut slot=testing::attached(controller(&worker,binding));
        let original=request(1,O::Open);let submitted=testing::execute(&mut slot,&original,binding);
        assert_eq!(submitted.status,S::Submitted);assert_eq!(testing::execute(&mut slot,&original,binding).status,S::Invalid);
        let mut stale=request(2,O::Read);stale.attachment=2;stale.submission=submitted.submission;
        assert_eq!(testing::execute(&mut slot,&stale,binding).status,S::Invalid);
        stale.id=[3;32];stale.attachment=1;stale.submission=submitted.submission+1;
        assert_eq!(testing::execute(&mut slot,&stale,binding).status,S::Invalid);
        let mut next=3;assert_eq!(read(&mut slot,binding,submitted.submission,&mut next).status,S::Opened);
        next+=1;let mut consumed=request(next,O::Read);consumed.submission=submitted.submission;
        assert_eq!(testing::execute(&mut slot,&consumed,binding).status,S::Invalid);
        drop(slot);worker.stop();let exit=join(&mut worker);assert_eq!(exit.owner.runtime.binding(),binding);
    }

    #[test]
    fn product_private_capnp_interop_vectors_are_create_new_synthetic_fixtures() {
        use crate::agent_product_control::{Reply as WireReply,digest,MAX_REQUEST_BYTES,MAX_RESPONSE_BYTES};
        use morrow_agent_session_exec_v1_r2::{self as r2,SessionInfo,SessionSnapshot,StoredEvent,Event,Outcome};
        use serde_json::json;
        use std::io::Write;
        fn hex(b:&[u8])->String {b.iter().map(|b|format!("{b:02x}")).collect()}
        const HI:u64=0x9234_5678_9abc_def0;
        let attach=WireRequest::empty([1;32],[7;32],0,O::Attach);
        let mut create=WireRequest::empty([2;32],[7;32],HI,O::Submit);create.generation=HI+1;create.action=A::Create;
        create.session_id="session".into();create.parent="parent".into();create.parent_tail=HI+2;
        let mut admit=WireRequest::empty([3;32],[7;32],HI,O::AdmitWriter);admit.session_id="session".into();
        let mut open=WireRequest::empty([4;32],[7;32],HI,O::Submit);open.generation=HI+1;open.writer_slot=HI+3;
        open.action=A::OpenWriter;open.session_id="session".into();open.epoch=HI+4;
        let mut append=WireRequest::empty([5;32],[7;32],HI,O::Submit);append.generation=HI+1;append.writer_slot=HI+3;
        append.action=A::Append;append.session_id="session".into();append.epoch=HI+4;append.expected_tail=HI+5;
        append.events=vec![Event{event_id:"event-one".into(),body:vec![0,127,128,255]},Event{event_id:"event-two".into(),body:"合成".as_bytes().to_vec()}];
        let mut snapshot=WireRequest::empty([6;32],[7;32],HI,O::Submit);snapshot.generation=HI+1;snapshot.writer_slot=HI+3;
        snapshot.action=A::Snapshot;snapshot.session_id="session".into();snapshot.after=HI+3;snapshot.limit=16;
        let mut read_admit=WireRequest::empty([8;32],[7;32],HI,O::Read);read_admit.submission=HI+6;
        let mut read_snapshot=WireRequest::empty([9;32],[7;32],HI,O::Read);read_snapshot.submission=HI+7;
        let mut large_append=append.clone();large_append.id=[10;32];
        large_append.events=vec![Event{event_id:"large-event".into(),body:vec![0x5a;12*1024]}];
        let large_bytes=large_append.encode().unwrap();assert!(large_bytes.len()>8*1024 && large_bytes.len()<32*1024);
        // Capnp segment table: exact one segment, also for the large request.
        assert_eq!(&large_bytes[8..12],&0u32.to_le_bytes());
        let requests=[("attach",attach.clone()),("create",create),("admitWriter",admit),("openWriter",open),
            ("append",append),("largeAppend",large_append),("snapshot",snapshot.clone()),("readAdmit",read_admit.clone()),("readSnapshot",read_snapshot.clone())];
        let request_vectors:Vec<_>=requests.iter().map(|(label,q)| {
            let bytes=q.encode().unwrap();assert!(WireRequest::decode(&bytes).unwrap()==*q);
            json!({"label":label,"hex":hex(&bytes),"idHex":hex(&q.id),"taskHex":hex(&q.task),"attachment":q.attachment.to_string(),
                "operation":q.operation as u16,"submission":q.submission.to_string(),"writerSlot":q.writer_slot.to_string(),"generation":q.generation.to_string(),
                "action":q.action as u16,"sessionId":q.session_id,"parent":q.parent,"parentTail":q.parent_tail.to_string(),"after":q.after.to_string(),
                "limit":q.limit,"epoch":q.epoch.to_string(),"expectedTail":q.expected_tail.to_string(),
                "events":q.events.iter().map(|e|json!({"eventId":e.event_id,"bodyHex":hex(&e.body)})).collect::<Vec<_>>()})
        }).collect();
        let mut attached=WireReply::new(S::Attached);attached.attachment=HI;attached.lease_live=true;
        let mut admitted=WireReply::new(S::WriterAdmitted);admitted.attachment=HI;admitted.submission=HI+6;admitted.writer_slot=HI+3;
        admitted.generation=HI+1;admitted.lease_live=true;
        let checkpoint=b"synthetic checkpoint".to_vec();
        let info=SessionInfo{session_id:"session".into(),parent:Some("parent".into()),parent_tail:HI+2,epoch:HI+4,tail:HI+5,
            floor:HI+5,archived:false,revision:HI+8,checkpoint_tail:HI+4,checkpoint_sha256:r2::hash(&checkpoint),checkpoint_sealed:true,
            parent_checkpoint_sha256:Some([3;32])};
        let view=SessionSnapshot{info,events:vec![StoredEvent{sequence:HI+5,event:Event{event_id:"event-one".into(),body:vec![0,127,128,255]}}],gap:true,checkpoint};
        let original=snapshot.canonical_request().unwrap();let canonical=r2::Reply::new(&original,Outcome::Snapshot(view.clone())).unwrap();
        let mut result=WireReply::new(S::Reply);result.attachment=HI;result.submission=HI+7;result.writer_slot=0;
        result.generation=HI+1;result.lease_live=true;result.canonical_reply=canonical.encode().unwrap();
        result.canonical_request_sha256=canonical.request_sha256;result.outcome=Some(Outcome::Snapshot(view.clone()));
        let replies=[("attached","attach",&attach,attached),("writerAdmitted","readAdmit",&read_admit,admitted),
            ("snapshot","readSnapshot",&read_snapshot,result)];
        let reply_vectors:Vec<_>=replies.iter().map(|(label,request_label,q,r)| {
            let bytes=q.encode().unwrap();let reply=r.encode_for(q,&bytes).unwrap();
            json!({"label":label,"requestLabel":request_label,"hex":hex(&reply),"status":r.status as u16,"attachment":r.attachment.to_string(),
                "submission":r.submission.to_string(),"writerSlot":r.writer_slot.to_string(),"generation":r.generation.to_string(),"leaseLive":r.lease_live,
                "deliveryUnknown":r.delivery_unknown,"requestSha256":hex(&r2::hash(&bytes)),"canonicalRequestSha256":hex(&r.canonical_request_sha256),
                "canonicalReplyHex":hex(&r.canonical_reply)})
        }).collect();
        let document=json!({"schema":"PS30_SYNTHETIC_INTEROP_VECTORS_V1","scope":"codec fixtures, no authority or execution grant",
            "schemaSha256":hex(&digest()),"maxRequestBytes":MAX_REQUEST_BYTES,"maxResponseBytes":MAX_RESPONSE_BYTES,
            "requests":request_vectors,"replies":reply_vectors,"snapshotExpected":{
                "sessionId":view.info.session_id,"parent":view.info.parent,"parentTail":view.info.parent_tail.to_string(),"epoch":view.info.epoch.to_string(),
                "tail":view.info.tail.to_string(),"floor":view.info.floor.to_string(),"archived":view.info.archived,"revision":view.info.revision.to_string(),
                "checkpointTail":view.info.checkpoint_tail.to_string(),"checkpointSha256":hex(&view.info.checkpoint_sha256),
                "checkpointSealed":view.info.checkpoint_sealed,"parentCheckpointSha256":hex(&[3;32]),
                "gap":view.gap,"checkpointHex":hex(&view.checkpoint),"events":[{"sequence":(HI+5).to_string(),"eventId":"event-one","bodyHex":"007f80ff"}]}});
        let directory=std::env::temp_dir().join("ps30-interop-vectors");std::fs::create_dir(&directory).unwrap();
        let path=directory.join("vectors.json");let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.write_all(&serde_json::to_vec_pretty(&document).unwrap()).unwrap();file.sync_all().unwrap();
        println!("ps30 synthetic interop vectors: {}",path.display());
    }
}
