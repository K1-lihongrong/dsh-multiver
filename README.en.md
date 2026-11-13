<h1 align="center">
  <img src="docs/assets/readme/hero.png" alt="dsh-multiver" width="680">
</h1>

<p align="center"><strong>A lightweight, portable version manager for DeepSeek Harness</strong><br>
One home can serve multiple versions; one version can run multiple isolated instances.</p>

<p align="center"><a href="README.md">简体中文</a> | <strong>English</strong></p>

<p align="center">
  <a href="https://github.com/K1-lihongrong/dsh-multiver/releases"><img alt="Release" src="https://img.shields.io/github/v/release/K1-lihongrong/dsh-multiver?style=flat-square&color=4F6EF7"></a>
  <img alt="Windows 10+" src="https://img.shields.io/badge/Windows-10%2B-4F6EF7?style=flat-square&logo=windows&logoColor=white">
  <a href="https://github.com/K1-lihongrong/dsh-multiver/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/K1-lihongrong/dsh-multiver/total?style=flat-square&color=30A46C"></a>
  <a href="https://github.com/K1-lihongrong/dsh-multiver/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/K1-lihongrong/dsh-multiver?style=flat-square&color=F5C542"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-8B5CF6?style=flat-square"></a>
</p>

<p align="center">
  <a href="#download">Download</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/使用手册.md">Manual (Chinese)</a> ·
  <a href="docs/插件兼容矩阵.md">Plugin compatibility</a> ·
  <a href="docs/问题日志查看指南.md">Troubleshooting</a>
</p>

dsh-multiver is a lightweight, portable manager for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). It installs and switches versions, runs each version in an embedded window, and manages isolated instances that can share data across versions. A headless CLI covers the same operations for scripts and automation.

## Highlights

- 🪶 **Small and portable**: one Windows exe, about 10 MB; no installer, no bundled runtime.
- 💾 **Version management**: install, switch, set default and batch uninstall; pnpm store hardlinks keep disk usage down.
- 🧬 **Multiple instances**: one home can be mounted to several versions; one version can run several isolated windows.
- 🖥️ **Embedded runtime**: open dsh inside the manager with a dedicated WebView2 cache, or hand it to the system browser.
- 🧩 **Plugin management**: inspect, install, remove and update community plugins, including instance homes and compatibility hints.
- ⌨️ **CLI automation**: 24 commands for versions, instances, run, export/import and maintenance, with `--json` and `--dry-run`.

## Download

Windows 10/11, x64:

| File | Notes |
| :--- | :--- |
| [dsh-multiver.exe](https://github.com/K1-lihongrong/dsh-multiver/releases/latest) | Portable; place it in a **normal writable folder** and run it. |

Do not place the exe in a DSH workspace, `C:\Program Files` or a read-only folder: WebView2 and the updater need to create and rename temporary files there.

> Linux is not packaged or released. Linux CI only compiles and runs tests.

## Quick start

1. Download `dsh-multiver.exe` and put it in a writable folder.
2. Start it; the environment check will report Node.js, pnpm, disk and registry status.
3. Install a dsh version, then click **Run**. Use the instance menu next to **Run** to create or select an isolated instance.
4. Use **Instance Management** to see all instances, including orphaned homes left by uninstalled versions.

The default data root is next to the exe. It contains `versions/`, `home/`, `homes/<id>/`, `store/` and `logs/`.

## CLI

```console
dsh-multiver.exe list --json
dsh-multiver.exe install 0.2.2-rc.1
dsh-multiver.exe instance create --name compare --attach 0.2.2-rc.1
dsh-multiver.exe run 0.2.2-rc.1 --instance <id>
dsh-multiver.exe doctor
```

Run `dsh-multiver.exe --help` for the full command list. `--json` is the machine-readable output; `--dry-run` previews destructive commands.

## Documentation

- [User manual (Chinese)](docs/使用手册.md)
- [CLI migration guide for v0.4.0 (Chinese)](docs/CLI迁移指南-v0.4.0.md)
- [Plugin compatibility matrix (Chinese)](docs/插件兼容矩阵.md)
- [Troubleshooting and logs (Chinese)](docs/问题日志查看指南.md)
- [Release notes](RELEASE_NOTES.md) · [Changelog](CHANGELOG.md)

## Known limitations

- Windows only for packaged releases; Linux is compile/test only.
- The exe folder must be writable (see Download).
- Very old dsh versions may fail to start because of an upstream Cordis HMR defect; see the compatibility matrix.
- The GUI is currently Chinese-first; this English page covers the main workflow, while detailed docs are in Chinese.

## License

[MIT](LICENSE)
