// **A run's pane, shown by its roles.**
//
// An automation is split into roles, and a run works a round per task under the role the task names
// (`amenbo_core::model::AutomationRunTask`). What a reader watching a run wants to know is which role
// is doing what, and the automation's picture did not say it: it drew where the run had been, not who
// was at work. So the pane's body draws the roles instead.
//
// **A role with a round is a card**, and a card says the role, the task the round is on, the step
// under way and how long the round has run. A round that may wait for a person, and one that failed,
// say so on the card. **A failed round that was filed as a task is one line under the pane**, naming the
// role, whom it was filed to and the task (`amenbo_core::ops::automation_stop`). **A role with no
// round is an icon on the bar at the bottom**, and nothing else:
// the bar scrolls sideways inside itself where the roles do not fit, so it never pushes the cards out
// of the pane.
//
// **The cards stand in the order the roles are defined in**, and never move for what they are doing:
// a card that jumped to the top when its round asked for a person would be a card the reader has to
// look for again. Only the room the cards are in scrolls — the pane's own place on the page stays.
//
// **One run is one pane** (`AMB-D-939`), and a run walks one round at a time, so the cards are its
// round's role or none. An automation that defines no role still walks rounds: its round is drawn as
// one card named after the automation, with no bar under it.
//
// The role's name and icon are the definition's as it is now (`AMB-D-1015`), read by the project's
// roles (`../core/projectRoles`) — the run's copy holds only which role its round is under.
import { memo, useEffect, useMemo, useState } from "react";
import { useProjectRoles } from "../core/projectRoles";
import { t, tf } from "../core/i18n";
import { Icon, isIconName, type IconName } from "../components/Icon";
import { clockOf, type Say } from "../talk/nameplate";
import type { TaskRoleDto } from "../bindings/bindings";

/** What a role's round is doing, as its card says it. */
export type RoundState =
  /** Under way. */
  | "running"
  /** Under way on a step that may stop and wait for a person (`Say.interactive`). Whether it is
   *  waiting is not something the app reads (`AMB-D-858`); the step's own flag is what it goes by. */
  | "asks"
  /** Held by a pause. */
  | "paused"
  /** The run failed in it. */
  | "failed";

/** One card: the role, or the automation where it defines none, and what its round is doing. */
export type RoleCard = {
  /** The role's id, or null for the round of an automation that defines no role. */
  readonly role: number | null;
  readonly name: string;
  readonly icon: IconName;
  readonly state: RoundState;
};

/** The icon a role is drawn with: the one it was given, where it is one drawn here. */
function roleIcon(role: TaskRoleDto): IconName {
  return role.icon !== null && isIconName(role.icon) ? role.icon : "person";
}

/**
 * **Which roles are cards and which are on the bar**, for the run `run` and the project's roles
 * `roles`, each in the order the roles are defined in.
 *
 * A run over has no round going, and nor does one waiting for its next task: what it still carries is
 * the round it closed before.
 */
export function roleCards(run: Say, roles: readonly TaskRoleDto[]): { cards: RoleCard[]; idle: TaskRoleDto[] } {
  const status = run.state?.status ?? "running";
  const going = status !== "completed" && status !== "canceled" && run.state?.waiting !== true;
  const state: RoundState =
    status === "failed" ? "failed" : status === "paused" ? "paused" : run.interactive ? "asks" : "running";
  const own = roles.filter((role) => role.automationId === run.automationId);
  if (own.length === 0) {
    // A pane no step has arrived in, on a run that has not read back yet, has no round to draw.
    const drawn = going && (run.round !== undefined || run.step !== "");
    return { cards: drawn ? [{ role: null, name: run.automation, icon: "robot", state }] : [], idle: [] };
  }
  const at = going ? run.round?.role ?? null : null;
  return {
    cards: own
      .filter((role) => role.id === at)
      .map((role) => ({ role: role.id, name: role.name, icon: roleIcon(role), state })),
    idle: own.filter((role) => role.id !== at),
  };
}

/** The clock, read again each second while `ticking`. */
function useNow(ticking: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [ticking]);
  return now;
}

/** What a card says under the time, where its round is doing more than working. */
function chipOf(card: RoleCard, run: Say): string | null {
  if (card.state === "asks") return t("auto.run.roles.asks");
  if (card.state === "failed") return t("auto.run.roles.failed");
  if (card.state === "paused") return run.state?.word ?? null;
  return null;
}

/**
 * **The line under the pane for a failed round**, where it was filed: whose round failed, whom it was
 * filed to — the role by its name, or a person — and the task. Null where no failed card was filed.
 */
