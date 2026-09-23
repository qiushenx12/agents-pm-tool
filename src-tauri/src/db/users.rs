use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Local};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::domain::{
    task::now_str,
    user::{self, User, DEFAULT_HOST_USERNAME, HOST_USER_ID},
};
use crate::error::{ApiError, ApiResult};

pub const SESSION_DAYS: i64 = 7;

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    let id: String = row.get("id")?;
    Ok(User {
        is_host: id == HOST_USER_ID,
        id,
        username: row.get("username")?,
        role: row.get("role")?,
        created_at: row.get("created_at")?,
        disabled: row.get::<_, i64>("disabled")? != 0,
    })
}

pub fn ensure_host(conn: &Connection) -> ApiResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO users
           (id, username, password_hash, role, created_at, disabled)
         VALUES (?1, ?2, '', 'super_admin', ?3, 0)",
        params![HOST_USER_ID, DEFAULT_HOST_USERNAME, now_str()],
    )?;
    conn.execute(
        "UPDATE users
         SET password_hash='', role='super_admin', disabled=0
         WHERE id=?1",
        [HOST_USER_ID],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> ApiResult<User> {
    conn.query_row("SELECT * FROM users WHERE id=?1", [id], row_to_user)
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => ApiError::not_found("用户不存在"),
            other => ApiError::from(other),
        })
}

pub fn list(conn: &Connection) -> ApiResult<Vec<User>> {
    let mut statement = conn.prepare(
        "SELECT * FROM users
         ORDER BY CASE WHEN id='host' THEN 0 ELSE 1 END, created_at ASC, username ASC",
    )?;
    let rows = statement.query_map([], row_to_user)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 提交人筛选的候选清单：与任务上 `submitter_name` 显示形态一一对应。
/// 每条是「用户名」或「Agent（用户名）」，覆盖未停用账号在可见项目里出现过的所有
/// (owner, submitter) 组合；owner 置空的历史任务归入「未知用户」。
/// `visible_projects` 为 None（管理员）时看全量；Some 时空集即无人可选。
pub fn list_submitter_names(
    conn: &Connection,
    visible_projects: Option<&[String]>,
) -> ApiResult<Vec<String>> {
    // 管理员不受项目范围限制；普通用户只看得见授权项目，候选同样收窄，
    // 避免从筛选器里枚举出不可见项目里出现过任务的提交人。
    let scope = match visible_projects {
        None => String::new(),
        Some([]) => return Ok(Vec::new()),
        Some(projects) => format!(
            "AND t.project IN ({})",
            vec!["?"; projects.len()].join(",")
        ),
    };
    let sql = format!(
        "SELECT u.username, t.submitter FROM tasks t
         LEFT JOIN users u ON u.id = t.owner_user_id
         WHERE (u.disabled = 0 OR u.id IS NULL) {scope}
         GROUP BY u.username, t.submitter"
    );
    let mut statement = conn.prepare(&sql)?;
    let params: Vec<&str> = match visible_projects {
        Some(projects) => projects.iter().map(String::as_str).collect(),
        None => Vec::new(),
    };
    let rows = statement.query_map(params_from_iter(params), |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, String>(1)?,
        ))
    })?;
    let mut names: Vec<String> = rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(username, submitter)| {
            crate::domain::task::submitter_name(&submitter, username.as_deref())
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    Ok(names)
}

fn validate_username(username: &str) -> ApiResult<&str> {
    let username = username.trim();
    if username.is_empty() || username.chars().count() > 64 {
        return Err(ApiError::unprocessable(
            "用户名不能为空且不能超过 64 个字符",
        ));
    }
    Ok(username)
}

fn validate_password(password: &str) -> ApiResult<()> {
    if password.chars().count() < 8 || password.chars().count() > 256 {
        return Err(ApiError::unprocessable("密码长度须为 8–256 个字符"));
    }
    Ok(())
}

fn hash_password(password: &str) -> ApiResult<String> {
    validate_password(password)?;
    let salt =
        SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).map_err(ApiError::internal)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(ApiError::internal)
}

pub fn create(conn: &Connection, username: &str, password: &str) -> ApiResult<User> {
    let username = validate_username(username)?;
    let id = uuid::Uuid::new_v4().to_string();
    let password_hash = hash_password(password)?;
    conn.execute(
        "INSERT INTO users (id, username, password_hash, role, created_at, disabled)
         VALUES (?1, ?2, ?3, 'user', ?4, 0)",
        params![id, username, password_hash, now_str()],
    )
    .map_err(|error| match error {
        rusqlite::Error::SqliteFailure(ref code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            ApiError::conflict("用户名已存在")
        }
        other => ApiError::from(other),
    })?;
    get(conn, &id)
}

