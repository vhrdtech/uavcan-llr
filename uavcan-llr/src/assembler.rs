use crate::types::{TransferId, CanId, Priority, TransferKind, NodeId};
use crate::tailbyte::{TailByte, Kind};
use heapless::FnvIndexMap;
use hash32_derive::Hash32;
use core::fmt::Formatter;

/// Used to index array of transfer pieces (incoming frames + index of the next piece)
type PieceIdx = u16;
/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;
/// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
/// Must be able to hold 2 * MAX_TRANSFERS
type TransferSeq = i16;

#[derive(Copy, Clone)]
struct TransferMachine<const MTU: usize, const MAX_PIECES: usize> {
    state: State,
    transfer_id: Option<TransferId>,
    first_piece_idx: Option<PieceIdx>,
    last_piece_idx: Option<PieceIdx>,
    last_piece_len: PieceByteIdx,
}

#[derive(Copy, Clone, Debug)]
enum State {
    Empty,
    AssemblingT1,
    AssemblingT0,
    Done,
    Failure,
}

enum PayloadKind<'a> {
    Empty,
    LessThanMTU(&'a [u8]),
    ExactlyMTU(&'a [u8]),
    Invalid,
}

impl<const MTU: usize, const MAX_PIECES: usize> TransferMachine<MTU, MAX_PIECES> where [u8; MTU - 1]: Sized {
    const fn reset() -> Self {
        TransferMachine {
            state: State::Empty,
            transfer_id: None,
            first_piece_idx: None,
            last_piece_idx: None,
            last_piece_len: 0
        }
    }

    fn advance(&mut self, frame_payload: &[u8], storage: &mut PiecesStorage<{MTU - 1}, MAX_PIECES>) -> Option<PieceIdx> {
        if frame_payload.len() >= 1 && frame_payload.len() <= MTU {
            let tail_byte = TailByte::from(*frame_payload.last().unwrap());
            let payload_kind = if frame_payload.len() == 1 {
                PayloadKind::Empty
            } else if frame_payload.len() < MTU {
                PayloadKind::LessThanMTU(&frame_payload[..frame_payload.len() - 1])
            } else {
                PayloadKind::ExactlyMTU(&frame_payload[..frame_payload.len() - 1])
            };
            self.advance_internal(payload_kind, Some(tail_byte), storage)
        } else {
            self.advance_internal(PayloadKind::Invalid, None, storage)
        }
    }

    fn advance_internal(&mut self, payload_kind: PayloadKind, tail_byte: Option<TailByte>, storage: &mut PiecesStorage<{MTU - 1}, MAX_PIECES>) -> Option<PieceIdx> {
        use State::*;
        #[derive(Eq, PartialEq, Debug)]
        enum AdvanceAction {
            /// Ignore incoming frame data
            Ignore,
            /// Save incoming frame data into storage
            Push,
            /// Check CRC of the whole transfer, push on success or fail and wipe storage otherwise
            CheckCrcAndPush,
            /// Ignore incoming frame data and wipe storage from all previous pieces received
            Drop
        }
        use AdvanceAction::*;
        // If more space is needed, lower priority transfer storage will be wiped and exactly one
        // freed up slot will be used for this one.
        // first_piece index of that l.p. transfer is returned in such case, it is then looked up
        // and marked as Failure.
        let mut destroyed_lower_priority_transfer_first_piece_idx = None;
        println!("tail: {:?}, state: {:?}", tail_byte, self.state);
        let (next_state, action) = match (tail_byte, self.state) {
            // Got a frame without the tail byte
            // No need to remove pieces from storage
            (None, Empty | Failure) => {
                (Failure, Ignore)
            }
            // Need to remove pieces from storage
            // TODO: Ignore empty frames in the middle of a transfer?
            (None, AssemblingT1 | AssemblingT0) => {
                (Failure, Drop)
            }
            // Ignore and do not destroy valid transfer
            (None, Done) => {
                (Done, Ignore)
            }
            (Some(tail_byte), state) => {
                match (tail_byte.kind, state) {
                    // Single frame transfer from "idle" states, ok
                    (Kind::SingleFrame, Empty | Done | Failure) => (Done, Push),

                    // Single frame transfer in the middle of a multi-frame one, error
                    // TODO: Accept single frame transfer in the middle of an ongoing one?
                    (Kind::SingleFrame, AssemblingT1 | AssemblingT0) => (Failure, Drop),

                    // Start of a multi-frame transfer from "idle" states, ok
                    (Kind::MultiFrame, Empty | Done | Failure) => (AssemblingT1, Push),

                    // Repeated start in the middle of a multi-frame transfer, error
                    // TODO: Accept multi frame transfer in the middle of an ongoing one?
                    (Kind::MultiFrame, AssemblingT1 | AssemblingT0) => (Failure, Drop),

                    // Frame with toggle=0 after previous one with toggle=1, ok
                    (Kind::MiddleT0, AssemblingT1) => {
                        match payload_kind {
                            PayloadKind::ExactlyMTU(_) => (AssemblingT0, Push),
                            _ => (Failure, Drop)
                        }
                    }

                    // Last frame with toggle=0 after previous one with toggle=1, ok
                    (Kind::EndT0, AssemblingT1) => {
                        match payload_kind {
                            PayloadKind::LessThanMTU(_) | PayloadKind::ExactlyMTU(_) => (Done, CheckCrcAndPush),
                            _ => (Failure, Drop)
                        }
                    }

                    // Frame with toggle=0 after previous one with toggle=0, reorder error
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT0) => (Failure, Drop),

                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT0 | Kind::EndT0, _) => (Failure, Drop),

                    // Frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::MiddleT1, AssemblingT0) => {
                        match payload_kind {
                            PayloadKind::ExactlyMTU(_) => (AssemblingT1, Push),
                            _ => (Failure, Drop)
                        }
                    }

                    // Last frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::EndT1, AssemblingT0) => {
                        match payload_kind {
                            PayloadKind::LessThanMTU(_) | PayloadKind::ExactlyMTU(_) => (Done, CheckCrcAndPush),
                            _ => (Failure, Drop)
                        }
                    }

                    // Frame with toggle=1 after previous one with toggle=1, reorder error
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT1) => (Failure, Drop),

                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT1 | Kind::EndT1, _) => (Failure, Drop),

                    // UAVCAN Version 0 tail byte, error
                    (Kind::SingleFrameV0 | Kind::MultiFrameV0, _) => (Failure, Ignore)
                }
            }
        };
        self.state = next_state;
        println!("next_state: {:?}, action: {:?}", next_state, action);
        match action {
            Ignore => {}
            Push | CheckCrcAndPush => {
                if action == CheckCrcAndPush {
                    for chunk in storage.traverse(self.first_piece_idx.unwrap()) {
                        println!("traverse: {:?}", chunk)
                    }
                }

                let mut payload = [0u8; MTU - 1];
                match payload_kind {
                    PayloadKind::LessThanMTU(data) | PayloadKind::ExactlyMTU(data) => {
                        payload[0..data.len()].copy_from_slice(data);
                    }
                    _ => unreachable!()
                }
                match self.last_piece_idx {
                    Some(idx) => {
                        storage.push_after(payload, idx);
                    }
                    None => {
                        match storage.push(payload) {
                            Ok(idx) => {
                                self.first_piece_idx = Some(idx);
                                self.last_piece_idx = Some(idx);
                            },
                            _ => {
                                unreachable!()
                            }
                        }
                    }
                }
            }
            Drop => {
                self.first_piece_idx.map(|idx| storage.remove_all(idx));
                self.first_piece_idx = None;
                self.last_piece_idx = None;
            }
        }
        destroyed_lower_priority_transfer_first_piece_idx
    }

    fn fail_without_storage_cleaning(&mut self) {
        self.state = State::Failure;
    }

    // Fail this transfer, remove all pieces from storage and return a number of pieces removed
    fn fail_and_clean_storage(&mut self, storage: &mut PiecesStorage<{MTU - 1}, MAX_PIECES>) -> PieceIdx {
        self.state = State::Failure;
        self.first_piece_idx.map(|idx| storage.remove_all(idx)).unwrap_or(0)
    }
}

