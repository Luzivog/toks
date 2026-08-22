use axum::{
    extract::{Json, State},
    http::StatusCode,
};

use crate::codex_router::BankedResetConsumed;

use super::ProxyState;

pub(super) async fn banked_reset_consumed(
    State(state): State<ProxyState>,
    Json(request): Json<BankedResetConsumed>,
) -> StatusCode {
    match state.engine.banked_reset_consumed(&request.account_id) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
