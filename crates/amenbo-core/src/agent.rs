//! The `amenbo agent --json` spec — the single source of truth for an AI agent. This spec alone
//! teaches everything an AI needs to operate amenbo: the philosophy, every command and flag, the
//! workflow, the rules, and what state to read. It lives in core so the CLI (`amenbo agent`) and the
//! GUI (the command palette / ⌘K Tauri command) are both fed from the same source. That every
//! subcommand in `cli.rs` shows up here is held by an integration test on the CLI side, which
//! catches a command that was never registered. amenbo is a single local store — there is no
//! sharing, sync, key or multi-device face — so the spec always returns the one personal shape
//! (`mode: personal`).

use serde_json::{json, Value};
use std::collections::HashMap;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_VERSION: &str = "1";

/// The spec with a locale applied, for the GUI to display. The single source of truth stays the
/// English [`build`]: the CLI and `agent --json`, which the AI reads, call it directly and get
/// English. Only the GUI's command palette goes through here, swapping the **prose fields**
/// (capability / command.summary / args.help / flags.help) for their translations just before
/// display. Command names, flag names and the CLI strings in examples are identifiers and runnable
/// lines, so they never change. An item with no translation, and an unknown locale, pass through in
/// English (graceful fallback) — add a translation and that one item starts rendering translated.
/// The default `en` is identical to [`build`]. Translations go in [`phrasebook`].
pub fn build_localized(locale: &str) -> Value {
    let mut spec = build();
    let table: HashMap<&str, &str> = phrasebook(locale).iter().copied().collect();
    if !table.is_empty() {
        localize_prose(&mut spec, &table);
    }
    spec
}

/// The per-locale phrasebook (English source → translation). The English spec, the source of truth,
/// never moves; the GUI's display changes exactly as far as the `(english, translation)` pairs added
/// here reach. The same English source gets the same translation wherever it appears, which is what
/// makes a help string shared by several flags translate once. The default `en`, and any unknown
/// locale, get an empty table and stay English.
fn phrasebook(locale: &str) -> &'static [(&'static str, &'static str)] {
    match locale {
        "ja" => JA_PHRASEBOOK,
        _ => &[],
    }
}

