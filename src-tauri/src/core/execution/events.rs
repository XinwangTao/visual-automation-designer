//! Execution events and status
//!
//! This module defines the events emitted during flow execution
//! and the execution status.
//!
//! Validates: Requirements 5.2, 5.3, 5.4

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::models::BlockId;

/// Execution status of a flow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionStatus {
    /// Flow is idle, not running
    Idle,
    /// Flow is currently running
    Running,
    /// Flow is paused
    Paused,
    /// Flow completed successfully
    Completed,
    /// Flow stopped by user
    Stopped,
    /// Flow stopped due to error
    Error,
}

impl Default for ExecutionStatus {
    fn default() -> Self {
        ExecutionStatus::Idle
    }
}

impl ExecutionStatus {
    /// Check if execution is in progress
    pub fn is_active(&self) -> bool {
        matches!(self, ExecutionStatus::Running | ExecutionStatus::Paused)
    }
}

/// Screen rectangle attached to a block event as evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitRect {
    /// Left edge in virtual-screen physical pixels.
    pub x: i32,
    /// Top edge in virtual-screen physical pixels.
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl HitRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}

/// Per-block diagnostic evidence for headless debugging.
///
/// `None` fields mean the block did not perform a match (wait, input, loop...).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockEvidence {
    /// Best NCC score seen (0.0~1.0), even when the match failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Where the target was found, in virtual-screen physical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_rect: Option<HitRect>,
    /// Human-readable target description (image name, UIA selector, coords).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Events emitted during flow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecutionEvent {
    /// Flow execution started
    Started {
        timestamp: DateTime<Utc>,
    },
    /// A block started executing
    BlockStarted {
        block_id: BlockId,
        #[serde(default)]
        attempt: u32,
        timestamp: DateTime<Utc>,
    },
    /// A block completed execution
    BlockCompleted {
        block_id: BlockId,
        success: bool,
        #[serde(default)]
        elapsed_ms: u64,
        #[serde(default)]
        attempt: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence: Option<BlockEvidence>,
        timestamp: DateTime<Utc>,
    },
    /// A block execution resulted in an error
    BlockError {
        block_id: BlockId,
        message: String,
        #[serde(default)]
        elapsed_ms: u64,
        #[serde(default)]
        attempt: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence: Option<BlockEvidence>,
        timestamp: DateTime<Utc>,
    },
    /// Flow execution completed successfully
    FlowCompleted {
        timestamp: DateTime<Utc>,
    },
    /// Flow execution failed after startup
    ExecutionFailed {
        message: String,
        block_id: Option<BlockId>,
        timestamp: DateTime<Utc>,
    },
    /// Flow execution stopped
    Stopped {
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// Flow execution paused
    Paused {
        block_id: BlockId,
        timestamp: DateTime<Utc>,
    },
    /// Flow execution resumed
    Resumed {
        block_id: BlockId,
        timestamp: DateTime<Utc>,
    },
}

impl ExecutionEvent {
    /// Create a Started event
    pub fn started() -> Self {
        ExecutionEvent::Started {
            timestamp: Utc::now(),
        }
    }

    /// Create a BlockStarted event
    pub fn block_started(block_id: BlockId) -> Self {
        Self::block_started_with(block_id, 1)
    }

    /// Create a BlockStarted event with an explicit attempt number.
    pub fn block_started_with(block_id: BlockId, attempt: u32) -> Self {
        ExecutionEvent::BlockStarted {
            block_id,
            attempt,
            timestamp: Utc::now(),
        }
    }

    /// Create a BlockCompleted event
    pub fn block_completed(block_id: BlockId, success: bool) -> Self {
        Self::block_completed_with(block_id, success, 0, 1, None)
    }

    /// Create a BlockCompleted event with timing, attempt and evidence.
    pub fn block_completed_with(
        block_id: BlockId,
        success: bool,
        elapsed_ms: u64,
        attempt: u32,
        evidence: Option<BlockEvidence>,
    ) -> Self {
        ExecutionEvent::BlockCompleted {
            block_id,
            success,
            elapsed_ms,
            attempt,
            evidence,
            timestamp: Utc::now(),
        }
    }

    /// Create a BlockError event
    pub fn block_error(block_id: BlockId, message: String) -> Self {
        Self::block_error_with(block_id, message, 0, 1, None)
    }

    /// Create a BlockError event with timing, attempt and evidence.
    pub fn block_error_with(
        block_id: BlockId,
        message: String,
        elapsed_ms: u64,
        attempt: u32,
        evidence: Option<BlockEvidence>,
    ) -> Self {
        ExecutionEvent::BlockError {
            block_id,
            message,
            elapsed_ms,
            attempt,
            evidence,
            timestamp: Utc::now(),
        }
    }

    /// Create a FlowCompleted event
    pub fn flow_completed() -> Self {
        ExecutionEvent::FlowCompleted {
            timestamp: Utc::now(),
        }
    }

    /// Create an ExecutionFailed event
    pub fn execution_failed(message: String, block_id: Option<BlockId>) -> Self {
        ExecutionEvent::ExecutionFailed {
            message,
            block_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a Stopped event
    pub fn stopped(reason: String) -> Self {
        ExecutionEvent::Stopped {
            reason,
            timestamp: Utc::now(),
        }
    }

    /// Create a Paused event
    pub fn paused(block_id: BlockId) -> Self {
        ExecutionEvent::Paused {
            block_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a Resumed event
    pub fn resumed(block_id: BlockId) -> Self {
        ExecutionEvent::Resumed {
            block_id,
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_status_default() {
        let status = ExecutionStatus::default();
        assert_eq!(status, ExecutionStatus::Idle);
    }

    #[test]
    fn test_execution_status_is_active() {
        assert!(!ExecutionStatus::Idle.is_active());
        assert!(ExecutionStatus::Running.is_active());
        assert!(ExecutionStatus::Paused.is_active());
        assert!(!ExecutionStatus::Completed.is_active());
        assert!(!ExecutionStatus::Stopped.is_active());
        assert!(!ExecutionStatus::Error.is_active());
    }

    #[test]
    fn test_execution_event_serialization() {
        let event = ExecutionEvent::started();
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("started"));
    }

    #[test]
    fn test_block_event_creation() {
        let block_id = BlockId::new();
        let event = ExecutionEvent::block_started(block_id.clone());
        match event {
            ExecutionEvent::BlockStarted { block_id: id, .. } => {
                assert_eq!(id, block_id);
            }
            _ => panic!("Expected BlockStarted event"),
        }
    }
}
