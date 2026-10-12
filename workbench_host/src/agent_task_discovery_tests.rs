//! Ordinary pure facts/codec/gate fixtures only. No Store, worker, thread,
//! preparation, session authority, native endpoint, process or VM is created.
use super::*;
fn key() -> TaskKey { TaskKey::from_bytes(&[7; 32]).unwrap() }
fn request() -> Request { Request { id: [3; 32] } }

#[test]
fn absent_task_requires_retained_original_owner() {
    assert_eq!(classify(false, true, None), Observation { kind: Kind::None, task: None });
    assert_eq!(classify(false, false, None).kind, Kind::Unavailable);
}
#[test]
fn running_means_retained_original_worker_not_permission_or_native_execution() {
    let value = classify(false, false, Some((key(), true, false)));
    assert_eq!(value, Observation { kind: Kind::RunningAgent, task: Some(key()) });
    // No lease/grant/endpoint/OS/permission field exists in this observation.
}
#[test]
fn retained_agent_exit_is_same_current_key_and_original_owner_present() {
    assert_eq!(classify(false, true, Some((key(), false, true))),
        Observation { kind: Kind::RetainedAgentExit, task: Some(key()) });
    assert_eq!(classify(false, false, Some((key(), false, true))).kind, Kind::Unavailable);
}
#[test]
fn other_tasks_never_expose_their_keys() {
    for owner in [true, false] {
        assert_eq!(classify(false, owner, Some((key(), false, false))),
            Observation { kind: Kind::OtherTask, task: None });
    }
}
#[test]
fn lost_or_gate_closed_never_discloses_retained_identity() {
    for owner in [true, false] {
        for flags in [(true,false),(false,true),(false,false)] {
            assert_eq!(classify(true, owner, Some((key(), flags.0, flags.1))),
                Observation { kind: Kind::Unavailable, task: None });
        }
    }
}
#[test]
fn inconsistent_owner_worker_exit_and_zero_key_are_unavailable() {
    for facts in [(true,true,false),(false,true,true),(true,true,true)] {
        assert_eq!(classify(false, facts.0, Some((key(), facts.1, facts.2))).kind, Kind::Unavailable);
    }
    let zero = TaskKey::from_bytes(&[0;32]).unwrap();
    assert_eq!(classify(false, false, Some((zero,true,false))).kind, Kind::Unavailable);
}
#[test]
fn canonical_request_and_all_five_bounded_observations_round_trip() {
    let q=request();let input=q.encode().unwrap();assert_eq!(Request::decode(&input).unwrap(),q);
    for observation in [classify(false,true,None),classify(false,false,Some((key(),true,false))),
        classify(false,true,Some((key(),false,true))),classify(false,false,Some((key(),false,false))),
        classify(true,true,None)] {
        let output=observation.encode_for(q,&input).unwrap();assert!(output.len()<MAX_FRAME_BYTES);
        assert_eq!(Observation::decode_for(q,&input,&output).unwrap(),observation);
    }
}
#[test]
fn reply_key_presence_is_restricted_to_exact_agent_lifecycles() {
    let q=request();let input=q.encode().unwrap();
    for kind in [Kind::None,Kind::OtherTask,Kind::Unavailable] {
        assert!(Observation{kind,task:Some(key())}.encode_for(q,&input).is_err());
    }
    for kind in [Kind::RunningAgent,Kind::RetainedAgentExit] {
        assert!(Observation{kind,task:None}.encode_for(q,&input).is_err());
    }
}
#[test]
fn zero_request_id_trailing_truncated_oversized_and_wrong_profile_rejected() {
    assert!(Request{id:[0;32]}.encode().is_err());
    let input=request().encode().unwrap();
    let mut wrong=input.clone();wrong[0]^=1;assert!(Request::decode(&wrong).is_err());
    let mut trailing=input.clone();trailing.extend([0;8]);assert!(Request::decode(&trailing).is_err());
    assert!(Request::decode(&input[..input.len()-1]).is_err());
    assert!(Request::decode(&vec![0;MAX_FRAME_BYTES+1]).is_err());
}
#[test]
fn reply_correlates_request_identity_digest_and_actual_request_hash() {
    let q=request();let input=q.encode().unwrap();
    let observation=classify(false,false,Some((key(),true,false)));
    let output=observation.encode_for(q,&input).unwrap();
    let different=Request{id:[4;32]};let alternate=different.encode().unwrap();
    assert!(Observation::decode_for(different,&alternate,&output).is_err());
    for needle in [digest(), q.id, <[u8;32]>::from(Sha256::digest(&input))] {
        let at=output.windows(32).position(|v|v==needle).unwrap();
        let mut changed=output.clone();changed[at]^=1;
        assert!(Observation::decode_for(q,&input,&changed).is_err());
    }
}
#[test]
fn response_cap_and_canonical_trailing_data_rejected() {
    let q=request();let input=q.encode().unwrap();
    let mut output=classify(false,true,None).encode_for(q,&input).unwrap();
    output.extend([0;8]);assert!(Observation::decode_for(q,&input,&output).is_err());
    assert!(Observation::decode_for(q,&input,&vec![0;MAX_FRAME_BYTES+1]).is_err());
}
#[test]
fn live_gate_delivers_little_endian_frame_and_closed_gate_delivers_nothing() {
    let gate=crate::product_gate::ProductGate::default();let mut output=Vec::new();
    deliver_native_reply(&mut output,b"observation",&gate).unwrap();
    assert_eq!(&output[..4],&(11u32).to_le_bytes());assert_eq!(&output[4..],b"observation");
    gate.revoke();output.clear();assert!(deliver_native_reply(&mut output,b"observation",&gate).is_err());
    assert!(output.is_empty());
}
#[test]
fn each_write_and_flush_failure_is_preserved_without_retry() {
    struct Broken { calls:usize, fail_at:usize }
    impl std::io::Write for Broken {
        fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.calls+=1;if self.calls==self.fail_at {Err(std::io::Error::other("synthetic"))}else{Ok(b.len())}}
        fn flush(&mut self)->std::io::Result<()>{self.calls+=1;if self.calls==self.fail_at {Err(std::io::Error::other("synthetic"))}else{Ok(())}}
    }
    for fail_at in 1..=3 {
        let mut output=Broken{calls:0,fail_at};
        assert!(deliver_native_reply(&mut output,b"observation",&crate::product_gate::ProductGate::default()).is_err());
        assert_eq!(output.calls,fail_at);
    }
}
#[test]
fn revoke_between_header_and_body_prevents_observation_body_delivery() {
    struct Revoke { gate:crate::product_gate::ProductGate, bytes:Vec<u8> }
    impl std::io::Write for Revoke {
        fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.bytes.extend(b);self.gate.revoke();Ok(b.len())}
        fn flush(&mut self)->std::io::Result<()>{panic!("flush after revoked header")}
    }
    let gate=crate::product_gate::ProductGate::default();let mut output=Revoke{gate:gate.clone(),bytes:Vec::new()};
    assert!(deliver_native_reply(&mut output,b"observation",&gate).is_err());
    assert_eq!(output.bytes.len(),4);
}