#[derive(Copy, Clone,)]
pub enum Piece<const N: usize> {
    Empty,
    /// bytes, next
    Filled([u8; N], PieceIdx)
}
struct PiecesStorage<const N: usize, const MAX_PIECES: usize> {
    items: [Piece<N>; MAX_PIECES],
    used: PieceIdx,
}
impl<const N: usize, const MAX_PIECES: usize> PiecesStorage<N, MAX_PIECES> {
    fn new() -> Self {
        PiecesStorage {
            items: [Piece::Empty; MAX_PIECES],
            used: 0
        }    
    }

    fn push(&mut self, data: [u8; N]) -> Result<PieceIdx, [u8; N]> {
        let empty_slot_idx = self.find_empty_slot().ok_or(data)?;
        self.items[empty_slot_idx as usize] = Piece::Filled(data, empty_slot_idx);
        self.used += 1;
        Ok(empty_slot_idx)
    }

    fn push_after(&mut self, data: [u8; N], after_piece_idx: PieceIdx) -> Result<(), [u8; N]> {
        let empty_slot_idx = self.find_empty_slot().ok_or(data)?;
        self.items[after_piece_idx as usize] = match self.items[after_piece_idx as usize] {
            Piece::Empty => return Err(data),
            Piece::Filled(data, next) => {
                Piece::Filled(data, empty_slot_idx)
            }
        };
        self.items[empty_slot_idx as usize] = Piece::Filled(data, empty_slot_idx);
        self.used += 1;
        Ok(())
    }

