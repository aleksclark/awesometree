use crate::model::error::{ErrorCode, SwitchboardError};

/// Ensure `child` only narrows `parent` policy.
/// Boolean keys: child may set false when parent is true (narrow); cannot set true when parent is false.
/// Other scalars must match exactly when present in both.
pub fn policy_narrows(
    parent: &serde_json::Value,
    child: &serde_json::Value,
) -> Result<(), SwitchboardError> {
    let Some(parent_obj) = parent.as_object() else {
        return Ok(());
    };
    let Some(child_obj) = child.as_object() else {
        return Err(SwitchboardError::new(
            ErrorCode::PolicyBroadening,
            "session policy must be an object",
        ));
    };
    for (k, pv) in parent_obj {
        let Some(cv) = child_obj.get(k) else {
            // Omitting a parent key is narrowing.
            continue;
        };
        if let Some(pb) = pv.as_bool() {
            let Some(cb) = cv.as_bool() else {
                return Err(SwitchboardError::new(
                    ErrorCode::PolicyBroadening,
                    format!("policy type mismatch for key {k}"),
                )
                .with_entity("policy", k));
            };
            if cb && !pb {
                return Err(SwitchboardError::new(
                    ErrorCode::PolicyBroadening,
                    format!("policy broadens parent at key {k}"),
                )
                .with_entity("policy", k));
            }
            continue;
        }
        if pv != cv {
            return Err(SwitchboardError::new(
                ErrorCode::PolicyBroadening,
                format!("policy differs from parent at key {k}"),
            )
            .with_entity("policy", k));
        }
    }
    Ok(())
}
