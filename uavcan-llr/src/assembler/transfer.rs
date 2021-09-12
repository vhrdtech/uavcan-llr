use crate::types::{TransferKind, NodeId, CanId, TransferId, Priority};
use core::fmt::{Formatter, Display, Result as FmtResult};
use crate::tailbyte::{TailByte, Kind};
use hash32_derive::Hash32;
use super::types::*;


#[derive(Copy, Clone)]
pub(crate) struct TransferMachine<const MTU: usize> {
    state: State,
    transfer_id: Option<TransferId>,
}
impl<const MTU: usize> Display for TransferMachine<MTU> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "TM({:?}, Tid:{:?}",
               self.state,
               self.transfer_id,
        )
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum State {
    Empty,
    AssemblingT1,
    AssemblingT0,
    Done,
    Failure,
}

pub(crate) enum PayloadKind {
    Empty,
    LessThanMTU,
    ExactlyMTU,
    Invalid,
}

#[derive(Eq, PartialEq, Debug)]
pub(crate) enum TransferMachineOutput {
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

    pub(crate) fn advance(
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

    pub(crate) fn fail(&mut self) {
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
pub(crate) struct Transfer<const MTU: usize> {
    pub(crate) transfer_machine: TransferMachine<MTU>,
    pub(crate) first_piece_idx: Option<PieceIdx>,
    pub(crate) last_piece_idx: Option<PieceIdx>,
    pub(crate) last_piece_len: PieceByteIdx,
    pub(crate) priority: Priority,
    pub(crate) sequence_number: TransferSeq,
    pub(crate) last_changed_timestamp: u32,
}
impl<const MTU: usize> Transfer<MTU>
{
    pub(crate) fn new(priority: Priority, sequence_number: TransferSeq, time_now: u32) -> Self {
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
pub(crate) struct TransfersMapKey {
    pub(crate) kind: TransferKind,
    pub(crate) source: NodeId,
}
impl From<CanId> for TransfersMapKey {
    fn from(can_id: CanId) -> Self {
        TransfersMapKey {
            kind: can_id.transfer_kind,
            source: can_id.source_node_id,
        }
    }
}