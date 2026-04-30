# Image Draw AI

基于 Tauri 2 + Rust + React + SQLite 的跨平台图形 AI 工具骨架，目标兼容 Windows 和 macOS，并预留 OpenAI 以及 OpenAI-compatible 中转站接入。

## 当前能力

- Tauri 2 桌面应用骨架
- React + Vite 前端界面
- Rust 后端 AppState
- SQLite 本地数据库初始化
- Provider 配置表，支持 `base_url` / 模型名 / 中转站配置
- 生成任务表，先保存任务历史
- OpenAI-compatible Provider 抽象，预留 `/images/generations` 调用

## 开发命令

```bash
pnpm install
pnpm tauri:dev
```

生成当前系统安装包：

```bash
pnpm build
```

macOS 打包：

```bash
pnpm build:mac
```

macOS Universal 打包：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm build:mac:universal
```

Windows 打包：

```bash
pnpm build:win
```

只构建前端产物：

```bash
pnpm web:build
```

只检查 Rust 后端：

```bash
cd src-tauri
cargo check
```

## 跨平台打包

Tauri 打包通常需要在目标系统上构建：

- macOS 安装包：在 macOS 上运行 `pnpm build`，输出 `.app` / `.dmg`
- Windows 安装包：在 Windows 上运行 `pnpm build`，输出 `.msi` / `.exe`

后续可以用 GitHub Actions 分别跑 `macos-latest` 和 `windows-latest`，自动产出两个平台的安装包。

## 数据位置

SQLite 数据库会创建在 Tauri 的 `app_data_dir` 下：

```txt
image_draw_ai.sqlite
```

图片文件后续建议保存到同一目录下的 `images/` 子目录，数据库只保存图片路径和元数据。

## 中转站配置

Provider 设计支持 OpenAI-compatible 中转站：

```json
{
  "kind": "openai-compatible",
  "base_url": "https://api.openai.com/v1",
  "text_model": "gpt-5",
  "image_model": "gpt-image-2"
}
```

后续可以在 UI 中把 `base_url`、`api_key`、模型名做成可编辑配置。
