#[deny(warnings)]

use crate::types::TransferId;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TailByte {
    pub start_of_transfer: bool,
    pub end_of_transfer: bool,
    pub toggle_bit: bool,
    pub id: TransferId,
}
pub enum Kind {
    SingleFrame,
    SingleFrameV0,
    Start,
    StartV0,
    MiddleT1,
    MiddleT0,
    EndT1,
    EndT0,
}
impl TailByte {
    pub fn single_frame_transfer(id: TransferId) -> TailByte {
        TailByte {
            start_of_transfer: true,
            end_of_transfer: true,
            toggle_bit: true,
            id
        }
    }

    pub fn multi_frame_transfer(id: TransferId, frame_count: usize) -> TailByteIter {
        TailByteIter {
            tail_byte: TailByte {
                start_of_transfer: true,
                end_of_transfer: false,
                toggle_bit: true,
                id
            },
            current_frame: 0,
            frame_count
        }
    }

    pub fn as_byte(&self) -> u8 {
        ((self.start_of_transfer as u8) << 7) |
        ((self.end_of_transfer as u8) << 6) |
        ((self.toggle_bit as u8) << 5) |
        self.id.inner()
    }

    pub fn kind(&self) -> Kind {
        Kind::EndT0
    }
}
impl From<u8> for TailByte {
    fn from(byte: u8) -> Self {
        TailByte {
            start_of_transfer: byte & (1 << 7) != 0,
            end_of_transfer: byte & (1 << 6) != 0,
            toggle_bit: byte & (1 << 5) != 0,
            id: unsafe { TransferId::new_unchecked(byte & 31) }
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
            if self.current_frame == 1 {
                self.tail_byte.start_of_transfer = false;
            }
            if self.current_frame == self.frame_count - 1 {
                self.tail_byte.end_of_transfer = true;
            }
            let r = Some(self.tail_byte);
            self.current_frame += 1;
            self.tail_byte.toggle_bit = !self.tail_byte.toggle_bit;
            r
        }
    }
}