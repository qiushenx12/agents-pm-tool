use crate::{db::history, domain::user::User, error::ApiResult, server::CoreState};
use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct HistoryQuery {
    before: Option<i64>,
}

pub async fn list(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<history::HistoryPage>> {
    let conn = core.db.lock().unwrap();
    Ok(Json(history::list(&conn, &user, &id, query.before)?))
}

pub async fn undo(
    State(core): State<CoreState>,
    Extension(user): Extension<User>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<crate::domain::task::Task>>> {
    let mut conn = core.db.lock().unwrap();
    let result = crate::db::undo::apply(&mut conn, &user, id)?;
    let notices: Vec<_> = result
        .before
        .iter()
        .zip(&result.tasks)
        .filter_map(|(before, after)| super::finish_notice::notice_on_finish(before, after))
        .collect();
    let mut tasks = result.tasks;
    let visible = crate::db::permissions::visible_projects(&conn, &user)?;
    crate::db::tasks::retain_visible_dependencies(&conn, &mut tasks, visible.as_deref())?;
    drop(conn);
    for notice in notices {
        core.finish_notices.notify(notice);
    }
    core.events.notify();
    Ok(Json(tasks))
}
