# YAQMC B 站收藏夹插件

在 YAQMC 的媒体库里选择 B 站收藏夹，把翻唱、现场和想听的分 P 加入同一个播放队列。

当前测试版为 `0.1.0-alpha.2`。YAQMC Android 从 0.1.7 起支持插件安装，选择收藏夹、记住常听分 P 和打开原视频需要配套的 Android 0.2.0-beta.1 测试客户端。模拟器验证了分 P 选择和队列顺序。游客试听时音频服务器返回 HTTP 403，登录后的实际播放仍待验证。

## 使用

1. 在「设置 → 插件 → 插件商店」搜索 `bilibili`，打开 `ltzu929/yaqmc-plugin-bilibili`，勾选「包含预览版本」，选择版本并安装、启用。也可以从 [GitHub Releases](https://github.com/ltzu929/yaqmc-plugin-bilibili/releases) 下载 `.yaqmc-plugin` 文件后手动导入。
2. 在媒体库选择「B 站收藏夹」，进入账号设置，在哔哩哔哩页面完成登录。
3. 打开「视频收藏夹 → 选择收藏夹」，勾选想听的收藏夹。这里的选择只保存到本机。
4. 点开视频后，可以播放整场，也可以勾选分 P 后保存或播放。收藏夹里的「播放全部」会采用保存的选择。曲目的更多菜单可以打开对应的原视频。

## 本地构建

安装 Python 3 和 Rust 工具链后，在仓库根目录运行：

```sh
rustup target add wasm32-wasip2
python pack.py
```

产物位于 `dist/`。在 YAQMC 的「设置 → 插件」选择本地文件，可检查安装包及其所需权限。

构建完成后，可运行 `cargo test --locked --manifest-path tests/contract/Cargo.toml`。测试使用打包所用的 Wasm 组件和模拟平台响应，检查登录、分页、分 P 身份、音源选择、平台拒绝及退出账号。
