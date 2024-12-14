use corncobs::{max_encoded_len, CobsError};
use ssmarshal::Error as MarshalError;
use thiserror::Error;

use crate::codec::Codec;

#[cfg(feature = "crc")]
pub const CKSUM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

#[derive(Debug)]
pub enum SerializeError {}

#[derive(Debug, Error)]
pub enum DeserializeError {
    #[error("invalid framing: {0}")]
    InvalidFraming(CobsError),
    #[error("{0}")]
    MarshalError(MarshalError),
    #[cfg(feature = "crc")]
    /// The CRC computed from the data does not match the checksum
    #[error("CRC mismatch, value {0} != calculated {0}")]
    CrcMismatch { crc: u32, calculated: u32 },
}

// This manual conversion is required when thiserror is compiled without std
impl From<CobsError> for DeserializeError {
    fn from(value: CobsError) -> Self {
        DeserializeError::InvalidFraming(value)
    }
}

// This manual conversion is required when thiserror is compiled without std
impl From<MarshalError> for DeserializeError {
    fn from(value: MarshalError) -> Self {
        DeserializeError::MarshalError(value)
    }
}

// Blanket implementation of Codec for all serializable/deserializable types
impl<P> Codec<P> for P
where
    // `P` must be both serializable and deserializable to form a valid payload
    P: for<'de> serde::Deserialize<'de> + serde::Serialize,
{
    type DeserializeError = DeserializeError;
    type SerializeError = SerializeError;

    /// Maximum possible length of the serialized packet. Required by the deserializer to determine
    /// how much memory needs to be allocated for the packet.
    const MAX_SERIALIZED_LEN: usize = max_encoded_len(
        size_of::<P>()
            + size_of::<u32>()
            + if cfg!(feature = "crc") {
                size_of::<u32>()
            } else {
                0
            },
    );

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
        let mut n_ser = 0;
        // Serialize the value
        n_ser += ssmarshal::serialize(out_buf, self).unwrap();
        #[cfg(feature = "crc")]
        {
            // Calculate CRC for the serialized value
            let crc = CKSUM.checksum(&out_buf[0..n_ser]);
            // Serialize the CRC
            n_ser += ssmarshal::serialize(&mut out_buf[n_ser..], &crc).unwrap();
        }
        // memcpy
        let buf_copy = *out_buf;
        // Encode the whole message into a COBS packet
        let n = corncobs::encode_buf(&buf_copy[0..n_ser], out_buf);
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
        let n = corncobs::decode_in_place(in_buf)?;
        let (t, _resp_used) = ssmarshal::deserialize(&in_buf[0..n])?;
        #[cfg(feature = "crc")]
        {
            let crc_buf = &in_buf[_resp_used..];
            let (crc, _crc_used) = ssmarshal::deserialize::<u32>(crc_buf).unwrap();
            let pkg_crc = CKSUM.checksum(&in_buf[0.._resp_used]);
            if crc != pkg_crc {
                return Err(DeserializeError::CrcMismatch {
                    crc,
                    calculated: pkg_crc,
                });
            }
        }
        Ok(t)
    }
}
