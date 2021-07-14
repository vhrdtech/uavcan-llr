#![cfg_attr(not(test), no_std)]
#[deny(warnings)]

pub mod types;
pub mod slicer;
pub mod assembler;
pub mod tailbyte;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Error {
    WrongReservedBit,
    NoneZeroHighBits,
}

#[cfg(test)]
mod tests {
    extern crate std;
    use core::convert::TryFrom;
    use crate::types::*;
    use crate::tailbyte::TailByte;
    use crate::Error;
    use crate::slicer::frame_count;

    #[test]
    fn check_ids() {
        assert!(NodeId::new(10).is_some());
        assert!(NodeId::new(128).is_none());
        assert!(SubjectId::new(10).is_some());
        assert!(SubjectId::new(8192).is_none());
        assert!(ServiceId::new(10).is_some());
        assert!(ServiceId::new(512).is_none());
    }

    #[test]
    fn check_transfer_id() {
        assert_eq!(CanId::try_from(0b111 << 29), Err(Error::NoneZeroHighBits));
        assert_eq!(CanId::try_from(0b00010000_00000000_00001000_00000111), Ok(CanId::new_message_kind(
            NodeId::new(7).unwrap(),
            SubjectId::new(8).unwrap(),
            false,
            Priority::Nominal
        )));
        assert_eq!(CanId::try_from(0b00010001_00000000_00000000_01111111), Ok(CanId::new_message_kind(
            NodeId::new(127).unwrap(),
            SubjectId::new(0).unwrap(),
            true,
            Priority::Nominal
        )));
        assert_eq!(CanId::try_from(0b00010000_00000000_00001000_10000111), Err(Error::WrongReservedBit));
        assert_eq!(CanId::try_from(0b00010000_10000000_00001000_00000111), Err(Error::WrongReservedBit));
        assert_eq!(CanId::try_from(0b00000010_01111111_11000011_10000111), Ok(CanId::new_service_kind(
            NodeId::new(7).unwrap(),
            NodeId::new(7).unwrap(),
            ServiceId::new(511).unwrap(),
            false,
            Priority::Exceptional
        )));
        assert_eq!(CanId::try_from(0b00000011_01111111_11000000_01111111), Ok(CanId::new_service_kind(
            NodeId::new(127).unwrap(),
            NodeId::new(0).unwrap(),
            ServiceId::new(511).unwrap(),
            true,
            Priority::Exceptional
        )));
    }

    #[test]
    fn check_tailbyte() {
        assert_eq!(TailByte::single_frame_transfer(TransferId::new(10).unwrap()).as_byte(), 0b1110_1010);
        let mut multi = TailByte::multi_frame_transfer(TransferId::new(7).unwrap(), 0);
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::multi_frame_transfer(TransferId::new(7).unwrap(), 1);
        assert_eq!(multi.next(), Some(TailByte::from(0b1110_0111)));
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::multi_frame_transfer(TransferId::new(7).unwrap(), 2);
        assert_eq!(multi.next(), Some(TailByte::from(0b1010_0111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0100_0111)));
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::multi_frame_transfer(TransferId::new(31).unwrap(), 3);
        assert_eq!(multi.next(), Some(TailByte::from(0b1011_1111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0001_1111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0111_1111)));
        assert_eq!(multi.next(), None);
    }

    #[test]
    fn check_frame_count() {
        assert_eq!(frame_count(0, 8), 1);
        assert_eq!(frame_count(1, 8), 1);
        assert_eq!(frame_count(6, 8), 1);
        assert_eq!(frame_count(7, 8), 1);

        assert_eq!(frame_count(8, 8), 2);  // 7+t 1+crc+t
        assert_eq!(frame_count(12, 8), 2); // 7+t 5+crc+t

        assert_eq!(frame_count(13, 8), 3); // 7+t 6+cr+t c+t
        assert_eq!(frame_count(14, 8), 3); // 7+t 7+t 2+t
        assert_eq!(frame_count(19, 8), 3); // 7+t 7+t 5+crc+t

        assert_eq!(frame_count(20, 8), 4); // 7+t 7+t 6+cr+t c+t
        assert_eq!(frame_count(21, 8), 4); // 7+t 7+t 7+t crc+t
        assert_eq!(frame_count(26, 8), 4); // 7+t 7+t 7+t 5+crc+t
    }
}
