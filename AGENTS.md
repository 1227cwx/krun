# KRun 项目约定

## 提交规则

**每次修改都必须提交到 GitHub。** 这是硬性要求，不依赖用户每次提醒。

完成任何代码、资源或文档改动后：

1. 运行质量检查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
2. 暂存改动并创建提交，提交信息使用清晰的英文 `type: summary` 格式，例如：
   - `feat: add compact search bar`
   - `fix: reset search state on hide`
   - `style: align settings controls`
3. 推送到远程 `main`：
   ```bash
   git push origin main
   ```
4. 确认 `git status` 干净，且本地 `HEAD` 与 `git ls-remote origin refs/heads/main` 一致。

如果一次任务包含多个独立改动，可以拆成多个提交，但**不允许把改动留在工作区不提交**。

## 分支

远程仓库：`git@github.com:1227cwx/krun.git`，默认分支 `main`。

## 发布产物

- 最终二进制：`release/KRun.exe`，由 `build-release.ps1` 生成。
- `release/KRun.exe` 与 `release/launcher.json` 不进入 Git 历史，只通过 GitHub Releases 发布。
- 不要把本机 `launcher.json`、`target/`、`.zcode/` 或测试脚本提交进仓库。

## 架构约定

- 纯 Rust + Win32/GDI，不引入 WebView、Electron、WinUI 3 或大型 GUI 运行时。
- 用户可见文案使用中文，代码标识符与提交信息使用英文。
- 新增 UI 必须同时更新：`layout.rs`（几何与命中）、`render.rs`（绘制）、对应交互测试。
- 配置始终保存在 EXE 同目录的 `launcher.json`，通过临时文件 + `MoveFileExW` 原子替换。
