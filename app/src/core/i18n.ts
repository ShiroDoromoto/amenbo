// Lightweight i18n: UI labels are localized by config.language (read through the snapshot). No
// translation library — just a ja/en label dictionary. The language is looked up from
// snapshot.language on every call (getSnapshot is a synchronous cache read, so it is safe to call
// during render). An unset or unknown language falls back to the default, `ja`.
import { getSnapshot } from "./snapshot";
import { type ErrorCode, isErrorCode } from "./errorCodes";
import { type DoctorIssueKind, isDoctorIssueKind } from "./doctorKinds";
import type { Priority, Status } from "../mock/types";

export type Lang = "ja" | "en";

/** Normalizes a BCP-47-ish code to a supported language; unset or unsupported means `ja`. */
export function normalizeLang(code?: string | null): Lang {
  return code?.toLowerCase().startsWith("en") ? "en" : "ja";
}

/** The current UI language (snapshot.language, normalized). */
export function currentLang(): Lang {
  return normalizeLang(getSnapshot().language);
}

/** The locale each language writes its dates in, when nothing else is asked for. */
const LANG_LOCALE: Record<Lang, string> = { ja: "ja-JP", en: "en-US" };

/**
 * The locale dates are written in — `config.date_locale` when it is set, otherwise the one that
 * goes with the language.
 *
 * Two settings, because they answer different questions: the language decides the words, this
 * decides the date's shape. They agree for most people, which is why the second one is normally
 * unset; it exists for the reader whose answers differ — a Japanese UI with ISO dates (`sv-SE`),
 * say.
 *
 * A tag the platform cannot use falls back to the language's rather than throwing: `Intl` rejects a
 * malformed tag with a `RangeError`, and nothing is stopping a typo from reaching here — the store
 * keeps the value opaque, since what is a usable locale is the formatter's judgement and not
 * something amenbo can settle when the value is written.
 */
export function dateLocale(): string {
  const fallback = LANG_LOCALE[currentLang()];
  const declared = getSnapshot().dateLocale?.trim();
  if (!declared) return fallback;
  try {
    return Intl.DateTimeFormat.supportedLocalesOf(declared).length > 0 ? declared : fallback;
  } catch {
    return fallback;
  }
}

const STATUS: Record<Lang, Record<Status, string>> = {
  ja: { todo: "未着手", in_progress: "進行中", done: "完了", blocked: "ブロック", rejected: "却下" },
  en: { todo: "To do", in_progress: "In progress", done: "Done", blocked: "Blocked", rejected: "Rejected" },
};

const PRIORITY: Record<Lang, Record<Priority, string>> = {
  ja: { high: "高", medium: "中", low: "低" },
  en: { high: "High", medium: "Med", low: "Low" },
};

const VIEW: Record<Lang, Record<"list" | "board" | "calendar" | "timeline", string>> = {
  ja: { list: "リスト", board: "ボード", calendar: "カレンダー", timeline: "タイムライン" },
  en: { list: "List", board: "Board", calendar: "Calendar", timeline: "Timeline" },
};

