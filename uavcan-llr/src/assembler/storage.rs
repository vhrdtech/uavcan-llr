use core::fmt::{Formatter, Display, Result as FmtResult};
use super::types::*;

#[derive(Copy, Clone)]
pub(crate)  enum Piece<const N: usize> {
    Empty,
    /// bytes, next
    Filled([u8; N], PieceIdx),
}

pub(crate)  struct PiecesStorage<const N: usize, const MAX_PIECES: usize> {
    items: [Piece<N>; MAX_PIECES],
    used: PieceIdx,
}
impl<const N: usize, const MAX_PIECES: usize> PiecesStorage<N, MAX_PIECES> {
    pub(crate) fn new() -> Self {
        PiecesStorage {
            items: [Piece::Empty; MAX_PIECES],
            used: 0,
        }
    }

    pub(crate) fn push(&mut self, data: [u8; N]) -> Result<PieceIdx, [u8; N]> {
        let empty_slot_idx = self.find_empty_slot().ok_or(data)?;
        self.items[empty_slot_idx as usize] = Piece::Filled(data, empty_slot_idx);
        self.used += 1;
        Ok(empty_slot_idx)
    }

    pub(crate) fn push_after(&mut self, data: [u8; N], after_piece_idx: PieceIdx) -> Result<PieceIdx, [u8; N]> {
        let empty_slot_idx = self.find_empty_slot().ok_or(data)?;
        self.items[after_piece_idx as usize] = match self.items[after_piece_idx as usize] {
            Piece::Empty => return Err(data),
            Piece::Filled(data, _) => Piece::Filled(data, empty_slot_idx),
        };
        self.items[empty_slot_idx as usize] = Piece::Filled(data, empty_slot_idx);
        self.used += 1;
        Ok(empty_slot_idx)
    }

    pub(crate) fn traverse(&self, first_piece_idx: PieceIdx) -> PiecesIter<N, MAX_PIECES> {
        PiecesIter {
            items: &self.items,
            idx: Some(first_piece_idx),
        }
    }

    pub(crate) fn find_empty_slot(&self) -> Option<PieceIdx> {
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
    pub(crate) fn remove_all(&mut self, first_piece_idx: PieceIdx) -> PieceIdx {
        let mut idx = first_piece_idx;
        let mut removed = 0;
        loop {
            self.items[idx as usize] = match &self.items[idx as usize] {
                Piece::Empty => {
                    break;
                }
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

    pub(crate) fn len(&self) -> usize {
        self.used as usize
    }
}
impl<const N: usize, const MAX_PIECES: usize> Display for PiecesStorage<N, MAX_PIECES> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        for (i, slot) in self.items.iter().enumerate() {
            match slot {
                Piece::Empty => {
                    write!(f, "{}: []\t", i).ok();
                }
                Piece::Filled(data, next) => {
                    write!(f, "{}: {:02x?}->({})\t", i, data, next).ok();
                }
            }
            if (i + 1) % 8 == 0 {
                writeln!(f, "").ok();
            }
        }
        write!(f, "")
    }
}

pub(crate) struct PiecesIter<'a, const N: usize, const MAX_PIECES: usize> {
    items: &'a [Piece<N>; MAX_PIECES],
    idx: Option<PieceIdx>,
}
impl<'a, const N: usize, const MAX_PIECES: usize> Iterator for PiecesIter<'a, N, MAX_PIECES> {
    type Item = (&'a [u8], bool);

    fn next(&mut self) -> Option<Self::Item> {
        let (out, next_idx) = match self.idx {
            Some(idx) => match &self.items[idx as usize] {
                Piece::Empty => (None, None),
                Piece::Filled(data, next_idx) => {
                    if idx == *next_idx {
                        (Some((&data[..], true)), None)
                    } else {
                        (Some((&data[..], false)), Some(*next_idx))
                    }
                }
            },
            None => (None, None),
        };
        self.idx = next_idx;
        out
    }
}