#[deny(warnings)]

use crate::types::TransferId;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TailByte {
    pub kind: Kind,
    pub id: TransferId,
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Kind {
    /// start = 0, end = 0, toggle = 0
    MiddleT0 = 0b000,
    /// start = 0, end = 0, toggle = 1
    MiddleT1 = 0b001,
    /// start = 0, end = 1, toggle = 0
    EndT0 = 0b010,
    /// start = 0, end = 1, toggle = 1
    EndT1 = 0b011,
    /// start = 1, end = 0, toggle = 0
    MultiFrameV0 = 0b100,
    /// start = 1, end = 0, toggle = 1
    MultiFrame = 0b101,
    /// start = 1, end = 1, toggle = 0
    SingleFrameV0 = 0b110,
    /// start = 1, end = 1, toggle = 1
    SingleFrame = 0b111,
}
impl From<u8> for Kind {
    fn from(val: u8) -> Self {
        use Kind::*;
        match val & 0b111 {
            0b000 => MiddleT0,
            0b001 => MiddleT1,
            0b010 => EndT0,
            0b011 => EndT1,
            0b100 => MultiFrameV0,
            0b101 => MultiFrame,
            0b110 => SingleFrameV0,
            0b111 => SingleFrame,
            _ => unreachable!()
        }
    }
}

impl TailByte {
    pub fn new_single_frame(id: TransferId) -> TailByte {
        TailByte {
            kind: Kind::SingleFrame,
            id
        }
    }

    pub fn new_multi_frame(id: TransferId, frame_count: usize) -> TailByteIter {
        TailByteIter {
            tail_byte: TailByte {
                kind: if frame_count > 1 { Kind::MultiFrame } else { Kind::SingleFrame },
                id
            },
            current_frame: 0,
            frame_count
        }
    }

    pub fn as_byte(&self) -> u8 {
        ((self.kind as u8) << 5) | self.id.inner()
    }

    pub fn is_multi_frame_middle(&self) -> bool {
        match self.kind {
            Kind::MiddleT0 | Kind::MiddleT1 => true,
            _ => false
        }
    }

    pub fn is_multi_frame_end(&self) -> bool {
        match self.kind {
            Kind::EndT0 | Kind::EndT1 => true,
            _ => false
        }
    }
}
impl From<u8> for TailByte {
    fn from(byte: u8) -> Self {
        TailByte {
            kind: (byte >> 5).into(),
            // NOTE: unwrap: TransferId is 5 bits wide, will not fail
            id: TransferId::new(byte & 0b0001_1111).unwrap()
        }
    }
}
impl Into<u8> for TailByte {
    fn into(self) -> u8 {
        self.as_byte()
    }
}

pub struct TailByteIter {
    tail_byte: TailByte,
    current_frame: usize,
    frame_count: usize,
}
impl Iterator for TailByteIter {
    type Item = TailByte;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_frame == self.frame_count {
            None
        } else {
            if self.current_frame == self.frame_count - 1 && self.tail_byte.kind != Kind::SingleFrame {
                self.tail_byte.kind = if self.tail_byte.kind == Kind::MiddleT0 {
                    Kind::EndT1
                } else {
                    Kind::EndT0
                };
            } else if self.current_frame == 1 {
                self.tail_byte.kind = Kind::MiddleT0;
            } else if self.current_frame != 0 {
                self.tail_byte.kind = if self.tail_byte.kind == Kind::MiddleT0 {
                    Kind::MiddleT1
                } else {
                    Kind::MiddleT0
                };
            }
            self.current_frame += 1;
            Some(self.tail_byte)
        }
    }
}