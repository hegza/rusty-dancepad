#![cfg_attr(feature = "device", no_std)]

/// ADC values in millivolts (16-bit)
///
/// # Type arguments
///
/// * `N` - number of supported ADC channels and values.
pub type AdcValues<const N: usize> = [u16; N];

pub enum Command {}

pub enum Response {}
