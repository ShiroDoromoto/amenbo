// The one rule these three functions hold (`AMB-D-623`): the reader's language when the author wrote it,
// the author's own when they did not. Pinned here rather than at each face, because the four faces that
// draw a plugin's words must not each answer it differently — an untranslated label beside a translated
// one is the ordinary state, and it has to read as one line of prose either way.
import { describe, expect, it } from "vitest";
import type { PluginConfigOptionDto, PluginWantedSettingDto } from "../bindings/bindings";
import {
  optionLabel,
  pluginAbout,
  pluginDesc,
  settingHelp,
  settingLabel,
  settingPlaceholder,
} from "./pluginText";

const setting = (over: Partial<PluginWantedSettingDto>): PluginWantedSettingDto => ({
  key: "channel",
  label: "Channel",
  secret: false,
  required: true,
  readonly: false,
  fieldType: "text",
  options: [],
  when: [],
  ...over,
});

const option = (over: Partial<PluginConfigOptionDto>): PluginConfigOptionDto => ({
  value: "task.done",
  label: "Task finished",
  when: [],
  ...over,
});

describe("pluginDesc", () => {
  it("draws the reader's language where the catalog published one", () => {
    expect(pluginDesc({ desc: "post to a channel", descI18n: "チャンネルに投稿する" }))
      .toBe("チャンネルに投稿する");
  });

  // Absent is every ordinary case at once — nobody translated it, nobody published that language, or the
  // reader reads the base language — and none of them is worth a different answer.
  it("falls back to the author's own line, and treats null as absent", () => {
    expect(pluginDesc({ desc: "post to a channel" })).toBe("post to a channel");
    expect(pluginDesc({ desc: "post to a channel", descI18n: null })).toBe("post to a channel");
  });
});

describe("pluginAbout", () => {
  it("draws the reader's language where the author wrote one", () => {
    expect(pluginAbout({ about: "what it does", aboutI18n: "何をするか" })).toBe("何をするか");
    expect(pluginAbout({ about: "what it does" })).toBe("what it does");
  });

  // The one way it differs from the one-line description (`AMB-D-638`): a plugin may have no
  // description at all, and that absence is what sends the detail back to the repository's README.
  it("says nothing for a plugin whose author wrote none", () => {
    expect(pluginAbout({})).toBeUndefined();
    expect(pluginAbout({ about: null, aboutI18n: null })).toBeUndefined();
  });
});

describe("settingLabel", () => {
  it("captions a field in the reader's language, else the author's", () => {
    expect(settingLabel(setting({ labelI18n: "チャンネル" }))).toBe("チャンネル");
    expect(settingLabel(setting({}))).toBe("Channel");
  });
});

describe("settingHelp and settingPlaceholder", () => {
  it("take the reader's language, else the author's, else nothing at all", () => {
    expect(settingHelp(setting({ help: "Paste the URL.", helpI18n: "URL を貼る。" }))).toBe("URL を貼る。");
    expect(settingHelp(setting({ help: "Paste the URL." }))).toBe("Paste the URL.");
    expect(settingHelp(setting({}))).toBeUndefined();

    expect(settingPlaceholder(setting({ placeholder: "2026-01-31", placeholderI18n: "2026年1月31日" })))
      .toBe("2026年1月31日");
    expect(settingPlaceholder(setting({ placeholder: "2026-01-31" }))).toBe("2026-01-31");
    expect(settingPlaceholder(setting({}))).toBeUndefined();
  });
});

describe("optionLabel", () => {
  it("captions a candidate in the reader's language, else the author's", () => {
    expect(optionLabel(option({ labelI18n: "タスクが終わったとき" }))).toBe("タスクが終わったとき");
    expect(optionLabel(option({}))).toBe("Task finished");
  });

  // The value is the plugin's wire vocabulary, so a translation never has one to offer for it.
  it("leaves the stored value alone", () => {
    const o = option({ labelI18n: "タスクが終わったとき" });
    expect(o.value).toBe("task.done");
  });
});
