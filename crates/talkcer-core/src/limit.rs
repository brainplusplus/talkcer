//! Fixed-window request limit for a paired remote client.

use std::collections::HashMap;

use crate::error::CoreError;

/// Counts requests for each remote token.
#[derive(Debug)]
pub struct RateGate {
    limit: u32,
    used: HashMap<String, u32>,
}

impl RateGate {
    /// Allow `limit` requests per token for the life of this process.
    #[must_use]
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            used: HashMap::new(),
        }
    }

    /// Consume one request.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::RateLimited`] after the limit is consumed.
    pub fn allow(&mut self, key: &str) -> Result<(), CoreError> {
        let count = self.used.entry(key.to_owned()).or_insert(0);
        if *count >= self.limit {
            return Err(CoreError::RateLimited);
        }
        *count += 1;
        Ok(())
    }
}
