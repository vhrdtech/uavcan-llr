use crate::types::{CanId, Priority, TransferId};
use crate::tailbyte::TailByte;

/// Used to index array of transfer pieces (incoming frames + index of the next piece)
type PieceIdx = u16;
/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;
/// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
/// Must be able to hold 2 * MAX_TRANSFERS
type TransferSeq = i16;

pub struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> where [(); MTU - 1]: Sized {
    pieces: [Piece<{MTU - 1}>; MAX_PIECES],
    transfers: [Transfer; MAX_TRANSFERS],
    transfers_counter: TransferSeq,
    counters: Counters,
}
impl<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS> where [(); MTU - 1]: Sized {
    pub fn new() -> Self {
        Assembler {
            pieces: [Piece::Empty; MAX_PIECES],
            transfers: [Transfer::Empty; MAX_TRANSFERS],
            transfers_counter: 0,
            counters: Counters::default(),
        }
    }

    pub fn eat(&mut self, id: CanId, bytes: &[u8]) {
        let transfer_id = self.find_transfer_by_id(id);
        match transfer_id {
            Some(i) => {
                // Safe because find_transfer() iterates only inside transfers array
                let transfer = self.transfers.get_mut(i as usize).unwrap();
                *transfer = match transfer {
                    Transfer::Empty => {
                        // find_transfer returns non Empty transfers
                        // unreachable!();
                        Transfer::Empty
                    }
                    Transfer::Assembling(id, t) => {
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
                                        Some(lp_transfer) => {
                                            match lp_transfer {
                                                Transfer::Empty => {
                                                    unreachable!()
                                                }
                                                Transfer::Assembling(_, ta) => {
                                                    self.remove_pieces(ta.first_piece);
                                                    self.counters.destroyed_other_assembling += 1;
                                                    break;
                                                }
                                                Transfer::Done(_, td) => {
                                                    self.remove_pieces(td.first_piece);
                                                    self.counters.destroyed_while_done += 1;
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
                                            self.remove_pieces(t.first_piece);
                                            self.counters.destroyed_self_assembling += 1;
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        *transfer
                    }
                    Transfer::Done(id, first_piece) => {
                        // Transfer is marked as done, but more data came due to duplication?
                        self.counters.duplicate_frames += 1;
                        *transfer
                    }
                    Transfer::Error(id) => {
                        // Error was already detected, probably due to reordering or missing frames
                        // Remaining frames are still coming and dropped here
                        *transfer
                    }
                };
            },
            None => {
                // Potentially frame from a new transfer
            }
        }
    }

    fn get_transfer(&self, idx: TransferIdx) -> &mut Transfer {
        self.transfers.as_ptr()
    }

    fn find_transfer_by_id(&self, id: CanId) -> Option<TransferIdx> {
        use Transfer::*;
        for (i, t) in self.transfers.iter().enumerate() {
            match t {
                Empty => {}
                Assembling(idu, _) | Done(idu, _) | Error(idu) => {
                    if id == *idu {
                        return Some(i as TransferIdx)
                    }
                }
            }
        }
        None
    }

    fn find_lower_priority_transfer(&self, lower_than: Priority) -> Option<&Transfer> {
        use Transfer::*;
        for (i, t) in self.transfers.iter().enumerate() {
            match t {
                Empty => {},
                Assembling(id, _) | Done(id, _) | Error(id) => {
                    if id.priority < lower_than {
                        return Some(unsafe { self.transfers.get_unchecked(i) });
                    }
                }
            }
        }
        None
    }

    fn find_max_priority_done_transfer(&self) -> Option<TransferIdx> {
        use Transfer::*;
        let mut max_transfer: Option<(CanId, TransferIdx, TransferSeq)> = None;
        for (i, t) in self.transfers.iter().enumerate() {
            match t {
                Done(can_id, t) => {
                    max_transfer = match max_transfer {
                        Some((max_can_id, max_transfer_id, max_transfer_seq)) => {
                            // higher priority wins, same priority transfers in fifo order
                            if can_id.priority > max_can_id.priority {
                                Some((*can_id, i as TransferIdx, t.transfer_seq))
                            } else if can_id.priority == max_can_id.priority && max_transfer_seq.wrapping_sub(t.transfer_seq) < 0 {
                                Some((*can_id, i as TransferIdx, t.transfer_seq))
                            } else {
                                max_transfer
                            }
                        },
                        None => {
                            Some((*can_id, i as TransferIdx, t.transfer_seq))
                        }
                    };
                },
                _ => {}
            }
        }
        max_transfer.map(|mt| mt.1)
    }

    fn remove_transfer(&mut self, i: TransferIdx) {
        match self.transfers.get(i as usize).unwrap() {
            Transfer::Empty => {}
            Transfer::Assembling(_, _) => {}
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

    pub fn pop<'a>(&mut self, assemble_buf: &'a mut [u8]) -> Option<(CanId, &'a [u8])> {
        match self.find_max_priority_done_transfer() {
            Some(transfer_id) => {
                match self.transfers.get(transfer_id as usize).unwrap() {
                    Transfer::Done(can_id, t) => {
                        let mut i = 0;
                        let mut piece_id = t.first_piece;
                        loop {
                            match self.pieces.get(piece_id as usize).unwrap() {
                                Piece::Empty => {
                                    // shouldn't happen
                                    self.counters.pop_got_empty_piece += 1;
                                    break;
                                },
                                Piece::Filled(data, next) => {
                                    if *next != piece_id {
                                        piece_id = *next;
                                        assemble_buf[i .. i+data.len()].copy_from_slice(data);
                                        i += data.len();
                                    } else { // last piece
                                        if t.last_piece_bytes_used > 0 {
                                            assemble_buf[i .. i + usize::from(t.last_piece_bytes_used)].copy_from_slice(data);
                                            i += usize::from(t.last_piece_bytes_used);
                                        }
                                        break;
                                    }
                                }

                            }
                        }
                        Some((*can_id, &assemble_buf[..i]))
                    },
                    _ => {
                        None
                    }
                }
            }
            None => {
                None
            }
        }
    }
}

#[derive(Copy, Clone,)]
pub enum Piece<const N: usize> {
    Empty,
    /// bytes, next
    Filled([u8; N], PieceIdx)
}

#[derive(Copy, Clone,)]
enum Transfer {
    /// Empty transfer slot, can be used for new incoming transfers of any priority
    Empty,
    /// Re-assembly is in progress and no errors have been observed so far
    Assembling(CanId, TransferAssembling),
    /// Re-assembly is done, CRC check succeeded, transfer should be read out
    Done(CanId, TransferDone),
    /// Something went wrong
    Error(CanId),
}
#[derive(Copy, Clone,)]
struct TransferAssembling {
    transfer_id: TransferId,
    transfer_counter: TransferSeq,
    first_piece: PieceIdx,
    last_piece: PieceIdx,
    last_piece_bytes_used: PieceByteIdx,
}

#[derive(Copy, Clone,)]
struct TransferDone {
    first_piece: PieceIdx,
    last_piece_bytes_used: PieceByteIdx,
    transfer_seq: TransferSeq,
}

#[derive(Copy, Clone, Default)]
pub struct Counters {
    /// Incremented if a frame is received that belongs to a transfer already reassembled
    pub duplicate_frames: u32,
    pub destroyed_other_assembling: u32,
    pub destroyed_other_done: u32,
    pub destroyed_self_assembling: u32,

    pub pop_got_empty_piece: u32,
}