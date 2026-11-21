#![no_std]

extern crate alloc;

mod account;
mod allocator;
mod api;
mod library;
mod memory;
mod playback;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use serde_json::{json, Value};

wit_bindgen::generate!({ path: "wit", world: "provider-account" });

const PROVIDER: &str = "top.ltzu.yaqmc.bilibili";

fn error(code: &str, message: &str, retryable: bool) -> String {
    json!({"code": code, "message": message, "retryable": retryable}).to_string()
}

fn invalid() -> String {
    error("invalid-request", "视频或分 P 编号无效", false)
}

fn valid_bvid(bvid: &str) -> bool {
    bvid.len() == 12 && bvid.starts_with("BV") && bvid.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn video(bvid: &str) -> Result<Value, String> {
    if !valid_bvid(bvid) {
        return Err(invalid());
    }
    let session = account::saved()?;
    let data = api::data(
        &format!("/x/web-interface/view?bvid={bvid}"),
        &[
            "bvid", "title", "pic", "owner", "mid", "name", "pages", "cid", "part", "duration",
            "page",
        ],
        session.as_ref().map(account::handle).transpose()?,
    )?;
    if data["bvid"] != bvid
        || data["title"].as_str().is_none()
        || data["pages"].as_array().is_none_or(|p| p.is_empty())
    {
        return Err(error("upstream", "缺少视频或分 P 信息", false));
    }
    Ok(data)
}

fn cover_url(value: &Value) -> String {
    let raw = value.as_str().unwrap_or("");
    if let Some(rest) = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("//"))
    {
        return format!("https://{rest}");
    }
    raw.strip_prefix("https://")
        .map(|_| raw.to_string())
        .unwrap_or_default()
}

fn artist(data: &Value) -> Value {
    json!({"id": data["owner"]["mid"].to_string(), "name": data["owner"]["name"]})
}

fn artwork(data: &Value) -> Value {
    json!({"src": cover_url(&data["pic"]), "alt": data["title"], "dominantColor": "#475569"})
}

fn song(data: &Value, page: &Value, multipart: bool) -> Result<Value, String> {
    let bvid = data["bvid"].as_str().ok_or_else(invalid)?;
    let cid = page["cid"]
        .as_u64()
        .filter(|cid| *cid > 0)
        .ok_or_else(invalid)?;
    let id = format!("{bvid}:{cid}");
    let title = if multipart {
        page["part"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(data["title"].as_str().ok_or_else(invalid)?)
    } else {
        data["title"].as_str().ok_or_else(invalid)?
    };
    Ok(json!({
        "id": id, "title": title, "artists": [artist(data)],
        "album": {"id": bvid, "title": data["title"]}, "artwork": artwork(data),
        "durationMs": page["duration"].as_u64().unwrap_or(0).saturating_mul(1000),
        "trackNumber": page["page"], "isFavorite": false, "quality": "standard",
        "availability": {"status": "available"},
        "provider": {"providerId": PROVIDER, "trackId": id, "albumId": bvid, "mediaId": cid.to_string()}
    }))
}

struct Bilibili;

impl Guest for Bilibili {
    fn invoke(
        capability: String,
        operation: String,
        payload_json: String,
    ) -> Result<String, String> {
        let payload: Value = serde_json::from_str(&payload_json).map_err(|_| invalid())?;
        if capability == "provider.playback" {
            return match operation.as_str() {
                "playback.resolve" => playback::resolve(&payload).map(|v| v.to_string()),
                "playback.set-preferred-quality" => Ok(playback::status().to_string()),
                "playback.set-current-quality" => Ok(String::from("null")),
                _ => Err(error(
                    "unsupported-operation",
                    "没有其他音源可供切换",
                    false,
                )),
            };
        }
        if capability == "provider.catalog" && operation == "catalog.status" {
            return Ok(playback::status().to_string());
        }
        if capability == "provider.account" {
            return account::invoke(&operation, &payload).map(|v| v.to_string());
        }
        if capability != "provider.catalog"
            || !matches!(operation.as_str(), "catalog.album" | "catalog.song")
        {
            return Err(error(
                "unsupported-operation",
                "当前版本不支持此操作",
                false,
            ));
        }
        let id = payload["id"].as_str().ok_or_else(invalid)?;
        let (bvid, cid) = if operation == "catalog.song" {
            let (bvid, cid) = id.split_once(':').ok_or_else(invalid)?;
            (bvid, Some(cid.parse::<u64>().map_err(|_| invalid())?))
        } else {
            (id, None)
        };
        let data = video(bvid)?;
        let pages = data["pages"].as_array().ok_or_else(invalid)?;
        let multipart = pages.len() > 1;
        if let Some(cid) = cid {
            let page = pages
                .iter()
                .find(|p| p["cid"].as_u64() == Some(cid))
                .ok_or_else(invalid)?;
            return Ok(song(&data, page, multipart)?.to_string());
        }
        let tracks = pages
            .iter()
            .map(|p| song(&data, p, multipart))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({
            "id": bvid, "title": data["title"], "artist": artist(&data),
            "artwork": artwork(&data), "releaseYear": 0, "genre": "", "description": "", "tracks": tracks
        })
        .to_string())
    }
}

export!(Bilibili);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
