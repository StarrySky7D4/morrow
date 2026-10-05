# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · [Español](../../docs/readme/README.es.md) · **日本語** · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

明日のアイデアのために、少し余白を。

Morrow（明隙）は、カードでアイデアを整理するローカルファーストのワークスペースです。全プラットフォーム対応のプラグイン構成へ移行中です。旧名は daemon、コードのパッケージ名は `morrow_studio`。Windows では Flutter が画面を、Rust ホストと隔離された Wasm プラグインが処理を担当します。Web はブラウザー内で Rust/Wasm をローカル実行します。Android は従来の経路を維持しており、全プラットフォームでの製品・プラグイン機能の同等性は未検証です。

## 現在の開発チェックポイント

今回の更新にはC08–C10が含まれます。独立したfs-directory-request-v1 profileは元の承認済みディレクトリ選択とownerを使います。Windows限定検証では新Rust34、既存回帰191、frame5が通過し、Python33とnative C/C++の2つのconsumerは別に数えます。新Rust Wasmはコンパイルのみ通過し、実際の新Rust guest実行、C/C++ Wasm、Workbench製品／GUI、保護されたSession、他プラットフォームは未検証です。従来のCore IO FileListはUnsupported、SDK26／G04はOPENのままです。新しいReleaseはありません。 [C10](../../reports/reconstruction-2026-10-05/directory-request-sdk.md)

## 開発状況（2026-10-05）

アプリのソースチェックポイントは **0.1.9-test.58+62** です。開発先のブランチは `codex/windows-sdk-convergence-20261005` で、cloud `468ef2e` と C02–C07 の変更を基にします。最新状況と次の作業の基準は [プロジェクト状況](../../docs/PROJECT_STATUS.md)、実際の Windows 検証範囲は [C07 限定検証レポート](../../reports/reconstruction-2026-10-05/directory-owner-sdk.md) を参照してください。

C07の歴史的限定検証：Windows のローカル合成検証では、元の owner 経路の 115 メソッド（ディレクトリ取消 7、directory owner 14、ネイティブディレクトリ 16 を含む）、元の回帰 42 メソッド、11 プログラムのネットワーク 100 メソッドが成功しました。42 メソッドの実行では frozen-region の 14 件と remote-reader の 1 件をフィルター対象として維持し、子プロセスの補助プログラムは追加の成功メソッドに数えません。完全な SDK や製品の受入完了を示す数値ではありません。

信頼されたディレクトリ capture/page/finish は元の IoWorker キュー、Manager、runtime、instance、IoBinding、FileList 承認を使用します。時刻取得と権限確認は短い原子的ステップで行い、ネイティブ資源の実際の解放後に割当枠を解放します。Unknown の自動再実行やカーソルの周回はありません。独立して版管理する型付き WebSocket/SSE ペイロードライブラリは Rust、C、C++ のコーデックと限定 Guest 検証を提供し、ディレクトリ／blob コーデックと有界状態もローカル検証済みです。

## C08／C09の過去の限定結果

**SDK26／G04 は OPEN のままで、完全な SDK は凍結していません。** 本番の保護された owner、ワークスペースの製品受入／GUI、選択ダイアログと祖先由来の証明、本番の信頼済み選択と秘密値のライフサイクル、新しいディレクトリ request／import／feature／helper profile の交渉、blob の永続履歴、受信サービスの完全な製品検証、実際の TLS／API、他プラットフォームは未完了または NOT_RUN です。C08の元workerによるnative秘密生成は限定検証済みです。14メソッド成功、未選択69件はフィルター対象です。owner115、original42、network100も個別に再検証しました。[C08記録](../../reports/reconstruction-2026-10-05/directory-secret-factory.md)を参照してください。C08追加はローカルで未commit／push、既存文書commit772466は保持されています。この検証による新しいインストーラーの公開はなく、安定版 SDK でもありません。

C09のローカルな信頼済み相対ディレクトリ選択はWindows限定検証を通過し、新20メソッドと元の回帰範囲は[C09報告](../../reports/reconstruction-2026-10-05/directory-selection-owner.md)に記録されていますが、保持するopened root→相対子孫の句柄チェーンだけが対象で、picker、rootより上の由来や本番資格は証明しません。Workbenchホスト入口はコンパイル確認のみで、ワークスペース製品受入／GUIは未実行、公開guest FileListとconditional ReplaceはUnsupported、SDK26／G04はOPENです。push済みチェックポイントは772466で、C08/C09はローカル未commit／push、新版未公開です。

