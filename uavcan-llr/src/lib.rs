#![cfg_attr(not(test), no_std)]
#[deny(warnings)]

pub mod types;
pub mod slicer;
pub mod tailbyte;
pub mod assembler;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Error {
    WrongReservedBit,
    NoneZeroHighBits,
    #[cfg(feature = "vhrdcan")]
    StandardIdNotSupported,
}

#[cfg(test)]
mod tests {
    extern crate std;
    use core::convert::TryFrom;
    use crate::types::*;
    use crate::tailbyte::TailByte;
    use crate::Error;
    use crate::slicer::{Slicer, OwnedSlice, frame_count};

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
    fn check_priority() {
        assert!(Priority::Low < Priority::High);
    }

    #[test]
    fn check_transfer_id() {
        assert_eq!(CanId::try_from(0b111 << 29), Err(Error::NoneZeroHighBits));

        let id0 = CanId::new_message_kind(
            NodeId::new(7).unwrap(),
            SubjectId::new(8).unwrap(),
            false,
            Priority::Nominal
        );
        assert_eq!(CanId::try_from(0b00010000_00000000_00001000_00000111), Ok(id0));
        let id0_u32: u32 = id0.into();
        assert_eq!(id0_u32, 0b00010000_00000000_00001000_00000111);

        let id1 = CanId::new_message_kind(
            NodeId::new(127).unwrap(),
            SubjectId::new(0).unwrap(),
            true,
            Priority::Nominal
        );
        assert_eq!(CanId::try_from(0b00010001_00000000_00000000_01111111), Ok(id1));
        let id1_u32: u32 = id1.into();
        assert_eq!(id1_u32, 0b00010001_00000000_00000000_01111111);

        assert_eq!(CanId::try_from(0b00010000_00000000_00001000_10000111), Err(Error::WrongReservedBit));
        assert_eq!(CanId::try_from(0b00010000_10000000_00001000_00000111), Err(Error::WrongReservedBit));

        let id2 = CanId::new_service_kind(
            NodeId::new(7).unwrap(),
            NodeId::new(7).unwrap(),
            ServiceId::new(511).unwrap(),
            false,
            Priority::Exceptional
        );
        assert_eq!(CanId::try_from(0b00000010_01111111_11000011_10000111), Ok(id2));
        let id2_u32: u32 = id2.into();
        assert_eq!(id2_u32, 0b00000010_01111111_11000011_10000111);

        let id3 = CanId::new_service_kind(
            NodeId::new(127).unwrap(),
            NodeId::new(0).unwrap(),
            ServiceId::new(511).unwrap(),
            true,
            Priority::Exceptional
        );
        assert_eq!(CanId::try_from(0b00000011_01111111_11000000_01111111), Ok(id3));
        let id3_u32: u32 = id3.into();
        assert_eq!(id3_u32, 0b00000011_01111111_11000000_01111111);
    }

    #[test]
    fn check_tailbyte() {
        assert_eq!(TailByte::new_single_frame(TransferId::new(10).unwrap()).as_byte(), 0b1110_1010);
        let mut multi = TailByte::new_multi_frame(TransferId::new(7).unwrap(), 0);
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::new_multi_frame(TransferId::new(7).unwrap(), 1);
        assert_eq!(multi.next(), Some(TailByte::from(0b1110_0111)));
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::new_multi_frame(TransferId::new(7).unwrap(), 2);
        assert_eq!(multi.next(), Some(TailByte::from(0b1010_0111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0100_0111)));
        assert_eq!(multi.next(), None);
        let mut multi = TailByte::new_multi_frame(TransferId::new(31).unwrap(), 3);
        assert_eq!(multi.next(), Some(TailByte::from(0b1011_1111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0001_1111)));
        assert_eq!(multi.next(), Some(TailByte::from(0b0111_1111)));
        assert_eq!(multi.next(), None);
    }

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

    #[test]
    fn check_assembler() {
        let payload = [0, 1, 2, 3, 4, 5, 6];
        let mut slicer = Slicer::<8, 7>::new(&payload, TransferId::new(0).unwrap()).frames_owned();
        let transfer_bytes = slicer.next().unwrap();

        // let mut assembler = Assembler::<8, 128, 8>::new();
        // let id = CanId::new_message_kind(NodeId::new(0).unwrap(), SubjectId::new(0).unwrap(), false, Priority::Nominal);
        // assembler.eat(id, &transfer_bytes);

    }
}