    fn traverse(&self, first_piece_idx: PieceIdx) -> PiecesIter<N, MAX_PIECES> {
        PiecesIter {
            items: &self.items,
            idx: Some(first_piece_idx)
        }
    }

    fn find_empty_slot(&self) -> Option<PieceIdx> {
        for (i, slot) in self.items.iter().enumerate() {
            match slot {
                Piece::Empty => {
                    return Some(i as PieceIdx);
                }
                Piece::Filled(_, _) => {
                    continue;
                }
            }
        }
        None
    }

    /// Remove all pieces starting from first_piece_idx and return an amount of items removed
    fn remove_all(&mut self, first_piece_idx: PieceIdx) -> PieceIdx {
        let mut idx = first_piece_idx;
        let mut removed = 0;
        loop {
            self.items[idx as usize] = match &self.items[idx as usize] {
                Piece::Empty => {
                    break;
                },
                Piece::Filled(_, next) => {
                    self.used -= 1;
                    removed += 1;
                    if idx == *next {
                        break;
                    }
                    idx = *next;
                    Piece::Empty
                }
            }
        }
        removed
    }

    fn len(&self) -> usize {
        self.used as usize
    }
}

struct PiecesIter<'a, const N: usize, const MAX_PIECES: usize> {
    items: &'a [Piece<N>; MAX_PIECES],
    idx: Option<PieceIdx>,
}
impl<'a, const N: usize, const MAX_PIECES: usize> Iterator for PiecesIter<'a, N, MAX_PIECES> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let (out, next_idx) = match self.idx {
            Some(idx) => {
                match &self.items[idx as usize] {
                    Piece::Empty => (None, None),
                    Piece::Filled(data, next_idx) => {
                        if idx == *next_idx {
                            (Some(&data[..]), None)
                        } else {
                            (Some(&data[..]), Some(*next_idx))
                        }
                    }
                }
            }
            None => (None, None)
        };
        self.idx = next_idx;
        out
    }
}

