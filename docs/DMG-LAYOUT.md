# DMG Finder 安装窗口布局方案

本文档定义 Gold Ticker 后续 R11 的 DMG 布局实现方案。它描述的是拖拽安装型 DMG 的 Finder 窗口，不是带有“下一步”页面的 `.pkg` 安装向导。

## 当前行为与问题

当前 [packaging/build-app.sh](../packaging/build-app.sh) 将 `GoldTicker.app` 和 `/Applications` 符号链接复制到 staging 目录，然后由固定版本的 `dmgbuild` 直接生成带有 Finder 布局元数据的最终 DMG：

```bash
hdiutil create -volname "Gold Ticker" -srcfolder "$staging_dir" -format UDZO -ov "$dmg_path"
```

`-volname` 只设置 DMG 挂载后的卷名称。它不会设置 Finder 窗口大小、窗口背景、图标坐标、图标尺寸或视图模式。

`dmgbuild` 在生成过程中创建可写镜像、写入背景和 `.DS_Store` 布局数据，再输出最终的 `UDZO` 镜像。构建脚本随后只读挂载最终 DMG，验证内容、Applications 链接和 Finder 元数据。

## 目标体验

Gold Ticker 继续使用 macOS 常见的拖拽安装方式：用户打开 DMG 后，将应用拖到 Applications。目标窗口参数如下：

| 项目 | 目标 |
| --- | --- |
| 卷标 | `Gold Ticker` |
| Finder 视图 | `icon view`，手动排列 |
| 窗口尺寸 | 640 × 420 |
| `GoldTicker.app` 位置 | 约 `{180, 220}` |
| `Applications` 位置 | 约 `{460, 220}` |
| 图标尺寸 | 112 |
| 背景 | 可选的固定尺寸 PNG，建议 640 × 420 |

`.background` 目录仅用于保存背景资源，必须在 Finder 中隐藏。DMG 不应加入 README、免责声明、缓存、诊断日志或其他会分散安装操作的文件；发布说明仍由 GitHub Release 和仓库文档提供。

## 构建实现

构建脚本在本地和 `macos-14` runner 的临时虚拟环境中安装固定版本 `dmgbuild==1.6.5`。`packaging/dmg-settings.py` 固定卷标、窗口、图标大小和坐标；`packaging/dmg-background.png` 提供位于两个图标正中间的拖拽箭头。最终镜像会重新只读挂载检查，避免只验证中间文件。

## 推荐构建链

R11 已由 [packaging/build-app.sh](../packaging/build-app.sh) 使用声明式 `dmgbuild` 配置实现。它保留现有 app bundle、ad-hoc 签名、Info.plist、DMG 校验和和发布文件名逻辑，并在同一套配置中生成最终 DMG：

```text
staging directory
        ↓
临时虚拟环境安装固定版本 dmgbuild
        ↓
dmgbuild 创建最终 UDZO 镜像并写入背景、窗口和图标布局
        ↓
对最终 DMG 执行 hdiutil verify
        ↓
只读挂载最终 DMG，验证内容和布局元数据
        ↓
生成 SHA-256 sidecar
```

不再需要单独的 Finder AppleScript；背景源是版本控制中的 SVG，并在构建时生成固定尺寸的 `packaging/dmg-background.png`。

构建脚本使用 shell 的 `trap` 清理 staging 目录和临时虚拟环境。`dmgbuild` 自己负责临时镜像的挂载、布局数据写入和卸载。

## Finder 布局配置

`dmgbuild` 写入以下 Finder 数据：窗口约 640 × 420、手动 icon view、图标尺寸 112、`GoldTicker.app` 与 `Applications` 的固定坐标，以及位于两者之间的背景箭头。

## GitHub Actions 适配

现有 [.github/workflows/release.yml](../.github/workflows/release.yml) 使用 `macos-14`。R11 应继续由 `./packaging/build-app.sh` 产生发布文件，workflow 只需补充最终镜像检查，不应在 CI 中另写一套布局逻辑。

CI 至少应验证：

- `hdiutil verify` 对最终 DMG 成功；
- 最终 DMG 能以只读方式挂载；
- 挂载根目录包含 `GoldTicker.app`；
- `Applications` 是指向 `/Applications` 的符号链接；
- 不存在缓存、诊断日志、秘密或意外的 staging 文件；
- 最终 DMG 中存在 Finder 布局元数据或背景资源，且不是只验证临时镜像；
- SHA-256 sidecar 与最终 DMG 完全匹配。

构建脚本会把 AppleScript/Finder 图形会话作为已移除的旧依赖，不需要 runner 提供交互式 Finder 会话；最终的文件和 `.DS_Store` 检查仍不能完全替代真实 Finder 的视觉检查，因此仍需在登录的 macOS 桌面上打开最终 DMG，确认窗口尺寸、背景、图标位置和图标尺寸。

## 排错清单

当布局“写入成功但打开后没有变化”时，按以下顺序检查：

1. 确认实际打开的是本次构建产生的最终 DMG，而不是旧挂载卷或 Finder 缓存的窗口。
2. 确认 `dmgbuild` 配置引用了正确的背景资源和 staging 目录。
3. 确认最终镜像已经重新只读挂载检查，而不是只检查构建过程中的临时文件。
4. 弹出所有旧的同名卷后，再从 Finder 打开新的 DMG。
5. 检查背景文件路径、PNG 尺寸和隐藏状态。

## 为什么不使用 `.pkg` 或第三方工具

`.pkg` 会引入权限、安装位置、安装脚本、包签名和公证等额外问题。Gold Ticker 是单个 Menu Bar 应用，拖拽到 Applications 更符合当前产品和发布文档。

第三方 `create-dmg` 可以封装类似流程，但会增加外部工具安装、版本锁定、供应链审计和 GitHub Actions 维护成本。当前项目优先使用 macOS 自带的 `hdiutil`、Finder 和 `osascript`；只有原生方案在固定 runner 上无法稳定执行时，才重新评估固定版本的第三方工具。

## R11 完成标准

R11 完成前不得将 roadmap 标记为 `Completed`。完成时必须同时满足：

- 文档方案已由构建脚本和资源实际实现；
- `git diff --check`、`cargo fmt --check`、`cargo test`、`cargo clippy --all-targets -- -D warnings` 和 `cargo build --release` 通过；
- GitHub Actions 的 `macos-14` 构建成功；
- 最终 DMG 通过 `hdiutil verify`、只读挂载检查和 SHA-256 验证；
- 本地登录的 macOS Finder 视觉验收确认窗口、背景、图标位置和尺寸；
- 仍保留拖拽到 Applications 的安装模型，没有添加 Gatekeeper 绕过指引。
