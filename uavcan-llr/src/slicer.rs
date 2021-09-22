#[deny(warnings)]

use crate::types::TransferId;
use core::slice::Chunks;
use crate::tailbyte::TailByteIter;
use core::ops::Deref;

pub struct Slicer<'a, const MTU: usize, const MTU_M1: usize> {
    chunks: Chunks<'a, u8>,
    crc: [u8; 2],
    crc_bytes_left: u8, // = 2, 1 or 0
    tail_bytes: TailByteIter,
}

impl<'a, 'b, const MTU: usize, const MTU_M1: usize> Slicer<'a, MTU, MTU_M1> {
    pub fn new(payload: &'a[u8], transfer_id: TransferId) -> Slicer<'a, MTU, MTU_M1> {
        let max_chunk_len = MTU_M1;
        let chunks = payload.chunks(max_chunk_len);
        let mut crc = [0, 0];
        let crc_bytes_left = if payload.len() <= 7 { // single frame transfers are protected by CAN Bus CRC
            0
        } else {
            let mut crc16 = crc_any::CRCu16::crc16ccitt_false();
            crc16.digest(payload);
            let crc16 = crc16.get_crc().to_be_bytes();
            crc.copy_from_slice(&crc16);
            2
        };
        let tail_bytes = crate::tailbyte::TailByte::new_multi_frame(
            transfer_id,
            frame_count::<MTU>(payload.len(), MTU)
        );

        Slicer {
            chunks,
            crc,
            crc_bytes_left,
            tail_bytes,
        }
    }

    #[cfg(feature = "vhrdcan")]
    pub fn new_single(payload: OwnedSlice<MTU_M1>, can_id: crate::types::CanId, transfer_id: &mut TransferId) -> vhrdcan::Frame<MTU> {
        let tail_byte = crate::tailbyte::TailByte::new_single_frame(*transfer_id);
        transfer_id.increment();
        let mut frame_bytes = [0u8; MTU];
        frame_bytes[0..payload.used].copy_from_slice(&payload);
        frame_bytes[payload.used] = tail_byte.as_byte();
        unsafe {
            vhrdcan::Frame::new_unchecked(can_id.into(), &frame_bytes[0..payload.used + 1])
        }
    }

    pub fn frames_ref(self) -> RefSlicer<'a, MTU, MTU_M1> {
        RefSlicer {
            slicer: self
        }
    }

    pub fn frames_owned(self) -> OwnedSlicer<'a, MTU, MTU_M1> {
        OwnedSlicer {
            slicer: self.frames_ref()
        }
    }
}

pub fn frame_count<const MTU: usize>(payload_len: usize, mtu: usize) -> usize {
    if payload_len < MTU {
        1
    } else {
        let payload_len = payload_len + 2;
        payload_len / (mtu - 1) + (payload_len % (mtu - 1) != 0) as usize
    }
}

pub struct RefSlicer<'a, const MTU: usize, const MTU_M1: usize> {
    slicer: Slicer<'a, MTU, MTU_M1>
}

impl<'a, const MTU: usize, const MTU_M1: usize> Iterator for RefSlicer<'a, MTU, MTU_M1> {
    type Item = (&'a [u8], OwnedSlice<3>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.slicer.tail_bytes.next() {
            Some(tail_byte) => {
                let tail_byte = tail_byte.as_byte();
                match self.slicer.chunks.next() {
                    Some(chunk) => {
                        if chunk.len() <= MTU_M1 - 2 { // mtu8: len() <= 5 (2 byte crc and tail byte will fit)
                            if self.slicer.crc_bytes_left == 2 {
                                self.slicer.crc_bytes_left -= 2;
                                Some((chunk, OwnedSlice::new_three(self.slicer.crc[0], self.slicer.crc[1], tail_byte)))
                            } else if self.slicer.crc_bytes_left == 1 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == MTU_M1 - 1 { // mtu8: len() == 6 (1 byte of crc and tail byte will fit)
                            if self.slicer.crc_bytes_left == 2 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[0], tail_byte)))
                            } else if self.slicer.crc_bytes_left == 1 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == MTU_M1 { // mtu8: len() == 7 (only tail byte will fit), single frame transfer or full frames from multi frame
                            Some((chunk, OwnedSlice::new_one(tail_byte)))
                        } else { // size should be <= max_chunk_len=mtu-1 since we chunk input payload that way
                            unreachable!()
                        }
                    },
                    None => {
                        if self.slicer.crc_bytes_left == 2 {
                            self.slicer.crc_bytes_left -= 2;
                            Some((&[], OwnedSlice::new_three(self.slicer.crc[0], self.slicer.crc[1], tail_byte)))
                        } else if self.slicer.crc_bytes_left == 1 {
                            self.slicer.crc_bytes_left -= 1;
                            Some((&[], OwnedSlice::new_two(self.slicer.crc[1], tail_byte)))
                        } else { // can only be 2 or 1 bytes of crc left for transmission
                            unreachable!()
                        }
                    }
                }
            },
            None => None // all frames were consumed
        }
    }
}

