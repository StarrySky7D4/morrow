//! S-only PE table lookup from the original already-held image file.
//! No reopen, loader call, self hash, arbitrary file selector or JSON table.
use std::{fs::File,io,os::windows::fs::FileExt};
use crate::asuser_a1_protocol::{RoleTable,TABLE_BYTES};
fn bad()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"A1 S-only PE table rejected")}
fn exact(file:&File,buffer:&mut[u8],offset:u64)->io::Result<()> {
    let mut done=0;
    while done<buffer.len() {
        let n=file.seek_read(&mut buffer[done..],offset.checked_add(done as u64).ok_or_else(bad)?)?;
        if n==0{return Err(io::Error::new(io::ErrorKind::UnexpectedEof,"A1 held S PE truncated"));}
        done+=n;
    }
    Ok(())
}
pub(crate) fn from_held_file(file:&File)->io::Result<RoleTable> {
    let size=file.metadata()?.len();if size>512*1024*1024{return Err(bad());}
    let mut dos=[0;64];exact(file,&mut dos,0)?;
    if dos[..2]!=*b"MZ"{return Err(bad());}
    let pe=u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if pe<64||pe>size.saturating_sub(24){return Err(bad());}
    let mut coff=[0;24];exact(file,&mut coff,pe)?;
    if coff[..4]!=*b"PE\0\0"||coff[4..6]!=0x8664u16.to_le_bytes(){return Err(bad());}
    let count=u16::from_le_bytes(coff[6..8].try_into().unwrap()) as usize;
    let optional=u16::from_le_bytes(coff[20..22].try_into().unwrap()) as u64;
    if count==0||count>96||optional<112||optional>4096{return Err(bad());}
    let mut magic=[0;2];exact(file,&mut magic,pe+24)?;
    if magic!=0x20bu16.to_le_bytes(){return Err(bad());}
    let base=pe.checked_add(24+optional).ok_or_else(bad)?;
    let end=base.checked_add(count as u64*40).ok_or_else(bad)?;
    if end>size{return Err(bad());}
    let mut found=None;
    for index in 0..count {
        let mut row=[0;40];exact(file,&mut row,base+index as u64*40)?;
        if row[..8]!=*b".c28a1\0\0"{continue;}
        if found.is_some(){return Err(bad());}
        found=Some(section(&row,end,size)?);
    }
    let (offset,length)=found.ok_or_else(bad)?;
    let mut bytes=[0u8;4096];exact(file,&mut bytes[..length],offset)?;
    if bytes[TABLE_BYTES..length].iter().any(|v|*v!=0){return Err(bad());}
    RoleTable::decode(&bytes[..TABLE_BYTES])
}
fn section(row:&[u8;40],end:u64,total:u64)->io::Result<(u64,usize)> {
    let u=|o|u32::from_le_bytes(row[o..o+4].try_into().unwrap());
    let virtual_size=u(8);let length=u(16) as usize;let offset=u(20) as u64;let flags=u(36);
    if virtual_size!=TABLE_BYTES as u32||length<TABLE_BYTES||length>4096
        ||offset<end||offset.checked_add(length as u64).is_none_or(|v|v>total)
        ||flags&0x40000040!=0x40000040||flags&0xa0000000!=0 {return Err(bad());}
    Ok((offset,length))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn s_table_section_rejects_writable_executable_overlap_and_unbounded_payloads() {
        let mut row=[0;40];row[8..12].copy_from_slice(&80u32.to_le_bytes());
        row[16..20].copy_from_slice(&512u32.to_le_bytes());row[20..24].copy_from_slice(&1024u32.to_le_bytes());
        row[36..40].copy_from_slice(&0x40000040u32.to_le_bytes());
        assert_eq!(section(&row,512,2048).unwrap(),(1024,512));
        for flags in [0xc0000040u32,0x60000040,0x40] {
            let mut b=row;b[36..40].copy_from_slice(&flags.to_le_bytes());assert!(section(&b,512,2048).is_err());
        }
        let mut b=row;b[16..20].copy_from_slice(&8192u32.to_le_bytes());assert!(section(&b,512,2048).is_err());
        assert!(section(&row,1100,2048).is_err());assert!(section(&row,512,1500).is_err());
    }
}

