//! RFC 6455 WebSocket frame encoding and decoding.
//!
//! Provides pure, robust frame serialization and deserialization over any
//! `AsyncRead` and `AsyncWrite` stream without external C-bindings or unwieldy wrappers.

use bytes::{BufMut, BytesMut};
use std::io;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Error)]
pub enum WsCodecError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("frame payload too large: {0} bytes")]
    PayloadTooLarge(usize),
    #[error("invalid utf-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// WebSocket Opcode per RFC 6455 §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl Opcode {
    pub fn from_u8(byte: u8) -> Result<Self, WsCodecError> {
        match byte {
            0x0 => Ok(Self::Continuation),
            0x1 => Ok(Self::Text),
            0x2 => Ok(Self::Binary),
            0x8 => Ok(Self::Close),
            0x9 => Ok(Self::Ping),
            0xA => Ok(Self::Pong),
            _ => Err(WsCodecError::Protocol(format!(
                "unknown opcode 0x{byte:02x}"
            ))),
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// A parsed WebSocket frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<(u16, String)>),
}

const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024; // 16 MB max frame size

/// Read a single frame from an `AsyncRead` stream.
/// Returns `Ok(None)` if stream hits EOF before frame header.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Frame>, WsCodecError> {
    let mut header = [0u8; 2];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(WsCodecError::Io(e)),
    }

    let fin = (header[0] & 0x80) != 0;
    let opcode = Opcode::from_u8(header[0] & 0x0F)?;
    let masked = (header[1] & 0x80) != 0;
    let initial_len = (header[1] & 0x7F) as usize;

    // Read full payload length
    let payload_len = match initial_len {
        126 => {
            let mut buf = [0u8; 2];
            reader.read_exact(&mut buf).await?;
            u16::from_be_bytes(buf) as usize
        }
        127 => {
            let mut buf = [0u8; 8];
            reader.read_exact(&mut buf).await?;
            let len = u64::from_be_bytes(buf);
            if len > MAX_PAYLOAD_BYTES as u64 {
                return Err(WsCodecError::PayloadTooLarge(len as usize));
            }
            len as usize
        }
        len => len,
    };

    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(WsCodecError::PayloadTooLarge(payload_len));
    }

    // Masking key (if present)
    let mask_key = if masked {
        let mut key = [0u8; 4];
        reader.read_exact(&mut key).await?;
        Some(key)
    } else {
        None
    };

    // Read payload
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        reader.read_exact(&mut payload).await?;
    }

    // Unmask if masked
    if let Some(mask) = mask_key {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }

    if !fin {
        return Err(WsCodecError::Protocol(
            "fragmented frames not supported".to_string(),
        ));
    }

    match opcode {
        Opcode::Text => {
            let text = String::from_utf8(payload)?;
            Ok(Some(Frame::Text(text)))
        }
        Opcode::Binary => Ok(Some(Frame::Binary(payload))),
        Opcode::Ping => Ok(Some(Frame::Ping(payload))),
        Opcode::Pong => Ok(Some(Frame::Pong(payload))),
        Opcode::Close => {
            if payload.len() >= 2 {
                let code = u16::from_be_bytes([payload[0], payload[1]]);
                let reason = String::from_utf8_lossy(&payload[2..]).to_string();
                Ok(Some(Frame::Close(Some((code, reason)))))
            } else {
                Ok(Some(Frame::Close(None)))
            }
        }
        Opcode::Continuation => Err(WsCodecError::Protocol(
            "unexpected continuation frame".to_string(),
        )),
    }
}

/// Write a server-to-client frame (unmasked per RFC 6455 §5.1) to an `AsyncWrite` stream.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &Frame,
) -> Result<(), WsCodecError> {
    let (opcode, payload) = match frame {
        Frame::Text(text) => (Opcode::Text, text.as_bytes()),
        Frame::Binary(data) => (Opcode::Binary, data.as_slice()),
        Frame::Ping(data) => (Opcode::Ping, data.as_slice()),
        Frame::Pong(data) => (Opcode::Pong, data.as_slice()),
        Frame::Close(details) => {
            let mut bytes = Vec::new();
            if let Some((code, reason)) = details {
                bytes.extend_from_slice(&code.to_be_bytes());
                bytes.extend_from_slice(reason.as_bytes());
            }
            let mut out = BytesMut::new();
            encode_frame_header(&mut out, Opcode::Close, bytes.len(), false);
            out.extend_from_slice(&bytes);
            writer.write_all(&out).await?;
            writer.flush().await?;
            return Ok(());
        }
    };

    let mut out = BytesMut::with_capacity(10 + payload.len());
    encode_frame_header(&mut out, opcode, payload.len(), false);
    out.extend_from_slice(payload);

    writer.write_all(&out).await?;
    writer.flush().await?;
    Ok(())
}

