import { useEffect, useState } from "react";
import { Markdown } from "../components/Markdown";
import { errText, formatNumber, t, tf } from "../core/i18n";
import { openExternalUrl } from "../core/mutations";
import {
  pluginLayer,
  pluginLayerLabel,
  repoLinkBase,
  repoUrl,
  usePluginDetail,
  usePluginRepoFacts,
  type PluginDetail as PluginDetailDoc,
  type PluginEntry,
} from "../core/pluginCatalog";
import { formFields, installPlugin, type PluginInstall } from "../core/pluginInstalls";
import { pluginAbout, pluginDesc, settingLabel } from "../core/pluginText";
import { Icon } from "../components/Icon";

// The one plugin a user opened (`AMB-D-347`).
//
// The market list is drawn from the catalog alone, and everything that costs a request lives here
// instead: the stars, the current release's downloads and the README are per-repository, and the
// catalog's own detail document is per-plugin (`AMB-D-385`), so all of it is fetched when this opens and
// for this entry only. That asymmetry is the whole discovery design — browsing a catalog of thousands
// stays one static file, and a plugin is asked about only when someone actually wants to look at it.
//
// **The body is the author's own words where there are any** (`AMB-D-638`), in the reader's language
// (`AMB-D-623`). The README stood in for a description nobody could write anywhere else, and it is still
// what a plugin with no description of its own is drawn from — but where both exist they say the same
// thing twice, in two languages, so only one of them is on screen. Which one settles the GitHub request
// too: the README is not fetched where it would not be drawn, so the catalog's answer is waited for
// before the repository is asked anything.
//
// The figures never gate anything. What may be installed is decided by the asset's signature against
// Amenbo's own key (`AMB-D-371`); a star count is a display figure, and a download count includes
// whatever else pulls an asset, so both are read as a sense of scale and nothing more.

export function PluginDetail({ entry, install, onOpenInstalled, onClose }: {
  entry: PluginEntry;
  /** This machine's row for this entry, or `undefined` when it is not installed. */
  install?: PluginInstall;
  /** Go to the installed screen, which is where a plugin is turned on (`AMB-D-412`). */
  onOpenInstalled: () => void;
  onClose: () => void;
}) {
  const { detail } = usePluginDetail(entry.name);
  // `undefined` is the detail still on its way, and it is the one state that is not an answer: a
  // plugin the catalog does not carry (`null`) has no description, same as one whose author wrote none.
  const about = detail === undefined ? undefined : pluginAbout(detail ?? {});
  const { facts, loading, error } = usePluginRepoFacts(
    entry.repo,
    detail === undefined ? "unknown" : about === undefined,
  );
  const layer = pluginLayer(entry);

  // Escape closes it, like every other modal here — the detail is a look, and looking must be cheap to
  // back out of.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal__overlay" onClick={onClose}>
      <div className="plugdet" onClick={(e) => e.stopPropagation()}>
        <div className="plugdet__head">
          <strong className="plugdet__name">{entry.name}</strong>
          <span className={`chip ${layer === "official" ? "chip--official" : ""}`}>
            {pluginLayerLabel(entry)}
          </span>
          {/* The same pair the row wore, so opening an entry does not quietly drop a claim the list made. */}
          {entry.featured && <span className="chip chip--featured">{t("plugins.featured")}</span>}
          <span className="topbar__spacer" style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>{t("plugins.close")}</button>
        </div>

        <PluginActions entry={entry} install={install} onOpenInstalled={onOpenInstalled} />

        <div className="plugdet__desc">{pluginDesc(entry)}</div>
        <div className="plugdet__meta faint">
          <span>{entry.author}</span>
          <span>·</span>
          <span>{entry.category}</span>
          <span>·</span>
          <span>{entry.os.map((o) => t(`plugins.os.${o}`)).join(" / ")}</span>
          {entry.addedAt && (
            <>
              <span>·</span>
              <span>{tf("plugins.added", { date: entry.addedAt.slice(0, 10) })}</span>
            </>
          )}
        </div>

        {detail && <WhatItWants detail={detail} />}

        {/* Everything below this line came from GitHub, not from the catalog. */}
        <div className="plugdet__figures">
          <button className="feed__action" onClick={() => void openExternalUrl(repoUrl(entry.repo))}>
            {tf("plugins.openRepo", { repo: entry.repo })}
          </button>
          {loading && <span className="faint">{t("plugins.factsLoading")}</span>}
          {facts?.stars != null && <span><Icon name="star" /> {formatNumber(facts.stars)}</span>}
          {facts?.downloads != null && (
            <span><Icon name="arrowDown" /> {tf("plugins.downloads", { count: facts.downloads })}</span>
          )}
        </div>
        {/* Three different silences, and they are not the same news: too many requests means wait, a
            failure means the figures are missing but the entry is not, and neither is "this plugin has
            no stars". */}
        {facts?.rateLimited && <div className="plugdet__note">{t("plugins.rateLimited")}</div>}
        {error != null && !facts && <div className="plugdet__note">{t("plugins.factsError")}</div>}

        {/* Two bodies, and never both. The author's own text is published in the catalog, so a relative
            link in it names nothing this app could resolve and stays inert. The README is the one body
            here that came from somewhere: its relative paths name files in the repository it was read
            from, so that repository is what they are resolved against. */}
        <div className="plugdet__readme markdown">
          {about !== undefined ? (
            <Markdown>{about}</Markdown>
          ) : facts?.readme ? (
            <Markdown linkBase={repoLinkBase(entry.repo)}>{facts.readme}</Markdown>
          ) : (
            !loading && <span className="faint">{t("plugins.noReadme")}</span>
          )}
        </div>

        {/* The note names what was actually fetched, so it does not promise a README nothing went and
            got. */}
        <div className="plugdet__foot faint">
          {t(about !== undefined ? "plugins.figuresNote" : "plugins.factsNote")}
        </div>
      </div>
    </div>
  );
}