export function filedLine(cards: readonly RoleCard[], run: Say, roles: readonly TaskRoleDto[]): string | null {
  const incident = run.round?.incident;
  const failed = cards.find((card) => card.state === "failed");
  if (incident === undefined || failed === undefined) return null;
  const to = incident.role === null ? undefined : roles.find((role) => role.id === incident.role);
  if (to !== undefined) return tf("auto.run.roles.filedRole", { role: failed.name, to: to.name, ref: incident.ref });
  if (incident.person) return tf("auto.run.roles.filedPerson", { role: failed.name, ref: incident.ref });
  return tf("auto.run.roles.filed", { role: failed.name, ref: incident.ref });
}

/** One role's card. */
function Card({ card, run, now }: { card: RoleCard; run: Say; now: number }) {
  // The time is the round's, counted while it is under way: a paused or failed round has stopped at
  // a moment the pane does not know.
  const since = run.round?.startedAt ?? null;
  const ran = since !== null && (card.state === "running" || card.state === "asks") ? clockOf(now - since) : null;
  const chip = chipOf(card, run);
  return (
    <li className="runroles__card" data-role={card.role ?? undefined} data-state={card.state}>
      <span className="runroles__top">
        <Icon name={card.icon} />
        <span className="runroles__name">{card.name}</span>
      </span>
      {run.task !== null && (
        <span className="runroles__task" title={`${run.task.ref} ${run.task.title}`}>
          {run.task.ref} {run.task.title}
        </span>
      )}
      {run.step !== "" && <span className="runroles__step">{run.step}</span>}
      {(ran !== null || chip !== null) && (
        <span className="runroles__foot">
          {ran !== null && <span className="runroles__time">{tf("face.elapsed", { time: ran })}</span>}
          {chip !== null && <span className="runroles__chip" data-state={card.state}>{chip}</span>}
        </span>
      )}
    </li>
  );
}

/**
 * Drawn again only when what it reads of the run changes: the run's words are built afresh on each
 * draw of the workspace, and drawing every pane's cards again on each of those would slow the typing
 * in the panes around it.
 */
export const RunRoles = memo(
  function RunRoles({ run, project }: { run: Say; project: number }) {
    const roles = useProjectRoles(project);
    const { cards, idle } = useMemo(() => roleCards(run, roles), [run, roles]);
    const filed = useMemo(() => filedLine(cards, run, roles), [cards, run, roles]);
    const counting = (run.round?.startedAt ?? null) !== null;
    const now = useNow(counting && cards.some((card) => card.state === "running" || card.state === "asks"));
    return (
      <div className="runroles" aria-label={t("auto.run.faceRoles")} role="region">
        <div className="runroles__room">
          {cards.length === 0 ? (
            <p className="runroles__none">{t("auto.run.roles.none")}</p>
          ) : (
            <ul className="runroles__cards">
              {cards.map((card) => <Card key={card.role ?? "run"} card={card} run={run} now={now} />)}
            </ul>
          )}
        </div>
        {idle.length > 0 && (
          <div className="runroles__idle">
            <span className="runroles__idlecount">{tf("auto.run.roles.idle", { n: idle.length })}</span>
            <ul className="runroles__idlelist">
              {idle.map((role) => (
                <li key={role.id} data-role={role.id} title={role.name}>
                  <Icon name={roleIcon(role)} label={tf("auto.run.roles.idleOne", { name: role.name })} />
                </li>
              ))}
            </ul>
          </div>
        )}
        {filed !== null && (
          <p className="runroles__filed" role="status">
            {filed}
          </p>
        )}
      </div>
    );
  },
  (before, after) =>
    before.project === after.project &&
    before.run.run === after.run.run &&
    before.run.automation === after.run.automation &&
    before.run.automationId === after.run.automationId &&
    before.run.step === after.run.step &&
    before.run.interactive === after.run.interactive &&
    before.run.task?.ref === after.run.task?.ref &&
    before.run.task?.title === after.run.task?.title &&
    before.run.round?.role === after.run.round?.role &&
    before.run.round?.startedAt === after.run.round?.startedAt &&
    before.run.round?.incident?.ref === after.run.round?.incident?.ref &&
    before.run.round?.incident?.role === after.run.round?.incident?.role &&
    before.run.round?.incident?.person === after.run.round?.incident?.person &&
    before.run.state?.status === after.run.state?.status &&
    before.run.state?.word === after.run.state?.word &&
    (before.run.state?.waiting === true) === (after.run.state?.waiting === true),
);
