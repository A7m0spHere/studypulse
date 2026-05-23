# StudyPulse

> A lightweight Windows study dashboard with Pomodoro, local activity tracking, daily reports, and AI summaries.

StudyPulse 是一个轻量级 Windows 桌面学习辅助工具。用户点击“开始学习”后，程序会在本机记录学习会话、番茄钟状态、前台窗口、应用使用时长和键鼠活跃度，并在结束后生成本地日报。用户可以配置 OpenAI 兼容 API，让 AI 对日报进行复盘总结和追问聊天。

## 项目简介

StudyPulse 面向 PC 桌面学习场景，重点是“本地记录 + 简单复盘”。当前版本主要支持 Windows：

- 学习会话开始/结束
- 番茄钟计时
- Windows 前台窗口记录
- 应用使用时长统计
- 键鼠活跃度计数
- 本地日报与历史日报
- AI 总结与聊天追问
- OpenAI 兼容 API 配置

## 技术栈

- Tauri 2
- React
- TypeScript
- Vite
- TailwindCSS
- Rust
- SQLite
- Recharts

## 隐私说明

StudyPulse 默认本地保存数据。

- 不记录具体按键内容
- 不记录输入文本
- 不保存鼠标坐标
- 不截图、不录屏
- 不主动上传本地采集数据
- 只有用户主动生成 AI 总结或发送 AI 聊天消息时，才会把日报摘要发送给当前配置的 AI API

## 环境变量

复制 `.env.example` 并按需创建本地 `.env`。公开仓库不应提交真实 API key。

当前可选环境变量：

```env
STUDYPULSE_BUILTIN_API_KEY=
```

PowerShell 临时设置示例：

```powershell
$env:STUDYPULSE_BUILTIN_API_KEY="your-api-key"
```

## 安装运行方式

首次安装依赖：

```powershell
npm install
```

启动 Tauri 桌面开发环境：

```powershell
npm run tauri dev
```

## 开发命令

前端测试：

```powershell
npm test
```

前端构建：

```powershell
npm run build
```

Rust 检查和测试：

```powershell
cd src-tauri
cargo check
cargo test
```

## 打包命令

生成 Windows MSI：

```powershell
npm run tauri build -- --bundles msi
```

生成包含 MSI、测试证书和中文使用手册的发布 ZIP：

```powershell
npm run package:windows -- -Version 0.2.1
```

发布产物会生成在 `release/` 目录。`release/` 不提交到 Git，正式安装包建议上传到 GitHub Release。

## 版本发布说明

开发提交使用普通 Git commit：

```powershell
git add .
git commit -m "feat: describe your change"
```

正式版本使用 Git tag：

```powershell
git tag v0.2.1
git push origin main
git push origin v0.2.1
```

GitHub Release 建议流程：

1. 确认 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 版本一致。
2. 运行测试和构建。
3. 生成 MSI 和发布 ZIP。
4. 创建 Git tag，例如 `v0.2.1`。
5. 在 GitHub Releases 中选择对应 tag。
6. 上传 `release/` 中生成的安装 ZIP 或 MSI。
7. 在 Release Notes 中说明新增功能、修复内容和已知问题。

## GitHub 仓库信息建议

- Repository name: `studypulse`
- Description: `A lightweight Windows study dashboard with Pomodoro, local activity tracking, daily reports, and AI summaries.`
- Topics: `tauri`, `react`, `typescript`, `rust`, `sqlite`, `pomodoro`, `productivity`, `study-tracker`, `windows`, `ai`

## 当前版本

当前版本：`0.2.1`

这是一个可运行 MVP，适合课程展示、同学试用和继续迭代。它不是商业级复杂监控系统，也不会做隐私侵犯型采集。
