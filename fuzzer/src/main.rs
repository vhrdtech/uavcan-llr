use uavcan_llr::assembler::Assembler;
use uavcan_llr::slicer::Slicer;
use uavcan_llr::types::{TransferId, CanId, NodeId, SubjectId, Priority};

fn main() {
    let mut assembler = Assembler::<8, 32, 32, 10>::new();
    let payload = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    let slicer = Slicer::<8>::new(&payload, TransferId::new(31).unwrap()).frames_owned();
    let id0 = CanId::new_message_kind(
        NodeId::new(7).unwrap(),
        SubjectId::new(8).unwrap(),
        false,
        Priority::Nominal
    );
    for frame in slicer {
        println!("p: {}", frame.len());
        assembler.process_frame(id0, &frame, 0);
        println!("{}", assembler);
    }
}