/// Write a client-to-server frame (masked per RFC 6455 §5.1) — primarily for test suites and client simulations.
pub async fn write_client_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &Frame,
    mask: [u8; 4],
) -> Result<(), WsCodecError> {
    let (opcode, payload) = match frame {
        Frame::Text(text) => (Opcode::Text, text.as_bytes()),
        Frame::Binary(data) => (Opcode::Binary, data.as_slice()),
        Frame::Ping(data) => (Opcode::Ping, data.as_slice()),
        Frame::Pong(data) => (Opcode::Pong, data.as_slice()),
        Frame::Close(details) => {
            let mut bytes = Vec::new();
            if let Some((code, reason)) = details {
                bytes.extend_from_slice(&code.to_be_bytes());
                bytes.extend_from_slice(reason.as_bytes());
            }
            let mut masked = bytes.clone();
            for (i, b) in masked.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
            let mut out = BytesMut::new();
            encode_frame_header(&mut out, Opcode::Close, bytes.len(), true);
            out.extend_from_slice(&mask);
            out.extend_from_slice(&masked);
            writer.write_all(&out).await?;
            writer.flush().await?;
            return Ok(());
        }
    };

    let mut masked = payload.to_vec();
    for (i, b) in masked.iter_mut().enumerate() {
        *b ^= mask[i % 4];
    }

    let mut out = BytesMut::with_capacity(14 + masked.len());
    encode_frame_header(&mut out, opcode, masked.len(), true);
    out.extend_from_slice(&mask);
    out.extend_from_slice(&masked);

    writer.write_all(&out).await?;
    writer.flush().await?;
    Ok(())
}

fn encode_frame_header(buf: &mut BytesMut, opcode: Opcode, len: usize, masked: bool) {
    // Byte 0: FIN = 1 | Opcode
    buf.put_u8(0x80 | opcode.to_u8());

    let mask_bit = if masked { 0x80 } else { 0x00 };
    if len <= 125 {
        buf.put_u8(mask_bit | (len as u8));
    } else if len <= 65535 {
        buf.put_u8(mask_bit | 126);
        buf.put_u16(len as u16);
    } else {
        buf.put_u8(mask_bit | 127);
        buf.put_u64(len as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn server_unmasked_text_roundtrip() {
        let (mut client, mut server) = duplex(1024);
        let text = "{\"type\":\"hello.ack\",\"data\":{\"user_id\":\"12345\"}}";
        let frame = Frame::Text(text.to_string());

        write_frame(&mut server, &frame).await.unwrap();

        let received = read_frame(&mut client).await.unwrap().expect("a frame");
        assert_eq!(received, frame);
    }

    #[tokio::test]
    async fn client_masked_text_roundtrip() {
        let (mut client, mut server) = duplex(1024);
        let text = "{\"type\":\"hello\",\"data\":{\"client\":\"web@1.0.0\"}}";
        let frame = Frame::Text(text.to_string());
        let mask = [0x12, 0x34, 0x56, 0x78];

        write_client_frame(&mut client, &frame, mask).await.unwrap();

        let received = read_frame(&mut server).await.unwrap().expect("a frame");
        assert_eq!(received, frame);
    }

    #[tokio::test]
    async fn medium_payload_length_encoding() {
        let (mut client, mut server) = duplex(4096);
        let medium_payload = vec![b'a'; 300];
        let frame = Frame::Binary(medium_payload);

        write_frame(&mut server, &frame).await.unwrap();

        let received = read_frame(&mut client).await.unwrap().expect("a frame");
        assert_eq!(received, frame);
    }

    #[tokio::test]
    async fn ping_pong_frames() {
        let (mut client, mut server) = duplex(1024);
        let ping = Frame::Ping(b"heartbeat".to_vec());
        let mask = [0xaa, 0xbb, 0xcc, 0xdd];

        write_client_frame(&mut client, &ping, mask).await.unwrap();
        let received_ping = read_frame(&mut server).await.unwrap().expect("ping frame");
        assert_eq!(received_ping, ping);

        let pong = Frame::Pong(b"heartbeat".to_vec());
        write_frame(&mut server, &pong).await.unwrap();
        let received_pong = read_frame(&mut client).await.unwrap().expect("pong frame");
        assert_eq!(received_pong, pong);
    }

    #[tokio::test]
    async fn close_frame_with_code_and_reason() {
        let (mut client, mut server) = duplex(1024);
        let close = Frame::Close(Some((1000, "normal closure".to_string())));
        write_frame(&mut server, &close).await.unwrap();

        let received = read_frame(&mut client).await.unwrap().expect("close frame");
        assert_eq!(received, close);
    }
}
