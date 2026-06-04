<div align="center">

<img src="assets/logo.png" width="140" alt="Easy Move+Resize ロゴ">

# Easy Move+Resize (Windows 版)

![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6?logo=windows&logoColor=white)
![Rust](https://img.shields.io/badge/built%20with-Rust-CE412B?logo=rust&logoColor=white)
![Win32](https://img.shields.io/badge/API-windows--rs-1E3A8A)
![License](https://img.shields.io/badge/license-MIT-blue)

</div>

macOS の [dmarcotte/easy-move-resize](https://github.com/dmarcotte/easy-move-resize) を
Windows に移植したものです。修飾キーを押しながらウィンドウの **どこでもドラッグ** することで、
タイトルバーを掴まなくてもウィンドウを移動・リサイズできます。

## 操作

- **修飾キー + 左ドラッグ** … ウィンドウを移動
- **修飾キー + 右ドラッグ** … ウィンドウをリサイズ
  - 掴んだ位置（ウィンドウを 3×3 に分割した領域）に応じて動く辺/角が決まります。
    四隅 → その角、上下左右の中央 → その辺、中央 → 右下角。

修飾キーの既定は **Ctrl + Alt**。トレイアイコンのメニューから
**Alt のみ** / **Ctrl + Win** に切り替えられます。

ドラッグ中は対象ウィンドウ全体が半透明のシルエットで塗りつぶされ、中央に現在のサイズ（W × H）と座標（X, Y）が表示されます。

## トレイメニュー

- **Enabled** … 機能全体の有効/無効
- **Bring window to front** … 操作時にウィンドウを前面に出す
- **Start at login** … ログイン時に自動起動（HKCU の Run キーに登録、管理者不要）
- **Modifier** … 修飾キーの選択（Ctrl+Alt / Alt / Ctrl+Win）
- **Quit** … 終了

## ビルド

```sh
cargo build --release
```

`target/release/easy-move-resize.exe` が生成されます。実行するとタスクトレイに常駐します。

## 仕組み

- グローバルな低レベルマウスフック（`WH_MOUSE_LL`）で全マウスイベントを監視し、
  修飾キーが押された状態でのドラッグだけを横取りしてウィンドウを `SetWindowPos` で動かします。
- 矩形計算（移動・リサイズ・リサイズ領域判定）は `src/geometry.rs` に純粋ロジックとして分離し、
  ユニットテストで担保しています（`cargo test`）。
- 最大化されたウィンドウは対象外です。

## ライセンス

MIT
