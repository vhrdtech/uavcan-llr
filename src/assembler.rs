/// Used to index array of transfer pieces (incoming frames + index of the next piece)
type PieceIdx = u16;
/// Used to index into frame data
type PieceByteIdx = u8;
/// Used to index into transfer list
type TransferIdx = u8;
/// Used to sort incoming transfer by time of arrival, so that equal priority transfer are in fifo order
/// Must be able to hold 2 * MAX_TRANSFERS
type TransferSeq = i16;

struct TransferMachine<S> {
    state: S,
    first_piece: PieceIdx,
    last_piece_bytes_used: PieceByteIdx,
}
impl<S> TransferMachine<S> {
    pub fn into_state<NS>(self, new_state: NS) -> TransferMachine<NS> {
        TransferMachine {
            state: new_state,
            first_piece: self.first_piece,
            last_piece_bytes_used: self.last_piece_bytes_used
        }
    }
}

struct Empty {

}

struct AssemblingT1 {

}

struct AssemblingT0 {

}

struct Done {

}

struct Failure {

}

impl From<TransferMachine<Empty>> for TransferMachine<AssemblingT1> {
    fn from(_: TransferMachine<Empty>) -> TransferMachine<AssemblingT1> {
        todo!()
    }
}
impl From<TransferMachine<Empty>> for TransferMachine<Done> {
    fn from(v: TransferMachine<Empty>) -> TransferMachine<Done> {
        v.into_state(Done {})
    }
}
impl From<TransferMachine<Empty>> for TransferMachine<Failure> {
    fn from(_: TransferMachine<Empty>) -> TransferMachine<Failure> {
        todo!()
    }
}

impl From<TransferMachine<AssemblingT1>> for TransferMachine<AssemblingT0> {
    fn from(_: TransferMachine<AssemblingT1>) -> TransferMachine<AssemblingT0> {
        todo!()
    }
}
impl From<TransferMachine<AssemblingT0>> for TransferMachine<AssemblingT1> {
    fn from(_: TransferMachine<AssemblingT0>) -> TransferMachine<AssemblingT1> {
        todo!()
    }
}

impl From<TransferMachine<AssemblingT1>> for TransferMachine<Done> {
    fn from(_: TransferMachine<AssemblingT1>) -> TransferMachine<Done> {
        todo!()
    }
}
impl From<TransferMachine<AssemblingT0>> for TransferMachine<Done> {
    fn from(_: TransferMachine<AssemblingT0>) -> TransferMachine<Done> {
        todo!()
    }
}

impl From<TransferMachine<AssemblingT1>> for TransferMachine<Failure> {
    fn from(_: TransferMachine<AssemblingT1>) -> TransferMachine<Failure> {
        todo!()
    }
}
impl From<TransferMachine<AssemblingT0>> for TransferMachine<Failure> {
    fn from(_: TransferMachine<AssemblingT0>) -> TransferMachine<Failure> {
        todo!()
    }
}

enum TransferMachineWrapper {
    Empty(TransferMachine<Empty>),
    AssemblingT1(TransferMachine<AssemblingT1>),
    AssemblingT0(TransferMachine<AssemblingT0>),
    Done(TransferMachine<Done>),
    Failure(TransferMachine<Failure>)
}

struct Assembler<const MTU: usize, const MAX_PIECES: usize, const MAX_TRANSFERS: usize> where [(); MTU - 1]: Sized {
    transfers: [TransferMachineWrapper; MAX_TRANSFERS],
}