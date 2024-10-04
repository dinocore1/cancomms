use bytes::{Buf, BufMut, BytesMut};
use socketcan::frame::IdFlags;
use socketcan::{CanFrame, EmbeddedFrame, Frame};
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

pub struct CanFrameCodec;

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
                dst.put_slice(data);
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

        let mut can_frame = socketcan::frame::can_frame_default();

        let mut id_bytes = [0_u8; 4];
        id_bytes.copy_from_slice(&src[..4]);
        can_frame.can_id = u32::from_be_bytes(id_bytes);

        can_frame.can_dlc = src[4];

        let flags = IdFlags::from_bits_truncate(can_frame.can_id);
        if flags.contains(IdFlags::RTR) {
            src.advance(5);

            Ok(Some(CanFrame::from(can_frame)))
        } else {
            let len = can_frame.can_dlc as usize;
            if src.len() < 5 + len {
                // the full frame has not yet arrived
                src.reserve(5 + len - src.len());
                return Ok(None);
            }
            can_frame.data[..len].copy_from_slice(&src[5..5 + len]);

            // let data_frame = CanFrame::from_raw_id(can_id, data);
            src.advance(5 + len);
            Ok(Some(CanFrame::from(can_frame)))
        }
    }
}

#[cfg(test)]
mod test {
    use socketcan::{Id, StandardId};

    use super::*;

    #[test]
    fn test_encode_data_frame() {
        let mut encoder = CanFrameCodec;
        let can_frame = CanFrame::from_raw_id(10, &[1_u8, 2_u8, 3_u8]).unwrap();
        let mut dst = BytesMut::new();
        let r = encoder.encode(can_frame, &mut dst);
        assert!(r.is_ok());

        assert_eq!(&[0_u8, 0_u8, 0_u8, 10_u8, 3_u8, 1_u8, 2_u8, 3_u8], &dst[..]);
    }

    #[test]
    fn test_decode_data_frame() {
        let data = [0_u8, 0_u8, 0_u8, 10_u8, 3_u8, 1_u8, 2_u8, 3_u8];
        let mut src = BytesMut::from(&data[..]);
        let mut decoder = CanFrameCodec;
        let r = decoder.decode(&mut src);
        assert!(r.is_ok());
        let r = r.unwrap().unwrap();
        assert_eq!(10, r.id_word());
        assert_eq!(&[1_u8, 2_u8, 3_u8], r.data());
    }

    #[test]
    fn test_encode_remote_frame() {
        let mut encoder = CanFrameCodec;
        let can_frame = CanFrame::remote_from_raw_id(10, 3).unwrap();
        let mut dst = BytesMut::new();
        let r = encoder.encode(can_frame, &mut dst);
        assert!(r.is_ok());

        assert_eq!(&[64_u8, 0_u8, 0_u8, 10_u8, 3_u8], &dst[..]);
    }

    #[test]
    fn test_decode_remote_frame() {
        let data = [64_u8, 0_u8, 0_u8, 10_u8, 3_u8, 1_u8, 2_u8, 3_u8];
        let mut src = BytesMut::from(&data[..]);
        let mut decoder = CanFrameCodec;
        let r = match decoder.decode(&mut src) {
            Ok(Some(CanFrame::Remote(r))) => r,
            _ => panic!(""),
        };
        assert_eq!(r.id(), Id::Standard(StandardId::new(10).unwrap()));
        assert_eq!(3, r.dlc());
    }
}