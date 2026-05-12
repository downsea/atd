use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;

/// Errors from the wire codec.
///
/// SP-concurrency-baseline §5.2: callers need to distinguish "peer sent garbage"
/// (fatal; close connection) from "timed out before peer wrote" (retryable;
/// reissue). The legacy `std::io::Error` collapsed both into `InvalidData`,
/// leaving call sites to string-match the message — fragile and lossy.
#[derive(thiserror::Error, Debug)]
pub enum WireError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("frame length overflow: {0} bytes (max {MAX_FRAME_BYTES})")]
    LengthOverflow(u32),
    #[error("timeout after {0:?}")]
    Timeout(Duration),
}

impl From<WireError> for std::io::Error {
    /// Compat shim for callers still on `std::io::Result`. Preserves the
    /// underlying io error when present; wraps other variants as `InvalidData`.
    fn from(e: WireError) -> Self {
        match e {
            WireError::Io(io) => io,
            other => std::io::Error::new(std::io::ErrorKind::InvalidData, other.to_string()),
        }
    }
}

/// Write one length-prefixed JSON frame, with an optional total-operation deadline.
///
/// Deadline covers `write_all(len) + write_all(body) + flush` as one unit;
/// a partial write that times out leaves the writer in an unspecified state
/// (caller should close the connection).
pub async fn write_frame_with_deadline<W, T>(
    writer: &mut W,
    msg: &T,
    deadline: Option<Duration>,
) -> Result<(), WireError>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let fut = async {
        let body = serde_json::to_vec(msg)?;
        let len = u32::try_from(body.len()).map_err(|_| WireError::LengthOverflow(u32::MAX))?;
        if body.len() > MAX_FRAME_BYTES {
            return Err(WireError::LengthOverflow(len));
        }
        writer.write_all(&len.to_be_bytes()).await?;
        writer.write_all(&body).await?;
        writer.flush().await?;
        Ok::<(), WireError>(())
    };
    match deadline {
        None => fut.await,
        Some(d) => tokio::time::timeout(d, fut)
            .await
            .map_err(|_| WireError::Timeout(d))?,
    }
}

/// Read one length-prefixed JSON frame, with an optional total-operation deadline.
pub async fn read_frame_with_deadline<R, T>(
    reader: &mut R,
    deadline: Option<Duration>,
) -> Result<T, WireError>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    let fut = async {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);
        if len as usize > MAX_FRAME_BYTES {
            return Err(WireError::LengthOverflow(len));
        }
        let mut body = vec![0u8; len as usize];
        reader.read_exact(&mut body).await?;
        Ok::<T, WireError>(serde_json::from_slice(&body)?)
    };
    match deadline {
        None => fut.await,
        Some(d) => tokio::time::timeout(d, fut)
            .await
            .map_err(|_| WireError::Timeout(d))?,
    }
}

/// Back-compat wrapper for callers still on `std::io::Result`.
pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    write_frame_with_deadline(writer, msg, None)
        .await
        .map_err(Into::into)
}

/// Back-compat wrapper for callers still on `std::io::Result`.
pub async fn read_frame<R, T>(reader: &mut R) -> std::io::Result<T>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    read_frame_with_deadline(reader, None)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct M {
        kind: String,
        n: u32,
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let msg = M {
            kind: "ping".into(),
            n: 7,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();

        let mut cursor = std::io::Cursor::new(&buf);
        let back: M = read_frame(&mut cursor).await.unwrap();
        assert_eq!(back, msg);
    }

    #[tokio::test]
    async fn frame_uses_big_endian_u32_prefix() {
        let msg = M {
            kind: "x".into(),
            n: 1,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();
        let body_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(body_len, buf.len() - 4);
    }

    #[tokio::test]
    async fn oversized_frame_errors() {
        // Craft a header claiming 20 MiB; reader should refuse before allocating.
        let mut header = Vec::new();
        let bogus_len: u32 = 20 * 1024 * 1024;
        header.extend_from_slice(&bogus_len.to_be_bytes());
        let mut cursor = std::io::Cursor::new(header);
        let err = read_frame::<_, M>(&mut cursor).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    // ---- SP-concurrency-baseline Phase B Task 1 ----

    #[tokio::test]
    async fn read_frame_with_deadline_returns_timeout_on_no_data() {
        // Keep both halves alive but never write — read_exact will block
        // indefinitely, giving the deadline a real chance to fire.
        let (_producer, mut consumer) = tokio::io::duplex(64);
        let err = read_frame_with_deadline::<_, M>(&mut consumer, Some(Duration::from_millis(50)))
            .await
            .unwrap_err();
        assert!(matches!(err, WireError::Timeout(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn read_frame_with_deadline_succeeds_within_deadline() {
        let msg = M {
            kind: "ok".into(),
            n: 42,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();
        let mut cursor = std::io::Cursor::new(&buf);
        let back: M = read_frame_with_deadline(&mut cursor, Some(Duration::from_millis(500)))
            .await
            .unwrap();
        assert_eq!(back, msg);
    }

    #[tokio::test]
    async fn read_frame_with_deadline_none_means_unbounded() {
        // Regression: deadline=None must behave like the unbounded helper.
        let msg = M {
            kind: "k".into(),
            n: 1,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();
        let mut cursor = std::io::Cursor::new(&buf);
        let back: M = read_frame_with_deadline(&mut cursor, None).await.unwrap();
        assert_eq!(back, msg);
    }

    #[tokio::test]
    async fn write_frame_with_deadline_succeeds_to_in_memory_buf() {
        let msg = M {
            kind: "k".into(),
            n: 1,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame_with_deadline(&mut buf, &msg, Some(Duration::from_millis(500)))
            .await
            .unwrap();
        assert!(buf.len() > 4);
    }

    #[tokio::test]
    async fn write_frame_with_deadline_returns_timeout_on_blocked_writer() {
        // A DuplexStream with a 4-byte buffer; we write a frame whose body
        // is ~80 bytes so the second write_all blocks waiting for the reader
        // to drain. With no reader, the write times out.
        let (mut producer, _consumer) = tokio::io::duplex(4);
        let big_msg = M {
            kind: "x".repeat(50),
            n: 1,
        };
        let err =
            write_frame_with_deadline(&mut producer, &big_msg, Some(Duration::from_millis(50)))
                .await
                .unwrap_err();
        assert!(matches!(err, WireError::Timeout(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn wire_error_into_io_error_preserves_io_underlying() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let wire = WireError::Io(io_err);
        let back: std::io::Error = wire.into();
        assert_eq!(back.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn wire_error_into_io_error_wraps_non_io_as_invalid_data() {
        let wire = WireError::Timeout(Duration::from_millis(100));
        let back: std::io::Error = wire.into();
        assert_eq!(back.kind(), std::io::ErrorKind::InvalidData);
        assert!(back.to_string().contains("timeout"));
    }

    #[tokio::test]
    async fn length_overflow_distinct_from_io_error() {
        let mut header = Vec::new();
        let bogus_len: u32 = 20 * 1024 * 1024;
        header.extend_from_slice(&bogus_len.to_be_bytes());
        let mut cursor = std::io::Cursor::new(header);
        let err = read_frame_with_deadline::<_, M>(&mut cursor, None)
            .await
            .unwrap_err();
        assert!(
            matches!(err, WireError::LengthOverflow(n) if n == bogus_len),
            "got {err:?}"
        );
    }
}
