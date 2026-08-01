import { useMemo, useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { t } from "../core/i18n";
import { useAgentSpec, type CommandSpec } from "../core/reads";
import { asTyped } from "../core/keys";

// The command catalogue: a reference screen for browsing the spec from `amenbo agent --json` (whose
// source of truth is core::agent). Commands are grouped by capability, and each one expands to its
// description, its options (arguments and flags) and its examples. It only displays: the GUI never
// runs the CLI.
export function CommandCatalogScreen() {
  const { spec, loading } = useAgentSpec();
  // The catalogue lists commands to type, so each line is worded for the CLI this build installs.
  const cli = useCliCommandName();
  const [q, setQ] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  // Keep the curated order of the capabilities, and send any command that belongs to none of them to "other", at the end.
  const groups = useMemo(() => {
    const byName = new Map(spec.commands.map((c) => [c.name, c]));
    const seen = new Set<string>();
    const out: { capability: string; commands: CommandSpec[] }[] = [];
    for (const cap of spec.capabilities) {
      const cmds = cap.commands
        .map((n) => byName.get(n))
        .filter((c): c is CommandSpec => !!c);
      cmds.forEach((c) => seen.add(c.name));
      if (cmds.length) out.push({ capability: cap.capability, commands: cmds });
    }
    const rest = spec.commands.filter((c) => !seen.has(c.name));
    if (rest.length) out.push({ capability: t("commands.other"), commands: rest });
    return out;
  }, [spec]);

  const query = q.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      groups
        .map((g) => ({
          capability: g.capability,
          commands: g.commands.filter(
            (c) =>
              !query ||
              c.name.toLowerCase().includes(query) ||
              c.summary.toLowerCase().includes(query),
          ),
        }))
        .filter((g) => g.commands.length > 0),
    [groups, query],
  );

  const total = filtered.reduce((n, g) => n + g.commands.length, 0);

  return (
    <>
      <div className="board__toolbar">
        <input
          {...asTyped}
          className="palette__input cmdcat__search"
          placeholder={t("commands.search")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <div className="topbar__spacer" />
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("commands.note")}</span>
      </div>
      <div className="feed feed--virtual">
        {loading && total === 0 && <div className="feed__item faint">{t("commands.loading")}</div>}
        {!loading && total === 0 && <div className="feed__item faint">{t("commands.empty")}</div>}
        {filtered.map((g) => (
          <div key={g.capability}>
            <div className="feed__daygroup">{g.capability}</div>
            {g.commands.map((c) => (
              <CommandRow
                key={c.name}
                cli={cli}
                cmd={c}
                open={expanded === c.name}
                onToggle={() => setExpanded((cur) => (cur === c.name ? null : c.name))}
              />
            ))}
          </div>
        ))}
      </div>
    </>
  );
}

function CommandRow({ cli, cmd, open, onToggle }: { cli: string; cmd: CommandSpec; open: boolean; onToggle: () => void }) {
  const args = cmd.args ?? [];
  const flags = cmd.flags ?? [];
  const examples = cmd.examples ?? [];
  return (
    <div className="feed__item">
      <div className="feed__body">
        <button className="feed__line feed__action cmdcat__head" onClick={onToggle}>
          <code className="palette__cmd">{cli} {cmd.name}</code>{" "}
          <span className="muted">{cmd.summary}</span>
          <span className="faint"> {open ? "⌄" : "›"}</span>
        </button>
        {open && (args.length > 0 || flags.length > 0 || examples.length > 0) && (
          <div className="cmdcat__detail">
            {args.map((a) => (
              <div className="cmdcat__opt" key={`a-${a.name}`}>
                <code>{a.name}</code>
                {a.required ? <span className="cmdcat__req">{t("commands.required")}</span> : null}
                <span className="muted">{a.help}</span>
              </div>
            ))}
            {flags.map((f) => (
              <div className="cmdcat__opt" key={`f-${f.name}`}>
                <code>{f.name}</code>
                {f.required ? <span className="cmdcat__req">{t("commands.required")}</span> : null}
                <span className="muted">{f.help}</span>
              </div>
            ))}
            {examples.length > 0 && (
              <div className="cmdcat__examples">
                <span className="faint">{t("commands.examples")}</span>
                {examples.map((ex, i) => (
                  <code className="cmdcat__example" key={i}>{ex}</code>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
