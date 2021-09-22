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

