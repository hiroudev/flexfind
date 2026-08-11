# FlexFind

Windows向けの常駐ファイル名検索ツール。[Everything](https://www.voidtools.com/) のようなインクリメンタル検索を、タブ・スキャン対象/検索対象の分離・ディスク永続化索引つきで提供する。

## ダウンロード

**[最新版をダウンロード](https://github.com/hiroudev/flexfind/releases/latest)**(`.msi` または `.exe`)

## 特徴

- 起動しておけばタスクトレイに常駐し、ホットキーで即座に検索ウィンドウを呼び出せる
- ファイル名のインクリメンタル検索(ワイルドカード・除外条件などの簡易クエリ構文に対応)
- 複数タブで検索対象を切り替え
- 索引はディスクに永続化し、再起動後も再スキャン不要
- スキャン対象(索引を作る範囲)と検索対象(実際に検索する範囲)を分離して設定可能

## 動作要件

- Windows 10 / 11 (64bit)

## ビルド方法

Tauri 2 + React + TypeScript 製。

```bash
npm install
npm run tauri:dev    # 開発起動
npm run tauri:build  # .msi / .exe を生成(src-tauri/target/release/bundle/ 配下)
```

Rust ツールチェーン([rustup](https://rustup.rs/))と Node.js が別途必要。

## ライセンス

[MIT](./LICENSE)
