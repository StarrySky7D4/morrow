//! Host-selected four-byte LE length envelope. Payload interpretation belongs
//! exclusively to the supplied Cap'n Proto codec, not this streaming helper.

pub struct LengthDecoder {
    bytes: Vec<u8>,
    maximum_payload: usize,
    maximum_chunk: usize,
    poisoned: bool,
    prefix_validator: Option<fn(&[u8]) -> Result<usize, &'static str>>,
}

impl LengthDecoder {
    pub fn new(maximum_payload: usize, maximum_chunk: usize) -> Result<Self, &'static str> {
        if maximum_payload == 0
            || maximum_chunk == 0
            || maximum_payload
                .checked_add(4)
                .and_then(|v| v.checked_add(maximum_chunk))
                .is_none()
        {
            return Err("invalid frame bounds");
        }
        Ok(Self {
            bytes: vec![],
            maximum_payload,
            maximum_chunk,
            poisoned: false,
            prefix_validator: None,
        })
    }

    pub fn with_prefix_validator(
        mut self,
        validator: fn(&[u8]) -> Result<usize, &'static str>,
    ) -> Self {
        self.prefix_validator = Some(validator);
        self
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.poisoned {
            return Err("decoder already rejected");
        }
        if chunk.len() > self.maximum_chunk
            || self.bytes.len() + chunk.len() > self.maximum_payload + 4 + self.maximum_chunk
        {
            self.poisoned = true;
            return Err("bounded input queue exceeded");
        }
        self.bytes.extend_from_slice(chunk);
        // Reject a bad size as soon as its complete prefix arrives, before any
        // allocation based on the advertised payload length.
        self.validate_prefix()
    }

    fn validate_prefix(&mut self) -> Result<(), &'static str> {
        if self.bytes.len() >= 4 {
            if let Some(validate) = self.prefix_validator {
                if validate(&self.bytes[..4]).is_err() {
                    self.poisoned = true;
                    return Err("host codec rejected frame prefix");
                }
            }
            let length = u32::from_le_bytes(self.bytes[..4].try_into().unwrap()) as usize;
            if length == 0 || length > self.maximum_payload {
                self.poisoned = true;
                return Err("invalid payload length");
            }
        }
        Ok(())
    }

    pub fn next_payload(&mut self) -> Result<Option<Vec<u8>>, &'static str> {
        if self.poisoned {
            return Err("decoder already rejected");
        }
        self.validate_prefix()?;
        if self.bytes.len() < 4 {
            return Ok(None);
        }
        let length = u32::from_le_bytes(self.bytes[..4].try_into().unwrap()) as usize;
        if self.bytes.len() < 4 + length {
            return Ok(None);
        }
        let payload = self.bytes[4..4 + length].to_vec();
        self.bytes.drain(..4 + length);
        Ok(Some(payload))
    }

    pub fn has_partial(&self) -> bool {
        !self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_prefix_and_coalesced_messages_preserve_exact_payloads() {
        let mut decoder = LengthDecoder::new(8, 16).unwrap();
        decoder.feed(&[3, 0]).unwrap();
        assert!(decoder.next_payload().unwrap().is_none());
        decoder.feed(&[0, 0, 1, 2, 3, 1, 0, 0, 0, 4]).unwrap();
        assert_eq!(decoder.next_payload().unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(decoder.next_payload().unwrap(), Some(vec![4]));
        assert!(!decoder.has_partial());
    }
    #[test]
    fn oversized_prefix_rejected_without_body_and_cannot_resume() {
        let mut decoder = LengthDecoder::new(8, 4).unwrap();
        assert_eq!(
            decoder.feed(&u32::MAX.to_le_bytes()),
            Err("invalid payload length")
        );
        assert_eq!(decoder.feed(&[1]), Err("decoder already rejected"));
        assert!(decoder.next_payload().is_err());
    }
}
