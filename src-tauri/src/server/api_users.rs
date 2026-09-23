use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::{agent_permissions, permissions, users},
    domain::user::{User, HOST_USER_ID},
    error::{ApiError, ApiResult},
    server::CoreState,
};

fn require_admin(actor: &User) -> ApiResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("仅管理员可以管理用户"))
    }
}

pub async fn list_users(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
) -> ApiResult<Json<Vec<ManagedUser>>> {
    require_admin(&actor)?;
    let connection = core.db.lock().unwrap();
    let mut result = users::list(&connection)?;
    if !actor.is_super_admin() {
        result.retain(|user| user.role == "user");
    }
    Ok(Json(
        result
            .into_iter()
            .map(|user| {
                let has_permissions = user.is_admin()
                    || permissions::list(&connection, &user.id)?
                        .iter()
                        .any(|permission| permission.field == "project_access");
                Ok(ManagedUser {
                    user,
                    has_permissions,
                })
            })
            .collect::<ApiResult<Vec<_>>>()?,
    ))
}

#[derive(Debug, Serialize)]
pub struct ManagedUser {
    #[serde(flatten)]
    pub user: User,
    pub has_permissions: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchUserBody {
    pub username: Option<String>,
    pub role: Option<String>,
    pub disabled: Option<bool>,
}

pub async fn patch_user(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PatchUserBody>,
) -> ApiResult<Json<User>> {
    if body.username.is_none() && body.role.is_none() && body.disabled.is_none() {
        return Err(ApiError::bad_request("请选择要修改的用户字段"));
    }
    let connection = core.db.lock().unwrap();
    let target = users::get(&connection, &id)?;
    let is_self = actor.id == target.id;

    if is_self {
        if body.role.is_some() || body.disabled.is_some() {
            return Err(ApiError::forbidden("不能修改自己的角色或停用自己的账号"));
        }
    } else {
        require_admin(&actor)?;
        if !actor.is_super_admin() {
            if target.role != "user" {
                return Err(ApiError::forbidden("管理员只能管理普通用户"));
            }
            if body.username.is_some() || body.role.is_some() {
                return Err(ApiError::forbidden("管理员只能停用或启用普通用户"));
            }
        }
    }

    if target.id == HOST_USER_ID && (body.role.is_some() || body.disabled.is_some()) {
        return Err(ApiError::forbidden("内置主机账号不能降级或停用"));
    }
    if body.role.is_some() && !actor.is_super_admin() {
        return Err(ApiError::forbidden("仅超级管理员可以设置角色"));
    }

    let mut updated = target;
    if let Some(username) = body.username {
        updated = users::rename(&connection, &id, &username)?;
    }
    if let Some(role) = body.role {
        updated = users::set_role(&connection, &id, &role)?;
    }
    if let Some(disabled) = body.disabled {
        updated = users::set_disabled(&connection, &id, disabled)?;
    }
    Ok(Json(updated))
}

pub async fn delete_user(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    if !actor.is_super_admin() {
        return Err(ApiError::forbidden("仅超级管理员可以删除用户"));
    }
    if id == HOST_USER_ID {
        return Err(ApiError::forbidden("内置主机账号不能删除"));
    }
    if id == actor.id {
        return Err(ApiError::forbidden("不能删除当前登录账号"));
    }
    let connection = core.db.lock().unwrap();
    users::remove(&connection, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct PermissionsResponse {
    pub user: User,
    pub permissions: Vec<permissions::Permission>,
}

fn may_manage_permissions(actor: &User, target: &User) -> ApiResult<()> {
    require_admin(actor)?;
    if target.role != "user" {
        return Err(ApiError::forbidden(
            "管理员和超级管理员自动拥有全部权限，无需单独配置",
        ));
    }
    Ok(())
}

pub async fn get_permissions(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<Json<PermissionsResponse>> {
    let connection = core.db.lock().unwrap();
    let target = users::get(&connection, &id)?;
    may_manage_permissions(&actor, &target)?;
    Ok(Json(PermissionsResponse {
        permissions: permissions::list(&connection, &id)?,
        user: target,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutPermissionsBody {
    pub permissions: Vec<permissions::Permission>,
}

pub async fn put_permissions(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PutPermissionsBody>,
) -> ApiResult<Json<PermissionsResponse>> {
    let mut connection = core.db.lock().unwrap();
    let target = users::get(&connection, &id)?;
    may_manage_permissions(&actor, &target)?;
    let updated = permissions::replace(&mut connection, &id, &body.permissions)?;
    Ok(Json(PermissionsResponse {
        user: target,
        permissions: updated,
    }))
}

#[derive(Debug, Serialize)]
pub struct AgentPermissionsResponse {
    pub user: User,
    pub permissions: agent_permissions::AgentPermissions,
}

fn may_manage_agent_permissions(actor: &User, target: &User) -> ApiResult<()> {
    require_admin(actor)?;
    if target.role != "user" && !actor.is_super_admin() {
        return Err(ApiError::forbidden("仅超级管理员可以调整管理员的 Agent 权限"));
    }
    Ok(())
}

pub async fn get_agent_permissions(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
) -> ApiResult<Json<AgentPermissionsResponse>> {
    let conn = core.db.lock().unwrap();
    let target = users::get(&conn, &id)?;
    may_manage_agent_permissions(&actor, &target)?;
    Ok(Json(AgentPermissionsResponse {
        permissions: agent_permissions::get(&conn, &id)?,
        user: target,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutAgentPermissionsBody {
    pub permissions: agent_permissions::AgentPermissions,
}

pub async fn put_agent_permissions(
    State(core): State<CoreState>,
    Extension(actor): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PutAgentPermissionsBody>,
) -> ApiResult<Json<AgentPermissionsResponse>> {
    let conn = core.db.lock().unwrap();
    let target = users::get(&conn, &id)?;
    may_manage_agent_permissions(&actor, &target)?;
    Ok(Json(AgentPermissionsResponse {
        permissions: agent_permissions::put(&conn, &id, &body.permissions)?,
        user: target,
    }))
}
