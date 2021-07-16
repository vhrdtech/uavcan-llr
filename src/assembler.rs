use crate::types::{CanId, Priority, TransferId};
use core::convert::TryFrom;
use crate::tailbyte::TailByte;

/// Used to index array of transfer pieces (incoming frames + index of the next piece)
type PieceIdx = u16;
/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;

pub struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> where [(); MTU - 1]: Sized {
    pieces: [Piece<{MTU - 1}>; MAX_PIECES],
    transfers: [Transfer; MAX_TRANSFERS],
    counters: Counters,
}
impl<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS> where [(); MTU - 1]: Sized {
    pub fn new() -> Self {
        Assembler {
            pieces: [Piece::Empty; MAX_PIECES],
            transfers: [Transfer::Empty; MAX_TRANSFERS],
            counters: Counters::default(),
        }
    }

    pub fn eat(&mut self, id: CanId, bytes: &[u8]) {
        let transfer_id = self.find_transfer_by_id(id);
        match transfer_id {
            Some(i) => {
                // Safe because find_transfer() iterates only inside transfers array
                let transfer = self.transfers.get(i as usize).unwrap().clone();
                match transfer {
                    Transfer::Empty => {
                        // find_transfer returns non Empty transfers
                        unreachable!();
                    }
                    Transfer::Assembling(id, first_piece, last_piece, last_piece_len) => {
                        // New chunk of data came, find a place for it
                        match self.find_empty_slot() {
                            Some(i) => {
                                if bytes.len() == MTU - 1 { // Full frame, can be the last one also in a transfer
                                    let tail_byte = TailByte::from(*bytes.last().unwrap());
                                } else if bytes.len() < MTU - 1 && bytes.len() > 0 { // Only last frame or error
                                    let tail_byte = TailByte::from(*bytes.last().unwrap());
                                } else if bytes.len() == 0 { // error

                                } else { // > MTU-1, error

                                }

                            }
                            None => {
                                // no empty slots, destroy some lower priority transfer and free up
                                loop {
                                    match self.find_lower_priority_transfer(id.priority) {
                                        Some(lower_i) => {
                                            let lp_transfer = self.transfers.get(lower_i as usize).unwrap().clone();
                                            match lp_transfer {
                                                Transfer::Empty => {
                                                    unreachable!()
                                                }
                                                Transfer::Assembling(_, first_piece, _, _) => {
                                                    self.remove_pieces(first_piece);
                                                    break;
                                                }
                                                Transfer::Done(_, first_piece) => {
                                                    self.remove_pieces(first_piece);
                                                    break;
                                                }
                                                Transfer::Error(_) => {
                                                    // Errored transfers pieces are cleaned up right away on error
                                                    continue;
                                                }
                                            }
                                        }
                                        None => {
                                            // no space left and no other transfers can be replaced, this frame and whole transfer will be lost
                                            self.remove_transfer(i);
                                            self.remove_pieces(first_piece);
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Transfer::Done(id, first_piece) => {
                        // Transfer is marked as done, but more data came due to duplication?
                        self.counters.duplicate_frames += 1;
                    }
                    Transfer::Error(id) => {
                        // Error was already detected, probably due to reordering or missing frames
                        // Remaining frames are still coming and dropped here
                    }
                }
            },
            None => {
                // Potentially frame from a new transfer
            }
        }
    }

    fn find_transfer_by_id(&self, id: CanId) -> Option<TransferIdx> {
        use Transfer::*;
        for (i, t) in self.transfers.iter().enumerate() {
            match t {
                Empty => {}
                Assembling(idu, _, _, _) | Done(idu, _) | Error(idu) => {
                    if id == *idu {
                        return Some(i as TransferIdx)
                    }
                }
            }
        }
        None
    }

    fn find_lower_priority_transfer(&self, lower_than: Priority) -> Option<TransferIdx> {
        use Transfer::*;
        for (i, t) in self.transfers.iter().enumerate() {
            match t {
                Empty => {},
                Assembling(id, _, _, _) | Done(id, _) | Error(id) => {
                    if id.priority < lower_than {
                        return Some(i as TransferIdx);
                    }
                }
            }
        }
        None
    }

    fn remove_transfer(&mut self, i: TransferIdx) {
        match self.transfers.get(i as usize).unwrap() {
            Transfer::Empty => {}
            Transfer::Assembling(_, first_piece, _, _) => {}
            Transfer::Done(_, first_piece) => {}
            Transfer::Error(_) => {}
        }
        *self.transfers.get_mut(i as usize).unwrap() = Transfer::Empty;
    }

    fn remove_pieces(&mut self, first: PieceIdx) {
        let mut current = first;
        loop {
            let next = match self.pieces.get(current as usize).unwrap() {
                Piece::Empty => {
                    return;
                }
                Piece::Filled(_, next) => { *next }
            };
            *self.pieces.get_mut(current as usize).unwrap() = Piece::Empty;
            if current == next {
                return;
            }
            current = next;
        }
    }

    fn find_empty_slot(&self) -> Option<PieceIdx> {
        for (i, piece) in self.pieces.iter().enumerate() {
            match piece {
                Piece::Empty => {
                    return Some(i as PieceIdx);
                }
                Piece::Filled(_, _) => {}
            }
        }
        None
    }
}

#[derive(Copy, Clone,)]
pub enum Piece<const N: usize> {
    Empty,
    /// bytes, next
    Filled([u8; N], PieceIdx)
}

#[derive(Copy, Clone,)]
pub enum Transfer {
    /// Empty transfer slot, can be used for new incoming transfers of any priority
    Empty,
    /// Re-assembly is in progress and no errors have been observed so far
    /// id, first_piece, last_piece, last_piece_bytes_used
    Assembling(CanId, PieceIdx, PieceIdx, PieceByteIdx),
    /// Re-assembly is done, CRC check succeeded, transfer should be read out
    /// id, first_piece
    Done(CanId, PieceIdx),
    /// Something went wrong
    Error(CanId),
}
struct TransferAssembling {
    can_id: CanId,
    transfer_id: TransferId,
    first_piece: PieceIdx,
    last_piece: PieceIdx,
    last_piece_bytes_used: PieceByteIdx,
}

#[derive(Copy, Clone, Default)]
pub struct Counters {
    /// Incremented if a frame is received that belongs to a transfer already reassembled
    pub duplicate_frames: u32,

}