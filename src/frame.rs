use std::io::Cursor;
use std::io::Read;

use async_std::prelude::*;
use bytes::Buf;
use bytes::BufMut;
use bytes::BytesMut;
use socketcan::frame::IdFlags;
use socketcan::EmbeddedFrame;
use socketcan::Frame;
use tracing::{debug, info, error};
use async_std::net::TcpStream;
use socketcan::{frame::AsPtr, CanFrame};
use async_std::io::prelude::*;
use libc::{can_frame, CAN_MAX_DLEN};

const FRAME_SIZE: usize = 4 + 1 + CAN_MAX_DLEN;

pub struct Connection {
    stream: TcpStream,
    buffer: BytesMut,
}

impl Connection {

    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffer: BytesMut::with_capacity(1024),
        }
    }

    pub async fn read_frame(&mut self) -> async_std::io::Result<Option<CanFrame>>
    {
        loop {
            if let Some(frame) = parse_frame(&mut self.buffer)? {
                return Ok(Some(frame));
            }

            if 0 == self.stream.read(&mut self.buffer).await? {
                // The remote closed the connection. For this to be
                // a clean shutdown, there should be no data in the
                // read buffer. If there is, this means that the
                // peer closed the socket while sending a frame.
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, "connection reset by peer"));
                }
            }
        }
    }

    pub async fn write_frame(&mut self, frame: CanFrame) -> async_std::io::Result<()> {
        write_frame(&mut self.stream, frame).await
    }
}

async fn write_frame<'a, W>(stream: &'a mut W, frame: impl socketcan::Frame) -> async_std::io::Result<()>
where W: Write + Unpin
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

        let mut data = [0_u8 ; CAN_MAX_DLEN];

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
        assert_eq!(r.id(), socketcan::Id::Standard(StandardId::new(10).unwrap()));
    }
}