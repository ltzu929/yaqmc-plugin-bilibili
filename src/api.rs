use crate::{error, yaqmc};
use alloc::string::{String, ToString};
use serde_json::{json, Value};

pub const ORIGIN: &str = "https://api.bilibili.com";

pub fn data(path: &str, fields: &[&str], handle: Option<&str>) -> Result<Value, String> {
    let fields = ["code", "data"]
        .into_iter()
        .chain(fields.iter().copied())
        .collect::<alloc::vec::Vec<_>>();
    let mut request = json!({"url": alloc::format!("{ORIGIN}{path}"), "responseFormat":"json", "responseJsonFields":fields});
    if let Some(handle) = handle {
        request["credentialHeaders"] = json!([{"name":"cookie","handle":handle}]);
    }
    let raw = yaqmc::provider::network::request(&request.to_string())
        .map_err(|_| error("network", "连接 B 站失败", true))?;
    let mut envelope: Value = serde_json::from_str(&raw)
        .map_err(|_| error("malformed-response", "B 站返回的数据格式无效", false))?;
    let body = &mut envelope["bodyJson"];
    match body["code"].as_i64() {
        Some(0) if !body["data"].is_null() => Ok(body["data"].take()),
        Some(-101) => {
            if let Some(handle) = handle {
                yaqmc::provider::storage::kv_set(crate::account::EXPIRED_HANDLE, handle)?;
            }
            Err(error("authentication-expired", "请重新登录 B 站", false))
        }
        Some(-404) => Err(error("not-found", "内容已失效", false)),
        Some(-403 | -400 | 62002) => Err(error("unavailable", "当前无法访问该内容", false)),
        Some(-412 | -352) => Err(error("rate-limited", "请稍后再试", true)),
        _ => Err(error("provider-failure", "未能取得 B 站数据", true)),
    }
}
