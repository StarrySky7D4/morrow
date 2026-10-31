//! Bounded FILE_NOTIFY_INFORMATION decoding. No path opens or rename pairing.
use morrow_fs_watch_v1::{Batch, Change, End, Error, Kind, MAX_CHANGES, MAX_NAME_BYTES};

pub const BUFFER_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notification {
    Changes(Vec<Change>),
    Gap,
}

/// All records are validated before any observation is returned. A bounded
/// output overflow becomes an explicit gap, never a silently truncated list.
pub fn decode(bytes: &[u8]) -> Result<Notification, Error> {
    if bytes.is_empty() {
        return Ok(Notification::Gap);
    }
    if bytes.len() > BUFFER_BYTES {
        return Err(Error::Limit);
    }
    let mut spans = Vec::new();
    spans
        .try_reserve_exact(MAX_CHANGES)
        .map_err(|_| Error::Allocation)?;
    let mut offset = 0usize;
    loop {
        let header = bytes.get(offset..offset + 12).ok_or(Error::Invalid)?;
        let next = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let action = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let kind = Kind::try_from(u16::try_from(action).map_err(|_| Error::Invalid)?)?;
        if length == 0 || length > MAX_NAME_BYTES || !length.is_multiple_of(2) {
            return Err(Error::Invalid);
        }
        let start = offset.checked_add(12).ok_or(Error::Invalid)?;
        let end = start.checked_add(length).ok_or(Error::Invalid)?;
        let name = bytes.get(start..end).ok_or(Error::Invalid)?;
        if name == [b'.', 0]
            || name == [b'.', 0, b'.', 0]
            || name
                .chunks_exact(2)
                .any(|x| matches!(u16::from_le_bytes([x[0], x[1]]), 0 | 0x2f | 0x5c | 0x3a))
        {
            return Err(Error::Invalid);
        }
        if spans.len() == MAX_CHANGES {
            return Ok(Notification::Gap);
        }
        spans.push((kind, start, end));
        if next == 0 {
            if bytes.len() - end > 3 {
                return Err(Error::Invalid);
            }
            break;
        }
        if !next.is_multiple_of(4) || next < 12 + length {
            return Err(Error::Invalid);
        }
        offset = offset.checked_add(next).ok_or(Error::Invalid)?;
        if offset >= bytes.len() {
            return Err(Error::Invalid);
        }
    }
    let mut changes = Vec::new();
    changes
        .try_reserve_exact(spans.len())
        .map_err(|_| Error::Allocation)?;
    for (kind, start, end) in spans {
        let mut name = Vec::new();
        name.try_reserve_exact(end - start)
            .map_err(|_| Error::Allocation)?;
        name.extend_from_slice(&bytes[start..end]);
        changes.push(Change {
            kind,
            name_utf16_le: name,
        });
    }
    // The transport's independently pinned validator remains the final check.
    let bounded = Batch {
        watch_epoch: [1; 32],
        sequence: 1,
        changes,
        end: End::Continue,
    };
    bounded.validate()?;
    Ok(Notification::Changes(bounded.changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(values: &[(u32, &[u16])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (index, (action, name)) in values.iter().enumerate() {
            let length = name.len() * 2;
            let padded = (12 + length + 3) & !3;
            let next = if index + 1 == values.len() {
                0
            } else {
                padded as u32
            };
            out.extend(next.to_le_bytes());
            out.extend(action.to_le_bytes());
            out.extend((length as u32).to_le_bytes());
            for unit in *name {
                out.extend(unit.to_le_bytes());
            }
            out.resize(out.len() + padded - 12 - length, 0xa5);
        }
        out
    }

    #[test]
    fn distinct_native_actions_and_unpaired_utf16_are_preserved() {
        let raw = records(&[
            (1, &[97]),
            (2, &[98]),
            (3, &[99]),
            (4, &[0xd800, 97]),
            (5, &[0xdfff, 98]),
        ]);
        let Notification::Changes(items) = decode(&raw).unwrap() else {
            panic!()
        };
        assert_eq!(items.len(), 5);
        assert_eq!(
            items.iter().map(|x| x.kind as u16).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(items[3].name_utf16_le, [0, 0xd8, 97, 0]);
        assert_eq!(items[4].name_utf16_le, [0xff, 0xdf, 98, 0]);
    }
    #[test]
    fn an_unpaired_rename_is_not_synthesized_or_dropped() {
        let Notification::Changes(items) = decode(&records(&[(4, &[97])])).unwrap() else {
            panic!()
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, Kind::RenameOldName);
    }
    #[test]
    fn zero_completion_and_bounded_output_overflow_are_explicit_gaps() {
        assert_eq!(decode(&[]), Ok(Notification::Gap));
        let values = vec![(1, [97u16].as_slice()); MAX_CHANGES + 1];
        assert_eq!(decode(&records(&values)), Ok(Notification::Gap));
    }
    #[test]
    fn invalid_offsets_never_skip_into_unrelated_storage() {
        for next in [1u32, 12, 15, 19, 0xffff_fffc] {
            let mut raw = records(&[(1, &[97]), (2, &[98])]);
            raw[..4].copy_from_slice(&next.to_le_bytes());
            assert_eq!(decode(&raw), Err(Error::Invalid));
        }
    }
    #[test]
    fn truncated_header_name_and_extra_tail_are_rejected() {
        let raw = records(&[(1, &[97, 98, 99])]);
        for size in 1..18 {
            assert!(decode(&raw[..size]).is_err());
        }
        let mut extra = raw.clone();
        extra.extend([0; 4]);
        assert_eq!(decode(&extra), Err(Error::Invalid));
        assert_eq!(decode(&vec![0; BUFFER_BYTES + 1]), Err(Error::Limit));
    }
    #[test]
    fn unsupported_action_and_non_component_names_are_rejected() {
        for action in [0, 6, 65537, u32::MAX] {
            assert_eq!(decode(&records(&[(action, &[97])])), Err(Error::Invalid));
        }
        for name in [
            vec![0],
            vec![46],
            vec![46, 46],
            vec![97, 47, 98],
            vec![97, 92, 98],
            vec![97, 58, 98],
            vec![],
        ] {
            assert_eq!(decode(&records(&[(1, &name)])), Err(Error::Invalid));
        }
    }
    #[test]
    fn malformed_later_record_never_returns_a_valid_prefix() {
        let raw = records(&[(1, &[97]), (9, &[98])]);
        assert_eq!(decode(&raw), Err(Error::Invalid));
    }
    #[test]
    fn name_length_must_be_even_bounded_and_inside_the_record() {
        for length in [1u32, 1026, 0xffff_fffe] {
            let mut raw = records(&[(1, &[97])]);
            raw[8..12].copy_from_slice(&length.to_le_bytes());
            assert_eq!(decode(&raw), Err(Error::Invalid));
        }
    }
}