// Translation table for the UI chrome (fixed strings: nav, buttons, category headings). Keys read
// "area.name". Values carry no emoji — the JSX pairs an emoji with t(key), keeping decoration that
// does not depend on the language out of the dictionary.
const UI: Record<Lang, Record<string, string>> = {
  ja: {
    "topbar.refresh": "最新の状態に更新", "topbar.back": "戻る", "topbar.forward": "進む",
    "topbar.brandLink": "製品ページを開く",
    "side.smartViews": "スマートビュー", "side.projects": "プロジェクト", "side.other": "その他",
    "side.plugins": "プラグイン",
    "side.newProject": "新規プロジェクト", "side.newProjectPh": "プロジェクト名",
    "side.archived": "アーカイブ済み",
    "newproj.title": "新規プロジェクト", "newproj.nameLabel": "名前", "newproj.folderLabel": "フォルダ（任意）",
    "newproj.folderHint": "選ぶと、このフォルダで起動した AI がこのプロジェクトを操作できます（後で追加もできます）。.amenbo と AI 手引きだけを置き、フォルダの中身は触りません。",
    "newproj.chooseFolder": "フォルダを選ぶ", "newproj.changeFolder": "選び直す", "newproj.clearFolder": "外す",
    "newproj.create": "作成", "newproj.cancel": "やめる",
    "newproj.doneTitle": "プロジェクト『{name}』を作成しました",
    "newproj.doneCapability": "このフォルダで起動した AI がこのプロジェクトを操作できます。",
    "newproj.doneNoFolder": "フォルダは後から紐付けられます。紐付けると、そのフォルダで起動した AI がこのプロジェクトを操作できます。",
    "newproj.nextTitle": "次の一手",
    "newproj.copyStatus": "{cmd} status をコピー", "newproj.copied": "✓ コピーしました",
    "newproj.openTerminal": "ターミナルで開く", "newproj.openFinder": "Finder で開く",
    "newproj.openBoard": "ボードを開く",
    "projset.title": "プロジェクト設定", "projset.back": "← ボードへ戻る",
    "projset.general": "基本", "projset.nameLabel": "名前", "projset.notesLabel": "メモ",
    "projset.colorLabel": "色", "projset.viewLabel": "既定のビュー",
    "projset.save": "保存", "projset.saved": "✓ 保存しました", "projset.saving": "保存中…",
    "projset.notesPh": "このプロジェクトのメモ（任意）",
    "projset.danger": "アーカイブ・削除", "projset.archivedBadge": "アーカイブ済み",
    "projset.archive": "アーカイブする", "projset.unarchive": "アーカイブ解除",
    "projset.archiveHint": "アーカイブするとサイドバーの一覧から外れます（いつでも解除できます）。",
    "projset.delete": "プロジェクトを削除",
    "projset.deleteHint": "プロジェクトを削除します。属していたタスクもまとめて削除されます（残したい場合はアーカイブを使ってください）。",
    "projset.confirmArchive": "プロジェクト『{name}』をアーカイブしますか？",
    "projset.confirmUnarchive": "プロジェクト『{name}』のアーカイブを解除しますか？",
    "projset.confirmDelete": "プロジェクト『{name}』を削除しますか？属していたタスクもまとめて削除されます。",
    "projset.folders": "紐付けフォルダ",
    "projset.foldersHint": "これらのフォルダで起動した AI が、このプロジェクトを操作できます。",
    "projset.aiReady": "AI操作可", "projset.folderStale": "見つかりません",
    "projset.addFolder": "フォルダを追加", "projset.noFolders": "紐付けフォルダはまだありません。",
    "projset.unbind": "解除",
    "projset.confirmUnbind": "フォルダ「{path}」の紐付けを解除しますか？（.amenbo と管理ブロックを外します。ストアは残ります）",
    "projset.folderElsewhere": "別のところから来たポインタです。このフォルダの .amenbo はプロジェクト「{recorded}」と書いていますが、#{projectId} は「{actual}」です。再リンクで置き直せます。",
    "projset.folderNoSlug": "（名前なし）", "projset.relink": "再リンク",
    "projset.folderLegacyPointer": "このフォルダの .amenbo は移行前の古い形式で、どのプロジェクトを指しているか読めません。再リンクで今の形式に置き直せます。",
    "projset.folderNoPointer": "紐付けが切れています", "projset.folderNoPointerHint": "このフォルダに .amenbo がありません。ここで起動した AI はこのプロジェクトに解決しません。再リンクでポインタを置き直せます。",
    "nav.settings": "設定", "nav.onboarding": "はじめに",
    "nav.decisions": "決定記録", "nav.commands": "コマンド集",
    "dec.title": "決定記録",
    "dec.empty": "まだ決定はありません", "dec.new": "決定を記録", "dec.newTitlePh": "決定のタイトル",
    "dec.newBodyPh": "結論と根拠（議論の生ログは貼らない）", "dec.add": "記録する", "dec.cancel": "やめる",
    "dec.accept": "採択", "dec.reject": "却下", "dec.reopen": "議論に戻す",
    "dec.editAcceptedHint": "採択済みの編集は文言の訂正であって再決定ではありません（決定日時は変わりません）。覆すなら下の「リンクを張る」で新しい決定に置き換えてください。",
    "dec.status.proposed": "議論中", "dec.status.accepted": "採択済み", "dec.status.superseded": "置換済み", "dec.status.rejected": "却下",
    "dec.supersedes": "置き換えた決定", "dec.supersededBy": "置き換えた新しい決定", "dec.amends": "一部改訂した決定", "dec.amendedBy": "一部改訂した新しい決定", "dec.linkedTasks": "関連タスク",
    "dec.buildsOn": "前提にしている決定", "dec.builtOnBy": "これを前提にする決定",
    "dec.premiseStale": "この前提 {premise} は {by} に覆されています（見直しが要ります）",
    "dec.edge.add": "リンクを張る", "dec.edge.cancel": "やめる", "dec.edge.unlink": "外す",
    "dec.edge.unlinkConfirm": "{target} へのリンクを外します。決定そのものは取り消されません。よろしいですか？",
    "dec.edge.kind.supersedes": "これを置き換える", "dec.edge.kind.amends": "これを一部改訂する", "dec.edge.kind.buildsOn": "これを前提にする",
    "dec.edge.supersedeAccepts": "置き換えるからには決まっている——この議論中の決定は採択済みになります。",
    "dec.edge.supersedeAcceptsConfirm": "{target} を置き換えます。置き換えるからには決まっているので、この議論中の決定は採択済みになります。よろしいですか？",
    "dec.edge.supersedeRevisitConfirm": "{target} の上に立つ決定があります。置き換えるなら見直してください:\n{list}\n\nこのまま置き換えますか？",
    "dec.edge.searchPh": "リンク先の決定を検索（AMB-D-<n>・タイトル）",
    "dec.edge.noCandidates": "リンクを張れる決定がありません",
    "dec.notFound": "決定が見つかりません",
    "dec.unknownName": "(不明)",
    "dec.comments": "この決定への議論", "dec.reasonPh": "理由（任意・Markdown 可）を書く…",
    "dec.revisit": "この決定の上に立つ決定です。却下するなら見直してください:",
    "dec.filterAll": "すべて",
    "dec.searchFailed": "検索できなかった",
    "dec.searchPh": "タイトル・本文・コメント・AMB-D-<n> を検索",
    "dec.sort": "並び替え",
    "dec.sort.numberDesc": "番号が新しい順", "dec.sort.numberAsc": "番号が古い順",
    "dec.sort.decidedDesc": "決定日が新しい順", "dec.sort.decidedAsc": "決定日が古い順",
    "board.filter": "フィルタ:", "board.group": "グループ:",
    "filter.dim.status": "ステータス", "filter.dim.assignee": "担当", "filter.dim.priority": "優先度",
    "filter.opt.all": "すべて",
    "filter.opt.assignee.none": "未割り当て", "filter.opt.assignee.me": "自分", "filter.opt.assignee.meAi": "自分の AI",
    // The compound status option: both terminals, whichever way the task ended.
    "filter.opt.status.closed": "閉じた（完了・却下）",
    "board.searchPh": "タイトル・概要・コメント・AMB-T-<n> を検索",
    "board.addDimension": "分類", "board.dimensionNamePh": "分類名（Enter で追加）",
    "board.addDimensionValue": "値", "board.dimensionValuePh": "値名（Enter で追加）",
    "board.noDimensionValue": "(値なし)",
    "board.manageDimensions": "分類を管理",
    "dimmgr.title": "分類の管理", "dimmgr.close": "閉じる",
    "dimmgr.empty": "分類がありません。追加すると値を割り当てて列を割れます。",
    "dimmgr.namePh": "分類名", "dimmgr.notesPh": "説明（任意）",
    "dimmgr.values": "値", "dimmgr.valueNamePh": "値名",
    "dimmgr.addValue": "＋ 値", "dimmgr.addDimension": "＋ 分類を追加",
    "dimmgr.removeDim": "分類を削除", "dimmgr.removeValue": "値を削除",
    "dimmgr.ordered": "順序あり", "dimmgr.orderedHint": "値に順序を付け、下で並べ替えられるようにする",
    "dimmgr.timeAxis": "時間軸",
    "dimmgr.timeAxisHint": "この分類をプロジェクトの時間軸にする。値に期間を持たせ、今日を含む値を「現在」と示す",
    "dimmgr.moveUp": "上へ", "dimmgr.moveDown": "下へ",
    "dimmgr.periodStart": "開始日", "dimmgr.periodEnd": "終了日",
    "dimmgr.periodStartOpen": "開始日なし", "dimmgr.periodEndOpen": "継続中",
    "dimmgr.current": "現在", "dimmgr.currentHint": "今日がこの期間に入っています",
    "dimmgr.confirmRemoveDim": "分類「{name}」を削除しますか？値とタスクへの割り当ても外れます。",
    "dimmgr.confirmRemoveValue": "値「{name}」を削除しますか？この値のタスク割り当ても外れます。",
    // The column holds both terminals, so the affordance names them together — and the count beside the
    // heading stays the completed one alone, with the rejected said separately and never added in.
    "board.seeClosedInList": "閉じたものをリストで見る（全 {n} 件）",
    "board.rejectedCount": "却下 {n}",
    "board.seeMoreInList": "他 {n} 件をリストで見る",
    "board.notFound": "プロジェクトが見つかりません",
    "cal.today": "今日", "cal.prevMonth": "前の月", "cal.nextMonth": "次の月",
    "cal.noDue": "期日なし", "cal.empty": "期日付きのタスクがありません",
    "cal.more": "他 {n} 件", "cal.overdueDays": "{n}日超過", "cal.inDays": "あと{n}日",
    "card.assignee": "担当",
    "detail.tab.detail": "詳細", "detail.tab.activity": "アクティビティ",
    "detail.notFound": "タスクが見つかりません",
    "detail.unassign": "委任を解除", "detail.assignAi": "に任せる",
    "detail.assignee": "担当", "detail.unassigned": "担当者なし",
    "detail.project": "プロジェクト", "detail.none": "なし",
    "detail.blockedBy": "待ち", "detail.blockedByHint": "着手不可（依存）",
    "detail.notStarted": "着手日待ち",
    "detail.linkedDecisions": "動機の決定",
    "detail.premiseUnsettled": "根拠が未確定です。裁定を待つか link を外してください（予約できません）",
    "detail.priority": "優先度", "detail.priorityNone": "なし",
    "detail.notes": "メモ（Markdown）", "detail.edit": "編集", "detail.add": "追加",
    "detail.notesPh": "Markdown で概要を書く…", "detail.notesHint": "Markdown 可 · ⌘/Ctrl+Enter で保存 · Esc で取消",
    "detail.cancel": "取消", "detail.save": "保存", "detail.noNotes": "概要文はまだありません",
    "detail.activityCategory": "このタスクの動き", "detail.noComments": "まだコメントはありません",
    "detail.noActivity": "まだ動きはありません",
    "detail.commentPh": "コメントを書く…（Markdown 可・改行は Enter）",
    "detail.commentHint": "Markdown 可 · ⌘/Ctrl+Enter で送信", "detail.send": "送信",
    "detail.created": "作成", "detail.restoreHint": "削除は元に戻せません",
    "detail.delete": "削除", "detail.deleteTip": "このタスクを削除する（元に戻せません）",
    "detail.deleteConfirm": "「{title}」を削除しますか？",
    "comment.edit": "このコメントを編集",
    "comment.edited": "編集済み",
    "comment.remove": "このコメントを削除",
    "comment.removeConfirm": "このコメントを削除しますか？（添付ごと消え、元に戻せません）",
    "attach.section": "添付", "attach.add": "ファイルを添付", "attach.none": "添付はありません",
    "attach.dropHint": "ここにファイルをドロップ／または", "attach.dropActive": "ドロップして添付",
    "attach.download": "ダウンロード", "attach.remove": "取り除く",
    "attach.removeConfirm": "添付「{name}」を取り除きますか？", "attach.notLocal": "この端末に未取得（取得は今後対応）",
    "attach.unsupported": "プレビュー未対応の形式です", "attach.link": "リンク",
    "attach.failed": "添付に失敗しました",
    "commit.section": "コミット", "commit.add": "SHA を記録", "commit.none": "記録されたコミットはありません",
    "commit.placeholder": "コミット SHA（完全形・40 桁 または 64 桁 hex）",
    "commit.record": "記録", "commit.copy": "SHA をコピー", "commit.copied": "コピーしました",
    "commit.remove": "取り除く", "commit.removeConfirm": "コミット {sha} を取り除きますか？",
    "compose.new": "新規タスク", "compose.titlePh": "タイトル",
    "compose.notes": "メモ（Markdown・任意）", "compose.notesPh": "Markdown で概要を書く…（任意）",
    "compose.hint": "Enter で作成 · Esc で取消", "compose.cancel": "取消", "compose.create": "作成",
    // smart views (sidebar shows inbox/activity; archive is the header for the board-opened list)
    "smartview.inbox": "受信 @自分", "smartview.activity": "アクティビティ",
    "mailbox.notifyTitle": "amenbo 受信箱", "mailbox.notifyBody": "確認が必要な項目が {n} 件届きました",
    "mailbox.notifyFailed": "OS 通知を出せませんでした（システム設定で amenbo の通知を許可してください）",
    "pager.range": "{from}–{to} / {total}件", "pager.page": "{page}/{pages} ページ",
    // members screen
    // settings screen
    "settings.profile": "プロフィール", "settings.avatar": "アバター",
    "settings.facetNames": "表示名（人間 / AI）", "settings.facetNamesSave": "保存",
    "settings.humanNameLabel": "人間の表示名", "settings.aiNameLabel": "AI の表示名",
    "settings.facetNamesHint": "名簿の 2 表示名（人間 / AI）を変更します（空欄はその facet を据え置き）。",
    "settings.avatarChoose": "画像を選ぶ…", "settings.avatarReset": "identicon に戻す",
    "settings.avatarHint": "人間 / AI それぞれの顔を登録できます。画像は 96px に縮小して保存します（未設定なら facet ごとの identicon）。",
    "settings.appearance": "外観", "settings.theme": "テーマ", "settings.language": "言語",
    "settings.themeOs": "OS に従う", "settings.themeDark": "ダーク", "settings.themeLight": "ライト",
    "settings.developer": "開発者",
    "settings.perfLog": "perf ログ（計装）",
    "settings.perfLogNote": "読み取り/書き込み層の所要時間を計測し、予算超過を WARN で出します（core はロールングファイル、front はコンソール）。稼働中に切り替わります。",
    "settings.updates": "更新",
    "settings.updateCheck": "更新チェック",
    "settings.updateCheckOn": "オン",
    "settings.updateCheckOff": "オフ",
    "settings.updateCheckNote": "公開リリースに新しい版が出ていないかを確認します（インフラ面の通信のみ・ユーザーデータは送りません・タイムアウト付き・失敗は無視・1 日 1 回程度）。オフにすると確認しません。",
    "settings.perfLogOff": "オフ",
    "settings.perfLogBudget": "予算超過のみ",
    "settings.perfLogVerbose": "詳細（全イベント）",
    "settings.data": "データ", "settings.dataPath": "保存先",
    "settings.logs": "ログ",
    "settings.logsOpen": "ログのフォルダを開く",
    "settings.logsNote": "不具合を報告するときは、このフォルダを開いて中身を添付してください（動作ログと、オンにしていれば perf ログが入っています）。タスクや決定の中身は書き込まれません。",
    "settings.exportImport": "エクスポート",
    "settings.exportJson": "エクスポート",
    "settings.dataNote": "データはお使いの端末にローカル保存され、独自バイナリに閉じ込めません。エクスポートは全データ（全プロジェクト）をフォルダひとつに書き出します（他ツールへの移行用・片道）——可搬な JSON（export.json）と、添付ファイルの実体（attachments/・付いていたタスクや決定ごとに並びます）。amenbo へ戻すのはバックアップからの復元です。",
    "settings.exportDialogTitle": "エクスポート先（フォルダを作ります）",
    "settings.exportDone": "✓ エクスポートしました（{kb} KB・添付 {attachments} 件）",
    "settings.exportMissing": "／実体が見つからなかった添付が {missing} 件あります",
    "settings.transferCancelled": "キャンセルしました。",
    "settings.backup": "バックアップ・リストア",
    "settings.backupBtn": "バックアップ",
    "settings.restoreBtn": "バックアップから復元",
    "settings.backupDialogTitle": "バックアップの保存先",
    "settings.restoreDialogTitle": "復元するバックアップを選択",
    "settings.backupDone": "✓ バックアップを保存しました（{kb} KB）",
    "settings.restoreDone": "✓ 復元しました（添付 {attachments} 件）",
    "settings.restoreAside": "直前の状態は {path} へ退避しました（ここから元に戻せます）。",
    "settings.restoreSwept": "巻き戻せるのは最新の退避 1 本だけなので、古い退避 {n} 件を削除しました。",
    "settings.restoreMigrated": "アーカイブはバックアップした時の形のままではなく、この版へ運ばれました（形式 v{from} → v{to}: {steps}）。",
    "settings.restoreConfirm": "この端末のデータを、選んだバックアップで総入れ替えします。直前の状態はタイムスタンプ付きで自動退避するので元に戻せます。続けますか？",
    "settings.backupNote": "この端末のデータ（全プロジェクト）を、添付ファイルの実体ごと検証済みの単一ファイルへ書き出します（鍵は含まれません）。復元は破壊的ですが、直前の状態を自動退避します。端末外（iCloud 等）に保存する場合、ファイルは平文です——保存先の信頼は利用者の責任です（クラウド側の暗号とアカウント認証に委ねます）。",
    "settings.integrity": "整合性",
    "settings.doctor": "問題の確認と修復",
    "settings.doctorNote": "ストアの中（孤児参照など）と、この端末の紐付けフォルダ（.amenbo / AI 手引き）を検査します。検査そのものは何も書き換えません。CLI の amenbo doctor と同じ検査・同じ修復です。",
    "settings.doctorChecking": "検査中…",
    "settings.doctorRecheck": "再検査",
    "settings.doctorClean": "✓ 問題はありません。",
    "settings.doctorFound": "{errors} エラー / {warnings} 警告",
    "settings.doctorFix": "未参照ファイルと残骸の紐付けを掃除",
    "settings.doctorMore": "… 他 {count} 件",
    "settings.doctorNoneRepairable": "上の問題は、この掃除では直りません。",
    "settings.doctorFixing": "修復中…",
    "settings.doctorFixNote": "この掃除が触るのは2つだけです——どこからも参照されていない添付ファイルの回収と、どのプロジェクトも名乗らないフォルダ紐付けの忘却。上に並ぶ問題を直すものではありません。紐付けフォルダの問題は、直し方が一つに決まる行にだけボタンが付きます（決まらない行は、プロジェクト設定 > フォルダで紐付け先を選んでください）。",
    "settings.doctorRebind": "紐付け直す",
    "settings.doctorRepairing": "実行中…",
    "settings.doctorRepairDone": "✓ 直しました。",
    "settings.doctorFixDone": "✓ 修復しました（添付ファイル {blobs} 件・フォルダ紐付け {bindings} 件）",
    "settings.doctorFixNothing": "✓ 修復の対象はありませんでした。",
    "settings.dataOpPreparing": "準備中…",
    "settings.dataOpProgress": "[{done}/{total}] {phase}",
    "settings.dataOpProgressUnbounded": "[{done}] {phase}",
    "settings.dataOpCancel": "中止",
    "settings.dataOpCancelling": "中止しています…",
    "settings.dataOpPhase.snapshotting": "スナップショット作成",
    "settings.dataOpPhase.blobs": "添付ファイル",
    "settings.dataOpPhase.unpacking": "展開",
    "settings.dataOpPhase.verifying": "検証",
    "settings.dataOpPhase.copying": "書き込み",
    "settings.dataOpPhase.exporting": "エクスポート",
    "settings.dataOpPhase.migrating": "更新",
    "restart.title": "amenbo が更新されました",
    "restart.intro": "別のプロセスがストアを新しい形式へ更新しました。この画面は、まだメモリに残っている古い amenbo です。表示中のデータは既に古く、これ以上更新されません。",
    "restart.how": "再起動すると、ディスク上の新しい amenbo で開き直します（GUI と CLI は一体で配布されます）。",
    "restart.button": "再起動する",
    "restart.failed": "再起動できませんでした。amenbo を手動で終了して開き直してください。",
    "restart.stuck.title": "再起動しても直らないときは",
    "restart.stuck.intro": "ディスク上の amenbo がまだ古い、ということです。版を下げる道はありません——戻すなら、更新時に残った移行前バックアップから復元します。",
    "restart.stuck.how": "新しい版へ更新するか（GUI と CLI は一体配布です）、コマンドラインで移行前バックアップから復元してください:",
    "restart.stuck.command": "{cmd} restore <移行前バックアップ (.amenbo-backup)>",
    "restart.stuck.where": "移行前バックアップは、この端末のストアと同じフォルダに pre-migrate- で始まる名前で残っています。",
    "migrate.title": "データを更新しています",
    "migrate.intro": "この端末のストアを形式 v{from} から v{to} へ運びます（{steps} ステップ）。先に、まるごとの移行前バックアップを取ります。",
    "migrate.preparing": "この端末のストアを更新する準備をしています。別のプロセス（コマンドライン）が先に更新していれば、それが終わるのを待ちます。",
    "migrate.space": "移行前バックアップに 約 {required} MiB 必要です（アーカイブ 約 {archive} MiB ＋ 一時領域 約 {staging} MiB）。空きは 約 {free} MiB。",
    "migrate.safety": "終わるまで amenbo を閉じないでください。失敗したときは、開始前の状態へ丸ごと戻します。",
    "migrate.doneTitle": "データを更新しました",
    "migrate.doneIntro": "ストアを形式 v{version} へ更新しました。",
    "migrate.backupTo": "更新前のストア",
    "migrate.superseded": "戻れなくなった古い移行前バックアップ {count} 件を削除しました（戻れるのは最新の 1 本だけです）。",
    "migrate.olderBuilds": "古いバージョンの amenbo では、このストアをもう開けません（GUI と CLI は一体で配布されます）。",
    "migrate.continue": "amenbo を開く",
    "migrate.failedTitle": "更新に失敗しました",
    "migrate.retry": "やり直す",
    // activity screen
    "activity.filterKind": "種別", "activity.filterAll": "全体",
    "activity.filterSystem": "システム", "activity.filterComment": "発言",
    "activity.filterFacet": "担い手", "activity.filterHuman": "人間", "activity.filterAi": "AI",
    "activity.note": "人も AI も同じ流れを読む（AI は activity --json）",
    "activity.today": "今日", "activity.reply": "返信",
    "commands.note": "全コマンドの仕様（agent --json 由来・表示専用）",
    "commands.search": "コマンドを検索", "commands.empty": "コマンドがありません", "commands.loading": "読み込み中…",
    "commands.other": "その他", "commands.required": "必須", "commands.examples": "サンプル",
    // plugin market (the "find one" tab)
    "plugins.market": "マーケット", "plugins.searchPh": "プラグインを検索",
    "plugins.category": "カテゴリ", "plugins.anyCategory": "すべて",
    "plugins.os": "対応OS", "plugins.anyOs": "すべて",
    "plugins.os.macos": "macOS", "plugins.os.windows": "Windows", "plugins.os.linux": "Linux",
    "plugins.layer": "出所", "plugins.anyLayer": "すべて",
    "plugins.layer.official": "公式", "plugins.layer.listed": "掲載", "plugins.layer.third-party": "追加したカタログ",
    "plugins.sort": "並べ替え", "plugins.sort.featured": "おすすめ", "plugins.sort.new": "新着",
    "plugins.sort.name": "名前",
    "plugins.featured": "おすすめ",
    "plugins.added": "追加 {date}",
    "plugins.sources": "カタログ {count}", "plugins.offered": "{count} 件",
    "plugins.sourceDown": "接続できません", "plugins.addSource": "追加", "plugins.removeSource": "解除",
    "plugins.sourcePh": "カタログの catalog.json の URL（https://…）",
    "plugins.sourcesNote": "カタログを足すと、一覧に出るものが増え、その配布元の鍵で検証したプラグインを入れられるようになります。",
    "plugins.sourceKey": "鍵 {fp}", "plugins.sourceNoKey": "鍵なし（入れられません）",
    "plugins.sourceChecking": "確認中…",
    "plugins.trustTitle": "{url} を配布元として登録します。",
    "plugins.fingerprint": "署名鍵の指紋",
    "plugins.trustNote": "このカタログから入れるプラグインは、この鍵で検証されます。配布元が公表している指紋と同じか確かめてください。",
    "plugins.keyChangeNote": "鍵が変わった配布元は、取得をそこで止めます。信頼し直すには、解除してから登録し直してください。",
    "plugins.noKeyNote": "このカタログは鍵を公開していません。一覧は見られますが、ここからは何も入れられません。",
    "plugins.alreadyRegistered": "この URL は登録済みです。表示名（と、まだ鍵が無ければ鍵）だけが変わります。",
    "plugins.sourceName": "表示名", "plugins.sourceCancel": "やめる", "plugins.trustAndAdd": "信頼して登録",
    "plugins.count": "{shown} / {total} 件",
    "plugins.loading": "カタログを読み込み中…",
    "plugins.emptyCatalog": "掲載されているプラグインはまだありません",
    "plugins.emptyFilter": "条件に合うプラグインがありません",
    "plugins.unreachable": "{count} 件のカタログに接続できませんでした（一覧はその分欠けています）",
    "plugins.error": "カタログを読み込めませんでした",
    "plugins.dropped": "{count} 件はカタログの検証に通らず一覧に出ていません",
    "plugins.close": "閉じる",
    "plugins.openRepo": "GitHub で開く（{repo}）",
    "plugins.downloads": "⬇ {count}",
    "plugins.factsLoading": "GitHub から取得中…",
    "plugins.factsError": "GitHub から読めませんでした（カタログの情報だけ表示しています）",
    "plugins.rateLimited": "GitHub の回数制限に達しました。しばらく待つと取得できます。",
    "plugins.noReadme": "README はありません",
    "plugins.factsNote": "★・ダウンロード数・README は、開いたこの1件だけを GitHub から取得しています（カタログには入っていません）。数字は目安で、入れられるかどうかとは関係ありません。",
    "plugins.want.perDevice": "この端末ごとに有効化", "plugins.want.perProject": "プロジェクトごとに有効化",
    "plugins.want.events": "受け取る出来事: {events}",
    "plugins.want.settings": "入れたあとに設定するもの:", "plugins.want.secret": "秘密",
    "plugins.install": "インストール", "plugins.installing": "インストール中…",
    "plugins.installNote": "入れただけでは動きません。実行するには有効化が要ります。",
    "plugins.installed": "インストール済み", "plugins.enabledChip": "有効",
    "plugins.enable": "有効にする", "plugins.disable": "無効にする",
    "plugins.gate.machine": "この端末", "plugins.gate.project": "このプロジェクト",
    "plugins.enabledAt": "{where}で有効", "plugins.disabledAt": "{where}で無効",
    "plugins.pickProject": "プロジェクトを選択", "plugins.pickProjectNote": "このプラグインはプロジェクトごとに有効化します。対象のプロジェクトを選んでください。",
    "plugins.incompatible": "この版の amenbo では動きません",
    "plugins.droppedQueued": "未配達の {count} 件を捨てました。無効な間の出来事は届かず、有効に戻しても今からです。",
    "plugins.consentAsk": "「{name}」はこの端末で任意のコードを実行します。実行を許可しますか？",
    "plugins.consentOnce": "確認するのは初回だけです（この端末に記録します）。無効にしても記録は残ります。",
    "plugins.consentAgree": "許可して有効にする", "plugins.consentCancel": "やめる",
    // the installed screen (the "manage what you have" tab)
    "plugins.installedCount": "{count} 件",
    "plugins.incompatibleChip": "この版では動きません", "plugins.notFiring": "有効だが発火しません",
    "plugins.installsError": "インストール済みのプラグインを読めませんでした",
    "plugins.emptyInstalled": "この端末にはまだプラグインが入っていません",
    "plugins.emptyInstalledNote": "マーケットから入れられます。",
    // the settings form, generated from the schema the plugin's author declared
    "plugins.cfg.open": "設定", "plugins.cfg.hide": "設定を閉じる",
    "plugins.cfg.requiredUnset": "必須の未入力 {count} 件",
    "plugins.cfg.tier": "保存先",
    "plugins.cfg.tier.machine": "この端末の既定", "plugins.cfg.tier.project": "プロジェクトの上書き",
    "plugins.cfg.pickProject": "プロジェクトを選択",
    "plugins.cfg.pickProjectNote": "上書きするプロジェクトを選んでください。",
    "plugins.cfg.required": "必須", "plugins.cfg.unset": "未入力", "plugins.cfg.held": "設定済み",
    "plugins.cfg.fallback": "空のままなら、この端末の既定「{value}」を使います",
    "plugins.cfg.secretNote": "秘密はこの端末に1つだけ保存し、あとから表示することはできません（プロジェクトごとの上書きはありません）。",
    "plugins.cfg.secretReplace": "新しい値（入れ替えるときだけ）",
    "plugins.cfg.secretConfirm": "確認のためもう一度",
    "plugins.cfg.secretMismatch": "2つの入力が一致しません",
    "plugins.cfg.clear": "消す", "plugins.cfg.save": "保存", "plugins.cfg.saving": "保存中…",
    "plugins.cfg.saved": "保存しました", "plugins.cfg.cleared": "消しました",
    // the update banner and its explicit re-check
    "plugins.updates.title": "プラグインの更新があります（{count} 件）",
    "plugins.updates.apply": "更新", "plugins.updates.applyAll": "まとめて更新",
    "plugins.updates.applying": "更新中…",
    "plugins.updates.applied": "{count} 件を更新しました（有効/無効・設定はそのままです）",
    "plugins.updates.holdIncompatible": "{name}：新しい版はこの版の amenbo では動きません",
    "plugins.updates.holdSettings": "{name}：新しい版は未入力の必須設定を要求します（{keys}）",
    "plugins.updates.open": "インストール済みを開く",
    "plugins.updates.check": "更新を確認", "plugins.updates.checking": "確認中…",
    "plugins.updates.none": "更新はありません",
    "plugins.updates.waiting": "新しい版があります",
    "plugins.updates.rollback": "前の版に戻す",
    "plugins.updates.rollbackConfirm": "「{name}」を更新前の版に戻しますか？ 戻せるのは直前の1版だけで、戻すとその退避版は無くなります（有効/無効・設定・秘密はそのままです）。",
    "plugins.updates.rolledBack": "前の版に戻しました（{desc}）",
    // uninstall (what goes with it is the part worth saying out loud)
    "plugins.remove": "削除", "plugins.removing": "削除中…",
    "plugins.removeConfirm": "「{name}」を削除しますか？ 本体だけでなく、全プロジェクトの設定・秘密・許可の記録も削除されます。入れ直しても戻りません。",
    "plugins.removed": "{name} を削除しました（{what}）",
    "plugins.removedNothing": "{name} はこの端末にありませんでした",
    "plugins.removedPart.binary": "本体", "plugins.removedPart.settings": "設定",
    "plugins.removedPart.secrets": "秘密", "plugins.removedPart.consent": "許可の記録",
    "plugins.removedPart.runs": "実行ログ",
    "common.listSeparator": "・",
    // dynamic activity text (mutations.ts sysItem templates)
    "common.you": "あなた", "act.justNow": "たった今",
    "act.created": "「{title}」を作成", "act.completed": "「{title}」を完了",
    "act.reopened": "「{title}」を未完了に戻す", "act.statusChanged": "「{title}」のステータスを変更",
    "act.deleted": "「{title}」を削除",
    "act.assignedAi": "「{title}」を AI に委任", "act.unassigned": "「{title}」の担当を外す",
    "act.assignedTo": "「{title}」を {name} に割り当て",
    "app.loadError": "データを読み込めませんでした。", "app.loading": "読み込み中…",
    // lint hook consent: the question amenbo asks before writing into .git/hooks
    "hooks.title": "コミットに amenbo の参照が混ざらないようにしますか？",
    "hooks.why": "AMB-T-… のような参照は、それを発行したストアの外では何も意味しません。git のフックを置いて、コミットに混ざる前に止めます。",
    "hooks.scope": "訊くのはこの1回だけです。この答えは、amenbo が扱うリポジトリすべてに——これから追加するものにも——そのまま適用します。",
    "hooks.where": "{project} — {dir}",
    "hooks.yes": "はい（推奨）",
    "hooks.no": "いいえ",
    "hooks.hint": "後から `{cmd} hooks install` でも設置できます。個別に外すなら `{cmd} hooks uninstall`。",
    "hookSetup.title": "lint がコミットに掛かっていません",
    "hookSetup.where": "{project} — {dir}",
    "hookSetup.unwired": "{slots}: フックがありません。`{cmd}` で設置できます。",
    "hookRestored.title": "amenbo の lint ブロックを復旧しました",
    "hookRestored.slots": "{slots}: ブロックが変更・削除されていたので、現行の内容へ戻しました。",
    "app.crashTitle": "予期しないエラーが発生しました",
    "app.crashHint": "画面の描画中に問題が起きました。再読み込みで復帰できます。データは保存されています。",
    "app.crashReload": "再読み込み",
    "pane.close": "閉じる",
    "pane.discardConfirm": "未保存の入力があります。破棄して閉じますか？",
    "pane.resize": "ドラッグで幅を変更",
    "sidebar.resize": "ドラッグで幅を変更",
    "sidebar.collapse": "サイドバーをたたむ",
    "sidebar.expand": "サイドバーを開く",
    "health.title": "起動時の整合チェックで問題が見つかりました",
    "health.hint": "確認のみで自動修復はしません。設定 > 整合性で、すべての問題の確認と修復ができます。",
    "health.dismiss": "閉じる",
    "health.repair": "フォルダの紐付けを修復",
    "health.repairing": "修復中…",
    "health.repaired": "{count} 個のフォルダの紐付けを修復しました",
    "update.title": "アップデートがあります",
    "update.hint": "新しいバージョンが公開されています。アプリ内で更新できます（適用はこのボタンを押したときだけ・自動更新はしません）。",
    "update.open": "今すぐ更新",
    "update.checking": "更新を確認中…",
    "update.downloading": "ダウンロード中… {pct}%",
    "update.downloadingUnknown": "ダウンロード中…",
    "update.installing": "インストール中…",
    "update.ready": "更新の準備ができました。再起動して適用してください。",
    "update.restart": "再起動して適用",
    "update.dismiss": "閉じる",
    "update.upToDate": "最新です（v{version}）",
    "update.checkFailed": "更新を確認できませんでした",
    "managedBlock.title": "AI 手引き（CLAUDE.md / AGENTS.md）が古い版です",
    "managedBlock.hint": "アプリ更新で手引きブロックのフォーマットが変わりました（{count} フォルダ）。再同期するとマーカー内だけ現行版に更新します（あなたの記述は保持）。",
    "managedBlock.resync": "再同期",
    "managedBlock.resyncing": "再同期中…",
    "managedBlock.done": "AI 手引きを現行版へ再同期しました。",
    "orphanBinding.title": "どのプロジェクトのものでもないフォルダの紐付けが残っています",
    "orphanBinding.hint": "消したプロジェクトが索引に残した記録です（{count} フォルダ）。忘れても索引の行が消えるだけで、フォルダの中身にも .amenbo にも触れません。",
    "orphanBinding.forget": "索引から忘れる",
    "orphanBinding.forgetting": "処理中…",
    "orphanBinding.done": "残っていたフォルダの紐付けを索引から忘れました。",
    "common.equiv": "相当:", "common.otherSession": "他のセッション",
    "common.loadMore": "もっと見る（残り {n} 件）",
    "id.copyTip": "クリックでタスクIDをコピー", "id.copied": "コピーしました",
    "facet.human": "人", "facet.ai": "AI",
    // onboarding
    "setup.welcome": "ようこそ", "setup.tagline": "はじめに少しだけ設定します。後で設定からいつでも変えられます。",
    "setup.langQ": "表示言語を選んでください", "setup.nameQ": "あなたと、あなたの AI の呼び名は？",
    "setup.humanNamePh": "あなたの名前（例: 山田）", "setup.humanNameLabel": "あなたの表示名",
    "setup.aiNamePh": "AI の名前（既定: AI）", "setup.aiNameLabel": "AI の表示名",
    "setup.nameHint": "後で設定からいつでも変えられます（未入力なら既定の 人間 / AI）。",
    "setup.themeQ": "見た目のテーマ（任意）", "setup.skip": "スキップ", "setup.back": "戻る", "setup.next": "次へ", "setup.finish": "はじめる",
    "onboard.welcome": "amenbo へようこそ",
    "onboard.tagline": "人と AI で、ひとつのチーム。サーバーは要りません。データは端末から出ません。",
    "onboard.createLabel": "プロジェクトを作る", "onboard.createHint": "名前を付けて新しいプロジェクトを作成します",
    "onboard.createGo": "画面で作成",
    "onboard.openLabel": "既存のストアを開く", "onboard.openHint": "この端末にある既存ストアにこのフォルダを紐付けます",
    "onboard.projectIdPh": "プロジェクトID", "onboard.cliTag": "ターミナル",
    "onboard.copied": "✓ コマンドをコピー — ターミナルで実行", "onboard.manualCopy": "手動でコピー",
    "onboard.stepsTitle": "リファレンス: AI エージェントに任せる",
    "onboard.stepsIntro": "作成は上のボタンから。ここは AI エージェントに仕事を任せる基本の流れです（CLI でも同じことができます）。",
    "onboard.s1title": "このフォルダをプロジェクトにする",
    "onboard.s1a": "作業ディレクトリ直下に ", "onboard.s1b": "（ローカルの紐付け）と ", "onboard.s1c": " を置きます。",
    "onboard.s2title": "AI に覚えさせる",
    "onboard.s2a": " に「まず ", "onboard.s2b": " を実行して操作を学べ」。エージェントは一発で操作を習得します。",
    "onboard.s4title": "あとは置くだけ",
    "onboard.s4body": "タスクを AI 宛に置けば、AI が着手して自律で進めます。動きはアクティビティに流れます。",
    "onboard.s4cmd": "@Ai このPJを整理して",
    // list empty states
    "list.empty": "該当するタスクはありません", "list.emptyInbox": "あなた（と AI）宛の未対応はありません",
    "list.emptyArchived": "アーカイブした項目はありません",
    "list.unread": "未読", "list.archive": "受信箱からアーカイブ",
    // Inbox tabs (Inbox = the active inbox / Archived = set aside, restorable).
    "list.tabInbox": "受信", "list.tabArchived": "アーカイブ",
    // Inbox row actions: marking read (clear the dot, item stays) and archiving (set aside, restorable) are distinct.
    "list.markRead": "既読にする", "list.dismiss": "アーカイブ",
    // Archived tab row action: restore to the inbox (unarchive).
    "list.unarchive": "戻す", "list.unarchiveTitle": "受信箱へ戻す",
    // board / detail tooltips
    "status.changeTip": "ステータスを変更",
    // What the pull-down asks before a rejection. The wording says what is being kept — the reasoning —
    // rather than warning about the state change, which is undoable; the reason, once unwritten, is not.
    "reject.title": "{ref} を却下する",
    "reject.why": "やらないと決めた理由を残します（必須）。理由はタイムラインにコメントとして積まれます。",
    "reject.placeholder": "なぜやらないと決めたのか",
    "reject.confirm": "却下する", "reject.cancel": "やめる",
    "card.addTaskTip": "タスクを追加", "card.assigneeTip": "担当（任せた相手）",
    "block.deps": "着手不可（依存）: {names} の完了待ち",
    "block.decisions": "着手不可（根拠が未確定）: {refs}",
    "block.notStarted": "着手不可（着手日待ち）: {date} から",
    "premise.changed": "予約後に前提が変わった: {detail}",
    "premise.warn": "予約後に前提が変わりました（AMB-D-366）: {detail}。独立に仕上がる部分だけ完了するか、todo に戻して手放してください。",
    // One phrase covering both arms of the axis — unsettled (reopen/reject) and superseded. A word naming
    // only the first cannot describe a decision that is still accepted and merely stopped being current.
    "premise.noLongerSettled": "確定が外れた",
    "detail.premiseChanged": "予約後の変化",
    "detail.premiseChangedHint": "予約後に動いた前提（後から付いた／確定が外れた・着手可否が下がった）",
    "detail.premiseAdded": "予約後に付いた前提",
    "detail.premiseReopened": "予約後に確定が外れた前提（未採択化／置き換え。link は前からある）",
  },
  en: {
    "topbar.refresh": "Refresh from source", "topbar.back": "Back", "topbar.forward": "Forward",
    "topbar.brandLink": "Open the product page",
    "side.smartViews": "Smart views", "side.projects": "Projects", "side.other": "Other",
    "side.plugins": "Plugins",
    "side.newProject": "New project", "side.newProjectPh": "Project name",
    "side.archived": "Archived",
    "newproj.title": "New project", "newproj.nameLabel": "Name", "newproj.folderLabel": "Folder (optional)",
    "newproj.folderHint": "If you choose one, an AI launched in that folder can operate this project (you can add one later too). It only places .amenbo and the AI guide; the folder's contents are never touched.",
    "newproj.chooseFolder": "Choose a folder", "newproj.changeFolder": "Change", "newproj.clearFolder": "Clear",
    "newproj.create": "Create", "newproj.cancel": "Cancel",
    "newproj.doneTitle": "Created project “{name}”",
    "newproj.doneCapability": "An AI launched in this folder can now operate this project.",
    "newproj.doneNoFolder": "You can link a folder later; once linked, an AI launched there can operate this project.",
    "newproj.nextTitle": "Next",
    "newproj.copyStatus": "Copy {cmd} status", "newproj.copied": "✓ Copied",
    "newproj.openTerminal": "Open in terminal", "newproj.openFinder": "Reveal in Finder",
    "newproj.openBoard": "Open the board",
    "projset.title": "Project settings", "projset.back": "← Back to board",
    "projset.general": "General", "projset.nameLabel": "Name", "projset.notesLabel": "Notes",
    "projset.colorLabel": "Color", "projset.viewLabel": "Default view",
    "projset.save": "Save", "projset.saved": "✓ Saved", "projset.saving": "Saving…",
    "projset.notesPh": "Notes for this project (optional)",
    "projset.danger": "Archive & delete", "projset.archivedBadge": "Archived",
    "projset.archive": "Archive", "projset.unarchive": "Unarchive",
    "projset.archiveHint": "Archiving removes it from the sidebar list (you can unarchive anytime).",
    "projset.delete": "Delete project",
    "projset.deleteHint": "Deletes the project and all of its tasks (use Archive if you want to keep them).",
    "projset.confirmArchive": "Archive project “{name}”?",
    "projset.confirmUnarchive": "Unarchive project “{name}”?",
    "projset.confirmDelete": "Delete project “{name}”? All of its tasks are deleted too.",
    "projset.folders": "Linked folders",
    "projset.foldersHint": "An AI launched in these folders can operate this project.",
    "projset.aiReady": "AI-ready", "projset.folderStale": "missing",
    "projset.addFolder": "Add folder", "projset.noFolders": "No linked folders yet.",
    "projset.unbind": "Unbind",
    "projset.confirmUnbind": "Unbind folder “{path}”? (removes .amenbo and managed blocks; the store is kept)",
    "projset.folderElsewhere": "This pointer came from somewhere else: the folder's .amenbo names project “{recorded}”, but #{projectId} is “{actual}”. Re-link to rewrite it.",
    "projset.folderNoSlug": "(no name)", "projset.relink": "Re-link",
    "projset.folderLegacyPointer": "This folder's .amenbo is in the old pre-migration format, so which project it names can't be read. Re-link to rewrite it in the current format.",
    "projset.folderNoPointer": "not linked", "projset.folderNoPointerHint": "This folder has no .amenbo, so an AI launched here does not resolve to this project. Re-link to write the pointer back.",
    "nav.settings": "Settings", "nav.onboarding": "Get started",
    "nav.decisions": "Decisions", "nav.commands": "Commands",
    "dec.title": "Decision records",
    "dec.empty": "No decisions yet", "dec.new": "Record a decision", "dec.newTitlePh": "Decision title",
    "dec.newBodyPh": "Conclusion + rationale (don't paste raw discussion)", "dec.add": "Record", "dec.cancel": "Cancel",
    "dec.accept": "Accept", "dec.reject": "Reject", "dec.reopen": "Return to discussion",
    "dec.editAcceptedHint": "Editing an accepted decision is a wording fix, not a re-decision (the decided date does not change). To overturn it, supersede it with a new decision via “Link a decision” below.",
    "dec.status.proposed": "Proposed", "dec.status.accepted": "Accepted", "dec.status.superseded": "Superseded", "dec.status.rejected": "Rejected",
    "dec.supersedes": "Supersedes", "dec.supersededBy": "Superseded by", "dec.amends": "Amends", "dec.amendedBy": "Amended by", "dec.linkedTasks": "Linked tasks",
    "dec.buildsOn": "Builds on", "dec.builtOnBy": "Built on by",
    "dec.premiseStale": "This premise {premise} was superseded by {by} (worth a review)",
    "dec.edge.add": "Link a decision", "dec.edge.cancel": "Cancel", "dec.edge.unlink": "Unlink",
    "dec.edge.unlinkConfirm": "Remove the link to {target}. The decision itself is not undone. Continue?",
    "dec.edge.kind.supersedes": "Supersedes it", "dec.edge.kind.amends": "Amends it", "dec.edge.kind.buildsOn": "Builds on it",
    "dec.edge.supersedeAccepts": "Superseding means you have decided — this proposed decision becomes accepted.",
    "dec.edge.supersedeAcceptsConfirm": "Supersede {target}. Superseding means you have decided, so this proposed decision becomes accepted. Continue?",
    "dec.edge.supersedeRevisitConfirm": "These decisions stand on {target} — revisit them if you supersede it:\n{list}\n\nSupersede anyway?",
    "dec.edge.searchPh": "Search decisions to link (AMB-D-<n>, title)",
    "dec.edge.noCandidates": "No decision left to link",
    "dec.notFound": "Decision not found",
    "dec.unknownName": "(unknown)",
    "dec.comments": "Discussion", "dec.reasonPh": "Reason (optional, Markdown)…",
    "dec.revisit": "These decisions stand on this one — revisit them if you reject it:",
    "dec.filterAll": "All",
    "dec.searchFailed": "The search could not run",
    "dec.searchPh": "Search title / body / comments / AMB-D-<n>",
    "dec.sort": "Sort",
    "dec.sort.numberDesc": "Number, newest first", "dec.sort.numberAsc": "Number, oldest first",
    "dec.sort.decidedDesc": "Decided, newest first", "dec.sort.decidedAsc": "Decided, oldest first",
    "board.filter": "Filter:", "board.group": "Group:",
    "filter.dim.status": "Status", "filter.dim.assignee": "Assignee", "filter.dim.priority": "Priority",
    "filter.opt.all": "All",
    "filter.opt.assignee.none": "Unassigned", "filter.opt.assignee.me": "Me", "filter.opt.assignee.meAi": "My AI",
    "filter.opt.status.closed": "Closed (done or rejected)",
    "board.searchPh": "Search title / notes / comments / AMB-T-<n>",
    "board.addDimension": "Category", "board.dimensionNamePh": "Category name (Enter to add)",
    "board.addDimensionValue": "Value", "board.dimensionValuePh": "Value name (Enter to add)",
    "board.noDimensionValue": "(No value)",
    "board.manageDimensions": "Manage categories",
    "dimmgr.title": "Manage categories", "dimmgr.close": "Close",
    "dimmgr.empty": "No categories yet. Add one to split columns by its values.",
    "dimmgr.namePh": "Category name", "dimmgr.notesPh": "Description (optional)",
    "dimmgr.values": "Values", "dimmgr.valueNamePh": "Value name",
    "dimmgr.addValue": "＋ Value", "dimmgr.addDimension": "＋ Add category",
    "dimmgr.removeDim": "Delete category", "dimmgr.removeValue": "Delete value",
    "dimmgr.ordered": "Ordered", "dimmgr.orderedHint": "Give the values an order so you can reorder them below",
    "dimmgr.timeAxis": "Time axis",
    "dimmgr.timeAxisHint": "Make this category the project's time axis: its values carry periods, and the one covering today is marked current",
    "dimmgr.moveUp": "Move up", "dimmgr.moveDown": "Move down",
    "dimmgr.periodStart": "Start date", "dimmgr.periodEnd": "End date",
    "dimmgr.periodStartOpen": "No start date", "dimmgr.periodEndOpen": "Ongoing",
    "dimmgr.current": "Current", "dimmgr.currentHint": "Today falls inside this period",
    "dimmgr.confirmRemoveDim": "Delete the category \"{name}\"? Its values and task assignments are removed too.",
    "dimmgr.confirmRemoveValue": "Delete the value \"{name}\"? Task assignments to this value are removed too.",
    "board.seeClosedInList": "See all {n} closed in a list",
    "board.rejectedCount": "{n} rejected",
    "board.seeMoreInList": "See {n} more in a list",
    "board.notFound": "Project not found",
    "cal.today": "Today", "cal.prevMonth": "Previous month", "cal.nextMonth": "Next month",
    "cal.noDue": "No due date", "cal.empty": "No tasks with a due date",
    "cal.more": "+{n} more", "cal.overdueDays": "{n}d overdue", "cal.inDays": "in {n}d",
    "card.assignee": "Assignee",
    "detail.tab.detail": "Details", "detail.tab.activity": "Activity",
    "detail.notFound": "Task not found",
    "detail.unassign": "Unassign", "detail.assignAi": "Delegate to AI",
    "detail.assignee": "Assignee", "detail.unassigned": "Unassigned",
    "detail.project": "Project", "detail.none": "None",
    "detail.blockedBy": "Waiting on", "detail.blockedByHint": "blocked (dependency)",
    "detail.notStarted": "Starts on",
    "detail.linkedDecisions": "Motivated by",
    "detail.premiseUnsettled": "premise not settled — wait for the ruling or unlink it (cannot reserve)",
    "detail.priority": "Priority", "detail.priorityNone": "None",
    "detail.notes": "Notes (Markdown)", "detail.edit": "Edit", "detail.add": "Add",
    "detail.notesPh": "Write notes in Markdown…", "detail.notesHint": "Markdown · save with ⌘/Ctrl+Enter · Esc to cancel",
    "detail.cancel": "Cancel", "detail.save": "Save", "detail.noNotes": "No notes yet",
    "detail.activityCategory": "Activity", "detail.noComments": "No comments yet",
    "detail.noActivity": "No activity yet",
    "detail.commentPh": "Write a comment… (Markdown, Enter for newline)",
    "detail.commentHint": "Markdown · send with ⌘/Ctrl+Enter", "detail.send": "Send",
    "detail.created": "Created", "detail.restoreHint": "deletion cannot be undone",
    "detail.delete": "Delete", "detail.deleteTip": "Delete this task (cannot be undone)",
    "detail.deleteConfirm": "Delete “{title}”?",
    "comment.edit": "Edit this comment",
    "comment.edited": "edited",
    "comment.remove": "Delete this comment",
    "comment.removeConfirm": "Delete this comment? Its attachments go with it, and this cannot be undone.",
    "attach.section": "Attachments", "attach.add": "Attach file", "attach.none": "No attachments",
    "attach.dropHint": "Drop files here, or", "attach.dropActive": "Drop to attach",
    "attach.download": "Download", "attach.remove": "Remove",
    "attach.removeConfirm": "Remove attachment “{name}”?", "attach.notLocal": "Not stored on this device yet (fetch coming later)",
    "attach.unsupported": "Preview not supported for this type", "attach.link": "Link",
    "attach.failed": "Attachment failed",
    "commit.section": "Commits", "commit.add": "Record SHA", "commit.none": "No commits recorded",
    "commit.placeholder": "Commit SHA (full — 40 or 64 hex digits)",
    "commit.record": "Record", "commit.copy": "Copy SHA", "commit.copied": "Copied",
    "commit.remove": "Remove", "commit.removeConfirm": "Remove commit {sha}?",
    "compose.new": "New task", "compose.titlePh": "Title",
    "compose.notes": "Notes (Markdown, optional)", "compose.notesPh": "Write notes in Markdown… (optional)",
    "compose.hint": "Enter to create · Esc to cancel", "compose.cancel": "Cancel", "compose.create": "Create",
    // smart views (sidebar shows inbox/activity; archive is the header for the board-opened list)
    "smartview.inbox": "Inbox @me", "smartview.activity": "Activity",
    "mailbox.notifyTitle": "amenbo inbox", "mailbox.notifyBody": "{n} item(s) need your attention",
    "mailbox.notifyFailed": "Couldn't show an OS notification (allow amenbo notifications in system settings)",
    "pager.range": "{from}–{to} of {total}", "pager.page": "page {page}/{pages}",
    // members screen
    // settings screen
    "settings.profile": "Profile", "settings.avatar": "Avatar",
    "settings.facetNames": "Display names (Human / AI)", "settings.facetNamesSave": "Save",
    "settings.humanNameLabel": "Human display name", "settings.aiNameLabel": "AI display name",
    "settings.facetNamesHint": "Changes the two roster names (Human / AI) (a blank field leaves that facet unchanged).",
    "settings.avatarChoose": "Choose image…", "settings.avatarReset": "Reset to identicon",
    "settings.avatarHint": "Register a face for Human and AI. Images are downscaled to 96px before saving (a per-facet identicon is used when unset).",
    "settings.appearance": "Appearance", "settings.theme": "Theme", "settings.language": "Language",
    "settings.themeOs": "Follow OS", "settings.themeDark": "Dark", "settings.themeLight": "Light",
    "settings.developer": "Developer",
    "settings.perfLog": "Perf log (instrumentation)",
    "settings.perfLogNote": "Times the read/write layers and WARNs on a budget bust (core to a rolling file, front to the console). Applies live.",
    "settings.updates": "Updates",
    "settings.updateCheck": "Update check",
    "settings.updateCheckOn": "On",
    "settings.updateCheckOff": "Off",
    "settings.updateCheckNote": "Checks whether a newer release has been published (infra-side traffic only — no user data, timeout, failures ignored, about once a day). Turn off to skip the check.",
    "settings.perfLogOff": "Off",
    "settings.perfLogBudget": "Budget busts only",
    "settings.perfLogVerbose": "Verbose (all events)",
    "settings.data": "Data", "settings.dataPath": "Location",
    "settings.logs": "Logs",
    "settings.logsOpen": "Open the logs folder",
    "settings.logsNote": "Reporting a bug? Open this folder and attach what is in it (the diagnostic log, plus the perf log if you turned it on). No task or decision content is written to either.",
    "settings.exportImport": "Export",
    "settings.exportJson": "Export",
    "settings.dataNote": "Your data is stored locally on your device and never locked into a proprietary binary. Export writes everything (all projects) out into one folder — for migrating to other tools, one way: portable JSON (export.json) plus the attachment files themselves (attachments/, laid out under the task or decision they hang on). The way back into amenbo is restoring a backup.",
    "settings.exportDialogTitle": "Export to (a folder is created)",
    "settings.exportDone": "✓ Exported ({kb} KB, {attachments} attachment(s))",
    "settings.exportMissing": " · {missing} attachment(s) had no file left to take",
    "settings.transferCancelled": "Cancelled.",
    "settings.backup": "Backup & restore",
    "settings.backupBtn": "Back up everything",
    "settings.restoreBtn": "Restore from backup",
    "settings.backupDialogTitle": "Save backup as",
    "settings.restoreDialogTitle": "Choose a backup to restore",
    "settings.backupDone": "✓ Backup saved ({kb} KB)",
    "settings.restoreDone": "✓ Restored ({attachments} attachment(s) written)",
    "settings.restoreAside": "The previous state was set aside at {path} (restore from there to undo this).",
    "settings.restoreSwept": "Only the latest set-aside store can be rewound to, so {n} earlier one(s) were removed.",
    "settings.restoreMigrated": "The archive was not left in the shape it was backed up in — it was brought forward to this version (format v{from} → v{to}: {steps}).",
    "settings.restoreConfirm": "This replaces this device's data with the chosen backup. It's reversible — the current state is set aside with a timestamp first. Continue?",
    "settings.backupNote": "Writes this device's data (every project) — attachment files and all — to one verified file (no keys included). Restore is destructive but sets the current state aside first. If you keep a backup off this machine (e.g. iCloud), the file is plaintext — trusting that destination is your responsibility (it relies on the cloud's own encryption and account authentication).",
    "settings.integrity": "Integrity",
    "settings.doctor": "Check and repair problems",
    "settings.doctorNote": "Checks inside the store (orphan references and the like) and this device's bound folders (.amenbo / AI guidance). The check itself writes nothing. Same checks and same repairs as `amenbo doctor` on the CLI.",
    "settings.doctorChecking": "Checking…",
    "settings.doctorRecheck": "Check again",
    "settings.doctorClean": "✓ No problems found.",
    "settings.doctorFound": "{errors} error(s) / {warnings} warning(s)",
    "settings.doctorFix": "Sweep unreferenced files and leftover bindings",
    "settings.doctorMore": "… and {count} more",
    "settings.doctorNoneRepairable": "None of the problems above is fixed by this sweep.",
    "settings.doctorFixing": "Repairing…",
    "settings.doctorFixNote": "The sweep touches two things and no others: attachment files nothing references any more, and folder bindings no project claims. It does not fix the problems listed above. Bound-folder problems get their own button on the lines whose fix is unambiguous (for the rest, pick the project in Project settings > Folders).",
    "settings.doctorRebind": "Re-bind",
    "settings.doctorRepairing": "Working…",
    "settings.doctorRepairDone": "✓ Fixed.",
    "settings.doctorFixDone": "✓ Repaired ({blobs} attachment file(s) · {bindings} folder binding(s))",
    "settings.doctorFixNothing": "✓ Nothing to repair.",
    "settings.dataOpPreparing": "Preparing…",
    "settings.dataOpProgress": "[{done}/{total}] {phase}",
    "settings.dataOpProgressUnbounded": "[{done}] {phase}",
    "settings.dataOpCancel": "Cancel",
    "settings.dataOpCancelling": "Cancelling…",
    "settings.dataOpPhase.snapshotting": "Snapshotting",
    "settings.dataOpPhase.blobs": "Attachments",
    "settings.dataOpPhase.unpacking": "Unpacking",
    "settings.dataOpPhase.verifying": "Verifying",
    "settings.dataOpPhase.copying": "Writing",
    "settings.dataOpPhase.exporting": "Exporting",
    "settings.dataOpPhase.migrating": "Updating",
    "restart.title": "amenbo has been updated",
    "restart.intro": "Another process updated the store to a newer format. This window is the old amenbo, still in memory. What you see is already stale, and it will not refresh again.",
    "restart.how": "Restarting reopens it with the new amenbo already on disk (the GUI and the CLI ship together).",
    "restart.button": "Restart",
    "restart.failed": "Could not restart. Quit amenbo and open it again.",
    "restart.stuck.title": "If restarting does not help",
    "restart.stuck.intro": "Then the amenbo on disk is still the old one. There is no downgrade — the way back is the pre-migration backup the update left behind.",
    "restart.stuck.how": "Either install the newer version (the GUI and the CLI ship together), or restore from that backup on the command line:",
    "restart.stuck.command": "{cmd} restore <pre-migration backup (.amenbo-backup)>",
    "restart.stuck.where": "The pre-migration backup sits in the same folder as this device's store, named starting with pre-migrate-.",
    "migrate.title": "Updating your data",
    "migrate.intro": "Carrying this device's store from format v{from} to v{to} ({steps} step(s)). A whole pre-migration backup is taken first.",
    "migrate.preparing": "Getting ready to update this device's store. If another process (the command line) got here first, this waits for it to finish.",
    "migrate.space": "The pre-migration backup needs ~{required} MiB (archive ~{archive} MiB + staging ~{staging} MiB). ~{free} MiB is free.",
    "migrate.safety": "Do not quit amenbo until this is done. If it fails, the store is put back exactly as it was.",
    "migrate.doneTitle": "Your data has been updated",
    "migrate.doneIntro": "The store is now at format v{version}.",
    "migrate.backupTo": "The store as it was",
    "migrate.superseded": "Removed {count} older pre-migration backup(s) nothing can go back to (only the newest one is a way back).",
    "migrate.olderBuilds": "Older versions of amenbo can no longer open this store (the GUI and the CLI ship together).",
    "migrate.continue": "Open amenbo",
    "migrate.failedTitle": "The update failed",
    "migrate.retry": "Try again",
    // activity screen
    "activity.filterKind": "Kind", "activity.filterAll": "All",
    "activity.filterSystem": "System", "activity.filterComment": "Comments",
    "activity.filterFacet": "By", "activity.filterHuman": "Human", "activity.filterAi": "AI",
    "activity.note": "Humans and AI read the same stream (AI uses activity --json)",
    "activity.today": "Today", "activity.reply": "Reply",
    "commands.note": "Full command reference (from agent --json · read-only)",
    "commands.search": "Search commands", "commands.empty": "No commands", "commands.loading": "Loading…",
    "commands.other": "Other", "commands.required": "required", "commands.examples": "Examples",
    // plugin market (the "find one" tab)
    "plugins.market": "Market", "plugins.searchPh": "Search plugins",
    "plugins.category": "Category", "plugins.anyCategory": "Any",
    "plugins.os": "OS", "plugins.anyOs": "Any",
    "plugins.os.macos": "macOS", "plugins.os.windows": "Windows", "plugins.os.linux": "Linux",
    "plugins.layer": "Source", "plugins.anyLayer": "Any",
    "plugins.layer.official": "Official", "plugins.layer.listed": "Listed", "plugins.layer.third-party": "Third-party",
    "plugins.sort": "Sort", "plugins.sort.featured": "Featured", "plugins.sort.new": "Newest",
    "plugins.sort.name": "Name",
    "plugins.featured": "Featured",
    "plugins.added": "added {date}",
    "plugins.sources": "Catalogs {count}", "plugins.offered": "{count} plugins",
    "plugins.sourceDown": "not reachable", "plugins.addSource": "Add", "plugins.removeSource": "Remove",
    "plugins.sourcePh": "URL of a catalog.json (https://…)",
    "plugins.sourcesNote": "Adding a catalog widens what you see here, and lets you install what its own key signed.",
    "plugins.sourceKey": "key {fp}", "plugins.sourceNoKey": "no key — nothing installs",
    "plugins.sourceChecking": "Checking…",
    "plugins.trustTitle": "Register {url} as a source.",
    "plugins.fingerprint": "Signing key fingerprint",
    "plugins.trustNote": "Plugins installed from this catalog will be verified on this key. Check it against the fingerprint its publisher states.",
    "plugins.keyChangeNote": "If this catalog's key changes, amenbo stops there. Trusting a new one means removing it and registering it again.",
    "plugins.noKeyNote": "This catalog publishes no key. You can browse it, and nothing on it can be installed.",
    "plugins.alreadyRegistered": "This URL is already registered. Only the name changes — and the key, if none is pinned yet.",
    "plugins.sourceName": "Name", "plugins.sourceCancel": "Cancel", "plugins.trustAndAdd": "Trust and register",
    "plugins.count": "{shown} of {total}",
    "plugins.loading": "Loading the catalog…",
    "plugins.emptyCatalog": "No plugins are listed yet",
    "plugins.emptyFilter": "No plugin matches these filters",
    "plugins.unreachable": "{count} catalog(s) could not be reached — the list is missing what they hold",
    "plugins.error": "The catalog could not be loaded",
    "plugins.dropped": "{count} entr(ies) did not pass the catalog's checks and are not listed",
    "plugins.close": "Close",
    "plugins.openRepo": "Open on GitHub ({repo})",
    "plugins.downloads": "⬇ {count}",
    "plugins.factsLoading": "Reading GitHub…",
    "plugins.factsError": "GitHub could not be read — showing what the catalog holds",
    "plugins.rateLimited": "GitHub is rate-limiting this address; the figures come back after a while.",
    "plugins.noReadme": "No README",
    "plugins.factsNote": "Stars, downloads and the README are fetched from GitHub for this one open plugin (the catalog does not carry them). The figures are a sense of scale, and have no bearing on what can be installed.",
    "plugins.want.perDevice": "Enabled once for this device", "plugins.want.perProject": "Enabled per project",
    "plugins.want.events": "Woken for: {events}",
    "plugins.want.settings": "Settings it will ask for:", "plugins.want.secret": "secret",
    "plugins.install": "Install", "plugins.installing": "Installing…",
    "plugins.installNote": "Installing does not run it. Enabling is what lets it fire.",
    "plugins.installed": "Installed", "plugins.enabledChip": "Enabled",
    "plugins.enable": "Enable", "plugins.disable": "Disable",
    "plugins.gate.machine": "this device", "plugins.gate.project": "this project",
    "plugins.enabledAt": "Enabled for {where}", "plugins.disabledAt": "Disabled for {where}",
    "plugins.pickProject": "Pick a project", "plugins.pickProjectNote": "This plugin is enabled per project. Pick the project it should fire in.",
    "plugins.incompatible": "This build of amenbo cannot run it",
    "plugins.droppedQueued": "{count} waiting event(s) were dropped. Nothing arrives while it is off, and enabling it again starts from now.",
    "plugins.consentAsk": "“{name}” runs code of its own on this device. Allow it to run?",
    "plugins.consentOnce": "Asked once and remembered on this device. Disabling later keeps the answer.",
    "plugins.consentAgree": "Allow and enable", "plugins.consentCancel": "Cancel",
    // the installed screen (the "manage what you have" tab)
    "plugins.installedCount": "{count} installed",
    "plugins.incompatibleChip": "This build cannot run it", "plugins.notFiring": "Enabled, but not firing",
    "plugins.installsError": "The installed plugins could not be read",
    "plugins.emptyInstalled": "No plugin is installed on this device yet",
    "plugins.emptyInstalledNote": "The market is where you add one.",
    // the settings form, generated from the schema the plugin's author declared
    "plugins.cfg.open": "Settings", "plugins.cfg.hide": "Hide settings",
    "plugins.cfg.requiredUnset": "{count} required setting(s) not provided",
    "plugins.cfg.tier": "Saved as",
    "plugins.cfg.tier.machine": "This device's default", "plugins.cfg.tier.project": "A project's override",
    "plugins.cfg.pickProject": "Pick a project",
    "plugins.cfg.pickProjectNote": "Pick the project whose override you are writing.",
    "plugins.cfg.required": "Required", "plugins.cfg.unset": "Not provided", "plugins.cfg.held": "Provided",
    "plugins.cfg.fallback": "Left empty, this project runs on the device default (“{value}”)",
    "plugins.cfg.secretNote": "A secret is kept once for this device and cannot be shown again (there is no per-project override).",
    "plugins.cfg.secretReplace": "A new value (only to replace it)",
    "plugins.cfg.secretConfirm": "Type it again to confirm",
    "plugins.cfg.secretMismatch": "The two entries do not match",
    "plugins.cfg.clear": "Clear", "plugins.cfg.save": "Save", "plugins.cfg.saving": "Saving…",
    "plugins.cfg.saved": "Saved", "plugins.cfg.cleared": "Cleared",
    // the update banner and its explicit re-check
    "plugins.updates.title": "{count} plugin update(s) available",
    "plugins.updates.apply": "Update", "plugins.updates.applyAll": "Update all",
    "plugins.updates.applying": "Updating…",
    "plugins.updates.applied": "Updated {count} (gates, settings and secrets are unchanged)",
    "plugins.updates.holdIncompatible": "{name}: this build of amenbo cannot run the new one",
    "plugins.updates.holdSettings": "{name}: the new build needs setting(s) not provided ({keys})",
    "plugins.updates.open": "Open installed",
    "plugins.updates.check": "Check for updates", "plugins.updates.checking": "Checking…",
    "plugins.updates.none": "Everything is up to date",
    "plugins.updates.waiting": "A newer build",
    "plugins.updates.rollback": "Go back a build",
    "plugins.updates.rollbackConfirm": "Put “{name}” back to the build before the update? Only the one build before it is kept, and going back uses it up (the gate, the settings and the secrets stay as they are).",
    "plugins.updates.rolledBack": "Back on the previous build ({desc})",
    // uninstall (what goes with it is the part worth saying out loud)
    "plugins.remove": "Remove", "plugins.removing": "Removing…",
    "plugins.removeConfirm": "Remove “{name}”? Not just the plugin: its settings in every project, its secrets and the permission you gave it go too. A re-install starts clean.",
    "plugins.removed": "Removed {name} ({what})",
    "plugins.removedNothing": "{name} was not on this machine",
    "plugins.removedPart.binary": "the plugin", "plugins.removedPart.settings": "settings",
    "plugins.removedPart.secrets": "secrets", "plugins.removedPart.consent": "the permission",
    "plugins.removedPart.runs": "the run log",
    "common.listSeparator": ", ",
    // dynamic activity text (mutations.ts sysItem templates)
    "common.you": "You", "act.justNow": "just now",
    "act.created": "Created “{title}”", "act.completed": "Completed “{title}”",
    "act.reopened": "Reopened “{title}”", "act.statusChanged": "Changed status of “{title}”",
    "act.deleted": "Deleted “{title}”",
    "act.assignedAi": "Delegated “{title}” to AI", "act.unassigned": "Unassigned “{title}”",
    "act.assignedTo": "Assigned “{title}” to {name}",
    // app / shell chrome
    "app.loadError": "Failed to load data.", "app.loading": "Loading…",
    // lint hook consent: the question amenbo asks before writing into .git/hooks
    "hooks.title": "Keep amenbo's refs out of your commits?",
    "hooks.why": "A ref like AMB-T-… means nothing outside the store that issued it. A git hook stops one before it reaches a commit.",
    "hooks.scope": "Asked once. Your answer covers the repositories amenbo works in, now and the ones you add later.",
    "hooks.where": "{project} — {dir}",
    "hooks.yes": "Yes (recommended)",
    "hooks.no": "No",
    "hooks.hint": "`{cmd} hooks install` sets this up later, whenever you want it. `{cmd} hooks uninstall` opts one repository out.",
    "hookSetup.title": "The lint is not running on your commits",
    "hookSetup.where": "{project} — {dir}",
    "hookSetup.unwired": "{slots}: no hook there. `{cmd}` installs it.",
    "hookRestored.title": "amenbo restored its lint block",
    "hookRestored.slots": "{slots}: the block had been changed or removed — restored it to the current version.",
    "app.crashTitle": "Something went wrong",
    "app.crashHint": "The screen failed to render. Reload to recover — your data is safe.",
    "app.crashReload": "Reload",
    "pane.close": "Close",
    "pane.discardConfirm": "You have unsaved input. Discard it and close?",
    "pane.resize": "Drag to resize",
    "sidebar.resize": "Drag to resize",
    "sidebar.collapse": "Collapse sidebar",
    "sidebar.expand": "Expand sidebar",
    "health.title": "Startup integrity check found problems",
    "health.hint": "Read-only check, no automatic repair. Settings > Integrity lists every problem and can repair them.",
    "health.dismiss": "Dismiss",
    "health.repair": "Repair the folder pointers",
    "health.repairing": "Repairing…",
    "health.repaired": "Repaired the pointers of {count} folder(s)",
    "update.title": "An update is available",
    "update.hint": "A newer version has been published. You can update in place from here — it is applied only when you press this button; amenbo never updates itself silently.",
    "update.open": "Update now",
    "update.checking": "Checking for the update…",
    "update.downloading": "Downloading… {pct}%",
    "update.downloadingUnknown": "Downloading…",
    "update.installing": "Installing…",
    "update.ready": "The update is ready. Restart to apply it.",
    "update.restart": "Restart to apply",
    "update.dismiss": "Dismiss",
    "update.upToDate": "You're up to date (v{version})",
    "update.checkFailed": "Couldn't check for updates",
    "managedBlock.title": "The AI guidance (CLAUDE.md / AGENTS.md) is out of date",
    "managedBlock.hint": "An app update changed the guidance block format ({count} folder(s)). Resyncing rewrites only what is inside the markers to the current version (your own content is preserved).",
    "managedBlock.resync": "Resync",
    "managedBlock.resyncing": "Resyncing…",
    "managedBlock.done": "Resynced the AI guidance to the current version.",
    "orphanBinding.title": "Some bound folders belong to no project",
    "orphanBinding.hint": "A deleted project left these rows in the index ({count} folder(s)). Forgetting them only drops the index rows — the folder contents and the .amenbo pointer are untouched.",
    "orphanBinding.forget": "Forget them",
    "orphanBinding.forgetting": "Forgetting…",
    "orphanBinding.done": "Forgot the leftover folder bindings.",
    "common.equiv": "Equivalent:", "common.otherSession": "another session",
    "common.loadMore": "Load more ({n} more)",
    "id.copyTip": "Click to copy task ID", "id.copied": "Copied",
    "facet.human": "Human", "facet.ai": "AI",
    // onboarding
    "setup.welcome": "Welcome", "setup.tagline": "A quick setup to get started. You can change everything later in Settings.",
    "setup.langQ": "Choose your language", "setup.nameQ": "What should we call you and your AI?",
    "setup.humanNamePh": "Your name (e.g. Alice)", "setup.humanNameLabel": "Your display name",
    "setup.aiNamePh": "AI name (default: AI)", "setup.aiNameLabel": "AI display name",
    "setup.nameHint": "You can change these anytime in Settings (defaults: Human / AI).",
    "setup.themeQ": "Theme (optional)", "setup.skip": "Skip", "setup.back": "Back", "setup.next": "Next", "setup.finish": "Get started",
    "onboard.welcome": "Welcome to amenbo",
    "onboard.tagline": "People and AI, one team. No server required. Your data never leaves your device.",
    "onboard.createLabel": "Create a project", "onboard.createHint": "Give it a name to create a new project",
    "onboard.createGo": "Create in app",
    "onboard.openLabel": "Open an existing store", "onboard.openHint": "Attach this folder to a store already on this device",
    "onboard.projectIdPh": "project-id", "onboard.cliTag": "Terminal",
    "onboard.copied": "✓ Command copied — run it in your terminal", "onboard.manualCopy": "Copy manually",
    "onboard.stepsTitle": "Reference: hand work to your AI",
    "onboard.stepsIntro": "Create a project with the button above. This is the basic flow for handing work to your AI agent (the same works from the CLI).",
    "onboard.s1title": "Make this folder a project",
    "onboard.s1a": "Places ", "onboard.s1b": " (a local binding) and ", "onboard.s1c": " in your working directory.",
    "onboard.s2title": "Teach your AI",
    "onboard.s2a": " says “first run ", "onboard.s2b": " to learn the commands.” Agents pick it up in one shot.",
    "onboard.s4title": "Then just drop tasks",
    "onboard.s4body": "Assign a task to the AI and it starts and proceeds autonomously. Everything shows up in the activity feed.",
    "onboard.s4cmd": "@Ai tidy up this project",
    // list empty states
    "list.empty": "No matching tasks", "list.emptyInbox": "Nothing pending for you (and your AI)",
    "list.emptyArchived": "No archived items",
    "list.unread": "Unread", "list.archive": "Archive from inbox",
    // Inbox tabs (Inbox = the active inbox / Archived = set aside, restorable).
    "list.tabInbox": "Inbox", "list.tabArchived": "Archived",
    // Inbox row actions: marking read (clear the dot, item stays) and archiving (set aside, restorable) are distinct.
    "list.markRead": "Mark read", "list.dismiss": "Archive",
    // Archived tab row action: restore to the inbox (unarchive).
    "list.unarchive": "Restore", "list.unarchiveTitle": "Restore to inbox",
    // board / detail tooltips
    "status.changeTip": "Change status",
    "reject.title": "Reject {ref}",
    "reject.why": "The reasoning is kept (required). It lands as a comment on the timeline.",
    "reject.placeholder": "Why this will not be done",
    "reject.confirm": "Reject", "reject.cancel": "Cancel",
    "card.addTaskTip": "Add a task", "card.assigneeTip": "Assignee (whom it's delegated to)",
    "block.deps": "Cannot start (dependency): waiting on {names}",
    "block.decisions": "Cannot start (premise not settled): {refs}",
    "block.notStarted": "Cannot start (waiting on its start day): from {date}",
    "premise.changed": "Premises changed after you reserved this: {detail}",
    "premise.warn": "Premises changed after you reserved this (AMB-D-366): {detail}. Finish only the part that stands on its own, or hand it back by setting it to todo.",
    "premise.noLongerSettled": "no longer settled",
    "detail.premiseChanged": "Changed since reserved",
    "detail.premiseChangedHint": "Premises that moved after you reserved this — pinned on, or no longer settled (readiness withdrawn)",
    "detail.premiseAdded": "Pinned on after you reserved this",
    "detail.premiseReopened": "Stopped being settled after you reserved this — reopened or superseded (the link is older)",
  },
};

