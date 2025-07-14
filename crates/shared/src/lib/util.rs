pub fn usize_to_u64(val: usize) -> u64 {
    let result = val.try_into();
    match result {
        Ok(v) => v,
        // Intentional panic - if we're getting here, something is wrong with either code or the
        // machine architecture - application won't be able to continue successfully
        Err(error) => panic!("Couldn't convert usize to u64: {error:?}"),
    }
}

pub fn u64_to_usize(val: u64) -> usize {
    let result = val.try_into();
    match result {
        Ok(v) => v,
        // Intentional panic - if we're getting here, something is wrong with either code or the
        // machine architecture - application won't be able to continue successfully
        Err(error) => panic!("Couldn't convert u64 to usize: {error:?}"),
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum IntegerError {
    #[error("Integer overflow happened")]
    Overflow,
}

pub fn erroring_add(lhs: u64, rhs: u64) -> Result<u64, IntegerError> {
    match lhs.checked_add(rhs) {
        Some(val) => Ok(val),
        None => Err(IntegerError::Overflow),
    }
}
