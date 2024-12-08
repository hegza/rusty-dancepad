use core::fmt;

/// Trait for messages that can be passed to and from the device over a serial port
///
/// # Type arguments
///
/// * `P` - Message payload type. Must be serializable and deserializable for transfer by serial
///   wire.
pub trait Codec<P>
where
    Self: Sized,
    // `P` must be both serializable and deserializable to form a valid payload
    P: for<'de> serde::Deserialize<'de> + serde::Serialize,
{
    /// Error type representing what can go wrong during serialization
    type SerializeError: fmt::Debug;
    /// Error type representing what can go wrong during deserialization
    type DeserializeError: fmt::Debug;

    /// Maximum length of the encoded message in bytes. Required for determining required buffer
    /// size for ssmarshal.
    const MAX_SERIALIZED_LEN: usize;

    /// Serialize an instance of type `P` into bytes for transfer by serial. Returns the sub-slice of
    /// `out_buf` that was allocated for the encoded packet.
    ///
    /// # Arguments
    ///
    /// * `self` - The input value to serialize
    /// * `out_buf` - The buffer to use for the encoded value
    ///
    /// # Type arguments
    ///
    /// * `N` - Buffer size for the output packet, i.e., maximum size after serialization and anything
    ///   else one might want to include in the packet.
    fn serialize<'a, const N: usize>(
        &self,
        out_buf: &'a mut [u8; N],
    ) -> Result<&'a mut [u8], Self::SerializeError>;

    /// Deserialize an instance of type `P` from bytes
    ///
    /// # Arguments
    ///
    /// * `in_buf` - The bytes of a COBS packet. This buffer will be reused for the deserialized
    ///   packet.
    fn deserialize_in_place(in_buf: &mut [u8]) -> Result<Self, Self::DeserializeError>;
}