/** Localizes a fixed UI-chrome string. An unknown key falls back to ja, then to the key itself. */
export function t(key: string, lang: Lang = currentLang()): string {
  return UI[lang][key] ?? UI.ja[key] ?? key;
}

/**
 * t() with interpolation: substitutes `{name}` placeholders from vars. Use it wherever the
 * dictionary value is a sentence template rather than a plain label — dynamic activity lines, or
 * labels that carry a count. A placeholder with no matching var is left as `{name}`, so a missing
 * substitution shows up on screen instead of vanishing.
 */
export function tf(
  key: string,
  vars: Record<string, string | number> = {},
  lang: Lang = currentLang(),
): string {
  const tmpl = UI[lang][key] ?? UI.ja[key] ?? key;
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => (k in vars ? String(vars[k]) : `{${k}}`));
}

/**
 * The structured error a Tauri command rejects with (mirrors `CmdError` in src-tauri/error.rs).
 * Localization maps the stable `code` onto a per-language template; a code with no template (the
 * free-text variants) falls back to the `message` (ja) / `message_en` (en) that core returns —
 * lossless, since core carries both languages correctly.
 */
export interface CmdError {
  code: string;
  message: string; // Japanese (core's Display)
  message_en: string; // English
  fields?: Record<string, unknown> | null;
}

