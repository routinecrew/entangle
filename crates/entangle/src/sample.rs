use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use entangle_transport::PointerOffset;

use crate::contracts::ZeroCopySafe;

type SendFn = Box<dyn FnOnce(PointerOffset) -> Result<(), crate::error::PortError> + Send>;
type DropFn = Box<dyn FnOnce(PointerOffset) + Send>;

/// Immutable sample received from shared memory.
///
/// RAII wrapper: when dropped, the underlying slot offset is returned
/// to the sender for reclamation via the `drop_fn`.
pub struct Sample<T: ZeroCopySafe> {
    ptr: *const T,
    offset: PointerOffset,
    drop_fn: Option<Box<dyn FnOnce(PointerOffset) + Send>>,
    _marker: PhantomData<T>,
}

// Safety: Sample provides read-only access to shared memory.
// The underlying memory is synchronized by the transport layer.
unsafe impl<T: ZeroCopySafe> Send for Sample<T> {}
unsafe impl<T: ZeroCopySafe> Sync for Sample<T> {}

impl<T: ZeroCopySafe> Sample<T> {
    /// Create a new sample from a raw pointer and offset.
    ///
    /// # Safety
    /// The caller must ensure `ptr` is valid for reads of `T` for the
    /// lifetime of this Sample.
    pub(crate) unsafe fn new(
        ptr: *const T,
        offset: PointerOffset,
        drop_fn: impl FnOnce(PointerOffset) + Send + 'static,
    ) -> Self {
        Self {
            ptr,
            offset,
            drop_fn: Some(Box::new(drop_fn)),
            _marker: PhantomData,
        }
    }

    /// The pointer offset identifying this sample in shared memory.
    pub fn offset(&self) -> PointerOffset {
        self.offset
    }
}

impl<T: ZeroCopySafe> Deref for Sample<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // Safety: ptr was validated at construction time
        unsafe { &*self.ptr }
    }
}

impl<T: ZeroCopySafe> Drop for Sample<T> {
    fn drop(&mut self) {
        if let Some(f) = self.drop_fn.take() {
            f(self.offset);
        }
    }
}

/// Mutable sample for writing into shared memory before sending.
///
/// Created by `Publisher::loan()`. Write data via `DerefMut`, then call
/// `send()` to publish. If dropped without sending, the slot is returned
/// to the pool.
pub struct SampleMut<T: ZeroCopySafe> {
    ptr: *mut T,
    offset: PointerOffset,
    send_fn: Option<SendFn>,
    drop_fn: Option<DropFn>,
    _marker: PhantomData<T>,
}

unsafe impl<T: ZeroCopySafe> Send for SampleMut<T> {}

impl<T: ZeroCopySafe> SampleMut<T> {
    /// Create a new mutable sample.
    ///
    /// # Safety
    /// The caller must ensure `ptr` is valid for reads and writes of `T`.
    pub(crate) unsafe fn new(
        ptr: *mut T,
        offset: PointerOffset,
        send_fn: impl FnOnce(PointerOffset) -> Result<(), crate::error::PortError> + Send + 'static,
        drop_fn: impl FnOnce(PointerOffset) + Send + 'static,
    ) -> Self {
        Self {
            ptr,
            offset,
            send_fn: Some(Box::new(send_fn)),
            drop_fn: Some(Box::new(drop_fn)),
            _marker: PhantomData,
        }
    }

    /// Send this sample to all connected subscribers.
    ///
    /// Consumes the sample. Only the pointer offset is transmitted —
    /// zero bytes are copied.
    pub fn send(mut self) -> Result<(), crate::error::PortError> {
        let send_fn = self.send_fn.take().unwrap();
        self.drop_fn.take(); // prevent drop from deallocating
        send_fn(self.offset)
    }

    /// The pointer offset identifying this sample.
    pub fn offset(&self) -> PointerOffset {
        self.offset
    }
}

impl<T: ZeroCopySafe> Deref for SampleMut<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T: ZeroCopySafe> DerefMut for SampleMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }
}

impl<T: ZeroCopySafe> Drop for SampleMut<T> {
    fn drop(&mut self) {
        // If send() was not called, return the slot to the pool
        if let Some(f) = self.drop_fn.take() {
            f(self.offset);
        }
    }
}
