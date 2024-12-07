#![cfg_attr(feature = "device", no_std)]

/// Raw 16-bit ADC values
///
/// # Type arguments
///
/// * `N` - number of supported ADC channels and values.
#[derive(Clone)]
pub struct AdcValues<const N: usize>([u16; N]);

pub enum Command {}

pub enum Response {}