pub struct OwnedSlicer<'a, const MTU: usize, const MTU_M1: usize> {
    slicer: RefSlicer<'a, MTU, MTU_M1>
}
impl<'a, const MTU: usize, const MTU_M1: usize> Iterator for OwnedSlicer<'a, MTU, MTU_M1> {
    type Item = OwnedSlice<MTU>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.slicer.next() {
            Some((a, b)) => {
                let mut frame = [0u8; MTU];
                frame[0..a.len()].copy_from_slice(a);
                frame[a.len()..a.len() + b.len()].copy_from_slice(&b);
                Some(OwnedSlice::new(frame, a.len() + b.len()))
            },
            None => None
        }
    }
}
impl<'a, const MTU: usize, const MTU_M1: usize> OwnedSlicer<'a, MTU, MTU_M1> {
    #[cfg(feature = "vhrdcan")]
    pub fn vhrd(self, id: crate::types::CanId) -> VhrdOwnedSlicer<'a, MTU, MTU_M1> {
        VhrdOwnedSlicer {
            slicer: self,
            id: unsafe { vhrdcan::FrameId::Extended(vhrdcan::id::ExtendedId::new_unchecked(id.into())) }
        }
    }

    // #[cfg(feature = "vhrdcan")]
    // pub fn vhrd_pool<'b>(self, pool: &'b mut vhrdcan::FramePool, id: CanId) -> VhrdOwnedSlicerPool<'a, 'b, MTU> {
    //     VhrdOwnedSlicerPool {
    //         slicer: self,
    //         pool,
    //         id: unsafe { vhrdcan::FrameId::Extended(vhrdcan::id::ExtendedId::new_unchecked(id.into())) }
    //     }
    // }
}

#[cfg(feature = "vhrdcan")]
pub struct VhrdOwnedSlicer<'a, const MTU: usize, const MTU_M1: usize> {
    slicer: OwnedSlicer<'a, MTU, MTU_M1>,
    id: vhrdcan::FrameId,
}
#[cfg(feature = "vhrdcan")]
impl<'a, const MTU: usize, const MTU_M1: usize> Iterator for VhrdOwnedSlicer<'a, MTU, MTU_M1> {
    type Item = vhrdcan::Frame<MTU>;

    fn next(&mut self) -> Option<Self::Item> {
        self.slicer.next().map(|frame| {
            vhrdcan::Frame::new_move(self.id, frame.bytes, frame.used as u16).unwrap()
        })
    }
}