function isCmdError(e: unknown): e is CmdError {
  if (typeof e !== "object" || e === null) return false;
  const o = e as Record<string, unknown>;
  return typeof o.code === "string" && typeof o.message === "string" && typeof o.message_en === "string";
}

// code → per-language template (`{name}` interpolated from fields). Only the codes whose full
// sentence can be reconstructed from the structured fields live here (ambiguous_id, binding_stale);
// everything else falls through to message. The keys are typed against the single source
// `ErrorCode`, so renaming a code on the Rust side breaks this at compile time rather than drifting.
const ERR: Record<Lang, Partial<Record<ErrorCode, string>>> = {
  ja: {
    ambiguous_id: "ID「{prefix}」は複数の候補に一致します（候補: {candidates}）",
    binding_stale: "プロジェクトの紐付け先ディレクトリが見つかりません: {path}",
  },
  en: {
    ambiguous_id: "The id “{prefix}” matches multiple candidates ({candidates})",
    binding_stale: "The linked project directory was not found: {path}",
  },
};

/** Renders a CmdError as one line in the current UI language: code template, else that language's message. */
export function errLabel(err: CmdError, lang: Lang = currentLang()): string {
  const tmpl = isErrorCode(err.code) ? ERR[lang][err.code] : undefined;
  if (tmpl) {
    const f = err.fields ?? {};
    return tmpl.replace(/\{(\w+)\}/g, (_, k) => {
      const v = (f as Record<string, unknown>)[k];
      if (v === undefined || v === null) return `{${k}}`;
      return Array.isArray(v) ? v.join(", ") : String(v);
    });
  }
  return lang === "en" ? err.message_en : err.message;
}

