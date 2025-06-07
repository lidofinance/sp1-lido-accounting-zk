use std::num::TryFromIntError;

#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error("Failed to convert u64 to usize")]
    U64ToUsizeConversionError(TryFromIntError),

    #[error("Failed to convert u64 to usize")]
    UsizeToU64ConversionError(TryFromIntError),
}

pub fn usize_to_u64(val: usize) -> Result<u64, ConversionError> {
    val.try_into().map_err(ConversionError::UsizeToU64ConversionError)
}

pub fn u64_to_usize(val: u64) -> Result<usize, ConversionError> {
    val.try_into().map_err(ConversionError::U64ToUsizeConversionError)
}
