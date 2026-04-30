# Image Draw AI

Image Draw AI 是一个跨平台桌面端 AI 图像生成工具，支持多个图像模型供应商、参考图编辑、本地图库管理和 GitHub Releases 更新检测。

作者：ctexthuang

## 功能特性

- 文生图：输入提示词后调用已配置的图像模型生成图片。
- 图像编辑：导入参考图后，使用支持编辑能力的模型进行改图。
- 多供应商配置：支持 OpenAI、OpenAI-compatible 中转站、火山方舟 Seedream、阿里云百炼通义万相、腾讯混元图像、Google Gemini / Nano Banana。
- 模型列表管理：从供应商接口获取图片模型，选择后保存到本地，下次启动可直接使用。
- 生成参数：支持比例、分辨率、质量配置，分辨率最高可配置到 4K。
- 本地图库：生成图片保存到本地目录，可自定义存储位置并迁移旧图片。
- 强制停止：生成过程中可以中断当前任务。
- 软件更新：启动时检查 GitHub Releases，发现新版本时提示下载。
- 本地存储：配置、任务记录和图片元数据保存在本机 SQLite 数据库中。

## 下载安装

在 GitHub Releases 页面下载对应平台的安装包：

- macOS Apple Silicon：下载 `.dmg` 后安装。
- Windows x64：下载 Windows 安装包后运行。

macOS 如果提示来自未验证开发者，可以在系统设置的隐私与安全性中允许打开。

## 快速开始

1. 打开应用，点击左侧底部的「设置」。
2. 在「API 分类」中选择供应商。
3. 填写配置 ID、名称、Base URL 和 API Key。
4. 点击「获取模型」，等待模型列表返回。
5. 勾选要使用的图片模型。
6. 点击「保存配置」。
7. 回到主界面，输入提示词，选择模型、比例、分辨率和质量。
8. 点击「开始生成」。

如果需要图像编辑，先点击左侧「素材」导入参考图，再开始生成。

## 供应商配置

不同供应商的 Base URL 格式不同：

| API 分类 | Base URL 示例 | API Key |
| --- | --- | --- |
| OpenAI 官方 | `https://api.openai.com/v1` | OpenAI API Key |
| OpenAI-compatible / 中转站 | `https://your-api.example.com/v1` | 中转站 Key |
| 火山方舟 / Seedream | `https://ark.cn-beijing.volces.com/api/v3` | 火山方舟 Key |
| 阿里云百炼 / 通义万相 | `https://dashscope.aliyuncs.com/api/v1` | DashScope Key |
| 腾讯混元图像 | `https://aiart.tencentcloudapi.com` | `SecretId:SecretKey` |
| Google Gemini / Nano Banana | `https://generativelanguage.googleapis.com/v1beta` | Google AI Studio API Key |

切换 API 分类时，表单会切换到对应示例配置。只有点击「保存配置」后，本地数据才会被覆盖。

## 生成参数

- 图像模型：来自设置页保存的图片模型列表。
- 质量：`auto`、`high`、`medium`、`low`。
- 比例：包含 `1:1`、`1:2`、`2:1`、`9:16`、`16:9`、`3:4`、`4:3`、名片横版、名片竖版。
- 分辨率：会根据比例联动切换，最高提供 4K 规格。

部分供应商可能不支持所有质量、比例或分辨率。如果接口返回错误，请降低分辨率、切换质量，或换一个模型重试。

## 图库与文件

左侧「图库」可以打开图库设置弹窗：

- 查看当前图片保存目录。
- 选择新的保存目录。
- 自动迁移旧目录中的已生成图片。
- 打开当前保存目录。

更新软件不会改变自定义图库目录。

## 软件更新

应用启动时会自动检查 GitHub Releases：

- 有新版本时会弹出更新窗口。
- 没有新版本时不会打扰。
- 关闭更新窗口会停止本次启动期间的自动提示。
- 左侧「更新」按钮可以手动检查和打开下载页。

## 本地开发

依赖环境：

- Node.js
- pnpm
- Rust
- Tauri 2 相关系统依赖

安装依赖：

```bash
pnpm install
```

启动开发环境：

```bash
pnpm tauri:dev
```

只构建前端：

```bash
pnpm web:build
```

检查 Rust 后端：

```bash
cd src-tauri
cargo check --locked
```

构建当前平台安装包：

```bash
pnpm build
```

macOS 打包：

```bash
pnpm build:mac
```

Windows 打包：

```bash
pnpm build:win
```

## 技术栈

- Tauri 2
- Rust
- React
- TypeScript
- Vite
- SQLite

## 数据说明

应用数据保存在 Tauri 的 `app_data_dir` 下，包括：

- SQLite 数据库
- 供应商配置
- 任务记录
- 图片元数据
- 默认图片输出目录
- 自定义图库目录设置

API Key 当前保存在本地数据库中，请只在可信设备上使用。

## License

尚未指定开源协议。发布或分发前建议补充 `LICENSE` 文件。
