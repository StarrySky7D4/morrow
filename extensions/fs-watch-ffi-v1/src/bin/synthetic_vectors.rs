//! Explicit synthetic protocol vectors, not native observations or authority.
use morrow_fs_watch_v1::{Batch, Change, End, Kind};
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 {
        return Err("requires one new output directory".into());
    }
    let destination = PathBuf::from(&args[1]);
    fs::create_dir(&destination)?;
    let mut count = 0;
    let terminal = Batch {
        watch_epoch: [0x42; 32],
        sequence: 6,
        changes: Vec::new(),
        end: End::WindowEnded,
    };
    fs::write(destination.join("after-events.capnp"), terminal.encode()?)?;
    count += 1;
    for end in [
        End::WindowEnded,
        End::Gap,
        End::Revoked,
        End::Cancelled,
        End::ObservationFailed,
    ] {
        let batch = Batch {
            watch_epoch: [0x43; 32],
            sequence: 1,
            changes: Vec::new(),
            end,
        };
        fs::write(
            destination.join(format!("terminal-{}.capnp", end as u16)),
            batch.encode()?,
        )?;
        count += 1;
    }
    let unpaired = Batch {
        watch_epoch: [0x44; 32],
        sequence: 1,
        changes: vec![Change {
            kind: Kind::Added,
            name_utf16_le: vec![0x00, 0xd8, 0x61, 0x00],
        }],
        end: End::Continue,
    };
    fs::write(destination.join("unpaired.capnp"), unpaired.encode()?)?;
    count += 1;
    println!("SYNTHETIC_WATCH_VECTORS count={count} native_observations=0");
    Ok(())
}
