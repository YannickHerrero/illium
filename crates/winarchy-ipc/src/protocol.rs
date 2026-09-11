//! Bounded UTF-8 line framing, shared with the named-pipe transport.
use crate::command::Command;
use std::io::Read;
pub const MAX_COMMAND_BYTES: usize = 8191;

pub fn read_command(reader: &mut impl Read) -> Result<Command, String> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        reader
            .read_exact(&mut byte)
            .map_err(|e| format!("incomplete IPC command: {e}"))?;
        if byte[0] == b'\n' {
            break;
        }
        if bytes.len() == MAX_COMMAND_BYTES {
            return Err("IPC command exceeds 8191 bytes".into());
        }
        bytes.push(byte[0]);
    }
    std::str::from_utf8(&bytes)
        .map_err(|e| e.to_string())?
        .parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_terminator() {
        assert_eq!(read_command(&mut &b"quit\n"[..]), Ok(Command::Quit));
        assert!(read_command(&mut &b"quit"[..]).is_err());
        assert!(read_command(&mut &b""[..]).is_err());
    }
    #[test]
    fn bounds_and_utf8() {
        let exact = format!("quit{}\n", " ".repeat(MAX_COMMAND_BYTES - 4));
        assert_eq!(read_command(&mut exact.as_bytes()), Ok(Command::Quit));
        let oversized = format!("quit{}\n", " ".repeat(MAX_COMMAND_BYTES - 3));
        assert!(read_command(&mut oversized.as_bytes()).is_err());
        assert!(read_command(&mut &b"spawn \xff\n"[..]).is_err());
        assert_eq!(
            read_command(&mut "spawn éditeur\r\n".as_bytes()),
            Ok(Command::Spawn("éditeur".into()))
        );
    }
    #[test]
    fn consumes_one_frame() {
        let mut input = &b"status\nquit\n"[..];
        assert_eq!(read_command(&mut input), Ok(Command::Status));
        assert_eq!(input, b"quit\n");
    }
}