#[derive(Copy, Clone)]
struct Transfer<const MTU: usize, const MAX_PIECES: usize> {
    transfer_machine: TransferMachine<MTU, MAX_PIECES>,
    priority: Priority,
    sequence_number: TransferSeq,
    last_changed_timestamp: u32,

}

impl<const MTU: usize, const MAX_PIECES: usize> Transfer<MTU, MAX_PIECES> where [u8; MTU - 1]: Sized {
    fn new(priority: Priority, sequence_number: TransferSeq, time_now: u32) -> Self {
        Transfer {
            transfer_machine: TransferMachine::reset(),
            priority,
            sequence_number,
            last_changed_timestamp: time_now,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash32)]
struct TransfersMapKey {
    kind: TransferKind,
    source: NodeId,
}
impl From<CanId> for TransfersMapKey {
    fn from(can_id: CanId) -> Self {
        TransfersMapKey {
            kind: can_id.transfer_kind,
            source: can_id.source_node_id
        }
    }
}

pub struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize, const TRANSFER_LIFETIME: u32> where [u8; MTU - 1]: Sized {
    transfers: FnvIndexMap<TransfersMapKey, Transfer<MTU, MAX_PIECES>, MAX_TRANSFERS>,
    storage: PiecesStorage<{MTU - 1}, MAX_PIECES>,
    latest_sequence_number: TransferSeq,
}

impl<
    const MTU: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32
> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME> where [u8; MTU - 1]: Sized {
    pub fn new() -> Self {
        Assembler {
            transfers: FnvIndexMap::new(),
            storage: PiecesStorage::new(),
            latest_sequence_number: 0,
        }
    }

    pub fn process_frame(&mut self, id: CanId, payload: &[u8], time_now: u32) {
        // Remove outdated transfers (if any) to clean up space
        if self.storage.len() == MAX_PIECES {
            self.remove_outdated_transfers(time_now);
        }

        let key: TransfersMapKey = id.into();
        if !self.transfers.contains_key(&key) {
            if self.transfers.len() >= MAX_TRANSFERS {
                // No space left in transfers map
                // TODO: count
                return;
            }
            let mut transfer = Transfer::new(id.priority, self.latest_sequence_number, time_now);
            self.latest_sequence_number = self.latest_sequence_number.wrapping_add(1);
            // Will not fail because of the check above
            let _ = self.transfers.insert(key, transfer);
        }
        let transfer = match self.transfers.get_mut(&key) {
            Some(t) => t,
            _ => unreachable!()
        };
        match transfer.transfer_machine.advance(payload, &mut self.storage) {
            Some(idx) => self.fail_lower_priority_transfer(id.priority, idx),
            None => {}
        };
    }

    fn fail_lower_priority_transfer(&mut self, lower_than: Priority, first_piece_idx: PieceIdx) {
        for (_, transfer) in &mut self.transfers {
            match transfer.transfer_machine.first_piece_idx {
                Some(idx) => {
                    if transfer.priority < lower_than && idx == first_piece_idx {
                        transfer.transfer_machine.fail_without_storage_cleaning();
                    }
                }
                None => {}
            }
        }
    }

    fn remove_outdated_transfers(&mut self, time_now: u32) {
        for (_, transfer) in &mut self.transfers {
            if time_now - transfer.last_changed_timestamp > TRANSFER_LIFETIME {
                let slots_freed_up = transfer.transfer_machine.fail_and_clean_storage(&mut self.storage);
                if slots_freed_up >= 1 {
                    // Destroy only this transfer, since at least one slot is now free for new data
                    // and allow user to read out old transfers if it is slow
                    break;
                }
            }
        }
    }
}

impl<
    const MTU: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32
> core::fmt::Display for Assembler<MTU, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME> where [u8; MTU - 1]: Sized {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        for (key, transfer) in &self.transfers {
            write!(f, "{:?} {}", key.source, key.kind).ok();
        }
        write!(f, "")
    }
}