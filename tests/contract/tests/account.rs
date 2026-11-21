use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};
use wasmtime::component::{Component, Linker, TypedFunc};
use wasmtime::{Engine, Store};

#[derive(Default)]
struct State {
    values: HashMap<String, String>,
    revoked: Vec<String>,
    media_status: u16,
}

type Invoke = TypedFunc<(String, String, String), (Result<String, String>,)>;

fn wired() -> (Store<State>, Invoke) {
    let engine = Engine::default();
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-wasip2/release/yaqmc_plugin_bilibili.wasm");
    let component =
        Component::from_file(&engine, file).expect("build the plugin with python pack.py first");
    let mut linker = Linker::<State>::new(&engine);
    let mut storage = linker.instance("yaqmc:provider/storage@0.1.0").unwrap();
    storage
        .func_wrap("kv-get", |state, (key,): (String,)| {
            Ok((Ok::<_, String>(state.data().values.get(&key).cloned()),))
        })
        .unwrap();
    storage
        .func_wrap("kv-set", |mut state, (key, value): (String, String)| {
            state.data_mut().values.insert(key, value);
            Ok((Ok::<_, String>(true),))
        })
        .unwrap();
    storage
        .func_wrap("kv-delete", |mut state, (key,): (String,)| {
            Ok((Ok::<_, String>(
                state.data_mut().values.remove(&key).is_some(),
            ),))
        })
        .unwrap();
    linker
        .instance("yaqmc:provider/credentials@0.1.0")
        .unwrap()
        .func_wrap("delete", |mut state, (h,): (String,)| {
            state.data_mut().revoked.push(h);
            Ok((Ok::<_, String>(true),))
        })
        .unwrap();
    let mut utilities = linker.instance("yaqmc:provider/utilities@0.1.0").unwrap();
    utilities
        .func_wrap("unix-millis", |_, (): ()| Ok((123456u64,)))
        .unwrap();
    utilities
        .func_wrap("log", |_, (_level, _message): (String, String)| Ok(()))
        .unwrap();
    linker.instance("yaqmc:provider/network@0.1.0").unwrap().func_wrap("request",|state,(raw,):(String,)|{
        let req:Value=serde_json::from_str(&raw)?;
        if req["method"]=="HEAD" {
            assert_eq!(req["url"],"https://upos-sz-mirrorcos.bilivideo.com/test.m4s");
            assert!(req.get("credentialHeaders").is_none());
            return Ok((Ok::<_,String>(json!({"status":state.data().media_status,"contentLength":8000}).to_string()),));
        }
        let url=req["url"].as_str().unwrap().strip_prefix("https://api.bilibili.com").unwrap();
        assert_eq!(req["credentialHeaders"],json!([{"name":"cookie","handle":"test-handle"}]));
        let body=if url=="/x/web-interface/nav" {
            json!({"isLogin":true,"mid":123,"uname":"测试用户","face":""})
        } else if url=="/x/v3/fav/folder/created/list?up_mid=123&pn=1&ps=20" {
            json!({"count":1,"list":[{"id":42,"mid":123,"title":"翻唱","cover":"http://i1.hdslb.com/folder.jpg","media_count":2}],"has_more":false})
        } else if url=="/x/v3/fav/resource/list?media_id=42&pn=1&ps=20" {
            json!({"info":{"id":42,"mid":123,"title":"翻唱","media_count":2},"has_more":false,
                "medias":[{"type":2,"bvid":"BV1234567890","page":2},{"type":2,"bvid":"BV9876543210","page":1,
                "title":"单 P","cover":"","upper":{"mid":7,"name":"UP"},"ugc":{"first_cid":303},"duration":15}]})
        } else if url=="/x/web-interface/view?bvid=BV1234567890" {
            json!({"bvid":"BV1234567890","title":"现场","owner":{"mid":7,"name":"UP"},"pic":"",
                "pages":[{"cid":101,"page":1,"part":"开场","duration":20},{"cid":202,"page":2,"part":"返场","duration":30}]})
        } else if url=="/x/player/playurl?bvid=BV1234567890&cid=202&fnval=16" {
            json!({"dash":{"audio":[{"id":30232,"mimeType":"audio/mp4","codecs":"mp4a.40.2","bandwidth":128000,
                "baseUrl":"https://undeclared.invalid/ignored","backupUrl":["https://upos-sz-mirrorcos.bilivideo.com/test.m4s"]}]}})
        } else {panic!("unexpected URL {url}")};
        Ok((Ok::<_,String>(json!({"ok":true,"bodyJson":{"code":0,"data":body}}).to_string()),))
    }).unwrap();
    let mut store = Store::new(&engine, State::default());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let invoke = instance.get_typed_func(&mut store, "invoke").unwrap();
    (store, invoke)
}

