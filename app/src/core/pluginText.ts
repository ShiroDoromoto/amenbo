// **Which of a plugin author's two lines the reader sees** — the one place the choice is made
// (`AMB-D-623`).
//
// Core carries both halves and selects neither: a base value the author wrote, and the layer they wrote
// in other languages beside it (`AMB-D-621`). Everything here is the same one-line rule — take the
// reader's language, take the base when there is nothing there — held once so no face re-implements it,
// and so the fallback stays the same in all four places a plugin's words are drawn (the market list, the
// detail, the settings form, the update offer).
//
// **A fallback is never announced** (`AMB-D-623`). An untranslated plugin is an English line among
// translated ones and nothing on screen marks it: the reader is not being told about the catalog's
// coverage, they are reading a description.
import type { PluginConfigOptionDto, PluginWantedSettingDto } from "../bindings/bindings";

/** Anything carrying an author's one-line description: a catalog entry, or the build an update offers. */
export interface PluginDescribed {
  desc: string;
  descI18n?: string | null;
}

/** The one-line description in the reader's language, else the one its author wrote. */
export function pluginDesc(d: PluginDescribed): string {
  return d.descI18n ?? d.desc;
}

/** A setting's caption in the reader's language, else the author's own. */
export function settingLabel(f: PluginWantedSettingDto): string {
  return f.labelI18n ?? f.label;
}

/**
 * One candidate's caption in the reader's language, else the author's own. The value beside it is never
 * translated — it is what travels to the plugin.
 */
export function optionLabel(o: PluginConfigOptionDto): string {
  return o.labelI18n ?? o.label;
}
