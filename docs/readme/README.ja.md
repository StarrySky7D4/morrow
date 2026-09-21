# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · **日本語** · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

明日のアイデアのために、少し余白を。

Morrow（明隙）は、カードでアイデアを整理するローカルファーストのワークスペースです。全プラットフォーム対応のプラグイン構成へ移行中です。旧名は daemon、コードのパッケージ名は `morrow_studio`。Windows では Flutter が画面を、Rust ホストと隔離された Wasm プラグインが処理を担当します。Web／Android は従来の実装を使用しており、全プラットフォームでのプラグイン動作検証は未完了です。

## ダウンロードと互換性

現在のバージョンは **0.1.9-test.54+58**。**Windows x64 向けテストプレビュー**であり、安定版ではありません。Windows ZIP、対応するソース ZIP、SHA-256 一覧をダウンロードできます。すべて展開して `morrow_studio.exe` を実行し、DLL、ホスト、`data`、`plugins`、ライセンスファイルを残してください。

[test.54 をダウンロード](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.54) · [互換テスト版 test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` は、従来のデータ型と互換性を持つ最後の `0.1.x` テスト版です。以降の test 版では大規模な書き直しを進めるため、互換性を壊す変更が含まれる可能性があります。構成とデータモデルの確定・検証後に `0.2.0` を公開します。test.1 のデータは自動移行・上書きされません。更新前にライブラリと元の保護ファイルをバックアップしてください。保護は Windows ユーザーに紐づくため、データベースのコピーだけでは別アカウントへ移行できません。

## 主な機能

- カード：アイデア、プロジェクト、実験、お気に入り、検索、チェックリスト、削除の取り消し。Markdown 編集とプレビュー、添付ファイルの独立したコピーに対応。
- リッチコンテンツ：テキスト、スクリーンショット、ファイル、Office の書式付きテキストや表。複雑な独自形式はプレビューや添付に変換される場合があり、元のレイアウトの完全再現は保証しません。
- 外観：すりガラス、超透明、液体ガラス。標準・単色・テクスチャ・透明の背景、テーマのカラーホイール、カード／部品ごとの設定、視差効果やアニメーションを減らす設定に対応。
- メディア：画像・GIF・動画の背景、ローカル音楽、歌詞、フローティングヒント。形式の対応はプラットフォームとデコーダーに依存します。Web の透明表示はページ背景を透かし、デスクトップは透かしません。
- レイアウトと言語：レスポンシブな内容表示と独立した設定ページ。中国語、英語、ロシア語、フランス語、ドイツ語、スペイン語、日本語、韓国語、ポルトガル語。
- Windows のデータ保護：Rust による一元保存、監査記録の封印、ライブラリのスナップショット、元の識別情報のバックアップ・復元、同一識別情報での同時利用制限。

## 今回の最適化と検証

test.54 は開庫時の重複した完全検証をまとめ、証拠データのデコードを上限付きで並列化し、同じ検証トランザクション内で検証済みサイズとアーカイブのダイジェストを再利用します。開くたびに完全検証する方針は維持します。保存形式と同梱プラグインは変更せず、起動をまたぐ検証キャッシュも導入しません。

最後の読み取り最適化を直前の並列版と各 4 回交互に起動して比較しました。同一 PC・約 100 MB のライブラリコピーで、Dart エントリーから読み込み画面が消えるまでの中央値は 2.222 秒から 1.299 秒になりました。コピー登録時にファイルキャッシュが温まる可能性があり、キャッシュ消去後のコールドスタート保証ではありません。Core 568 件、Audit 97 件、実ホスト統合 2 件が成功、既存の Core 1 件はスキップです。条件はリリースノートを参照してください。

## ソースから実行

Flutter 3.44／Dart 3.12 または互換バージョンが必要です。Windows では Visual Studio の C++ デスクトップビルドツール、Windows SDK、Rust、PATH 上の Cap’n Proto コンパイラーも必要です。

Windows Rust ワークスペースの完全ビルド、統合検証、パッケージ作成：

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

既存の成果物は上書きできません。新しいバージョンまたは新しい出力先を使ってください。`-RefreshArtifact` でも上書きは許可されません。配布時は実行ディレクトリ全体を含めます。Web／Android の検証範囲は各プラットフォームの文書に従い、Windows のビルド成功で代替しません。

開発と基本チェック：

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## アーキテクチャ・SDK・今後の課題

目標は Flutter／Dart の UI、移植可能な Rust コア、交換可能なプラグイン実行基盤です。実行時の境界では固定契約、永続化と独自のデータ交換では Protobuf＋LZ4 を使用します。C／C++／Rust SDK と宣言的なプラグイン UI を開発中です。TS／JS プラグインは非対応で、動的 Dart プラグインも必須にしません。

完全な SDK はまだ凍結していません。管理された HTTP／HTTPS リクエスト、限定的な API サービスノード、TLS 識別情報管理は接続済みです。再起動後の Unknown 結果照合、完全なファイルシステム、3 言語の IO SDK、各プラットフォームの検証は残っています。開庫時は全履歴を走査します。読み取りと計算を重ねる完全なパイプライン、履歴の階層化、自動整理は未実装です。

## ドキュメント

- [リリースノートと検証](../../reports/0.1.9-test.54-release.md)
- [開発ボードと次の課題](../../docs/DEVELOPMENT_BOARD.md)
- [アーキテクチャのロードマップ](../../docs/FUTURE_ROADMAP.md)
- [機能移行と容量制限](../../docs/TEST1_RUST_PARITY.md)
- [C／C++／Rust SDK](../../sdk/README.md)
- [プラグイン UI 設計](../../docs/PLUGIN_SDK_AND_UI.md)
- [コンテンツ取得と形式対応](../../docs/RICH_CAPTURE.md)
- [Android ビルド](../../docs/ANDROID.md)
- [改名時の互換性](../../docs/RENAMING.md)
- [過去の README と段階別記録](../../docs/history/README-before-test54.md)

詳細設計と検証報告は主に中国語です。各言語の README は同じ現行版を案内しますが、翻訳の全面的なネイティブチェックは未実施です。

## ライセンス

`0.1.9-test.2` 以降の独自コード、SDK、文書、設定、素材は **AGPL-3.0-only** です。test.1 以前は Apache-2.0、第三者の内容は元のライセンスを維持します。配布物にはライセンス、著作権表示、対応ソースへの案内を含めます。

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