fn call(
    store: &mut Store<State>,
    invoke: &Invoke,
    op: &str,
    payload: Value,
) -> Result<Value, Value> {
    let capability = format!("provider.{}", op.split('.').next().unwrap());
    let (result,) = invoke
        .call(&mut *store, (capability, op.into(), payload.to_string()))
        .unwrap();
    invoke.post_return(store).unwrap();
    result
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .map_err(|raw| serde_json::from_str(&raw).unwrap())
}

#[test]
fn login_read_multipart_across_pages_and_sign_out() {
    let (mut store, invoke) = wired();
    assert_eq!(
        call(&mut store, &invoke, "account.snapshot", json!({})).unwrap()["state"],
        "guest"
    );
    assert_eq!(
        call(&mut store, &invoke, "account.playlists", json!({})).unwrap_err()["code"],
        "authentication-expired"
    );
    let prep = call(
        &mut store,
        &invoke,
        "account.auth.prepare-oauth",
        json!({"loginProvider":"bilibili"}),
    )
    .unwrap();
    assert_eq!(
        prep["browserSession"]["credentialOrigins"],
        json!(["https://api.bilibili.com"])
    );
    let auth=call(&mut store,&invoke,"account.auth.complete-oauth",json!({"browserCredentials":[{"origin":"https://api.bilibili.com","handle":"test-handle"}]})).unwrap();
    assert_eq!(auth["state"], "authenticated");
    let folders = call(
        &mut store,
        &invoke,
        "account.playlists",
        json!({"limit":50}),
    )
    .unwrap();
    let folder = &folders["items"][0];
    assert_eq!(folder["artwork"]["src"], "https://i1.hdslb.com/folder.jpg");
    assert_eq!(folder["reference"]["tid"], "42");
    let mut cursor = Value::Null;
    for (i, id) in ["BV1234567890:101", "BV1234567890:202", "BV9876543210:303"]
        .into_iter()
        .enumerate()
    {
        let result = call(
            &mut store,
            &invoke,
            "account.playlist-tracks",
            json!({"limit":1,"playlist":folder,"cursor":cursor}),
        )
        .unwrap();
        assert_eq!(result["tracks"]["items"][0]["id"], id);
        cursor = result["tracks"]["nextCursor"].clone();
        assert_eq!(cursor.is_null(), i == 2);
    }
    let song = call(
        &mut store,
        &invoke,
        "catalog.song",
        json!({"id":"BV1234567890:202"}),
    )
    .unwrap();
    assert_eq!(song["title"], "返场");
    store.data_mut().media_status = 200;
    let source = call(
        &mut store,
        &invoke,
        "playback.resolve",
        json!({"song":song}),
    )
    .unwrap();
    assert_eq!(source["contentLength"], 8000);
    assert_eq!(
        source["source"]["request"]["url"],
        "https://upos-sz-mirrorcos.bilivideo.com/test.m4s"
    );
    store.data_mut().media_status = 403;
    assert_eq!(
        call(
            &mut store,
            &invoke,
            "playback.resolve",
            json!({"song":song})
        )
        .unwrap_err()["code"],
        "authorization-rejected"
    );
    assert_eq!(
        call(&mut store, &invoke, "account.sign-out", json!({})).unwrap()["state"],
        "guest"
    );
    assert_eq!(store.data().revoked, ["test-handle"]);
}
