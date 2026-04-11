use std::sync::atomic::{AtomicU8, Ordering};

use entangle_lockfree::SpscQueue;

use crate::contracts::{ChannelState, PointerOffset, ReceiveError, SendError};

/// Zero-copy communication channel between a sender and receiver.
///
/// Consists of two SPSC queues:
/// - `send_queue`: sender pushes pointer offsets for the receiver
/// - `return_queue`: receiver returns consumed offsets for the sender to reclaim
///
/// No data is copied — only 64-bit pointer offsets are exchanged.
pub struct ZeroCopyChannel {
    send_queue: SpscQueue,
    return_queue: SpscQueue,
    state: AtomicU8,
    sender_id: u128,
    receiver_id: u128,
}

impl ZeroCopyChannel {
    /// Create a new channel with the given buffer capacity.
    pub fn new(capacity: usize, sender_id: u128, receiver_id: u128) -> Self {
        Self {
            send_queue: SpscQueue::new(capacity),
            return_queue: SpscQueue::new(capacity),
            state: AtomicU8::new(ChannelState::Creating as u8),
            sender_id,
            receiver_id,
        }
    }

    /// Transition to Connected state.
    pub fn connect(&self) {
        self.state
            .store(ChannelState::Connected as u8, Ordering::Release);
    }

    /// Transition to Disconnected state.
    pub fn disconnect(&self) {
        self.state
            .store(ChannelState::Disconnected as u8, Ordering::Release);
    }

    /// Current channel state.
    pub fn state(&self) -> ChannelState {
        match self.state.load(Ordering::Acquire) {
            0 => ChannelState::Creating,
            1 => ChannelState::Connected,
            _ => ChannelState::Disconnected,
        }
    }

    /// Send a pointer offset to the receiver.
    /// Only called by the sender (publisher).
    pub fn send(&self, offset: PointerOffset) -> Result<(), SendError> {
        if self.state() == ChannelState::Disconnected {
            return Err(SendError::ConnectionBroken);
        }
        if !self.send_queue.push(offset.raw()) {
            return Err(SendError::QueueFull);
        }
        Ok(())
    }

    /// Receive a pointer offset from the sender.
    /// Only called by the receiver (subscriber).
    pub fn receive(&self) -> Result<PointerOffset, ReceiveError> {
        if self.state() == ChannelState::Disconnected && self.send_queue.is_empty() {
            return Err(ReceiveError::ConnectionBroken);
        }
        self.send_queue
            .pop()
            .map(PointerOffset::from_raw)
            .ok_or(ReceiveError::Empty)
    }

    /// Return a consumed offset to the sender for reclamation.
    /// Only called by the receiver.
    pub fn return_offset(&self, offset: PointerOffset) {
        // Return queue should always have space (bounded by loaned samples).
        // If it's full, we spin — this is a correctness issue, not normal flow.
        while !self.return_queue.push(offset.raw()) {
            std::hint::spin_loop();
        }
    }

    /// Reclaim a returned offset. Called by the sender to reuse slots.
    pub fn reclaim(&self) -> Option<PointerOffset> {
        self.return_queue.pop().map(PointerOffset::from_raw)
    }

    /// Number of pending (unread) items in the send queue.
    pub fn pending_count(&self) -> usize {
        self.send_queue.len()
    }

    /// Number of items waiting to be reclaimed.
    pub fn return_count(&self) -> usize {
        self.return_queue.len()
    }

    /// Sender ID.
    pub fn sender_id(&self) -> u128 {
        self.sender_id
    }

    /// Receiver ID.
    pub fn receiver_id(&self) -> u128 {
        self.receiver_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_receive_cycle() {
        let ch = ZeroCopyChannel::new(4, 1, 2);
        ch.connect();

        let offset = PointerOffset::new(0, 1024);
        ch.send(offset).unwrap();

        let received = ch.receive().unwrap();
        assert_eq!(received, offset);

        ch.return_offset(received);
        let reclaimed = ch.reclaim().unwrap();
        assert_eq!(reclaimed, offset);
    }

    #[test]
    fn disconnected_send_fails() {
        let ch = ZeroCopyChannel::new(4, 1, 2);
        ch.connect();
        ch.disconnect();

        let offset = PointerOffset::new(0, 0);
        assert!(matches!(ch.send(offset), Err(SendError::ConnectionBroken)));
    }

    #[test]
    fn empty_receive() {
        let ch = ZeroCopyChannel::new(4, 1, 2);
        ch.connect();
        assert!(matches!(ch.receive(), Err(ReceiveError::Empty)));
    }

    #[test]
    fn queue_full() {
        let ch = ZeroCopyChannel::new(2, 1, 2);
        ch.connect();
        ch.send(PointerOffset::new(0, 0)).unwrap();
        ch.send(PointerOffset::new(0, 1)).unwrap();
        assert!(matches!(
            ch.send(PointerOffset::new(0, 2)),
            Err(SendError::QueueFull)
        ));
    }
}
