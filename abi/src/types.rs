use core::ops::Deref;

/// ADC values in millivolts (16-bit)
///
/// # Type arguments
///
/// * `N` - number of supported ADC channels and values.
#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
pub struct AdcValues<const N: usize>(pub [u16; N]);

impl<const N: usize> Default for AdcValues<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> From<AdcValues<N>> for [u16; N] {
    fn from(value: AdcValues<N>) -> Self {
        value.0
    }
}

impl<const N: usize> Deref for AdcValues<N> {
    type Target = [u16; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
