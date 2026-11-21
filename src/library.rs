use crate::{account, api, invalid, song, video, yaqmc::provider::utilities, PROVIDER};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use serde_json::{json, Value};

fn page(items: Vec<Value>, cursor: Option<String>, total: Option<u64>) -> Value {
    json!({"items":items,"nextCursor":cursor,"total":total,"fetchedAtMs":utilities::unix_millis(),
        "stale":false,"authRevision":0})
}

fn summary(folder: &Value, user: &Value) -> Result<Value, String> {
    let id = folder["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(invalid)?
        .to_string();
    let title = folder["title"].as_str().ok_or_else(invalid)?;
    Ok(json!({"providerId":PROVIDER,"profileId":"default","id":id,
        "reference":{"kind":"owned","tid":id},"ownership":"owned",
        "title":title,"description":folder["intro"].as_str().unwrap_or(""),
        "owner":{"id":user["mid"].to_string(),"displayName":user["name"]},
        "artwork":{"src":crate::cover_url(&folder["cover"]),"alt":title,"dominantColor":"#475569"},
        "capabilities":{"canAddTracks":false,"canRemoveTracks":false,"canRename":false,"canDelete":false,"canReorder":false},
        "trackCount":folder["media_count"].as_u64().unwrap_or(0),
        "updatedAtMs":folder["mtime"].as_u64().map(|t|t.saturating_mul(1000))}))
}

pub fn folders(payload: &Value, user: &Value) -> Result<Value, String> {
    let pn: u32 = payload["cursor"]
        .as_str()
        .unwrap_or("1")
        .parse()
        .map_err(|_| invalid())?;
    if pn == 0 {
        return Err(invalid());
    }
    let size = payload["limit"].as_u64().unwrap_or(20).clamp(1, 20);
    let uid = user["mid"].as_u64().ok_or_else(invalid)?;
    let data = api::data(
        &format!("/x/v3/fav/folder/created/list?up_mid={uid}&pn={pn}&ps={size}"),
        &[
            "count",
            "list",
            "has_more",
            "id",
            "mid",
            "title",
            "intro",
            "cover",
            "media_count",
            "mtime",
        ],
        Some(account::handle(user)?),
    )?;
    let folders = data["list"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| summary(f, user))
        .collect::<Result<Vec<_>, _>>()?;
    let next = if data["has_more"] == true && !folders.is_empty() {
        Some(pn.checked_add(1).ok_or_else(invalid)?.to_string())
    } else {
        None
    };
    Ok(page(folders, next, data["count"].as_u64()))
}

// Cursor identifies a remote page, a video within it and a part within that video.
// Remote page size remains fixed even when the host changes its requested track count.
fn position(cursor: Option<&str>) -> Result<(u32, usize, usize), String> {
    let Some(cursor) = cursor else {
        return Ok((1, 0, 0));
    };
    let mut fields = cursor.split('.');
    let pn = fields
        .next()
        .and_then(|s| s.parse().ok())
        .filter(|p| *p > 0)
        .ok_or_else(invalid)?;
    let row = fields
        .next()
        .and_then(|s| s.parse().ok())
        .filter(|v| *v < 20)
        .ok_or_else(invalid)?;
    let part = fields
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(invalid)?;
    if fields.next().is_some() {
        return Err(invalid());
    }
    Ok((pn, row, part))
}

pub fn tracks(payload: &Value, user: &Value) -> Result<Value, String> {
    let folder = payload["playlist"]["id"]
        .as_str()
        .ok_or_else(invalid)?
        .parse::<u64>()
        .map_err(|_| invalid())?;
    let limit = payload["limit"].as_u64().unwrap_or(20).clamp(1, 50) as usize;
    let (pn, mut row, mut part) = position(payload["cursor"].as_str())?;
    let data = api::data(
        &format!("/x/v3/fav/resource/list?media_id={folder}&pn={pn}&ps=20"),
        &[
            "info",
            "id",
            "mid",
            "title",
            "intro",
            "cover",
            "mtime",
            "media_count",
            "medias",
            "type",
            "bvid",
            "attr",
            "page",
            "upper",
            "name",
            "ugc",
            "first_cid",
            "duration",
            "has_more",
        ],
        Some(account::handle(user)?),
    )?;
    let info = &data["info"];
    if info["id"].as_u64() != Some(folder) || info["mid"] != user["mid"] {
        return Err(invalid());
    }
    let media = data["medias"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut output = Vec::new();
    let mut loaded_details = false;
    while row < media.len() && output.len() < limit {
        let item = &media[row];
        if item["type"] != 2
            || item["attr"].as_u64().unwrap_or(0) & 8 != 0
            || item["bvid"].as_str().is_none_or(str::is_empty)
        {
            row += 1;
            part = 0;
            continue;
        }
        let bvid = item["bvid"].as_str().ok_or_else(invalid)?;
        if item["page"] == 1 && item["ugc"]["first_cid"].as_u64().is_some_and(|c| c > 0) {
            let video = json!({"bvid":bvid,"title":item["title"],"pic":item["cover"],"owner":item["upper"]});
            let first =
                json!({"cid":item["ugc"]["first_cid"],"page":1,"duration":item["duration"]});
            output.push(song(&video, &first, false)?);
        } else {
            if loaded_details {
                break;
            }
            loaded_details = true;
            let details = video(bvid)?;
            let parts = details["pages"].as_array().ok_or_else(invalid)?;
            while part < parts.len() && output.len() < limit {
                output.push(song(&details, &parts[part], parts.len() > 1)?);
                part += 1;
            }
            if part < parts.len() {
                break;
            }
        }
        row += 1;
        part = 0;
    }
    let next = if row < media.len() {
        Some(format!("{pn}.{row}.{part}"))
    } else if data["has_more"] == true && !media.is_empty() {
        Some(format!("{}.0.0", pn.checked_add(1).ok_or_else(invalid)?))
    } else {
        None
    };
    Ok(json!({"summary":summary(info,user)?,"tracks":page(output,next,None)}))
}
