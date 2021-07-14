#[deny(warnings)]

use crate::types::TransferId;
use core::slice::Chunks;
use crate::tailbyte::TailByteIter;
use core::ops::Deref;

pub struct Slicer<'a> {
    chunks: Chunks<'a, u8>,
    max_chunk_len: usize, // = mtu - 1
    crc: [u8; 2],
    crc_bytes_left: u8, // = 2, 1 or 0
    tail_bytes: TailByteIter,
}

impl<'a, 'b> Slicer<'a> {
    pub fn new(payload: &'a[u8], transfer_id: TransferId, mtu: usize) -> Slicer<'a> {
        let max_chunk_len = mtu - 1;
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
        let tail_bytes = crate::tailbyte::TailByte::multi_frame_transfer(
            transfer_id,
            Self::frame_count(payload.len(), mtu)
        );

        Slicer {
            chunks,
            max_chunk_len,
            crc,
            crc_bytes_left,
            tail_bytes,
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
}

impl<'a> Iterator for Slicer<'a> {
    type Item = (&'a [u8], OwnedSlice);

    fn next(&mut self) -> Option<Self::Item> {
        match self.tail_bytes.next() {
            Some(tail_byte) => {
                let tail_byte = tail_byte.as_byte();
                match self.chunks.next() {
                    Some(chunk) => {
                        if chunk.len() <= self.max_chunk_len - 2 { // mtu8: len() <= 5 (2 byte crc and tail byte will fit)
                            if self.crc_bytes_left == 2 {
                                self.crc_bytes_left -= 2;
                                Some((chunk, OwnedSlice::new([self.crc[0], self.crc[1], tail_byte])))
                            } else if self.crc_bytes_left == 1 {
                                self.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == self.max_chunk_len - 1 { // mtu8: len() == 6 (1 byte of crc and tail byte will fit)
                            if self.crc_bytes_left == 2 {
                                self.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.crc[0], tail_byte)))
                            } else if self.crc_bytes_left == 1 {
                                self.crc_bytes_left -= 1;
                                Some((chunk, OwnedSlice::new_two(self.crc[1], tail_byte)))
                            } else { // can only be 2 or 1 bytes of crc left for transmission
                                unreachable!()
                            }
                        } else if chunk.len() == self.max_chunk_len { // mtu8: len() == 7 (only tail byte will fit), single frame transfer or full frames from multi frame
                            Some((chunk, OwnedSlice::new_one(tail_byte)))
                        } else { // size should be <= max_chunk_len=mtu-1 since we chunk input payload that way
                            unreachable!()
                        }
                    },
                    None => {
                        if self.crc_bytes_left == 2 {
                            self.crc_bytes_left -= 2;
                            Some((&[], OwnedSlice::new([self.crc[0], self.crc[1], tail_byte])))
                        } else if self.crc_bytes_left == 1 {
                            self.crc_bytes_left -= 1;
                            Some((&[], OwnedSlice::new_two(self.crc[1], tail_byte)))
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

pub struct OwnedSlice {
    bytes: [u8; 3],
    used: u8,
}
impl OwnedSlice {
    pub fn new_one(byte: u8) -> Self {
        OwnedSlice {
            bytes: [byte, 0, 0],
            used: 1
        }
    }
    pub fn new_two(byte0: u8, byte1: u8) -> Self {
        OwnedSlice {
            bytes: [byte0, byte1, 0],
            used: 2
        }
    }
    pub fn new(bytes: [u8; 3]) -> Self {
        OwnedSlice {
            bytes,
            used: 3
        }
    }
}
impl Deref for OwnedSlice {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes[0..self.used as usize]
    }
}