/**
 * What installing this plugin would mean, from the catalog's detail document (`AMB-D-385`).
 *
 * Everything here is the author's declaration, read **before** anything is installed: what layer it lives
 * at, what it will be woken for, and what it will want to be told — a secret among those is the line worth
 * seeing in advance, since it means handing over a credential. The one judgement is Amenbo's own:
 * a build this version cannot speak to says so here rather than at the enable that would refuse it.
 *
 * The layer is **said, not offered** (`AMB-D-601`). It is the author's declaration and there is nothing for
 * a reader to set, so it takes the place of the ordinary "enabled per project" line rather than arriving as
 * a second switch beside it: a device-wide plugin reads every project on this machine, and this is the face
 * where that is still worth knowing — after the install, the gate is the consent itself.
 *
 * It does not block the install button. Compatibility is enforced at the gate that fires the plugin
 * (`AMB-D-359`), and installing an inert plugin breaks nothing — so this warns and leaves the choice.
 */
function WhatItWants({ detail }: { detail: PluginDetailDoc }) {
  return (
    <div className="plugdet__wants">
      <div className="plugdet__meta faint">
        <span>
          {detail.scope === "machine"
            ? t("plugins.scope.machine")
            : t("plugins.want.perProject")}
        </span>
        {detail.events.length > 0 && (
          <>
            <span>·</span>
            <span>{tf("plugins.want.events", { events: detail.events.join(", ") })}</span>
          </>
        )}
      </div>
      {/* The settings alone (`AMB-D-727`): what a browse view answers here is what the plugin will want
          filled in, and a part the form draws is not one of those. */}
      {formFields(detail.config).length > 0 && (
        <div className="plugdet__meta faint">
          <span>{t("plugins.want.settings")}</span>
          {/* Each setting is one item on the line, told apart the way the entry's own meta row does it:
              a label can hold spaces, so the gap alone would not say where one ends. */}
          {formFields(detail.config).map((f, i) => (
            <span key={f.key}>
              {i > 0 && <span className="faint">· </span>}
              {settingLabel(f)}
              {f.required && ` (${t("plugins.cfg.required")})`}
              {f.secret && ` (${t("plugins.want.secret")})`}
            </span>
          ))}
        </div>
      )}
      {!detail.compatible && (
        <div className="plugdet__note">
          {detail.incompatibleReason ?? t("plugins.incompatible")}
        </div>
      )}
    </div>
  );
}

/**
 * The one act this screen performs on a plugin: **install** (`AMB-D-351`). Enabling is the other act and
 * it is not here — installing is aimed at no project, so the face that installs asks about none
 * (`AMB-D-412`).
 *
 * **What lands is inert**, and that is what this says once it has: the button does not turn into the
 * switch where it stood, which would read as being asked, after the fact, where the plugin was supposed
 * to go. What is offered instead is the one way on — the installed screen, where every plugin's switch
 * lives.
 */
function PluginActions({ entry, install, onOpenInstalled }: {
  entry: PluginEntry;
  install?: PluginInstall;
  onOpenInstalled: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (install) {
    return (
      <div className="plugdet__actions">
        <span className="chip">{t("plugins.installed")}</span>
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.landedInert")}</span>
        <button className="btn" onClick={onOpenInstalled}>{t("plugins.turnItOn")}</button>
      </div>
    );
  }

  const runInstall = async () => {
    setBusy(true);
    setError(null);
    try {
      await installPlugin(entry.name);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="plugdet__actions">
      <button className="btn" disabled={busy} onClick={() => void runInstall()}>
        {busy ? t("plugins.installing") : t("plugins.install")}
      </button>
      <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.installNote")}</span>
      {error && <div className="plugdet__note">{error}</div>}
    </div>
  );
}
