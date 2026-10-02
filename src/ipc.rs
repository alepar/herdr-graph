//! Daemon IPC envelope (spec §1): 4-byte big-endian length + JSON body.
//! Request `{version:1, request_id, command:{kind, args}}` → response `{version, request_id, result}`.
//! Socket server/client live in hg-zmi.4; this file owns only the envelope and framing.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{Read, Write};

pub const IPC_VERSION: u32 = 1;
pub const MAX_FRAME_LEN: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcCommand {
    pub kind: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub version: u32,
    pub request_id: String,
    pub command: IpcCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    BadRequest,
    UnknownCommand,
    VersionMismatch,
    Rejected,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum IpcResult {
    Ok { value: serde_json::Value },
    Error { code: IpcErrorCode, message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcResponse {
    pub version: u32,
    pub request_id: String,
    pub result: IpcResult,
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame of {0} bytes exceeds limit")]
    TooLarge(u64),
    #[error("frame json: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn encode_frame<T: Serialize>(msg: &T) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() as u64 > MAX_FRAME_LEN as u64 {
        return Err(FrameError::TooLarge(body.len() as u64));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode one frame from the front of `buf`. Ok(None) = need more bytes; Ok(Some((msg, consumed))).
pub fn decode_frame<T: DeserializeOwned>(buf: &[u8]) -> Result<Option<(T, usize)>, FrameError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len as u64));
    }
    let end = 4 + len as usize;
    if buf.len() < end {
        return Ok(None);
    }
    Ok(Some((serde_json::from_slice(&buf[4..end])?, end)))
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    w.write_all(&encode_frame(msg)?)?;
    w.flush()?;
    Ok(())
}

pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let mut hdr = [0u8; 4];
    r.read_exact(&mut hdr)?;
    let len = u32::from_be_bytes(hdr);
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len as u64));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn req() -> IpcRequest {
        IpcRequest {
            version: IPC_VERSION,
            request_id: "r1".into(),
            command: IpcCommand { kind: "status".into(), args: serde_json::json!({"a": 1}) },
        }
    }

    #[test]
    fn frame_roundtrip() {
        let bytes = encode_frame(&req()).unwrap();
        let back: IpcRequest = read_frame(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(back, req());
    }

    #[test]
    fn frame_is_big_endian_length_prefixed() {
        let bytes = encode_frame(&req()).unwrap();
        let body = &bytes[4..];
        assert_eq!(bytes[..4], (body.len() as u32).to_be_bytes());
        let v: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(v["version"], 1);
    }

    #[test]
    fn decode_incomplete_returns_none() {
        let bytes = encode_frame(&req()).unwrap();
        assert!(decode_frame::<IpcRequest>(&bytes[..bytes.len() - 1]).unwrap().is_none());
        assert!(decode_frame::<IpcRequest>(&bytes[..3]).unwrap().is_none());
        let (m, used) = decode_frame::<IpcRequest>(&bytes).unwrap().unwrap();
        assert_eq!((m, used), (req(), bytes.len()));
    }

    #[test]
    fn oversize_frame_rejected() {
        let hdr = (MAX_FRAME_LEN + 1).to_be_bytes();
        assert!(matches!(decode_frame::<IpcRequest>(&hdr), Err(FrameError::TooLarge(_))));
        assert!(matches!(
            read_frame::<_, IpcRequest>(&mut Cursor::new(hdr.to_vec())),
            Err(FrameError::TooLarge(_))
        ));
    }

    #[test]
    fn response_error_shape() {
        let r = IpcResult::Error { code: IpcErrorCode::Rejected, message: "no".into() };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "error");
        assert_eq!(v["code"], "rejected");
    }
}