/**
 * One doctor issue (a Tauri `DoctorIssueDto` / an entry of `StartupHealthDto.issues`). Core holds no
 * prose for an issue — it returns only the kind (template id) and params (the specifics), and this
 * surface composes the sentence a human reads. The suggested fix is likewise written in terms of
 * **this surface's own affordances**: the "Repair" action under Settings > Integrity, re-linking a
 * folder in the project settings folder list, the re-sync banner for the AI guide. Never point a GUI
 * user at a CLI command — the CLI has its own English prose in
 * `crates/amenbo-cli/src/doctor_text.rs`.
 */
export interface DoctorIssueLike {
  kind: string;
  params: Record<string, string>;
}

type DoctorTemplate = { message: string; fix: string };

// kind → per-language template (`{name}` interpolated from params). The keys are an exhaustive
// Record typed against the single source `DoctorIssueKind`, so when Rust adds a kind this stops
// type-checking until the prose exists in both languages — a missing string cannot slip through.
const DOCTOR: Record<Lang, Record<DoctorIssueKind, DoctorTemplate>> = {
  ja: {
    self_dependency: {
      message: "依存 {dep} が自分自身を待っています。",
      fix: "タスクの依存からこの関係を外してください。",
    },
    duplicate_order_key: {
      message: "プロジェクト {project} で並び順（{order_key}）が重複しています。",
      fix: "タスクを並べ替えると解消します。",
    },
    stale_managed_block: {
      message: "{path} の AI 手引き（amenbo 管理ブロック）が古い版です（v{version} → v{current}）。",
      fix: "「再同期」でこのフォルダの手引きを現行版へ更新できます（あなたの記述は保持）。",
    },
    legacy_pointer: {
      message: "{path} は旧形式の紐付けです。ここで起動した AI はプロジェクトへ解決しません。",
      fix: "「紐付け直す」でプロジェクト #{project} へ紐付け直せます。",
    },
    legacy_pointer_ambiguous: {
      message: "{path} は旧形式の紐付けで、どのプロジェクトのものか定まりません。",
      fix: "プロジェクト設定 > フォルダで、このフォルダをどのプロジェクトに紐付けるか決めてください。",
    },
    missing_pointer: {
      message:
        "{dir} はプロジェクト #{project} の紐付けフォルダとして記録されていますが、目印（.amenbo）がありません。ここで起動した AI はそのプロジェクトへ解決しません。",
      fix: "「紐付け直す」で目印（.amenbo）を置き直せます（プロジェクト #{project}）。",
    },
    missing_pointer_ambiguous: {
      message:
        "{dir} は紐付けフォルダ（{claims}）として記録されていますが、目印（.amenbo）が無く、どのプロジェクトのものか定まりません。",
      fix: "プロジェクト設定 > フォルダで、このフォルダをどのプロジェクトに紐付けるか決めてください。",
    },
    orphan_binding: {
      message: "{dir} は紐付けフォルダとして残っていますが、主張するプロジェクトがありません（消したプロジェクトの残骸です）。",
      fix: "「修復する」で一覧から忘れます（フォルダの中身には触れません）。",
    },
    dead_ref: {
      message: "{at} の本文が {refs} を指していますが、その先はありません（読んだ人は空振りします）。",
      fix: "本文を開いて直してください——参照を消すか、代わりになるものへ向け直します。何が言いたかったかは書いた人にしか分かりません。",
    },
    start_after_due: {
      message: "{task} は着手日 {start_on}・期日 {due_on} です。期日を過ぎた日まで、このタスクは受信箱に出てきません。",
      fix: "どちらかの宣言が誤りです。着手日か期日のいずれかを直してください。打ち間違いがどちらなのかは決められないので、勝手にどちらかを採ることはしません。",
    },
  },
  en: {
    self_dependency: {
      message: "Dependency {dep} waits on itself.",
      fix: "Remove that link from the task's dependencies.",
    },
    duplicate_order_key: {
      message: "Project {project} has tasks sharing the same ordering key ({order_key}).",
      fix: "Re-order the tasks and it resolves itself.",
    },
    stale_managed_block: {
      message: "The AI guidance (amenbo managed block) in {path} is stale (v{version} → v{current}).",
      fix: "“Resync” brings this folder's guidance up to date (your own content is preserved).",
    },
    legacy_pointer: {
      message: "{path} is an old-format binding — an AI started there does not resolve to the project.",
      fix: "“Re-bind” binds the folder to project #{project} again.",
    },
    legacy_pointer_ambiguous: {
      message: "{path} is an old-format binding, and it does not point at one particular project.",
      fix: "Pick the project for this folder in Project settings > Folders.",
    },
    missing_pointer: {
      message:
        "{dir} is recorded as a folder bound to project #{project}, but the marker (.amenbo) is gone — an AI started there does not resolve to that project.",
      fix: "“Re-bind” puts the marker (.amenbo) back (project #{project}).",
    },
    missing_pointer_ambiguous: {
      message:
        "{dir} is recorded as a bound folder ({claims}), but the marker (.amenbo) is gone and it does not point at one particular project.",
      fix: "Pick the project for this folder in Project settings > Folders.",
    },
    orphan_binding: {
      message: "{dir} is still listed as a bound folder, but no project claims it (a leftover from a deleted project).",
      fix: "“Repair” forgets it from the list (the folder itself is untouched).",
    },
    dead_ref: {
      message: "The body at {at} points at {refs}, and there is nothing there — a reader sent after one comes back empty-handed.",
      fix: "Open the body and edit it: drop the ref, or point it at what stands in its place. Only the person who wrote it knows what it meant to say.",
    },
    start_after_due: {
      message: "{task} is set to start on {start_on} but was due on {due_on} — it stays out of the inbox until a day that is already past its deadline.",
      fix: "One of the two is wrong: correct either the start day or the due day. Nothing picks a winner between them, since either one could be the typo.",
    },
  },
};

