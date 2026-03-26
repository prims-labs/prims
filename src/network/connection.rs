use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionLimits {
    pub max_established_incoming: usize,
    pub max_established_outgoing: usize,
    pub reconnect_delay_secs: u64,
    pub temporary_ban_secs: u64,
    pub max_invalid_messages: u32,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_established_incoming: 32,
            max_established_outgoing: 32,
            reconnect_delay_secs: 5,
            temporary_ban_secs: 60,
            max_invalid_messages: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeerPenalty {
    invalid_messages: u32,
    banned_until_unix: Option<u64>,
    last_disconnect_unix: Option<u64>,
}

impl PeerPenalty {
    pub fn register_disconnect(&mut self, now_unix: u64) {
        self.last_disconnect_unix = Some(now_unix);
    }

    pub fn can_reconnect(&self, now_unix: u64, limits: &ConnectionLimits) -> bool {
        match self.last_disconnect_unix {
            Some(last_disconnect) => {
                now_unix.saturating_sub(last_disconnect) >= limits.reconnect_delay_secs
            }
            None => true,
        }
    }

    pub fn register_invalid_message(&mut self, now_unix: u64, limits: &ConnectionLimits) -> bool {
        self.invalid_messages += 1;

        if self.invalid_messages >= limits.max_invalid_messages {
            self.invalid_messages = 0;
            self.banned_until_unix = Some(now_unix.saturating_add(limits.temporary_ban_secs));
            return true;
        }

        false
    }

    pub fn is_banned(&self, now_unix: u64) -> bool {
        matches!(self.banned_until_unix, Some(until) if now_unix < until)
    }

    pub fn clear_expired_ban(&mut self, now_unix: u64) {
        if matches!(self.banned_until_unix, Some(until) if now_unix >= until) {
            self.banned_until_unix = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_messages_trigger_temporary_ban() {
        let limits = ConnectionLimits::default();
        let mut peer = PeerPenalty::default();
        let now = 1_000;

        assert!(!peer.register_invalid_message(now, &limits));
        assert!(!peer.register_invalid_message(now, &limits));
        assert!(peer.register_invalid_message(now, &limits));
        assert!(peer.is_banned(now + 1));
    }

    #[test]
    fn reconnect_delay_is_enforced() {
        let limits = ConnectionLimits::default();
        let mut peer = PeerPenalty::default();
        let now = 2_000;

        peer.register_disconnect(now);

        assert!(!peer.can_reconnect(now + 1, &limits));
        assert!(peer.can_reconnect(now + limits.reconnect_delay_secs, &limits));
    }

    #[test]
    fn expired_ban_is_cleared() {
        let limits = ConnectionLimits::default();
        let mut peer = PeerPenalty::default();
        let now = 3_000;

        for _ in 0..limits.max_invalid_messages {
            peer.register_invalid_message(now, &limits);
        }

        assert!(peer.is_banned(now + 1));

        peer.clear_expired_ban(now + limits.temporary_ban_secs);

        assert!(!peer.is_banned(now + limits.temporary_ban_secs));
    }
}
