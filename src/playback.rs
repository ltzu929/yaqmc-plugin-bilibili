use crate::{account, api, error, invalid, yaqmc, PROVIDER};
use alloc::{
    format,
    string::{String, ToString},
};
use serde_json::{json, Value};

pub fn status() -> Value {
    json!({"providerId":PROVIDER,"profileId":"default","displayName":"B 站收藏夹","connection":"ready",
        "message":"","preferredQuality":"automatic","capabilities":{"search":false,"album":true,
        "artist":false,"playlist":false,"lyrics":false,"wordTimedLyrics":false,"streaming":true,"qualitySelection":false}})
}

fn declared_media_url<'a>(track: &'a Value, manifest: &Value) -> Option<&'a str> {
    let permissions = manifest["permissions"].as_array()?;
    [track["baseUrl"].as_str(), track["base_url"].as_str()]
        .into_iter()
        .flatten()
        .chain(
            track["backupUrl"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .chain(
            track["backup_url"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .find(|url| {
            let Some((host, _)) = url.strip_prefix("https://").and_then(|u| u.split_once('/'))
            else {
                return false;
            };
            host.ends_with(".bilivideo.com")
                && permissions.iter().any(|p| {
                    p.as_str().and_then(|p| p.strip_prefix("network:https://")) == Some(host)
                })
        })
}

pub fn resolve(payload: &Value) -> Result<Value, String> {
    let id = payload["song"]["id"].as_str().ok_or_else(invalid)?;
    let (bvid, cid) = id.split_once(':').ok_or_else(invalid)?;
    if !crate::valid_bvid(bvid) {
        return Err(invalid());
    }
    let cid = cid
        .parse::<u64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(invalid)?;
    let session = account::saved()?;
    let source = api::data(
        &format!("/x/player/playurl?bvid={bvid}&cid={cid}&fnval=16"),
        &[
            "dash",
            "audio",
            "id",
            "bandwidth",
            "codecs",
            "mimeType",
            "mime_type",
            "baseUrl",
            "base_url",
            "backupUrl",
            "backup_url",
            "is_preview",
        ],
        session.as_ref().map(account::handle).transpose()?,
    )?;
    let manifest: Value =
        serde_json::from_str(include_str!("../manifest.json")).map_err(|_| invalid())?;
    let (track, url) = source["dash"]["audio"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|track| {
            track["codecs"]
                .as_str()
                .is_some_and(|c| c.starts_with("mp4a."))
                && track["mimeType"].as_str().or(track["mime_type"].as_str()) == Some("audio/mp4")
        })
        .filter_map(|track| declared_media_url(track, &manifest).map(|url| (track, url)))
        .max_by_key(|(track, _)| track["bandwidth"].as_u64().unwrap_or(0))
        .ok_or_else(|| {
            error(
                "full-playback-unavailable",
                "当前视频未提供可播放的音轨",
                false,
            )
        })?;
    let head = yaqmc::provider::network::request(&json!({"method":"HEAD","url":url}).to_string())
        .map_err(|_| error("network", "音频连接失败", true))?;
    let response: Value = serde_json::from_str(&head).map_err(|_| invalid())?;
    match response["status"].as_u64() {
        Some(200) => {}
        Some(401 | 403) => {
            yaqmc::provider::utilities::log(
                "warn",
                &format!("Bilibili audio CDN returned HTTP {}", response["status"]),
            );
            return Err(error(
                "authorization-rejected",
                "B 站未允许播放该音频",
                false,
            ));
        }
        Some(404 | 410) => return Err(error("url-expired", "音频地址已失效", true)),
        _ => return Err(error("network", "读取音频信息失败", true)),
    }
    let length = response["contentLength"]
        .as_u64()
        .filter(|s| *s > 0)
        .ok_or_else(|| error("range-unsupported", "音源未提供文件长度", false))?;
    let preview = source["is_preview"] == 1 || source["is_preview"] == true;
    let rendition = track["id"].as_u64().ok_or_else(invalid)?;
    Ok(json!({"source":{"kind":"https","request":{"url":url}},
        "cacheKey":format!("{id}:{rendition}:{}",if preview {"preview"} else {"full"}),
        "format":"aac","mimeType":"audio/mp4","qualityLabel":"standard","bitrateKbps":track["bandwidth"].as_u64().map(|b|b/1000),
        "contentLength":length,"isPreview":preview,"selection":{"requestedQuality":"automatic",
            "resolvedQuality":"standard","preview":preview,"qualityCapabilities":[]}}))
}