/// The Japanese phrasebook for the GUI, covering every prose string the command palette renders:
/// capability / command.summary / args.help / flags.help. Each key must match the English spec in
/// [`build`] character for character, because that is how the lookup is keyed. The CLI strings in
/// examples, command names and flag names are identifiers and runnable lines, so they are not
/// translated and pass through in English. The long blocks (principles, workflow and the like) are
/// outside what the palette displays, so they are not listed here. [`tr`] silently returns the
/// English source when a lookup misses, so a drift between the table and
/// the spec is invisible at runtime — which is why three tests hold the two together: no orphan
/// keys, no prose without a translation, and **no translation left in English** (an intentionally
/// identical one has to declare itself in the tests' `JA_VERBATIM`). Change one character of the
/// English source and its key here must change with it, or the tests fail.
const JA_PHRASEBOOK: &[(&str, &str)] = &[
    // capability (the group headings)
    ("Register a task", "タスクを登録する"),
    ("Find and filter tasks (see filterGrammar)", "タスクを検索・絞り込む（filterGrammar 参照）"),
    ("See a task's details, project, blockers and dependents", "タスクの詳細・プロジェクト・ブロッカー・被依存を見る"),
    ("Edit a task's fields (title / notes / due / start / priority)", "タスクのフィールドを編集する（タイトル / メモ / 期日 / 開始日 / 優先度）"),
    ("Track progress and reserve a task by moving it to in_progress (todo / in_progress / done / blocked)", "進捗を管理し、in_progress にしてタスクを予約する（未着手 / 進行中 / 完了 / ブロック）"),
    ("Split larger work into separate tasks and link blockers (there are no subtasks)", "大きな作業を別タスクに分割しブロッカーを結ぶ（サブタスクは無い）"),
    ("Anchor a task to the git commits that implemented it — record / list / forget SHAs (the chain from history back to a task)", "タスクを、それを実装した git コミットに結び付ける ── SHA を記録／一覧／削除する（履歴からタスクへ戻る鎖）"),
    ("Re-home a task to another project and reorder it", "タスクを別プロジェクトへ移し、並び順を変える"),
    ("Assign a task to a person or that person's AI, hand it back, or clear it", "タスクを人・またはその人の AI に割り当てる・戻す・外す"),
    ("Discuss on a task's timeline (a comment posted by mistake can be edited or deleted)", "タスクのタイムラインで議論する（誤投稿したコメントは編集・削除できる）"),
    ("Discuss on a decision's timeline (accept/reject reasons land here; a comment posted by mistake can be edited or deleted)", "決定記録のタイムラインで議論する（採択・却下の理由はここに残る。誤投稿したコメントは編集・削除できる）"),
    ("Attach what text cannot hold — screenshots, raw logs, benchmarks — to tasks, decisions, comments", "テキストに載せられないもの ── スクリーンショット・生ログ・ベンチマーク ── をタスク・決定記録・コメントに添付する"),
    ("Read the shared activity timeline (system events plus comments)", "共有のアクティビティ（システムイベントとコメント）を読む"),
    ("Record a decision — an append-only \"why we chose X\"", "決定を記録する ── 追記のみの「なぜ X を選んだか」"),
    ("Find and search decisions", "決定記録を検索する"),
    ("See a decision, its supersession chain, and the premises it stands on", "決定記録・その置換チェーン・前提にしている決定を見る"),
    ("Record that a decision stands on an older one (read that first; revisit this if it is overturned)", "ある決定が古い決定の上に立っていると記録する（前提は先に読む・覆されたらこの決定を見直す）"),
    ("Move a decision through its lifecycle (accept / reject / reopen / edit / supersede / delete)", "決定記録をライフサイクルに沿って動かす（採択 / 却下 / 再オープン / 編集 / 置換 / 削除）"),
    ("Link a decision to its implementation tasks", "決定記録とその実装タスクを結ぶ"),
    ("Promote a task comment into a decision", "タスクのコメントを決定記録へ昇格する"),
    ("Organize work into projects and order them (classification is via dimensions)", "作業をプロジェクトに整理し並べる（分類は次元で行う）"),
    ("Define classification axes (dimensions) with values and assign them to tasks", "分類軸（次元）と値を定義し、タスクに割り当てる"),
    ("See what to do now (overdue / today / in progress)", "いま何をすべきかを見る（期限超過 / 今日 / 進行中）"),
    ("Inspect configuration and this store's identity", "設定とこのストアの識別情報を確認する"),
    ("Update amenbo: open the installer, or self-update the standalone CLI in place (`--apply` / undo with `--rollback`)", "amenbo を更新する：インストーラを開く、または単独 CLI をその場で自己更新する（`--apply` / `--rollback` で取り消し）"),
    ("See projects", "プロジェクトを見る"),
    ("Allow an AI launched in a folder to operate amenbo (bind a folder to a project, unbind it, or re-sync its managed guidance block)", "フォルダで起動した AI に amenbo の操作を許可する（フォルダをプロジェクトに紐付ける、紐付けを外す、または管理ガイダンスブロックを再同期する）"),
    ("Take all data out (data sovereignty; no lock-in — export is one way, `restore` is the way back in)", "全データを外へ出す（データ主権・囲い込み無し ── export は片道で、戻す道は `restore`）"),
    ("Check data integrity", "データの整合性を点検する"),
    ("Write a verified full snapshot of the store's truth source to a single file", "ストアの真実源を検証済みの完全スナップショットとして 1 ファイルに書き出す"),
    ("Restore the store's truth source from a verified snapshot (the recovery side of backup)", "検証済みスナップショットからストアの真実源を復元する（バックアップの復旧側）"),
    ("Physically erase content from the store — a comment in full, or one accepted decision's body (human-gated maintenance)", "ストアから内容を物理的に消去する ── コメント全体、または採択済み決定 1 件の本文（人間ゲートの保守作業）"),
    // command.summary
    ("No arguments. Shows today's tasks and suggested next operations (discover).", "引数なし。今日のタスクと次にすべき操作の候補を表示します（発見）。"),
    ("Presents how to work here — the workflow and rules in full, plus an index of the commands (this JSON). The AI's entry point. A command's own flags and examples are pulled on demand with --command <name>, so the entry point stays small; --full prints them all inline.", "ここでの働き方——ワークフローとルールを全量、コマンドは索引で提示します（この JSON）。AI の入口です。各コマンドのフラグ・実行例は --command <name> で必要な時に引くので、入口は小さいまま保たれます（--full で全量を一度に出せます）。"),
    ("Shows version information.", "バージョン情報を表示します。"),
    ("Updates amenbo. By default opens this OS's one-piece installer (GUI + CLI) — resolved from the published latest.json, falling back to the releases page — in your browser. `--apply` self-updates the standalone CLI in place instead: it downloads the new CLI over TLS and swaps this binary (no installer, no elevation), keeping the replaced binary beside it; a GUI-managed CLI is updated from the desktop app, not here. `--rollback` undoes the last `--apply` offline, restoring that kept binary. Applying is always your explicit call — amenbo never updates in the background.", "amenbo を更新します。既定ではこの OS 向けの一体型インストーラ（GUI+CLI 同梱）を——公開 latest.json から解決し、無ければリリースページへフォールバック——既定ブラウザで開きます。`--apply` は代わりに単独 CLI をその場で自己更新します：新しい CLI を TLS で取得してこのバイナリを置換し（インストーラ不要・昇格不要）、置換前のバイナリを隣に残します；GUI 管理下の CLI はここではなくデスクトップアプリから更新されます。`--rollback` は直前の `--apply` をオフラインで取り消し、残しておいたバイナリを戻します。適用は常に利用者の明示操作で——amenbo が背後で更新することはありません。"),
    ("print the installer URL instead of opening a browser (headless / scripted use)", "ブラウザを開かず、インストーラ URL を表示します（ヘッドレス／スクリプト用途）"),
    ("self-update the standalone CLI in place (download + swap this binary) instead of opening the installer", "インストーラを開く代わりに、単独 CLI をその場で自己更新します（このバイナリをダウンロード＋置換）"),
    ("undo the last --apply, restoring the previous binary kept beside this one (offline, no download)", "直前の --apply を取り消し、隣に残した以前のバイナリを戻します（オフライン・ダウンロード不要）"),
    ("Shows configuration (store location, default view, etc.).", "設定（ストアの場所、既定ビューなど）を表示します。"),
    ("Changes a configuration value. Known keys: default_view, language, date_locale, human_name, ai_name (the display names of the two actors — this is the only way to rename either), human_avatar, ai_avatar (their icons, as a data:image/png;base64 URI), ai_allow_project_ops, onboarded, startup_integrity_check (read-only integrity doctor at open; warnings only; default on), update_check (checks a static latest.json for a newer release; infra-side only — no user data; timeout + silent-fail + cached; default on; AMENBO_UPDATE_CHECK=0 overrides).", "設定値を変更します。既知のキー: default_view, language, date_locale, human_name, ai_name（2 つの主体の表示名 ── 改名する口はここだけです）, human_avatar, ai_avatar（そのアイコン。data:image/png;base64 の URI）, ai_allow_project_ops, onboarded, startup_integrity_check（オープン時に走る読み取り専用の整合性ドクター。警告のみ・既定オン）, update_check（静的 latest.json を照会して新版に気付く。インフラ面のみ・ユーザーデータ非搭載・タイムアウト/失敗サイレント/キャッシュ・既定オン・AMENBO_UPDATE_CHECK=0 で無効化）。"),
    ("Shows this store's identity (display name / hardware-copy check).", "このストアの識別情報（表示名 / ハードウェア複製チェック）を表示します。"),
    ("Initializes a folder so an AI launched there is allowed to operate amenbo. amenbo does not read or write the project's contents (source or files). The store itself lives in app-data (a single database for the whole device); only .amenbo (a dir→project pointer) and AGENTS.md (the AI guide) are placed in the folder. On a device that already holds an amenbo store, init makes a new project in this folder — it does not start a second store. Secrets (keys) are also kept in the user area, not in the project directory. AGENTS.md is English-based and embeds the global user language (config language / --language) as a 'communicate with the human in this language' directive. A folder already bound to another project via .amenbo is rejected by default (init_pointer_exists; prevents clobbering the production pointer). A folder that has no .amenbo but already holds an amenbo managed block (in CLAUDE.md/AGENTS.md) is no longer rejected on the marker alone: init reverse-looks-up the bindings registry and, if exactly one live project claims the folder, recovers the lost pointer (a bind, not a new project); if several claim it, it stops as ambiguous (init_ambiguous_owners); if none do, it proceeds and idempotently regenerates the block. Use bind to re-bind to an existing one, or --force to truly recreate and overwrite.", "フォルダを初期化し、そこで起動した AI が amenbo を操作できるようにします。amenbo はプロジェクトの中身（ソースやファイル）を読み書きしません。ストア本体は app-data（端末ぜんぶで 1 つのデータベース）にあり、フォルダには .amenbo（ディレクトリ→プロジェクトのポインタ）と AGENTS.md（AI 手引き）だけを置きます。すでに amenbo のストアがある端末では、init はこのフォルダに新しいプロジェクトを作ります（2 つ目のストアは起こしません）。秘密（鍵）もプロジェクトディレクトリではなくユーザー領域に保持します。AGENTS.md は英語ベースで、グローバルのユーザー言語（config の language / --language）を「この言語で人間とやり取りする」指示として埋め込みます。すでに .amenbo で別プロジェクトに紐付いたフォルダは既定で拒否します（init_pointer_exists・本番ポインタの上書きを防ぐ）。.amenbo は無いが amenbo 管理ブロック（CLAUDE.md/AGENTS.md 内）をすでに持つフォルダは、marker の存在だけでは拒否しません。init は bindings レジストリを逆引きし、このフォルダを主張する生存プロジェクトがちょうど 1 件なら消えたポインタを復旧し（新規プロジェクトは作らず bind 相当）、複数あれば曖昧として止め（init_ambiguous_owners）、0 件なら続行して管理ブロックを冪等再生成します。既存に紐付け直すには bind を、本当に作り直して上書きするには --force を使います。"),
    ("Allows an AI launched in this folder to operate an existing project (it does not touch the contents; it just places a .amenbo pointer and locally registers project→dir). Shows the current binding when --project is omitted. Several folders may point at the same project (many-to-one). Binding a subdirectory of a folder that is already managed (a parent has a .amenbo) is rejected (binding_nested_tree) so a stray bind cannot shadow the root pointer; pass --force to bind it intentionally. If the target is gone, binding_stale. By default the pointer lands in the current directory; pass --dir <path> to place it in another existing folder (bind a folder from outside it).", "このフォルダで起動した AI に既存プロジェクトの操作を許可します（中身には触れず、.amenbo ポインタを置き、project→dir の参照をローカル登録するだけ）。--project 省略時は現在の紐付けを表示します。同じプロジェクトを複数のフォルダが指せます（多対一）。すでに管理下のフォルダ（親が .amenbo を持つ）のサブディレクトリを紐付けるのは、ルートポインタを覆い隠さないよう拒否します（binding_nested_tree）。意図的に紐付けるなら --force を渡します。対象が消えていれば binding_stale です。既定ではポインタを現在のフォルダに置きますが、--dir <path> で別の実在フォルダに置けます（そのフォルダの外から紐付ける）。"),
    ("Removes this folder's .amenbo binding (and amenbo's managed blocks in AGENTS.md/CLAUDE.md, keeping your own content), the inverse of bind/init. The project itself is kept: this is a many-to-one unbind, so only this folder's pointer is removed and other folders bound to the same project are untouched. It also forgets this folder from the local project→folder reference registry. If the folder has no .amenbo of its own it is not unbound (unbind_no_binding); an inherited binding from an ancestor is reported, not silently removed, so the whole tree is never unbound by accident.", "このフォルダの .amenbo 紐付け（と AGENTS.md/CLAUDE.md 内の amenbo 管理ブロック。あなたの内容は残します）を取り除きます。bind/init の逆です。プロジェクト自体は残ります: これは多対一の紐付け解除なので、外すのはこのフォルダのポインタだけで、同じプロジェクトに紐付いた他のフォルダには触れません。ローカルの project→folder 参照レジストリからもこのフォルダを忘れます。フォルダ自身が .amenbo を持たなければ解除しません（unbind_no_binding）。祖先から継承した紐付けは、黙って外さず報告します（ツリー全体を誤って解除しないため）。"),
    ("Shows a summary of what to do now (overdue / today / in progress).", "いま何をすべきか（期限超過 / 今日 / 進行中）の要約を表示します。"),
    ("Shows activity (system events plus comments) as one timeline. History reads newest-first; passing a cursor to --since reads the increment oldest-first (an agent's poll-for-what-changed). Humans and the AI read the same stream. Every response carries an opaque cursor; --for me narrows it to what a facet should act on.", "アクティビティ（システムイベントとコメント）を 1 つのタイムラインで表示します。履歴は新しい順で読み、--since にカーソルを渡すと差分を古い順で読みます（エージェントの「何が変わったか」ポーリング）。人間と AI は同じ流れを読みます。すべての応答は不透明なカーソルを持ち、--for me は facet が対応すべきものに絞ります。"),
    ("Data integrity check (orphan references, broken ordering, bound folders whose CLAUDE.md/AGENTS.md still carry an outdated managed-block version after a binary update, bound folders whose .amenbo pointer is still in a pre-migration format, etc.). Side-effect-free by default: this face reports, it never rewrites. What it reports about a folder heals on its own the next time you run amenbo there — the managed block follows this binary and a legacy .amenbo is upgraded — so what stays listed here is the folders you have not been in (sync-guide resyncs every bound folder's block at once). --fix repairs fixable problems: it sweeps attachment files nothing references any more (the delete path reclaims its own, so this collects only what it had to spare) and forgets folder bindings no live project claims. Every repair is non-destructive — it drops nothing you can read.", "データ整合性を点検します（孤立参照、壊れた並び順、バイナリ更新後も古い版の managed block を持ち続けている紐付けフォルダの CLAUDE.md/AGENTS.md、移行前の形式のままの `.amenbo` ポインタを持つ紐付けフォルダ など）。既定では副作用なし（この面は指摘するだけで、書き換えません）。指摘されたフォルダは、そこで次に amenbo を実行したときに自力で直ります——managed block はこのバイナリの現行版へ追従し、旧形式の `.amenbo` は現行形式へ書き換わります。ここに残り続けるのは「あなたが入っていないフォルダ」です（sync-guide が全紐付けフォルダの block を一度に再同期します）。--fix は直せる問題を修復します——参照の無くなった添付ファイルの実体を掃き（削除経路は自分が孤児にしたぶんを自分で回収するので、ここへ落ちてくるのは見送られた取りこぼしだけです）、生きたプロジェクトが誰も主張しないフォルダ紐付けを索引から忘れます。どの修復も非破壊です（読めるものは何ひとつ落としません）。"),
    ("Re-syncs amenbo's managed guidance block in bound folders to this binary's current format version. A folder follows on its own the moment you run amenbo in it, so this is for the folders you have not been in — and for a block amenbo could not write (a read-only checkout). Idempotent and low-churn: a folder's CLAUDE.md/AGENTS.md is rewritten only when its managed block actually changed, each folder's own language label is preserved (never downgraded), and your content outside the markers is untouched. By default it targets every folder locally bound on this machine (the machine's binding registry, store-independent) so its scope matches what doctor scans; pass --dir to resync just one folder. Moved/renamed folders are skipped silently.", "紐付けフォルダの amenbo 管理ガイダンスブロックを、このバイナリの現行フォーマット版へ再同期します。フォルダは、そこで amenbo を実行した時点で自力で追従するので、これは「あなたが入っていないフォルダ」——と、amenbo が書き込めなかったブロック（読み取り専用のチェックアウト等）——のためのコマンドです。冪等かつ低 churn: フォルダの CLAUDE.md/AGENTS.md は managed block が実際に変わった時だけ書き換え、各フォルダ自身の言語ラベルは保持し（劣化させません）、マーカー外のあなたの内容には触れません。既定ではこの端末でローカルに紐付いた全フォルダ（この端末の紐付けレジストリ・ストア非依存）を対象にし、doctor の走査範囲と一致します。--dir で 1 フォルダだけ再同期します。移動／改名されたフォルダは黙って飛ばします。"),
    ("resync just this folder (defaults to every locally bound folder)", "このフォルダだけ再同期します（既定は全てのローカル紐付けフォルダ）"),
    ("Checks the shape of the given tasks (all data when omitted). Side-effect-free.", "指定したタスクの形を点検します（省略時は全データ）。副作用なし。"),
    ("Finds amenbo refs (AMB-T-<n> / AMB-D-<n> …) in text on its way out of this store — a commit message, a diff, a file — reports each as path:line, and exits non-zero if there is one. An id resolves only for someone holding this store; anywhere else it is a reference into nothing. Read-only: it reports and never edits (there is no --fix). With no arguments it reads the staged diff (git diff --cached) and scans what the commit ADDS — a ref in untouched or deleted text is not what this commit is leaking. Pass file paths to lint those instead (the message file git hands a commit-msg hook included), or --stdin for piped text. A bare #<n> is left alone: that is a GitHub issue, and a T-<n> may be another tracker's — which is exactly what the AMB- namespace settles. It opens no store and resolves no id (the AMB- prefix is the whole test), so it answers the same in a checkout, in CI, and over any text at all, and needs no .amenbo to run. The exit code is the verdict: 0 clean, 1 a ref was found (or the input could not be read).", "このストアから出ていくテキスト——コミットメッセージ・diff・ファイル——の中から amenbo の ref（AMB-T-<n> / AMB-D-<n> など）を見つけ、path:line で報告し、1 件でもあれば非 0 で終了します。ID を辿れるのはこのストアを持つ人だけで、それ以外の場所では何も指さない参照です。読み取り専用: 報告するだけで、書き換えは一切しません（--fix はありません）。引数なしなら staged diff（git diff --cached）を読み、そのコミットが「追加する」ぶんだけを見ます——触っていない行や削除される行の ref は、このコミットが漏らしているものではないからです。ファイルパスを渡せばそちらを lint します（git が commit-msg フックへ渡すメッセージファイルを含む）。パイプしたテキストは --stdin です。素の #<n> には触れません: それは GitHub の issue であり、T-<n> は他のトラッカーのものかもしれない——AMB- という namespace はまさにそれを決着させるためにあります。ストアを開かず ID も解決しません（AMB- という接頭辞だけが判定のすべて）。だからチェックアウトでも CI でも任意のテキストでも同じ答えを返し、.amenbo は不要です。終了コードが判定そのものです: 0 は綺麗、1 は ref を検出（または入力を読めなかった）。"),
    ("file(s) to lint (default: the staged diff)", "lint するファイル（既定: staged diff）"),
    ("lint the text piped on stdin instead", "代わりに stdin にパイプされたテキストを lint します"),
    ("machine-readable output (ok / count / hits[path,line,ref])", "機械可読な出力（ok / count / hits[path,line,ref]）"),
    ("report nothing and let the exit code speak — for a caller that wants only the verdict (the hook amenbo installs does not pass it: a refused commit has to say what refused it)", "何も報告せず終了コードだけで語ります ── 判定だけが欲しい呼び出し側向けです（amenbo が入れるフックはこれを渡しません: 弾かれたコミットは何に弾かれたかを言う必要があるため）"),
    ("Catch an amenbo ref in text on its way out of this store, before it lands somewhere it means nothing (read-only; reports path:line and exits non-zero)", "このストアから出ていくテキストの中の amenbo ref を、何も意味しない場所へ落ちる前に捕まえます（読み取り専用・path:line で報告し非 0 終了）"),
    ("Run the lint on every commit, by installing it as a git hook (asked once for the lint as a feature, on this device — one answer covers the repositories amenbo works in, later ones included; amenbo touches only the hook it wrote)", "lint を git フックとして入れ、コミットのたびに走らせます（訊くのは lint という機能に対して端末で一度だけ・その1つの答えが amenbo の扱うリポジトリすべてに——後から追加するものにも——適用されます・amenbo が触るのは自分が書いたフックだけ）"),
    ("Writes the git hooks that run `amenbo lint` on every commit: `pre-commit` reads the staged diff, and `commit-msg` reads the message, which is the only place git offers it (at pre-commit time no message exists yet). One lint, two of git's doors. Installing means writing into your git plumbing, which amenbo does not do unasked: it asks once — for the lint as a feature, on this device — and that one answer covers every slot and every repository, the ones bound after it included. `install` is the explicit face of that, wiring the repository it runs in; it is usable any time, including after a `no`, and it takes back an earlier `uninstall` here. amenbo marks the hooks it writes and touches nothing else: a hook from husky, lefthook or your own hand is NEVER overwritten — install steps around it, wiring the slots it may own and naming the one line to add to the rest (`amenbo lint || exit 1`, or `amenbo lint \"$1\" || exit 1` for commit-msg). Only an install with no slot to write at all is refused. Re-running over amenbo's own hooks rewrites them, which is how a newer build's hooks land. They honour core.hooksPath, exit 0 when amenbo is not on PATH (a convenience, not a gate), and one commit is bypassed with `git commit --no-verify`.", "コミットのたびに `amenbo lint` を走らせる git のフックを書きます: `pre-commit` は staged diff を、`commit-msg` はコミット文を読みます（pre-commit の時点ではコミット文がまだ存在しないため、git がそれを渡すのは commit-msg だけです）。lint は1つ、git の入口が2つです。インストールはあなたの git 配管への書き込みであり、amenbo は黙ってそれをしません: 訊くのは **lint という機能**に対して端末で一度だけで、その1つの答えが全ての枠・全てのリポジトリを——後から bind するものも含めて——覆います。`install` はその明示の面で、実行したリポジトリを配線します。いつでも——`no` と答えた後でも——使え、ここで先に `uninstall` していればそれを取り消します。amenbo は自分が書いたフックに印を付け、それ以外には触れません: husky・lefthook・自作のフックは**絶対に上書きしません**——install はそれを避けて通り、書いてよい枠だけを配線し、残りには足すべき1行（`amenbo lint || exit 1`、commit-msg なら `amenbo lint \"$1\" || exit 1`）を示します。書ける枠が1つも無い install だけが拒否されます。amenbo 自身のフックへの再実行は書き直しで、新しいビルドのフックはこうして入ります。フックは core.hooksPath に従い、amenbo が PATH に無ければ 0 で抜け（gate ではなく利便性のため）、1 回のコミットは `git commit --no-verify` で迂回できます。"),
    ("Removes the lint hooks amenbo wrote from this repository, and opts it out so a device-wide yes does not re-wire it at the next startup (this is per repository — it does not touch the device's answer). The mirror of install, refusal for refusal and partial for partial: a hook amenbo did not write is not amenbo's to delete and is left alone, and only a call with nothing of ours to remove and a stranger in the way is refused. With no hooks of ours there, it records the opt-out and does nothing else. It closes the question for this repository, not the door — `hooks install` re-wires it whenever you want it back.", "amenbo が書いた lint フックをこのリポジトリから削除し、端末全体の yes があっても次回起動で再配線されないよう、このリポジトリを opt-out として記録します（これはリポジトリ単位で、端末の答えには触れません）。install の鏡像で、拒否も部分成立も対称です: amenbo が書いていないフックは amenbo が消してよいものではなく、そのまま残します。自分のフックが1つも無く他人のフックだけがある呼び出しだけが拒否されます。自分のフックが無ければ、opt-out を記録するだけで他には何もしません。このリポジトリでの質問を閉じるだけで、扉は閉じません——`hooks install` でいつでも配線し直せます。"),
    ("Shows the two facts side by side: what is in each hook slot (no hook / amenbo's, with its marker version / one amenbo did not write), and what this device answered (not asked yet / yes / no) — plus a line when this repository is opted out. They are independent on purpose — the answer says what was answered and is NEVER read as a mirror of the disk, which is what makes a hook deleted or added by hand a state amenbo can see rather than one that breaks it. Read-only.", "2つの事実を並べて示します: フックの枠に何があるか（フック無し / amenbo のもの・印の版付き / amenbo が書いていないもの）と、この端末が何と答えたか（未回答 / yes / no）——加えて、このリポジトリが opt-out されていればその行。この2つが独立なのは設計です——答えは「何と答えたか」を語るだけで、ディスクの鏡としては**決して**読みません。だから手で消された・足されたフックは、amenbo が破綻する事態ではなく、見て取れる状態になります。読み取り専用。"),
    ("machine-readable output (in_git_repo / hooks / consent)", "機械可読な出力（in_git_repo / state / consent）"),
    ("Creates a project.", "プロジェクトを作成します。"),
    ("Lists projects.", "プロジェクトを一覧します。"),
    ("Shows project details (counts, etc.) plus bound_folders: the folders whose .amenbo points at this project (the reverse of bind), each inspected — exists (false = the folder moved or was deleted), pointer_missing (the folder is there but its .amenbo is gone), legacy (a pre-migration pointer) and mismatch (the pointer belongs to another store).", "プロジェクトの詳細（件数など）に加え、bound_folders を表示します: .amenbo がこのプロジェクトを指すフォルダ（bind の逆）で、各フォルダの検分結果付き——exists（false＝移動／削除された）、pointer_missing（フォルダは在るのに .amenbo が消えた）、legacy（移行前の旧形式ポインタ）、mismatch（ポインタが別のストアのもの）。"),
    ("Updates a project.", "プロジェクトを更新します。"),
    ("Reorders a project.", "プロジェクトの並び順を変えます。"),
    ("Archives a project.", "プロジェクトをアーカイブします。"),
    ("Unarchives a project.", "プロジェクトのアーカイブを解除します。"),
    ("Deletes a project — permanently, with its tasks and everything hanging off them (a delete is physical and irreversible; archive instead if you want it kept).", "プロジェクトを完全に削除します。属するタスクとその一切も一緒に消えます（削除は物理削除で不可逆。残したいならアーカイブを使ってください）。"),
    ("Adds a dimension (a user-defined classification axis) to a project. New projects seed no dimensions — create the axes you need. An axis is single-select; --ordered gives the values an explicit order, --time-axis marks it as the ordered time lane.", "プロジェクトに次元（ユーザー定義の分類軸）を追加します。新規プロジェクトは次元を持たないので、必要な軸を作ってください。軸は単一選択です。--ordered は値に明示的な順序を与え、--time-axis は順序付き時間レーンとして印を付けます。"),
    ("Lists a project's dimensions in display order, each with its values.", "プロジェクトの次元を表示順で、それぞれの値とともに一覧します。"),
    ("Shows a dimension: name, notes, kind (single-select, ordered, time-axis), and its values.", "次元を表示します: 名前、メモ、種類（単一選択、順序付き、時間軸）、そして値。"),
    ("Renames a dimension.", "次元の名前を変えます。"),
    ("Updates a dimension's name, notes, value ordering, and/or time-axis role. Only the given fields change.", "次元の名前・メモ・値の順序・時間軸の役割を更新します。指定したフィールドだけが変わります。"),
    ("Reorders a dimension within its project.", "プロジェクト内で次元の並び順を変えます。"),
    ("Deletes a dimension permanently; its values and task assignments go with it (alias: delete).", "次元を完全に削除します。その値とタスクへの割り当ても一緒に消えます（別名: delete）。"),
    ("Adds a value to a dimension (appended after existing values). On a time-axis dimension the value can carry a period.", "次元に値を追加します（既存の値の後ろに追加）。時間軸の次元では、値は期間を持てます。"),
    ("Renames a dimension value.", "次元の値の名前を変えます。"),
    ("Updates a dimension value's name and/or its period (time-axis dimensions only). Only the given fields change; an open end means the period is ongoing.", "次元の値の名前や期間を更新します（期間は時間軸の次元のみ）。指定したフィールドだけが変わります。終わりが開いていれば、その期間は継続中です。"),
    ("first day of the value's period (time-axis dimensions only)", "値の期間の初日（時間軸の次元のみ）"),
    ("last day of the value's period; omit to leave it ongoing (time-axis dimensions only)", "値の期間の最終日。省くと継続中になります（時間軸の次元のみ）"),
    ("open the period's start", "期間の始まりを開きます"),
    ("open the period's end (the value becomes ongoing)", "期間の終わりを開きます（その値は継続中になります）"),
    ("Reorders a value within its dimension.", "次元内で値の並び順を変えます。"),
    ("Deletes a dimension value permanently; its task assignments go with it (alias: value-delete).", "次元の値を完全に削除します。そのタスクへの割り当ても一緒に消えます（別名: value-delete）。"),
    ("Assigns a task a value of a dimension. An axis is single-select, so the task's prior value on that axis is replaced.", "タスクに次元の値を割り当てます。軸は単一選択なので、その軸の以前の値は置き換わります。"),
    ("Clears a task's value of a dimension.", "タスクの次元の値をクリアします。"),
    ("Creates a task in a project. Break larger work into separate tasks linked with task depend (no subtasks).", "プロジェクトにタスクを作成します。大きな作業は task depend で結んだ別タスクに分けます（サブタスクは無い）。"),
    ("Lists tasks. --limit/--offset page in sort order (JSON carries total_matched = the count before paging, count = this page).", "タスクを一覧します。--limit/--offset はソート順でページングします（JSON は total_matched = ページング前の件数、count = このページの件数を持ちます）。"),
    ("Shows task details — project, blockers (blocked_by) and dependents (blocks: what finishing this task would unblock).", "タスクの詳細を表示します ── プロジェクト、ブロッカー（blocked_by）、被依存（blocks: このタスクを終えると着手可能になるもの）。"),
    ("Updates a task. --start is not a note to self: a day still ahead holds the task at ready:no and refuses its reserve, so declare one only when you mean it (--clear-start takes it back).", "タスクを更新します。--start は覚え書きではありません: 未到来の日はタスクを ready:no に留め、予約を拒みます。そのつもりのときだけ宣言してください（--clear-start で取り消せます）。"),
    ("Marks a task done.", "タスクを完了にします。"),
    ("Returns a completed task to not-done (sugar for status=todo).", "完了したタスクを未完了に戻します（status=todo の糖衣）。"),
    ("Explicitly changes the progress state (todo/in_progress/done/blocked). Setting in_progress reserves the task: a compare-and-swap that succeeds only from todo, so a second session's reserve is rejected with already_reserved (the double-work guard), and only a ready task can be reserved, so an open blocker, an unsettled premise, or a start day still ahead is rejected with not_ready (there is no --force; correct the declaration with task update --start). todo hands it back. done marks completed. blocked declares an external stall only — an unmet premise is derived as ready:no, never declared here.", "ステータスを明示的に変えます（todo/in_progress/done/blocked）。in_progress は着手の予約です: 現在 todo のときだけ成功する compare-and-swap で、別セッションの予約は already_reserved で弾かれ（二重着手ガード）、さらに予約できるのは ready なタスクだけなので、未完了のブロッカー・未確定の根拠・未到来の着手日があれば not_ready で弾かれます（--force は無い。前倒しするなら task update --start で宣言を直します）。todo で手放します。done は完了にします。blocked は外部要因で止まっていることの宣言に限ります ── 前提の未達は ready:no として導出されるもので、ここで宣言しません。"),
    ("Marks blocked (stuck) — for an external stall only (a second machine, a human go/no-go); an unmet premise is derived as ready:no instead. --reason is recorded as a comment.", "ブロック（行き詰まり）にします ── 外部要因（2 台目の実機、人間の go/no-go）で止まっている場合に限ります。前提の未達は代わりに ready:no として導出されます。--reason はコメントとして記録されます。"),
    ("Re-homes a task to another project and reorders it (a task belongs to exactly one project).", "タスクを別プロジェクトへ移し、並び順を変えます（タスクはちょうど 1 つのプロジェクトに属します）。"),
    ("Makes this task depend on another task (--on becomes a blocker that must be done first — the edge has teeth: while the blocker is open, reserving this task is rejected with not_ready). Self-reference and cycles are rejected, and so is an edge that would cross projects (a project's context must not leak into another — both ends must sit in the same project; an inbox task, belonging to none, is not a crossing). Idempotent. Derived ready/blocked_by_open is reflected in the ready: filter of task show and list.", "このタスクを別タスクに依存させます（--on が先に完了しなければならないブロッカーになります ── このエッジには歯があり、ブロッカーが未完了のあいだ、このタスクの予約は not_ready で弾かれます）。自己参照と循環は拒否します。プロジェクトを跨ぐエッジも拒否します（あるプロジェクトの文脈を別のプロジェクトへ流さない ── 両端は同じプロジェクトに居ること。どの PJ にも属さない受信箱のタスクは跨ぎになりません）。冪等。導出される ready/blocked_by_open は task show と list の ready: フィルタに反映されます。"),
    ("Removes a dependency (idempotent). If removal makes the task startable, it emits task.unblocked.", "依存を取り除きます（冪等）。取り除いた結果タスクが着手可能になれば task.unblocked を発します。"),
    ("Records a git commit SHA on a task (1 task : many commits) — the anchor from history back to a task, since a public commit carries no store-local reference. amenbo stores the SHA as an opaque string: it never reads git, verifies the commit, or knows which forge it lives on. The SHA is validated at the door — only full-length lower-case hex is admitted (40 for SHA-1, 64 for SHA-256), case is folded, and short forms, branches, tags and revisions are refused. Idempotent: a SHA already on the task is a no-op (the `(task_id, sha)` index sees bytes only).", "タスクに git コミット SHA を記録します（1 タスク : 多コミット）── 公開コミットは store ローカルの参照を持たないため、履歴からタスクへ戻る鎖はタスク側にしか張れません。amenbo は SHA を不透明な文字列として保存するだけで、git を読まず・コミットの実在を検証せず・どの forge にあるかも知りません。SHA は玄関で検証します ── 通すのは完全形の小文字 hex だけ（SHA-1 は 40 桁・SHA-256 は 64 桁）で、大文字は畳み、短縮形・ブランチ・タグ・revision は拒否します。冪等: すでにそのタスクに在る SHA は no-op です（`(task_id, sha)` 索引はバイト列しか見ません）。"),
    ("Lists a task's recorded commit SHAs, oldest first. To go the other way — the task a SHA belongs to — read the commit with `git show <sha>` (amenbo does not read git).", "タスクに記録されたコミット SHA を古い順で一覧します。逆向き ── ある SHA がどのタスクのものか ── を辿るには、そのコミットを `git show <sha>` で読みます（amenbo は git を読みません）。"),
    ("Forgets a commit SHA on a task — a hard delete (idempotent; the SHA is normalised the way it was stored, so any case removes it). The commit itself and the task are untouched.", "タスクのコミット SHA を忘れます ── 物理削除です（冪等・SHA は保存時と同じに正規化されるので、どの大小文字でも消えます）。コミット自体とタスクには触れません。"),
    ("Assigns an assignee to a task. Use --ai to delegate to 'that person's AI' (assignee_kind=ai). Reassignment is plain — the task just moves to its new assignee, with no special status.", "タスクに担当を割り当てます。--ai で「その人の AI」に委任します（assignee_kind=ai）。付け替えは素朴で、タスクが新しい担当へ移るだけで特別な状態にはなりません。"),
    ("Removes a task's assignee.", "タスクの担当を外します。"),
    ("Deletes a task permanently, with its comments, dependency edges and attachments (a delete is physical and irreversible).", "タスクを完全に削除します（コメント・依存エッジ・添付も一緒に消えます）。"),
    ("Adds a comment to a task.", "タスクにコメントを追加します。"),
    ("Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `comment list` prints it.", "誤投稿したコメントを削除します ── 完全に消え、付いていた添付も一緒に消えます。コメントは id で指定します。id は `comment list` が表示します。"),
    ("Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `decision comment list` prints it.", "誤投稿したコメントを削除します ── 完全に消え、付いていた添付も一緒に消えます。コメントは id で指定します。id は `decision comment list` が表示します。"),
    ("Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay, so links to it keep resolving. Prefer this over deleting and re-posting when you only need to fix what a comment says. Identify the comment by id; `comment list` prints it.", "コメント本文をその場で書き換えます── id・タイムライン上の位置・付いていた添付はそのまま残るので、そのコメントへの参照も生きたままです。言い直したいだけなら、削除して投稿し直すよりこちらを使ってください。コメントは id で指定します。id は `comment list` が表示します。"),
    ("Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay. This edits a comment, not the decision's own body (conclusion + rationale); the two are separate. Identify the comment by id; `decision comment list` prints it.", "コメント本文をその場で書き換えます── id・タイムライン上の位置・付いていた添付はそのまま残ります。これはコメントの編集であって、決定自身の本文（結論＋根拠）とは別物です。コメントは id で指定します。id は `decision comment list` が表示します。"),
    ("the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "新しい本文（Markdown）── 元の本文をまるごと置き換えます。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("Shows a task's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).", "タスクのコメントを古い順で表示します。--limit/--offset はページングします（JSON は total_matched = ページング前の件数、count = このページの件数を持ちます）。"),
    ("Adds a comment to a decision's timeline. The decision's own body (conclusion + rationale) holds what was decided; comments are the way to discuss it or record accept/reject reasoning (`decision accept/reject --reason` is thin sugar over this).", "決定記録のタイムラインにコメントを追加します。決定自身の本文（結論＋根拠）は決めた内容そのもので、その周りの議論や採択・却下の理由の記録はコメントで行います（`decision accept/reject --reason` はこれの薄い糖衣です）。"),
    ("Shows a decision's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).", "決定記録のコメントを古い順で表示します。--limit/--offset はページングします（JSON は total_matched = ページング前の件数、count = このページの件数を持ちます）。"),
    ("Attaches a file (content-addressed `blob`) or external link (--url) to a single TASK comment, kept separate from the parent task's own attachments so a comment's own attachment timeline is preserved. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Identify the comment by id; find ids with `comment list <task> --json`. A decision comment is attached to with `decision comment attach` — the two comment tables number apart, so the command, not the id, says which table an id belongs to. List them with `attach ls --task-comment <id>`.", "単一の**タスク**コメントにファイル（内容アドレスの `blob`）や外部リンク（--url）を添付します。親タスク自身の添付とは分けて保持し、コメント固有の添付タイムラインを保ちます。`task attach` と同じ 2 モードで、何を添付するかの判断も同じです ── そちらで読んでください。コメントは id で指定します。id は `comment list <task> --json` で見つけます。決定記録のコメントには `decision comment attach` で添付します ── 2 つのコメント表は別々に採番するので、その id がどちらの表のものかは id ではなくコマンドが語ります。一覧は `attach ls --task-comment <id>` です。"),
    ("Attaches a file (content-addressed `blob`) or external link (--url) to a single DECISION comment — the mirror of `comment attach` (which takes task comments), down to the judgement of what is worth attaching (`task attach`). Identify the comment by id; find ids with `decision comment list <decision> --json`. List them with `attach ls --decision-comment <id>`.", "単一の**決定記録**コメントにファイル（内容アドレスの `blob`）や外部リンク（--url）を添付します ── `comment attach`（タスクのコメントを取る）の鏡で、何を添付するかの判断まで同じです（`task attach`）。コメントは id で指定します。id は `decision comment list <decision> --json` で見つけます。一覧は `attach ls --decision-comment <id>` です。"),
    ("Records a new decision (proposed) — an append-only \"why we chose X\". A decision is a Task sibling (project-scoped), NOT a task: it has no mailbox workflow and never appears in task lists. Decisions have their own device-global number space, shown as AMB-D-N (tasks are AMB-T-N); the kind code keeps AMB-D-<n> / AMB-T-<n> unambiguous. The body should be the conclusion + rationale (compress; do not paste raw discussion, and keep PII out). The project defaults to the bound project.", "新しい決定を記録します（議論中）── 追記のみの「なぜ X を選んだか」。決定はタスクの兄弟（プロジェクト単位）であってタスクではありません: 受信箱のワークフローを持たず、タスク一覧には現れません。決定は端末で一意の番号空間を持ち AMB-D-N で表示されます（タスクは AMB-T-N）。種別コードで AMB-D-<n> / AMB-T-<n> が曖昧になりません。本文は結論＋根拠にします（圧縮する・議論の生ログを貼らない・PII を入れない）。プロジェクトは紐付いたプロジェクトを既定にします。"),
    ("Lists decisions (status:proposed|accepted|rejected, current:yes|no — being superseded is not a status but a derived view, so current:no lists the decisions another decision supersedes — text: over title+body, project:, decided_before:/decided_after: over the day a decision was accepted (YYYY-MM-DD, or today/-30d; both ends inclusive; a decision that was never accepted has no such day and matches neither)). To ask which policies were settled by a date, compose this filter with current: — there is no separate as-of switch, and the composition recovers neither status transitions nor deleted decisions. Sort by decided/created/number/title/status (prefix - for descending; default -created). --limit/--offset page (JSON carries total_matched = the count before paging, count = this page). --with-body adds each decision's body to the rows — a projection that composes with --filter/--limit/--offset (narrow by keywords/status and page; it does not dump the whole corpus).", "決定記録を一覧します（status:proposed|accepted|rejected、current:yes|no ── 「置換された」は status ではなく派生ビューです。current:no ＝ 別の決定に置き換えられた決定、text: はタイトル＋本文が対象、project:、decided_before:/decided_after: は採択された日で絞ります〔YYYY-MM-DD または today/-30d・両端を含む。採択されていない決定はその日を持たず、どちらにも一致しません〕）。「T までに決まっていた方針」は、このフィルタと current: の合成で得ます ── 専用の as-of スイッチはありません。合成でもステータス遷移の履歴や削除された決定は辿れません。並び替えは decided/created/number/title/status（先頭 - で降順・既定 -created）。--limit/--offset はページングします（JSON は total_matched = ページング前の件数、count = このページの件数を持ちます）。--with-body は各決定の本文を行に足します ── --filter/--limit/--offset と合成できる射影です（キーワード/状態で絞ってページングする。コーパス全体を吐き出しはしません）。"),
    ("Shows a decision: body, status, the supersession chain (both directions), the premises it builds on (read those first — a premise another decision has overturned is flagged, because this decision then stands on rotten ground) and the decisions that build on it (the impact radius: overturn this one and they want revisiting), and its linked tasks.", "決定記録を表示します: 本文、状態、置換チェーン（両方向）、前提にしている決定（先に読むべき決定。別の決定に覆された前提には印が付きます ── その決定は腐った前提の上に立っています）、この決定を前提にする決定（影響半径: これを覆すなら見直しが要る決定）、関連タスク。"),
    ("Records that a decision builds on (takes as a premise) an existing one: the standing decision draws a `builds_on` edge at the premise, which stays current and is not corrected — the edge only says read the premise first, and revisit this decision if the premise is ever overturned. Draw it only when that revisiting test says yes: same topic, cited in the body, or merely consulted is not a premise. supersedes / amends already imply it, so drawing it on a pair that carries one of them is a no-op (one pair, one edge). The reverse lookup is the impact radius `decision supersede` / `reject` / `delete` show you.", "ある決定が既存の決定を**前提にしている**ことを記録します: 立っている側が `builds_on` エッジで前提を指しますが、前提は現行のままで訂正もされません ── このエッジが言うのは「前提を先に読め」と「前提が覆されたらこの決定を見直せ」の 2 つだけです。張るのは、その見直しテストが YES のときだけにします: 同じテーマだから／本文で言及したから／参考にしたから、は前提ではありません。supersedes / amends は builds_on を含意するので、それらが立っているペアに張るのは no-op です（1 ペア 1 エッジ）。逆引きが、`decision supersede` / `reject` / `delete` が提示する影響半径です。"),
    ("the decision that stands on the premise", "前提の上に立っている側の決定"),
    ("the premise it stands on (stays current)", "その決定が前提にしている決定（現行のまま）"),
    ("Edits a decision's title/body in place — proposed or accepted alike. Editing is not re-deciding, so an accepted decision's decided_at/decided_by are left untouched, and there is no revision history. Supersede when a new decision replaces it; a rejected decision is terminal and cannot be edited.", "決定のタイトル／本文をその場で編集します ── proposed も accepted も同じ。編集は再決定ではないので、採択済みの decided_at/decided_by は触らず、リビジョン履歴も持ちません。新しい決定が置き換えるときは supersede します。却下済みは終端で編集できません。"),
    ("Accepts a decision (proposed → accepted); stamps decided_at/decided_by. --reason records the reason for accepting as a decision comment — the reason lives on the timeline, not in a dedicated field.", "決定を採択します（proposed → accepted）。decided_at/decided_by を刻みます。--reason は採択の理由を決定コメントとして記録します── 理由は専用フィールドでなくタイムラインに残ります。"),
    ("Rejects a decision (proposed → rejected). --reason records the reason for rejecting as a decision comment — the reason lives on the timeline, not in a dedicated field.", "決定を却下します（proposed → rejected）。--reason は却下の理由を決定コメントとして記録します── 理由は専用フィールドでなくタイムラインに残ります。"),
    ("Un-settles an accepted decision back to proposed (accepted → proposed; clears decided_at/decided_by), and sends the tasks that rest on it back to ready:no. Use it to pull a too-hastily accepted decision back into debate — neither reject (a verdict) nor supersede (a replacement) says that. It is not needed to edit: an accepted decision edits in place. No-op if already proposed; refused for rejected decisions. A decision another one supersedes stays accepted (currency is derived, not a status), so it can be reopened too.", "採択済みの決定を proposed へ差し戻します（accepted → proposed・decided_at/decided_by をクリア）。その決定に立つタスクは ready:no へ戻ります。早すぎた採択を議論へ戻す用途 ── reject（裁定）でも supersede（置換）でも表せません。編集には要りません: 採択済みもその場で編集できます。すでに proposed なら何もしません。rejected には拒否します。覆された決定は accepted のまま（現行性は派生で status ではない）なので reopen できます。"),
    ("Deletes (retires) a decision — accepted ones included. The delete is physical and irreversible, its comments and edges go with it, and linked tasks are unlinked. Use this to retire a decision outright; use supersede when a new decision replaces it (which keeps the old one readable).", "決定を削除（退役）します ── 採択済みも含みます。削除は物理削除で不可逆。コメントとエッジも一緒に消え、関連タスクは結びを外されます。決定をきっぱり退役させるときに使い、新しい決定が置き換えるときは supersede を使います（そちらは元の決定を読める形で残します）。"),
    ("Records that a new decision replaces an existing one (supersession chain): the new one is accepted and draws a `supersedes` edge at the old one, which stops being current (the old row itself is not touched — currency is derived from the edge, not stored). A decision may supersede several others — each supersede draws its own edge, none replaces the last.", "新しい決定が既存を置き換えることを記録します（置換チェーン）: 新しい方が採択され `supersedes` エッジで古い方を指し、古い方は現行でなくなります（古い方の行は触りません ── 現行性はエッジからの派生で、保存しません）。1 つの決定が複数を置き換えられます（supersede のたびにエッジが 1 本増え、前のエッジを消しません）。"),
    ("Records that a new decision amends (partially revises) an existing one: the new one draws an `amends` edge at the old one, which stays current (not superseded) — read the two together. A decision may amend several others (one edge each). Amend only records the revision link; it does not change either side's status (the amending side stays proposed until you accept it separately). Use supersede when the new decision fully replaces the old one.", "新しい決定が既存を一部改訂することを記録します: 新しい方が `amends` エッジで古い方を指しますが、古い方は現行のまま（superseded にならない）── 2 つを併せて読みます。1 つの決定が複数を改訂できます（1 本ずつエッジが増えます）。amend は改訂リンクを記録するだけで、どちらの status も変えません（改訂する側は proposed のままで、accept は別途行います）。新しい決定が古い方を完全に置き換えるときは supersede を使います。"),
    ("Undo a decision-to-decision edge drawn at the wrong target", "宛先を間違えて張った決定間エッジを外す"),
    ("the decision the edge was drawn from (the newer one)", "エッジを張った側の決定（新しい方）"),
    ("the decision it points at (the older one)", "指されている側の決定（古い方）"),
    ("Removes a decision-to-decision edge that should never have been drawn (supersedes / amends / builds_on alike — a pair carries one edge, so naming the pair names it). This is a correction, not a reversal of the decision: superseding a decision back is a new decision, whereas an edge drawn at the wrong target is a miswiring with nothing to remember. Removing a `supersedes` edge makes its target current again on its own (currency is derived from the edges, not stored). No-op when the pair carries no edge.", "決定間のエッジを外します（誤って張った配線の訂正・supersedes / amends / builds_on のいずれも同じ 1 本で外れます ── 1 ペアに立つエッジは 1 本なので、ペアを指せばエッジが決まります）。これは決定の取り消しではありません: 覆した決定を覆し返すのは新しい決定ですが、宛先を間違えて張ったエッジは配線ミスであり、歴史として残す価値はありません。`supersedes` を外せば、対象は自動的に現行へ戻ります（現行性はエッジからの派生で、保存フラグではありません）。エッジの無いペアを外すのは no-op です。"),
    ("Links (or --unlink) a decision and a task — the decision is the task's premise (many-to-many). The edge is a precondition, not a mere cross-reference: the task cannot be reserved until the decision is settled (accepted and current), and a proposed or rejected premise, or one another decision supersedes, rejects the reserve with not_ready until it is ruled on, unlinked, or relinked to the successor. So link implementation tasks only — a purely historical reference belongs in the decision's body, not in an edge, and linking a decision to the task of deciding it locks that task forever. The task must sit in the decision's own project (no edge crosses projects; an inbox task, belonging to none, is not a crossing).", "決定記録とタスクを結びます（または --unlink）── その決定はタスクの根拠（前提）です（多対多）。エッジは単なる相互参照ではなく前提条件です: 決定が確定する（accepted かつ現行）まで、そのタスクは予約できません。proposed / rejected の根拠、および別の決定に置き換えられた根拠は、裁定されるか、link を外すか、後継へ張り替えるまで、予約を not_ready で弾きます。したがって結ぶのは実装タスクだけにします ── 単に歴史として参照したいならエッジでなく決定の本文に書きます。決定そのものを裁定するタスクに、その決定を結ぶと永久に着手できなくなります。タスクはその決定と同じプロジェクトに居ること（エッジはプロジェクトを跨がない。どの PJ にも属さない受信箱のタスクは跨ぎになりません）。"),
    ("Promotes a task comment into a decision: the comment text becomes the body, the project defaults to the comment's task project, and the new decision is linked to that task.", "タスクのコメントを決定記録へ昇格します: コメント本文が本文になり、プロジェクトはコメントのタスクのプロジェクトを既定にし、新しい決定はそのタスクに結ばれます。"),
    ("Attaches a file or external link to a task. WHAT TO ATTACH: an attachment's bytes are not searchable — `task list --filter text:` runs over title / notes / comment bodies only — so text is always the body's job, and attaching is giving up on ever finding it again. The test: if the content holds a word you might one day search for, that word stays in the body. An attachment is not where words go to disappear; it is where the backing evidence sits. Attach non-text you generated yourself — the screenshot of a GUI check (so the next session can see what you verified instead of taking your word for it), images, video, PDF, binary samples; they could never be searched anyway, so nothing is lost, but write one line in the body saying what is in it. Attach long raw data behind a conclusion — a failing run's log, a profile, a before/after benchmark: keep the conclusion and the fragments worth searching (the error line, the identifier) in the body, and attach only the raw data. Do not attach text that fits in the body (short output, a minimal repro), source or diffs (anchor those with `task commit add`), or reasoning and history (that is a decision record). A URL belongs in the body; `--url` adds a click path for the human GUI, it does not replace writing the link down. MECHANICS: a file is ingested as a content-addressed `blob` (the bytes are copied into the store keyed by their BLAKE3 digest; the truth source records only metadata — hash/filename/mime/size); --url instead records an external link (`url` mode, not managed). MIME is guessed from the file extension. The blob is checked against the per-file size cap before ingest. Manage attachments with `attach ls/show/open/rm`.", "タスクにファイルや外部リンクを添付します。【何を添付するか】添付のバイト列は検索されません ── `task list --filter text:` が走るのは title / notes / コメント本文だけ ── なので、テキストは常に本文の仕事であり、添付するとは二度と見つけられなくなることを受け入れることです。判定: その内容に、将来検索するかもしれない語が含まれるなら、その語は本文に残します。添付は語を消す場所ではなく、裏付けを置く場所です。添付するのは自分が生成した非テキスト ── GUI 検証のスクリーンショット（次のセッションがあなたの言葉を信じる代わりに、検証した画面そのものを見られます）、画像、動画、PDF、バイナリ標本。そもそも検索に載り得ないので失うものはありませんが、何が写っているかの一行は本文に書きます。結論の裏にある長い生データも添付します ── 失敗した実行のログ、プロファイル、前後のベンチマーク: 結論と検索に値する断片（エラー行、識別子）は本文に残し、生データだけを添付します。添付しないのは、本文に収まるテキスト（短い出力・最小 repro）、ソースと diff（アンカーは `task commit add`）、理由と経緯（それは決定記録です）。URL は本文に書くもので、`--url` は人間の GUI にクリック導線を足すだけであり、本文に書くことの代わりにはなりません。【機構】ファイルは内容アドレスの `blob` として取り込まれます（バイト列は BLAKE3 ダイジェストをキーにストアへ複製され、真実源はメタデータ ── ハッシュ/ファイル名/mime/サイズ ── だけを記録）。--url は代わりに外部リンクを記録します（`url` モード・管理下ではない）。MIME はファイル拡張子から推測します。blob は取り込み前にファイルごとのサイズ上限で検査されます。添付は `attach ls/show/open/rm` で管理します。"),
    ("Attaches a file (content-addressed `blob`) or external link (--url) to a decision record. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Manage with `attach ls/show/open/rm`.", "決定記録にファイル（内容アドレスの `blob`）や外部リンク（--url）を添付します。`task attach` と同じ 2 モードで、何を添付するかの判断も同じです ── そちらで読んでください。`attach ls/show/open/rm` で管理します。"),
    ("Lists the attachments on a task, decision, or a single comment, in attach order. WHETHER TO OPEN ONE: decide from the metadata this prints — name, mime, size — and from what the body already told you. The body carries the conclusion and the searchable words (see `task attach`); an attachment is the backing evidence behind them, so most of the time the listing alone answers your question and reading the bytes only spends context. Open one when you actually need the evidence — the body's claim is what you must check, or the raw data is what you were sent for — and when in doubt, do not: an attachment you read and did not need has cost you the very context it was put there to save. To read one, `attach save --out <path>` writes the bytes to a file you can open (`attach open` hands it to the OS's default opener, which is the human's route, not yours). A comment is named by a flag, not by the positional target: the task and decision comment tables number apart, so a bare id cannot say which table it belongs to.", "タスク・決定記録・単一コメントの添付を、添付順で一覧します。【開くかどうか】ここが表示するメタデータ ── 名前・mime・サイズ ── と、本文がすでに語っていることで判断します。本文は結論と検索語を持ち（`task attach` 参照）、添付はその裏付けです。したがって多くの場合は一覧だけで答えが出ますし、バイト列を読むのはコンテキストを使うだけです。開くのは裏付けが実際に要るとき ── 本文の主張そのものを検証したい、あるいは生データこそが取りに来たもの ── に限り、迷うなら開かないこと: 読んで不要だった添付は、それが節約するために置かれたはずのコンテキストを食っています。読むには `attach save --out <path>` でバイト列をファイルに落とします（`attach open` は OS の既定アプリに渡すもので、人間の経路であってあなたの経路ではありません）。コメントは位置引数ではなくフラグで指します: タスクと決定記録のコメント表は別々に採番するので、裸の id ではどちらの表のものか言えません。"),
    ("list this task comment's attachments (id from `comment list`)", "このタスクコメントの添付を一覧します（id は `comment list` で分かります）"),
    ("list this decision comment's attachments (id from `decision comment list`)", "この決定記録コメントの添付を一覧します（id は `decision comment list` で分かります）"),
    ("target task comment ref, AMB-C-n (from `comment list`)", "対象のタスクコメント参照 AMB-C-n（`comment list` で分かります）"),
    ("target decision comment ref, AMB-C-n (from `decision comment list`)", "対象の決定記録コメント参照 AMB-C-n（`decision comment list` で分かります）"),
    ("Shows one attachment's metadata (kind, filename, mime, size, blob hash or url).", "1 件の添付のメタデータ（種類、ファイル名、mime、サイズ、blob ハッシュまたは url）を表示します。"),
    ("Opens an attachment — a blob via the OS default opener, or the external URL. This puts it in front of the human at their screen; an agent reads an attachment with `attach save` instead. A blob whose bytes are not present locally reports not_found.", "添付を開きます ── blob は OS の既定オープナで、外部 URL はそのまま。これは画面の前に居る人間に見せるための口です。エージェントが添付を読むときは代わりに `attach save` を使います。バイト列がローカルに無い blob は not_found を返します。"),
    ("Saves a blob attachment's bytes to a file — the CLI counterpart of the GUI's download (`open` only spills to a temp file, and `export` takes the whole store), and the way an agent reads an attachment: save it, then read the file. Decide whether it is worth reading before you save it, from `attach ls`'s metadata — the bytes land in your context and rarely repay it. `--out` is a file path, or an existing directory to save under the attachment's own filename; with no `--out` that filename lands in the current directory. Refuses to overwrite an existing destination unless `--force`. A URL attachment has no bytes to save (open the link with `attach open`); a blob whose bytes are not present locally reports not_found.", "blob 添付のバイト列をファイルへ保存します ── GUI のダウンロードに対応する CLI 側の口であり（`open` は一時ファイルへ吐くだけ、`export` はストア全体を出します）、エージェントが添付を読む経路でもあります: 保存してから、そのファイルを読みます。読む価値があるかは保存する前に `attach ls` のメタデータで判断してください ── バイト列はあなたのコンテキストに載り、それに見合うことは稀です。`--out` はファイルパス、または既存ディレクトリ（その中に添付自身のファイル名で保存）。`--out` を省くとそのファイル名でカレントディレクトリに置きます。既存の保存先は `--force` が無い限り上書きしません。URL 添付は保存するバイト列を持ちません（`attach open` でリンクを開きます）。バイト列がローカルに無い blob は not_found を返します。"),
    ("file path, or a directory to save under the attachment's filename (default: that filename in the CWD)", "ファイルパス、または添付のファイル名で保存するディレクトリ（既定はそのファイル名でカレントディレクトリ）"),
    ("overwrite the destination if it exists (default refuses)", "保存先が既にあれば上書きします（既定は拒否）"),
    ("Removes an attachment — permanently. The blob bytes are reclaimed with the attachment once nothing else references them (content-addressing means another attachment may share the same bytes — those are left alone). Bytes ingested within the last hour are kept for now, in case an attach is in flight elsewhere; the sweep in `doctor --fix` collects them later. Destructive — confirms unless --yes.", "添付を取り除きます ── 完全に消えます。blob のバイト列は、他に参照が無くなっていればこの削除と同時に回収されます（content-address なので同じバイト列を別の添付が指していることがあり、その場合は残します）。取り込みから 1 時間以内のバイト列は、他所で進行中の attach かもしれないので保留し、`doctor --fix` の全走査が後から回収します。破壊的 ── --yes が無ければ確認します。"),
    ("Exports all data — everything on this device, as JSON, and nothing narrower: export exists for moving to another tool, which an excerpt or a human-readable table does not serve. The core of data sovereignty, and one way: amenbo writes your data out for whatever you move to next, and reads nothing back in — the way back is `restore` from a `backup` archive. `--out <dir>` writes an **export directory**: `export.json` plus `attachments/`, holding every attachment's actual file under the task or decision it hangs on (each row names its `export_path`). With no `--out` the same JSON streams to stdout — a stream has nowhere to put the files, so that shape carries records only.", "全データをエクスポートします ── この端末のすべてを JSON で、それより狭い形は持ちません: export は他ツールへ移るためのもので、抜粋も人間向けの表もその役に立たないからです。データ主権の核であり、**片道**です: amenbo はあなたのデータを次に使うツールのために書き出すだけで、読み戻す口は持ちません ── 戻す道は `backup` アーカイブからの `restore` です。`--out <dir>` は**エクスポート先ディレクトリ**を作ります: `export.json` と `attachments/` ── 添付の実ファイルを、付いていたタスクや決定記録ごとに並べます（各行が自分の `export_path` を名乗ります）。`--out` 無しなら同じ JSON を標準出力へ流します ── ストリームには実ファイルの置き場が無いので、レコードだけを運びます。"),
    ("Backs up everything on this device — one database, holding every project — into one verified `.amenbo-backup` archive at the given path (VACUUM INTO: checkpointed, transactionally consistent, no torn DB+WAL; bounded-verified; the manifest records its migration generation). The attachment bytes (blobs) are bundled too, so a restore elsewhere brings the files back and not just the rows referencing them. Secrets (keys/identity) are not part of the engine, so none are included. The destination must not already exist (managed generation rotation is retired).", "この端末を、検証済みの 1 つの `.amenbo-backup` アーカイブへバックアップします（この端末のデータ＝データベース 1 つ＝全プロジェクトです。VACUUM INTO ＝チェックポイント済みで一貫・DB+WAL の破れなし・有界検証、manifest にマイグレーション世代を記録）。添付の実バイト（blob）も同梱するので、他の端末で復元しても参照行だけでなくファイル本体が戻ります。秘密（鍵/識別情報）はエンジンの一部でないので含みません。既存でない宛先が必須です（管理世代ローテーションは退役）。"),
    ("Restores this device from a verified `.amenbo-backup` archive at the given path — a destructive replace of the database the archive carries (all-or-nothing stage-and-swap; the replaced truth source is set aside with a timestamp; an archive newer than this build is refused — update first). The snapshot is validated before anything is swapped in, so an unusable archive is refused without harm. The archive's attachment bytes (blobs) are placed additively — a blob the machine already holds is left alone, and none are ever deleted. An archive written before the consolidation carries the pre-consolidation shape (a list of stores) and is refused whole by its layout version, before its manifest is even parsed, rather than partially applied: restore it with the build that wrote it. Destructive — confirms unless --yes.", "`.amenbo-backup` アーカイブからこの端末を復元します ── アーカイブが含むデータベースで破壊的に総入れ替えします（all-or-nothing の stage-and-swap・置き換えた旧真実源はタイムスタンプ付きで退避・ビルドより新しいアーカイブは拒否＝先に更新）。差し込み前にスナップショットを検証するので、使えないアーカイブは無害に拒否されます。添付の実バイト（blob）は加算的に配置します ── 既に在るものはそのまま・消すことはありません。統合前に書かれたアーカイブは旧い形（ストアの配列）なので、manifest を読む前にレイアウト版で拒否します── 部分適用はせず、書き出した当時のビルドで復元してください。破壊的 ── --yes が無ければ確認します。"),
    ("Physically erases one or more task comments from this store's truth source — deletes the read-model row outright — then VACUUMs so the bytes leave the file (unrecoverable). An ordinary delete removes the row — and `comment rm` deletes a comment posted by mistake — but the freed pages keep their bytes readable until something reclaims them, so this is the deliberate, gated exception: use it for content that must be GONE from the file. Identify comments by id; find ids with `comment list <task> --json`. Human-gated maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the erased content — delete it after verifying.", "このストアの真実源から 1 件以上のタスクコメントを物理的に消去します ── 読み取りモデルの行をきっぱり削除し ── その後 VACUUM してバイト列をファイルから抜きます（復元不可）。通常の削除でも行は消え（誤投稿は `comment rm`）ますが、空いたページのバイト列は回収されるまで読めるままです。これはそこまで行う意図的でゲート付きの例外です: ファイルから消え去るべき内容に使います。コメントは id で指定します。id は `comment list <task> --json` で見つけます。人間ゲートの保守作業: まず安全スナップショットを取り、--yes が無ければ確認し、AI アクターには拒否されます（人間が実行する必要）。安全スナップショットにはまだ消去した内容が残るので、確認後に削除してください。"),
    ("Redacts an accepted decision's body: overwrites it with the given text in place (the prior body is physically replaced, not merely superseded), then VACUUMs — so one section can be removed while the decision keeps its number, links and other fields. The replacement body comes from --body, --body-file, or stdin. Destructive maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the old body — delete it after verifying.", "採択済み決定の本文を墨消しします: 与えたテキストでその場を上書きし（以前の本文は supersede でなく物理的に置換）、その後 VACUUM します ── これで決定が番号・結び・他フィールドを保ったまま、1 節だけ除けます。置換本文は --body、--body-file、または標準入力から取ります。破壊的な保守作業: まず安全スナップショットを取り、--yes が無ければ確認し、AI アクターには拒否されます（人間が実行する必要）。安全スナップショットにはまだ古い本文が残るので、確認後に削除してください。"),
    // flags.help / args.help (the help text of flags and arguments)
    ("machine-readable output", "機械可読な出力"),
    ("machine-readable output (recommended)", "機械可読な出力（推奨）"),
    ("print one command's full spec (flags, args, examples) instead of the entry point", "入口の代わりに、1 コマンドの完全仕様（フラグ・引数・実行例）を出す"),
    ("print every command's full spec inline (scripts / verification)", "全コマンドの完全仕様をその場に並べる（スクリプト／検証用）"),
    ("machine-readable output — { count, cursor, has_more, items }", "機械可読な出力 ── { count, cursor, has_more, items }"),
    ("config key", "設定キー"),
    ("config value", "設定値"),
    ("the first local user name (at initial genesis)", "最初のローカルユーザー名（初回生成時）"),
    ("sets the user language (ja/en etc.) in the global config and embeds it in AGENTS.md", "グローバル設定にユーザー言語（ja/en など）を設定し、AGENTS.md に埋め込みます"),
    ("create a new project and overwrite even if a .amenbo already exists (default rejects clobbering)", "すでに .amenbo があっても新しいプロジェクトを作って上書きします（既定は上書きを拒否）"),
    ("project ID to bind (omit to show)", "紐付けるプロジェクト ID（省略で表示）"),
    ("bind even inside an already-managed tree (a parent has a .amenbo); default rejects to avoid shadowing the root pointer", "すでに管理下のツリー内（親が .amenbo を持つ）でも紐付けます。既定はルートポインタを覆い隠さないよう拒否します"),
    ("place the .amenbo pointer in this existing directory instead of the current one (bind a folder from outside it, git -C style)", "現在のフォルダでなく、この実在ディレクトリに .amenbo ポインタを置きます（そのフォルダの外から紐付ける・git -C 方式）"),
    ("folder to unbind (defaults to the current directory)", "紐付けを外すフォルダ（既定は現在のディレクトリ）"),
    ("scope (default today)", "範囲（既定は今日）"),
    ("this task only", "このタスクのみ"),
    ("only tasks belonging to this project", "このプロジェクトに属するタスクのみ"),
    ("a date (today / +3d / YYYY-MM-DD) reads history on/after it, newest-first; an opaque cursor from a prior response reads only what is strictly newer, oldest-first (incremental) — pass the response's cursor to resume where you left off", "日付（today / +3d / YYYY-MM-DD）はその日以降の履歴を新しい順で読みます。以前の応答の不透明なカーソルは厳密により新しいものだけを古い順で読みます（差分）── 応答のカーソルを渡すと続きから再開できます"),
    ("filter by which stream an item came from: `system` for the events amenbo stamps itself, `comment` for what a facet wrote. Distinct from the `kind` a system item carries in its payload, which names the event — task.created / status_changed / assigned / moved / deleted", "どちらの流れから来た項目かで絞り込みます: `system` は amenbo 自身が刻むイベント、`comment` は facet が書いたもの。system の項目が payload に持つ `kind`（イベント名 ── task.created / status_changed / assigned / moved / deleted）とは別物です"),
    ("filter by the issuer's facet (a read filter separate from the global --actor)", "発行者の facet で絞り込む（グローバルの --actor とは別の読み取りフィルタ）"),
    ("narrow to what a facet should act on: activity on tasks assigned to that facet (destination axis; me = your own facet). Distinct from --by, which filters by who issued the event", "その facet が対応すべきものに絞ります: その facet に割り当てられたタスクのアクティビティ（宛先の軸・me は自分の facet）。イベントを発行した人で絞る --by とは別物です"),
    ("max count (history: newest-first window; incremental: oldest items after the cursor). has_more marks when the window was cut", "最大件数（履歴: 新しい順の窓・差分: カーソル以降の古いものから）。has_more は窓が切られたことを示します"),
    ("number of items to skip (newest first; paging / going back through history)", "スキップする件数（新しい順・ページング／履歴を遡る）"),
    ("repair fixable problems (reclaim unreferenced attachment files, forget folder bindings no live project claims; both are non-destructive)", "直せる問題を修復します（参照の無い添付ファイルの実体を回収し、生きたプロジェクトが誰も主張しないフォルダ紐付けを索引から忘れます。どちらも非破壊です）"),
    ("skip the --fix confirmation", "--fix の確認をスキップします"),
    ("ID(s) to check (multiple allowed)", "点検する ID（複数可）"),
    ("machine-readable output (issues carry a fix_hint)", "機械可読な出力（問題は fix_hint を持ちます）"),
    ("project name (required, non-empty)", "プロジェクト名（必須・空不可）"),
    ("default view (default list)", "既定ビュー（既定は list）"),
    ("description (Markdown)", "説明（Markdown）"),
    ("description (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "説明（Markdown）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("color", "色"),
    ("include archived ones too", "アーカイブ済みも含める"),
    ("project ID", "プロジェクト ID"),
    ("before the given ID", "指定 ID の前"),
    ("after the given ID", "指定 ID の後"),
    ("to the top", "先頭へ"),
    ("to the bottom", "末尾へ"),
    ("skip confirmation", "確認をスキップ"),
    ("owning project (defaults to the bound project; an AI omits it)", "所有プロジェクト（既定は紐付いたプロジェクト。AI は指定しません）"),
    ("dimension name", "次元名"),
    ("description / notes (Markdown)", "説明／メモ（Markdown）"),
    ("give the values an explicit order", "値に明示的な順序を与える"),
    ("mark as the time axis (an ordered view lane)", "時間軸として印を付ける（順序付きのビューレーン）"),
    ("whether the values carry an explicit order", "値が明示的な順序を持つか"),
    (
        "name this axis the project's time axis (its values then carry periods), or unname it",
        "この軸をプロジェクトの時間軸に指名する（以後その値が期間を持つ）／指名を解く",
    ),
    ("target project (defaults to the bound project; an AI omits it)", "対象プロジェクト（既定は紐付いたプロジェクト。AI は指定しません）"),
    ("dimension id or name", "次元の id または名前"),
    ("new name", "新しい名前"),
    ("new notes (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "新しいメモ（Markdown）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("value name", "値名"),
    ("value id or name (within the dimension)", "値の id または名前（次元内）"),
    ("task ref (AMB-T-n)", "タスク参照（AMB-T-n）"),
    ("owning project (a project-less task is refused). A human must name one — omit it to list the existing projects. An AI does not: the binding fills the slot, and naming a project is refused", "所有プロジェクト（プロジェクトの無いタスクは拒否します）。人間は必ず指定します ── 省略すると既存プロジェクトを一覧します。AI は指定しません: 束縛が置き場を埋め、プロジェクトの名指しは拒否されます"),
    ("title (required, non-empty)", "タイトル（必須・空不可）"),
    ("due date (YYYY-MM-DD / today / +3d)", "期日（YYYY-MM-DD / today / +3d）"),
    ("start date", "開始日"),
    ("priority", "優先度"),
    ("delegate at creation to a facet — a name / `me` / `human` (the human), or `me-ai` / `ai` (the human's AI); same as a follow-up task assign, saving the create+assign round trip", "作成時に facet へ委任します ── 名前 / `me` / `human`（人間）、または `me-ai` / `ai`（人間の AI）。後続の task assign と同じで、作成＋割り当ての往復を省きます"),
    ("with --to, delegate to 'that person's AI' (assignee_kind=ai)", "--to と併せて「その人の AI」に委任します（assignee_kind=ai）"),
    ("filter by project (human only — an AI is already scoped to its bound project)", "プロジェクトで絞り込む（human 専用 ── AI は束縛プロジェクトに閉じています）"),
    ("filter expression (see filterGrammar)", "フィルタ式（filterGrammar 参照）"),
    ("sort (order/due/priority/created/title; prefix - for descending)", "並び替え（order/due/priority/created/title・先頭 - で降順）"),
    ("max count (in sort order; pairs with --offset for paging)", "最大件数（ソート順・--offset と組でページング）"),
    ("number of items to skip in sort order (paging)", "ソート順でスキップする件数（ページング）"),
    ("task ID", "タスク ID"),
    ("the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and revisions are refused)", "完全形のコミット SHA ── SHA-1 は 40 桁・SHA-256 は 64 桁の hex（短縮形・ブランチ・タグ・revision は拒否）"),
    ("the commit SHA to forget (any case — normalised the way it was stored)", "忘れるコミット SHA（大小文字を問わない ── 保存時と同じに正規化されます）"),
    ("clear the due date", "期日をクリアする"),
    ("clear the start date", "開始日をクリアする"),
    ("clear the priority", "優先度をクリアする"),
    // Deliberately identical: strings we chose not to translate (the intent is declared in the
    // tests' JA_VERBATIM).
    ("todo / in_progress / done / blocked", "todo / in_progress / done / blocked"),
    ("reason it is stuck (recorded as a comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "行き詰まった理由（コメントとして記録）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("destination project (omit it to reorder within the current project). An AI cannot re-home a task — every other project is outside its reach", "移動先プロジェクト（省略すると現在のプロジェクト内で並び替えます）。AI はタスクを別プロジェクトへ移せません ── 他のプロジェクトは到達範囲の外です"),
    ("the task ID being blocked", "ブロックされる側のタスク ID"),
    ("the task ID of the blocker that must be done first", "先に完了すべきブロッカーのタスク ID"),
    ("the blocker task ID to remove", "取り除くブロッカーのタスク ID"),
    ("assignee facet: `me` / `self` / `human` or the human's display name → the human; `me-ai` / `ai` → the human's AI. The account-id / public-key forms are gone with the account reference dimension.", "担当 facet: `me` / `self` / `human` または人間の表示名 → 人間、`me-ai` / `ai` → 人間の AI。account-id / 公開鍵の形式はアカウント参照次元とともに廃止されました。"),
    ("delegate to 'that person's AI' (assignee_kind=ai)", "「その人の AI」に委任します（assignee_kind=ai）"),
    ("target task ID", "対象タスク ID"),
    ("comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "コメント本文（Markdown）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("max count (oldest first; pairs with --offset for paging)", "最大件数（古い順・--offset と組でページング）"),
    ("number of items to skip, oldest first (paging)", "スキップする件数・古い順（ページング）"),
    ("target comment ref, AMB-C-n (from `comment list`)", "対象コメント参照 AMB-C-n（`comment list` で分かります）"),
    ("target comment ref, AMB-C-n (from `decision comment list`)", "対象コメント参照 AMB-C-n（`decision comment list` で分かります）"),
    ("file path to ingest as a blob, or the external URL with --url", "blob として取り込むファイルパス、または --url での外部 URL"),
    ("treat <source> as an external URL link instead of ingesting a file", "<source> をファイル取り込みでなく外部 URL リンクとして扱う"),
    ("display label (defaults to the file name / URL)", "表示ラベル（既定はファイル名／URL）"),
    ("decision title", "決定のタイトル"),
    ("conclusion + rationale (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "結論＋根拠（Markdown）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("project (name or ID; defaults to the bound project — an AI omits it, and naming one is refused)", "プロジェクト（名前か ID・既定は紐付いたプロジェクト ── AI は指定せず、名指しは拒否されます）"),
    ("limit to the given project (human only)", "指定プロジェクトに限定する（human 専用）"),
    ("e.g. status:accepted text:sync", "例: status:accepted text:sync"),
    ("decided/created/number/title/status (- for descending; default -created)", "decided/created/number/title/status（- で降順・既定 -created）"),
    ("include each decision's body (projection; composes with --filter/--limit/--offset)", "各決定の本文を含める（射影・--filter/--limit/--offset と合成可能）"),
    ("decision ref (AMB-D-n)", "決定参照（AMB-D-n）"),
    ("target decision ref (AMB-D-n)", "対象の決定参照（AMB-D-n）"),
    ("reason for accepting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "採択の理由（決定コメントとして記録します）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("reason for rejecting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "却下の理由（決定コメントとして記録します）。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("new title", "新しいタイトル"),
    ("new body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).", "新しい本文。`-` を渡すと本文を標準入力から読みます（引用符付きの引数からはシェルがコードスパンを食います）"),
    ("skip the confirmation prompt", "確認プロンプトをスキップします"),
    ("the new decision (it replaces the old one)", "新しい決定（古い方を置き換える）"),
    ("the decision being replaced", "置き換えられる決定"),
    ("the new decision (it amends the old one)", "新しい決定（古い方を一部改訂する）"),
    ("the decision being amended (stays current)", "一部改訂される決定（現行のまま）"),
    ("decision ref (AMB-D-n)", "決定参照（AMB-D-n）"),
    ("task ref (AMB-T-n)", "タスク参照（AMB-T-n）"),
    ("remove the link instead of creating it", "結びを作る代わりに取り除く"),
    ("the comment ref to promote, AMB-C-n", "昇格するコメント参照 AMB-C-n"),
    ("project (defaults to the comment's task project)", "プロジェクト（既定はコメントのタスクのプロジェクト）"),
    ("target task ref (AMB-T-n)", "対象タスク参照（AMB-T-n）"),
    ("file path to ingest, or the external URL with --url", "取り込むファイルパス、または --url での外部 URL"),
    ("target decision ref (AMB-D-n)", "対象決定参照（AMB-D-n）"),
    ("task / decision ref (AMB-T-n / AMB-D-n — the kind code is what disjoins the two spaces)", "タスク／決定参照（AMB-T-n / AMB-D-n。2 つの空間を分けるのは種別コード）"),
    ("attachment id", "添付 id"),
    ("the export directory to create (must not exist yet). Default: stream to stdout", "作るエクスポート先ディレクトリ（既存でないこと）。既定は標準出力へのストリーム"),
    ("destination .amenbo-backup archive that must not already exist", "既存でない宛先の .amenbo-backup アーカイブ"),
    ("the .amenbo-backup archive to restore from (must exist and pass verification)", "復元元の .amenbo-backup アーカイブ（存在し検証を通ること）"),
    ("comment ref(s) to erase, AMB-C-n", "消去するコメント参照 AMB-C-n"),
    ("decision reference (AMB-D-n)", "決定参照（AMB-D-n）"),
    ("replacement body (Markdown); omit to use --body-file or stdin", "置換本文（Markdown）。省略すると --body-file または標準入力を使います"),
    ("read the replacement body from this file instead of --body/stdin", "--body/標準入力の代わりにこのファイルから置換本文を読みます"),
    // plugin validate (author-facing manifest check)
    ("Validate a plugin manifest against the catalog rules before submitting it (an author's self-check)", "提出前にプラグインの manifest を目録の規約に照らして検証する（作者の自己チェック）"),
    ("Validates a plugin manifest file against the catalog rules — a well-formed id, checksum, https url, repo, non-empty OS set and config schema — reporting every problem it finds so an author can self-check before opening a catalog PR. It reads the same rules amenbo enforces at the install/intake door, so the two never disagree. The path may be .yaml (the form authored in the catalog repo) or .json (the aggregated catalog.json form); the format is taken from the extension, defaulting to YAML. A manifest that does not even parse is reported too — a missing required field is the shape half of the fail-closed door. It opens no store and needs no binding, so it runs anywhere (a plugin checkout, CI). The exit code is the verdict: 0 valid, 1 invalid (or the file could not be read).", "プラグインの manifest ファイルを目録の規約——整った id・checksum・https の url・repo・空でない対応OS集合・config スキーマ——に照らして検証し、見つかった問題をすべて報告します。作者は目録へ PR を出す前に自己チェックできます。amenbo が install/取り込みの入口で強制するのと同じ規約を読むため、両者が食い違うことはありません。パスは .yaml（目録リポで作者が書く形式）でも .json（集約後の catalog.json 形式）でもよく、形式は拡張子で判定し、既定は YAML です。そもそもパースできない manifest も報告します——必須フィールドの欠落は fail-closed の入口の「形」のチェックで弾かれます。ストアを開かず binding も不要なので、どこでも（プラグインのチェックアウトや CI）走ります。終了コードが判定です: 0 は有効、1 は無効（またはファイルを読めなかった）。"),
    ("path to the manifest file (.yaml or .json)", "manifest ファイルのパス（.yaml または .json）"),
    // plugin list / enable / disable (this machine's installed plugins and their gates)
    ("See this machine's installed plugins, open or close each one's gate (install ≠ enable), and remove one with everything it left behind", "このマシンにインストール済みのプラグインを見て、それぞれのゲートを開閉し（install ≠ enable）、残したものごと撤去する"),
    ("Lists the plugins installed on this machine — name, description and the official badge — beside whose gate is open. The two facts sit together because installing a plugin never runs it: an installed plugin that fires nothing is the ordinary state, not a fault. Reads only the app-data plugins/ directory and this machine's enable state — no network, no catalog fetch — so it answers the same offline. A directory it cannot read as an install is skipped rather than allowed to hide the rest. --json adds each plugin's subscribed events and the path of the executable amenbo would run.", "このマシンにインストール済みのプラグインを——名前・説明・公式バッジ——ゲートが開いているかと並べて一覧します。2つの事実を並べるのは、インストールしただけではプラグインは決して実行されないからです：インストール済みで何も発火しないのは通常の状態であって、異常ではありません。読むのは app-data の plugins/ ディレクトリとこのマシンの有効化状態だけ——ネットワークも目録の取得も使わない——ので、オフラインでも同じ答えを返します。インストールとして読めないディレクトリは、残りを覆い隠さないよう読み飛ばします。--json では各プラグインの購読イベントと、amenbo が実行する実行ファイルのパスも返します。"),
    ("Enables an installed plugin: records the one-time consent to run its code, and opens the gate it fires through. Installing puts a plugin on disk and nothing more — this is the step that lets it run, and it is asked once: a later disable keeps the consent, so re-enabling never asks again. Fail-closed on the settings the plugin's author marked required — while one is empty the enable is refused and the empty fields are named; fill them with `plugin config set` and enable again. amenbo checks only that a value is present, at the machine-default tier the gate itself lives at (a per-project override cannot satisfy a machine-global enable); whether the value is *meaningful* is the plugin author's to judge at run time. Fail-closed on compatibility too: a plugin whose manifest reads a different event-payload contract than this amenbo speaks, or needs an amenbo newer than the one running, is refused with both versions named — update amenbo (or the plugin) rather than run one against a payload it cannot read.", "インストール済みのプラグインを有効化します：そのコードを実行することへの一度きりの同意を記録し、発火するゲートを開きます。インストールはプラグインをディスクに置くだけで、実行を許すのはこの手順です。同意を尋ねるのは一度だけ——後で disable しても同意は残るので、再度の enable で尋ね直しません。作者が required と印した設定に対しては fail-closed です——1つでも空なら enable は拒否され、空のフィールド名が示されます。`plugin config set` で埋めてから enable し直してください。amenbo が見るのは値が入っているかどうかだけで、見る層はゲート自身が在るマシン既定の層です（プロジェクト単位の上書きではマシングローバルな有効化を満たせません）。値が意味として妥当かどうかは、実行時に作者が判断することです。互換宣言に対しても fail-closed です：manifest が読むイベントペイロード規約がこの amenbo の話す規約と違う場合、または必要とする amenbo の版が実行中の版より新しい場合、双方の版を示して拒否します——読めないペイロードを渡して走らせるのではなく、amenbo（またはプラグイン）を更新してください。"),
    ("the installed plugin's name", "インストール済みプラグインの名前"),
    ("Closes an enabled plugin's gate: it stops firing, while staying installed and staying consented — so enabling it again later asks for nothing. Deliberately does not require the plugin to still read as installed: this is how a plugin is stopped, and a half-broken install is exactly when stopping it matters most. Disabling one that is already off changes nothing and says so.", "有効なプラグインのゲートを閉じます：発火は止まりますが、インストール済みのまま・同意も保持したままです——後で有効化し直すときに何も尋ねられません。プラグインがインストール済みとして読めることを、あえて要求しません：これはプラグインを止める手段であり、壊れかけたインストールこそ止めたい場面だからです。すでに無効なものを disable しても何も変わらず、その旨を伝えます。"),
    ("the plugin's name", "プラグインの名前"),
    ("Removes a plugin and everything it left behind: the binary and its directory, its settings in every project on this device, its secrets, and the recorded consent. Disabling stops a plugin while keeping all of that — this is the other end, and the difference is the point: a re-install of the same name starts clean, asking for consent again and inheriting no setting. It works from the name alone and never asks whether the plugin still reads as installed, so it is also how a half-broken install is cleaned up; a name that holds nothing is reported as such, not an error. The steps run worst-residue-first — the gate and consent, then the secrets, then the settings, then the binary — so an interrupted removal leaves an inert directory and never a plugin that still fires. Confirms unless --yes.", "プラグインと、それが残したものすべてを撤去します：実行ファイルとそのディレクトリ、この端末の全プロジェクトの設定、秘密、そして記録された同意。disable はそれらを保ったまま実行だけを止めますが、これはその反対側であり、違いこそが要点です：同じ名前で入れ直しても真っさらから始まり、同意は再取得、設定は何も引き継ぎません。名前だけで動き、プラグインがインストール済みとして読めるかを問わないので、壊れかけた install の後始末にもこれを使えます。何も持たない名前はその旨を報告するだけで、エラーにはなりません。手順は「残ると最も困るもの」から——ゲートと同意、秘密、設定、最後に実行ファイル——なので、途中で中断しても残るのは不活性なディレクトリだけで、発火し続けるプラグインが残ることはありません。--yes が無ければ確認します。"),
];

/// The overlay applied just before display: it swaps only the prose fields of the English spec for
/// their translations. Identifiers (`name`) and the CLI strings in examples are left alone, and a
/// string with no translation keeps its English source.
fn localize_prose(spec: &mut Value, table: &HashMap<&str, &str>) {
    if let Some(caps) = spec.get_mut("capabilities").and_then(Value::as_array_mut) {
        for c in caps {
            tr(c.get_mut("capability"), table);
        }
    }
    if let Some(cmds) = spec.get_mut("commands").and_then(Value::as_array_mut) {
        for c in cmds {
            tr(c.get_mut("summary"), table);
            for bag in ["args", "flags"] {
                if let Some(items) = c.get_mut(bag).and_then(Value::as_array_mut) {
                    for item in items {
                        tr(item.get_mut("help"), table);
                    }
                }
            }
        }
    }
}

/// Swaps one string for its translation, leaving the English source in place when there is none.
fn tr(field: Option<&mut Value>, table: &HashMap<&str, &str>) {
    if let Some(v) = field {
        if let Some(translated) = v.as_str().and_then(|s| table.get(s)) {
            *v = Value::String((*translated).to_string());
        }
    }
}

/// Builds the spec. A single local store, so there is one shape: personal mode.
pub fn build() -> Value {
    json!({
        "amenbo": "A local-first task manager with no central server. You (the AI agent) operate it on the user's behalf.",
        "operating": [
            "The user runs amenbo here on purpose — setting it up in this folder (amenbo init) is their standing instruction that the work here be tracked with amenbo. So manage the work with it as you go, without waiting to be told to each time: this is operating the tool the user chose, not automating something they did not ask for.",
            "Task-ify the request: when the user asks for something substantive, record it as a task (task add) and give it an assignee (`--to me-ai` when you are the one continuing), keep its status current as you work (in_progress → done), and leave the outcome on its timeline (comment add) so the next session can pick up. The backlog — not the chat log or your own memory — is where the work and its state live.",
            "Right granularity, so the backlog stays signal not noise. Make a task for: a concrete change or deliverable, multi-step work, anything that outlives a single session, or work you are handing to a person or their AI. Do not make one for: a question you answer on the spot, a throwaway one-off with no follow-up, or something an open task already covers (comment on that task instead of duplicating it). When the work is substantive and you are unsure, prefer a task — but never pad the backlog with trivia.",
            "Explore by narrowing, not by dumping. The store is a corpus you read a slice of, not a file you load: narrow with `--filter` (`text:<word>`, `status:`, `assignee:`, `decision:`, `task:`) and `--limit`, read the list, then open the few that matter with `task show` / `decision show` (and `comment list` for a task's timeline). Reach for a full dump — `task list --json` with no filter, `decision list --with-body` — only when you genuinely need every row, which is rare: the corpus grows without bound, so what fits today is what times out in a year, and the tokens you spend reading it are tokens you no longer have for the work. The same shape is why this document indexes its commands instead of inlining them: find, then pull.",
            "Surface a consequential choice, don't just make it: when a request carries a decision that matters — one option picked among real alternatives, a hard-to-reverse commitment, or a rationale that will outlive this session — offer to record the why as a decision (decision add starts it proposed, for the human to accept or reject). This is about the substance in front of you, not how much it was discussed: a choice that arrives fully formed in one message — the session's first message included — weighs the same as one reached over several turns. What clears the bar is project-specific and the user and their AI's call, so lean toward surfacing an important 'why' rather than letting it pass unrecorded. This is a nudge, not the standing task-ify duty above — don't paper every change with a decision, and offer the record rather than author it as a fait accompli."
        ],
        "mode": "personal",
        "version": VERSION,
        "schemaVersion": SCHEMA_VERSION,
        "updateAvailable": false,
        "principles": [
            "No central server (the data always lives on the user's device).",
            "Works fully offline (local-first).",
            "Data can be fully exported at any time (no lock-in).",
            "A delete is physical and irreversible; archiving a project is what keeps a record you no longer work on (there is no task-level archive — a task you have finished is done, not archived)."
        ],
        "conventions": {
            "id": "The id **is** the conversational number, and every ref amenbo shows carries the `AMB-` namespace: a task whose `id` is `<n>` is shown as AMB-T-<n>, and a decision whose `id` is `<n>` as AMB-D-<n>. In --json, `id` carries the number and the `ref` field beside it carries that rendering — which is what the human-readable output leads with. Numbers are device-global — one number names one task, with no project context needed — and they come from two sibling spaces (tasks and decisions), so a number alone can name both task AMB-T-<n> and decision AMB-D-<n>; the kind code is what disjoins them. Quote the namespaced form in anything you write — a bare `T-<n>` is another tracker's ref as much as ours, which is exactly what `AMB-` settles. Reading is looser than writing: commands still accept the bare forms (`<n>` / `#<n>` / `T-<n>` / `D-<n>`) alongside the namespaced ones, so a ref copied off the screen pastes straight back.",
            "output": "Output defaults to human-readable text. Read commands produce machine-readable output with --json.",
            "markdown": "Body / free-text fields — task notes (--notes), comments (--text), and decision bodies (--body) — are Markdown, rendered in the GUI (the CLI shows the raw source). Write for a reader scanning fast: lead with the conclusion / TL;DR, prefer bullet lists and tables over paragraphs, keep one point per line (even a single newline shows as a line break, so never run everything onto one line), and split anything long under headings. Renders: GFM tables and task lists, and ```mermaid fenced diagrams (flowchart / sequenceDiagram / stateDiagram / erDiagram; broken syntax falls back to the source, so it never breaks the page). Does not render: raw HTML is ignored, and images are not inline — attach them with task attach / comment attach instead. Reach for Mermaid only when a relationship, flow, state machine, or sequence genuinely reads better than prose or a table; since the CLI shows the raw diagram source, put the key point in one line of text first and let the diagram support it — never carry meaning in the diagram alone.",
            "dates": "Dates are YYYY-MM-DD. Relative forms like 'today' / 'tomorrow' / '+3d' are also accepted.",
            "destructive": "Destructive operations prompt for confirmation by default. Pass --yes / -y to run them non-interactively.",
            "globalFlags": ["--json", "--yes", "--quiet", "--no-color", "--actor <human|ai>", "--project <name|id> (human only — see reach)"],
            "explicitTarget": "By default the project is fixed by location — the .amenbo pointer found upward from the CWD. For a human, one pre-subcommand override acts like `git -C`: `--project <name|id>` overrides the effective project context (defaults like `decision add`'s project; note a ref no longer depends on it — numbers are device-global). Resolution is `--project` (explicit) > `.amenbo` (CWD) > error, with no silent guessing. It is a side path for driving a folderless project from anywhere; the everyday route is to bind a folder. **An AI has no such override** (see reach): for it, location is the only answer. This device holds a single store, so there is none to select.",
            "reach": "An AI works inside the project its folder is bound to, and nowhere else. The `.amenbo` pointer is not a way of naming a store; it **is** the AI's reach: what it can list, read, and write. Three consequences, all enforced (an AI does not have to remember them — the CLI refuses with `out_of_reach`, never a silent empty result): (1) **you do not pick a project.** `--project` and the `project:` filter are human-only vocabulary; passing either is refused even when it names your own bound project. The flip side is that you rarely need one — `task add` / `decision add` / `dimension add` / `dimension list` take their project from the binding, so just omit it. (2) **you read only your project.** Lists, timelines, `status` and `project list` show your project's rows; an id from another project — a task, a decision, a comment id, an attachment id, a dimension — is refused rather than served. (3) **you write only inside it.** Mutations outside are refused, and so is creating anything outside (a new project included). Being out of reach is not being absent: the answer is `out_of_reach`, never `not_found` — the row exists, you just cannot get to it from here. A folder with no binding at all reaches nothing, so an AI there is refused outright: ask the human to `amenbo bind --project <name or id>`, or work in a bound folder. Why: what an AI reads lands in its context and leaks from there into summaries, commits and handoffs, and a write outside its project is a decision made without that project's context. This closes amenbo's own surface, and nothing more — the human is not scoped (an overview is a person's job), and a shell can still read the file underneath.",
            "facet": "The facet of an operation (a person / that person's AI) is given via --actor or the AMENBO_ACTOR environment variable. An AI agent only needs to set AMENBO_ACTOR=ai once for it to apply to every command. It is stamped onto created_by / assign / activity, and echoed back as acted_facet on write results so a mis-set facet is immediately visible. A facet-stamping write run in a machine context (--json or a non-TTY pipe) without a declared facet is refused with facet_required (exit 2) instead of silently defaulting to human, so AI actions are never quietly recorded as a human's. Interactive humans (a TTY without --json) still default to human; pure reads/discovery never require a facet.",
            "exitCodes": { "0": "success", "1": "error", "2": "unknown command or bad arguments (also facet_required)" }
        },
        "agentCycle": [
            "The AI's recommended execution backbone (set AMENBO_ACTOR=ai) — a proven default for proceeding autonomously while avoiding parallel collisions, not a mandate. When you follow it, do these four steps in order; branch to a `cycles` entry only when its trigger fires.",
            "1. list: your mailbox is `amenbo task list --filter \"assignee:me-ai status:todo ready:yes\" --sort priority --json`. Take the highest-priority task; if it comes back empty, widen once to `assignee:none status:todo ready:yes` and assign what you take. (`status:todo` is fresh, unreserved work — a task already `in_progress` is one another session is on, so it stays out of the mailbox and you never double-book; `blocked` is deliberately out, being an external stall — a second machine, a human go/no-go — that you should not self-assign. Waiting on a ruling, or on a day, is not one of those: an unsettled premise and a start day still ahead are derived as `ready:no`, never declared as `blocked`. `ready:yes` hides work whose declared premises are unmet — an open blocker, a linked decision that is not settled, or a start day still ahead — so you never grab it early; query `ready:no` to see each task's blocked_by_open, blocked_by_decisions and not_started_until, and `start:future` for the queue waiting on its day alone.)",
            "2. reserve: `amenbo task status <id> in_progress` (todo→in_progress), then re-confirm it is in_progress with `amenbo task show <id>` before starting. `status` is the whole double-work guard, and reserving is a compare-and-swap: `→ in_progress` succeeds only when the task is currently `todo`. If another session reserved it first, your reserve is rejected with `already_reserved` (a non-zero exit) instead of silently succeeding — so the collision is actually detected. The reserve also requires `ready`, so a task with an open blocker, an unsettled premise, or a start day still ahead is rejected with `not_ready` (also a non-zero exit), and there is no `--force`. The two failures pull in opposite directions. On `already_reserved` the task is taken by someone else: go back to step 1 and pick the next one. On `not_ready` it is your own declaration that holds it: resolve the premise — finish the blocker or `task undepend` it; `decision accept` the linked decision, or `decision link --unlink` it; correct a start day you declared wrong with `task update <id> --start today` (or `--clear-start`) — and reserve only then. Both guards judge this transition only: an edge added, or a decision rejected, under a running task never strips its status, and `→ todo` / `→ blocked` / `→ done` stay unconditional, so the hand-back path is never closed.",
            "3. execute: first read the task's latest comments and whatever they reference — linked notes, decisions, attachments (`amenbo comment list <id>`) — if the direction feels undecided, you have not read enough; then do the work, moving state with `amenbo task status <id> blocked` if you stall.",
            "4. finish: `amenbo task done <id>`, and leave the context on the task's timeline with `amenbo comment add <id> --text ...` so the next session can pick up. (If you decide not to take a reserved task, hand it back with `amenbo task status <id> todo`.)"
        ],
        "cycles": cycles(),
        "inspect": [
            "amenbo config --json",
            "amenbo status --json",
            "amenbo project list --json",
            "amenbo task list --json",
            "amenbo doctor --json",
            "amenbo validate --json"
        ],
        "commands": all_commands(),
        "capabilities": capabilities(),
        "filterGrammar": {
            "description": "task list's --filter combines key:value pairs (whitespace-separated) with AND.",
            "keys": {
                "done": ["true", "false"],
                "status": ["todo", "in_progress", "done", "blocked", "todo,in_progress (comma = any-of)"],
                "due": ["today", "overdue", "week", "none", "YYYY-MM-DD"],
                "start": ["today", "future", "none"],
                "priority": ["high", "medium", "low", "none"],
                "project": ["<id>", "<name (exact)>", "(human only — an AI's list is already its bound project)"],
                "text": ["<substring match over title + notes + comment bodies>"],
                "number": ["AMB-T-<n>", "<n>", "#<n> / T-<n> (bare forms, still accepted)", "AMB-D-<n>"],
                "ref": ["alias of number"],
                "assignee": ["none", "me", "me-ai", "<user name or ID>"],
                "ai": ["true", "false"],
                "ready": ["yes", "no"],
                "blocked": ["none", "open"],
                "decision": ["AMB-D-<n>", "D-<n>", "<n> (the tasks a decision links to)"],
                "commit": ["<full 40/64-hex sha>", "(the tasks recording this commit sha — the reverse chain git → task)"],
                "dim": ["<axis>=<value>", "<axis>=none (no value on that axis)"],
                "dimension": ["alias of dim"],
                "time_axis": ["<value>", "none", "(sugar for dim:<the time_axis axis>=…)"]
            },
            "example": "amenbo task list --filter \"done:false due:today priority:high\" --sort due --json",
            "note": "Different keys combine with AND. `status:` alone takes a comma-separated any-of set (`status:todo,in_progress` = todo OR in_progress) — this is the only key with in-key OR; all others are single-valued. `dim:` (alias `dimension:`) is the only *repeatable* key: `dim:Category=bug dim:Area=core` ANDs the two axes; an axis and a value each resolve by name (exact, case-insensitive) or by id (exact), and `=none` selects tasks with no live value on that axis. A name that resolves to nothing is an error, not an empty result — including on `=none`, where an axis that does not exist would otherwise match *every* task. (Axes are per-project, so a name two projects share resolves to both and the filter ORs them; scope with `project:` to mean one of them.) `time_axis:` is sugar for the axis carrying `role: time_axis` — it keys on the role, not on the axis's name, so it works whatever the user named it; with no time axis declared it names nothing, so it is an error too. Values holding whitespace cannot be expressed (the filter splits on whitespace) — filter by the value's id instead. The AI mailbox query is defined once in agentCycle step 1 (referenced, not repeated here). `ready:yes` = every declared premise is met, so the task can be reserved; `ready:no` (= `blocked:open`) = a premise is unmet — an open blocker (blocked_by_open), a linked decision that is not settled (blocked_by_decisions), or a declared start day that has not come (not_started_until). `start:` takes named arms only, no bare date (unlike `due:`): `start:future` is the queue waiting on its day, `start:today` what has come, `start:none` what declared no day. `number:` (alias `ref:`) filters by the conversational number: a bare number / `AMB-T-<n>` (or the bare `#<n>` / `T-<n>`) match a task's number; an `AMB-D-<n>` names a decision and so matches no task (tasks and decisions have separate number spaces). `ai:true` narrows to work delegated to any AI (`assignee_kind=ai`) — an independent axis from `assignee:` (whereas `assignee:me-ai` is *your* AI); `ai:false` excludes AI-delegated work. Ordering (don't grab future work yet) is not merely shown here: the same predicate guards the reserve transition, so a `ready:no` task is rejected with not_ready even when named by number. `decision:` walks the decision⇄task link from the task side — `--filter \"decision:AMB-D-<n> status:todo\"` is the open work a decision produced — and `decision list --filter \"task:AMB-T-<n>\"` walks it from the other side (the decisions a task rests on); a ref from the wrong space (`decision:AMB-T-<n>`) is an error, not an empty result. `commit:<full sha>` walks the reverse chain git → task (the tasks that recorded a commit); a SHA is a free value, not a name the store knows, so one matching nothing is an empty result, not an error — and since the door stores full hex only, a short SHA simply matches nothing."
        },
        "notes": [
            "amenbo assigns the id. The AI does not create with a specified id (use the id returned after creation).",
            "One task belongs to exactly one project. task move re-homes it to another project.",
            "Priority and due date are independent. A task can be both high and due in a week.",
            "The AI (--actor ai) can only delete tasks it created as the AI. Deleting tasks created by others and archiving/deleting projects are denied by ai_guardrail (ask a human; the local policy config ai_allow_project_ops can allow those). Reversible ops on the bound project itself (update/move/unarchive) are allowed — the guard covers only the destructive/hiding ones."
        ]
    })
}

/// cold path: the trigger-indexed catalog agentCycle branches to. Each cycle separates a
/// `backbone` (what always applies within that cycle, in order) from `optional` items (done
/// only when the item's `trigger` situation matches). Every item carries a machine-readable
/// `kind` so it stays self-describing if flattened/filtered; optional items additionally carry
/// a `trigger`. This is how amenbo *shows what you can do* without directing you to do it: the
/// AI self-gates on the trigger. Referencing only real command names is guarded by the
/// `cycles_reference_real_commands` test.
fn cycles() -> Value {
    json!({
        "description": "Cold-path catalog branched to from agentCycle when a trigger fires. `backbone` always applies within its cycle (in order); each `optional` item is done only when its `trigger` matches (the AI self-gates — amenbo shows what you can do, it does not direct you). Every item carries `kind`; optional items also carry `trigger`.",
        "taskShaping": {
            "when": "You are registering or decomposing work.",
            "backbone": [
                backbone_step("There are no subtasks: decompose larger work into separate tasks, each belonging to exactly one project.", &["task add"]),
                backbone_step("Link ordering with dependency edges: a task with an open blocker stays out of the mailbox, and its reserve is refused (not_ready) until the blocker is done. Declare only the order you mean — the edge is enforced, not advisory.", &["task depend", "task undepend"]),
            ],
            "optional": [
                optional_step("the work spans ordered time-direction stages", "Add a --time-axis dimension and place tasks on its ordered values, gating later stages behind earlier ones with a dependency.", &["dimension add", "dimension set"]),
                optional_step("you are handing work to a person, that person's AI, or yourself (`--to me-ai`) to continue", "Delegate at creation (--to/--ai) or afterward.", &["task add", "task assign"]),
                optional_step("the work embeds a consequential choice — an option picked among alternatives, or a hard-to-reverse commitment", "Offer to record the rationale as a decision, and link it to the tasks that implement it — the link makes it their premise, so they cannot be reserved until it is accepted. Surface it for the human to accept or reject; don't author it as already settled (see the `decision` cycle).", &["decision add", "decision link"]),
                optional_step("a task's shape looks off (missing project/priority, malformed)", "Check its shape before relying on it.", &["validate"]),
            ]
        },
        "decision": {
            "when": "You're handling work that carries a consequential choice — an option picked among alternatives, a hard-to-reverse commitment, or a rationale that will outlive this session. This is about the substance in front of you, not how much it was discussed: a request that arrives fully formed in one message — the session's first message included — counts as much as a conclusion reached over several turns. The bar is project-specific and the user and their AI's to judge; when unsure, surface it rather than let an important 'why' go unrecorded.",
            "backbone": [
                backbone_step("Freeze the rationale as an append-only decision — it starts proposed (your proposal; the human accepts or rejects it), so offer the record, don't impose it.", &["decision add", "decision promote"]),
            ],
            "optional": [
                optional_step("a new decision wholly replaces an existing one", "Chain it as a supersession — the old one stops being current (the edge says so; no status changes).", &["decision supersede"]),
                optional_step("a new decision partially revises an existing one that stays current", "Chain it as an amendment — the old one is not superseded; read the two together.", &["decision amend"]),
                optional_step("a proposed decision is agreed and ready to settle", "Accept it (stamps decided_at/decided_by).", &["decision accept"]),
                optional_step("an accepted decision needs a minor fix (typo / stale line)", "Edit it in place — accepting no longer freezes the body, so there is no reopen/re-accept round-trip. Supersede stays for a change of mind; reopen for un-settling a too-hasty acceptance.", &["decision edit"]),
                optional_step("you just added or accepted a decision", "Check whether it semantically contradicts an existing one. Pull a bounded, relevant neighbourhood — narrow by the new decision's key terms (decision list --filter \"status:accepted text:<term>\" --with-body --limit N), not the whole corpus — and read those bodies. If one contradicts, propose it to the human as a candidate supersede/amend with your reasoning; do not author the edge yourself (detection proposes, the human disposes).", &["decision list", "decision supersede", "decision amend"]),
                optional_step("the decision drove concrete implementation tasks", "Link it to those tasks: the link declares the decision is their premise, so they stay unreservable (not_ready) until it is accepted and current — link before proposing, and the work cannot start ahead of its own rationale. It freezes what it touches, so link implementation tasks only: never the task of ruling on the decision itself, and never a rejected decision, or one that has been superseded, that you merely want to cite (that belongs in the body).", &["decision link"]),
            ]
        },
        "decisionAudit": {
            "when": "Periodically — contradictions accumulate over time as new decisions land far from older ones.",
            "backbone": [
                backbone_step("Sweep the current (accepted) decisions for semantically contradicting pairs, but keep each pass bounded: page through a slice you can actually reason over (decision list --filter status:accepted --with-body --limit N --offset …) rather than loading everything at once, and rotate the window across passes. Detection is best-effort recall by design — as the corpus outgrows one pass, coverage stays bounded to what you can reach now and repeated passes widen it over time.", &["decision list"]),
                backbone_step("Surface each suspected contradiction to the human as a candidate supersede/amend with your reasoning. Never author the edge yourself, and only run supersede/amend once the human confirms — detection proposes, the human disposes, so a false positive cannot silently kill a good decision.", &["decision supersede", "decision amend"]),
            ],
            "optional": []
        },
        "executionExceptions": {
            "when": "You hit something that breaks the straight-line cycle.",
            "backbone": [
                backbone_step("Progress state and assignment are orthogonal: reassignment is plain (the task just moves to its new assignee — no special status, no round-trip counter), and status=blocked is reserved for a physical blocker no one can move past. An unmet premise is not one: a pending dependency, an unsettled linked decision, and a start day that has not come are all derived as ready:no, so declaring them blocked duplicates a truth amenbo already computes.", &[]),
            ],
            "optional": [
                optional_step("you cannot proceed and need a human decision or action", "Reassign to the human and say what is needed (a silent reassignment is unhelpful).", &["task assign", "comment add"]),
                optional_step("a physical blocker no one can move past", "Mark it blocked with the reason.", &["task block"]),
                optional_step("your reserve was rejected with not_ready", "Resolve the premise you declared, rather than working around it: finish the blocker or drop the edge; get the linked decision accepted, unlink it, or relink it to its successor; move or clear a start day that has not come. There is no --force. A start day is the one premise that resolves itself: if the work is meant to wait, leave it and take the next task.", &["task done", "task undepend", "decision accept", "decision link", "task update"]),
                optional_step("you decide not to take a task you reserved", "Hand it back with `task status <id> todo` so another session can take it.", &["task status"]),
            ]
        },
        "commit": {
            "when": "You are about to commit, or to send text out of this store some other way (a PR body, an issue, a message).",
            "backbone": [
                backbone_step("Lint what is leaving, before it leaves: the staged diff, and the commit message. It reports and never edits, so a ref it names is yours to rewrite out of the text.", &["lint"]),
                backbone_step("Once the commit lands, record its SHA on the task — a task takes many, and a SHA already there is a no-op, so anchor every commit you merge.", &["task commit add"]),
            ],
            "optional": [
                optional_step("the human wants every commit linted without anyone having to remember", "Offer the lint hooks (opt-in, asked once for the device).", &["hooks install", "hooks status"]),
            ]
        },
        "worktree": {
            "when": "You are starting work on a task.",
            "backbone": [
                backbone_step("Cut a worktree per task, whenever the work will produce commits. Reserving guards the task, not the files, so two sessions on two tasks still share one working tree. Work that lands no commit — a local-only edit under gitignore — needs none.", &[]),
                backbone_step("Cut it outside the project folder. A worktree cut inside inherits that folder's `.amenbo` through the upward walk, and amenbo refuses to run there (`nested_worktree`).", &[]),
                backbone_step("Operate amenbo in the project folder itself. A worktree outside it carries no `.amenbo` and so reaches nothing — that is the intent, and binding this checkout is not the way around it.", &[]),
            ],
            "optional": []
        }
    })
}

/// cold-path backbone item: always applies within its cycle, in order (no trigger).
fn backbone_step(step: &str, commands: &[&str]) -> Value {
    json!({ "kind": "backbone", "step": step, "commands": commands })
}

/// cold-path optional item: done only when its `trigger` situation matches (the AI self-gates).
fn optional_step(trigger: &str, step: &str, commands: &[&str]) -> Value {
    json!({ "kind": "optional", "trigger": trigger, "step": step, "commands": commands })
}

fn cmd(name: &str, summary: &str, flags: Value, examples: Value) -> Value {
    json!({ "name": name, "summary": summary, "flags": flags, "examples": examples })
}

/// Lists what amenbo can do, phrased by intent and neutral about order. Each capability names the
/// commands that realise it in `commands`; no sequence, no recommended workflow — capability first.
/// Only commands that actually exist in `all_commands()` may be named, which the
/// `capabilities_reference_real_commands` test enforces, catching both a typo and a command nobody
/// listed.
fn capabilities() -> Value {
    let caps = vec![
        cap("Register a task", &["task add"]),
        cap("Find and filter tasks (see filterGrammar)", &["task list"]),
        cap("See a task's details, project, blockers and dependents", &["task show"]),
        cap("Edit a task's fields (title / notes / due / start / priority)", &["task update"]),
        cap(
            "Track progress and reserve a task by moving it to in_progress (todo / in_progress / done / blocked)",
            &["task status", "task done", "task reopen", "task block"],
        ),
        cap(
            "Split larger work into separate tasks and link blockers (there are no subtasks)",
            &["task depend", "task undepend"],
        ),
        cap(
            "Anchor a task to the git commits that implemented it — record / list / forget SHAs (the chain from history back to a task)",
            &["task commit add", "task commit list", "task commit rm"],
        ),
        cap(
            "Re-home a task to another project and reorder it",
            &["task move"],
        ),
        cap(
            "Assign a task to a person or that person's AI, hand it back, or clear it",
            &["task assign", "task unassign"],
        ),
        cap("Discuss on a task's timeline (a comment posted by mistake can be edited or deleted)", &["comment add", "comment list", "comment edit", "comment rm"]),
        cap("Discuss on a decision's timeline (accept/reject reasons land here; a comment posted by mistake can be edited or deleted)", &["decision comment add", "decision comment list", "decision comment edit", "decision comment rm"]),
        cap(
            "Attach what text cannot hold — screenshots, raw logs, benchmarks — to tasks, decisions, comments",
            &["task attach", "decision attach", "comment attach", "decision comment attach", "attach ls", "attach show", "attach open", "attach save", "attach rm"],
        ),
        cap(
            "Read the shared activity timeline (system events plus comments)",
            &["activity"],
        ),
        cap("Record a decision — an append-only \"why we chose X\"", &["decision add"]),
        cap("Find and search decisions", &["decision list"]),
        cap("See a decision, its supersession chain, and the premises it stands on", &["decision show"]),
        cap(
            "Record that a decision stands on an older one (read that first; revisit this if it is overturned)",
            &["decision builds-on"],
        ),
        cap(
            "Move a decision through its lifecycle (accept / reject / reopen / edit / supersede / delete)",
            &["decision accept", "decision reject", "decision reopen", "decision edit", "decision supersede", "decision delete"],
        ),
        cap("Undo a decision-to-decision edge drawn at the wrong target", &["decision unlink"]),
        cap("Link a decision to its implementation tasks", &["decision link"]),
        cap("Promote a task comment into a decision", &["decision promote"]),
        cap(
            "Organize work into projects and order them (classification is via dimensions)",
            &[
                "project add", "project list", "project show", "project update", "project move",
                "project archive", "project unarchive",
            ],
        ),
        cap(
            "Define classification axes (dimensions) with values and assign them to tasks",
            &[
                "dimension add", "dimension list", "dimension show", "dimension rename",
                "dimension update", "dimension move", "dimension rm",
                "dimension value-add", "dimension value-rename", "dimension value-update",
                "dimension value-move", "dimension value-rm", "dimension set", "dimension unset",
            ],
        ),
        cap("See what to do now (overdue / today / in progress)", &["status"]),
        cap("Inspect configuration and this store's identity", &["config", "config set", "whoami"]),
        cap("Update amenbo: open the installer, or self-update the standalone CLI in place (`--apply` / undo with `--rollback`)", &["update"]),
        cap("See projects", &["project list", "project show"]),
        cap(
            "Allow an AI launched in a folder to operate amenbo (bind a folder to a project, unbind it, or re-sync its managed guidance block)",
            &["init", "bind", "unbind", "sync-guide"],
        ),
        cap(
            "Take all data out (data sovereignty; no lock-in — export is one way, `restore` is the way back in)",
            &["export"],
        ),
        cap(
            "Check data integrity",
            &["doctor", "validate"],
        ),
        cap(
            "Catch an amenbo ref in text on its way out of this store, before it lands somewhere it means nothing (read-only; reports path:line and exits non-zero)",
            &["lint"],
        ),
        cap(
            "Run the lint on every commit, by installing it as a git hook (asked once for the lint as a feature, on this device — one answer covers the repositories amenbo works in, later ones included; amenbo touches only the hook it wrote)",
            &["hooks install", "hooks uninstall", "hooks status"],
        ),
        cap(
            "Write a verified full snapshot of the store's truth source to a single file",
            &["backup"],
        ),
        cap(
            "Restore the store's truth source from a verified snapshot (the recovery side of backup)",
            &["restore"],
        ),
        cap(
            "Physically erase content from the store — a comment in full, or one accepted decision's body (human-gated maintenance)",
            &["hard-erase comment", "hard-erase decision"],
        ),
        cap(
            "Validate a plugin manifest against the catalog rules before submitting it (an author's self-check)",
            &["plugin validate"],
        ),
        cap(
            "See this machine's installed plugins, open or close each one's gate (install ≠ enable), and remove one with everything it left behind",
            &["plugin list", "plugin enable", "plugin disable", "plugin uninstall"],
        ),
    ];
    Value::Array(caps)
}

fn cap(capability: &str, commands: &[&str]) -> Value {
    json!({ "capability": capability, "commands": commands })
}

fn all_commands() -> Value {
    json!([
        cmd("amenbo", "No arguments. Shows today's tasks and suggested next operations (discover).",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo", "amenbo --json"])),
        cmd("agent", "Presents how to work here — the workflow and rules in full, plus an index of the commands (this JSON). The AI's entry point. A command's own flags and examples are pulled on demand with --command <name>, so the entry point stays small; --full prints them all inline.",
            json!([{ "name": "--command <name>", "help": "print one command's full spec (flags, args, examples) instead of the entry point" },
                   { "name": "--full", "help": "print every command's full spec inline (scripts / verification)" },
                   { "name": "--json", "help": "machine-readable output (recommended)" }]),
            json!(["amenbo agent --json", "amenbo agent --command \"task add\" --json", "amenbo agent --full --json"])),
        cmd("version", "Shows version information.",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo version --json"])),
        cmd("update", "Updates amenbo. By default opens this OS's one-piece installer (GUI + CLI) — resolved from the published latest.json, falling back to the releases page — in your browser. `--apply` self-updates the standalone CLI in place instead: it downloads the new CLI over TLS and swaps this binary (no installer, no elevation), keeping the replaced binary beside it; a GUI-managed CLI is updated from the desktop app, not here. `--rollback` undoes the last `--apply` offline, restoring that kept binary. Applying is always your explicit call — amenbo never updates in the background.",
            json!([{ "name": "--print", "help": "print the installer URL instead of opening a browser (headless / scripted use)" },
                   { "name": "--apply", "help": "self-update the standalone CLI in place (download + swap this binary) instead of opening the installer" },
                   { "name": "--rollback", "help": "undo the last --apply, restoring the previous binary kept beside this one (offline, no download)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo update", "amenbo update --print", "amenbo update --apply", "amenbo update --rollback"])),
        cmd("config", "Shows configuration (store location, default view, etc.).",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo config --json"])),
        json!({ "name": "config set", "summary": "Changes a configuration value. Known keys: default_view, language, date_locale, human_name, ai_name (the display names of the two actors — this is the only way to rename either), human_avatar, ai_avatar (their icons, as a data:image/png;base64 URI), ai_allow_project_ops, onboarded, startup_integrity_check (read-only integrity doctor at open; warnings only; default on), update_check (checks a static latest.json for a newer release; infra-side only — no user data; timeout + silent-fail + cached; default on; AMENBO_UPDATE_CHECK=0 overrides).",
            "args": [{ "name": "key", "required": true, "help": "config key" },
                     { "name": "value", "required": true, "help": "config value" }],
            "flags": [], "examples": ["amenbo config set default_view board", "amenbo config set language ja", "amenbo config set startup_integrity_check false"] }),
        cmd("whoami", "Shows this store's identity (display name / hardware-copy check).",
            json!([{ "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo whoami --json"])),
        json!({ "name": "init", "summary": "Initializes a folder so an AI launched there is allowed to operate amenbo. amenbo does not read or write the project's contents (source or files). The store itself lives in app-data (a single database for the whole device); only .amenbo (a dir→project pointer) and AGENTS.md (the AI guide) are placed in the folder. On a device that already holds an amenbo store, init makes a new project in this folder — it does not start a second store. Secrets (keys) are also kept in the user area, not in the project directory. AGENTS.md is English-based and embeds the global user language (config language / --language) as a 'communicate with the human in this language' directive. A folder already bound to another project via .amenbo is rejected by default (init_pointer_exists; prevents clobbering the production pointer). A folder that has no .amenbo but already holds an amenbo managed block (in CLAUDE.md/AGENTS.md) is no longer rejected on the marker alone: init reverse-looks-up the bindings registry and, if exactly one live project claims the folder, recovers the lost pointer (a bind, not a new project); if several claim it, it stops as ambiguous (init_ambiguous_owners); if none do, it proceeds and idempotently regenerates the block. Use bind to re-bind to an existing one, or --force to truly recreate and overwrite.",
            "args": [], "flags": [{ "name": "--name <str>", "help": "the first local user name (at initial genesis)" },
                                   { "name": "--language <code>", "help": "sets the user language (ja/en etc.) in the global config and embeds it in AGENTS.md" },
                                   { "name": "--force", "help": "create a new project and overwrite even if a .amenbo already exists (default rejects clobbering)" }],
            "examples": ["amenbo init", "amenbo init --name Alice --language ja"] }),
        cmd("bind", "Allows an AI launched in this folder to operate an existing project (it does not touch the contents; it just places a .amenbo pointer and locally registers project→dir). Shows the current binding when --project is omitted. Several folders may point at the same project (many-to-one). Binding a subdirectory of a folder that is already managed (a parent has a .amenbo) is rejected (binding_nested_tree) so a stray bind cannot shadow the root pointer; pass --force to bind it intentionally. If the target is gone, binding_stale. By default the pointer lands in the current directory; pass --dir <path> to place it in another existing folder (bind a folder from outside it).",
            json!([{ "name": "--project <id>", "help": "project ID to bind (omit to show)" },
                   { "name": "--dir <path>", "help": "place the .amenbo pointer in this existing directory instead of the current one (bind a folder from outside it, git -C style)" },
                   { "name": "--force", "help": "bind even inside an already-managed tree (a parent has a .amenbo); default rejects to avoid shadowing the root pointer" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo bind --project 3", "amenbo bind --project \"Site rebuild\" --dir /work/repo", "amenbo bind --json"])),
        cmd("unbind", "Removes this folder's .amenbo binding (and amenbo's managed blocks in AGENTS.md/CLAUDE.md, keeping your own content), the inverse of bind/init. The project itself is kept: this is a many-to-one unbind, so only this folder's pointer is removed and other folders bound to the same project are untouched. It also forgets this folder from the local project→folder reference registry. If the folder has no .amenbo of its own it is not unbound (unbind_no_binding); an inherited binding from an ancestor is reported, not silently removed, so the whole tree is never unbound by accident.",
            json!([{ "name": "--dir <path>", "help": "folder to unbind (defaults to the current directory)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo unbind", "amenbo unbind --dir /work/repo --json"])),
        cmd("status", "Shows a summary of what to do now (overdue / today / in progress).",
            json!([{ "name": "--scope <today|overdue|week>", "help": "scope (default today)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo status --json", "amenbo status --scope week"])),
        cmd("activity", "Shows activity (system events plus comments) as one timeline. History reads newest-first; passing a cursor to --since reads the increment oldest-first (an agent's poll-for-what-changed). Humans and the AI read the same stream. Every response carries an opaque cursor; --for me narrows it to what a facet should act on.",
            json!([{ "name": "--task <id>", "help": "this task only" },
                   { "name": "--project <id>", "help": "only tasks belonging to this project" },
                   { "name": "--since <date|cursor>", "help": "a date (today / +3d / YYYY-MM-DD) reads history on/after it, newest-first; an opaque cursor from a prior response reads only what is strictly newer, oldest-first (incremental) — pass the response's cursor to resume where you left off" },
                   { "name": "--kind <system|comment>", "help": "filter by which stream an item came from: `system` for the events amenbo stamps itself, `comment` for what a facet wrote. Distinct from the `kind` a system item carries in its payload, which names the event — task.created / status_changed / assigned / moved / deleted" },
                   { "name": "--by <human|ai>", "help": "filter by the issuer's facet (a read filter separate from the global --actor)" },
                   { "name": "--for <me|human|ai>", "help": "narrow to what a facet should act on: activity on tasks assigned to that facet (destination axis; me = your own facet). Distinct from --by, which filters by who issued the event" },
                   { "name": "--limit <n>", "help": "max count (history: newest-first window; incremental: oldest items after the cursor). has_more marks when the window was cut" },
                   { "name": "--offset <n>", "help": "number of items to skip (newest first; paging / going back through history)" },
                   { "name": "--json", "help": "machine-readable output — { count, cursor, has_more, items }" }]),
            json!(["amenbo activity --json", "amenbo activity --task 12 --json", "amenbo activity --limit 100 --offset 100 --json", "amenbo activity --for me --since cur1_… --limit 50 --json"])),
        cmd("sync-guide", "Re-syncs amenbo's managed guidance block in bound folders to this binary's current format version. A folder follows on its own the moment you run amenbo in it, so this is for the folders you have not been in — and for a block amenbo could not write (a read-only checkout). Idempotent and low-churn: a folder's CLAUDE.md/AGENTS.md is rewritten only when its managed block actually changed, each folder's own language label is preserved (never downgraded), and your content outside the markers is untouched. By default it targets every folder locally bound on this machine (the machine's binding registry, store-independent) so its scope matches what doctor scans; pass --dir to resync just one folder. Moved/renamed folders are skipped silently.",
            json!([{ "name": "--dir <path>", "help": "resync just this folder (defaults to every locally bound folder)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo sync-guide", "amenbo sync-guide --dir /work/repo --json"])),
        cmd("doctor", "Data integrity check (orphan references, broken ordering, bound folders whose CLAUDE.md/AGENTS.md still carry an outdated managed-block version after a binary update, bound folders whose .amenbo pointer is still in a pre-migration format, etc.). Side-effect-free by default: this face reports, it never rewrites. What it reports about a folder heals on its own the next time you run amenbo there — the managed block follows this binary and a legacy .amenbo is upgraded — so what stays listed here is the folders you have not been in (sync-guide resyncs every bound folder's block at once). --fix repairs fixable problems: it sweeps attachment files nothing references any more (the delete path reclaims its own, so this collects only what it had to spare) and forgets folder bindings no live project claims. Every repair is non-destructive — it drops nothing you can read.",
            json!([{ "name": "--fix", "help": "repair fixable problems (reclaim unreferenced attachment files, forget folder bindings no live project claims; both are non-destructive)" },
                   { "name": "--yes/-y", "help": "skip the --fix confirmation" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo doctor --json", "amenbo doctor --fix --yes"])),
        json!({ "name": "validate", "summary": "Checks the shape of the given tasks (all data when omitted). Side-effect-free.",
            "args": [{ "name": "id...", "required": false, "help": "ID(s) to check (multiple allowed)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output (issues carry a fix_hint)" }],
            "examples": ["amenbo validate --json", "amenbo validate AMB-T-<n> --json"] }),
        json!({ "name": "lint", "summary": "Finds amenbo refs (AMB-T-<n> / AMB-D-<n> …) in text on its way out of this store — a commit message, a diff, a file — reports each as path:line, and exits non-zero if there is one. An id resolves only for someone holding this store; anywhere else it is a reference into nothing. Read-only: it reports and never edits (there is no --fix). With no arguments it reads the staged diff (git diff --cached) and scans what the commit ADDS — a ref in untouched or deleted text is not what this commit is leaking. Pass file paths to lint those instead (the message file git hands a commit-msg hook included), or --stdin for piped text. A bare #<n> is left alone: that is a GitHub issue, and a T-<n> may be another tracker's — which is exactly what the AMB- namespace settles. It opens no store and resolves no id (the AMB- prefix is the whole test), so it answers the same in a checkout, in CI, and over any text at all, and needs no .amenbo to run. The exit code is the verdict: 0 clean, 1 a ref was found (or the input could not be read).",
            "args": [{ "name": "path...", "required": false, "help": "file(s) to lint (default: the staged diff)" }],
            "flags": [{ "name": "--stdin", "help": "lint the text piped on stdin instead" },
                      { "name": "--json", "help": "machine-readable output (ok / count / hits[path,line,ref])" },
                      { "name": "--quiet", "help": "report nothing and let the exit code speak — for a caller that wants only the verdict (the hook amenbo installs does not pass it: a refused commit has to say what refused it)" }],
            "examples": ["amenbo lint", "amenbo lint --json", "amenbo lint .git/COMMIT_EDITMSG", "amenbo lint --stdin < message.txt"] }),
        cmd("hooks install", "Writes the git hooks that run `amenbo lint` on every commit: `pre-commit` reads the staged diff, and `commit-msg` reads the message, which is the only place git offers it (at pre-commit time no message exists yet). One lint, two of git's doors. Installing means writing into your git plumbing, which amenbo does not do unasked: it asks once — for the lint as a feature, on this device — and that one answer covers every slot and every repository, the ones bound after it included. `install` is the explicit face of that, wiring the repository it runs in; it is usable any time, including after a `no`, and it takes back an earlier `uninstall` here. amenbo marks the hooks it writes and touches nothing else: a hook from husky, lefthook or your own hand is NEVER overwritten — install steps around it, wiring the slots it may own and naming the one line to add to the rest (`amenbo lint || exit 1`, or `amenbo lint \"$1\" || exit 1` for commit-msg). Only an install with no slot to write at all is refused. Re-running over amenbo's own hooks rewrites them, which is how a newer build's hooks land. They honour core.hooksPath, exit 0 when amenbo is not on PATH (a convenience, not a gate), and one commit is bypassed with `git commit --no-verify`.",
            json!([]), json!(["amenbo hooks install"])),
        cmd("hooks uninstall", "Removes the lint hooks amenbo wrote from this repository, and opts it out so a device-wide yes does not re-wire it at the next startup (this is per repository — it does not touch the device's answer). The mirror of install, refusal for refusal and partial for partial: a hook amenbo did not write is not amenbo's to delete and is left alone, and only a call with nothing of ours to remove and a stranger in the way is refused. With no hooks of ours there, it records the opt-out and does nothing else. It closes the question for this repository, not the door — `hooks install` re-wires it whenever you want it back.",
            json!([]), json!(["amenbo hooks uninstall"])),
        cmd("hooks status", "Shows the two facts side by side: what is in each hook slot (no hook / amenbo's, with its marker version / one amenbo did not write), and what this device answered (not asked yet / yes / no) — plus a line when this repository is opted out. They are independent on purpose — the answer says what was answered and is NEVER read as a mirror of the disk, which is what makes a hook deleted or added by hand a state amenbo can see rather than one that breaks it. Read-only.",
            json!([{ "name": "--json", "help": "machine-readable output (in_git_repo / hooks / consent)" }]), json!(["amenbo hooks status --json"])),
        cmd("project add", "Creates a project.",
            json!([{ "name": "--name <str>", "required": true, "help": "project name (required, non-empty)" },
                   { "name": "--view <list|board>", "help": "default view (default list)" },
                   { "name": "--notes <str>", "help": "description (Markdown)" },
                   { "name": "--color <str>", "help": "color" }]),
            json!(["amenbo project add --name \"Site Redesign\" --view board"])),
        cmd("project list", "Lists projects.",
            json!([{ "name": "--archived", "help": "include archived ones too" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo project list --json"])),
        json!({ "name": "project show", "summary": "Shows project details (counts, etc.) plus bound_folders: the folders whose .amenbo points at this project (the reverse of bind), each inspected — exists (false = the folder moved or was deleted), pointer_missing (the folder is there but its .amenbo is gone), legacy (a pre-migration pointer) and mismatch (the pointer belongs to another store).",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo project show AMB-P-<n> --json"] }),
        json!({ "name": "project update", "summary": "Updates a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--name <str>", "help": "" }, { "name": "--notes <str>", "help": "" },
                      { "name": "--view <list|board>", "help": "" }, { "name": "--color <str>", "help": "" }],
            "examples": ["amenbo project update AMB-P-<n> --name \"Redesign Project\""] }),
        json!({ "name": "project move", "summary": "Reorders a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--before <id>", "help": "before the given ID" }, { "name": "--after <id>", "help": "after the given ID" },
                      { "name": "--top", "help": "to the top" }, { "name": "--bottom", "help": "to the bottom" }],
            "examples": ["amenbo project move AMB-P-<n> --top"] }),
        json!({ "name": "project archive", "summary": "Archives a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [], "examples": ["amenbo project archive AMB-P-<n>"] }),
        json!({ "name": "project unarchive", "summary": "Unarchives a project.",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [], "examples": ["amenbo project unarchive AMB-P-<n>"] }),
        json!({ "name": "project delete", "summary": "Deletes a project — permanently, with its tasks and everything hanging off them (a delete is physical and irreversible; archive instead if you want it kept).",
            "args": [{ "name": "id", "required": true, "help": "project ID" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo project delete AMB-P-<n> --yes"] }),

        cmd("dimension add", "Adds a dimension (a user-defined classification axis) to a project. New projects seed no dimensions — create the axes you need. An axis is single-select; --ordered gives the values an explicit order, --time-axis marks it as the ordered time lane.",
            json!([{ "name": "--project <id>", "help": "owning project (defaults to the bound project; an AI omits it)" },
                   { "name": "--name <str>", "required": true, "help": "dimension name" },
                   { "name": "--notes <str>", "help": "description / notes (Markdown)" },
                   { "name": "--ordered", "help": "give the values an explicit order" },
                   { "name": "--time-axis", "help": "mark as the time axis (an ordered view lane)" }]),
            json!(["amenbo dimension add --name \"Category\" --ordered"])),
        cmd("dimension list", "Lists a project's dimensions in display order, each with its values.",
            json!([{ "name": "--project <id>", "help": "target project (defaults to the bound project; an AI omits it)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo dimension list --json"])),
        json!({ "name": "dimension show", "summary": "Shows a dimension: name, notes, kind (single-select, ordered, time-axis), and its values.",
            "args": [{ "name": "id", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo dimension show Category --json"] }),
        json!({ "name": "dimension rename", "summary": "Renames a dimension.",
            "args": [{ "name": "id", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--name <str>", "help": "new name" }],
            "examples": ["amenbo dimension rename AMB-DIM-<n> --name \"Area\""] }),
        json!({ "name": "dimension update", "summary": "Updates a dimension's name, notes, value ordering, and/or time-axis role. Only the given fields change.",
            "args": [{ "name": "id", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--name <str>", "help": "new name" },
                      { "name": "--notes <str>", "help": "new notes (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                      { "name": "--ordered <bool>", "help": "whether the values carry an explicit order" },
                      { "name": "--time-axis <bool>", "help": "name this axis the project's time axis (its values then carry periods), or unname it" }],
            "examples": ["amenbo dimension update AMB-DIM-<n> --notes \"how we slice work\"",
                         "amenbo dimension update Era --time-axis true"] }),
        json!({ "name": "dimension move", "summary": "Reorders a dimension within its project.",
            "args": [{ "name": "id", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo dimension move AMB-DIM-<n> --after AMB-DIM-<m>"] }),
        json!({ "name": "dimension rm", "summary": "Deletes a dimension permanently; its values and task assignments go with it (alias: delete).",
            "args": [{ "name": "id", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo dimension rm AMB-DIM-<n> --yes"] }),
        json!({ "name": "dimension value-add", "summary": "Adds a value to a dimension (appended after existing values). On a time-axis dimension the value can carry a period.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id or name" }],
            "flags": [{ "name": "--name <str>", "help": "value name" },
                      { "name": "--start <date>", "help": "first day of the value's period (time-axis dimensions only)" },
                      { "name": "--end <date>", "help": "last day of the value's period; omit to leave it ongoing (time-axis dimensions only)" }],
            "examples": ["amenbo dimension value-add Category --name \"Design\"",
                         "amenbo dimension value-add Era --name \"Beta\" --start 2026-07-08"] }),
        json!({ "name": "dimension value-rename", "summary": "Renames a dimension value.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "flags": [{ "name": "--name <str>", "help": "new name" }],
            "examples": ["amenbo dimension value-rename Category Design --name \"Architecture\""] }),
        json!({ "name": "dimension value-update", "summary": "Updates a dimension value's name and/or its period (time-axis dimensions only). Only the given fields change; an open end means the period is ongoing.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "flags": [{ "name": "--name <str>", "help": "new name" },
                      { "name": "--start <date>", "help": "first day of the value's period (time-axis dimensions only)" },
                      { "name": "--end <date>", "help": "last day of the value's period; omit to leave it ongoing (time-axis dimensions only)" },
                      { "name": "--clear-start", "help": "open the period's start" },
                      { "name": "--clear-end", "help": "open the period's end (the value becomes ongoing)" }],
            "examples": ["amenbo dimension value-update Era Beta --end 2026-12-31",
                         "amenbo dimension value-update Era Beta --clear-end"] }),
        json!({ "name": "dimension value-move", "summary": "Reorders a value within its dimension.",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "flags": [{ "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo dimension value-move Category Design --top"] }),
        json!({ "name": "dimension value-rm", "summary": "Deletes a dimension value permanently; its task assignments go with it (alias: value-delete).",
            "args": [{ "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo dimension value-rm Category Design --yes"] }),
        json!({ "name": "dimension set", "summary": "Assigns a task a value of a dimension. An axis is single-select, so the task's prior value on that axis is replaced.",
            "args": [{ "name": "task", "required": true, "help": "task ref (AMB-T-n)" },
                     { "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "examples": ["amenbo dimension set 42 Category Design"] }),
        json!({ "name": "dimension unset", "summary": "Clears a task's value of a dimension.",
            "args": [{ "name": "task", "required": true, "help": "task ref (AMB-T-n)" },
                     { "name": "dimension", "required": true, "help": "dimension id or name" },
                     { "name": "value", "required": true, "help": "value id or name (within the dimension)" }],
            "examples": ["amenbo dimension unset 42 Category Design"] }),

        cmd("task add", "Creates a task in a project. Break larger work into separate tasks linked with task depend (no subtasks).",
            json!([{ "name": "--title <str>", "required": true, "help": "title (required, non-empty)" },
                   { "name": "--project <id>", "help": "owning project (a project-less task is refused). A human must name one — omit it to list the existing projects. An AI does not: the binding fills the slot, and naming a project is refused" },
                   { "name": "--due <date>", "help": "due date (YYYY-MM-DD / today / +3d)" },
                   { "name": "--start <date>", "help": "start date" },
                   { "name": "--priority <high|medium|low>", "help": "priority" },
                   { "name": "--notes <str>", "help": "description (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                   { "name": "--to <who>", "help": "delegate at creation to a facet — a name / `me` / `human` (the human), or `me-ai` / `ai` (the human's AI); same as a follow-up task assign, saving the create+assign round trip" },
                   { "name": "--ai", "help": "with --to, delegate to 'that person's AI' (assignee_kind=ai)" }]),
            json!(["amenbo task add --title \"Create wireframes\" --due tomorrow --priority high",
                   "amenbo task add --title \"Triage logs\" --to Alice --ai"])),
        cmd("task list", "Lists tasks. --limit/--offset page in sort order (JSON carries total_matched = the count before paging, count = this page).",
            json!([{ "name": "--project <id>", "help": "filter by project (human only — an AI is already scoped to its bound project)" },
                   { "name": "--filter <expr>", "help": "filter expression (see filterGrammar)" },
                   { "name": "--sort <key>", "help": "sort (order/due/priority/created/title; prefix - for descending)" },
                   { "name": "--limit <n>", "help": "max count (in sort order; pairs with --offset for paging)" },
                   { "name": "--offset <n>", "help": "number of items to skip in sort order (paging)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo task list --json",
                   "amenbo task list --filter \"done:false due:today\" --json",
                   "amenbo task list --sort -created --limit 20 --offset 20 --json"])),
        json!({ "name": "task show", "summary": "Shows task details — project, blockers (blocked_by) and dependents (blocks: what finishing this task would unblock).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo task show AMB-T-<n> --json"] }),
        json!({ "name": "task update", "summary": "Updates a task. --start is not a note to self: a day still ahead holds the task at ready:no and refuses its reserve, so declare one only when you mean it (--clear-start takes it back).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--title <str>", "help": "" }, { "name": "--notes <str>", "help": "" },
                      { "name": "--due <date>", "help": "" }, { "name": "--start <date>", "help": "" },
                      { "name": "--priority <high|medium|low>", "help": "" },
                      { "name": "--clear-due", "help": "clear the due date" },
                      { "name": "--clear-start", "help": "clear the start date" },
                      { "name": "--clear-priority", "help": "clear the priority" }],
            "examples": ["amenbo task update AMB-T-<n> --due +2d --priority medium"] }),
        json!({ "name": "task done", "summary": "Marks a task done.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task done AMB-T-<n>"] }),
        json!({ "name": "task reopen", "summary": "Returns a completed task to not-done (sugar for status=todo).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task reopen AMB-T-<n>"] }),
        json!({ "name": "task status", "summary": "Explicitly changes the progress state (todo/in_progress/done/blocked). Setting in_progress reserves the task: a compare-and-swap that succeeds only from todo, so a second session's reserve is rejected with already_reserved (the double-work guard), and only a ready task can be reserved, so an open blocker, an unsettled premise, or a start day still ahead is rejected with not_ready (there is no --force; correct the declaration with task update --start). todo hands it back. done marks completed. blocked declares an external stall only — an unmet premise is derived as ready:no, never declared here.",
            "args": [{ "name": "id", "required": true, "help": "task ID" },
                     { "name": "status", "required": true, "help": "todo / in_progress / done / blocked" }],
            "flags": [], "examples": ["amenbo task status AMB-T-<n> in_progress", "amenbo task status AMB-T-<n> done"] }),
        json!({ "name": "task block", "summary": "Marks blocked (stuck) — for an external stall only (a second machine, a human go/no-go); an unmet premise is derived as ready:no instead. --reason is recorded as a comment.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--reason <str>", "help": "reason it is stuck (recorded as a comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo task block AMB-T-<n> --reason \"Awaiting client confirmation\""] }),
        json!({ "name": "task move", "summary": "Re-homes a task to another project and reorders it (a task belongs to exactly one project).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--project <id>", "help": "destination project (omit it to reorder within the current project). An AI cannot re-home a task — every other project is outside its reach" },
                      { "name": "--before <id>", "help": "" }, { "name": "--after <id>", "help": "" },
                      { "name": "--top", "help": "" }, { "name": "--bottom", "help": "" }],
            "examples": ["amenbo task move AMB-T-<n> --project AMB-P-<n> --top"] }),
        json!({ "name": "task depend", "summary": "Makes this task depend on another task (--on becomes a blocker that must be done first — the edge has teeth: while the blocker is open, reserving this task is rejected with not_ready). Self-reference and cycles are rejected, and so is an edge that would cross projects (a project's context must not leak into another — both ends must sit in the same project; an inbox task, belonging to none, is not a crossing). Idempotent. Derived ready/blocked_by_open is reflected in the ready: filter of task show and list.",
            "args": [{ "name": "id", "required": true, "help": "the task ID being blocked" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the task ID of the blocker that must be done first" }],
            "examples": ["amenbo task depend AMB-T-<n> --on AMB-T-<m>"] }),
        json!({ "name": "task undepend", "summary": "Removes a dependency (idempotent). If removal makes the task startable, it emits task.unblocked.",
            "args": [{ "name": "id", "required": true, "help": "the task ID being blocked" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the blocker task ID to remove" }],
            "examples": ["amenbo task undepend AMB-T-<n> --on AMB-T-<m>"] }),
        json!({ "name": "task commit add", "summary": "Records a git commit SHA on a task (1 task : many commits) — the anchor from history back to a task, since a public commit carries no store-local reference. amenbo stores the SHA as an opaque string: it never reads git, verifies the commit, or knows which forge it lives on. The SHA is validated at the door — only full-length lower-case hex is admitted (40 for SHA-1, 64 for SHA-256), case is folded, and short forms, branches, tags and revisions are refused. Idempotent: a SHA already on the task is a no-op (the `(task_id, sha)` index sees bytes only).",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and revisions are refused)" }],
            "flags": [],
            "examples": ["amenbo task commit add AMB-T-<n> 0123456789abcdef0123456789abcdef01234567"] }),
        json!({ "name": "task commit list", "summary": "Lists a task's recorded commit SHAs, oldest first. To go the other way — the task a SHA belongs to — read the commit with `git show <sha>` (amenbo does not read git).",
            "args": [{ "name": "task", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo task commit list AMB-T-<n> --json"] }),
        json!({ "name": "task commit rm", "summary": "Forgets a commit SHA on a task — a hard delete (idempotent; the SHA is normalised the way it was stored, so any case removes it). The commit itself and the task are untouched.",
            "args": [{ "name": "task", "required": true, "help": "task ID" },
                     { "name": "sha", "required": true, "help": "the commit SHA to forget (any case — normalised the way it was stored)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo task commit rm AMB-T-<n> 0123456789abcdef0123456789abcdef01234567 --yes"] }),
        json!({ "name": "task assign", "summary": "Assigns an assignee to a task. Use --ai to delegate to 'that person's AI' (assignee_kind=ai). Reassignment is plain — the task just moves to its new assignee, with no special status.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--to <who>", "required": true, "help": "assignee facet: `me` / `self` / `human` or the human's display name → the human; `me-ai` / `ai` → the human's AI. The account-id / public-key forms are gone with the account reference dimension." },
                      { "name": "ai", "help": "delegate to 'that person's AI' (assignee_kind=ai)" }],
            "examples": ["amenbo task assign AMB-T-<n> --to Sato", "amenbo task assign AMB-T-<n> --to Sato --ai"] }),
        json!({ "name": "task unassign", "summary": "Removes a task's assignee.",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [], "examples": ["amenbo task unassign AMB-T-<n>"] }),
        json!({ "name": "task delete", "summary": "Deletes a task permanently, with its comments, dependency edges and attachments (a delete is physical and irreversible).",
            "args": [{ "name": "id", "required": true, "help": "task ID" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo task delete AMB-T-<n> --yes"] }),


        json!({ "name": "comment rm", "summary": "Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target comment ref, AMB-C-n (from `comment list`)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo comment rm AMB-C-<n> --yes"] }),
        json!({ "name": "comment edit", "summary": "Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay, so links to it keep resolving. Prefer this over deleting and re-posting when you only need to fix what a comment says. Identify the comment by id; `comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target comment ref, AMB-C-n (from `comment list`)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo comment edit AMB-C-<n> --text \"Corrected: the benchmark was 10k, not 1k\""] }),
        json!({ "name": "comment add", "summary": "Adds a comment to a task.",
            "args": [{ "name": "task", "required": true, "help": "target task ID" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo comment add AMB-T-<n> --text \"Awaiting client confirmation\""] }),
        json!({ "name": "comment list", "summary": "Shows a task's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).",
            "args": [{ "name": "task", "required": true, "help": "target task ID" }],
            "flags": [{ "name": "--limit <n>", "help": "max count (oldest first; pairs with --offset for paging)" },
                      { "name": "--offset <n>", "help": "number of items to skip, oldest first (paging)" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo comment list AMB-T-<n> --json", "amenbo comment list AMB-T-<n> --limit 20 --offset 20 --json"] }),
        json!({ "name": "comment attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single TASK comment, kept separate from the parent task's own attachments so a comment's own attachment timeline is preserved. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Identify the comment by id; find ids with `comment list <task> --json`. A decision comment is attached to with `decision comment attach` — the two comment tables number apart, so the command, not the id, says which table an id belongs to. List them with `attach ls --task-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target task comment ref, AMB-C-n (from `comment list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL)" }],
            "examples": ["amenbo comment attach AMB-C-<n> ./note.png", "amenbo comment attach AMB-C-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision comment attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a single DECISION comment — the mirror of `comment attach` (which takes task comments), down to the judgement of what is worth attaching (`task attach`). Identify the comment by id; find ids with `decision comment list <decision> --json`. List them with `attach ls --decision-comment <id>`.",
            "args": [{ "name": "comment", "required": true, "help": "target decision comment ref, AMB-C-n (from `decision comment list`)" },
                     { "name": "source", "required": true, "help": "file path to ingest as a blob, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL)" }],
            "examples": ["amenbo decision comment attach AMB-C-<n> ./benchmark.csv"] }),

        json!({ "name": "decision add", "summary": "Records a new decision (proposed) — an append-only \"why we chose X\". A decision is a Task sibling (project-scoped), NOT a task: it has no mailbox workflow and never appears in task lists. Decisions have their own device-global number space, shown as AMB-D-N (tasks are AMB-T-N); the kind code keeps AMB-D-<n> / AMB-T-<n> unambiguous. The body should be the conclusion + rationale (compress; do not paste raw discussion, and keep PII out). The project defaults to the bound project.",
            "args": [], "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                                   { "name": "--body <str>", "help": "conclusion + rationale (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." },
                                   { "name": "--project <id>", "help": "project (name or ID; defaults to the bound project — an AI omits it, and naming one is refused)" }],
            "examples": ["amenbo decision add --title \"Adopt UTC for stored timestamps\" --body \"store in UTC and localize on display — removes timezone ambiguity\" --project AMB-P-<n>"] }),
        cmd("decision list", "Lists decisions (status:proposed|accepted|rejected, current:yes|no — being superseded is not a status but a derived view, so current:no lists the decisions another decision supersedes — text: over title+body, project:, decided_before:/decided_after: over the day a decision was accepted (YYYY-MM-DD, or today/-30d; both ends inclusive; a decision that was never accepted has no such day and matches neither)). To ask which policies were settled by a date, compose this filter with current: — there is no separate as-of switch, and the composition recovers neither status transitions nor deleted decisions. Sort by decided/created/number/title/status (prefix - for descending; default -created). --limit/--offset page (JSON carries total_matched = the count before paging, count = this page). --with-body adds each decision's body to the rows — a projection that composes with --filter/--limit/--offset (narrow by keywords/status and page; it does not dump the whole corpus).",
            json!([{ "name": "--project <id>", "help": "limit to the given project (human only)" },
                   { "name": "--filter <expr>", "help": "e.g. status:accepted text:sync" },
                   { "name": "--sort <key>", "help": "decided/created/number/title/status (- for descending; default -created)" },
                   { "name": "--limit <n>", "help": "max count (in sort order; pairs with --offset for paging)" },
                   { "name": "--offset <n>", "help": "number of items to skip in sort order (paging)" },
                   { "name": "--with-body", "help": "include each decision's body (projection; composes with --filter/--limit/--offset)" },
                   { "name": "--json", "help": "machine-readable output" }]),
            json!(["amenbo decision list --filter status:accepted --json", "amenbo decision list --sort -decided", "amenbo decision list --filter \"status:accepted text:sync\" --with-body --limit 20 --json"])),
        json!({ "name": "decision show", "summary": "Shows a decision: body, status, the supersession chain (both directions), the premises it builds on (read those first — a premise another decision has overturned is flagged, because this decision then stands on rotten ground) and the decisions that build on it (the impact radius: overturn this one and they want revisiting), and its linked tasks.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision show AMB-D-<n> --json"] }),
        json!({ "name": "decision edit", "summary": "Edits a decision's title/body in place — proposed or accepted alike. Editing is not re-deciding, so an accepted decision's decided_at/decided_by are left untouched, and there is no revision history. Supersede when a new decision replaces it; a rejected decision is terminal and cannot be edited.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--title <str>", "help": "new title" }, { "name": "--body <str>", "help": "new body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision edit AMB-D-<n> --body \"…refined rationale…\""] }),
        json!({ "name": "decision accept", "summary": "Accepts a decision (proposed → accepted); stamps decided_at/decided_by. --reason records the reason for accepting as a decision comment — the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "reason for accepting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision accept AMB-D-<n>", "amenbo decision accept AMB-D-<n> --reason \"agreed after the perf review\""] }),
        json!({ "name": "decision reject", "summary": "Rejects a decision (proposed → rejected). --reason records the reason for rejecting as a decision comment — the reason lives on the timeline, not in a dedicated field.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--reason <str>", "help": "reason for rejecting (recorded as a decision comment). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision reject AMB-D-<n>", "amenbo decision reject AMB-D-<n> --reason \"superseded by the simpler approach in AMB-D-<m>\""] }),
        json!({ "name": "decision reopen", "summary": "Un-settles an accepted decision back to proposed (accepted → proposed; clears decided_at/decided_by), and sends the tasks that rest on it back to ready:no. Use it to pull a too-hastily accepted decision back into debate — neither reject (a verdict) nor supersede (a replacement) says that. It is not needed to edit: an accepted decision edits in place. No-op if already proposed; refused for rejected decisions. A decision another one supersedes stays accepted (currency is derived, not a status), so it can be reopened too.",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [], "examples": ["amenbo decision reopen AMB-D-<n>"] }),
        json!({ "name": "decision delete", "summary": "Deletes (retires) a decision — accepted ones included. The delete is physical and irreversible, its comments and edges go with it, and linked tasks are unlinked. Use this to retire a decision outright; use supersede when a new decision replaces it (which keeps the old one readable).",
            "args": [{ "name": "id", "required": true, "help": "decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--yes", "help": "skip the confirmation prompt" }],
            "examples": ["amenbo decision delete AMB-D-<n> --yes"] }),
        json!({ "name": "decision supersede", "summary": "Records that a new decision replaces an existing one (supersession chain): the new one is accepted and draws a `supersedes` edge at the old one, which stops being current (the old row itself is not touched — currency is derived from the edge, not stored). A decision may supersede several others — each supersede draws its own edge, none replaces the last.",
            "args": [{ "name": "decision", "required": true, "help": "the new decision (it replaces the old one)" }],
            "flags": [{ "name": "--replaces <id>", "required": true, "help": "the decision being replaced" }],
            "examples": ["amenbo decision supersede AMB-D-<n> --replaces AMB-D-<m>"] }),
        json!({ "name": "decision amend", "summary": "Records that a new decision amends (partially revises) an existing one: the new one draws an `amends` edge at the old one, which stays current (not superseded) — read the two together. A decision may amend several others (one edge each). Amend only records the revision link; it does not change either side's status (the amending side stays proposed until you accept it separately). Use supersede when the new decision fully replaces the old one.",
            "args": [{ "name": "decision", "required": true, "help": "the new decision (it amends the old one)" }],
            "flags": [{ "name": "--amends <id>", "required": true, "help": "the decision being amended (stays current)" }],
            "examples": ["amenbo decision amend AMB-D-<n> --amends AMB-D-<m>"] }),
        json!({ "name": "decision builds-on", "summary": "Records that a decision builds on (takes as a premise) an existing one: the standing decision draws a `builds_on` edge at the premise, which stays current and is not corrected — the edge only says read the premise first, and revisit this decision if the premise is ever overturned. Draw it only when that revisiting test says yes: same topic, cited in the body, or merely consulted is not a premise. supersedes / amends already imply it, so drawing it on a pair that carries one of them is a no-op (one pair, one edge). The reverse lookup is the impact radius `decision supersede` / `reject` / `delete` show you.",
            "args": [{ "name": "decision", "required": true, "help": "the decision that stands on the premise" }],
            "flags": [{ "name": "--on <id>", "required": true, "help": "the premise it stands on (stays current)" }],
            "examples": ["amenbo decision builds-on AMB-D-<n> --on AMB-D-<m>"] }),
        json!({ "name": "decision unlink", "summary": "Removes a decision-to-decision edge that should never have been drawn (supersedes / amends / builds_on alike — a pair carries one edge, so naming the pair names it). This is a correction, not a reversal of the decision: superseding a decision back is a new decision, whereas an edge drawn at the wrong target is a miswiring with nothing to remember. Removing a `supersedes` edge makes its target current again on its own (currency is derived from the edges, not stored). No-op when the pair carries no edge.",
            "args": [{ "name": "decision", "required": true, "help": "the decision the edge was drawn from (the newer one)" }],
            "flags": [{ "name": "--from <id>", "required": true, "help": "the decision it points at (the older one)" }],
            "examples": ["amenbo decision unlink AMB-D-<n> --from AMB-D-<m>"] }),
        json!({ "name": "decision link", "summary": "Links (or --unlink) a decision and a task — the decision is the task's premise (many-to-many). The edge is a precondition, not a mere cross-reference: the task cannot be reserved until the decision is settled (accepted and current), and a proposed or rejected premise, or one another decision supersedes, rejects the reserve with not_ready until it is ruled on, unlinked, or relinked to the successor. So link implementation tasks only — a purely historical reference belongs in the decision's body, not in an edge, and linking a decision to the task of deciding it locks that task forever. The task must sit in the decision's own project (no edge crosses projects; an inbox task, belonging to none, is not a crossing).",
            "args": [{ "name": "decision", "required": true, "help": "decision ref (AMB-D-n)" }, { "name": "task", "required": true, "help": "task ref (AMB-T-n)" }],
            "flags": [{ "name": "--unlink", "help": "remove the link instead of creating it" }],
            "examples": ["amenbo decision link AMB-D-<n> AMB-T-<n>", "amenbo decision link AMB-D-<n> AMB-T-<n> --unlink"] }),
        json!({ "name": "decision promote", "summary": "Promotes a task comment into a decision: the comment text becomes the body, the project defaults to the comment's task project, and the new decision is linked to that task.",
            "args": [{ "name": "comment", "required": true, "help": "the comment ref to promote, AMB-C-n" }],
            "flags": [{ "name": "--title <str>", "required": true, "help": "decision title" },
                      { "name": "--project <id>", "help": "project (defaults to the comment's task project)" }],
            "examples": ["amenbo decision promote AMB-C-<n> --title \"Standardize on ISO-8601 dates\""] }),
        json!({ "name": "decision comment rm", "summary": "Deletes a comment posted by mistake — permanently, and its attachments go with it. Identify the comment by id; `decision comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target comment ref, AMB-C-n (from `decision comment list`)" }],
            "flags": [{ "name": "--yes", "help": "skip confirmation" }],
            "examples": ["amenbo decision comment rm AMB-C-<n> --yes"] }),
        json!({ "name": "decision comment edit", "summary": "Rewrites a comment's body in place — the id, its place on the timeline, and its attachments all stay. This edits a comment, not the decision's own body (conclusion + rationale); the two are separate. Identify the comment by id; `decision comment list` prints it.",
            "args": [{ "name": "comment", "required": true, "help": "target comment ref, AMB-C-n (from `decision comment list`)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment edit AMB-C-<n> --text \"Corrected: the benchmark was 10k, not 1k\""] }),
        json!({ "name": "decision comment add", "summary": "Adds a comment to a decision's timeline. The decision's own body (conclusion + rationale) holds what was decided; comments are the way to discuss it or record accept/reject reasoning (`decision accept/reject --reason` is thin sugar over this).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--text <str>", "required": true, "help": "comment body (Markdown). Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument)." }],
            "examples": ["amenbo decision comment add AMB-D-<n> --text \"revisited after the 10k benchmark — still holds\""] }),
        json!({ "name": "decision comment list", "summary": "Shows a decision's comments, oldest first. --limit/--offset page (JSON carries total_matched = the count before paging, count = this page).",
            "args": [{ "name": "decision", "required": true, "help": "target decision ref (AMB-D-n)" }],
            "flags": [{ "name": "--limit <n>", "help": "max count (oldest first; pairs with --offset for paging)" },
                      { "name": "--offset <n>", "help": "number of items to skip, oldest first (paging)" },
                      { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo decision comment list AMB-D-<n> --json"] }),

        json!({ "name": "task attach", "summary": "Attaches a file or external link to a task. WHAT TO ATTACH: an attachment's bytes are not searchable — `task list --filter text:` runs over title / notes / comment bodies only — so text is always the body's job, and attaching is giving up on ever finding it again. The test: if the content holds a word you might one day search for, that word stays in the body. An attachment is not where words go to disappear; it is where the backing evidence sits. Attach non-text you generated yourself — the screenshot of a GUI check (so the next session can see what you verified instead of taking your word for it), images, video, PDF, binary samples; they could never be searched anyway, so nothing is lost, but write one line in the body saying what is in it. Attach long raw data behind a conclusion — a failing run's log, a profile, a before/after benchmark: keep the conclusion and the fragments worth searching (the error line, the identifier) in the body, and attach only the raw data. Do not attach text that fits in the body (short output, a minimal repro), source or diffs (anchor those with `task commit add`), or reasoning and history (that is a decision record). A URL belongs in the body; `--url` adds a click path for the human GUI, it does not replace writing the link down. MECHANICS: a file is ingested as a content-addressed `blob` (the bytes are copied into the store keyed by their BLAKE3 digest; the truth source records only metadata — hash/filename/mime/size); --url instead records an external link (`url` mode, not managed). MIME is guessed from the file extension. The blob is checked against the per-file size cap before ingest. Manage attachments with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target task ref (AMB-T-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL)" }],
            "examples": ["amenbo task attach AMB-T-<n> ./design.png", "amenbo task attach AMB-T-<n> https://example.com/spec --url --name spec"] }),
        json!({ "name": "decision attach", "summary": "Attaches a file (content-addressed `blob`) or external link (--url) to a decision record. Same two modes as `task attach`, and the same judgement of what is worth attaching — read it there. Manage with `attach ls/show/open/rm`.",
            "args": [{ "name": "id", "required": true, "help": "target decision ref (AMB-D-n)" },
                     { "name": "source", "required": true, "help": "file path to ingest, or the external URL with --url" }],
            "flags": [{ "name": "--url", "help": "treat <source> as an external URL link instead of ingesting a file" },
                      { "name": "--name <str>", "help": "display label (defaults to the file name / URL)" }],
            "examples": ["amenbo decision attach AMB-D-<n> ./benchmark.csv"] }),
        cmd("attach ls", "Lists the attachments on a task, decision, or a single comment, in attach order. WHETHER TO OPEN ONE: decide from the metadata this prints — name, mime, size — and from what the body already told you. The body carries the conclusion and the searchable words (see `task attach`); an attachment is the backing evidence behind them, so most of the time the listing alone answers your question and reading the bytes only spends context. Open one when you actually need the evidence — the body's claim is what you must check, or the raw data is what you were sent for — and when in doubt, do not: an attachment you read and did not need has cost you the very context it was put there to save. To read one, `attach save --out <path>` writes the bytes to a file you can open (`attach open` hands it to the OS's default opener, which is the human's route, not yours). A comment is named by a flag, not by the positional target: the task and decision comment tables number apart, so a bare id cannot say which table it belongs to.",
            json!([{ "name": "target", "help": "task / decision ref (AMB-T-n / AMB-D-n — the kind code is what disjoins the two spaces)" },
                   { "name": "--task-comment <id>", "help": "list this task comment's attachments (id from `comment list`)" },
                   { "name": "--decision-comment <id>", "help": "list this decision comment's attachments (id from `decision comment list`)" }]),
            json!(["amenbo attach ls AMB-T-<n> --json", "amenbo attach ls --task-comment 42 --json"])),
        cmd("attach show", "Shows one attachment's metadata (kind, filename, mime, size, blob hash or url).",
            json!([{ "name": "id", "required": true, "help": "attachment id" }]),
            json!(["amenbo attach show 01ATT…"])),
        cmd("attach open", "Opens an attachment — a blob via the OS default opener, or the external URL. This puts it in front of the human at their screen; an agent reads an attachment with `attach save` instead. A blob whose bytes are not present locally reports not_found.",
            json!([{ "name": "id", "required": true, "help": "attachment id" }]),
            json!(["amenbo attach open 01ATT…"])),
        cmd("attach save", "Saves a blob attachment's bytes to a file — the CLI counterpart of the GUI's download (`open` only spills to a temp file, and `export` takes the whole store), and the way an agent reads an attachment: save it, then read the file. Decide whether it is worth reading before you save it, from `attach ls`'s metadata — the bytes land in your context and rarely repay it. `--out` is a file path, or an existing directory to save under the attachment's own filename; with no `--out` that filename lands in the current directory. Refuses to overwrite an existing destination unless `--force`. A URL attachment has no bytes to save (open the link with `attach open`); a blob whose bytes are not present locally reports not_found.",
            json!([{ "name": "id", "required": true, "help": "attachment id" },
                   { "name": "--out <path>", "help": "file path, or a directory to save under the attachment's filename (default: that filename in the CWD)" },
                   { "name": "--force", "help": "overwrite the destination if it exists (default refuses)" }]),
            json!(["amenbo attach save 01ATT… --out ./spec.pdf", "amenbo attach save 01ATT… --out ~/Downloads"])),
        cmd("attach rm", "Removes an attachment — permanently. The blob bytes are reclaimed with the attachment once nothing else references them (content-addressing means another attachment may share the same bytes — those are left alone). Bytes ingested within the last hour are kept for now, in case an attach is in flight elsewhere; the sweep in `doctor --fix` collects them later. Destructive — confirms unless --yes.",
            json!([{ "name": "id", "required": true, "help": "attachment id" },
                   { "name": "--yes", "help": "skip confirmation" }]),
            json!(["amenbo attach rm 01ATT… --yes"])),

        cmd("export", "Exports all data — everything on this device, as JSON, and nothing narrower: export exists for moving to another tool, which an excerpt or a human-readable table does not serve. The core of data sovereignty, and one way: amenbo writes your data out for whatever you move to next, and reads nothing back in — the way back is `restore` from a `backup` archive. `--out <dir>` writes an **export directory**: `export.json` plus `attachments/`, holding every attachment's actual file under the task or decision it hangs on (each row names its `export_path`). With no `--out` the same JSON streams to stdout — a stream has nowhere to put the files, so that shape carries records only.",
            json!([{ "name": "--out <path>", "help": "the export directory to create (must not exist yet). Default: stream to stdout" }]),
            json!(["amenbo export --out ./amenbo-export",
                   "amenbo export > ./amenbo-export.json"])),
        json!({ "name": "backup", "summary": "Backs up everything on this device — one database, holding every project — into one verified `.amenbo-backup` archive at the given path (VACUUM INTO: checkpointed, transactionally consistent, no torn DB+WAL; bounded-verified; the manifest records its migration generation). The attachment bytes (blobs) are bundled too, so a restore elsewhere brings the files back and not just the rows referencing them. Secrets (keys/identity) are not part of the engine, so none are included. The destination must not already exist (managed generation rotation is retired).",
            "args": [{ "name": "path", "required": false, "help": "destination .amenbo-backup archive that must not already exist" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo backup ./everything.amenbo-backup"] }),
        json!({ "name": "restore", "summary": "Restores this device from a verified `.amenbo-backup` archive at the given path — a destructive replace of the database the archive carries (all-or-nothing stage-and-swap; the replaced truth source is set aside with a timestamp; an archive newer than this build is refused — update first). The snapshot is validated before anything is swapped in, so an unusable archive is refused without harm. The archive's attachment bytes (blobs) are placed additively — a blob the machine already holds is left alone, and none are ever deleted. An archive written before the consolidation carries the pre-consolidation shape (a list of stores) and is refused whole by its layout version, before its manifest is even parsed, rather than partially applied: restore it with the build that wrote it. Destructive — confirms unless --yes.",
            "args": [{ "name": "path", "required": false, "help": "the .amenbo-backup archive to restore from (must exist and pass verification)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }, { "name": "--yes/-y", "help": "skip confirmation" }],
            "examples": ["amenbo restore ./everything.amenbo-backup --yes"] }),
        json!({ "name": "hard-erase comment", "summary": "Physically erases one or more task comments from this store's truth source — deletes the read-model row outright — then VACUUMs so the bytes leave the file (unrecoverable). An ordinary delete removes the row — and `comment rm` deletes a comment posted by mistake — but the freed pages keep their bytes readable until something reclaims them, so this is the deliberate, gated exception: use it for content that must be GONE from the file. Identify comments by id; find ids with `comment list <task> --json`. Human-gated maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the erased content — delete it after verifying.",
            "args": [{ "name": "ids", "required": true, "help": "comment ref(s) to erase, AMB-C-n" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase comment AMB-C-<n> --yes"] }),
        json!({ "name": "hard-erase decision", "summary": "Redacts an accepted decision's body: overwrites it with the given text in place (the prior body is physically replaced, not merely superseded), then VACUUMs — so one section can be removed while the decision keeps its number, links and other fields. The replacement body comes from --body, --body-file, or stdin. Destructive maintenance: takes a safety backup first (a `pre-erase-*.amenbo-backup` archive next to the store, which `restore` puts the store back from — only the newest is kept), confirms unless --yes, and is refused for the AI actor (a human must run it). The safety backup still holds the old body — delete it after verifying.",
            "args": [{ "name": "id", "required": true, "help": "decision reference (AMB-D-n)" }],
            "flags": [{ "name": "--body <text>", "help": "replacement body (Markdown); omit to use --body-file or stdin" }, { "name": "--body-file <path>", "help": "read the replacement body from this file instead of --body/stdin" }, { "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo hard-erase decision AMB-D-<n> --body-file ./redacted.md --yes"] }),
        json!({ "name": "plugin validate", "summary": "Validates a plugin manifest file against the catalog rules — a well-formed id, checksum, https url, repo, non-empty OS set and config schema — reporting every problem it finds so an author can self-check before opening a catalog PR. It reads the same rules amenbo enforces at the install/intake door, so the two never disagree. The path may be .yaml (the form authored in the catalog repo) or .json (the aggregated catalog.json form); the format is taken from the extension, defaulting to YAML. A manifest that does not even parse is reported too — a missing required field is the shape half of the fail-closed door. It opens no store and needs no binding, so it runs anywhere (a plugin checkout, CI). The exit code is the verdict: 0 valid, 1 invalid (or the file could not be read).",
            "args": [{ "name": "path", "required": true, "help": "path to the manifest file (.yaml or .json)" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin validate plugins/worktree.yaml", "amenbo plugin validate ./manifest.json --json"] }),
        json!({ "name": "plugin list", "summary": "Lists the plugins installed on this machine — name, description and the official badge — beside whose gate is open. The two facts sit together because installing a plugin never runs it: an installed plugin that fires nothing is the ordinary state, not a fault. Reads only the app-data plugins/ directory and this machine's enable state — no network, no catalog fetch — so it answers the same offline. A directory it cannot read as an install is skipped rather than allowed to hide the rest. --json adds each plugin's subscribed events and the path of the executable amenbo would run.",
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin list", "amenbo plugin list --json"] }),
        json!({ "name": "plugin enable", "summary": "Enables an installed plugin: records the one-time consent to run its code, and opens the gate it fires through. Installing puts a plugin on disk and nothing more — this is the step that lets it run, and it is asked once: a later disable keeps the consent, so re-enabling never asks again. Fail-closed on the settings the plugin's author marked required — while one is empty the enable is refused and the empty fields are named; fill them with `plugin config set` and enable again. amenbo checks only that a value is present, at the machine-default tier the gate itself lives at (a per-project override cannot satisfy a machine-global enable); whether the value is *meaningful* is the plugin author's to judge at run time. Fail-closed on compatibility too: a plugin whose manifest reads a different event-payload contract than this amenbo speaks, or needs an amenbo newer than the one running, is refused with both versions named — update amenbo (or the plugin) rather than run one against a payload it cannot read.",
            "args": [{ "name": "name", "required": true, "help": "the installed plugin's name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin enable worktree"] }),
        json!({ "name": "plugin disable", "summary": "Closes an enabled plugin's gate: it stops firing, while staying installed and staying consented — so enabling it again later asks for nothing. Deliberately does not require the plugin to still read as installed: this is how a plugin is stopped, and a half-broken install is exactly when stopping it matters most. Disabling one that is already off changes nothing and says so.",
            "args": [{ "name": "name", "required": true, "help": "the plugin's name" }],
            "flags": [{ "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin disable worktree"] }),
        json!({ "name": "plugin uninstall", "summary": "Removes a plugin and everything it left behind: the binary and its directory, its settings in every project on this device, its secrets, and the recorded consent. Disabling stops a plugin while keeping all of that — this is the other end, and the difference is the point: a re-install of the same name starts clean, asking for consent again and inheriting no setting. It works from the name alone and never asks whether the plugin still reads as installed, so it is also how a half-broken install is cleaned up; a name that holds nothing is reported as such, not an error. The steps run worst-residue-first — the gate and consent, then the secrets, then the settings, then the binary — so an interrupted removal leaves an inert directory and never a plugin that still fires. Confirms unless --yes.",
            "args": [{ "name": "name", "required": true, "help": "the plugin's name" }],
            "flags": [{ "name": "--yes/-y", "help": "skip confirmation" }, { "name": "--json", "help": "machine-readable output" }],
            "examples": ["amenbo plugin uninstall worktree --yes"] }),
    ])
}

/// The entry-point spec: **how to work here in full, the commands as an index**. The only difference
/// from [`build`] is `commands` — the exhaustive list of names, and nothing else. The entry point is
/// always read, so a fat one costs tens of thousands of tokens at the head of every session, however
/// small the task. The index's whole job is that no command goes unknown; what each one is for is
/// already mapped intent-first by `capabilities`, and the rest — summary, flags, args, examples —
/// comes back whole from the detail side (`agent --command <name>`, `<cmd> --help`, `agent --full`).
/// Nothing becomes unreachable; only the route to it changes.
pub fn build_index() -> Value {
    let mut spec = build();
    let index: Vec<Value> = spec["commands"]
        .as_array()
        .map(|cmds| cmds.iter().map(|c| c["name"].clone()).collect())
        .unwrap_or_default();
    if let Value::Object(map) = &mut spec {
        map.insert("commands".to_string(), Value::Array(index));
        // Unless we say it is an index, the AI reads it as the whole spec and hallucinates flags.
        // Say how to pull the rest, right here.
        map.insert(
            "commandDetail".to_string(),
            json!("`commands` is an index: every command's name, and nothing more. For what one is for, read `capabilities` — it maps intent to command names. To use one, pull its full spec — summary, flags, args, examples — with `amenbo agent --command <name>` (or `amenbo <cmd> --help`); never guess a flag from the name. `amenbo agent --full` prints every command's full spec at once, but you rarely need it — pull the two or three you are about to run."),
        );
    }
    spec
}

/// One command's full spec (`name` / `summary` / `args` / `flags` / `examples`) — the detail side an
/// index row points at. `name` is the name the command is registered under in the agent spec,
/// compound names with a space (`task add`) included; [`command_names`] is the canonical list.
pub fn command_spec(name: &str) -> Option<Value> {
    build()["commands"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .cloned()
}

/// Every command name registered in the agent JSON — what the "was this command ever registered?"
/// test compares against.
pub fn command_names() -> Vec<String> {
    build()["commands"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entries of [`JA_PHRASEBOOK`] we **chose not to translate** (translation identical to the
    /// source) — strings such as enum values, which translating would turn into a lie. An identical
    /// pair is indistinguishable from one somebody forgot to translate, so the intent has to be
    /// declared here, and [`ja_phrasebook_values_are_translated`] holds the two lists against each
    /// other: add to one without the other and it fails.
    const JA_VERBATIM: &[&str] = &[
        // The status values themselves — the identifiers the CLI accepts. Translating them would
        // teach a value nobody can type.
        "todo / in_progress / done / blocked",
    ];

    /// Discipline: one store, so `mode` is always the single `personal` shape.
    #[test]
    fn spec_is_single_personal_store() {
        assert_eq!(build()["mode"], "personal");
    }

    /// Wiring: `en` (the default, and any unknown locale) is character-for-character identical to
    /// the English source [`build`] — the prose is not touched at all.
    #[test]
    fn localized_en_is_identical_to_english_source() {
        assert_eq!(build_localized("en"), build());
        assert_eq!(build_localized("fr"), build(), "an unknown locale passes through in English too");
    }

    /// Wiring: `ja` swaps the prose fields (capability / summary / args.help / flags.help) for their
    /// translations, leaves an untranslated item in English, and never touches identifiers (`name`)
    /// or the CLI strings in examples.
    #[test]
    fn localized_ja_overlays_prose_only() {
        let en = build();
        let ja = build_localized("ja");

        // A capability group heading is translated.
        let en_caps = en["capabilities"].as_array().unwrap();
        let ja_caps = ja["capabilities"].as_array().unwrap();
        let en_reg = en_caps.iter().find(|c| c["commands"][0] == "task add").unwrap();
        let ja_reg = ja_caps.iter().find(|c| c["commands"][0] == "task add").unwrap();
        assert_eq!(en_reg["capability"], "Register a task");
        assert_eq!(ja_reg["capability"], "タスクを登録する");

        let find = |spec: &Value, name: &str| -> Value {
            spec["commands"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap().clone()
        };

        // A command.summary is translated.
        assert_eq!(find(&en, "version")["summary"], "Shows version information.");
        assert_eq!(find(&ja, "version")["summary"], "バージョン情報を表示します。");

        // The no-crossing-projects rule is stated in both languages. Rewrite the English and forget
        // the phrasebook and `tr` silently hands back the source, so catch it here: the Japanese
        // must not still be coming out in English.
        for (name, en_mark, ja_mark) in [
            ("task depend", "cross projects", "プロジェクトを跨ぐエッジも拒否"),
            ("decision link", "no edge crosses projects", "エッジはプロジェクトを跨がない"),
        ] {
            let en_sum = find(&en, name)["summary"].as_str().unwrap().to_string();
            let ja_sum = find(&ja, name)["summary"].as_str().unwrap().to_string();
            assert!(en_sum.contains(en_mark), "the EN {name} does not state the no-crossing rule: {en_sum}");
            assert!(ja_sum.contains(ja_mark), "the JA {name} has no translation to pull (still English): {ja_sum}");
        }

        // flags.help / args.help are translated; identifiers (name) are not.
        let ja_show = find(&ja, "task show");
        assert_eq!(ja_show["name"], "task show", "a command name is an identifier — it is never translated");
        assert_eq!(ja_show["args"][0]["name"], "id", "an argument name is an identifier — it is never translated");
        assert_eq!(ja_show["args"][0]["help"], "タスク ID", "args.help is translated");
        assert_eq!(ja_show["flags"][0]["help"], "機械可読な出力", "flags.help is translated");

        // The CLI strings in examples are untouched — a runnable line is as good as an identifier.
        assert_eq!(find(&ja, "version")["examples"], en["commands"].as_array().unwrap()
            .iter().find(|c| c["name"] == "version").unwrap()["examples"]);

        // The long blocks localize_prose does not reach (principles, workflow) pass through in
        // English even under ja, as does any prose with no translation.
        assert_eq!(ja["principles"], en["principles"]);
        assert_eq!(ja["agentCycle"], en["agentCycle"]);
    }

    /// Discipline: every item of the cold-path `cycles` must be self-describing — `kind` is
    /// mandatory, an `optional` item carries a `trigger`, a `backbone` item does not — and every
    /// command it names must exist.
    #[test]
    fn cycles_reference_real_commands() {
        let spec = build();
        let known: std::collections::HashSet<String> = command_names().into_iter().collect();
        let cycles = spec["cycles"].as_object().expect("cycles is an object");
        let mut item_count = 0;
        for (name, cycle) in cycles {
            if name == "description" {
                continue;
            }
            assert!(cycle["when"].as_str().is_some_and(|s| !s.is_empty()), "cycle {name} needs a `when`");
            for (bucket, expect_trigger) in [("backbone", false), ("optional", true)] {
                let items = cycle[bucket].as_array().unwrap_or_else(|| panic!("cycle {name}.{bucket} is an array"));
                for it in items {
                    item_count += 1;
                    let kind = it["kind"].as_str().unwrap_or("");
                    assert_eq!(kind, bucket, "item in {name}.{bucket} has kind {kind:?}: {it}");
                    assert!(it["step"].as_str().is_some_and(|s| !s.is_empty()), "item missing step text: {it}");
                    if expect_trigger {
                        assert!(it["trigger"].as_str().is_some_and(|s| !s.is_empty()), "optional item needs a trigger: {it}");
                    } else {
                        assert!(it.get("trigger").is_none(), "backbone item must not carry a trigger: {it}");
                    }
                    let cmds = it["commands"].as_array().expect("item.commands is an array");
                    for c in cmds {
                        let n = c.as_str().unwrap_or("");
                        assert!(known.contains(n), "cycles references unknown command {n:?}: {it}");
                    }
                }
            }
        }
        assert!(item_count >= 9, "expected every cycle to carry its items, got {item_count}");
    }

    /// Collects the localizable prose `build()` actually emits (capability / command.summary /
    /// args.help / flags.help), walking the same path `localize_prose` does. An empty help — a flag
    /// self-evident enough not to need one — has nothing to translate and is dropped.
    fn spec_prose(en: &Value) -> std::collections::HashSet<&str> {
        let mut prose: std::collections::HashSet<&str> = std::collections::HashSet::new();
        if let Some(caps) = en["capabilities"].as_array() {
            for c in caps {
                prose.extend(c["capability"].as_str());
            }
        }
        if let Some(cmds) = en["commands"].as_array() {
            for c in cmds {
                prose.extend(c["summary"].as_str());
                for bag in ["args", "flags"] {
                    if let Some(items) = c[bag].as_array() {
                        for item in items {
                            prose.extend(item["help"].as_str());
                        }
                    }
                }
            }
        }
        prose.remove("");
        prose
    }

    /// Every key of `JA_PHRASEBOOK` must match the spec's localizable prose character for
    /// character. Reword the spec and leave the key behind, and that item renders in English in the
    /// GUI.
    #[test]
    fn ja_phrasebook_has_no_orphan_keys() {
        let en = build();
        let prose = spec_prose(&en);
        let orphans: Vec<&str> =
            JA_PHRASEBOOK.iter().map(|(k, _)| *k).filter(|k| !prose.contains(k)).collect();
        assert!(orphans.is_empty(), "orphan keys in JA_PHRASEBOOK matching no spec prose: {orphans:#?}");
    }

    /// The other direction: every piece of the spec's localizable prose must have a translation in
    /// `JA_PHRASEBOOK`. Untranslated prose shows up in English in the GUI's command palette, so
    /// without this the palette drifts quietly back towards a half-English one with every command
    /// and flag added. Paired with the orphan-key test above, it holds the phrasebook and the spec
    /// in exact correspondence.
    #[test]
    fn ja_phrasebook_covers_every_spec_prose() {
        let en = build();
        let translated: std::collections::HashSet<&str> =
            JA_PHRASEBOOK.iter().map(|(k, _)| *k).collect();
        let mut missing: Vec<&str> =
            spec_prose(&en).into_iter().filter(|p| !translated.contains(p)).collect();
        missing.sort_unstable();
        assert!(missing.is_empty(), "spec prose with no translation in JA_PHRASEBOOK: {missing:#?}");
    }

    /// Every key can line up and the GUI still render in English, if the **values** are English. The
    /// two tests above only compare key sets, so this one inspects the translations themselves: each
    /// must contain Japanese (non-ASCII) and differ from its source. An item that is identical on
    /// purpose (an enum value) has to declare itself in [`JA_VERBATIM`], and is allowed only there.
    #[test]
    fn ja_phrasebook_values_are_translated() {
        let verbatim: std::collections::HashSet<&str> = JA_VERBATIM.iter().copied().collect();
        let untranslated: Vec<&str> = JA_PHRASEBOOK
            .iter()
            .filter(|(en, ja)| !verbatim.contains(en) && (en == ja || ja.is_ascii()))
            .map(|(en, _)| *en)
            .collect();
        assert!(
            untranslated.is_empty(),
            "JA_PHRASEBOOK translations still in English (forgotten; if identical on purpose, declare it in JA_VERBATIM): {untranslated:#?}"
        );

        // And back the other way: JA_VERBATIM declares an entry identical on purpose, so it had
        // better actually be identical. A half-translated item squatting here is an item the guard
        // above no longer covers.
        for en in JA_VERBATIM {
            let (_, ja) = JA_PHRASEBOOK
                .iter()
                .find(|(k, _)| k == en)
                .unwrap_or_else(|| panic!("orphan key in JA_VERBATIM, absent from JA_PHRASEBOOK: {en:?}"));
            assert_eq!(ja, en, "a JA_VERBATIM entry carries a translation (take it out of the list)");
        }
    }

    // ──────────────────── The two layers: index ⇄ full spec ────────────────────

    /// The entry point carries how to work here in full and the commands as an index. An index row
    /// is the name alone: anything else on it — a summary, let alone flags/args/examples — is the
    /// detail side leaking back into the layer that is read every session.
    #[test]
    fn the_entry_point_indexes_commands_and_keeps_everything_else_whole() {
        let full = build();
        let index = build_index();

        for key in ["principles", "operating", "agentCycle", "cycles", "conventions", "filterGrammar", "notes", "capabilities", "inspect"] {
            assert_eq!(index[key], full[key], "{key} is cut from the entry point (only commands are indexed)");
        }

        let rows = index["commands"].as_array().expect("commands is an array");
        let names: Vec<&Value> = full["commands"].as_array().unwrap().iter().map(|c| &c["name"]).collect();
        assert_eq!(rows.iter().collect::<Vec<_>>(), names, "the index is every command's name, in the source's order — a command missing from it cannot be pulled");
        for row in rows {
            assert!(row.as_str().is_some_and(|n| !n.is_empty()), "an index row is the name alone: {row}");
        }
        assert!(index["commandDetail"].as_str().unwrap().contains("--command"), "an index that does not say how to pull invites hallucination");
        assert!(index["commandDetail"].as_str().unwrap().contains("capabilities"), "a name-only index must point at the map from intent to name");
    }

    /// Removing information is a non-goal: every name in the index must lead to its full spec. Only
    /// the route changed.
    #[test]
    fn every_indexed_command_can_be_pulled_in_full() {
        for name in command_names() {
            let spec = command_spec(&name).unwrap_or_else(|| panic!("indexed command {name} cannot be pulled"));
            assert_eq!(spec["name"], json!(name));
            assert!(spec.get("summary").is_some(), "the full spec of {name} has no summary");
            // Whatever the entry point dropped comes back whole on the detail side.
            let full_row = build()["commands"].as_array().unwrap().iter()
                .find(|c| c["name"] == json!(name)).cloned().unwrap();
            assert_eq!(spec, full_row, "the pulled spec differs from the source of truth");
        }
        assert!(command_spec("no such command").is_none());
    }

    /// The entry point must teach how to explore — narrow, list, then open the few that matter. Lose
    /// that and the AI falls back on dumping everything, melting its context before it has started
    /// the work. The norm and the two-layer shape guard the same thing, so they are kept together.
    #[test]
    fn the_entry_point_teaches_narrowing_before_reading() {
        let operating = build()["operating"].as_array().unwrap().clone();
        let prose: String =
            operating.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
        for needle in ["--filter", "--limit", "task show"] {
            assert!(prose.contains(needle), "operating does not teach the exploration tool {needle}");
        }
    }
}
