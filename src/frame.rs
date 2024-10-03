use std::io::Cursor;
use std::io::Read;

use async_std::io::prelude::*;
use async_std::net::TcpStream;
use async_std::prelude::*;
use bytes::Buf;
use bytes::BytesMut;
use libc::CAN_MAX_DLEN;
use socketcan::frame::IdFlags;
use socketcan::CanFrame;
use socketcan::EmbeddedFrame;
use socketcan::Frame;

const FRAME_SIZE: usize = 4 + 1 + CAN_MAX_DLEN;

pub async fn read_frame<R>(
    buf: &mut BytesMut,
    stream: &mut R,
) -> async_std::io::Result<Option<CanFrame>>
where
    R: futures::AsyncRead + Unpin,
{
    loop {
        if let Some(frame) = parse_frame(buf)? {
            return Ok(Some(frame));
        }

        if 0 == stream.read(buf).await? {
            // The remote closed the connection. For this to be
            // a clean shutdown, there should be no data in the
            // read buffer. If there is, this means that the
            // peer closed the socket while sending a frame.
            if buf.is_empty() {
                return Ok(None);
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "connection reset by peer",
                ));
            }
        }
    }
}

pub async fn write_frame<W>(
    stream: &mut W,
    frame: impl socketcan::Frame,
) -> async_std::io::Result<()>
where
    W: futures::AsyncWrite + Unpin,
{
    let data = frame.data();
    let len = data.len() as u8;
    stream.write_all(&[len]).await?;
    stream.write_all(data).await?;
    stream.write_all(&u32::to_be_bytes(frame.id_word())).await?;
    stream.flush().await?;
    Ok(())
}

fn parse_frame(buf: &mut BytesMut) -> async_std::io::Result<Option<CanFrame>> {
    let mut c = Cursor::new(&buf[..]);
    if check_frame(&mut c) {
        let mut data = [0_u8; CAN_MAX_DLEN];

        c.set_position(0);

        let len = c.get_u8() as usize;
        let data = &mut data[..len];

        c.read_exact(data)?;
        let can_id = c.get_u32();

        buf.advance(len + 5);

        let flags = IdFlags::from_bits_truncate(can_id);
        if flags.contains(IdFlags::RTR) {
            return Ok(CanFrame::remote_from_raw_id(can_id, len));
        } else {
            return Ok(CanFrame::from_raw_id(can_id, data));
        }
    }

    Ok(None)
}

fn check_frame(src: &mut Cursor<&[u8]>) -> bool {
    if src.has_remaining() == false {
        return false;
    }

    let len = src.get_u8() as usize;
    if src.remaining() < len + 4 {
        return false;
    }

    true
}

#[cfg(test)]
mod test {
    use socketcan::StandardId;

    use super::*;

    #[test]
    fn test_parse() {
        let b = [3_u8, 1_u8, 2_u8, 3_u8, 0_u8, 0_u8, 0_u8, 10_u8];
        let mut buf = BytesMut::from(&b[..]);
        let r = parse_frame(&mut buf);
        assert!(r.is_ok());
        let r = r.unwrap();
        assert!(r.is_some());
        let r = r.unwrap();
        assert_eq!(
            r.id(),
            socketcan::Id::Standard(StandardId::new(10).unwrap())
        );
    }

    #[test]
    fn parse_advances_buf() {
        let b = [3_u8, 1_u8, 2_u8, 3_u8, 0_u8, 0_u8, 0_u8, 10_u8, 4_u8];
        let mut buf = BytesMut::from(&b[..]);
        let _r = parse_frame(&mut buf);
        assert_eq!(buf.remaining(), 1);
        let r = parse_frame(&mut buf);
        assert!(r.is_ok());
        assert!(r.unwrap().is_none());
    }
}