// #[cfg(feature = "vhrdcan")]
// pub struct VhrdOwnedSlicerPool<'a, 'b, const MTU: usize> {
//     slicer: OwnedSlicer<'a, MTU>,
//     id: vhrdcan::FrameId,
//     pool: &'b mut vhrdcan::FramePool
// }
// #[cfg(feature = "vhrdcan")]
// impl<'a, 'b> Iterator for VhrdOwnedSlicerPool<'a, 'b, 8> {
//     type Item = vhrdcan::Frame;
//
//     fn next(&mut self) -> Option<Self::Item> {
//         self.slicer.next().map(|frame| {
//             self.pool.new_frame_from_raw(
//                 vhrdcan::Frame::new_move(
//                     self.id,
//                     frame.bytes,
//                     frame.used as u8
//                 ).unwrap())
//         })
//     }
// }

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct OwnedSlice<const N: usize> {
    pub bytes: [u8; N],
    pub used: usize,
}
impl<const N: usize> OwnedSlice<N> {
    pub fn new_one(byte: u8) -> Self {
        let mut bytes = [0u8; N];
        bytes[0] = byte;
        OwnedSlice {
            bytes,
            used: 1
        }
    }
    pub fn new_two(byte0: u8, byte1: u8) -> Self {
        let mut bytes = [0u8; N];
        bytes[0] = byte0;
        bytes[1] = byte1;
        OwnedSlice {
            bytes,
            used: 2
        }
    }
    pub fn new_three(byte0: u8, byte1: u8, byte2: u8) -> Self {
        let mut bytes = [0u8; N];
        bytes[0] = byte0;
        bytes[1] = byte1;
        bytes[2] = byte2;
        OwnedSlice {
            bytes,
            used: 3
        }
    }
    pub fn new(bytes: [u8; N], used: usize) -> Self {
        OwnedSlice {
            bytes, used
        }
    }
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() <= N {
            let mut bytes_copy = [0u8; N];
            bytes_copy[0..bytes.len()].copy_from_slice(bytes);
            Some(OwnedSlice {
                bytes: bytes_copy,
                used: bytes.len()
            })
        } else {
            None
        }
    }
}
impl<const N: usize> Deref for OwnedSlice<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes[0..self.used]
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use crate::types::*;
    use crate::slicer::{Slicer, OwnedSlice, frame_count};

    #[test]
    fn check_frame_count() {
        assert_eq!(frame_count::<8>(0, 8), 1);
        assert_eq!(frame_count::<8>(1, 8), 1);
        assert_eq!(frame_count::<8>(6, 8), 1);
        assert_eq!(frame_count::<8>(7, 8), 1);

        assert_eq!(frame_count::<8>(8, 8), 2);  // 7+t 1+crc+t
        assert_eq!(frame_count::<8>(12, 8), 2); // 7+t 5+crc+t

        assert_eq!(frame_count::<8>(13, 8), 3); // 7+t 6+cr+t c+t
        assert_eq!(frame_count::<8>(14, 8), 3); // 7+t 7+t crc+t
        assert_eq!(frame_count::<8>(19, 8), 3); // 7+t 7+t 5+crc+t

        assert_eq!(frame_count::<8>(20, 8), 4); // 7+t 7+t 6+cr+t c+t
        assert_eq!(frame_count::<8>(21, 8), 4); // 7+t 7+t 7+t crc+t
        assert_eq!(frame_count::<8>(26, 8), 4); // 7+t 7+t 7+t 5+crc+t
    }

    #[test]
    fn check_slicer() {
        let payload = [0, 1, 2, 3, 4, 5, 6];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(0).unwrap()).frames_owned();
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0, 1, 2, 3, 4, 5, 6, 0b1110_0000],
            used: 8
        }));
        assert_eq!(slicer.next(), None);

        let payload = [0, 1, 2, 3, 4, 5, 6, 7];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(1).unwrap()).frames_owned();
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0, 1, 2, 3, 4, 5, 6, 0b1010_0001],
            used: 8
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [7, 0x17, 0x8d, 0b0100_0001, 0, 0, 0, 0],
            used: 4,
        }));
        assert_eq!(slicer.next(), None);

        let payload = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(2).unwrap()).frames_owned();
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0, 1, 2, 3, 4, 5, 6, 0b1010_0010],
            used: 8
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [7, 8, 9, 10, 11, 12, 0xac, 0b0000_0010],
            used: 8,
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0xdd, 0b0110_0010, 0, 0, 0, 0, 0, 0],
            used: 2,
        }));
        assert_eq!(slicer.next(), None);

        let payload = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(31).unwrap()).frames_owned();
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0, 1, 2, 3, 4, 5, 6, 0b1011_1111],
            used: 8
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [7, 8, 9, 10, 11, 12, 13, 0b0001_1111],
            used: 8,
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0x78, 0xcb, 0b0111_1111, 0, 0, 0, 0, 0],
            used: 3,
        }));
        assert_eq!(slicer.next(), None);

        let payload = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(0).unwrap()).frames_owned();
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0, 1, 2, 3, 4, 5, 6, 0b1010_0000],
            used: 8
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [7, 8, 9, 10, 11, 12, 13, 0b0000_0000],
            used: 8,
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [14, 15, 16, 17, 18, 19, 20, 0b0010_0000],
            used: 8,
        }));
        assert_eq!(slicer.next(), Some(OwnedSlice {
            bytes: [0xdd, 0x0a, 0b0100_0000, 0, 0, 0, 0, 0],
            used: 3,
        }));
        assert_eq!(slicer.next(), None);
    }
}