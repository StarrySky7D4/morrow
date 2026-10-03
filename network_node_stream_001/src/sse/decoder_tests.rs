use super::{Decoder, DecoderLimits, Error, Event, Finish};

fn decode(chunks: &[&[u8]]) -> (Vec<Event>, Finish) {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    let mut events = Vec::new();
    for chunk in chunks {
        let mut remaining = *chunk;
        while !remaining.is_empty() {
            let step = decoder.feed(remaining).unwrap();
            assert!(step.consumed > 0 && step.consumed <= remaining.len());
            if step.event.is_none() {
                assert_eq!(step.consumed, remaining.len());
            }
            remaining = &remaining[step.consumed..];
            if let Some(event) = step.event {
                events.push(event);
            }
        }
    }
    (events, decoder.finish().unwrap())
}
fn small() -> DecoderLimits {
    DecoderLimits {
        max_line_bytes: 8,
        max_event_bytes: 16,
        max_total_bytes: 128,
        max_events: 8,
        max_id_bytes: 4,
        max_retry_digits: 3,
    }
}
fn expect_error(decoder: &mut Decoder, input: &[u8], error: Error) {
    assert_eq!(decoder.feed(input).err(), Some(error));
    assert_eq!(decoder.feed(b"data: later\n\n").err(), Some(error));
    assert_eq!(decoder.finish(), Err(error));
    assert_eq!(decoder.finish(), Err(error));
}

#[test]
fn every_split_preserves_bom_utf8_crlf_multiline_and_metadata() {
    let input = "\u{feff}id: first\r\nevent: update\r\ndata: 你好\r\ndata: second\r\nretry: 123\r\n\r\ndata: next\r\n\r\n".as_bytes();
    for split in 0..=input.len() {
        let (events, finish) = decode(&[&input[..split], &input[split..]]);
        assert_eq!(events.len(), 2, "split={split}");
        assert_eq!(events[0].kind, "update");
        assert_eq!(events[0].data, "你好\nsecond");
        assert_eq!(events[0].id, "first");
        assert_eq!(events[0].retry, Some(123));
        assert_eq!(events[1].kind, "message");
        assert_eq!(events[1].id, "first");
        assert_eq!(events[1].retry, Some(123));
        assert!(!finish.truncated);
    }
}

#[test]
fn one_byte_chunks_cover_all_line_endings_and_only_initial_bom() {
    let input = "\u{feff}data: a\r\rdata: b\n\ndata: \u{feff}c\r\n\r\n".as_bytes();
    let chunks: Vec<_> = input.chunks(1).collect();
    let (events, finish) = decode(&chunks);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].data, "a");
    assert_eq!(events[1].data, "b");
    assert_eq!(events[2].data, "\u{feff}c");
    assert!(!finish.truncated);
}

#[test]
fn feed_yields_one_event_and_leaves_exact_remainder_unparsed() {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    let input = b"data: one\n\ndata: two\n\n";
    let first = decoder.feed(input).unwrap();
    assert_eq!(first.consumed, 11);
    assert_eq!(first.event.unwrap().data, "one");
    let second = decoder.feed(&input[first.consumed..]).unwrap();
    assert_eq!(second.consumed, 11);
    assert_eq!(second.event.unwrap().data, "two");
    assert!(!decoder.finish().unwrap().truncated);
}

#[test]
fn delivered_event_does_not_validate_or_consume_next_event_early() {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    let input = b"data: good\n\ndata: \xff\n\n";
    let step = decoder.feed(input).unwrap();
    assert_eq!(step.event.unwrap().data, "good");
    expect_error(&mut decoder, &input[step.consumed..], Error::InvalidUtf8);
}

#[test]
fn field_rules_empty_data_type_reset_id_reset_nul_and_unknown() {
    let input = b"event: discarded\n\n: comment\nunknown: ignored\nid: kept\nevent: custom\nevent:\ndata\ndata:  two\n\nid: bad\0value\ndata: x\n\nid\ndata: y\n\n";
    let (events, finish) = decode(&[input]);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].kind, "message");
    assert_eq!(events[0].data, "\n two"); // Remove only one optional space.
    assert_eq!(events[0].id, "kept");
    assert_eq!(events[1].id, "kept"); // NUL-containing ID was ignored.
    assert_eq!(events[2].id, "");
    assert!(!finish.truncated);
}