function fill(tmpl: string, params: Record<string, string>): string {
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => (k in params ? params[k] : `{${k}}`));
}

/**
 * Turns a doctor issue into "what is broken" plus "how to fix it" in the current UI language. A kind
 * outside the contract — a newer core reporting an issue this build has never heard of — is printed
 * as the bare kind, so the screen degrades instead of crashing.
 */
export function doctorText(
  issue: DoctorIssueLike,
  lang: Lang = currentLang(),
): { message: string; fixHint: string } {
  if (!isDoctorIssueKind(issue.kind)) return { message: issue.kind, fixHint: "" };
  const tmpl = DOCTOR[lang][issue.kind];
  return { message: fill(tmpl.message, issue.params), fixHint: fill(tmpl.fix, issue.params) };
}

/**
 * Turns an invoke rejection into one human-readable line: a structured `CmdError` is localized by
 * code, a bare string or Error passes through. Every catch site must go through this — `String(e)`
 * would render a CmdError as "[object Object]".
 */
export function errText(e: unknown): string {
  if (isCmdError(e)) return errLabel(e);
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function statusLabel(s: Status, lang: Lang = currentLang()): string {
  return STATUS[lang][s];
}
export function priorityLabel(p: Priority, lang: Lang = currentLang()): string {
  return PRIORITY[lang][p];
}
export function viewLabel(v: "list" | "board" | "calendar" | "timeline", lang: Lang = currentLang()): string {
  return VIEW[lang][v];
}
