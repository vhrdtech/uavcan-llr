use crate::tailbyte::{Kind, TailByte};
use crate::types::{CanId, NodeId, Priority, TransferId, TransferKind};
use core::fmt::Formatter;
use hash32_derive::Hash32;
use heapless::FnvIndexMap;
use super::storage::{PieceIdx, PiecesStorage};


/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;
/// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
/// Must be able to hold 2 * MAX_TRANSFERS
type TransferSeq = i16;

#[derive(Copy, Clone)]
struct TransferMachine<const MTU: usize> {
    state: State,
    transfer_id: Option<TransferId>,
}
impl<const MTU: usize>  core::fmt::Display for TransferMachine<MTU> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TM({:?}, Tid:{:?}",
               self.state,
               self.transfer_id,
        )
    }
}

#[derive(Copy, Clone, Debug)]
enum State {
    Empty,
    AssemblingT1,
    AssemblingT0,
    Done,
    Failure,
}

enum PayloadKind {
    Empty,
    LessThanMTU,
    ExactlyMTU,
    Invalid,
}

#[derive(Eq, PartialEq, Debug)]
enum TransferMachineOutput {
    /// Ignore incoming frame data
    Ignore,
    /// Save incoming frame data into storage
    Push,
    /// Check CRC of the whole transfer, push on success or fail and wipe storage otherwise
    CheckCrcAndPush,
    /// Ignore incoming frame data and wipe storage from all previous pieces received
    Drop,
}

impl<const MTU: usize> TransferMachine<MTU>
{
    const fn reset() -> Self {
        TransferMachine {
            state: State::Empty,
            transfer_id: None,
        }
    }

    fn advance(
        &mut self,
        payload_kind: PayloadKind,
        tail_byte: Option<TailByte>,
    ) -> TransferMachineOutput {
        use State::*;
        use TransferMachineOutput::*;

        // println!("tail: {:?}, state: {:?}", tail_byte, self.state);
        let (next_state, output) = match (tail_byte, self.state) {
            // Got a frame without the tail byte
            // No need to remove pieces from storage
            (None, Empty | Failure) => (Failure, Ignore),
            // Need to remove pieces from storage
            // TODO: Ignore empty frames in the middle of a transfer?
            (None, AssemblingT1 | AssemblingT0) => (Failure, Drop),
            // Ignore and do not destroy valid transfer
            (None, Done) => (Done, Ignore),
            (Some(tail_byte), state) => {
                self.transfer_id = Some(tail_byte.id);
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
                    (Kind::MiddleT0, AssemblingT1) => match payload_kind {
                        PayloadKind::ExactlyMTU => (AssemblingT0, Push),
                        _ => (Failure, Drop),
                    },

                    // Last frame with toggle=0 after previous one with toggle=1, ok
                    (Kind::EndT0, AssemblingT1) => match payload_kind {
                        PayloadKind::LessThanMTU | PayloadKind::ExactlyMTU => {
                            (Done, CheckCrcAndPush)
                        }
                        _ => (Failure, Drop),
                    },

                    // Frame with toggle=0 after previous one with toggle=0, reorder error
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT0) => (Failure, Drop),

                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT0 | Kind::EndT0, _) => (Failure, Drop),

                    // Frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::MiddleT1, AssemblingT0) => match payload_kind {
                        PayloadKind::ExactlyMTU => (AssemblingT1, Push),
                        _ => (Failure, Drop),
                    },

                    // Last frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::EndT1, AssemblingT0) => match payload_kind {
                        PayloadKind::LessThanMTU | PayloadKind::ExactlyMTU => {
                            (Done, CheckCrcAndPush)
                        }
                        _ => (Failure, Drop),
                    },

                    // Frame with toggle=1 after previous one with toggle=1, reorder error
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT1) => (Failure, Drop),

                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT1 | Kind::EndT1, _) => (Failure, Drop),

                    // UAVCAN Version 0 tail byte, error
                    (Kind::SingleFrameV0 | Kind::MultiFrameV0, _) => (Failure, Ignore),
                }
            }
        };
        self.state = next_state;
        output
    }

    fn fail(&mut self) {
        self.state = State::Failure;
    }
    //
    // // Fail this transfer, remove all pieces from storage and return a number of pieces removed
    // fn fail_and_clean_storage(
    //     &mut self,
    //     storage: &mut PiecesStorage<{ MTU - 1 }, MAX_PIECES>,
    // ) -> PieceIdx {
    //     self.state = State::Failure;
    //     self.first_piece_idx
    //         .map(|idx| storage.remove_all(idx))
    //         .unwrap_or(0)
    // }
}



