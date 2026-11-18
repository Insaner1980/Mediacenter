//! Bounded native-messaging framing. Payloads can contain private text; never log them.
use std::io::{self, Read, Write};
pub const MAX_NATIVE_BYTES: usize = 64 * 1024;
pub const EXTENSION_ORIGIN: &str = "chrome-extension://njhbcllpkbihpkioegopblabopnkhndk/";
pub fn read_frame(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0; 4];
    loop {
        match input.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    input.read_exact(&mut header[1..])?;
    let length = u32::from_ne_bytes(header) as usize;
    if length == 0 || length > MAX_NATIVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native frame size",
        ));
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}
pub fn write_frame(output: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_NATIVE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native frame size",
        ));
    }
    output.write_all(&(payload.len() as u32).to_ne_bytes())?;
    output.write_all(payload)?;
    output.flush()
}
pub fn valid_origin(origin: &str) -> bool {
    origin == EXTENSION_ORIGIN
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_framing_preserves_unicode_and_detects_truncation() {
        let payload = "synthetic café 🦊".as_bytes();
        let mut bytes = Vec::new();
        write_frame(&mut bytes, payload).unwrap();
        assert_eq!(
            read_frame(&mut bytes.as_slice()).unwrap(),
            Some(payload.to_vec())
        );
        assert!(read_frame(&mut &bytes[..bytes.len() - 1]).is_err());
        assert!(read_frame(&mut &[1u8][..]).is_err());
        assert_eq!(read_frame(&mut &[][..]).unwrap(), None);
    }
    #[test]
    fn rejects_oversized_frames_before_body_allocation() {
        assert!(read_frame(&mut &(MAX_NATIVE_BYTES as u32 + 1).to_ne_bytes()[..]).is_err());
        assert!(write_frame(&mut Vec::new(), &vec![0; MAX_NATIVE_BYTES + 1]).is_err());
    }
    #[test]
    fn accepts_only_exact_bundled_extension_origin() {
        assert!(valid_origin(EXTENSION_ORIGIN));
        for bad in [
            "chrome-extension://other/",
            "*",
            "",
            "chrome-extension://njhbcllpkbihpkioegopblabopnkhndk/extra",
        ] {
            assert!(!valid_origin(bad));
        }
    }
}
