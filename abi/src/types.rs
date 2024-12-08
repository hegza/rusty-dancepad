use core::{fmt, ops::Deref};

use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize,
};

/// ADC values in millivolts (16-bit)
///
/// # Type arguments
///
/// * `N` - number of supported ADC channels and values.
#[derive(Clone, Debug, PartialEq)]
pub struct AdcValues<const N: usize>([u16; N]);

impl<const N: usize> Serialize for AdcValues<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for e in self.0.iter() {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}

impl<'de, const N: usize> Deserialize<'de> for AdcValues<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AdcValuesVisitor<const N: usize>;

        impl<'de, const N: usize> Visitor<'de> for AdcValuesVisitor<N> {
            type Value = AdcValues<N>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an array of ")?;
                let mut buffer = itoa::Buffer::new();
                let printed = buffer.format(N);
                formatter.write_str(printed)?;
                formatter.write_str(" u16 values")?;
                Ok(())
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = [0u16; N];
                for i in 0..N {
                    values[i] = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(AdcValues(values))
            }
        }

        deserializer.deserialize_tuple(N, AdcValuesVisitor::<N>)
    }
}

impl<const N: usize> Deref for AdcValues<N> {
    type Target = [u16; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
