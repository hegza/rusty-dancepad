use corncobs::{max_encoded_len, CobsError};
use thiserror::Error;

use crate::codec::Codec;

#[derive(Debug, Error)]
pub enum SerializeError {
    #[error("failed to serialize: {0}")]
    PostcardError(#[from] postcard::Error),
}

#[derive(Debug, Error)]
pub enum DeserializeError {
    #[error("invalid framing: {0}")]
    InvalidFraming(CobsError),
    #[error("failed to deserialize: {0}")]
    PostcardError(#[from] postcard::Error),
}

// This manual conversion is required when thiserror is compiled without std
impl From<CobsError> for DeserializeError {
    fn from(value: CobsError) -> Self {
        DeserializeError::InvalidFraming(value)
    }
}

// Blanket implementation of Codec for all serializable/deserializable types
impl<P> Codec<P> for P
where
    // `P` must be both serializable and deserializable to form a valid payload
    P: for<'de> serde::Deserialize<'de>
        + serde::Serialize
        + postcard::experimental::max_size::MaxSize,
{
    type DeserializeError = DeserializeError;
    type SerializeError = SerializeError;

    /// Maximum possible length of the serialized packet. Required by the deserializer to determine
    /// how much memory needs to be allocated for the packet.
    const MAX_SERIALIZED_LEN: usize = max_encoded_len(P::POSTCARD_MAX_SIZE + size_of::<u32>());

    /// Serialize an instance of type `P` into a COBS packet with a CRC check for redundancy. Returns
    /// the sub-slice of `out_buf` that was allocated.
    ///
    /// # Arguments
    ///
    /// * `value` - the value to serialize
    ///
    /// # Type arguments
    ///
    /// * `'a` - lifetime of `out_buf` which bounds also the lifetime of return value which is a view to `out_buf`
    /// * `N` - buffer size for the output packet, i.e., maximum size after serialization and anything
    ///   else you might want to include in the packet. This should generally match `MAX_SERIALIZED_LEN`.
    fn serialize<'a, const N: usize>(
        &self,
        out_buf: &'a mut [u8; N],
    ) -> Result<&'a mut [u8], Self::SerializeError> {
        // memcpy
        let mut buf_copy = *out_buf;
        // Serialize the value
        let serialized = postcard::to_slice(self, &mut buf_copy)?;
        // Encode the whole message into a COBS packet
        let n = corncobs::encode_buf(&serialized, out_buf);
        Ok(&mut out_buf[0..n])
    }

    /// Deserialize an instance of type `T` from a COBS packet with a CRC check for redundancy
    ///
    /// # Arguments
    ///
    /// * `in_buf` - the bytes of a COBS packet
    ///
    /// # Errors
    ///
    /// * `CobsDecodeFailed` - when the COBS packet could not be decoded (i.e., there was a
    ///   mispositioned zero in the input buffer)
    /// * `MarshalError`
    /// * `CrcMismatch` - when the checksum computed from the data does not match the checksum
    fn deserialize_in_place(in_buf: &mut [u8]) -> Result<Self, Self::DeserializeError> {
        let t = postcard::from_bytes_cobs(in_buf)?;
        Ok(t)
    }
}
