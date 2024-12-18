//! [PushBuffer] is one of the suggested data-structures for handling allocation of incoming
//! characters over serial.

#[derive(Debug)]
pub(crate) struct BufferOverflow;

/// Buffer for constructing longer messages from bytes
///
/// Supports two operations:
///
/// - `push`    - Push a byte at the end of the buffer
/// - `finish`  - Return the completed message, once only
///
/// # Type arguments
///
/// * `LEN` - number of bytes for the buffer allocation. Should be able to fit any possible input.
#[derive(Debug)]
pub(crate) struct PushBuffer<const LEN: usize> {
    /// Allocation for the incoming bytes
    buf: [u8; LEN],
    /// Current length of the buffer
    len: usize,
}

impl<const LEN: usize> PushBuffer<LEN> {
    /// Push a byte at the end of the buffer
    ///
    /// Returns a [BufferOverflow] if the buffer already contains [LEN] bytes.
    pub fn push(&mut self, byte: u8) -> Result<(), BufferOverflow> {
        if self.is_full() {
            return Err(BufferOverflow);
        }
        // SAFETY: we have checked above that there is still room in the buffer
        *unsafe { self.buf.get_unchecked_mut(self.len) } = byte;
        self.len += 1;
        Ok(())
    }

    pub fn is_full(&self) -> bool {
        self.len == LEN
    }

    /// Return the finished buffer
    ///
    /// `self` is moved to make sure the buffer is consumed only once, when it's completed.
    pub fn finish(self) -> ([u8; LEN], usize) {
        (self.buf, self.len)
    }
}

impl<const LEN: usize> Default for PushBuffer<LEN> {
    fn default() -> Self {
        Self {
            buf: [0; LEN],
            len: 0,
        }
    }
}
