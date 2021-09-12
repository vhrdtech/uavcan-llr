#[deny(warnings)]

use crate::types::TransferId;
use core::slice::Chunks;
use crate::tailbyte::TailByteIter;
use core::ops::Deref;

pub struct Slicer<'a, const MTU: usize> {
    chunks: Chunks<'a, u8>,
    crc: [u8; 2],
    crc_bytes_left: u8, // = 2, 1 or 0
    tail_bytes: TailByteIter,
}

impl<'a, 'b, const MTU: usize> Slicer<'a, MTU> {
    pub fn new(payload: &'a[u8], transfer_id: TransferId) -> Slicer<'a, MTU> {
        let max_chunk_len = MTU - 1;
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
            frame_count(payload.len(), MTU)
        );

        Slicer {
            chunks,
            crc,
            crc_bytes_left,
            tail_bytes,
        }
    }

    #[cfg(feature = "vhrdcan")]
    pub fn new_single(payload: OwnedSlice<{MTU - 1}>, can_id: crate::types::CanId, transfer_id: &mut TransferId) -> vhrdcan::Frame<MTU> {
        let tail_byte = crate::tailbyte::TailByte::new_single_frame(*transfer_id);
        transfer_id.increment();
        let mut frame_bytes = [0u8; MTU];
        frame_bytes[0..payload.used].copy_from_slice(&payload);
        frame_bytes[payload.used] = tail_byte.as_byte();
        unsafe {
            vhrdcan::Frame::new_unchecked(can_id.into(), &frame_bytes[0..payload.used + 1])
        }
    }

    pub fn frames_ref(self) -> RefSlicer<'a, MTU> {
        RefSlicer {
            slicer: self
        }
    }

    pub fn frames_owned(self) -> OwnedSlicer<'a, MTU> {
        OwnedSlicer {
            slicer: self.frames_ref()
        }
    }
}

pub fn frame_count(payload_len: usize, mtu: usize) -> usize {
    if payload_len <= 7 {
        1
    } else {
        let payload_len = payload_len + 2;
        payload_len / (mtu - 1) + (payload_len % (mtu - 1) != 0) as usize
    }
}

pub struct RefSlicer<'a, const MTU: usize> {
    slicer: Slicer<'a, MTU>
}

impl<'a, const MTU: usize> Iterator for RefSlicer<'a, MTU> {
    type Item = (&'a [u8], OwnedSlice<3>);

    fn next(&mut self) -> Option<Self::Item> {
        match self.slicer.tail_bytes.next() {
            Some(tail_byte) => {
                let tail_byte = tail_byte.as_byte();
                match self.slicer.chunks.next() {
                    Some(chunk) => {
                        if chunk.len() <= MTU - 1 - 2 { // mtu8: len() <= 5 (2 byte crc and tail byte will fit)
                            if self.slicer.crc_bytes_left == 2 {
                                self.slicer.crc_bytes_left -= 2;
                                Some((chunk, OwnedSlice::new_three(self.slicer.crc[0], self.slicer.crc[1], tail_byte)))
                            } else if self.slicer.crc_bytes_left == 1 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == MTU - 1 - 1 { // mtu8: len() == 6 (1 byte of crc and tail byte will fit)
                            if self.slicer.crc_bytes_left == 2 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[0], tail_byte)))
                            } else if self.slicer.crc_bytes_left == 1 {
                                self.slicer.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.slicer.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == MTU - 1 { // mtu8: len() == 7 (only tail byte will fit), single frame transfer or full frames from multi frame
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

pub struct OwnedSlicer<'a, const MTU: usize> {
    slicer: RefSlicer<'a, MTU>
}
impl<'a, const MTU: usize> Iterator for OwnedSlicer<'a, MTU> {
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
impl<'a, const MTU: usize> OwnedSlicer<'a, MTU> {
    #[cfg(feature = "vhrdcan")]
    pub fn vhrd(self, id: crate::types::CanId) -> VhrdOwnedSlicer<'a, MTU> {
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
pub struct VhrdOwnedSlicer<'a, const MTU: usize> {
    slicer: OwnedSlicer<'a, MTU>,
    id: vhrdcan::FrameId,
}
#[cfg(feature = "vhrdcan")]
impl<'a, const MTU: usize> Iterator for VhrdOwnedSlicer<'a, MTU> {
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