pub fn authenticate_password(conn: &Connection, username: &str, password: &str) -> ApiResult<User> {
    let record: Option<(String, String)> = conn
        .query_row(
            "SELECT id, password_hash FROM users WHERE username=?1",
            [username.trim()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((id, password_hash)) = record else {
        return Err(ApiError::unauthorized("用户名或密码不正确"));
    };
    if id == HOST_USER_ID || password_hash.is_empty() {
        return Err(ApiError::forbidden("主机账号不能使用密码登录"));
    }
    let parsed = PasswordHash::new(&password_hash)
        .map_err(|_| ApiError::unauthorized("用户名或密码不正确"))?;
    if Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(ApiError::unauthorized("用户名或密码不正确"));
    }
    let user = get(conn, &id)?;
    if user.disabled {
        return Err(ApiError::forbidden("账号已停用，请联系管理员"));
    }
    Ok(user)
}

fn session_expiry() -> String {
    (Local::now() + Duration::days(SESSION_DAYS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn create_session(conn: &Connection, user_id: &str) -> ApiResult<String> {
    let token = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (token, user_id, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![token, user_id, now_str(), session_expiry()],
    )?;
    Ok(token)
}

pub fn authenticate_session(conn: &Connection, token: &str) -> ApiResult<User> {
    let record: Option<(String, String)> = conn
        .query_row(
            "SELECT user_id, expires_at FROM sessions WHERE token=?1",
            [token],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((user_id, expires_at)) = record else {
        return Err(ApiError::unauthorized("登录已失效，请重新登录"));
    };
    if expires_at <= now_str() {
        conn.execute("DELETE FROM sessions WHERE token=?1", [token])?;
        return Err(ApiError::unauthorized("登录已过期，请重新登录"));
    }
    let user = get(conn, &user_id)?;
    if user.disabled {
        conn.execute("DELETE FROM sessions WHERE token=?1", [token])?;
        return Err(ApiError::forbidden("账号已停用，请联系管理员"));
    }
    conn.execute(
        "UPDATE sessions SET expires_at=?2 WHERE token=?1",
        params![token, session_expiry()],
    )?;
    Ok(user)
}

pub fn delete_session(conn: &Connection, token: &str) -> ApiResult<()> {
    conn.execute("DELETE FROM sessions WHERE token=?1", [token])?;
    Ok(())
}

pub fn rename(conn: &Connection, id: &str, username: &str) -> ApiResult<User> {
    let username = validate_username(username)?;
    conn.execute(
        "UPDATE users SET username=?2 WHERE id=?1",
        params![id, username],
    )
    .map_err(|error| match error {
        rusqlite::Error::SqliteFailure(ref code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            ApiError::conflict("用户名已存在")
        }
        other => ApiError::from(other),
    })?;
    get(conn, id)
}

pub fn set_role(conn: &Connection, id: &str, role: &str) -> ApiResult<User> {
    if !user::is_valid_role(role) {
        return Err(ApiError::unprocessable(format!(
            "角色不合法，合法取值：{}",
            user::ROLES.join(" / ")
        )));
    }
    conn.execute("UPDATE users SET role=?2 WHERE id=?1", params![id, role])?;
    get(conn, id)
}

pub fn set_disabled(conn: &Connection, id: &str, disabled: bool) -> ApiResult<User> {
    conn.execute(
        "UPDATE users SET disabled=?2 WHERE id=?1",
        params![id, i64::from(disabled)],
    )?;
    if disabled {
        conn.execute("DELETE FROM sessions WHERE user_id=?1", [id])?;
        conn.execute("UPDATE agent_tokens SET revoked=1 WHERE user_id=?1", [id])?;
    }
    get(conn, id)
}

pub fn remove(conn: &Connection, id: &str) -> ApiResult<()> {
    let changed = conn.execute("DELETE FROM users WHERE id=?1", [id])?;
    if changed == 0 {
        return Err(ApiError::not_found("用户不存在"));
    }
    Ok(())
}

pub fn active_agent_token(conn: &Connection, user_id: &str) -> ApiResult<Option<String>> {
    conn.query_row(
        "SELECT token FROM agent_tokens
         WHERE user_id=?1 AND revoked=0 ORDER BY created_at DESC LIMIT 1",
        [user_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(ApiError::from)
}

pub fn set_agent_token(conn: &Connection, user_id: &str, token: &str) -> ApiResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE agent_tokens SET revoked=1 WHERE user_id=?1",
        [user_id],
    )?;
    tx.execute(
        "INSERT INTO agent_tokens (token, user_id, created_at, revoked)
         VALUES (?1, ?2, ?3, 0)",
        params![token, user_id, now_str()],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn regenerate_agent_token(conn: &Connection, user_id: &str) -> ApiResult<String> {
    get(conn, user_id)?;
    let token = uuid::Uuid::new_v4().to_string();
    set_agent_token(conn, user_id, &token)?;
    Ok(token)
}

pub fn revoke_agent_tokens(conn: &Connection, user_id: &str) -> ApiResult<()> {
    conn.execute(
        "UPDATE agent_tokens SET revoked=1 WHERE user_id=?1",
        [user_id],
    )?;
    Ok(())
}

pub fn authenticate_agent_token(conn: &Connection, token: &str) -> ApiResult<User> {
    let user_id: Option<String> = conn
        .query_row(
            "SELECT user_id FROM agent_tokens WHERE token=?1 AND revoked=0",
            [token],
            |row| row.get(0),
        )
        .optional()?;
    let Some(user_id) = user_id else {
        return Err(ApiError::unauthorized("Agent token 缺失、已吊销或不正确"));
    };
    let user = get(conn, &user_id)?;
    if user.disabled {
        return Err(ApiError::forbidden("Agent token 所属账号已停用"));
    }
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn submitter_names_scoped_to_visible_projects() {
        let mut conn = db::open_memory().unwrap();
        let alice = create(&conn, "alice", "password-123").unwrap();
        let bob = create(&conn, "bob", "password-123").unwrap();
        conn.execute(
            "INSERT INTO projects (name, color, sort_order, local_path, git_url, created_at)
             VALUES ('other-project', '#007AFF', 2, '', '', ?1)",
            [now_str()],
        )
        .unwrap();
        let mut new_task = |owner: &str, project: &str, submitter: &str| crate::db::tasks::create(
            &mut conn,
            &crate::db::tasks::NewTask {
                project,
                task_type: "优化",
                description: "x",
                note: "",
                submitter,
                owner_user_id: Some(owner),
                priority: None,
                predecessor_task_ids: &[],
                unlock_task_ids: &[],
            },
        )
        .unwrap();
        // alice 在默认项目同时有「用户」和「Agent」任务；bob 只在 other-project 有 Agent 任务
        new_task(&alice.id, "default-project", "用户");
        new_task(&alice.id, "default-project", "Agent");
        new_task(&bob.id, "other-project", "Agent");
        // owner 置空的历史任务归到「未知用户」
        conn.execute(
            "INSERT INTO tasks (id, seq, project, type, description, status, submitter, created_at, updated_at, position)
             VALUES ('legacy', 999, 'default-project', '优化', 'legacy', '未开始', '用户', ?1, ?1, 999)",
            [now_str()],
        )
        .unwrap();

        // 管理员（None）看全量：每个 (owner, submitter) 组合一条
        assert_eq!(
            list_submitter_names(&conn, None).unwrap(),
            vec![
                "Agent（alice）".to_string(),
                "Agent（bob）".to_string(),
                "alice".to_string(),
                "未知用户".to_string(),
            ]
        );
        // 普通用户收窄到可见项目
        let visible = vec!["default-project".to_string()];
        assert_eq!(
            list_submitter_names(&conn, Some(&visible)).unwrap(),
            vec![
                "Agent（alice）".to_string(),
                "alice".to_string(),
                "未知用户".to_string(),
            ]
        );
        // 空可见项目 → 空名单
        assert_eq!(list_submitter_names(&conn, Some(&[])).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn password_is_hashed_and_session_slides() {
        let conn = db::open_memory().unwrap();
        let user = create(&conn, "alice", "correct horse").unwrap();
        let stored: String = conn
            .query_row(
                "SELECT password_hash FROM users WHERE id=?1",
                [&user.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored, "correct horse");
        assert!(stored.starts_with("$argon2"));
        assert_eq!(
            authenticate_password(&conn, "alice", "correct horse")
                .unwrap()
                .id,
            user.id
        );
        assert!(authenticate_password(&conn, "alice", "wrong pass").is_err());

        let token = create_session(&conn, &user.id).unwrap();
        conn.execute(
            "UPDATE sessions SET expires_at='2099-01-01 00:00:00' WHERE token=?1",
            [&token],
        )
        .unwrap();
        authenticate_session(&conn, &token).unwrap();
        let expiry: String = conn
            .query_row(
                "SELECT expires_at FROM sessions WHERE token=?1",
                [&token],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(expiry, "2099-01-01 00:00:00");
    }

    #[test]
    fn expired_and_disabled_sessions_are_rejected() {
        let conn = db::open_memory().unwrap();
        let user = create(&conn, "bob", "password8").unwrap();
        let token = create_session(&conn, &user.id).unwrap();
        conn.execute(
            "UPDATE sessions SET expires_at='2000-01-01 00:00:00' WHERE token=?1",
            [&token],
        )
        .unwrap();
        assert_eq!(
            authenticate_session(&conn, &token).unwrap_err().status,
            axum::http::StatusCode::UNAUTHORIZED
        );

        let token = create_session(&conn, &user.id).unwrap();
        conn.execute("UPDATE users SET disabled=1 WHERE id=?1", [&user.id])
            .unwrap();
        assert_eq!(
            authenticate_session(&conn, &token).unwrap_err().status,
            axum::http::StatusCode::FORBIDDEN
        );
    }
}
