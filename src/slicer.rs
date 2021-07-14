#[deny(warnings)]

pub fn slice_to_closure<'a, F: FnMut(&'a [u8], u8) -> ()>(payload: &'a [u8], mtu: usize, mut f: F) {
    for chunk in payload.chunks(mtu - 1) {
        f(chunk, 0);
    }
}