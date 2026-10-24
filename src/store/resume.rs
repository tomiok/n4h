use super::*;
use crate::resume::ResumeFacts;
use serde_json::{Value, json};

fn operation_summary(record: &StoredOperation) -> Value {
    let op = &record.operation;
    json!({"operation_id": op.id, "checkpoint_id": op.checkpoint_id, "created_at": op.created_at,
        "operation_kind": op.kind, "source_id": op.source_id, "status": record.status,
        "files_total": op.files.len(), "origin": "n4h_record"})
}

impl Store {
    pub fn resume_facts(&self, limit: usize) -> Result<ResumeFacts> {
        let transaction = self.database.begin_read()?;
        let operations_table = transaction.open_table(OPERATIONS)?;
        let mut recent = Vec::new();
        let mut pending = Vec::new();
        for row in operations_table.iter()? {
            let (id, _) = row?;
            // Only metadata: never load checkpoint blobs or read source files.
            let record = load_record(&transaction, id.value())?;
            if pending_status(&record.status)? {
                pending.push(operation_summary(&record));
            }
            recent.push(operation_summary(&record));
        }
        recent.sort_by(|a, b| {
            b["created_at"]
                .as_u64()
                .cmp(&a["created_at"].as_u64())
                .then_with(|| b["operation_id"].as_str().cmp(&a["operation_id"].as_str()))
        });
        pending.sort_by(|a, b| {
            a["created_at"]
                .as_u64()
                .cmp(&b["created_at"].as_u64())
                .then_with(|| a["operation_id"].as_str().cmp(&b["operation_id"].as_str()))
        });
        let pending_total = pending.len();
        let operations_total = recent.len();
        pending.truncate(limit);
        recent.truncate(limit);
        let table = transaction.open_table(PLANS)?;
        let sources = transaction.open_table(SOURCE_INDEX)?;
        let now = model::now();
        let mut plans = Vec::new();
        for row in table.iter()? {
            let (id, payload) = row?;
            let plan: model::Plan = serde_json::from_str(payload.value())?;
            if plan.id != id.value() {
                return Err(invalid_state("Plan key does not match its stored ID."));
            }
            model::validate_changes(plan.files.iter().map(|f| &f.change), plan.snapshot_version)?;
            let source = source_key("apply", &plan.id);
            let operation = sources
                .get(source.as_str())?
                .map(|id| load_record(&transaction, id.value()))
                .transpose()?;
            if operation
                .as_ref()
                .is_some_and(|r| r.operation.kind != "apply" || r.operation.source_id != plan.id)
            {
                return Err(invalid_state(
                    "Plan source index points to a different operation.",
                ));
            }
            let selected: usize = plan.files.iter().map(|f| f.match_ids.len()).sum();
            let expires = plan.created_at.saturating_add(model::RESULT_TTL);
            plans.push(json!({"plan_id": plan.id, "result_id": plan.result_id, "created_at": plan.created_at,
                "files_total": plan.files.len(), "selected_matches": selected + plan.unchanged_matches,
                "expires_at": expires, "expired": now >= expires,
                "expiry_scope": "unapplied_plan_and_inspection_not_checkpoint",
                "application": operation.as_ref().map(operation_summary), "origin": "n4h_record"}));
        }
        plans.sort_by(|a, b| {
            b["created_at"]
                .as_u64()
                .cmp(&a["created_at"].as_u64())
                .then_with(|| b["plan_id"].as_str().cmp(&a["plan_id"].as_str()))
        });
        let plans_total = plans.len();
        plans.truncate(limit);
        Ok(ResumeFacts {
            pending,
            pending_total,
            operations: recent,
            operations_total,
            plans,
            plans_total,
        })
    }
}
