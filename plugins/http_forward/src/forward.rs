/// Matches the experimental runtime's exact read/output capacity contract.
pub(crate) const FRAME_BYTES: usize = 128 * 1024;

pub(crate) trait Transport {
    fn read(&mut self, output: &mut [u8; FRAME_BYTES]) -> i32;
    fn call(&mut self, input: &[u8], output: &mut [u8; FRAME_BYTES]) -> i32;
    fn complete(&mut self, response: &[u8]) -> i32;
}

/// Forward exactly once. A structured denial/Unknown response is returned unchanged.
/// A nonzero return means the guest transport failed, never permission to retry.
pub(crate) fn run(host: &mut impl Transport) -> i32 {
    let mut request = Box::new([0; FRAME_BYTES]);
    let length = host.read(&mut request);
    if length <= 0 || length as usize > FRAME_BYTES {
        return 1;
    }
    let mut response = Box::new([0; FRAME_BYTES]);
    let written = host.call(&request[..length as usize], &mut response);
    if written <= 0 || written as usize > FRAME_BYTES {
        return 2;
    }
    if host.complete(&response[..written as usize]) != 0 {
        return 3;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host {
        input: Vec<u8>,
        response: Vec<u8>,
        read_result: i32,
        call_result: i32,
        complete_result: i32,
        reads: usize,
        calls: usize,
        completions: Vec<Vec<u8>>,
    }
    impl Default for Host {
        fn default() -> Self {
            Self {
                input: vec![0, 7, 0, 9],
                response: vec![0, 255, 0, 1, 9],
                read_result: 4,
                call_result: 5,
                complete_result: 0,
                reads: 0,
                calls: 0,
                completions: vec![],
            }
        }
    }
    impl Transport for Host {
        fn read(&mut self, output: &mut [u8; FRAME_BYTES]) -> i32 {
            self.reads += 1;
            output[..self.input.len()].copy_from_slice(&self.input);
            self.read_result
        }
        fn call(&mut self, input: &[u8], output: &mut [u8; FRAME_BYTES]) -> i32 {
            assert_eq!(input, self.input);
            // The host ABI rejects overlapping live input and output regions.
            let start = input.as_ptr() as usize;
            let out = output.as_ptr() as usize;
            assert!(start + input.len() <= out || out + output.len() <= start);
            self.calls += 1;
            output[..self.response.len()].copy_from_slice(&self.response);
            self.call_result
        }
        fn complete(&mut self, response: &[u8]) -> i32 {
            self.completions.push(response.to_vec());
            self.complete_result
        }
    }

    #[test]
    fn one_call_preserves_the_exact_request_and_response_bytes() {
        let mut host = Host::default();
        assert_eq!(run(&mut host), 0);
        assert_eq!((host.reads, host.calls), (1, 1));
        assert_eq!(host.completions, vec![host.response]);
    }

    #[test]
    fn invalid_input_length_never_enters_io() {
        for length in [-1, 0, FRAME_BYTES as i32 + 1, i32::MAX] {
            let mut host = Host {
                read_result: length,
                ..Host::default()
            };
            assert_eq!(run(&mut host), 1);
            assert_eq!(host.calls, 0);
            assert!(host.completions.is_empty());
        }
    }

    #[test]
    fn invalid_reply_length_never_completes_or_retries() {
        for length in [-1, 0, FRAME_BYTES as i32 + 1, i32::MAX] {
            let mut host = Host {
                call_result: length,
                ..Host::default()
            };
            assert_eq!(run(&mut host), 2);
            assert_eq!(host.calls, 1);
            assert!(host.completions.is_empty());
        }
    }

    #[test]
    fn completion_failure_never_replays_io_or_completion() {
        let mut host = Host {
            complete_result: -1,
            ..Host::default()
        };
        assert_eq!(run(&mut host), 3);
        assert_eq!(host.calls, 1);
        assert_eq!(host.completions, vec![host.response]);
    }

    #[test]
    fn full_capacity_input_and_response_are_not_truncated() {
        let mut host = Host {
            input: vec![17; FRAME_BYTES],
            response: vec![231; FRAME_BYTES],
            read_result: FRAME_BYTES as i32,
            call_result: FRAME_BYTES as i32,
            ..Host::default()
        };
        assert_eq!(run(&mut host), 0);
        assert_eq!(host.calls, 1);
        assert_eq!(host.completions, vec![host.response]);
    }
}
