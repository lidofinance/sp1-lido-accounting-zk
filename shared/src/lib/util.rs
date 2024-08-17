pub fn usize_to_u64(val: usize) -> u64 {
    val.try_into()
        .expect("Couldn't convert usize to u64 - are you on 128-bit address space architecture?")
}
