use crate::{
    api, error,
    yaqmc::provider::{credentials, storage},
    PROVIDER,
};
use alloc::string::{String, ToString};
use serde_json::{json, Value};

const SESSION: &str = "account.session";
pub const EXPIRED_HANDLE: &str = "account.expired-handle";

pub fn saved() -> Result<Option<Value>, String> {
    storage::kv_get(SESSION)?
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|_| error("provider-failure", "账号记录无效", false))
        })
        .transpose()
}

pub fn handle(session: &Value) -> Result<&str, String> {
    session["handle"]
        .as_str()
        .ok_or_else(|| error("authentication-expired", "请登录 B 站", false))
}

fn snapshot(session: Option<&Value>) -> Value {
    let mut value = json!({
        "providerId": PROVIDER, "profileId":"default", "revision":0,
        "capabilities":{"qrLogin":false,"favoriteRead":false,"favoriteWrite":false,
            "playlistRead":true,"playlistWrite":false,"recentHistoryRead":false},
        "state":"guest","profile":null,"entitlement":null
    });
    if let Some(user) = session {
        value["state"] = json!("authenticated");
        value["profile"] =
            json!({"nickname":user["name"], "avatarUrl":user["face"],"maskedIdentity":""});
        value["entitlement"] = json!({"tier":"unknown","membership":"unknown","permittedQualities":["standard"],"restrictions":[]});
    }
    value
}

pub fn invoke(op: &str, payload: &Value) -> Result<Value, String> {
    match op {
        "account.auth.login-methods" => {
            Ok(json!([{"id":"bilibili","flow":"oauth","label":"登录哔哩哔哩"}]))
        }
        "account.auth.prepare-oauth" if payload["loginProvider"] == "bilibili" => Ok(json!({
            "url":"https://passport.bilibili.com/login",
            "navigationAllowlist":["https://passport.bilibili.com/", "https://www.bilibili.com/"],
            "callbackMatcher":{"urlPrefix":"https://api.bilibili.com/"},
            "browserSession":{"url":"https://api.bilibili.com/","format":"cookie",
                "cookieNames":["SESSDATA"],"requiredCookieNames":["SESSDATA"],"credentialOrigins":[api::ORIGIN]}
        })),
        "account.auth.complete-oauth" => {
            let new_handle = payload["browserCredentials"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["origin"] == api::ORIGIN))
                .and_then(|item| item["handle"].as_str())
                .ok_or_else(|| error("authorization-rejected", "请完成 B 站登录", false))?;
            let user = api::data(
                "/x/web-interface/nav",
                &["isLogin", "mid", "uname", "face"],
                Some(new_handle),
            )?;
            if user["isLogin"] != true || user["mid"].as_u64().unwrap_or(0) == 0 {
                return Err(error("authorization-rejected", "登录尚未完成", false));
            }
            let previous = saved()?;
            let session = json!({"handle":new_handle, "mid":user["mid"], "name":user["uname"], "face":crate::cover_url(&user["face"])});
            storage::kv_set(SESSION, &session.to_string())?;
            if let Some(old) = previous
                .as_ref()
                .and_then(|s| s["handle"].as_str())
                .filter(|h| *h != new_handle)
            {
                let _ = credentials::delete(old);
            }
            Ok(snapshot(Some(&session)))
        }
        "account.snapshot" | "account.auth.cancel-oauth" => {
            let session = saved()?;
            let mut state = snapshot(session.as_ref());
            if let Some(user) = &session {
                if storage::kv_get(EXPIRED_HANDLE)?.as_deref() == Some(handle(user)?) {
                    state["state"] = json!("session-expired");
                }
            }
            Ok(state)
        }
        "account.playlists" | "account.playlist-tracks" => {
            let session =
                saved()?.ok_or_else(|| error("authentication-expired", "请先登录 B 站", false))?;
            if op == "account.playlists" {
                crate::library::folders(payload, &session)
            } else {
                crate::library::tracks(payload, &session)
            }
        }
        "account.sign-out" => {
            if let Some(session) = saved()? {
                credentials::delete(handle(&session)?)?;
                storage::kv_delete(SESSION)?;
                storage::kv_delete(EXPIRED_HANDLE)?;
            }
            Ok(snapshot(None))
        }
        _ => Err(error(
            "unsupported-operation",
            "当前账号不支持此操作",
            false,
        )),
    }
}
