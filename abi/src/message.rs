use crate::types::AdcValues;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[repr(C)]
pub enum Command {
    /// Get ADC values in millivolts
    GetValues,
    /// Get button press thresholds in terms of raw ADC value
    GetThresh,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[repr(C)]
pub enum Response<const N: usize> {
    Values(AdcValues<N>),
    Thresh(AdcValues<N>),
}
