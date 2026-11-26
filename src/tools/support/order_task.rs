//! Order tracking through the MCP Tasks extension (`io.modelcontextprotocol/tasks`).
//!
//! When the client declares the tasks extension, a `submit_order` that really
//! reaches the exchange answers with a task instead of a plain result. The
//! task follows the order: `working` while it rests or fills partially,
//! `completed` once it is filled, rejected or expired, `cancelled` once it is
//! withdrawn. The client polls `tasks/get`; the model no longer has to call
//! `order_detail` in a loop.
//!
//! `tasks/cancel` is refused: withdrawing the order goes through
//! `cancel_order` and its confirmation gate. US accounts get the plain order
//! ID, since their order detail has a different shape.
//!
//! The server keeps no task state. The task ID is the order ID, and every
//! `tasks/get` reads the order with the caller's own token, so:
//! - any instance behind the load balancer can answer, with no shared store;
//! - a task ID from another account resolves to nothing (the order lookup is
//!   scoped to the token, exactly like `order_detail`);
//! - there is no TTL to pick: the task lives as long as the order can be read
//!   (`ttlMs: null`), which covers GTC/GTD orders resting for days.

use longbridge::trade::{OrderDetail, OrderStatus};
use rmcp::model::{CallToolResult, DetailedTask, JsonObject, Task, TaskPayload, TaskStatus};

/// Prefix that marks a task ID as an order task.
const PREFIX: &str = "order:";
/// Suggested `tasks/get` interval while the order rests.
pub const POLL_INTERVAL_MS: u64 = 5_000;

pub fn task_id(order_id: &str) -> String {
    format!("{PREFIX}{order_id}")
}

/// The order ID of an order task, or `None` for an ID this server never issued.
pub fn order_id(task_id: &str) -> Option<&str> {
    task_id
        .strip_prefix(PREFIX)
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// The seed task returned by `submit_order`.
pub fn created(order_id: &str) -> Task {
    let now = now();
    Task::new(task_id(order_id), TaskStatus::Working, now.clone(), now)
        .with_status_message("Submitted; waiting for the exchange")
        .with_poll_interval_ms(POLL_INTERVAL_MS)
}

/// Where an order's status puts its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Still resting, pending or partly filled.
    Working,
    /// Fully filled.
    Filled,
    /// Ended without a fill the user asked to stop (rejected, expired).
    Failed,
    /// Withdrawn. `partial` when some quantity filled before.
    Cancelled { partial: bool },
}

pub fn phase(status: OrderStatus) -> Phase {
    match status {
        OrderStatus::Filled => Phase::Filled,
        OrderStatus::Rejected | OrderStatus::Expired => Phase::Failed,
        OrderStatus::Canceled => Phase::Cancelled { partial: false },
        OrderStatus::PartialWithdrawal => Phase::Cancelled { partial: true },
        _ => Phase::Working,
    }
}

/// Build the `tasks/get` answer from the order and its tool JSON
/// (the same JSON `order_detail` returns).
pub fn detailed(order: &OrderDetail, detail: serde_json::Value) -> DetailedTask {
    let now = now();
    let created_at = order
        .submitted_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now.clone());
    let message = format!(
        "{:?}: filled {}/{}",
        order.status, order.executed_quantity, order.quantity
    );
    let task = Task::new(
        task_id(&order.order_id),
        TaskStatus::Working,
        created_at,
        now,
    )
    .with_status_message(message);
    let result = |is_error: bool| {
        let mut result = CallToolResult::structured(detail.clone());
        result.is_error = Some(is_error);
        match serde_json::to_value(result).unwrap_or_default() {
            serde_json::Value::Object(map) => map,
            _ => JsonObject::new(),
        }
    };
    let payload = match phase(order.status) {
        Phase::Working => {
            return DetailedTask::new(
                task.with_poll_interval_ms(POLL_INTERVAL_MS),
                TaskPayload::Working,
            );
        }
        Phase::Filled => TaskPayload::Completed {
            result: result(false),
        },
        Phase::Failed => TaskPayload::Completed {
            result: result(true),
        },
        // A partly filled order that was withdrawn still has fills the user
        // must see, so it completes with the detail rather than `cancelled`,
        // which carries no result.
        Phase::Cancelled { partial: true } => TaskPayload::Completed {
            result: result(false),
        },
        Phase::Cancelled { partial: false } => TaskPayload::Cancelled,
    };
    DetailedTask::new(task, payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_round_trips_and_rejects_foreign_ids() {
        assert_eq!(
            order_id(&task_id("701276261045858304")),
            Some("701276261045858304")
        );
        assert_eq!(order_id("order:"), None);
        assert_eq!(order_id("order:1/../2"), None);
        assert_eq!(order_id("task-1"), None);
    }

    #[test]
    fn statuses_map_to_task_phases() {
        for s in [
            OrderStatus::New,
            OrderStatus::WaitToNew,
            OrderStatus::PartialFilled,
            OrderStatus::PendingReplace,
            OrderStatus::Replaced,
            OrderStatus::PendingCancel,
            OrderStatus::NotReported,
        ] {
            assert_eq!(phase(s), Phase::Working, "{s:?}");
        }
        assert_eq!(phase(OrderStatus::Filled), Phase::Filled);
        assert_eq!(phase(OrderStatus::Rejected), Phase::Failed);
        assert_eq!(phase(OrderStatus::Expired), Phase::Failed);
        assert_eq!(
            phase(OrderStatus::Canceled),
            Phase::Cancelled { partial: false }
        );
        assert_eq!(
            phase(OrderStatus::PartialWithdrawal),
            Phase::Cancelled { partial: true }
        );
    }

    #[test]
    fn seed_task_is_working_with_no_ttl() {
        let task = created("42");
        assert_eq!(task.task_id, "order:42");
        assert_eq!(task.status, TaskStatus::Working);
        assert_eq!(task.ttl_ms, None);
        assert_eq!(task.poll_interval_ms, Some(POLL_INTERVAL_MS));
    }
}