## ダウンロードと互換性

最新の公開アプリプレビューは **0.1.9-test.56+60**。**Windows x64 向けテストプレビュー**であり、安定版ではありません。Windows ZIP、対応するソース ZIP、SHA-256 一覧をダウンロードできます。すべて展開して `morrow_studio.exe` を実行し、DLL、ホスト、`data`、`plugins`、ライセンスファイルを残してください。

[test.56 をダウンロード](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56) · [互換テスト版 test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` は、従来のデータ型と互換性を持つ最後の `0.1.x` テスト版です。以降の test 版では大規模な書き直しを進めるため、互換性を壊す変更が含まれる可能性があります。構成とデータモデルの確定・検証後に `0.2.0` を公開します。test.1 のデータは自動移行・上書きされません。更新前にライブラリと元の保護ファイルをバックアップしてください。保護は Windows ユーザーに紐づくため、データベースのコピーだけでは別アカウントへ移行できません。

## 主な機能

- カード：アイデア、プロジェクト、実験、お気に入り、検索、チェックリスト、削除の取り消し。Markdown 編集とプレビュー、添付ファイルの独立したコピーに対応。
- リッチコンテンツ：テキスト、スクリーンショット、ファイル、Office の書式付きテキストや表。複雑な独自形式はプレビューや添付に変換される場合があり、元のレイアウトの完全再現は保証しません。
- 外観：すりガラス、超透明、液体ガラス。標準・単色・テクスチャ・透明の背景、テーマのカラーホイール、カード／部品ごとの設定、視差効果やアニメーションを減らす設定に対応。
- メディア：画像・GIF・動画の背景、ローカル音楽、歌詞、フローティングヒント。形式の対応はプラットフォームとデコーダーに依存します。Web の透明表示はページ背景を透かし、デスクトップは透かしません。
- レイアウトと言語：レスポンシブな内容表示と独立した設定ページ。中国語、英語、ロシア語、フランス語、ドイツ語、スペイン語、日本語、韓国語、ポルトガル語。
- Windows のデータ保護：Rust による一元保存、監査記録の封印、ライブラリのスナップショット、元の識別情報のバックアップ・復元、同一識別情報での同時利用制限。

## 公開版 test.56 の変更と検証

test.56 では右クリック操作、カードの並べ替えとドラッグ、tips の項目別編集、独立したテーマプラグインを追加しました。瀑布流レイアウト、フィルター結果の更新、終了処理も改善しました。正式 UI の完全な自動保存とプラットフォーム間の機能同等性は引き続き未完了です。

### test.55 の過去の変更

test.55 では 7 種類の外観スタイルと深度調整を追加し、操作部品のアニメーションと部品間の余白を改善しました。設定の保存とフォントの適用を修正し、バックグラウンドでの安全な終了を改善して、下書き復元の基盤を追加しました。正式 UI の自動保存はまだ完成していません。

### test.54 の過去の最適化と検証

test.54 は開庫時の重複した完全検証をまとめ、証拠データのデコードを上限付きで並列化し、同じ検証トランザクション内で検証済みサイズとアーカイブのダイジェストを再利用します。開くたびに完全検証する方針は維持します。保存形式と同梱プラグインは変更せず、起動をまたぐ検証キャッシュも導入しません。

最後の読み取り最適化を直前の並列版と各 4 回交互に起動して比較しました。同一 PC・約 100 MB のライブラリコピーで、Dart エントリーから読み込み画面が消えるまでの中央値は 2.222 秒から 1.299 秒になりました。コピー登録時にファイルキャッシュが温まる可能性があり、キャッシュ消去後のコールドスタート保証ではありません。Core 568 件、Audit 97 件、実ホスト統合 2 件が成功、既存の Core 1 件はスキップです。条件は [test.54 のリリースノート](../../reports/0.1.9-test.54-release.md)を参照してください。

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

完全な SDK はまだ凍結していません。管理された HTTP／HTTPS リクエスト、限定的な API サービスノード、TLS 識別情報管理は接続済みです。再起動後の Unknown 結果照合、完全なファイルシステム、新しいディレクトリguestの実際の実行と製品検証、各プラットフォームの検証は残っています。開庫時は全履歴を走査します。読み取りと計算を重ねる完全なパイプライン、履歴の階層化、自動整理は未実装です。

## ドキュメント

- [プロジェクトの最新状況](../../docs/PROJECT_STATUS.md)
- [リリースノートと検証](../../reports/0.1.9-test.56-release.md)
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
