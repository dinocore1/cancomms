use bytes::{Buf, BufMut, BytesMut};
use socketcan::frame::IdFlags;
use socketcan::{CanDataFrame, CanFrame, EmbeddedFrame, Frame};
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

pub struct CanFrameCodec {}

impl Encoder<CanFrame> for CanFrameCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: CanFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            CanFrame::Data(d) => {
                let len = d.len();
                dst.reserve(5 + len);

                dst.put_u32(d.id_word());
                dst.put_u8(len as u8);

                let data = d.data();
                dst.extend_from_slice(data);
                Ok(())
            }

            CanFrame::Remote(r) => {
                let len = r.len();
                dst.reserve(5);

                dst.put_u32(r.id_word());
                dst.put_u8(len as u8);
                Ok(())
            }

            CanFrame::Error(e) => {
                todo!()
            }
        }
    }
}

impl Decoder for CanFrameCodec {
    type Item = CanFrame;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 5 {
            return Ok(None);
        }

        let mut id_bytes = [0_u8; 4];
        id_bytes.copy_from_slice(&src[..4]);
        let can_id = u32::from_be_bytes(id_bytes);

        let len = src[4] as usize;

        let flags = IdFlags::from_bits_truncate(can_id);
        if flags.contains(IdFlags::RTR) {
            src.advance(5);
            Ok(CanFrame::remote_from_raw_id(can_id, len))
        } else {
            if src.len() < 5 + len {
                // the full frame has not yet arrived
                src.reserve(5 + len - src.len());
                return Ok(None);
            }
            let data = &src[5..5 + len];
            let data_frame = CanFrame::from_raw_id(can_id, data);
            src.advance(5 + len);
            Ok(data_frame)
        }
    }
}
