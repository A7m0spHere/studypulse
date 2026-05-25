# StudyPulse

> A lightweight Windows study dashboard with Pomodoro, local activity tracking, daily reports, and AI summaries.

作者使用 Codex vibe coding。

StudyPulse 是一个轻量级 Windows 桌面学习辅助工具。用户点击“开始学习”后，程序会在本机记录学习会话、番茄钟状态、前台窗口、应用使用时长和键鼠活跃度，并在结束后生成本地日报。用户可以配置 DeepSeek 或自定义 OpenAI 兼容 API，让 AI 对日报进行复盘总结和追问聊天。

## 功能

- 学习会话开始、结束和本次计时
- 番茄钟，支持预设和自定义时长
- Windows 前台窗口记录
- 应用使用时长排行
- 键鼠活跃度计数
- 本地日报与历史日报
- AI 总结和聊天追问
- DeepSeek 与自定义 OpenAI 兼容 API 配置

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

StudyPulse 默认把数据保存在本机 SQLite 数据库中。

- 不记录具体按键内容
- 不记录输入文本
- 不保存鼠标坐标
- 不截图、不录屏
- 不主动上传本地采集数据
- 只有用户主动生成 AI 总结或发送 AI 聊天消息时，才会把日报摘要发送到当前配置的 AI API

## 环境变量

普通使用不需要环境变量。AI 供应商请在桌面应用的设置页中配置，不要把真实 API Key 写入仓库。

## 安装运行

安装依赖：

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

## 打包

生成 Windows MSI：

```powershell
npm run tauri build -- --bundles msi
```

安装包通常输出到：

```text
src-tauri/target/release/bundle/msi/
```

## 版本发布

建议流程：

1. 日常开发使用 `git commit` 提交。
2. 正式版本使用 Git tag，例如 `git tag v0.2.2`。
3. 推送代码和 tag：`git push origin main --tags`。
4. 在 GitHub Release 中上传 MSI 或 ZIP 安装包。