#[derive(Copy, Clone)]
struct Transfer<const MTU: usize> {
    transfer_machine: TransferMachine<MTU>,
    first_piece_idx: Option<PieceIdx>,
    last_piece_idx: Option<PieceIdx>,
    last_piece_len: PieceByteIdx,
    priority: Priority,
    sequence_number: TransferSeq,
    last_changed_timestamp: u32,
}
impl<const MTU: usize> Transfer<MTU>
{
    fn new(priority: Priority, sequence_number: TransferSeq, time_now: u32) -> Self {
        Transfer {
            transfer_machine: TransferMachine::reset(),
            first_piece_idx: None,
            last_piece_idx: None,
            last_piece_len: 0,
            priority,
            sequence_number,
            last_changed_timestamp: time_now,
        }
    }
}
impl<const MTU: usize> core::fmt::Display for Transfer<MTU> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} Seq:{} t:{} {} {:?}..={:?}/{}",
               self.priority,
               self.sequence_number,
               self.last_changed_timestamp,
               self.transfer_machine,
               self.first_piece_idx,
               self.last_piece_idx,
               self.last_piece_len,
        )
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
            source: can_id.source_node_id,
        }
    }
}

pub struct Assembler<
    const MTU: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
> where
    [u8; MTU - 1]: Sized,
{
    transfers: FnvIndexMap<TransfersMapKey, Transfer<MTU>, MAX_TRANSFERS>,
    storage: PiecesStorage<{ MTU - 1 }, MAX_PIECES>,
    latest_sequence_number: TransferSeq,
}

impl<
    const MTU: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME>
    where
        [u8; MTU - 1]: Sized,
{
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
            _ => unreachable!(),
        };

        match Self::drive_state_machine(&mut self.storage, transfer, payload) {
            Ok(_) => {}
            Err(_) => {}
        }
    }

    fn drive_state_machine(storage: &mut PiecesStorage<{MTU - 1}, MAX_PIECES>, transfer: &mut Transfer<MTU>, payload: &[u8]) -> Result<(), ()> {
        let mut payload_owned = [0u8; MTU - 1];
        let (payload_kind, tail_byte) = if payload.len() >= 1 && payload.len() <= MTU {
            let tail_byte = Some(TailByte::from(*payload.last().unwrap()));
            if payload.len() == 1 {
                (PayloadKind::Empty, tail_byte)
            } else if payload.len() < MTU {
                transfer.last_piece_len = payload.len() as PieceByteIdx;
                (PayloadKind::LessThanMTU, tail_byte)
            } else {
                transfer.last_piece_len = payload.len() as PieceByteIdx;
                (PayloadKind::ExactlyMTU, tail_byte)
            }
        } else {
            (PayloadKind::Invalid, None)
        };

        let output = transfer.transfer_machine.advance(payload_kind, tail_byte);
        // If more space is needed, lower priority transfer storage will be wiped and exactly one
        // freed up slot will be used for this one.
        // first_piece index of that l.p. transfer is returned in such case, it is then looked up
        // and marked as Failure
        use TransferMachineOutput::*;
        match output {
            Ignore => {}
            Push | CheckCrcAndPush => {
                if output == CheckCrcAndPush {
                    for chunk in storage.traverse(transfer.first_piece_idx.unwrap()) {
                        println!("traverse: {:?}", chunk)
                    }
                }

                match transfer.last_piece_idx {
                    Some(idx) => {
                        transfer.last_piece_idx =
                            storage.push_after(payload_owned, idx)
                                .map(|idx| Some(idx)).map_err(|_| ())?;
                    }
                    None => match storage.push(payload_owned) {
                        Ok(idx) => {
                            transfer.first_piece_idx = Some(idx);
                            transfer.last_piece_idx = Some(idx);
                        }
                        _ => {
                            return Err(());
                        }
                    },
                }
            }
            Drop => {
                transfer.first_piece_idx.map(|idx| storage.remove_all(idx));
                // self.first_piece_idx = None;
                // self.last_piece_idx = None;
            }
        }
        Ok(())
    }

    // fn fail_lower_priority_transfer(&mut self, lower_than: Priority, first_piece_idx: PieceIdx) {
    //     for (_, transfer) in &mut self.transfers {
    //         match transfer.transfer_machine.first_piece_idx {
    //             Some(idx) => {
    //                 if transfer.priority < lower_than && idx == first_piece_idx {
    //                     transfer.transfer_machine.fail();
    //                 }
    //             }
    //             None => {}
    //         }
    //     }
    // }

    fn remove_outdated_transfers(&mut self, time_now: u32) {
        // for (_, transfer) in &mut self.transfers {
        //     if time_now - transfer.last_changed_timestamp > TRANSFER_LIFETIME {
        //         let slots_freed_up = transfer
        //             .transfer_machine
        //             .fail();
        //         if slots_freed_up >= 1 {
        //             // Destroy only this transfer, since at least one slot is now free for new data
        //             // and allow user to read out old transfers if it is slow
        //             break;
        //         }
        //     }
        // }
    }
}

impl<
    const MTU: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
> core::fmt::Display for Assembler<MTU, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME>
    where
        [u8; MTU - 1]: Sized,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Transfers:").ok();
        let mut i = 1;
        for (key, transfer) in &self.transfers {
            writeln!(
                f,
                "{}/{}: {} {} {}",
                i,
                self.transfers.len(),
                key.source,
                key.kind,
                transfer
            )
                .ok();
            i += 1;
        }
        writeln!(f, "Storage:").ok();
        writeln!(f, "{}", self.storage).ok();
        write!(f, "")
    }
}
