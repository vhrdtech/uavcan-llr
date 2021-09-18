use crate::tailbyte::{TailByte};
use crate::types::{CanId};
use core::fmt::{Formatter, Display, Result as FmtResult};
use heapless::FnvIndexMap;
use super::storage::{PiecesStorage};
use super::transfer::{PayloadKind, Transfer, TransfersMapKey, TransferMachineOutput};
use super::types::*;

pub struct Assembler<
    const MTU: usize,
    const MTU_M1: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
>
{
    transfers: FnvIndexMap<TransfersMapKey, Transfer<MTU>, MAX_TRANSFERS>,
    storage: PiecesStorage<MTU_M1, MAX_PIECES>,
    latest_sequence_number: TransferSeq,
    counters: Counters,
}

impl<
    const MTU: usize,
    const MTU_M1: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
> Assembler<MTU, MTU_M1, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME>
{
    pub fn new() -> Self {
        Assembler {
            transfers: FnvIndexMap::new(),
            storage: PiecesStorage::new(),
            latest_sequence_number: 0,
            counters: Counters::default(),
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
            let transfer = Transfer::new(id.priority, self.latest_sequence_number, time_now);
            self.latest_sequence_number = self.latest_sequence_number.wrapping_add(1);
            // Will not fail because of the check above
            let _ = self.transfers.insert(key, transfer);
        }
        let transfer = match self.transfers.get_mut(&key) {
            Some(t) => t,
            _ => unreachable!(),
        };

        match Self::drive_state_machine(&mut self.storage, transfer, payload, &mut self.counters) {
            Ok(_) => {

            },
            Err(_) => {

            }
        }
    }

    fn drive_state_machine(storage: &mut PiecesStorage<MTU_M1, MAX_PIECES>, transfer: &mut Transfer<MTU>, payload: &[u8], counters: &mut Counters) -> Result<(), ()> {
        let mut payload_owned = [0u8; MTU_M1];
        let (payload_kind, tail_byte) = if payload.len() >= 1 && payload.len() <= MTU {
            let tail_byte = Some(TailByte::from(*payload.last().unwrap()));
            if payload.len() == 1 {
                (PayloadKind::Empty, tail_byte)
            } else if payload.len() < MTU {
                transfer.last_piece_len = payload.len() as PieceByteIdx;
                payload_owned[0..payload.len() - 1].copy_from_slice(&payload[0..payload.len() - 1]);
                (PayloadKind::LessThanMTU, tail_byte)
            } else {
                transfer.last_piece_len = payload.len() as PieceByteIdx;
                payload_owned.copy_from_slice(&payload[0..payload.len() - 1]);
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
                    let mut crc16 = crc_any::CRCu16::crc16ccitt_false();
                    let mut received_crc16 = [0, 0];
                    for (chunk, is_last) in storage.traverse(transfer.first_piece_idx.unwrap()) {
                        if is_last {
                            match payload.len() {
                                0 => {
                                    unreachable!()
                                },
                                1 => {
                                    unreachable!()
                                },
                                2 => {
                                    crc16.digest(&chunk[..chunk.len() - 1]);
                                    crc16.digest(&[payload[0]]);
                                    received_crc16[0] = *chunk.last().unwrap();
                                    received_crc16[1] = payload[0];
                                },
                                3 => {
                                    crc16.digest(chunk);
                                    received_crc16[0] = payload[0];
                                    received_crc16[1] = payload[1];
                                },
                                _ => {
                                    crc16.digest(chunk);
                                    crc16.digest(&payload[0..payload.len() - 3]);
                                    received_crc16[0] = payload[payload.len() - 2];
                                    received_crc16[1] = payload[payload.len() - 1];
                                }
                            }
                        } else {
                            crc16.digest(chunk);
                        }
                    }
                    let received_crc16 = u16::from_be_bytes(received_crc16);
                    if crc16.get_crc() != received_crc16 {
                        transfer.first_piece_idx.map(|idx| storage.remove_all(idx));
                        counters.transfers_with_bad_crc += 1;
                        return Ok(())
                    } else {
                        counters.transfers_with_good_crc += 1;
                    }
                }

                match transfer.last_piece_idx {
                    Some(idx) => {
                        transfer.last_piece_idx = match storage.push_after(payload_owned, idx) {
                            Ok(idx) => Some(idx),
                            Err(_) => {
                                return Err(());
                            }
                        };
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
        for (_, transfer) in &mut self.transfers {
            if time_now - transfer.last_changed_timestamp > TRANSFER_LIFETIME {
                match transfer.first_piece_idx {
                    Some(idx) => {
                        if self.storage.remove_all(idx) >= 1 {
                            // Destroy only this transfer, since at least one slot is now free for new data
                            // and allow user to read out old transfers if it is slow
                            break;
                        }
                        transfer.transfer_machine.fail();
                    },
                    None => {}
                }
            }
        }
    }
}

impl<
    const MTU: usize,
    const MTU_M1: usize,
    const MAX_PIECES: usize,
    const MAX_TRANSFERS: usize,
    const TRANSFER_LIFETIME: u32,
> Display for Assembler<MTU, MTU_M1, MAX_PIECES, MAX_TRANSFERS, TRANSFER_LIFETIME>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
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
        write!(f, "{}", self.storage).ok();
        writeln!(f, "{:?}", self.counters)
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct Counters {
    pub transfers_with_good_crc: usize,
    pub transfers_with_bad_crc: usize,
}
