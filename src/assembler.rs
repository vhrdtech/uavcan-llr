use crate::types::TransferId;
use crate::tailbyte::{TailByte, Kind};

/// Used to index array of transfer pieces (incoming frames + index of the next piece)
type PieceIdx = u16;
/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;
/// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
/// Must be able to hold 2 * MAX_TRANSFERS
type TransferSeq = i16;

struct TransferMachine<const MTU: usize> {
    state: State,
    transfer_id: TransferId,
    first_piece_idx: Option<PieceIdx>,
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

enum TransferMachineInput {
    EmptyFrame,

}

impl<const MTU: usize> TransferMachine<MTU> {
    pub fn reset() -> Self {
        TransferMachine {
            state: State::Empty,
            transfer_id: Default::default(),
            first_piece_idx: None,
            last_piece_len: 0
        }
    }

    pub fn advance(&mut self, frame_payload: &[u8], storage: &mut PiecesStorage) {
        if frame_payload.len() >= 1 && frame_payload.len() <= MTU {
            let tail_byte = TailByte::from(*frame_payload.last().unwrap());
            self.advance_internal(Some(tail_byte), storage);
        } else {
            self.advance_internal(None, storage);
        }
    }

    fn advance_internal(&mut self, tail_byte: Option<TailByte>, storage: &mut PiecesStorage) {
        use State::*;
        self.state = match (tail_byte, self.state) {
            // Got a frame without tail byte
            // No need to remove pieces from storage
            (None, Empty | Failure) => {
                Failure
            }
            // Need to remove pieces from storage
            (None, AssemblingT1 | AssemblingT0) => {
                self.first_piece_idx.map(|idx| storage.remove_all(idx));
                Failure
            }
            // Ignore and do not destroy valid transfer
            (None, Done) => {
                Done
            }
            (Some(tail_byte), state) => {
                match (tail_byte.kind(), state) {
                    // Single frame transfer from "idle" states, ok
                    (Kind::SingleFrame, Empty | Done | Failure) => {

                        Done
                    }
                    // Single frame transfer in the middle of a multi-frame one, error
                    (Kind::SingleFrame, AssemblingT1 | AssemblingT0) => {
                        self.first_piece_idx.map(|idx| storage.remove_all(idx));
                        Failure
                    }

                    // Start of a multi-frame transfer from "idle" states, ok
                    (Kind::Start, Empty | Done | Failure) => {

                        AssemblingT1
                    }
                    // Repeated start in the middle of a multi-frame transfer, error
                    (Kind::Start, AssemblingT1 | AssemblingT0) => {

                        Failure
                    }

                    // Frame with toggle=0 after previous one with toggle=1, ok
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT1) => {

                        AssemblingT0
                    }
                    // Frame with toggle=0 after previous one with toggle=0, reorder error
                    (Kind::MiddleT0 | Kind::EndT0, AssemblingT0) => {

                        Failure
                    }
                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT0 | Kind::EndT0, _) => {

                        Failure
                    }

                    // Frame with toggle=1 after previous one with toggle=0, ok
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT0) => {

                        AssemblingT1
                    }
                    // Frame with toggle=1 after previous one with toggle=1, reorder error
                    (Kind::MiddleT1 | Kind::EndT1, AssemblingT1) => {

                        Failure
                    }
                    // Frame that doesn't belong to an ongoing multi-frame transfer, error
                    (Kind::MiddleT1 | Kind::EndT1, _) => {

                        Failure
                    }

                    // UAVCAN Version 0 tail byte, error
                    (Kind::SingleFrameV0 | Kind::StartV0, _) => {

                        Failure
                    }
                }
            }
        };
    }
}

struct PiecesStorage {

}

impl PiecesStorage {
    fn remove_all(&mut self, first_piece_idx: PieceIdx) {

    }
}

// pub struct Transfer {
//     transfer_machine: TransferMachine,
//
// }
//
// struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> where [(); MTU - 1]: Sized {
//     transfers: [TransferMachine; MAX_TRANSFERS],
// }