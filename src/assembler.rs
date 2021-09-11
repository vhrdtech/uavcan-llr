use crate::types::{TransferId, CanId, Priority, TransferKind, NodeId};
use crate::tailbyte::{TailByte, Kind};
use heapless::FnvIndexMap;
use hash32_derive::Hash32;

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

#[derive(Copy, Clone)]
enum State {
    Empty,
    AssemblingT1,
    AssemblingT0,
    Done,
    Failure,
}

impl<const MTU: usize, const MAX_PIECES: usize> TransferMachine<MTU, MAX_PIECES> {
    const fn reset() -> Self {
        TransferMachine {
            state: State::Empty,
            transfer_id: None,
            first_piece_idx: None,
            last_piece_idx: None,
            last_piece_len: 0
        }
    }

    fn advance(&mut self, frame_payload: &[u8], storage: &mut PiecesStorage<MAX_PIECES>) -> Option<PieceIdx> {
        if frame_payload.len() >= 1 && frame_payload.len() <= MTU {
            let tail_byte = TailByte::from(*frame_payload.last().unwrap());
            self.advance_internal(Some(tail_byte), storage)
        } else {
            self.advance_internal(None, storage)
        }
    }

    fn advance_internal(&mut self, tail_byte: Option<TailByte>, storage: &mut PiecesStorage<MAX_PIECES>) -> Option<PieceIdx> {
        use State::*;
        enum AdvanceAction {
            /// Ignore incoming frame data
            Ignore,
            /// Save incoming frame data into storage
            Push,
            /// Ignore incoming frame data and wipe storage from all previous pieces received
            Drop
        }
        use AdvanceAction::*;
        // If more space is needed, lower priority transfer storage will be wiped and exactly one
        // freed up slot will be used for this one.
        // first_piece index of that l.p. transfer is returned in such case, it is then looked up
        // and marked as Failure.
        let mut destroyed_lower_priority_transfer_first_piece_idx = None;
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
                    (Kind::SingleFrame, Empty | Done | Failure) => {

                        (Done, Push)
                    }
                    // Single frame transfer in the middle of a multi-frame one, error
                    // TODO: Accept single frame transfer in the middle of an ongoing one?
                    (Kind::SingleFrame, AssemblingT1 | AssemblingT0) => {
                        (Failure, Drop)
                    }

                    // Start of a multi-frame transfer from "idle" states, ok
                    (Kind::MultiFrame, Empty | Done | Failure) => {

                        (AssemblingT1, Push)
                    }
                    // Repeated start in the middle of a multi-frame transfer, error
                    // TODO: Accept multi frame transfer in the middle of an ongoing one?
                    (Kind::MultiFrame, AssemblingT1 | AssemblingT0) => {

                        (Failure, Drop)
                    }

                    // Frame with toggle=0 after previous one with toggle=1, ok
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT1) => {

                        (AssemblingT0, Push)
                    }
                    // Frame with toggle=0 after previous one with toggle=0, reorder error
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT0) => {

                        (Failure, Drop)
                    }
                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT0 | Kind::EndT0, _) => {

                        (Failure, Drop)
                    }

                    // Frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT0) => {

                        (AssemblingT1, Push)
                    }
                    // Frame with toggle=1 after previous one with toggle=1, reorder error
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT1) => {

                        (Failure, Drop)
                    }
                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT1 | Kind::EndT1, _) => {

                        (Failure, Drop)
                    }

                    // UAVCAN Version 0 tail byte, error
                    (Kind::SingleFrameV0 | Kind::MultiFrameV0, _) => {

                        (Failure, Ignore)
                    }
                }
            }
        };
        self.state = next_state;
        match action {
            Ignore => {}
            Push => {
                match self.last_piece_idx {
                    Some(idx) => {

                    }
                    None => {

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
    fn fail_and_clean_storage(&mut self) -> PieceIdx {
        todo!()
    }
}

struct PiecesStorage<const MAX_PIECES: usize> {

}

impl<const MAX_PIECES: usize> PiecesStorage<MAX_PIECES> {
    fn remove_all(&mut self, first_piece_idx: PieceIdx) {

    }

    fn len(&self) -> usize {
        todo!()
    }
}

#[derive(Copy, Clone)]
struct Transfer<const MTU: usize, const MAX_PIECES: usize> {
    transfer_machine: TransferMachine<MTU, MAX_PIECES>,
    priority: Priority,
    sequence_number: TransferSeq,
    last_changed_timestamp: u32,

}

impl<const MTU: usize, const MAX_PIECES: usize> Transfer<MTU, MAX_PIECES> {
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


pub struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize, const TRANSFER_LIFETIME: u32> {
    transfers: FnvIndexMap<TransfersMapKey, Transfer<MTU, MAX_PIECES>, MAX_TRANSFERS>,
    storage: PiecesStorage<MAX_PIECES>,
    latest_sequence_number: TransferSeq,
}

impl<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize, const TRANSFER_LIFETIME: u32> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME> {
    pub fn new() -> Self {
        Assembler {
            transfers: FnvIndexMap::new(),
            storage: PiecesStorage {},
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
                let slots_freed_up = transfer.transfer_machine.fail_and_clean_storage();
                if slots_freed_up >= 1 {
                    // Destroy only this transfer, since at least one slot is now free for new data
                    // and allow user to read out old transfers if it is slow
                    break;
                }
            }
        }
    }
}