#[test]
fn retry_is_persistent_inert_metadata_and_non_digits_are_ignored() {
    let input = b"retry: 123\ndata: first\n\nretry: -1\nretry: +2\nretry: 1 2\nretry:\ndata: next\n\nretry: 0\ndata: last\n\n";
    let (events, finish) = decode(&[input]);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].retry, Some(123));
    assert_eq!(events[1].retry, Some(123));
    assert_eq!(events[2].retry, Some(0));
    assert!(!finish.truncated);
}

#[test]
fn empty_input_is_a_zero_consumption_no_event_step() {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    let step = decoder.feed(b"").unwrap();
    assert_eq!(step.consumed, 0);
    assert!(step.event.is_none());
    assert_eq!(decoder.finish(), Ok(Finish { truncated: false }));
}

#[test]
fn eof_does_not_dispatch_any_unterminated_data_or_metadata_block() {
    for input in [b"data: x".as_slice(), b"data: x\n", b"data: x\r\n", b"id: pending\n", b": comment\n", b"unknown: value"] {
        let (events, finish) = decode(&[input]);
        assert!(events.is_empty());
        assert!(finish.truncated);
    }
    let (events, finish) = decode(&[b"data: complete\n\ndata: partial\n"]);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "complete");
    assert!(finish.truncated);
}

#[test]
fn eof_after_blank_cr_is_complete_and_later_lf_is_only_separator_tail() {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    let input = b"data: x\r\n\r\n";
    let step = decoder.feed(input).unwrap();
    assert_eq!(step.consumed, input.len() - 1);
    assert_eq!(step.event.unwrap().data, "x");
    let tail = decoder.feed(&input[step.consumed..]).unwrap();
    assert_eq!(tail.consumed, 1);
    assert!(tail.event.is_none());
    assert_eq!(decoder.finish(), Ok(Finish { truncated: false }));
}

#[test]
fn invalid_utf8_in_data_comments_unknown_fields_and_non_initial_bom_is_checked() {
    for input in [b"data: \xff\n\n".as_slice(), b": \xff\n", b"unknown: \xff\n", b"\xef\xbbx\n"] {
        let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
        expect_error(&mut decoder, input, Error::InvalidUtf8);
    }
}

#[test]
fn eof_rejects_half_utf8_even_in_discarded_fields_or_initial_bom() {
    for input in [b"data: \xe4\xbd".as_slice(), b": \xe4", b"unknown: \xf0\x9f", b"\xef", b"\xef\xbb"] {
        let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
        let step = decoder.feed(input).unwrap();
        assert_eq!(step.consumed, input.len());
        assert!(step.event.is_none());
        assert_eq!(decoder.finish(), Err(Error::InvalidUtf8));
        assert_eq!(decoder.feed(b"\n\n").err(), Some(Error::InvalidUtf8));
    }
}

#[test]
fn completed_initial_bom_alone_is_not_a_truncated_event() {
    let (events, finish) = decode(&[b"\xef", b"\xbb", b"\xbf"]);
    assert!(events.is_empty());
    assert!(!finish.truncated);
}

#[test]
fn raw_line_limit_is_inclusive_and_counts_utf8_bytes() {
    let mut limits = small();
    limits.max_line_bytes = 9;
    let mut decoder = Decoder::new(limits).unwrap();
    let step = decoder.feed("data: 你\n\n".as_bytes()).unwrap();
    assert_eq!(step.event.unwrap().data, "你");
    let mut decoder = Decoder::new(small()).unwrap();
    expect_error(&mut decoder, "data: 你\n\n".as_bytes(), Error::LineLimit);
}

#[test]
fn raw_block_limit_counts_ignored_comment_and_unknown_bytes() {
    let mut limits = small();
    limits.max_event_bytes = 12;
    for input in [b":123456\n:123456\n\n".as_slice(), b"x:12345\nx:12345\n\n"] {
        let mut decoder = Decoder::new(limits).unwrap();
        expect_error(&mut decoder, input, Error::EventLimit);
    }
}

