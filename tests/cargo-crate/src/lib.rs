pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |acc, b| acc ^ b)
}

pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 2);
    frame.push(u8::try_from(payload.len()).expect("payload fits a frame"));
    frame.extend_from_slice(payload);
    frame.push(checksum(payload));
    frame
}

pub fn decode(frame: &[u8]) -> Option<&[u8]> {
    let (&len, rest) = frame.split_first()?;
    let (payload, tail) = rest.split_at_checked(usize::from(len))?;
    (tail == [checksum(payload)]).then_some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let frame = encode(b"hello");
        assert_eq!(decode(&frame), Some(&b"hello"[..]));
    }

    #[test]
    fn corrupted_frames_are_rejected() {
        let mut frame = encode(b"hello");
        frame[2] ^= 0xff;
        assert_eq!(decode(&frame), None);
        assert_eq!(decode(&[]), None);
    }
}
