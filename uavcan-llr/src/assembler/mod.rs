pub mod assembler;
pub use assembler::Assembler;

mod storage;
mod transfer;

// Various types used instead of usize.
// TODO: Check if this actually produces smaller memory footprint.
pub(crate) mod types {
    /// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
    /// Must be able to hold 2 * MAX_TRANSFERS
    pub(crate) type TransferSeq = i16;
    /// Used to index into frame data
    pub(crate) type PieceByteIdx = u8;
    /// Used to index array of transfer pieces (incoming frames + index of the next piece)
    pub(crate) type PieceIdx = u16;
}
