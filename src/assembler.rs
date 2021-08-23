struct TransferMachine<S> {
    state: S
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
    fn from(_: TransferMachine<Empty>) -> TransferMachine<Assembling> {
        todo!()
    }
}
impl From<TransferMachine<Empty>> for TransferMachine<Done> {
    fn from(_: TransferMachine<Empty>) -> TransferMachine<Done> {
        todo!()
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
    fn from(_: TransferMachine<AssemblingT1>) -> TransferMachine<Done> {
        todo!()
    }
}
impl From<TransferMachine<AssemblingT0>> for TransferMachine<Failure> {
    fn from(_: TransferMachine<AssemblingT0>) -> TransferMachine<Done> {
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