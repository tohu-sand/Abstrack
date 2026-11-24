# 第三者ソフトウェアのライセンス

Abstrack が直接依存している第三者ソフトウェアの一覧です。間接依存を含む完全な一覧とライセンス全文は次のファイルにあり、インストーラーにも同梱されます。

- [licenses/THIRD-PARTY-NOTICES-windows.txt](licenses/THIRD-PARTY-NOTICES-windows.txt)（Windows x64 向けビルド）
- [licenses/THIRD-PARTY-NOTICES-macos.txt](licenses/THIRD-PARTY-NOTICES-macos.txt)（macOS arm64 向けビルド）

各パッケージのバージョンは `Cargo.lock` と `package-lock.json` を参照してください。

## アプリに含まれるパッケージ

| パッケージ | 用途 | ライセンス |
|---|---|---|
| [Tauri](https://github.com/tauri-apps/tauri)（`tauri`、`tauri-plugin-dialog`、`@tauri-apps/api`） | ウィンドウ、メニュー、ファイルダイアログ、IPC | Apache-2.0 OR MIT |
| [lopdf](https://github.com/J-F-Liu/lopdf) | PDFの解析 | MIT |
| [pdfium-render](https://github.com/ajrcarey/pdfium-render) | PDFium のバインディング | MIT OR Apache-2.0 |
| [serde](https://github.com/serde-rs/serde)、[serde_json](https://github.com/serde-rs/json) | 設定ファイルと IPC のシリアライズ | MIT OR Apache-2.0 |
| [flate2](https://github.com/rust-lang/flate2-rs) | ページ画像の圧縮 | MIT OR Apache-2.0 |
| [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) | flate2 のバックエンド | Zlib |
| [webview2-com](https://github.com/wravery/webview2-rs) | WebView2 API の呼び出し（Windows のみ） | MIT |
| [windows、windows-core](https://github.com/microsoft/windows-rs) | Win32 API の呼び出し（Windows のみ） | MIT OR Apache-2.0 |
| [Svelte](https://github.com/sveltejs/svelte) | フロントエンド | MIT |
| [Vite](https://github.com/vitejs/vite) | フロントエンドのビルド | MIT |

## 同梱するバイナリ

PDFium はリポジトリに含めていません。`npm run fetch-pdfium` が [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) の `chromium/8125` ビルドを `vendor/pdfium/` にダウンロードします。

ダウンロードしたフォルダの `LICENSE` が pdfium-binaries のライセンスです。`licenses/` には、PDFium 本体と、PDFium に含まれるライブラリ（Abseil、AGG、dragonbox、fast_float、FreeType、HarfBuzz、Highway、ICU、Little CMS、libjpeg-turbo、OpenJPEG、libpng、LLVM libc、simdutf、zlib）のライセンスが入っています。インストーラーには、これらが `licenses/pdfium/` として同梱されます。

## 開発時のみ使用

配布物には含まれません。TypeScript、svelte-check、`@sveltejs/vite-plugin-svelte`、`@tauri-apps/cli`（`package.json`）、qpdf。
