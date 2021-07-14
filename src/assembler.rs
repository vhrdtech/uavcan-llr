use crate::types::CanId;
use core::convert::TryFrom;

type PieceIdx = u16;

pub struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> where [(); {MTU - 1}]: Sized {
    pieces: [Piece<{MTU - 1}>; MAX_PIECES],
    transfers: [Transfer; MAX_TRANSFERS],

}
impl<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> Assembler<MTU, MAX_PIECES, MAX_TRANSFERS> where [(); {MTU - 1}]: Sized {
    pub fn new() -> Self {
        Assembler {
            pieces: [Piece::new_empty(); MAX_PIECES],
            transfers: [Transfer::new_empty(); MAX_TRANSFERS],
        }
    }
}

#[derive(Copy, Clone,)]
pub struct Piece<const N: usize> {
    is_used: bool,
    next: PieceIdx,
    bytes: [u8; N]
}
impl<const N: usize> Piece<N> {
    pub fn new_empty() -> Self {
        Piece {
            is_used: false,
            next: 0,
            bytes: [0; N]
        }
    }
}

#[derive(Copy, Clone,)]
pub struct Transfer {
    is_used: bool,
    id: CanId,
    first_piece: PieceIdx,
    last_piece_bytes_used: u8,

}
impl Transfer {
    pub fn new_empty() -> Self {
        Transfer {
            is_used: false,
            id: CanId::try_from(0).unwrap(),
            first_piece: 0,
            last_piece_bytes_used: 0
        }
    }
}