#[test]
fn raw_block_limit_includes_blank_delimiter_at_exact_boundary() {
    let mut limits = small();
    limits.max_event_bytes = 9;
    let mut decoder = Decoder::new(limits).unwrap();
    assert_eq!(decoder.feed(b"data: x\n\n").unwrap().event.unwrap().data, "x");
    limits.max_event_bytes = 8;
    let mut decoder = Decoder::new(limits).unwrap();
    expect_error(&mut decoder, b"data: x\n\n", Error::EventLimit);
}

#[test]
fn total_raw_limit_counts_bom_crlf_and_empty_or_comment_blocks() {
    let mut limits = small();
    limits.max_total_bytes = 16;
    let mut decoder = Decoder::new(limits).unwrap();
    assert!(decoder.feed(b":\n\n:\n\n:\n\n:\n\n:\n\n\n").unwrap().event.is_none());
    expect_error(&mut decoder, b"\n", Error::TotalLimit);
    let limits = DecoderLimits { max_line_bytes: 1, max_event_bytes: 3, max_total_bytes: 3, max_events: 1, max_id_bytes: 1, max_retry_digits: 1 };
    let mut decoder = Decoder::new(limits).unwrap();
    assert!(decoder.feed(b"\xef\xbb\xbf").unwrap().event.is_none());
    expect_error(&mut decoder, b"\n", Error::TotalLimit);
    let mut limits = small();
    limits.max_total_bytes = 16;
    let mut decoder = Decoder::new(limits).unwrap();
    assert!(decoder.feed(b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n").unwrap().event.is_none());
    expect_error(&mut decoder, b"\n", Error::TotalLimit);
}

#[test]
fn event_count_limit_counts_empty_data_events_but_not_empty_blocks() {
    let mut limits = small();
    limits.max_events = 1;
    let mut decoder = Decoder::new(limits).unwrap();
    let first = decoder.feed(b"\n\n: x\n\ndata:\n\n").unwrap();
    assert_eq!(first.event.unwrap().data, "");
    expect_error(&mut decoder, b"data:\n\n", Error::EventCountLimit);
}

#[test]
fn id_limit_is_inclusive_and_nul_id_does_not_replace_saved_id() {
    let mut decoder = Decoder::new(small()).unwrap();
    assert!(decoder.feed(b"id: 1234\n\n").unwrap().event.is_none());
    assert!(decoder.feed(b"id: \0xxx\n\n").unwrap().event.is_none());
    let event = decoder.feed(b"data: x\n\n").unwrap().event.unwrap();
    assert_eq!(event.id, "1234");
    let mut limits = small();
    limits.max_line_bytes = 9;
    let mut decoder = Decoder::new(limits).unwrap();
    expect_error(&mut decoder, b"id: 12345\n", Error::IdLimit);
}

#[test]
fn valid_numeric_retry_has_digit_and_u64_limits() {
    let mut limits = small();
    limits.max_line_bytes = 16;
    limits.max_event_bytes = 32;
    let mut decoder = Decoder::new(limits).unwrap();
    expect_error(&mut decoder, b"retry: 1234\n", Error::RetryLimit);
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    expect_error(&mut decoder, b"retry: 18446744073709551616\n", Error::RetryLimit);
}

#[test]
fn invalid_limits_rejected_without_allocating_from_limits() {
    let limits = small();
    for invalid in [
        DecoderLimits { max_line_bytes: 0, ..limits },
        DecoderLimits { max_event_bytes: 7, ..limits },
        DecoderLimits { max_total_bytes: 15, ..limits },
        DecoderLimits { max_events: 0, ..limits },
        DecoderLimits { max_id_bytes: 9, ..limits },
        DecoderLimits { max_retry_digits: 0, ..limits },
        DecoderLimits { max_retry_digits: 21, ..limits },
    ] {
        assert_eq!(Decoder::new(invalid).err(), Some(Error::InvalidLimits));
    }
}

#[test]
fn finish_idempotent_and_later_feed_closed_error_is_fused() {
    let mut decoder = Decoder::new(DecoderLimits::default()).unwrap();
    assert!(decoder.feed(b"data: partial\n").unwrap().event.is_none());
    assert_eq!(decoder.finish(), Ok(Finish { truncated: true }));
    assert_eq!(decoder.finish(), Ok(Finish { truncated: true }));
    expect_error(&mut decoder, b"data: late\n\n", Error::Closed);
}
