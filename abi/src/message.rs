use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, MaxSize)]
#[repr(C)]
pub enum Command {
    /// Get ADC values in millivolts
    GetValues,
    /// Get button press thresholds in terms of raw ADC value
    GetThresh,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, MaxSize)]
#[repr(C)]
pub enum Response {
    Values4([u16; 4]),
    Thresh4([u16; 4]),
}