#[test]
fn private_observation_interop_vectors_are_create_new_synthetic_fixtures() {
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes { write!(&mut out, "{byte:02x}").unwrap(); }
        out
    }
    fn create(path: &std::path::Path, bytes: &[u8]) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    let q = request();
    let input = q.encode().unwrap();
    assert_eq!(Request::decode(&input).unwrap(), q);
    let observations = [
        ("none", classify(false, true, None)),
        ("runningAgent", classify(false, false, Some((key(), true, false)))),
        ("retainedAgentExit", classify(false, true, Some((key(), false, true)))),
        ("otherTask", classify(false, false, Some((key(), false, false)))),
        ("unavailable", classify(true, true, None)),
    ];
    let mut fixtures = Vec::new();
    for (name, observation) in observations {
        let output = observation.encode_for(q, &input).unwrap();
        assert_eq!(Observation::decode_for(q, &input, &output).unwrap(), observation);
        fixtures.push((name, observation, output));
    }
    let Some(root) = std::env::var_os("MORROW_AGENT_DISCOVERY_FIXTURES") else { return; };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute(), "synthetic fixture output must be absolute");
    // One caller-selected new fixture directory; never adopt or clean an existing path.
    std::fs::create_dir(&root).unwrap();
    create(&root.join("request.bin"), &input);
    let entries = fixtures.iter().map(|(name, observation, output)| {
        create(&root.join(format!("{name}.reply.bin")), output);
        format!("{{\"kind\":\"{name}\",\"taskHex\":\"{}\",\"replyHex\":\"{}\"}}",
            observation.task.as_ref().map_or(String::new(), |v| hex(v.as_bytes())), hex(output))
    }).collect::<Vec<_>>().join(",\n");
    let json = format!("{{\n\"schema\":\"STAGE31_AGENT_DISCOVERY_SYNTHETIC_INTEROP_V1\",\n\"version\":{VERSION},\n\"schemaSha256\":\"{}\",\n\"requestIdHex\":\"{}\",\n\"requestHex\":\"{}\",\n\"replies\":[{}]\n}}\n",
        hex(&digest()), hex(&q.id), hex(&input), entries);
    create(&root.join("vectors.json"), json.as_bytes());
}
